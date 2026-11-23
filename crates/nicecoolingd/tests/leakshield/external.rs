use std::sync::Arc;
use std::time::Duration;

use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::sim::Request;
use nicecoolingd::leakshield::DeviceEvent;
use nicecoolingd::leakshield::external::{ExternalDetector, LastKnown, POLL_PERIOD};
use nicecoolingd::leakshield::save_settings::SAVE_SETTINGS_DELAY;
use nicecoolingd::leakshield::worker::{Job, WorkerParts, WriteWorker};
use nicecoolingd::leakshield::write::FieldEdit;
use tokio::sync::mpsc;

use crate::{LEAKSHIELD_ID, Rig};

fn detector(rig: &Rig) -> ExternalDetector {
    ExternalDetector::new(
        rig.io.clone(),
        Arc::clone(&rig.registry),
        Arc::clone(&rig.last_known),
    )
}

#[tokio::test(start_paused = true)]
async fn external_change_emits_signal_and_writes_nothing() {
    let rig = Rig::new("external");
    let (events, mut rx) = mpsc::unbounded_channel();
    tokio::spawn(detector(&rig).run(POLL_PERIOD, events));

    // The first read becomes the last known value.
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(rx.try_recv().is_err());

    // The user changes the page time with the device button.
    rig.sim.change_report3(63, &[0x02, 0x58]);
    rig.sim.clear_requests();
    tokio::time::sleep(POLL_PERIOD).await;

    assert_eq!(
        rx.try_recv().unwrap(),
        DeviceEvent::ExternalChange {
            fields: vec!["display_page_time".to_owned()]
        }
    );
    // The change is kept: the next poll finds nothing new.
    tokio::time::sleep(POLL_PERIOD).await;
    assert!(rx.try_recv().is_err());
    // Only reads reached the device.
    assert!(
        rig.sim
            .requests()
            .iter()
            .all(|r| matches!(r, Request::GetFeature { id: 3, .. }))
    );
    assert!(rig.sim.output_reports().is_empty());
}

#[tokio::test(start_paused = true)]
async fn own_write_emits_no_signal() {
    let rig = Rig::new("own-write");
    let detector = detector(&rig);
    assert_eq!(detector.poll().unwrap(), None);
    let (events, _rx) = mpsc::unbounded_channel();
    let worker = WriteWorker::spawn(WorkerParts {
        io: rig.io.clone(),
        registry: Arc::clone(&rig.registry),
        backups: rig.backups(),
        last_known: Arc::clone(&rig.last_known),
        events,
        save_delay: SAVE_SETTINGS_DELAY,
        failed_backup: Arc::new(|_| {}),
    });

    worker
        .submit(Job::Write {
            report1: rig.io.read_report1().unwrap(),
            edits: vec![FieldEdit::scalar("display_page_time", 150)],
        })
        .await
        .unwrap();

    assert_eq!(detector.poll().unwrap(), None);
}

#[test]
fn detects_change_across_daemon_restart() {
    let rig = Rig::new("restart");
    assert_eq!(detector(&rig).poll().unwrap(), None);
    // The daemon stops. Another tool changes report 3. The daemon starts again.
    rig.sim.change_report3(63, &[0x02, 0x58]);
    rig.sim.change_report3(535, &[0x00, 0x03]);

    let last_known = Arc::new(LastKnown::open(&rig.dir, LEAKSHIELD_ID));
    let restarted = ExternalDetector::new(rig.io.clone(), Arc::clone(&rig.registry), last_known);
    assert_eq!(
        restarted.poll().unwrap(),
        Some(vec![
            "display_page_time".to_owned(),
            "alarm_enable".to_owned()
        ])
    );
    assert_eq!(restarted.poll().unwrap(), None);
}

#[test]
fn volatile_bytes_ignored() {
    let mut registry = Registry::leakshield();
    registry.report3.volatile_mask = vec![200];
    let rig = Rig::with_registry("volatile-external", registry);
    rig.sim.make_volatile(200);
    let detector = detector(&rig);
    assert_eq!(detector.poll().unwrap(), None);
    assert_eq!(detector.poll().unwrap(), None);
    assert_eq!(detector.poll().unwrap(), None);
}
