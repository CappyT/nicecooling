//! The spacing of the Profile writes in the write worker, with the clock of Tokio paused
//! (task 7.5).

use std::sync::Arc;
use std::time::Duration;

use nicecooling_core::profile::Slot;
use nicecooling_protocol::report3::Report3;
use nicecooling_protocol::sim::Request;
use nicecoolingd::leakshield::save_settings::SAVE_SETTINGS_DELAY;
use nicecoolingd::leakshield::worker::{WorkerParts, WriteWorker};
use nicecoolingd::leakshield::write::WriteError;
use nicecoolingd::state::slots::with_slot_number;
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep};

use crate::Rig;

fn start(rig: &Rig) -> WriteWorker {
    let (events, _rx) = mpsc::unbounded_channel();
    WriteWorker::spawn(WorkerParts {
        io: rig.io.clone(),
        registry: Arc::clone(&rig.registry),
        backups: rig.backups(),
        last_known: Arc::clone(&rig.last_known),
        events,
        save_delay: SAVE_SETTINGS_DELAY,
    })
}

/// The report 3 of the device with the number of `slot`.
fn stored(rig: &Rig, slot: u32) -> Report3 {
    let device = Report3::decode(&rig.sim.report3()).unwrap();
    with_slot_number(&rig.registry, &device, Slot::new(slot).unwrap()).unwrap()
}

fn writes(rig: &Rig) -> usize {
    rig.sim
        .requests()
        .iter()
        .filter(|r| matches!(r, Request::SetFeature { .. }))
        .count()
}

async fn submit(rig: &Rig, worker: &WriteWorker, slot: u32) -> Result<Instant, WriteError> {
    let report1 = rig.io.read_report1().unwrap();
    worker
        .submit_profile(report1, stored(rig, slot))
        .await
        .map(|_| Instant::now())
}

#[tokio::test(start_paused = true)]
async fn write_at_once_when_idle() {
    let rig = Rig::new("spacing-idle");
    let worker = start(&rig);
    let start = Instant::now();

    let done = submit(&rig, &worker, 2).await.unwrap();

    assert_eq!(done - start, Duration::ZERO);
    assert_eq!(writes(&rig), 1);
    assert_eq!(rig.sim.report3()[747], 2);
    // The last profile write is older than 2 s: the next one goes out at once as well.
    sleep(Duration::from_millis(2001)).await;
    let before = Instant::now();
    let done = submit(&rig, &worker, 3).await.unwrap();
    assert_eq!(done - before, Duration::ZERO);
    assert_eq!(writes(&rig), 2);
}

#[tokio::test(start_paused = true)]
async fn request_within_2s_waits_until_1500ms_after_last_write() {
    let rig = Rig::new("spacing-wait");
    let worker = start(&rig);
    let start = Instant::now();
    submit(&rig, &worker, 2).await.unwrap();

    sleep(Duration::from_millis(500)).await;
    let second = tokio::spawn({
        let (rig_sim, worker) = (rig.sim.clone(), worker.clone());
        let report1 = rig.io.read_report1().unwrap();
        let report3 = stored(&rig, 3);
        async move {
            let result = worker.submit_profile(report1, report3).await;
            (result.is_ok(), Instant::now(), rig_sim.report3()[747])
        }
    });
    sleep(Duration::from_millis(900)).await;
    // 1.4 s after the first write: the request still waits.
    assert_eq!(writes(&rig), 1);
    assert_eq!(rig.sim.report3()[747], 2);

    let (ok, done, number) = second.await.unwrap();
    assert!(ok);
    // The write goes out 1.5 s after the last profile write.
    assert_eq!(done - start, Duration::from_millis(1500));
    assert_eq!((writes(&rig), number), (2, 3));
}

#[tokio::test(start_paused = true)]
async fn only_newest_waiting_request_is_written() {
    let rig = Rig::new("spacing-newest");
    let worker = start(&rig);
    let start = Instant::now();
    submit(&rig, &worker, 1).await.unwrap();

    let mut waiting = Vec::new();
    for slot in [2, 3, 4] {
        sleep(Duration::from_millis(100)).await;
        let report1 = rig.io.read_report1().unwrap();
        let report3 = stored(&rig, slot);
        let worker = worker.clone();
        waiting.push(tokio::spawn(async move {
            worker
                .submit_profile(report1, report3)
                .await
                .map(|_| Instant::now())
        }));
    }
    let results = futures_util::future::join_all(waiting).await;

    // The two older requests are dropped. The newest goes out 1.5 s after the first write.
    assert!(matches!(results[0], Ok(Err(WriteError::Superseded))));
    assert!(matches!(results[1], Ok(Err(WriteError::Superseded))));
    let done = *results[2].as_ref().unwrap().as_ref().unwrap();
    assert_eq!(done - start, Duration::from_millis(1500));
    assert_eq!(writes(&rig), 2);
    assert_eq!(rig.sim.report3()[747], 4);
}

#[tokio::test(start_paused = true)]
async fn a_cancel_drops_the_waiting_request() {
    let rig = Rig::new("spacing-cancel");
    let worker = start(&rig);
    submit(&rig, &worker, 1).await.unwrap();
    let waiting = tokio::spawn({
        let worker = worker.clone();
        let report1 = rig.io.read_report1().unwrap();
        let report3 = stored(&rig, 3);
        async move { worker.submit_profile(report1, report3).await }
    });
    sleep(Duration::from_millis(200)).await;

    worker.cancel_profile_write().await;

    assert!(matches!(
        waiting.await.unwrap(),
        Err(WriteError::Superseded)
    ));
    sleep(Duration::from_secs(10)).await;
    assert_eq!(writes(&rig), 1);
}

#[tokio::test(start_paused = true)]
async fn save_settings_4s_after_last_profile_write() {
    let rig = Rig::new("spacing-save");
    let worker = start(&rig);
    submit(&rig, &worker, 2).await.unwrap();
    // The second write comes after the gap of 2 s, 2.5 s after the first.
    sleep(Duration::from_millis(2500)).await;
    submit(&rig, &worker, 3).await.unwrap();

    // 4 s after the first write: no command, because the second write moved it back.
    sleep(Duration::from_millis(1600)).await;
    assert!(rig.sim.output_reports().is_empty());
    // 3.9 s after the second write.
    sleep(Duration::from_millis(2300)).await;
    assert!(rig.sim.output_reports().is_empty());
    // 4 s after the second write: one save-settings command.
    sleep(Duration::from_millis(200)).await;
    assert_eq!(rig.sim.commands().len(), 1);
    sleep(Duration::from_secs(20)).await;
    assert_eq!(rig.sim.commands().len(), 1);
}
