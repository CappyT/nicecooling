//! The pump stop override on the fake tree of the reference Board: every Channel goes to 100%, the
//! GPU stays, and the Channels without a Curve get their BIOS state back when the pump runs again.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nicecooling_core::clock::SimClock;
use nicecooling_hwmon::fake::FakeSysfs;
use nicecoolingd::daemon::{self, Daemon, Options, Paths};
use nicecoolingd::watchdog::Notifier;

struct NoPing;

impl Notifier for NoPing {
    fn ping(&mut self) {}
}

const PROFILE: &str = r#"{
  "schema_version": 1, "id": "profile_default", "name": "Default",
  "channels": {
    "nct6798_pwm1": {"control": "curve", "input": "k10temp_temp1", "points": [[40, 20], [80, 100]],
                     "smoothing_s": 0, "step_limit_pct_per_s": 5, "fixed_duty": null},
    "nct6798_pwm3": {"control": "fixed", "input": null, "points": [], "smoothing_s": null,
                     "step_limit_pct_per_s": null, "fixed_duty": 50}
  },
  "pump_floors": {}, "slot": 1
}"#;

const STATE: &str = r#"{
  "schema_version": 1,
  "loops": [{"id": "loop_1", "name": "Main loop", "pump_channels": ["nct6798_pwm6"],
             "flow_sensor": null, "calibration_table": null,
             "water_temperature": null, "leakshield": null, "pump_floor": 40}]
}"#;

struct Rig {
    fake: FakeSysfs,
    clock: SimClock,
    daemon: Daemon,
    state: PathBuf,
}

impl Rig {
    async fn start() -> Self {
        let fake = FakeSysfs::reference_board();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        let state = std::env::temp_dir().join(format!("nicecooling-pump-{}", std::process::id()));
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
        let clock = SimClock::default();
        let daemon = daemon::start(&options, Arc::new(clock.clone()), NoPing)
            .await
            .unwrap();
        Self {
            fake,
            clock,
            daemon,
            state,
        }
    }

    /// One simulated second and one tick. The pause lets the sources publish with the new time.
    async fn step(&mut self, seconds: u32) {
        for _ in 0..seconds {
            self.clock.advance(Duration::from_secs(1));
            tokio::time::sleep(Duration::from_millis(100)).await;
            self.daemon.runner.tick();
        }
    }

    /// Changes the tachometer of Channel 6. The fake file is read-only, as the kernel makes it.
    fn set_pump_rpm(&self, rpm: u32) {
        use std::os::unix::fs::PermissionsExt;
        let path = self.fake.attr_path("nct6798", "fan6_input");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        fs::write(&path, rpm.to_string()).unwrap();
    }

    fn read(&self, chip: &str, attribute: &str) -> String {
        fs::read_to_string(self.fake.attr_path(chip, attribute))
            .unwrap()
            .trim()
            .to_owned()
    }

    /// `pwmN` and `pwmN_enable` of the Channel `n` of the Super I/O chip.
    fn channel(&self, n: u32) -> (String, String) {
        (
            self.read("nct6798", &format!("pwm{n}")),
            self.read("nct6798", &format!("pwm{n}_enable")),
        )
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.state);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gpu_pwm_untouched() {
    let mut rig = Rig::start().await;
    let before: Vec<_> = (1..=7).map(|n| rig.channel(n)).collect();
    let gpu = (
        rig.read("amdgpu", "pwm1"),
        rig.read("amdgpu", "pwm1_enable"),
    );
    rig.step(2).await;
    // Channel 2 has no Curve: it stays on its Hardware curve.
    assert_eq!(rig.channel(2), before[1]);

    // The pump stops: the tachometer of Channel 6 reads 0 while its `pwm6` is above zero.
    rig.set_pump_rpm(0);
    rig.step(4).await;
    assert_eq!(rig.channel(2), before[1]);
    rig.step(2).await;
    assert!(
        rig.daemon
            .runner
            .context()
            .alarms
            .is_active("pump-stop", "loop_1")
    );
    // Every Channel is at 100% in the tick where the Alarm rises.
    for n in 1..=7 {
        let (raw, _) = rig.channel(n);
        assert_eq!(raw, "255", "pwm{n}");
    }
    assert_eq!(
        (
            rig.read("amdgpu", "pwm1"),
            rig.read("amdgpu", "pwm1_enable")
        ),
        gpu
    );

    // The pump runs again. 10 s later the Alarm closes.
    rig.set_pump_rpm(4821);
    rig.step(9).await;
    assert!(
        rig.daemon
            .runner
            .context()
            .alarms
            .is_active("pump-stop", "loop_1")
    );
    rig.step(2).await;
    assert!(!rig.daemon.runner.context().alarms.any_active("pump-stop"));
    // The Channels without a Curve or a fixed Duty have their BIOS state back.
    for n in [2, 4, 5, 6, 7] {
        assert_eq!(rig.channel(n), before[n as usize - 1], "pwm{n}");
    }
    // The Curve and the fixed Duty ramp down from 100% (5% a second).
    let raw = |n| rig.channel(n).0.parse::<u32>().unwrap();
    assert!(raw(3) > 128 && raw(3) < 255, "pwm3 is {}", raw(3));
    assert_eq!(rig.channel(3).1, "1");
    assert_eq!(
        (
            rig.read("amdgpu", "pwm1"),
            rig.read("amdgpu", "pwm1_enable")
        ),
        gpu
    );
}
