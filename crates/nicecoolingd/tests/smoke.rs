//! The daemon library on the fake tree of the reference Board, for 5 ticks, with the D-Bus API on
//! a private bus.

mod support;

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use futures_util::StreamExt;
use nicecooling_api::channel::ChannelProxy;
use nicecooling_api::manager::ManagerProxy;
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
    assert_eq!(daemon.leakshields.len(), 1);
    fs::remove_dir_all(state).unwrap();
}
