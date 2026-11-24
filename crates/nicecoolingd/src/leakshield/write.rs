//! The write path of design D11: backup, re-read, change the fields, write, read back, compare.

use std::fmt;
use std::io;
use std::thread;
use std::time::Duration;

use nicecooling_protocol::access::{Access, ReadOnlyReason, WriteRefusal};
use nicecooling_protocol::device::DeviceError;
use nicecooling_protocol::registry::{Field, Registry, Scalar};
use nicecooling_protocol::report1::Report1;
use nicecooling_protocol::report3::Report3;
use nicecooling_protocol::report8::Report8;

use super::DeviceIo;
use super::assets::restore_report8;
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
    /// The read-back differs from the written report (outside the volatile mask for report 3).
    /// `report` is the number of the report that failed: 3 or 8.
    VerifyFailed {
        report: u8,
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
    /// The registry of the firmware lacks the command, or does not cover the stored firmware.
    NotSupported,
    /// The slot has no stored report 3.
    NoStoredSlot,
    /// `SetStoredFields` for the slot that the device runs: `SetFields` changes it.
    ActiveSlot,
    /// The daemon sets this field itself.
    FieldNotEditable(String),
    /// The value does not fit the kind of the field.
    BadValue {
        field: String,
    },
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(r) => write!(f, "write refused: {r:?}"),
            Self::OutOfRange { field } => write!(f, "value out of range for {field}"),
            Self::Busy => write!(f, "a write is queued"),
            Self::DeviceChanged => write!(f, "report 3 changed since the last read"),
            Self::VerifyFailed { report, backup, .. } => {
                write!(f, "read-back of report {report} differs, backup {backup}")
            }
            Self::Disconnected { .. } => write!(f, "device disconnected"),
            Self::Device { error, .. } => write!(f, "{error}"),
            Self::Backup(e) => write!(f, "backup failed: {e}"),
            Self::NoSuchBackup => write!(f, "no such backup"),
            Self::BackupMismatch(why) => write!(f, "backup does not fit the device: {why}"),
            Self::NotSupported => write!(f, "the registry of this firmware lacks the command"),
            Self::NoStoredSlot => write!(f, "the slot has no stored report 3"),
            Self::ActiveSlot => write!(f, "the slot is the active slot of the device"),
            Self::FieldNotEditable(field) => write!(f, "{field} is not an editable field"),
            Self::BadValue { field } => write!(f, "the value of {field} does not fit the field"),
        }
    }
}

impl WriteError {
    /// The backup that a restore can use after this error, when one exists.
    pub fn backup_id(&self) -> Option<&str> {
        match self {
            Self::VerifyFailed { backup, .. } => Some(backup),
            Self::Disconnected { backup } | Self::Device { backup, .. } => backup.as_deref(),
            _ => None,
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
    access: Access<'a>,
    edits: &'a [FieldEdit],
) -> Result<Vec<(&'a Field, &'a FieldEdit)>, WriteError> {
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

/// fw 1022 stalls the feature read that follows a report 3 write: the read gets EPIPE while the
/// device stores the report (measured with usbmon). The read-back waits and tries again.
const READ_BACK_RETRY: Duration = Duration::from_millis(50);
const READ_BACK_ATTEMPTS: u32 = 20;

/// Runs `read` again while the device answers with EPIPE, up to 20 times, 50 ms apart.
pub(super) fn retry_stalled<T>(
    mut read: impl FnMut() -> Result<T, DeviceError>,
) -> Result<T, DeviceError> {
    let mut attempts = 1;
    loop {
        match read() {
            Err(DeviceError::Io(e))
                if e.kind() == io::ErrorKind::BrokenPipe && attempts < READ_BACK_ATTEMPTS =>
            {
                attempts += 1;
                thread::sleep(READ_BACK_RETRY);
            }
            other => return other,
        }
    }
}

fn read_back(io: &DeviceIo) -> Result<Report3, DeviceError> {
    retry_stalled(|| io.read_report3())
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
    let read_back = match read_back(env.io) {
        Ok(report) => report,
        // A report that does not decode has a bad CRC or size: the device holds something else.
        Err(DeviceError::BadReport(_)) => {
            return Err(WriteError::VerifyFailed {
                report: 3,
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
            report: 3,
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
    let planned = plan(Access::new(env.registry, report1), edits)?;
    let backup = take_backup(env, report1, Reason::BeforeWrite, now_us)?;
    write_planned(env, base, planned, backup)
}

/// `write_fields` under a backup that the caller took before: the Verification tool writes the
/// fields of a group under the one backup of its run, so the backup retention removes nothing.
pub fn write_fields_under(
    env: &WriteEnv<'_>,
    report1: &Report1,
    base: &Report3,
    edits: &[FieldEdit],
    backup: &str,
) -> Result<Written, WriteError> {
    let planned = plan(Access::new(env.registry, report1), edits)?;
    write_planned(env, base, planned, backup.to_owned())
}

fn write_planned(
    env: &WriteEnv<'_>,
    base: &Report3,
    planned: Vec<(&Field, &FieldEdit)>,
    backup: String,
) -> Result<Written, WriteError> {
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

/// Writes `report3` as it is and reads it back, with no backup and no range check: the caller
/// holds the bytes. The Verification tool writes back the report 3 that it read before its first
/// write. `backup` names the backup of the run in an error.
pub fn write_verified(
    env: &WriteEnv<'_>,
    report3: &Report3,
    backup: &str,
) -> Result<Report3, WriteError> {
    write_and_verify(env, report3, backup)
}

/// A Profile activation (D14): backs up reports 3, 5 and 8, then writes the stored report 3 as it
/// is stored. It does not read the report again before the write and does not read it back, so
/// the written report is the new last known report 3.
pub fn write_stored(
    env: &WriteEnv<'_>,
    report1: &Report1,
    report3: &Report3,
    now_us: u64,
) -> Result<Written, WriteError> {
    if let Some(reason) = Access::new(env.registry, report1).device_reason() {
        return Err(WriteError::Refused(WriteRefusal::ReadOnly(reason)));
    }
    let backup = take_backup(env, report1, Reason::BeforeWrite, now_us)?;
    env.io
        .write_report3(report3)
        .map_err(|e| device_error(e, Some(&backup)))?;
    Ok(Written {
        backup,
        report3: report3.clone(),
    })
}

/// A Profile activation of an edited slot (design D18): the verified write of D11 without the
/// second read of report 3, because the slot replaces the device settings as a whole. It backs up
/// reports 3, 5 and 8, writes the stored report 3 and reads it back. The read-back must match
/// outside the volatile mask. The written report is the read-back.
pub fn write_edited(
    env: &WriteEnv<'_>,
    report1: &Report1,
    report3: &Report3,
    now_us: u64,
) -> Result<Written, WriteError> {
    if let Some(reason) = Access::new(env.registry, report1).device_reason() {
        return Err(WriteError::Refused(WriteRefusal::ReadOnly(reason)));
    }
    let backup = take_backup(env, report1, Reason::BeforeWrite, now_us)?;
    let report3 = write_and_verify(env, report3, &backup)?;
    Ok(Written { backup, report3 })
}

/// The report 3 of a stored slot with `edits` applied, with the rules of `SetFields` against the
/// firmware and the structure of the slot. It does not read the device. The CRC is valid after the
/// encode of the result.
pub fn edit_stored(
    registry: &Registry,
    firmware: u16,
    structure: u16,
    stored: &Report3,
    edits: &[FieldEdit],
) -> Result<Report3, WriteError> {
    let access = Access::for_firmware(registry, firmware, structure);
    if let Some(edit) = edits.iter().find(|e| e.field == "device_profile") {
        return Err(WriteError::FieldNotEditable(edit.field.clone()));
    }
    let mut report3 = stored.clone();
    for (field, edit) in plan(access, edits)? {
        let scalar = field.value_type.scalar;
        for (offset, &raw) in field.element_offsets().into_iter().zip(&edit.raw) {
            report3.set_bytes(offset, &encode(scalar, raw));
        }
    }
    Ok(report3)
}

/// Restores a backup: writes its report 3 and reads it back, then writes its report 8 when the
/// device holds another one (through the write of an asset: whole report, 250 ms, read-back).
/// Report 5 stays as it is. A backup without a report 8 restores report 3 only. The state from
/// before the restore becomes a backup first.
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
    if !set.report8.is_empty() && Report8::decode(&set.report8).is_err() {
        return Err(WriteError::BackupMismatch(
            "report 8 of the backup is invalid",
        ));
    }
    let backup = take_backup(env, report1, Reason::BeforeWrite, now_us)?;
    let report3 = write_and_verify(env, &stored, &backup)?;
    if !set.report8.is_empty() {
        restore_report8(env, &set.report8, &backup)?;
    }
    Ok(Written { backup, report3 })
}
