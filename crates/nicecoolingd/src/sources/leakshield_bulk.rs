//! The vendor interface 0 of the Leakshield through `nusb`: bulk OUT endpoint 0x02 carries the
//! Feed message. No kernel driver claims the interface.

use std::sync::Mutex;
use std::time::Duration;

use nicecooling_protocol::device::{BulkTransport, DeviceError};
use nusb::transfer::{Buffer, Bulk, Out};
use nusb::{DeviceInfo, Endpoint, MaybeFuture};

const INTERFACE: u8 = 0;
const ENDPOINT_OUT: u8 = 0x02;
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(1);

pub struct NusbBulk {
    // The interface stays open as long as the endpoint is in use.
    _interface: nusb::Interface,
    endpoint: Mutex<Endpoint<Bulk, Out>>,
}

fn open_error(e: nusb::Error) -> DeviceError {
    match e.kind() {
        nusb::ErrorKind::Disconnected => DeviceError::Disconnected,
        _ => DeviceError::Io(e.into()),
    }
}

impl NusbBulk {
    pub fn open(info: &DeviceInfo) -> Result<Self, DeviceError> {
        let device = info.open().wait().map_err(open_error)?;
        let interface = device
            .claim_interface(INTERFACE)
            .wait()
            .map_err(open_error)?;
        let endpoint = interface
            .endpoint::<Bulk, Out>(ENDPOINT_OUT)
            .map_err(open_error)?;
        Ok(Self {
            _interface: interface,
            endpoint: Mutex::new(endpoint),
        })
    }
}

impl BulkTransport for NusbBulk {
    fn write(&self, data: &[u8]) -> Result<(), DeviceError> {
        let mut endpoint = self.endpoint.lock().unwrap();
        let completion = endpoint.transfer_blocking(Buffer::from(data), TRANSFER_TIMEOUT);
        completion.status.map_err(|e| {
            let e = std::io::Error::from(e);
            // nusb maps a vanished device to `ConnectionAborted`.
            if e.kind() == std::io::ErrorKind::ConnectionAborted {
                DeviceError::Disconnected
            } else {
                DeviceError::Io(e)
            }
        })
    }
}
