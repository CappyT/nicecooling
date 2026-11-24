//! The Leakshield part of a Profile activation on the simulated device (task 7.2).

use std::sync::Arc;
use std::time::Duration;

use nicecooling_core::profile::Slot;
use nicecooling_protocol::report3::Report3;
use nicecooling_protocol::sim::Request;
use nicecoolingd::leakshield::DeviceEvent;
use nicecoolingd::leakshield::write::WriteError;
use nicecoolingd::profiles::activate::{Decision, Failure, apply_slot};
use nicecoolingd::state::{SlotStore, StateStore};

use crate::{LEAKSHIELD_ID, Rig};

const DISPLAY_PAGE_TIME: usize = 63;

fn slots(rig: &Rig) -> SlotStore {
    SlotStore::new(Arc::new(StateStore::new(&rig.dir)))
}

fn slot(n: u8) -> Slot {
    Slot::new(u32::from(n)).unwrap()
}

fn slot_file(rig: &Rig, n: u8) -> std::path::PathBuf {
    rig.dir
        .join("leakshield")
        .join(LEAKSHIELD_ID)
        .join(format!("slot-{n}"))
        .join("report3.bin")
}

/// Changes the stored report 3 of a slot, so that a write is told apart from the device state.
fn change_slot(rig: &Rig, n: u8, offset: usize, value: &[u8]) -> Vec<u8> {
    let mut report = Report3::decode(&std::fs::read(slot_file(rig, n)).unwrap()).unwrap();
    report.set_bytes(offset, value);
    std::fs::write(slot_file(rig, n), report.encode()).unwrap();
    report.encode()
}

fn feature_writes(rig: &Rig) -> usize {
    rig.sim
        .requests()
        .iter()
        .filter(|r| matches!(r, Request::SetFeature { .. }))
        .count()
}

fn set_meta(rig: &Rig, n: u8, edit: impl FnOnce(&mut serde_json::Value)) {
    let file = slot_file(rig, n).with_file_name("meta.json");
    let mut meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    edit(&mut meta);
    std::fs::write(file, serde_json::to_vec(&meta).unwrap()).unwrap();
}

#[tokio::test(start_paused = true)]
async fn same_slot_number_writes_nothing() {
    let rig = Rig::new("slot-same");
    let device = rig.attached();
    rig.sim.edit_report1(|r| r.active_profile = 3);
    rig.sim.clear_requests();

    let decision = apply_slot(&device, &slots(&rig), slot(3)).await.unwrap();

    assert_eq!(decision, Decision::Keep);
    assert_eq!(feature_writes(&rig), 0);
    tokio::time::sleep(Duration::from_secs(10)).await;
    assert!(rig.sim.output_reports().is_empty());
}

#[tokio::test(start_paused = true)]
async fn different_slot_number_writes_slot_report3_as_stored() {
    let rig = Rig::new("slot-different");
    let device = rig.attached();
    let stored = change_slot(&rig, 2, DISPLAY_PAGE_TIME, &[0x01, 0x2C]);

    let decision = apply_slot(&device, &slots(&rig), slot(2)).await.unwrap();

    assert!(matches!(decision, Decision::Write(_)));
    // The device holds the stored bytes of slot 2: the number 2 and the changed field.
    assert_eq!(rig.sim.report3(), stored);
    assert_eq!(rig.sim.report3()[747], 2);
    assert_eq!(feature_writes(&rig), 1);
    // The save-settings command follows 4 s after the write.
    assert!(rig.sim.output_reports().is_empty());
    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(rig.sim.commands().len(), 1);
    // The backup of the state before the write exists.
    assert_eq!(rig.backups().list().unwrap().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn no_readback_after_profile_write() {
    let rig = Rig::new("slot-no-readback");
    let device = rig.attached();
    rig.sim.clear_requests();

    apply_slot(&device, &slots(&rig), slot(4)).await.unwrap();

    let requests = rig.sim.requests();
    let write = requests
        .iter()
        .position(|r| matches!(r, Request::SetFeature { id: 3, .. }))
        .expect("the report 3 write");
    // The backup reads come before the write. Nothing reads report 3 after it.
    assert!(
        requests[write + 1..]
            .iter()
            .all(|r| !matches!(r, Request::GetFeature { .. })),
        "{requests:?}"
    );
    assert!(
        requests[..write]
            .iter()
            .any(|r| matches!(r, Request::GetFeature { id: 3, .. }))
    );
}

#[tokio::test(start_paused = true)]
async fn profile_write_during_action_and_release() {
    let rig = Rig::new("slot-modes");
    // The fixture was dumped during a deaeration.
    let device = rig.attached();
    assert_eq!(device.info().running_action, "deaerate");

    apply_slot(&device, &slots(&rig), slot(2)).await.unwrap();
    assert_eq!(rig.sim.report3()[747], 2);

    // The Mode is RELEASE and no Action runs. The next request is older than the spacing window of
    // 2 s, so it reads report 1.
    tokio::time::sleep(Duration::from_millis(2001)).await;
    rig.sim.edit_report1(|r| r.mode = 3);
    rig.sim.edit_report1(|r| r.active_profile = 2);
    apply_slot(&device, &slots(&rig), slot(3)).await.unwrap();
    assert_eq!(rig.sim.report3()[747], 3);
    assert_eq!(device.info().mode, "release");
}

#[tokio::test(start_paused = true)]
async fn unknown_firmware_skips_leakshield_part() {
    let rig = Rig::new("slot-unknown-firmware");
    let device = rig.attached();
    rig.sim.edit_report1(|r| r.firmware_version = 1019);
    rig.sim.clear_requests();

    let result = apply_slot(&device, &slots(&rig), slot(2)).await;

    assert!(
        matches!(result, Err(Failure::UnknownFirmware)),
        "{result:?}"
    );
    assert_eq!(feature_writes(&rig), 0);
}

#[tokio::test(start_paused = true)]
async fn firmware_mismatch_skips_leakshield_part() {
    let rig = Rig::new("slot-firmware");
    let device = rig.attached();
    set_meta(&rig, 2, |meta| meta["firmware"] = 1019.into());
    rig.sim.clear_requests();

    let result = apply_slot(&device, &slots(&rig), slot(2)).await;

    assert!(
        matches!(result, Err(Failure::FirmwareMismatch)),
        "{result:?}"
    );
    assert_eq!(feature_writes(&rig), 0);
}

#[tokio::test(start_paused = true)]
async fn serial_mismatch_skips_leakshield_part() {
    let rig = Rig::new("slot-serial");
    let device = rig.attached();
    set_meta(&rig, 2, |meta| meta["serial"] = "11111-22222".into());
    rig.sim.clear_requests();

    let result = apply_slot(&device, &slots(&rig), slot(2)).await;

    assert!(matches!(result, Err(Failure::SerialMismatch)), "{result:?}");
    assert_eq!(feature_writes(&rig), 0);
}

#[tokio::test(start_paused = true)]
async fn write_error_is_reported_and_the_report_stays() {
    let rig = Rig::new("slot-write-error");
    let device = rig.attached();
    let before = rig.sim.report3();
    rig.sim.fail_feature_writes(true);

    let result = apply_slot(&device, &slots(&rig), slot(2)).await;

    assert!(
        matches!(result, Err(Failure::Write(WriteError::Device { .. }))),
        "{result:?}"
    );
    assert_eq!(rig.sim.report3(), before);
}

/// The device button changed a field. The number of the device differs from the slot, so the
/// whole stored report 3 goes over the change.
#[tokio::test(start_paused = true)]
async fn external_change_overwritten_when_numbers_differ() {
    let rig = Rig::new("slot-overwrite");
    let device = rig.attached();
    rig.sim.change_report3(DISPLAY_PAGE_TIME, &[0x02, 0x58]);

    apply_slot(&device, &slots(&rig), slot(2)).await.unwrap();

    // The slot holds the value of the first read: 0x0064.
    assert_eq!(&rig.sim.report3()[DISPLAY_PAGE_TIME..][..2], [0x00, 0x64]);
}

#[tokio::test(start_paused = true)]
async fn external_change_kept_when_numbers_equal() {
    let rig = Rig::new("slot-kept");
    let device = rig.attached();
    rig.sim.edit_report1(|r| r.active_profile = 3);
    rig.sim.change_report3(DISPLAY_PAGE_TIME, &[0x02, 0x58]);
    rig.sim.clear_requests();

    apply_slot(&device, &slots(&rig), slot(3)).await.unwrap();

    assert_eq!(&rig.sim.report3()[DISPLAY_PAGE_TIME..][..2], [0x02, 0x58]);
    assert_eq!(feature_writes(&rig), 0);
}

#[tokio::test(start_paused = true)]
async fn profile_write_emits_no_external_change() {
    let rig = Rig::new("slot-no-external");
    let device = rig.attached();
    let mut events = device.take_events().unwrap();
    let period = Duration::from_secs(10);
    device.spawn_external_detection(period);
    // The first read becomes the last known value.
    tokio::time::sleep(Duration::from_secs(1)).await;
    change_slot(&rig, 2, DISPLAY_PAGE_TIME, &[0x01, 0x2C]);

    apply_slot(&device, &slots(&rig), slot(2)).await.unwrap();
    tokio::time::sleep(period * 3).await;

    // The written report is the last known report 3: the poll finds no difference.
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(event, DeviceEvent::ExternalChange { .. }),
            "{event:?}"
        );
    }
}

#[tokio::test(start_paused = true)]
async fn failed_write_backup_per_leakshield() {
    let failing = Rig::new("failed-backup-a");
    let working = Rig::new("failed-backup-b");
    let (device_a, device_b) = (failing.attached(), working.attached());
    assert_eq!(device_a.info().failed_write_backup, "");
    failing.sim.fail_feature_writes(true);

    assert!(
        apply_slot(&device_a, &slots(&failing), slot(2))
            .await
            .is_err()
    );
    apply_slot(&device_b, &slots(&working), slot(2))
        .await
        .unwrap();

    let backups = failing.backups().list().unwrap();
    assert_eq!(backups.len(), 1);
    assert_eq!(device_a.info().failed_write_backup, backups[0].id);
    assert_eq!(device_b.info().failed_write_backup, "");
}

#[tokio::test(start_paused = true)]
async fn failed_write_backup_cleared_by_good_write() {
    let rig = Rig::new("failed-backup-good");
    let device = rig.attached();
    rig.sim.fail_feature_writes(true);
    assert!(apply_slot(&device, &slots(&rig), slot(2)).await.is_err());
    assert_ne!(device.info().failed_write_backup, "");

    rig.sim.fail_feature_writes(false);
    apply_slot(&device, &slots(&rig), slot(2)).await.unwrap();

    assert_eq!(device.info().failed_write_backup, "");
}

#[tokio::test(start_paused = true)]
async fn failed_write_backup_cleared_by_restore() {
    let rig = Rig::new("failed-backup-restore");
    let device = rig.attached();
    rig.sim.fail_feature_writes(true);
    assert!(apply_slot(&device, &slots(&rig), slot(2)).await.is_err());
    let backup = device.info().failed_write_backup;
    assert_ne!(backup, "");

    rig.sim.fail_feature_writes(false);
    device.restore_backup(&backup).await.unwrap();

    assert_eq!(device.info().failed_write_backup, "");
}
