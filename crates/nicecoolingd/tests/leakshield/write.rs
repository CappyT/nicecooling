use std::sync::Arc;
use std::time::Duration;

use nicecooling_protocol::access::{ReadOnlyReason, WriteRefusal};
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report3::Report3;
use nicecooling_protocol::sim::Request;
use nicecoolingd::leakshield::DeviceEvent;
use nicecoolingd::leakshield::attached::Info;
use nicecoolingd::leakshield::fields::FieldValue;
use nicecoolingd::leakshield::save_settings::SAVE_SETTINGS_DELAY;
use nicecoolingd::leakshield::worker::{Job, WorkerParts, WriteWorker};
use nicecoolingd::leakshield::write::{FieldEdit, WriteError};
use tokio::sync::mpsc;

use crate::Rig;

/// A low-risk field at `plausible-read`: offset 63, scale 0.1 s.
const WRITABLE: &str = "display_page_time";
/// A Safety field at `code-defined`.
const NOT_VERIFIED: &str = "shield_pressure_low";

fn start(rig: &Rig) -> (WriteWorker, mpsc::UnboundedReceiver<DeviceEvent>) {
    let (events, rx) = mpsc::unbounded_channel();
    let worker = WriteWorker::spawn(WorkerParts {
        io: rig.io.clone(),
        registry: Arc::clone(&rig.registry),
        backups: rig.backups(),
        last_known: Arc::clone(&rig.last_known),
        events,
        save_delay: SAVE_SETTINGS_DELAY,
        info: Arc::new(tokio::sync::watch::channel(Info::default()).0),
    });
    (worker, rx)
}

fn write_job(rig: &Rig, raw: i64) -> Job {
    Job::Write {
        report1: rig.io.read_report1().unwrap(),
        edits: vec![FieldEdit::scalar(WRITABLE, raw)],
    }
}

/// Reads report 3 as the daemon does at attach, so the last known value exists.
fn seed_last_known(rig: &Rig) {
    let report = rig.io.read_report3().unwrap();
    rig.last_known.lock().set(report).unwrap();
}

#[tokio::test(start_paused = true)]
async fn write_success() {
    let rig = Rig::new("write-success");
    seed_last_known(&rig);
    let original = rig.sim.report3();
    let (worker, _events) = start(&rig);

    let written = worker.submit(write_job(&rig, 300)).await.unwrap();

    // Only the field and the CRC changed.
    let stored = rig.sim.report3();
    let changed: Vec<usize> = (0..750).filter(|&i| stored[i] != original[i]).collect();
    assert_eq!(changed, [63, 64, 748, 749]);
    assert_eq!(&stored[63..65], &300u16.to_be_bytes());
    assert_eq!(written.report3, Report3::decode(&stored).unwrap());
    // The backup holds the state from before.
    let backups = rig.backups().list().unwrap();
    assert_eq!(backups.len(), 1);
    assert_eq!(backups[0].id, written.backup);
    assert_eq!(
        rig.backups().get(&written.backup).unwrap().report3,
        original
    );
    // The last known value follows the write.
    assert_eq!(rig.last_known.lock().get(), Some(&written.report3));
    // The write sent one feature write and no output report yet.
    let sets = rig
        .sim
        .requests()
        .into_iter()
        .filter(|r| matches!(r, Request::SetFeature { .. }))
        .count();
    assert_eq!(sets, 1);
    assert!(rig.sim.output_reports().is_empty());
}

#[tokio::test(start_paused = true)]
async fn changed_between_read_and_write_returns_device_changed() {
    let rig = Rig::new("device-changed");
    seed_last_known(&rig);
    let original = rig.sim.report3();
    let (worker, _events) = start(&rig);

    // Another tool changes a byte after the first read.
    rig.sim.change_report3(300, &[0x55]);
    rig.sim.clear_requests();
    let error = worker.submit(write_job(&rig, 150)).await.unwrap_err();

    assert!(matches!(error, WriteError::DeviceChanged), "{error:?}");
    assert!(
        !rig.sim
            .requests()
            .iter()
            .any(|r| matches!(r, Request::SetFeature { .. }))
    );
    assert_eq!(rig.sim.report3()[300], 0x55);
    assert_eq!(&rig.sim.report3()[63..65], &original[63..65]);
}

#[tokio::test(start_paused = true)]
async fn volatile_byte_change_is_ok() {
    let mut registry = Registry::leakshield();
    registry.report3.volatile_mask = vec![200];
    let rig = Rig::with_registry("volatile", registry);
    // The device changes this byte on every read.
    rig.sim.make_volatile(200);
    seed_last_known(&rig);
    let (worker, _events) = start(&rig);

    worker.submit(write_job(&rig, 150)).await.unwrap();

    assert_eq!(&rig.sim.report3()[63..65], &150u16.to_be_bytes());
}

#[tokio::test(start_paused = true)]
async fn unexpected_byte_change_returns_verify_failed_with_backup_id() {
    let rig = Rig::new("verify-failed");
    seed_last_known(&rig);
    let (worker, _events) = start(&rig);

    // The device stores a different byte at offset 300 than the written one.
    rig.sim.tamper_next_set_feature(300, 0x55);
    let error = worker.submit(write_job(&rig, 150)).await.unwrap_err();

    let WriteError::VerifyFailed { backup, offsets } = error else {
        panic!("{error:?}");
    };
    assert_eq!(offsets, [300]);
    assert!(rig.backups().list().unwrap().iter().any(|b| b.id == backup));
    // The failed write does not become the last known value.
    assert_ne!(
        rig.last_known.lock().get().unwrap().encode(),
        rig.sim.report3()
    );
}

#[tokio::test(start_paused = true)]
async fn non_verified_field_refused() {
    let rig = Rig::new("not-verified");
    seed_last_known(&rig);
    let (worker, _events) = start(&rig);

    let job = Job::Write {
        report1: rig.io.read_report1().unwrap(),
        edits: vec![FieldEdit::scalar(NOT_VERIFIED, 1000)],
    };
    rig.sim.clear_requests();
    let error = worker.submit(job).await.unwrap_err();

    assert!(
        matches!(&error, WriteError::Refused(WriteRefusal::NotVerified(id)) if id == NOT_VERIFIED),
        "{error:?}"
    );
    // Nothing reached the device, not even a backup read.
    assert!(rig.sim.requests().is_empty());
    assert!(rig.backups().list().unwrap().is_empty());
}

#[tokio::test(start_paused = true)]
async fn second_queued_write_returns_busy() {
    let rig = Rig::new("busy");
    seed_last_known(&rig);
    let (worker, _events) = start(&rig);

    // The first future queues its write and waits. The second finds the queue full.
    let (first, second) = tokio::join!(
        worker.submit(write_job(&rig, 150)),
        worker.submit(write_job(&rig, 160)),
    );

    assert!(first.is_ok());
    assert!(matches!(second, Err(WriteError::Busy)));
    assert_eq!(&rig.sim.report3()[63..65], &150u16.to_be_bytes());
    // The queue is free again.
    assert!(worker.submit(write_job(&rig, 160)).await.is_ok());
}

#[tokio::test(start_paused = true)]
async fn three_writes_in_4s_send_one_save_settings() {
    let rig = Rig::new("three-writes");
    seed_last_known(&rig);
    let (worker, _events) = start(&rig);

    for raw in [150, 160, 170] {
        worker.submit(write_job(&rig, raw)).await.unwrap();
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
    // 1 s have passed since the last write, 3 s are left.
    tokio::time::sleep(Duration::from_millis(2900)).await;
    assert!(rig.sim.commands().is_empty());
    tokio::time::sleep(Duration::from_millis(200)).await;
    let commands = rig.sim.commands();
    assert_eq!(commands.len(), 1);
    assert_eq!(commands[0].command, 2);
    // No second command follows.
    tokio::time::sleep(Duration::from_secs(20)).await;
    assert_eq!(rig.sim.commands().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn save_settings_failure_emits_write_failed() {
    let rig = Rig::new("save-failed");
    seed_last_known(&rig);
    let (worker, mut events) = start(&rig);
    rig.sim.fail_output(true);

    let written = worker.submit(write_job(&rig, 150)).await.unwrap();
    tokio::time::sleep(Duration::from_millis(4100)).await;

    let event = events.try_recv().unwrap();
    let DeviceEvent::WriteFailed { error, backup } = event else {
        panic!("{event:?}");
    };
    assert!(!error.is_empty());
    assert_eq!(backup, written.backup);
    assert!(events.try_recv().is_err());
}

#[tokio::test(start_paused = true)]
async fn disconnect_during_write_offers_restore() {
    let rig = Rig::new("disconnect");
    seed_last_known(&rig);
    let original = rig.sim.report3();
    let (worker, _events) = start(&rig);

    // The device takes the write and vanishes before the read-back.
    rig.sim.disconnect_after_set_feature();
    let error = worker.submit(write_job(&rig, 150)).await.unwrap_err();
    let WriteError::Disconnected {
        backup: Some(backup),
    } = error
    else {
        panic!("{error:?}");
    };
    assert_ne!(rig.sim.report3(), original);

    // After the replug the backup brings report 3 back.
    rig.sim.reconnect();
    let restore = Job::Restore {
        report1: rig.io.read_report1().unwrap(),
        backup,
    };
    worker.submit(restore).await.unwrap();
    assert_eq!(rig.sim.report3(), original);
}

#[tokio::test(start_paused = true)]
async fn save_settings_waits_4s_after_a_failed_write() {
    let rig = Rig::new("save-after-failed");
    seed_last_known(&rig);
    let (worker, _events) = start(&rig);

    worker.submit(write_job(&rig, 150)).await.unwrap();
    tokio::time::sleep(Duration::from_secs(3)).await;
    // The device stores another byte at 65: the read-back fails, and report 3 on the device changed.
    rig.sim.tamper_next_set_feature(65, 3);
    let error = worker.submit(write_job(&rig, 160)).await.unwrap_err();
    assert!(
        matches!(error, WriteError::VerifyFailed { .. }),
        "{error:?}"
    );

    // 1.5 s after the last report 3 write: the command of the first write is not due.
    tokio::time::sleep(Duration::from_millis(1500)).await;
    assert!(rig.sim.commands().is_empty(), "{:?}", rig.sim.commands());
    tokio::time::sleep(Duration::from_millis(2600)).await;
    assert_eq!(rig.sim.commands().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn pending_save_settings_is_sent_when_the_last_handle_drops() {
    let rig = Rig::new("save-on-drop");
    seed_last_known(&rig);
    let (worker, _events) = start(&rig);

    worker.submit(write_job(&rig, 150)).await.unwrap();
    drop(worker);
    // Not before its time: the device needs 4 s after the last write.
    tokio::time::sleep(Duration::from_secs(3)).await;
    assert!(rig.sim.commands().is_empty());
    tokio::time::sleep(Duration::from_secs(2)).await;

    assert_eq!(rig.sim.commands().len(), 1);
}

#[tokio::test(start_paused = true)]
async fn write_refuses_report3_of_another_structure() {
    let rig = Rig::new("structure");
    rig.sim.change_report3(1, &101u16.to_be_bytes());
    seed_last_known(&rig);
    let (worker, _events) = start(&rig);
    let original = rig.sim.report3();

    let error = worker.submit(write_job(&rig, 150)).await.unwrap_err();

    assert!(
        matches!(
            error,
            WriteError::Refused(WriteRefusal::ReadOnly(ReadOnlyReason::UnknownStructure))
        ),
        "{error:?}"
    );
    assert_eq!(rig.sim.report3(), original);
}

#[tokio::test(start_paused = true)]
async fn set_fields_refuses_sensor_unit_and_device_profile() {
    let rig = Rig::new("safety-units");
    seed_last_known(&rig);
    let (worker, _events) = start(&rig);
    let original = rig.sim.report3();

    for edit in [
        FieldEdit {
            field: "sensor_unit".into(),
            raw: vec![3; 20],
        },
        FieldEdit::scalar("device_profile", 4),
    ] {
        let id = edit.field.clone();
        let job = Job::Write {
            report1: rig.io.read_report1().unwrap(),
            edits: vec![edit],
        };
        let error = worker.submit(job).await.unwrap_err();
        assert!(
            matches!(&error, WriteError::Refused(WriteRefusal::NotVerified(f)) if *f == id),
            "{id}: {error:?}"
        );
    }
    assert_eq!(rig.sim.report3(), original);
}

#[tokio::test]
async fn set_fields_allowed_while_action_runs() {
    let rig = Rig::new("action-write");
    // The fixture was dumped during a deaeration.
    let device = rig.attached();
    assert_eq!(device.info().running_action, "deaerate");
    assert_eq!(device.info().read_only_reason, "");

    device
        .set_fields(vec![(WRITABLE.into(), FieldValue::Number(30.0))])
        .await
        .unwrap();

    assert_eq!(&rig.sim.report3()[63..65], &300u16.to_be_bytes());
}
