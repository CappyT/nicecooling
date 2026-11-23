//! The daemon library on the fake tree of the reference Board, for 5 ticks, with the D-Bus API on
//! a private bus.

mod support;

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use nicecooling_api::alarms::AlarmsProxy;
use nicecooling_api::channel::ChannelProxy;
use nicecooling_api::manager::ManagerProxy;
use nicecooling_core::clock::SimClock;
use nicecooling_core::tick::percent_to_raw;
use nicecooling_hwmon::fake::FakeSysfs;
use nicecooling_protocol::feed::FeedMessage;
use nicecooling_protocol::sim::SimDevice;
use nicecoolingd::authz::TestAuthorizer;
use nicecoolingd::daemon::{self, Options, Paths};
use nicecoolingd::dbus;
use nicecoolingd::leakshield::{DeviceIo, attached, clock};
use nicecoolingd::tick_runner::TokioClock;
use nicecoolingd::watchdog::Notifier;

struct Pings(Arc<AtomicUsize>);

impl Notifier for Pings {
    fn ping(&mut self) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

const PROFILE: &str = r#"{
  "schema_version": 1, "id": "profile_default", "name": "Default",
  "channels": {
    "nct6798_pwm1": {"control": "curve", "input": "k10temp_temp1", "points": [[40, 20], [80, 100]],
                     "smoothing_s": null, "step_limit_pct_per_s": null, "fixed_duty": null},
    "nct6798_pwm3": {"control": "fixed", "input": null, "points": [], "smoothing_s": null,
                     "step_limit_pct_per_s": null, "fixed_duty": 50}
  },
  "pump_floors": {}, "slot": 1
}"#;

/// `loop_1` pumps with `nct6798_pwm6` (4821 rpm) and measures its flow on `nct6798_fan3` (629 rpm).
const STATE: &str = r#"{
  "schema_version": 1,
  "loops": [{"id": "loop_1", "name": "Main loop", "pump_channels": ["nct6798_pwm6"],
             "flow_sensor": "nct6798_fan3", "calibration_table": "alphacool_17558",
             "water_temperature": null, "leakshield": "leakshield_20417_31072", "pump_floor": 40}]
}"#;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smoke() {
    let fake = FakeSysfs::reference_board();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let state = std::env::temp_dir().join(format!("nicecooling-smoke-{}", std::process::id()));
    fs::create_dir_all(state.join("profiles/profile_default")).unwrap();
    fs::write(state.join("profiles/profile_default/profile.json"), PROFILE).unwrap();
    fs::write(state.join("state.json"), STATE).unwrap();

    let mut options = Options::new(Paths {
        sysfs_root: fake.root().to_owned(),
        state_dir: state.clone(),
        boards_dir: root.join("data/boards"),
        calibration_dir: root.join("data/calibration"),
    });
    options.tick_period = Duration::from_millis(50);
    options.source_period = Duration::from_millis(50);
    options.boot_id = "test-boot".into();
    // The simulated Leakshield. Its clock is right, so the daemon sends no clock command.
    let sim = SimDevice::fw1022();
    let now = clock::device_seconds(attached::now_unix()).unwrap();
    sim.edit_report1(|r| r.device_clock = now);
    options.leakshields = vec![DeviceIo::new(Arc::new(sim.clone()), Arc::new(sim.clone()))];
    let pings = Arc::new(AtomicUsize::new(0));
    let mut daemon = daemon::start(&options, Arc::new(TokioClock::new()), Pings(pings.clone()))
        .await
        .unwrap();

    // Before the first tick: the snapshot exists, no lease, no Channel changed.
    assert!(state.join("bios-snapshot.json").exists());
    assert!(!state.join("lease.json").exists());
    assert_eq!(
        fs::read_to_string(fake.attr_path("nct6798", "pwm3"))
            .unwrap()
            .trim(),
        "96"
    );

    let bus = support::bus::PrivateBus::start();
    let server = bus.connect().await;
    let client = bus.connect().await;
    let _served = dbus::serve(&daemon.hub, &server, TestAuthorizer::allow_all())
        .await
        .unwrap();
    let manager = ManagerProxy::new(&client).await.unwrap();
    let mut signals = manager.receive_sensors_updated().await.unwrap();

    daemon.run(Some(5)).await;

    // The API sees the 5 ticks and the state of the Channels.
    for _ in 0..5 {
        tokio::time::timeout(Duration::from_secs(5), signals.next())
            .await
            .expect("one SensorsUpdated for each tick")
            .unwrap();
    }
    assert_eq!(manager.version().await.unwrap(), env!("CARGO_PKG_VERSION"));
    let pwm3 = ChannelProxy::builder(&client)
        .path("/io/github/cappyt/NiceCooling1/channel/nct6798_pwm3")
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    assert_eq!(pwm3.raw().await.unwrap(), 128);
    assert_eq!(pwm3.control().await.unwrap(), "fixed");

    let read = |chip: &str, attribute: &str| {
        fs::read_to_string(fake.attr_path(chip, attribute))
            .unwrap()
            .trim()
            .to_owned()
    };
    assert_eq!(pings.load(Ordering::SeqCst), 5);
    assert!(state.join("lease.json").exists());
    assert_eq!(
        (read("nct6798", "pwm3"), read("nct6798", "pwm3_enable")),
        ("128".into(), "1".into())
    );
    // Tctl is 61.875 C: the Curve gives 63.75%.
    assert_eq!(read("nct6798", "pwm1"), percent_to_raw(63.75).to_string());
    assert_eq!(read("nct6798", "pwm1_enable"), "1");
    // The pump Channel and the GPU stay as they were.
    assert_eq!(
        (read("nct6798", "pwm6"), read("nct6798", "pwm6_enable")),
        ("255".into(), "0".into())
    );
    assert_eq!(
        (read("amdgpu", "pwm1"), read("amdgpu", "pwm1_enable")),
        ("0".into(), "2".into())
    );
    assert!(
        daemon
            .runner
            .context()
            .warnings
            .is_active("manual-at-snapshot", "nct6798_pwm6")
    );
    assert!(
        !daemon
            .runner
            .context()
            .warnings
            .is_active("lost-input", "nct6798_pwm1")
    );
    assert_eq!(daemon.runner.context().values["k10temp_temp1"], 61.875);

    // The Feed message of the Loop goes to the simulated Leakshield: the pump speed in slot 0
    // and the calibrated flow in slot 1 (L/h with one decimal).
    let flow = daemon.runner.context().values["loop_1_flow"];
    assert!(flow > 0.0);
    let mut sent = Vec::new();
    for _ in 0..100 {
        sent = sim.bulk_messages();
        if !sent.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        !sent.is_empty(),
        "the Feed message did not reach the device"
    );
    let message = FeedMessage::decode(sent.last().unwrap()).unwrap();
    assert_eq!((message.value(0), message.unit(0)), (4821, 3));
    assert_eq!(
        (message.value(1), message.unit(1)),
        ((flow * 10.0).round() as i16, 12)
    );
    // The device was read, and the clock command did not go out.
    assert!(sim.commands().is_empty());
    assert_eq!(daemon.fleet.all().len(), 1);
    fs::remove_dir_all(state).unwrap();
}

/// One simulated second and one tick, `seconds` times. The pause lets the sources publish with the
/// new time.
async fn step(daemon: &mut daemon::Daemon, clock: &SimClock, seconds: u32) {
    for _ in 0..seconds {
        clock.advance(Duration::from_secs(1));
        tokio::time::sleep(Duration::from_millis(100)).await;
        daemon.runner.tick();
    }
    tokio::time::sleep(Duration::from_millis(150)).await;
}

/// The daemon raises a pump stop Alarm on the fake tree, with a poweroff pending, and a cancel
/// from the API keeps the PC on. The tick runs on a simulated clock and logind is a mock.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn smoke_pump_stop_alarm_is_raised_and_cancelled() {
    use std::os::unix::fs::PermissionsExt;

    let fake = FakeSysfs::reference_board();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let state = std::env::temp_dir().join(format!("nicecooling-smoke-pump-{}", std::process::id()));
    let _ = fs::remove_dir_all(&state);
    fs::create_dir_all(state.join("profiles/profile_default")).unwrap();
    fs::write(state.join("profiles/profile_default/profile.json"), PROFILE).unwrap();
    fs::write(state.join("state.json"), STATE).unwrap();
    let mut options = Options::new(Paths {
        sysfs_root: fake.root().to_owned(),
        state_dir: state.clone(),
        boards_dir: root.join("data/boards"),
        calibration_dir: root.join("data/calibration"),
    });
    options.source_period = Duration::from_millis(30);
    options.boot_id = "test-boot".into();
    let sim = SimDevice::fw1022();
    let now = clock::device_seconds(attached::now_unix()).unwrap();
    sim.edit_report1(|r| r.device_clock = now);
    options.leakshields = vec![DeviceIo::new(Arc::new(sim.clone()), Arc::new(sim.clone()))];
    let sim_clock = SimClock::default();
    let mut daemon = daemon::start(&options, Arc::new(sim_clock.clone()), Pings(Arc::default()))
        .await
        .unwrap();

    let bus = support::bus::PrivateBus::start();
    let server = bus.connect().await;
    let client = bus.connect().await;
    let logind = support::logind_mock::Login1Handles::start(bus.connect().await).await;
    let _served = dbus::serve(&daemon.hub, &server, TestAuthorizer::allow_all())
        .await
        .unwrap();
    let alarms = AlarmsProxy::builder(&client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let mut cancelled = alarms.receive_poweroff_cancelled().await.unwrap();
    let mut cleared = alarms.receive_alarm_cleared().await.unwrap();

    let set_pump_rpm = |rpm: u32| {
        let path = fake.attr_path("nct6798", "fan6_input");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        fs::write(&path, rpm.to_string()).unwrap();
    };

    // The pump Channel was in manual mode at the snapshot: a Warning, and no Alarm.
    let pump_stops = |rows: Vec<nicecooling_api::alarms::ActiveAlarm>| -> Vec<_> {
        rows.into_iter().filter(|r| r.1 == "pump-stop").collect()
    };
    step(&mut daemon, &sim_clock, 2).await;
    assert!(pump_stops(alarms.active().await.unwrap()).is_empty());
    // The pump stops. After 5 s the Alarm rises, with the default response: poweroff in 30 s.
    set_pump_rpm(0);
    step(&mut daemon, &sim_clock, 7).await;
    let active = pump_stops(alarms.active().await.unwrap());
    assert_eq!(active.len(), 1);
    let (instance, event, severity, source, response, _, poweroff_at_us, _) = active[0].clone();
    assert_eq!(
        (
            event.as_str(),
            severity.as_str(),
            source.as_str(),
            response.as_str()
        ),
        ("pump-stop", "alarm", "loop_1", "poweroff")
    );
    assert!(poweroff_at_us > 0);

    alarms.cancel_poweroff(&instance).await.unwrap();
    let signal = tokio::time::timeout(Duration::from_secs(5), cancelled.next())
        .await
        .expect("PoweroffCancelled")
        .unwrap();
    assert_eq!(signal.args().unwrap().instance, instance);
    assert_eq!(pump_stops(alarms.active().await.unwrap())[0].6, 0);
    // The delay is long gone, and the PC stays on.
    step(&mut daemon, &sim_clock, 40).await;
    assert_eq!(logind.call_count(), 0);

    // The pump runs again: the Alarm closes after 10 s.
    set_pump_rpm(4821);
    step(&mut daemon, &sim_clock, 12).await;
    let signal = tokio::time::timeout(Duration::from_secs(5), cleared.next())
        .await
        .expect("AlarmCleared")
        .unwrap();
    assert_eq!(signal.args().unwrap().instance, instance);
    assert!(pump_stops(alarms.active().await.unwrap()).is_empty());
    assert_eq!(logind.call_count(), 0);
    fs::remove_dir_all(state).unwrap();
}
