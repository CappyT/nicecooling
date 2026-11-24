//! The display assets of report 8: 40 names, 16 icons and the logo. aquasuite writes the whole
//! report with a new CRC and waits 250 ms (`docs/protocol/leakshield.md`, section Report 8). The
//! daemon patches the bytes of one asset in the report that the device holds, so every other byte
//! stays as the device has it. A backup comes first and a read-back follows.

use std::thread;
use std::time::Duration;

use nicecooling_protocol::access::{Access, WriteRefusal};
use nicecooling_protocol::bitmap::Bitmap;
use nicecooling_protocol::device::DeviceError;
use nicecooling_protocol::report1::Report1;
use nicecooling_protocol::report8::{AssetError, ICON_SLOTS, NAME_COUNT, Report8};

use super::backup::Reason;
use super::write::{WriteEnv, WriteError, device_error, retry_stalled, take_backup};

/// aquasuite waits this long after the write of report 8.
pub const WRITE_DELAY: Duration = Duration::from_millis(250);
/// Differing offsets that a `VerifyFailed` error lists.
const LISTED_OFFSETS: usize = 32;

/// The assets as the D-Bus API shows them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayAssets {
    pub names: Vec<String>,
    pub icons: Vec<Bitmap>,
    pub logo: Bitmap,
}

impl DisplayAssets {
    pub fn of(report: &Report8) -> Self {
        Self {
            names: (0..NAME_COUNT)
                .map(|i| report.name(i).unwrap_or_default())
                .collect(),
            icons: (0..ICON_SLOTS).filter_map(|i| report.icon(i)).collect(),
            logo: report.logo(),
        }
    }
}

/// One change of report 8.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetEdit {
    Name { index: u8, text: String },
    Icon { index: u8, image: Bitmap },
    Logo(Bitmap),
}

impl AssetEdit {
    pub fn apply(&self, report: &mut Report8) -> Result<(), AssetError> {
        match self {
            Self::Name { index, text } => report.set_name(usize::from(*index), text),
            Self::Icon { index, image } => report.set_icon(usize::from(*index), image),
            Self::Logo(image) => report.set_logo(image),
        }
    }

    /// Checks the edit without a device.
    pub fn check(&self) -> Result<(), AssetError> {
        self.apply(&mut Report8::blank())
    }

    fn label(&self) -> String {
        match self {
            Self::Name { index, .. } => format!("display name {index}"),
            Self::Icon { index, .. } => format!("display icon {index}"),
            Self::Logo(_) => "display logo".to_owned(),
        }
    }
}

/// Writes one asset: back up reports 3, 5 and 8, read report 8, change the asset, write the
/// report, wait 250 ms and read it back. The read-back must equal the written bytes. Returns the
/// backup id.
pub fn write_asset(
    env: &WriteEnv<'_>,
    report1: &Report1,
    edit: &AssetEdit,
    now_us: u64,
) -> Result<String, WriteError> {
    if let Some(reason) = Access::new(env.registry, report1).device_reason() {
        return Err(WriteError::Refused(WriteRefusal::ReadOnly(reason)));
    }
    edit.check().map_err(|_| WriteError::BadValue {
        field: edit.label(),
    })?;
    let backup = take_backup(env, report1, Reason::BeforeWrite, now_us)?;
    let current = env
        .io
        .read_report8()
        .map_err(|e| device_error(e, Some(&backup)))?;
    let mut report = Report8::decode(&current).map_err(|_| WriteError::BadValue {
        field: edit.label(),
    })?;
    edit.apply(&mut report).map_err(|_| WriteError::BadValue {
        field: edit.label(),
    })?;
    let bytes = report.encode();
    if bytes == current {
        return Ok(backup);
    }
    write_report8_verified(env, &bytes, &backup)?;
    Ok(backup)
}

/// Writes the report 8 of a backup when the device holds another one. Reads report 8 first: a
/// report that equals the device report sends nothing.
pub(super) fn restore_report8(
    env: &WriteEnv<'_>,
    report8: &[u8],
    backup: &str,
) -> Result<(), WriteError> {
    let current =
        retry_stalled(|| env.io.read_report8()).map_err(|e| device_error(e, Some(backup)))?;
    if current == report8 {
        return Ok(());
    }
    write_report8_verified(env, report8, backup)
}

/// Writes a whole report 8 with its CRC, waits 250 ms and reads it back. The read-back must equal
/// the written bytes.
fn write_report8_verified(
    env: &WriteEnv<'_>,
    bytes: &[u8],
    backup: &str,
) -> Result<(), WriteError> {
    env.io
        .write_report8(bytes)
        .map_err(|e| device_error(e, Some(backup)))?;
    thread::sleep(WRITE_DELAY);
    match retry_stalled(|| env.io.read_report8()) {
        Ok(read) if read == bytes => Ok(()),
        Ok(read) => Err(WriteError::VerifyFailed {
            report: 8,
            backup: backup.to_owned(),
            offsets: (1..bytes.len())
                .filter(|&i| bytes[i] != read[i])
                .take(LISTED_OFFSETS)
                .collect(),
        }),
        // A read that does not decode holds something else than the written report.
        Err(DeviceError::BadReport(_)) => Err(WriteError::VerifyFailed {
            report: 8,
            backup: backup.to_owned(),
            offsets: Vec::new(),
        }),
        Err(e) => Err(device_error(e, Some(backup))),
    }
}
