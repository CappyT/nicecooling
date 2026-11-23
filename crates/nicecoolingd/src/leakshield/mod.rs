//! Leakshield device handling: the I/O wrapper, backups, the write path and its worker, the Feed
//! message, external change detection, the clock, Mode and Action. `attached` joins them for the
//! D-Bus API.

use std::sync::Arc;
use std::time::Duration;

use nicecooling_protocol::device::{BulkTransport, DeviceError, HidTransport};
use nicecooling_protocol::feed::FeedMessage;
use nicecooling_protocol::report1::{REPORT1_ID, REPORT1_SIZE, Report1};
use nicecooling_protocol::report2::Report2;
use nicecooling_protocol::report3::{REPORT3_SIZE, Report3};
use nicecooling_protocol::report5::{REPORT5_SIZE, Report5};
use nicecooling_protocol::report8::{REPORT8_SIZE, Report8};

pub mod action;
pub mod attached;
pub mod backup;
pub mod clock;
pub mod external;
pub mod feed;
pub mod fields;
pub mod mode;
pub mod save_settings;
pub mod worker;
pub mod write;

pub use attached::{AttachParams, Attached};

/// How long `read_report1` waits for each input report.
const INPUT_TIMEOUT: Duration = Duration::from_secs(2);
/// Input reports that `read_report1` skips before it gives up.
const INPUT_ATTEMPTS: usize = 8;

/// Something the daemon reports on the D-Bus signals of the `Leakshield` interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceEvent {
    /// The save-settings command failed. `backup` is the backup that a restore can use.
    WriteFailed { error: String, backup: String },
    /// Report 3 changed outside the volatile mask without a write of the daemon.
    ExternalChange { fields: Vec<String> },
}

/// Runs a blocking device call. On the multi-thread runtime of the daemon it tells the runtime
/// first, so that a slow transfer does not stall other tasks.
pub(crate) fn blocking<R>(f: impl FnOnce() -> R) -> R {
    use tokio::runtime::{Handle, RuntimeFlavor};
    match Handle::try_current().map(|h| h.runtime_flavor()) {
        Ok(RuntimeFlavor::MultiThread) => tokio::task::block_in_place(f),
        _ => f(),
    }
}

/// The two transports of one Leakshield with the typed reads and writes of its reports.
#[derive(Clone)]
pub struct DeviceIo {
    hid: Arc<dyn HidTransport>,
    bulk: Arc<dyn BulkTransport>,
}

impl std::fmt::Debug for DeviceIo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("DeviceIo")
    }
}

impl DeviceIo {
    pub fn new(hid: Arc<dyn HidTransport>, bulk: Arc<dyn BulkTransport>) -> Self {
        Self { hid, bulk }
    }

    /// Report 1 comes from `read()` of the input reports that the device sends.
    pub fn read_report1(&self) -> Result<Report1, DeviceError> {
        for _ in 0..INPUT_ATTEMPTS {
            let bytes = self.hid.read_input(INPUT_TIMEOUT)?;
            if bytes.first() == Some(&REPORT1_ID) && bytes.len() == REPORT1_SIZE {
                return Ok(Report1::decode(&bytes)?);
            }
        }
        Err(DeviceError::Timeout)
    }

    pub fn read_report3(&self) -> Result<Report3, DeviceError> {
        Ok(Report3::decode(&self.hid.get_feature(3, REPORT3_SIZE)?)?)
    }

    /// Raw bytes, validated by the codec.
    pub fn read_report5(&self) -> Result<Vec<u8>, DeviceError> {
        let bytes = self.hid.get_feature(5, REPORT5_SIZE)?;
        Report5::decode(&bytes)?;
        Ok(bytes)
    }

    /// Raw bytes, validated by the codec.
    pub fn read_report8(&self) -> Result<Vec<u8>, DeviceError> {
        let bytes = self.hid.get_feature(8, REPORT8_SIZE)?;
        Report8::decode(&bytes)?;
        Ok(bytes)
    }

    /// `Report3` holds a valid CRC, so the device always gets one.
    pub fn write_report3(&self, report: &Report3) -> Result<(), DeviceError> {
        self.hid.set_feature(&report.encode())
    }

    pub fn send_command(&self, command: Report2) -> Result<(), DeviceError> {
        self.hid.write_output(&command.encode())
    }

    pub fn send_feed(&self, message: &FeedMessage) -> Result<(), DeviceError> {
        self.bulk.write(&message.encode())
    }
}
