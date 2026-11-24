//! The two transports of a Leakshield. The daemon implements them for hidraw and `nusb`, and
//! `sim.rs` implements them for tests.

use std::fmt;
use std::io;
use std::time::Duration;

use crate::error::ReportError;

pub const VENDOR_ID: u16 = 0x0c70;
pub const PRODUCT_ID: u16 = 0xf014;

#[derive(Debug)]
pub enum DeviceError {
    /// The device is gone: unplugged, or its node vanished.
    Disconnected,
    /// No input report arrived in time.
    Timeout,
    Io(io::Error),
    /// The device answered with a report that does not decode.
    BadReport(ReportError),
}

impl From<ReportError> for DeviceError {
    fn from(e: ReportError) -> Self {
        Self::BadReport(e)
    }
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Disconnected => write!(f, "device disconnected"),
            Self::Timeout => write!(f, "timeout"),
            Self::Io(e) => write!(f, "device I/O error: {e}"),
            Self::BadReport(e) => write!(f, "bad report from the device: {e}"),
        }
    }
}

impl std::error::Error for DeviceError {}

/// HID interface. Every method takes `&self`, so one thread can block in `read_input` while
/// another one sets a feature report.
pub trait HidTransport: Send + Sync {
    /// Returns the newest input report that the device sent, with its report ID. It waits up to
    /// `timeout` when no report is queued. The daemon reads report 1 with this call and never asks
    /// for it with an ioctl.
    fn read_input(&self, timeout: Duration) -> Result<Vec<u8>, DeviceError>;

    /// Feature read. `len` is the report size with its report ID byte. The answer starts with
    /// the report ID.
    fn get_feature(&self, id: u8, len: usize) -> Result<Vec<u8>, DeviceError>;

    /// Feature write. `report` starts with the report ID.
    fn set_feature(&self, report: &[u8]) -> Result<(), DeviceError>;

    /// Output report write (report 2). `report` starts with the report ID.
    fn write_output(&self, report: &[u8]) -> Result<(), DeviceError>;
}

/// Bulk OUT endpoint 0x02 of interface 0.
pub trait BulkTransport: Send + Sync {
    fn write(&self, data: &[u8]) -> Result<(), DeviceError>;
}
