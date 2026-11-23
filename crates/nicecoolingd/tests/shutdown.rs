//! `nicecoolingd` on SIGTERM while a hwmon read is stuck in the kernel.

use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use nicecooling_hwmon::fake::FakeSysfs;

const PROFILE: &str = r#"{
  "schema_version": 1, "id": "profile_default", "name": "Default",
  "channels": {
    "nct6798_pwm3": {"control": "fixed", "input": null, "points": [], "smoothing_s": null,
                     "step_limit_pct_per_s": null, "fixed_duty": 50}
  },
  "pump_floors": {}, "slot": 1
}"#;

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !done() {
        assert!(Instant::now() < deadline, "timeout: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn exited(child: &mut Child) -> bool {
    child.try_wait().unwrap().is_some()
}

#[test]
fn sigterm_exits_while_a_device_read_is_stuck() {
    let fake = FakeSysfs::reference_board();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let state = std::env::temp_dir().join(format!("nicecooling-shutdown-{}", std::process::id()));
    let _ = fs::remove_dir_all(&state);
    fs::create_dir_all(state.join("profiles/profile_default")).unwrap();
    fs::write(state.join("profiles/profile_default/profile.json"), PROFILE).unwrap();

    let mut daemon = Command::new(env!("CARGO_BIN_EXE_nicecoolingd"))
        .arg("--sysfs-root")
        .arg(fake.root())
        .arg("--state-dir")
        .arg(&state)
        .arg("--boards-dir")
        .arg(repo.join("data/boards"))
        .arg("--calibration-dir")
        .arg(repo.join("data/calibration"))
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    // The lease appears with the first write, so the SIGTERM handler is installed by then.
    wait_for("the first control write", || {
        state.join("lease.json").exists()
    });

    // The next read of this sensor blocks in `open`, as a read of a hung device does.
    let sensor = fake.attr_path("k10temp", "temp1_input");
    fs::remove_file(&sensor).unwrap();
    assert!(
        Command::new("mkfifo")
            .arg(&sensor)
            .status()
            .unwrap()
            .success()
    );
    std::thread::sleep(Duration::from_millis(2500));

    assert!(
        Command::new("kill")
            .args(["-TERM", &daemon.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    let stopped = Instant::now();
    wait_for("the daemon exits", || {
        let done = exited(&mut daemon);
        if !done && stopped.elapsed() > Duration::from_secs(5) {
            daemon.kill().unwrap();
            panic!("the daemon waits for the stuck read after SIGTERM");
        }
        done
    });
    fs::remove_dir_all(state).unwrap();
}
