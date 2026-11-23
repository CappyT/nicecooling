//! The attached Leakshields, and the task that attaches them: at the start and when the USB bus
//! reports a Leakshield. A device is matched by its USB path. A removal sets the Leakshield
//! disconnected, and a replug at the same path gives it new transports. A device in its
//! bootloader is skipped.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use nicecooling_api::ids::leakshield_id;
use nicecooling_core::clock::Clock;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::dbus::Hub;
use crate::leakshield::feed::{CoveredLookup, LinkedSink, SoftwareLookup};
use crate::leakshield::{AttachParams, Attached, DeviceIo};
use crate::sources::hotplug::{UsbDevice, UsbEvent, UsbPath};
use crate::sources::leakshield as leakshield_source;
use crate::sources::usb::{OpenError, UsbBus};
use crate::value_table::ValueTable;

/// The kernel makes the hidraw node a moment after the USB device shows up.
pub const OPEN_RETRY_DELAY: Duration = Duration::from_millis(250);
const OPEN_ATTEMPTS: u32 = 40;

#[derive(Default)]
struct Inner {
    by_id: BTreeMap<String, Arc<Attached>>,
    /// Where each device was seen last. A removal keeps its entry, so the replug finds it.
    paths: BTreeMap<UsbPath, String>,
    /// The source and detection tasks of the devices that attached after the start.
    tasks: Vec<JoinHandle<()>>,
}

/// The Leakshields of the daemon. The Feed sender and the Software sensor slots look them up here.
#[derive(Default)]
pub struct Fleet(Mutex<Inner>);

impl Fleet {
    fn inner(&self) -> MutexGuard<'_, Inner> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn insert(&self, device: Arc<Attached>, path: Option<UsbPath>) {
        let mut inner = self.inner();
        if let Some(path) = path {
            inner.paths.insert(path, device.id());
        }
        inner.by_id.insert(device.id(), device);
    }

    pub fn get(&self, id: &str) -> Option<Arc<Attached>> {
        self.inner().by_id.get(id).cloned()
    }

    pub fn all(&self) -> Vec<Arc<Attached>> {
        self.inner().by_id.values().cloned().collect()
    }

    pub fn at_path(&self, path: &UsbPath) -> Option<Arc<Attached>> {
        let inner = self.inner();
        inner.by_id.get(inner.paths.get(path)?).cloned()
    }

    pub fn io_map(&self) -> BTreeMap<String, DeviceIo> {
        self.inner()
            .by_id
            .iter()
            .map(|(id, d)| (id.clone(), d.io().clone()))
            .collect()
    }

    pub fn software_lookup(self: &Arc<Self>) -> SoftwareLookup {
        let fleet = Arc::clone(self);
        Arc::new(move |id| fleet.get(id).map(|d| d.software_slots()))
    }

    pub fn covered_lookup(self: &Arc<Self>) -> CoveredLookup {
        let fleet = Arc::clone(self);
        Arc::new(move |id| fleet.get(id).map(|d| d.feed_covered()))
    }

    /// Tells each device whether a Loop links it.
    pub fn linked_sink(self: &Arc<Self>) -> LinkedSink {
        let fleet = Arc::clone(self);
        Arc::new(move |linked| {
            for device in fleet.all() {
                device.set_feed_linked(linked.contains(&device.id()));
            }
        })
    }

    fn keep(&self, tasks: impl IntoIterator<Item = JoinHandle<()>>) {
        self.inner().tasks.extend(tasks);
    }
}

impl Drop for Fleet {
    fn drop(&mut self) {
        for task in &self.inner().tasks {
            task.abort();
        }
    }
}

/// Opens Leakshields on a bus.
#[derive(Clone)]
pub struct UsbAttach {
    pub bus: Arc<dyn UsbBus>,
    pub retry_delay: Duration,
}

impl UsbAttach {
    /// Opens the device. It retries while the hidraw node is not there yet. `None` after a
    /// warning when the device does not open.
    pub async fn open(&self, device: &UsbDevice) -> Option<DeviceIo> {
        for attempt in 1..=OPEN_ATTEMPTS {
            let (bus, target) = (self.bus.clone(), device.clone());
            match tokio::task::spawn_blocking(move || bus.open(&target)).await {
                Ok(Ok(io)) => return Some(io),
                Ok(Err(OpenError::NotReady(_))) if attempt < OPEN_ATTEMPTS => {
                    tokio::time::sleep(self.retry_delay).await;
                }
                Ok(Err(e)) => {
                    eprintln!(
                        "nicecoolingd: cannot open the Leakshield at {}: {e}",
                        device.path
                    );
                    return None;
                }
                Err(_) => return None,
            }
        }
        None
    }

    /// The Leakshields on the bus that the daemon can use, opened. A device in its bootloader is
    /// skipped with a warning.
    pub async fn open_present(&self) -> Vec<(UsbPath, DeviceIo)> {
        let devices = match self.bus.list() {
            Ok(devices) => devices,
            Err(e) => {
                eprintln!("nicecoolingd: cannot list the USB devices: {e}");
                return Vec::new();
            }
        };
        let mut opened = Vec::new();
        for device in devices.into_iter().filter(|d| d.is_leakshield()) {
            if skip_bootloader(&device) {
                continue;
            }
            if let Some(io) = self.open(&device).await {
                opened.push((device.path, io));
            }
        }
        opened
    }
}

fn skip_bootloader(device: &UsbDevice) -> bool {
    if device.in_bootloader() {
        eprintln!(
            "nicecoolingd: the Leakshield at {} is in its bootloader: skipped",
            device.path
        );
    }
    device.in_bootloader()
}

/// Reacts to the USB events: attach, detach, reattach.
pub struct Attacher {
    pub fleet: Arc<Fleet>,
    pub usb: UsbAttach,
    pub params: AttachParams,
    pub hub: Arc<Hub>,
    pub values: ValueTable,
    pub clock: Arc<dyn Clock>,
    pub source_period: Duration,
    pub external_period: Duration,
}

impl Attacher {
    pub fn spawn(self, mut events: mpsc::UnboundedReceiver<UsbEvent>) -> JoinHandle<()> {
        tokio::spawn(async move {
            while let Some(event) = events.recv().await {
                match event {
                    UsbEvent::Connected(device) => self.connected(device).await,
                    UsbEvent::Disconnected { path } => self.disconnected(&path),
                }
            }
        })
    }

    fn disconnected(&self, path: &UsbPath) {
        if let Some(device) = self.fleet.at_path(path) {
            eprintln!("nicecoolingd: the Leakshield at {path} is gone");
            device.detach();
        }
    }

    async fn connected(&self, device: UsbDevice) {
        if !device.is_leakshield() || skip_bootloader(&device) {
            return;
        }
        // The start lists the devices after it starts the watch, so a device can arrive twice.
        if self
            .fleet
            .at_path(&device.path)
            .is_some_and(|d| d.info().connected)
        {
            return;
        }
        let Some(io) = self.usb.open(&device).await else {
            return;
        };
        let probe = io.clone();
        let report1 = tokio::task::spawn_blocking(move || probe.read_report1()).await;
        let serial = match report1 {
            Ok(Ok(report1)) => report1.serial_text(),
            Ok(Err(e)) => {
                eprintln!(
                    "nicecoolingd: the Leakshield at {} did not answer: {e}",
                    device.path
                );
                return;
            }
            Err(_) => return,
        };
        if let Some(known) = self.fleet.get(&leakshield_id(&serial)) {
            eprintln!(
                "nicecoolingd: the Leakshield {} is back at {}",
                known.id(),
                device.path
            );
            self.fleet.insert(known.clone(), Some(device.path));
            known.reattach(&io);
            return;
        }
        match Attached::open(io, &self.params) {
            Ok(attached) => self.add(attached, device.path),
            Err(e) => eprintln!(
                "nicecoolingd: the Leakshield at {} did not answer: {e}",
                device.path
            ),
        }
    }

    /// Starts the tasks of a device that attaches after the start and shows it to the API.
    fn add(&self, device: Arc<Attached>, path: UsbPath) {
        eprintln!("nicecoolingd: attached the Leakshield {}", device.id());
        self.fleet.insert(device.clone(), Some(path));
        let source = leakshield_source::spawn_source(
            device.clone(),
            self.values.clone(),
            self.clock.clone(),
            self.source_period,
        );
        let detection = device.spawn_external_detection(self.external_period);
        let feed_reset = device.spawn_feed_reset(self.values.clone());
        self.fleet.keep([source.task, detection, feed_reset]);
        self.hub.add_leakshield(device);
    }
}
