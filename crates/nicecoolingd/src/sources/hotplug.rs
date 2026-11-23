//! USB devices and hot-plug. The Leakshield is matched by its USB path, so a replug reattaches the
//! same device. `UsbEvent` is the event of this module. The `nusb` adapter converts into it, and
//! tests send it.

use std::collections::HashMap;
use std::fmt;

use nicecooling_protocol::device::{PRODUCT_ID, VENDOR_ID};
use nusb::hotplug::HotplugEvent;
use nusb::{DeviceId, DeviceInfo, MaybeFuture};
use tokio::sync::mpsc;

/// USB device version (BCD) of a Leakshield in its bootloader: 2.00. The application has 1.00.
const BOOTLOADER_BCD: u16 = 0x0200;
/// The bootloader has no keyboard interface.
const BOOTLOADER_INTERFACES: usize = 2;

/// `<bus>-<port>.<port>...`, the name that sysfs gives the device: `1-2.3`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UsbPath(pub String);

impl UsbPath {
    /// nusb gives the bus as `001` on Linux; sysfs names the device `1-...`.
    pub fn new(bus_id: &str, port_chain: &[u8]) -> Self {
        let bus = bus_id.trim_start_matches('0');
        let bus = if bus.is_empty() { "0" } else { bus };
        let ports: Vec<String> = port_chain.iter().map(u8::to_string).collect();
        Self(format!("{bus}-{}", ports.join(".")))
    }
}

impl fmt::Display for UsbPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What the daemon needs to know about a USB device before it opens it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsbDevice {
    pub path: UsbPath,
    pub vendor: u16,
    pub product: u16,
    pub bcd_device: u16,
    pub interfaces: usize,
}

impl UsbDevice {
    pub fn is_leakshield(&self) -> bool {
        (self.vendor, self.product) == (VENDOR_ID, PRODUCT_ID)
    }

    /// A Leakshield that a firmware update left in its bootloader. Its HID interface has a short
    /// report 1 and no settings, so the daemon never opens it.
    pub fn in_bootloader(&self) -> bool {
        self.bcd_device == BOOTLOADER_BCD && self.interfaces == BOOTLOADER_INTERFACES
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsbEvent {
    Connected(UsbDevice),
    Disconnected { path: UsbPath },
}

/// Converts `nusb` events. `nusb` names a removed device by an id only, so the adapter keeps the
/// path of each device it saw.
#[derive(Default)]
pub struct NusbEvents {
    paths: HashMap<DeviceId, UsbPath>,
}

pub fn describe(info: &DeviceInfo) -> UsbDevice {
    UsbDevice {
        path: UsbPath::new(info.bus_id(), info.port_chain()),
        vendor: info.vendor_id(),
        product: info.product_id(),
        bcd_device: info.device_version(),
        interfaces: info.interfaces().count(),
    }
}

impl NusbEvents {
    /// Remembers the path of a device that exists already, so that its removal has a path.
    pub fn remember(&mut self, info: &DeviceInfo) -> UsbDevice {
        let device = describe(info);
        self.paths.insert(info.id(), device.path.clone());
        device
    }

    pub fn convert(&mut self, event: HotplugEvent) -> Option<UsbEvent> {
        match event {
            HotplugEvent::Connected(info) => Some(UsbEvent::Connected(self.remember(&info))),
            HotplugEvent::Disconnected(id) => self
                .paths
                .remove(&id)
                .map(|path| UsbEvent::Disconnected { path }),
        }
    }
}

/// Watches the USB bus and sends each event until the watch ends or `events` closes. It starts
/// the watch first and lists the devices after it, so that no removal slips through.
pub async fn watch(events: mpsc::UnboundedSender<UsbEvent>) -> std::io::Result<()> {
    use futures_core::Stream;

    let mut watch = nusb::watch_devices().map_err(std::io::Error::from)?;
    let mut known = NusbEvents::default();
    for info in nusb::list_devices().wait().map_err(std::io::Error::from)? {
        known.remember(&info);
    }
    loop {
        let next = tokio::select! {
            () = events.closed() => return Ok(()),
            next = std::future::poll_fn(|cx| Pin::new(&mut watch).poll_next(cx)) => next,
        };
        let Some(event) = next else {
            return Ok(());
        };
        if let Some(event) = known.convert(event)
            && events.send(event).is_err()
        {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(bcd_device: u16, interfaces: usize) -> UsbDevice {
        UsbDevice {
            path: UsbPath("1-2.3".to_owned()),
            vendor: VENDOR_ID,
            product: PRODUCT_ID,
            bcd_device,
            interfaces,
        }
    }

    #[test]
    fn path_of_bus_and_ports() {
        assert_eq!(UsbPath::new("1", &[2, 3]).0, "1-2.3");
        assert_eq!(UsbPath::new("3", &[4]).0, "3-4");
        assert_eq!(UsbPath::new("001", &[5, 1]).0, "1-5.1");
        assert_eq!(UsbPath::new("010", &[2]).0, "10-2");
    }

    #[test]
    fn bootloader_has_bcd_2_00_and_two_interfaces() {
        assert!(device(0x0200, 2).in_bootloader());
        // The application: version 1.00 and three interfaces.
        assert!(!device(0x0100, 3).in_bootloader());
        assert!(!device(0x0200, 3).in_bootloader());
        assert!(!device(0x0100, 2).in_bootloader());
    }
}
