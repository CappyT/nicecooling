use std::fs;

use nicecooling_protocol::ReportError;
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report3::Report3;
use nicecooling_protocol::sim::Request;
use nicecoolingd::leakshield::backup::{BackupSet, ImportError, Reason, Saved};
use nicecoolingd::leakshield::write::{WriteEnv, restore, take_backup};
use sha2::{Digest, Sha256};

use crate::{LEAKSHIELD_ID, Rig, SERIAL};

const NOV_14_2023_US: u64 = 1_700_000_000_000_000;

/// The reports of the simulated device, valid for `import`.
fn device_set(rig: &Rig) -> BackupSet {
    BackupSet {
        report3: rig.io.read_report3().unwrap().encode(),
        report5: rig.io.read_report5().unwrap(),
        report8: rig.io.read_report8().unwrap(),
    }
}

fn sha256_hex(set: &BackupSet) -> String {
    let mut hash = Sha256::new();
    hash.update(&set.report3);
    hash.update(&set.report5);
    hash.update(&set.report8);
    hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn retention_after_25_distinct_backups_keeps_first_and_last_20() {
    let rig = Rig::new("retention");
    let backups = rig.backups();
    let mut set = device_set(&rig);
    let mut ids = Vec::new();
    for n in 0..25u64 {
        // A distinct content for each backup. `save` checks sizes only.
        set.report5[10] = n as u8;
        let saved = backups
            .save(
                &set,
                1022,
                Reason::BeforeWrite,
                NOV_14_2023_US + n * 1_000_000,
            )
            .unwrap();
        assert!(saved.written);
        ids.push(saved.id);
    }
    let kept: Vec<String> = backups.list().unwrap().into_iter().map(|b| b.id).collect();
    assert_eq!(kept.len(), 21);
    assert_eq!(kept[0], ids[0]);
    assert_eq!(kept[1..], ids[5..]);
}

#[test]
fn duplicate_hash_not_written() {
    let rig = Rig::new("duplicate");
    let backups = rig.backups();
    let set = device_set(&rig);
    let first = backups
        .save(&set, 1022, Reason::BeforeWrite, NOV_14_2023_US)
        .unwrap();
    let second = backups
        .save(&set, 1022, Reason::Manual, NOV_14_2023_US + 5_000_000)
        .unwrap();
    assert_eq!(
        second,
        Saved {
            id: first.id,
            written: false
        }
    );
    assert_eq!(backups.list().unwrap().len(), 1);
}

#[test]
fn restore_writes_report3_only() {
    let rig = Rig::new("restore");
    let backups = rig.backups();
    let env = WriteEnv {
        io: &rig.io,
        registry: &rig.registry,
        backups: &backups,
    };
    let report1 = rig.io.read_report1().unwrap();
    let original = rig.sim.report3();
    let id = take_backup(&env, &report1, Reason::Manual, NOV_14_2023_US).unwrap();
    // The device changes, then the restore brings the old report 3 back.
    rig.sim.change_report3(63, &[0x01, 0x2C]);
    assert_ne!(rig.sim.report3(), original);
    rig.sim.clear_requests();
    restore(&env, &report1, &id, NOV_14_2023_US + 1_000_000).unwrap();
    assert_eq!(rig.sim.report3(), original);
    let writes: Vec<Request> = rig
        .sim
        .requests()
        .into_iter()
        .filter(|r| !matches!(r, Request::GetFeature { .. }))
        .collect();
    assert_eq!(writes, [Request::SetFeature { id: 3, len: 750 }]);
    assert!(rig.sim.output_reports().is_empty());
}

#[test]
fn backup_directory_name_format() {
    let rig = Rig::new("dirname");
    let set = device_set(&rig);
    let saved = rig
        .backups()
        .save(&set, 1022, Reason::BeforeWrite, NOV_14_2023_US)
        .unwrap();
    let expected = format!("20231114T221320Z-{}", &sha256_hex(&set)[..12]);
    assert_eq!(saved.id, expected);
    let dir = rig.dir.join("backups").join(LEAKSHIELD_ID).join(&saved.id);
    for file in ["report3.bin", "report5.bin", "report8.bin", "meta.json"] {
        assert!(dir.join(file).is_file(), "{file}");
    }
}

#[test]
fn backup_meta_json_fields() {
    let rig = Rig::new("meta");
    let set = device_set(&rig);
    let saved = rig
        .backups()
        .save(&set, 1022, Reason::BeforeWrite, NOV_14_2023_US)
        .unwrap();
    let path = rig
        .dir
        .join("backups")
        .join(LEAKSHIELD_ID)
        .join(&saved.id)
        .join("meta.json");
    let meta: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let mut sorted: Vec<&str> = meta
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    sorted.sort_unstable();
    assert_eq!(
        sorted,
        [
            "created_us",
            "firmware",
            "leakshield",
            "reason",
            "schema_version",
            "serial",
            "sha256"
        ]
    );
    assert_eq!(meta["schema_version"], 1);
    assert_eq!(meta["created_us"], NOV_14_2023_US);
    assert_eq!(meta["leakshield"], LEAKSHIELD_ID);
    assert_eq!(meta["serial"], SERIAL);
    assert_eq!(meta["firmware"], 1022);
    // The first backup of a serial has the reason `first`, whatever the caller asked for.
    assert_eq!(meta["reason"], "first");
    assert_eq!(meta["sha256"], sha256_hex(&set));
}

#[test]
fn import_rejects_bad_crc() {
    let rig = Rig::new("import-crc");
    let backups = rig.backups();
    let mut set = device_set(&rig);
    set.report3[100] ^= 0xFF;
    let error = backups.import(&set, 100, 1022, NOV_14_2023_US).unwrap_err();
    assert!(matches!(
        error,
        ImportError::Invalid {
            report: 3,
            error: ReportError::BadCrc { .. }
        }
    ));
    let mut short = device_set(&rig);
    short.report8.pop();
    assert!(matches!(
        backups.import(&short, 100, 1022, NOV_14_2023_US),
        Err(ImportError::Invalid {
            report: 8,
            error: ReportError::WrongSize { .. }
        })
    ));
    assert!(backups.list().unwrap().is_empty());
}

#[test]
fn import_rejects_other_structure() {
    let rig = Rig::new("import-structure");
    let backups = rig.backups();
    let mut set = device_set(&rig);
    let mut report3 = Report3::decode(&set.report3).unwrap();
    report3.set_bytes(1, &101u16.to_be_bytes());
    set.report3 = report3.encode();
    let error = backups.import(&set, 100, 1022, NOV_14_2023_US).unwrap_err();
    assert!(matches!(
        error,
        ImportError::OtherStructure {
            backup: 101,
            device: 100
        }
    ));
    assert!(backups.list().unwrap().is_empty());

    let own = device_set(&rig);
    let saved = backups.import(&own, 100, 1022, NOV_14_2023_US).unwrap();
    assert!(saved.written);
}

#[test]
fn get_backup_returns_three_reports() {
    let rig = Rig::new("get");
    let backups = rig.backups();
    let set = device_set(&rig);
    let saved = backups
        .save(&set, 1022, Reason::Manual, NOV_14_2023_US)
        .unwrap();
    assert_eq!(backups.get(&saved.id).unwrap(), set);
    // An id that is not a backup name never reaches the file system.
    assert!(backups.get("../../etc").is_err());
}

#[test]
fn retention_keeps_the_first_saved_backup_when_the_clock_goes_back() {
    let rig = Rig::new("retention-clock");
    let backups = rig.backups();
    let mut set = device_set(&rig);
    let mut ids = Vec::new();
    for n in 0..21u64 {
        set.report5[10] = n as u8;
        let saved = backups
            .save(
                &set,
                1022,
                Reason::BeforeWrite,
                NOV_14_2023_US + n * 1_000_000,
            )
            .unwrap();
        ids.push(saved.id);
    }
    // The PC clock is one day behind for the 22nd backup, so it looks older than all others.
    set.report5[10] = 99;
    let latest = backups
        .save(
            &set,
            1022,
            Reason::BeforeWrite,
            NOV_14_2023_US - 86_400_000_000,
        )
        .unwrap();

    let kept: Vec<String> = backups.list().unwrap().into_iter().map(|b| b.id).collect();
    assert!(kept.contains(&ids[0]), "the first backup is gone: {kept:?}");
    // The backup that a write result points at stays.
    assert!(
        kept.contains(&latest.id),
        "the new backup is gone: {kept:?}"
    );
}

#[test]
fn an_import_is_never_the_first_backup() {
    let rig = Rig::new("retention-import-first");
    let backups = rig.backups();
    let mut set = device_set(&rig);
    set.report5[10] = 1;
    let imported = backups.import(&set, 100, 1022, NOV_14_2023_US).unwrap();
    let mut device = device_set(&rig);
    device.report5[10] = 2;
    let first = backups
        .save(
            &device,
            1022,
            Reason::BeforeWrite,
            NOV_14_2023_US + 1_000_000,
        )
        .unwrap();
    for n in 0..21u64 {
        device.report5[10] = 10 + n as u8;
        backups
            .save(
                &device,
                1022,
                Reason::BeforeWrite,
                NOV_14_2023_US + (2 + n) * 1_000_000,
            )
            .unwrap();
    }

    let list = backups.list().unwrap();
    let reasons: Vec<(&str, &str)> = list
        .iter()
        .filter(|b| b.id == imported.id || b.id == first.id)
        .map(|b| (b.id.as_str(), b.reason.as_str()))
        .collect();
    assert_eq!(reasons, [(first.id.as_str(), "first")], "{list:?}");
}

/// Report 3 of the device with `shield_pressure_low` 0 mbar (range 30-430), `default_mode` 0
/// (codes 1-3) and `test_pressure` 0.
fn out_of_range_set(rig: &Rig) -> BackupSet {
    let registry = Registry::leakshield();
    let mut set = device_set(rig);
    let mut report3 = Report3::decode(&set.report3).unwrap();
    report3.set_bytes(731, &0i16.to_be_bytes());
    report3.set_bytes(724, &[0]);
    report3.set_bytes(725, &0i16.to_be_bytes());
    set.report3 = report3.encode();
    let out_of_range: Vec<&str> = registry
        .field
        .iter()
        .filter(|f| !f.in_range_in(&report3))
        .map(|f| f.id.as_str())
        .collect();
    assert_eq!(
        out_of_range,
        ["default_mode", "test_pressure", "shield_pressure_low"]
    );
    set
}

#[tokio::test]
async fn import_refuses_report3_with_a_field_out_of_its_range() {
    let rig = Rig::new("import-range");
    let device = rig.attached();

    let error = device.import_backup(&out_of_range_set(&rig)).unwrap_err();

    assert_eq!(
        error.dbus_name(),
        "io.github.cappyt.NiceCooling1.Error.InvalidArgument"
    );
    assert!(rig.backups().list().unwrap().is_empty());
}

#[test]
fn restore_refuses_report3_with_a_field_out_of_its_range() {
    let rig = Rig::new("restore-range");
    let backups = rig.backups();
    let env = WriteEnv {
        io: &rig.io,
        registry: &rig.registry,
        backups: &backups,
    };
    let report1 = rig.io.read_report1().unwrap();
    let original = rig.sim.report3();
    // A backup file that holds out-of-range values, however it got there.
    let saved = backups
        .save(
            &out_of_range_set(&rig),
            1022,
            Reason::Manual,
            NOV_14_2023_US,
        )
        .unwrap();

    let error = restore(&env, &report1, &saved.id, NOV_14_2023_US + 1).unwrap_err();

    assert!(
        matches!(
            error,
            nicecoolingd::leakshield::write::WriteError::BackupMismatch(_)
        ),
        "{error:?}"
    );
    assert_eq!(rig.sim.report3(), original);
}
