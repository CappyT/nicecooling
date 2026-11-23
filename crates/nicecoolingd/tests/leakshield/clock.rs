use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling_protocol::device::{BulkTransport, DeviceError, HidTransport};
use nicecooling_protocol::report2::Report2;
use nicecooling_protocol::sim::SimDevice;
use nicecoolingd::leakshield::attached::now_unix;
use nicecoolingd::leakshield::clock;
use nicecoolingd::leakshield::save_settings::SAVE_SETTINGS_DELAY;
use nicecoolingd::leakshield::{AttachParams, Attached, DeviceIo};

use crate::Rig;

/// `set_clock`, report 2 command 37000.
const SET_CLOCK: u32 = 37000;

fn put_device_clock(rig: &Rig, offset_s: i64) {
    let seconds = clock::device_seconds(now_unix() + offset_s).unwrap();
    rig.sim.edit_report1(|r| r.device_clock = seconds);
}

#[test]
fn set_clock_reads_back_within_1s() {
    let rig = Rig::new("clock-set");
    put_device_clock(&rig, -86_400);

    let now = now_unix();
    assert!(clock::sync(&rig.io, &rig.registry, now).unwrap());

    let report1 = rig.io.read_report1().unwrap();
    assert!(clock::offset(&report1, now_unix()).abs() <= 1);
    assert_eq!(
        rig.sim.commands(),
        [Report2::new(SET_CLOCK, clock::device_seconds(now).unwrap())]
    );
}

#[tokio::test]
async fn sync_clock_writes_at_any_offset() {
    let rig = Rig::new("clock-any-offset");
    let device = rig.attached();
    for (offset, writes) in [(5, 1), (-5, 2), (3 * 3600, 3), (-3 * 3600, 4)] {
        put_device_clock(&rig, offset);
        device.sync_clock().unwrap();
        assert_eq!(rig.sim.commands().len(), writes, "offset {offset} s");
        let last = rig.sim.commands().pop().unwrap();
        assert_eq!(last.command, SET_CLOCK);
        assert!(clock::offset(&rig.io.read_report1().unwrap(), now_unix()).abs() <= 1);
    }
}

#[tokio::test]
async fn drift_3h_sets_clock() {
    let rig = Rig::new("clock-drift-3h");
    put_device_clock(&rig, 3 * 3600);
    let device = rig.attached();

    assert!(device.poll(now_unix()).is_some());

    let commands = rig.sim.commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].command, SET_CLOCK);
    assert!(clock::offset(&rig.io.read_report1().unwrap(), now_unix()).abs() <= 1);
}

#[tokio::test]
async fn drift_20s_writes_nothing() {
    let rig = Rig::new("clock-drift-20s");
    put_device_clock(&rig, 20);
    let device = rig.attached();

    assert!(device.poll(now_unix()).is_some());

    assert!(rig.sim.output_reports().is_empty());
    assert!(device.clock_offset() >= 19);
}

#[test]
fn auto_sync_stops_at_60_s() {
    let rig = Rig::new("clock-threshold");
    let now = now_unix();
    for (offset, wrote) in [(59, false), (-59, false), (61, true), (-61, true)] {
        put_device_clock(&rig, offset);
        let report1 = rig.io.read_report1().unwrap();
        let sent = clock::auto_sync(&rig.io, &rig.registry, &report1, now).unwrap();
        assert_eq!(sent, wrote, "offset {offset} s");
    }
}

/// The HID side of the simulated device, with a slow report 1 and a clock on each output report.
struct SlowInput {
    sim: SimDevice,
    delay: Duration,
    sent_at: Mutex<Vec<i64>>,
}

impl HidTransport for SlowInput {
    fn read_input(&self, timeout: Duration) -> Result<Vec<u8>, DeviceError> {
        std::thread::sleep(self.delay);
        self.sim.read_input(timeout)
    }
    fn get_feature(&self, id: u8, len: usize) -> Result<Vec<u8>, DeviceError> {
        self.sim.get_feature(id, len)
    }
    fn set_feature(&self, report: &[u8]) -> Result<(), DeviceError> {
        self.sim.set_feature(report)
    }
    fn write_output(&self, report: &[u8]) -> Result<(), DeviceError> {
        self.sent_at.lock().unwrap().push(now_unix());
        self.sim.write_output(report)
    }
}

#[tokio::test]
async fn sync_clock_sends_the_time_of_the_send() {
    let rig = Rig::new("clock-latency");
    let hid = Arc::new(SlowInput {
        sim: rig.sim.clone(),
        delay: Duration::from_millis(2500),
        sent_at: Mutex::new(Vec::new()),
    });
    let bulk: Arc<dyn BulkTransport> = Arc::new(rig.sim.clone());
    let device = Attached::open(
        DeviceIo::new(hid.clone(), bulk),
        &AttachParams {
            state_dir: rig.dir.clone(),
            registry: Arc::clone(&rig.registry),
            save_delay: SAVE_SETTINGS_DELAY,
        },
    )
    .unwrap();

    device.sync_clock().unwrap();

    let sent_at = hid.sent_at.lock().unwrap()[0];
    let device_time = i64::from(rig.sim.commands()[0].parameter) + clock::DEVICE_EPOCH_UNIX;
    assert!(
        (sent_at - device_time).abs() <= 1,
        "the device clock is {} s behind the PC at the send",
        sent_at - device_time
    );
}
