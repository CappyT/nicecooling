//! Races between two `SetMode` calls, on the simulated device.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use nicecooling_core::profile::Slot;
use nicecooling_protocol::device::{BulkTransport, DeviceError, HidTransport};
use nicecooling_protocol::report3::Report3;
use nicecooling_protocol::sim::SimDevice;
use nicecoolingd::leakshield::external::ExternalDetector;
use nicecoolingd::leakshield::save_settings::SAVE_SETTINGS_DELAY;
use nicecoolingd::leakshield::{AttachParams, Attached, DeviceIo};
use nicecoolingd::state::{SlotStore, StateStore};
use tokio::time::sleep;

use crate::{LEAKSHIELD_ID, Rig};

const DEFAULT_MODE: usize = 724;

fn slots(rig: &Rig) -> SlotStore {
    SlotStore::new(Arc::new(StateStore::new(&rig.dir)))
}

fn stored(slots: &SlotStore, n: u32) -> Report3 {
    slots
        .load(LEAKSHIELD_ID, Slot::new(n).unwrap())
        .unwrap()
        .unwrap()
        .report3
}

fn open(rig: &Rig, hid: Arc<dyn HidTransport>) -> Arc<Attached> {
    let bulk: Arc<dyn BulkTransport> = Arc::new(rig.sim.clone());
    Attached::open(
        DeviceIo::new(hid, bulk),
        &AttachParams {
            state_dir: rig.dir.clone(),
            registry: Arc::clone(&rig.registry),
            save_delay: SAVE_SETTINGS_DELAY,
        },
    )
    .unwrap()
}

/// Holds the thread named `x` in its first read of report 1 after its `set_mode` went out.
struct HoldAfterSend {
    sim: SimDevice,
    sent: AtomicBool,
    hold: Mutex<Option<std_mpsc::Receiver<()>>>,
    signal: Mutex<Option<std_mpsc::Sender<()>>>,
}

fn is_x() -> bool {
    thread::current().name() == Some("x")
}

impl HidTransport for HoldAfterSend {
    fn read_input(&self, timeout: Duration) -> Result<Vec<u8>, DeviceError> {
        if is_x()
            && self.sent.load(Ordering::SeqCst)
            && let Some(hold) = self.hold.lock().unwrap().take()
        {
            let _ = hold.recv();
        }
        self.sim.read_input(timeout)
    }
    fn get_feature(&self, id: u8, len: usize) -> Result<Vec<u8>, DeviceError> {
        self.sim.get_feature(id, len)
    }
    fn set_feature(&self, report: &[u8]) -> Result<(), DeviceError> {
        self.sim.set_feature(report)
    }
    fn write_output(&self, report: &[u8]) -> Result<(), DeviceError> {
        let result = self.sim.write_output(report);
        if is_x() {
            self.sent.store(true, Ordering::SeqCst);
            if let Some(signal) = self.signal.lock().unwrap().take() {
                let _ = signal.send(());
            }
        }
        result
    }
}

/// Fix 1: two `SetMode` calls overlap (two D-Bus clients). The device gets `shield`, then
/// `monitor`, but the call of `shield` replaces the poll last. The daemon follows `shield`, the
/// device stores `monitor`, and the slot of the device never gets it.
#[tokio::test(start_paused = true)]
async fn overlapping_set_modes_follow_the_mode_that_the_device_got_last() {
    let rig = Rig::new("rv-overlap");
    rig.sim.set_device_slot(1);
    let (release, hold) = std_mpsc::channel();
    let (signal, sent) = std_mpsc::channel();
    let hid = Arc::new(HoldAfterSend {
        sim: rig.sim.clone(),
        sent: AtomicBool::new(false),
        hold: Mutex::new(Some(hold)),
        signal: Mutex::new(Some(signal)),
    });
    let device = open(&rig, hid);
    let slots = slots(&rig);

    let x = thread::Builder::new()
        .name("x".into())
        .spawn({
            let (device, slots) = (device.clone(), slots.clone());
            move || device.set_mode("shield", &slots)
        })
        .unwrap();
    sent.recv().unwrap();
    let (done, y_done) = std_mpsc::channel();
    let y = thread::spawn({
        let (device, slots) = (device.clone(), slots.clone());
        move || {
            let result = device.set_mode("monitor", &slots);
            let _ = done.send(());
            result
        }
    });
    // A fix that serializes the calls blocks `y` here: the wait ends without it.
    let _ = y_done.recv_timeout(Duration::from_secs(2));
    release.send(()).unwrap();
    x.join().unwrap().unwrap();
    y.join().unwrap().unwrap();

    sleep(Duration::from_secs(120)).await;
    assert_eq!(rig.sim.report3()[DEFAULT_MODE], 2);
    assert_eq!(
        stored(&slots, 1).bytes_at(DEFAULT_MODE, 1),
        [2],
        "the slot of the device misses the Mode that the device stored"
    );
}

/// Holds the first read of report 3 after `arm` until `release`, after the device answered it.
struct HoldReport3 {
    sim: SimDevice,
    armed: AtomicBool,
    entered: Mutex<Option<std_mpsc::Sender<()>>>,
    hold: Mutex<Option<std_mpsc::Receiver<()>>>,
}

impl HidTransport for HoldReport3 {
    fn read_input(&self, timeout: Duration) -> Result<Vec<u8>, DeviceError> {
        self.sim.read_input(timeout)
    }
    fn get_feature(&self, id: u8, len: usize) -> Result<Vec<u8>, DeviceError> {
        let result = self.sim.get_feature(id, len);
        if id == 3 && self.armed.swap(false, Ordering::SeqCst) {
            if let Some(entered) = self.entered.lock().unwrap().take() {
                let _ = entered.send(());
            }
            if let Some(hold) = self.hold.lock().unwrap().take() {
                let _ = hold.recv();
            }
        }
        result
    }
    fn set_feature(&self, report: &[u8]) -> Result<(), DeviceError> {
        self.sim.set_feature(report)
    }
    fn write_output(&self, report: &[u8]) -> Result<(), DeviceError> {
        self.sim.write_output(report)
    }
}

/// Fix 1: a newer `SetMode` arrives while the older poll is inside its read of report 3. The
/// older poll ends after the newer call set the expected Mode, and clears it. The next poll of
/// the external change detection then reports the daemon's own Mode as an external change.
#[tokio::test(start_paused = true)]
async fn an_older_poll_does_not_clear_the_expected_mode_of_a_newer_one() {
    let rig = Rig::new("rv-expected");
    rig.sim.set_device_slot(1);
    // A last known report 3 on disk, which `Attached::open` loads.
    ExternalDetector::new(
        rig.io.clone(),
        Arc::clone(&rig.registry),
        Arc::clone(&rig.last_known),
    )
    .poll()
    .unwrap();
    rig.sim.set_mode_store_reads(1);
    let (entered_tx, entered) = std_mpsc::channel();
    let (release, hold) = std_mpsc::channel();
    let hid = Arc::new(HoldReport3 {
        sim: rig.sim.clone(),
        armed: AtomicBool::new(false),
        entered: Mutex::new(Some(entered_tx)),
        hold: Mutex::new(Some(hold)),
    });
    let device = open(&rig, hid.clone());
    let slots = slots(&rig);
    let mut events = device.take_events().unwrap();

    device.set_mode("shield", &slots).unwrap();
    hid.armed.store(true, Ordering::SeqCst);
    // The newer call comes while the older poll (at 1 s) reads report 3, as a D-Bus call does.
    let newer = thread::spawn({
        let (device, slots) = (device.clone(), slots.clone());
        move || {
            entered.recv().unwrap();
            let result = device.set_mode("monitor", &slots);
            release.send(()).unwrap();
            result
        }
    });
    sleep(Duration::from_millis(1500)).await;
    newer.join().unwrap().unwrap();

    // The detection polls before the first poll of the newer Mode (at 2 s).
    let _detection = device.spawn_external_detection(Duration::from_secs(10));
    sleep(Duration::from_millis(100)).await;
    assert_eq!(rig.sim.report3()[DEFAULT_MODE], 2);
    assert_eq!(events.try_recv().ok(), None);
}
