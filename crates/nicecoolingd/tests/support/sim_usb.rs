//! A simulated USB bus: the daemon lists, opens and watches it instead of the real one. No test
//! reaches `/dev`, `/sys` or the USB bus.

use std::io;
use std::sync::{Arc, Mutex};

use nicecooling_protocol::device::{DeviceError, PRODUCT_ID, VENDOR_ID};
use nicecooling_protocol::sim::SimDevice;
use nicecoolingd::leakshield::DeviceIo;
use nicecoolingd::sources::hotplug::{UsbDevice, UsbEvent, UsbPath};
use nicecoolingd::sources::usb::{OpenError, UsbBus};
use tokio::sync::mpsc::UnboundedSender;

struct Plugged {
    device: UsbDevice,
    sim: SimDevice,
}

#[derive(Default)]
struct Bus {
    plugged: Vec<Plugged>,
    watchers: Vec<UnboundedSender<UsbEvent>>,
    /// The next opens fail as "no hidraw node yet" this many times.
    not_ready: u32,
    opened: Vec<UsbPath>,
}

#[derive(Clone, Default)]
pub struct SimUsb(Arc<Mutex<Bus>>);

impl std::fmt::Debug for SimUsb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SimUsb")
    }
}

pub fn device(path: &str, bcd_device: u16, interfaces: usize) -> UsbDevice {
    UsbDevice {
        path: UsbPath(path.to_owned()),
        vendor: VENDOR_ID,
        product: PRODUCT_ID,
        bcd_device,
        interfaces,
    }
}

impl SimUsb {
    fn bus(&self) -> std::sync::MutexGuard<'_, Bus> {
        self.0.lock().unwrap()
    }

    /// A device on the bus before the daemon starts.
    pub fn present(&self, device: UsbDevice, sim: &SimDevice) {
        self.bus().plugged.push(Plugged {
            device,
            sim: sim.clone(),
        });
    }

    /// The device appears on the bus and the watch reports it.
    pub fn plug(&self, device: UsbDevice, sim: &SimDevice) {
        sim.reconnect();
        let mut bus = self.bus();
        bus.plugged.push(Plugged {
            device: device.clone(),
            sim: sim.clone(),
        });
        bus.watchers
            .retain(|w| w.send(UsbEvent::Connected(device.clone())).is_ok());
    }

    /// The device leaves the bus: its transports fail and the watch reports it.
    pub fn unplug(&self, path: &str) {
        let path = UsbPath(path.to_owned());
        let mut bus = self.bus();
        if let Some(i) = bus.plugged.iter().position(|p| p.device.path == path) {
            bus.plugged.remove(i).sim.disconnect();
        }
        bus.watchers.retain(|w| {
            w.send(UsbEvent::Disconnected { path: path.clone() })
                .is_ok()
        });
    }

    /// The watch reports a device that is on the bus already, as the start race can.
    pub fn announce(&self, device: UsbDevice) {
        self.bus()
            .watchers
            .retain(|w| w.send(UsbEvent::Connected(device.clone())).is_ok());
    }

    pub fn not_ready_for(&self, opens: u32) {
        self.bus().not_ready = opens;
    }

    /// The paths that the daemon opened, in order.
    pub fn opened(&self) -> Vec<UsbPath> {
        self.bus().opened.clone()
    }
}

impl UsbBus for SimUsb {
    fn list(&self) -> io::Result<Vec<UsbDevice>> {
        Ok(self
            .bus()
            .plugged
            .iter()
            .map(|p| p.device.clone())
            .collect())
    }

    fn open(&self, device: &UsbDevice) -> Result<DeviceIo, OpenError> {
        let mut bus = self.bus();
        if bus.not_ready > 0 {
            bus.not_ready -= 1;
            return Err(OpenError::NotReady("no hidraw node yet".to_owned()));
        }
        bus.opened.push(device.path.clone());
        let sim = bus
            .plugged
            .iter()
            .find(|p| p.device.path == device.path)
            .map(|p| p.sim.clone())
            .ok_or(OpenError::Failed(DeviceError::Disconnected))?;
        Ok(DeviceIo::new(Arc::new(sim.clone()), Arc::new(sim)))
    }

    fn watch(&self, events: UnboundedSender<UsbEvent>) {
        self.bus().watchers.push(events);
    }
}
