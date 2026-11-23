//! The USB bus as the daemon uses it: list the Leakshields, open one, watch for hot-plug. The
//! trait is the seam of the tests: they plug a simulated bus and never touch the real one.

use std::fmt;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use nicecooling_protocol::device::DeviceError;
use nusb::MaybeFuture;
use tokio::sync::mpsc;

use super::hotplug::{self, UsbDevice, UsbEvent, describe};
use super::leakshield_bulk::NusbBulk;
use super::leakshield_hid::{Hidraw, find_hidraw};
use crate::leakshield::DeviceIo;

#[derive(Debug)]
pub enum OpenError {
    /// The device is on the bus but its hidraw node does not exist yet. A retry can succeed.
    NotReady(String),
    Failed(DeviceError),
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotReady(why) => write!(f, "not ready: {why}"),
            Self::Failed(e) => write!(f, "{e}"),
        }
    }
}

pub trait UsbBus: Send + Sync + fmt::Debug {
    /// The Leakshields on the bus now, with the devices in their bootloader.
    fn list(&self) -> io::Result<Vec<UsbDevice>>;

    /// Opens the two interfaces of a Leakshield. It blocks.
    fn open(&self, device: &UsbDevice) -> Result<DeviceIo, OpenError>;

    /// Sends the events of the bus until `events` closes. It returns at once.
    fn watch(&self, events: mpsc::UnboundedSender<UsbEvent>);
}

/// The real bus: `nusb` for the enumeration, the hot-plug watch and the bulk endpoint, and the
/// hidraw node that sysfs names for the HID interface.
#[derive(Debug)]
pub struct NusbBus {
    sysfs_root: PathBuf,
    dev_root: PathBuf,
}

impl NusbBus {
    /// `sysfs_root` is `/sys` and `dev_root` is `/dev` in the daemon.
    pub fn new(sysfs_root: PathBuf, dev_root: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            sysfs_root,
            dev_root,
        })
    }

    fn infos(&self) -> io::Result<Vec<nusb::DeviceInfo>> {
        Ok(nusb::list_devices()
            .wait()
            .map_err(io::Error::from)?
            .filter(|info| describe(info).is_leakshield())
            .collect())
    }
}

impl UsbBus for NusbBus {
    fn list(&self) -> io::Result<Vec<UsbDevice>> {
        Ok(self.infos()?.iter().map(describe).collect())
    }

    fn open(&self, device: &UsbDevice) -> Result<DeviceIo, OpenError> {
        let info = self
            .infos()
            .map_err(|e| OpenError::Failed(DeviceError::Io(e)))?
            .into_iter()
            .find(|info| describe(info).path == device.path)
            .ok_or(OpenError::Failed(DeviceError::Disconnected))?;
        let name = find_hidraw(&self.sysfs_root, &device.path.0)
            .ok_or_else(|| OpenError::NotReady(format!("no hidraw node at {}", device.path)))?;
        let hid = match Hidraw::open(&Hidraw::node(&self.dev_root, &name)) {
            Ok(hid) => hid,
            Err(DeviceError::Disconnected) => {
                return Err(OpenError::NotReady(format!("{name} is not there yet")));
            }
            Err(e) => return Err(OpenError::Failed(e)),
        };
        let bulk = NusbBulk::open(&info).map_err(OpenError::Failed)?;
        Ok(DeviceIo::new(Arc::new(hid), Arc::new(bulk)))
    }

    fn watch(&self, events: mpsc::UnboundedSender<UsbEvent>) {
        tokio::spawn(async move {
            if let Err(e) = hotplug::watch(events).await {
                eprintln!("nicecoolingd: the USB hot-plug watch ended: {e}");
            }
        });
    }
}
