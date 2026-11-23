//! A hwmon chip that loads after the daemon started (`asus_ec_sensors` can) feeds its Inputs
//! without a restart.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nicecooling_core::tick::percent_to_raw;
use nicecooling_hwmon::fake::FakeSysfs;
use nicecoolingd::daemon::{self, Options, Paths};
use nicecoolingd::tick_runner::TokioClock;
use nicecoolingd::watchdog::Notifier;

struct NoWatchdog;

impl Notifier for NoWatchdog {
    fn ping(&mut self) {}
}

const PROFILE: &str = r#"{
  "schema_version": 1, "id": "profile_default", "name": "Default",
  "channels": {
    "nct6798_pwm3": {"control": "curve", "input": "asusec_late_temp1",
                     "points": [[20, 20], [80, 100]], "smoothing_s": 0,
                     "step_limit_pct_per_s": 100, "fixed_duty": null}
  },
  "pump_floors": {}, "slot": 1
}"#;

#[tokio::test]
async fn late_chip_feeds_a_curve_without_a_restart() {
    let fake = FakeSysfs::reference_board();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let state = std::env::temp_dir().join(format!("nicecooling-rescan-{}", std::process::id()));
    let _ = fs::remove_dir_all(&state);
    fs::create_dir_all(state.join("profiles/profile_default")).unwrap();
    fs::write(state.join("profiles/profile_default/profile.json"), PROFILE).unwrap();

    let mut options = Options::new(Paths {
        sysfs_root: fake.root().to_owned(),
        state_dir: state.clone(),
        boards_dir: root.join("data/boards"),
        calibration_dir: root.join("data/calibration"),
    });
    options.tick_period = Duration::from_millis(50);
    options.source_period = Duration::from_millis(50);
    options.rescan_period = Duration::from_millis(50);
    options.boot_id = "test-boot".into();
    let mut daemon = daemon::start(&options, Arc::new(TokioClock::new()), NoWatchdog)
        .await
        .unwrap();

    // The Input does not exist yet: the Channel runs at 100%.
    daemon.run(Some(8)).await;
    assert!(
        daemon
            .runner
            .context()
            .warnings
            .is_active("lost-input", "nct6798_pwm3")
    );
    assert!(
        !daemon
            .hub
            .sensor_ids()
            .contains(&"asusec_late_temp1".to_owned())
    );

    fake.add_hwmon(
        20,
        "asusec_late",
        "devices/platform/asus_ec_sensors_late",
        None,
        &[("name", "asusec_late"), ("temp1_input", "50000")],
    );
    daemon.run(Some(30)).await;

    let ctx = daemon.runner.context();
    assert_eq!(ctx.values["asusec_late_temp1"], 50.0);
    assert!(!ctx.warnings.is_active("lost-input", "nct6798_pwm3"));
    // 50 C on the Curve gives 60%.
    let raw = fs::read_to_string(fake.attr_path("nct6798", "pwm3")).unwrap();
    assert_eq!(raw.trim(), percent_to_raw(60.0).to_string());
    assert!(
        daemon
            .hub
            .sensor_ids()
            .contains(&"asusec_late_temp1".to_owned())
    );
    fs::remove_dir_all(state).unwrap();
}
