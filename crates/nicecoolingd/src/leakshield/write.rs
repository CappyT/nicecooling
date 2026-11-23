//! The write path of design D11: backup, re-read, change the fields, write, read back, compare.

use std::fmt;
use std::io;

use nicecooling_protocol::access::{Access, ReadOnlyReason, WriteRefusal};
use nicecooling_protocol::device::DeviceError;
use nicecooling_protocol::registry::{Field, Registry, Scalar};
use nicecooling_protocol::report1::Report1;
use nicecooling_protocol::report3::Report3;

use super::DeviceIo;
use super::backup::{BackupSet, Backups, Reason};

/// New raw values of one field: one value for each element of `Field::element_offsets`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldEdit {
    pub field: String,
    pub raw: Vec<i64>,
}

impl FieldEdit {
    pub fn scalar(field: &str, raw: i64) -> Self {
        Self {
            field: field.to_owned(),
            raw: vec![raw],
        }
    }
}

#[derive(Debug)]
pub enum WriteError {
    /// The device or the field is read-only. Nothing was sent to the device.
    Refused(WriteRefusal),
    /// A value is out of range or has the wrong count. Nothing was sent to the device.
    OutOfRange {
        field: String,
    },
    /// The worker already holds a write.
    Busy,
    /// Report 3 changed outside the volatile mask after the first read.
    DeviceChanged,
    /// The read-back differs from the written report outside the volatile mask.
    VerifyFailed {
        backup: String,
        offsets: Vec<usize>,
    },
    /// The device vanished. `backup` is the backup that a restore can use, when one exists.
    Disconnected {
        backup: Option<String>,
    },
    Device {
        error: DeviceError,
        backup: Option<String>,
    },
    Backup(io::Error),
    NoSuchBackup,
    /// The backup does not fit the device: structure or firmware.
    BackupMismatch(&'static str),
    /// The registry of the firmware lacks the command.
    NotSupported,
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(r) => write!(f, "write refused: {r:?}"),
            Self::OutOfRange { field } => write!(f, "value out of range for {field}"),
            Self::Busy => write!(f, "a write is queued"),
            Self::DeviceChanged => write!(f, "report 3 changed since the last read"),
            Self::VerifyFailed { backup, .. } => write!(f, "read-back differs, backup {backup}"),
            Self::Disconnected { .. } => write!(f, "device disconnected"),
            Self::Device { error, .. } => write!(f, "{error}"),
            Self::Backup(e) => write!(f, "backup failed: {e}"),
            Self::NoSuchBackup => write!(f, "no such backup"),
            Self::BackupMismatch(why) => write!(f, "backup does not fit the device: {why}"),
            Self::NotSupported => write!(f, "the registry of this firmware lacks the command"),
        }
    }
}

impl std::error::Error for WriteError {}

/// What a successful write leaves behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// The backup that holds the state from before the write.
    pub backup: String,
    /// Report 3 as the read-back returned it.
    pub report3: Report3,
}

pub struct WriteEnv<'a> {
    pub io: &'a DeviceIo,
    pub registry: &'a Registry,
    pub backups: &'a Backups,
}

pub(super) fn device_error(error: DeviceError, backup: Option<&str>) -> WriteError {
    let backup = backup.map(str::to_owned);
    match error {
        DeviceError::Disconnected => WriteError::Disconnected { backup },
        error => WriteError::Device { error, backup },
    }
}

fn scalar_bounds(scalar: Scalar) -> (i64, i64) {
    match scalar {
        Scalar::U8 => (0, i64::from(u8::MAX)),
        Scalar::U16 => (0, i64::from(u16::MAX)),
        Scalar::I16 => (i64::from(i16::MIN), i64::from(i16::MAX)),
    }
}

fn encode(scalar: Scalar, raw: i64) -> Vec<u8> {
    match scalar {
        Scalar::U8 => vec![raw as u8],
        Scalar::U16 | Scalar::I16 => (raw as u16).to_be_bytes().to_vec(),
    }
}

/// Checks the edits against the registry and the firmware. No device I/O.
fn plan<'a>(
    registry: &'a Registry,
    report1: &Report1,
    edits: &'a [FieldEdit],
) -> Result<Vec<(&'a Field, &'a FieldEdit)>, WriteError> {
    let access = Access::new(registry, report1);
    let mut planned = Vec::new();
    for edit in edits {
        let field = access
            .check_write(&edit.field)
            .map_err(WriteError::Refused)?;
        let (low, high) = scalar_bounds(field.value_type.scalar);
        let valid = edit.raw.len() == field.element_offsets().len()
            && edit
                .raw
                .iter()
                .all(|&raw| (low..=high).contains(&raw) && field.raw_in_range(raw));
        if !valid {
            return Err(WriteError::OutOfRange {
                field: edit.field.clone(),
            });
        }
        planned.push((field, edit));
    }
    Ok(planned)
}

/// The first field of the registry that holds a value outside its range in `report`.
pub fn out_of_range_field<'a>(registry: &'a Registry, report: &Report3) -> Option<&'a str> {
    registry
        .field
        .iter()
        .find(|field| !field.in_range_in(report))
        .map(|field| field.id.as_str())
}

/// Reads reports 3, 5 and 8 and saves them. Returns the backup id.
pub fn take_backup(
    env: &WriteEnv<'_>,
    report1: &Report1,
    reason: Reason,
    now_us: u64,
) -> Result<String, WriteError> {
    let set = BackupSet {
        report3: env
            .io
            .read_report3()
            .map_err(|e| device_error(e, None))?
            .encode(),
        report5: env.io.read_report5().map_err(|e| device_error(e, None))?,
        report8: env.io.read_report8().map_err(|e| device_error(e, None))?,
    };
    env.backups
        .save(&set, report1.firmware_version, reason, now_us)
        .map(|saved| saved.id)
        .map_err(WriteError::Backup)
}

/// Writes `report3` and reads it back. The read-back must match outside the volatile mask.
fn write_and_verify(
    env: &WriteEnv<'_>,
    report3: &Report3,
    backup: &str,
) -> Result<Report3, WriteError> {
    let mask = &env.registry.report3.volatile_mask;
    env.io
        .write_report3(report3)
        .map_err(|e| device_error(e, Some(backup)))?;
    let read_back = match env.io.read_report3() {
        Ok(report) => report,
        // A report that does not decode has a bad CRC or size: the device holds something else.
        Err(DeviceError::BadReport(_)) => {
            return Err(WriteError::VerifyFailed {
                backup: backup.to_owned(),
                offsets: Vec::new(),
            });
        }
        Err(e) => return Err(device_error(e, Some(backup))),
    };
    let offsets = read_back.differing_offsets(report3, mask);
    if offsets.is_empty() {
        Ok(read_back)
    } else {
        Err(WriteError::VerifyFailed {
            backup: backup.to_owned(),
            offsets,
        })
    }
}

/// D11: `base` is the report 3 that the caller read first. The write changes only the edited
/// fields and keeps every other byte of the report that the second read returned.
pub fn write_fields(
    env: &WriteEnv<'_>,
    report1: &Report1,
    base: &Report3,
    edits: &[FieldEdit],
    now_us: u64,
) -> Result<Written, WriteError> {
    let planned = plan(env.registry, report1, edits)?;
    let backup = take_backup(env, report1, Reason::BeforeWrite, now_us)?;
    let mask = &env.registry.report3.volatile_mask;
    let mut report3 = env
        .io
        .read_report3()
        .map_err(|e| device_error(e, Some(&backup)))?;
    if report3.structure_number() != env.registry.structure {
        return Err(WriteError::Refused(WriteRefusal::ReadOnly(
            ReadOnlyReason::UnknownStructure,
        )));
    }
    if !report3.differing_offsets(base, mask).is_empty() {
        return Err(WriteError::DeviceChanged);
    }
    for (field, edit) in planned {
        let scalar = field.value_type.scalar;
        for (offset, &raw) in field.element_offsets().into_iter().zip(&edit.raw) {
            report3.set_bytes(offset, &encode(scalar, raw));
        }
    }
    let report3 = write_and_verify(env, &report3, &backup)?;
    Ok(Written { backup, report3 })
}

/// Writes report 3 of a backup and reads it back. Reports 5 and 8 stay as they are. The state
/// from before the restore becomes a backup first.
pub fn restore(
    env: &WriteEnv<'_>,
    report1: &Report1,
    backup_id: &str,
    now_us: u64,
) -> Result<Written, WriteError> {
    if let Some(reason) = Access::new(env.registry, report1).device_reason() {
        return Err(WriteError::Refused(WriteRefusal::ReadOnly(reason)));
    }
    let set = env.backups.get(backup_id).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => WriteError::NoSuchBackup,
        _ => WriteError::Backup(e),
    })?;
    let firmware = env
        .backups
        .firmware(backup_id)
        .map_err(WriteError::Backup)?;
    let stored = Report3::decode(&set.report3)
        .map_err(|_| WriteError::BackupMismatch("report 3 of the backup is invalid"))?;
    if stored.structure_number() != env.registry.structure {
        return Err(WriteError::BackupMismatch("structure"));
    }
    if out_of_range_field(env.registry, &stored).is_some() {
        return Err(WriteError::BackupMismatch("a field is out of its range"));
    }
    if firmware != report1.firmware_version {
        return Err(WriteError::BackupMismatch("firmware"));
    }
    let backup = take_backup(env, report1, Reason::BeforeWrite, now_us)?;
    let report3 = write_and_verify(env, &stored, &backup)?;
    Ok(Written { backup, report3 })
}
