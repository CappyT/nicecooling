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
use crate::support::SET_CLOCK;

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

/// The `set_clock` commands that the device got.
fn set_clocks(rig: &Rig) -> usize {
    rig.sim
        .commands()
        .iter()
        .filter(|c| c.command == SET_CLOCK)
        .count()
}

#[tokio::test(start_paused = true)]
async fn the_clock_is_set_at_the_attach_and_a_drift_of_20s_waits_for_the_period() {
    let rig = Rig::new("clock-drift-20s");
    put_device_clock(&rig, 20);
    let device = rig.attached();

    assert!(device.poll(now_unix()).is_some());
    assert_eq!(set_clocks(&rig), 1, "at the attach");
    assert!(clock::offset(&rig.io.read_report1().unwrap(), now_unix()).abs() <= 1);

    // A drift of 20 s is under the 60 s threshold.
    put_device_clock(&rig, 20);
    assert!(device.poll(now_unix()).is_some());
    assert_eq!(set_clocks(&rig), 1);
    assert!(device.clock_offset() >= 19);
}

#[tokio::test(start_paused = true)]
async fn the_clock_is_set_every_30_minutes_and_after_a_replug() {
    let rig = Rig::new("clock-periodic");
    put_device_clock(&rig, 0);
    let device = rig.attached();
    device.poll(now_unix());
    assert_eq!(set_clocks(&rig), 1);

    tokio::time::advance(Duration::from_secs(29 * 60)).await;
    device.poll(now_unix());
    assert_eq!(set_clocks(&rig), 1, "29 minutes after the last set");

    tokio::time::advance(Duration::from_secs(61)).await;
    device.poll(now_unix());
    assert_eq!(set_clocks(&rig), 2, "30 minutes after the last set");

    // The device comes back: it gets the clock at once.
    device.reattach(&rig.io.clone());
    assert_eq!(set_clocks(&rig), 3);
}

#[tokio::test(start_paused = true)]
async fn a_drift_over_60s_is_set_at_once_but_not_every_poll() {
    let rig = Rig::new("clock-drift-over-60s");
    put_device_clock(&rig, 0);
    let device = rig.attached();
    device.poll(now_unix());
    assert_eq!(set_clocks(&rig), 1);

    put_device_clock(&rig, 300);
    tokio::time::advance(Duration::from_secs(10)).await;
    device.poll(now_unix());
    assert_eq!(set_clocks(&rig), 1, "less than a minute after the last set");

    tokio::time::advance(Duration::from_secs(60)).await;
    device.poll(now_unix());
    assert_eq!(set_clocks(&rig), 2);
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
