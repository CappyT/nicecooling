//! `nicecoolingd --restore-bios` as a process, on a fake sysfs tree.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use nicecooling_hwmon::fake::FakeSysfs;

fn run(fake: &FakeSysfs, state: &std::path::Path) -> std::process::ExitStatus {
    Command::new(env!("CARGO_BIN_EXE_nicecoolingd"))
        .args(["--restore-bios", "--sysfs-root"])
        .arg(fake.root())
        .arg("--state-dir")
        .arg(state)
        .status()
        .unwrap()
}

#[test]
fn restore_bios_without_lease_exits_0() {
    let fake = FakeSysfs::reference_board();
    let state = std::env::temp_dir().join(format!("nicecooling-restore-{}", std::process::id()));
    fs::create_dir_all(&state).unwrap();
    let pwm = fake.attr_path("nct6798", "pwm3");
    assert!(run(&fake, &state).success());
    assert_eq!(fs::read_to_string(pwm).unwrap().trim(), "96");
    assert!(fs::read_dir(&state).unwrap().next().is_none());
    fs::remove_dir_all(state).unwrap();
}

#[test]
fn restore_bios_without_snapshot_exits_0_and_sets_raw_255() {
    let fake = FakeSysfs::reference_board();
    let state = std::env::temp_dir().join(format!("nicecooling-restore-fb-{}", std::process::id()));
    fs::create_dir_all(&state).unwrap();
    fs::write(
        state.join("lease.json"),
        r#"{"schema_version":1,"created_us":1,"boot_id":"x"}"#,
    )
    .unwrap();
    assert!(run(&fake, &state).success());
    assert_eq!(
        fs::read_to_string(fake.attr_path("nct6798", "pwm3"))
            .unwrap()
            .trim(),
        "255"
    );
    assert_eq!(
        fs::read_to_string(fake.attr_path("nct6798", "pwm3_enable"))
            .unwrap()
            .trim(),
        "1"
    );
    assert!(state.join("lease.json").exists());
    fs::remove_dir_all(state).unwrap();
}

#[test]
fn restore_bios_fallback_exits_1_when_a_write_fails() {
    let fake = FakeSysfs::reference_board();
    let state = std::env::temp_dir().join(format!("nicecooling-restore-ff-{}", std::process::id()));
    fs::create_dir_all(&state).unwrap();
    fs::write(
        state.join("lease.json"),
        r#"{"schema_version":1,"created_us":1,"boot_id":"x"}"#,
    )
    .unwrap();
    let pwm = fake.attr_path("nct6798", "pwm3");
    fs::set_permissions(&pwm, fs::Permissions::from_mode(0o444)).unwrap();
    assert_eq!(run(&fake, &state).code(), Some(1));
    // The other Channels are still set.
    assert_eq!(
        fs::read_to_string(fake.attr_path("nct6798", "pwm4"))
            .unwrap()
            .trim(),
        "255"
    );
    fs::remove_dir_all(state).unwrap();
}
