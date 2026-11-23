//! Hot-plug: the Leakshield is matched by its USB path, so a replug reattaches the same device.
//! `UsbEvent` is the event of this module. The `nusb` adapter converts into it, and tests send it.

use std::collections::HashMap;
use std::fmt;

use nicecooling_protocol::device::{PRODUCT_ID, VENDOR_ID};
use nusb::hotplug::HotplugEvent;
use nusb::{DeviceId, DeviceInfo};

/// `<bus>-<port>.<port>...`, the name that sysfs gives the device: `1-2.3`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct UsbPath(pub String);

impl UsbPath {
    pub fn new(bus_id: &str, port_chain: &[u8]) -> Self {
        let ports: Vec<String> = port_chain.iter().map(u8::to_string).collect();
        Self(format!("{bus_id}-{}", ports.join(".")))
    }
}

impl fmt::Display for UsbPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsbEvent {
    Connected {
        path: UsbPath,
        vendor: u16,
        product: u16,
    },
    Disconnected {
        path: UsbPath,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// The Leakshield is there: open it and resume reading and the Feed message.
    Attach,
    /// The Leakshield is gone: its transfers fail until it comes back.
    Detach,
    Ignore,
}

/// Follows one Leakshield by USB path. The first Leakshield that connects sets the path.
#[derive(Debug, Default)]
pub struct Attachment {
    path: Option<UsbPath>,
    attached: bool,
}

impl Attachment {
    /// Follows the Leakshield at `path`, which is already attached.
    pub fn at(path: UsbPath) -> Self {
        Self {
            path: Some(path),
            attached: true,
        }
    }

    pub fn path(&self) -> Option<&UsbPath> {
        self.path.as_ref()
    }

    pub fn on_event(&mut self, event: &UsbEvent) -> Action {
        match event {
            UsbEvent::Connected {
                path,
                vendor,
                product,
            } if (*vendor, *product) == (VENDOR_ID, PRODUCT_ID) => {
                if self.path.get_or_insert_with(|| path.clone()) == path && !self.attached {
                    self.attached = true;
                    Action::Attach
                } else {
                    Action::Ignore
                }
            }
            UsbEvent::Disconnected { path }
                if self.path.as_ref() == Some(path) && self.attached =>
            {
                self.attached = false;
                Action::Detach
            }
            _ => Action::Ignore,
        }
    }
}

/// Converts `nusb` events. `nusb` names a removed device by an id only, so the adapter keeps the
/// path of each device it saw.
#[derive(Default)]
pub struct NusbEvents {
    paths: HashMap<DeviceId, UsbPath>,
}

impl NusbEvents {
    pub fn connected(&mut self, info: &DeviceInfo) -> UsbEvent {
        let path = UsbPath::new(info.bus_id(), info.port_chain());
        self.paths.insert(info.id(), path.clone());
        UsbEvent::Connected {
            path,
            vendor: info.vendor_id(),
            product: info.product_id(),
        }
    }

    pub fn convert(&mut self, event: HotplugEvent) -> Option<UsbEvent> {
        match event {
            HotplugEvent::Connected(info) => Some(self.connected(&info)),
            HotplugEvent::Disconnected(id) => self
                .paths
                .remove(&id)
                .map(|path| UsbEvent::Disconnected { path }),
        }
    }
}

/// Watches the USB bus and calls `on_event` for each event until the watch ends. The caller owns
/// the initial enumeration: it passes the devices that exist at the start to `NusbEvents::connected`.
pub async fn watch(mut on_event: impl FnMut(UsbEvent)) -> std::io::Result<()> {
    use futures_core::Stream;
    use std::pin::Pin;

    let mut watch = nusb::watch_devices().map_err(std::io::Error::from)?;
    let mut events = NusbEvents::default();
    while let Some(event) = std::future::poll_fn(|cx| Pin::new(&mut watch).poll_next(cx)).await {
        if let Some(event) = events.convert(event) {
            on_event(event);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn connected(path: &str) -> UsbEvent {
        UsbEvent::Connected {
            path: UsbPath(path.to_owned()),
            vendor: VENDOR_ID,
            product: PRODUCT_ID,
        }
    }

    fn disconnected(path: &str) -> UsbEvent {
        UsbEvent::Disconnected {
            path: UsbPath(path.to_owned()),
        }
    }

    #[test]
    fn path_of_bus_and_ports() {
        assert_eq!(UsbPath::new("1", &[2, 3]).0, "1-2.3");
        assert_eq!(UsbPath::new("3", &[4]).0, "3-4");
    }

    #[test]
    fn replug_at_the_same_path_attaches_again() {
        let mut attachment = Attachment::at(UsbPath("1-2.3".to_owned()));
        assert_eq!(attachment.on_event(&disconnected("1-2.3")), Action::Detach);
        assert_eq!(attachment.on_event(&disconnected("1-2.3")), Action::Ignore);
        assert_eq!(attachment.on_event(&connected("1-2.3")), Action::Attach);
    }

    #[test]
    fn other_path_and_other_device_are_ignored() {
        let mut attachment = Attachment::at(UsbPath("1-2.3".to_owned()));
        assert_eq!(attachment.on_event(&disconnected("1-4")), Action::Ignore);
        attachment.on_event(&disconnected("1-2.3"));
        assert_eq!(attachment.on_event(&connected("1-4")), Action::Ignore);
        let other = UsbEvent::Connected {
            path: UsbPath("1-2.3".to_owned()),
            vendor: 0x1234,
            product: 0x5678,
        };
        assert_eq!(attachment.on_event(&other), Action::Ignore);
    }

    #[test]
    fn first_leakshield_sets_the_path() {
        let mut attachment = Attachment::default();
        assert_eq!(attachment.on_event(&connected("2-1")), Action::Attach);
        assert_eq!(attachment.path(), Some(&UsbPath("2-1".to_owned())));
    }
}
