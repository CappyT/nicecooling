//! The spacing of the Profile writes in the write worker, with the clock of Tokio paused
//! (task 7.5).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling_core::profile::Slot;
use nicecooling_protocol::device::{BulkTransport, DeviceError, HidTransport};
use nicecooling_protocol::report3::Report3;
use nicecooling_protocol::sim::{Request, SimDevice};
use nicecoolingd::leakshield::DeviceIo;
use nicecoolingd::leakshield::attached::Info;
use nicecoolingd::leakshield::save_settings::SAVE_SETTINGS_DELAY;
use nicecoolingd::leakshield::worker::{Job, ProfileRequest, WorkerParts, WriteWorker};
use nicecoolingd::profiles::activate::{Decision, Failure};
use nicecoolingd::state::slots::SlotMeta;
use nicecoolingd::state::{SlotStore, StateStore};
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep};

use crate::{LEAKSHIELD_ID, Rig, SERIAL};

const DISPLAY_PAGE_TIME: usize = 63;

fn start(rig: &Rig) -> WriteWorker {
    start_with(rig, rig.io.clone())
}

fn start_with(rig: &Rig, io: DeviceIo) -> WriteWorker {
    let (events, _rx) = mpsc::unbounded_channel();
    WriteWorker::spawn(WorkerParts {
        io,
        registry: Arc::clone(&rig.registry),
        backups: rig.backups(),
        last_known: Arc::clone(&rig.last_known),
        events,
        save_delay: SAVE_SETTINGS_DELAY,
        info: Arc::new(tokio::sync::watch::channel(Info::default()).0),
    })
}

/// The four slots of the device, each with its own number.
fn slots(rig: &Rig) -> SlotStore {
    let store = SlotStore::new(Arc::new(StateStore::new(&rig.dir)));
    let report1 = rig.io.read_report1().unwrap();
    let report3 = Report3::decode(&rig.sim.report3()).unwrap();
    let meta = SlotMeta::new(report1.firmware_version, report1.structure_number, SERIAL);
    store
        .init(&rig.registry, LEAKSHIELD_ID, &report3, &meta)
        .unwrap();
    store
}

fn slot_file(rig: &Rig, n: u32) -> std::path::PathBuf {
    rig.dir
        .join("leakshield")
        .join(LEAKSHIELD_ID)
        .join(format!("slot-{n}"))
        .join("report3.bin")
}

fn request(slots: &SlotStore, slot: u32) -> ProfileRequest {
    ProfileRequest {
        slot: Slot::new(slot).unwrap(),
        slots: slots.clone(),
        leakshield: LEAKSHIELD_ID.to_owned(),
        serial: SERIAL.to_owned(),
    }
}

/// The device reports the number of the slot that it holds, as the real device does.
fn device_holds(rig: &Rig, slot: u8) {
    rig.sim.edit_report1(|r| r.active_profile = slot);
}

fn writes(rig: &Rig) -> usize {
    rig.sim
        .requests()
        .iter()
        .filter(|r| matches!(r, Request::SetFeature { .. }))
        .count()
}

/// Asks for `slot` and waits for the end. Returns the time of the end.
async fn submit(
    rig: &Rig,
    worker: &WriteWorker,
    slots: &SlotStore,
    slot: u32,
) -> Result<Instant, Failure> {
    let ticket = worker.request_profile(request(slots, slot));
    let done = ticket.await.unwrap();
    // The simulated device keeps its report 1: it follows the write here.
    if let Ok(done) = &done
        && matches!(done.decision, Decision::Write(_))
    {
        device_holds(rig, slot as u8);
    }
    done.map(|_| Instant::now())
}

#[tokio::test(start_paused = true)]
async fn write_at_once_when_idle() {
    let rig = Rig::new("spacing-idle");
    let (worker, slots) = (start(&rig), slots(&rig));
    let start = Instant::now();

    let done = submit(&rig, &worker, &slots, 2).await.unwrap();

    assert_eq!(done - start, Duration::ZERO);
    assert_eq!(writes(&rig), 1);
    assert_eq!(rig.sim.report3()[747], 2);
    // The last profile write is older than 2 s: the next one goes out at once as well.
    sleep(Duration::from_millis(2001)).await;
    let before = Instant::now();
    let done = submit(&rig, &worker, &slots, 3).await.unwrap();
    assert_eq!(done - before, Duration::ZERO);
    assert_eq!(writes(&rig), 2);
}

#[tokio::test(start_paused = true)]
async fn request_within_2s_waits_until_1500ms_after_last_write() {
    let rig = Rig::new("spacing-wait");
    let (worker, slots) = (start(&rig), slots(&rig));
    let start = Instant::now();
    submit(&rig, &worker, &slots, 2).await.unwrap();

    sleep(Duration::from_millis(500)).await;
    let second = tokio::spawn({
        let (rig_sim, ticket) = (rig.sim.clone(), worker.request_profile(request(&slots, 3)));
        async move {
            let result = ticket.await.unwrap();
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

fn report1_reads(rig: &Rig) -> usize {
    rig.sim
        .requests()
        .iter()
        .filter(|r| matches!(r, Request::ReadInput))
        .count()
}

#[tokio::test(start_paused = true)]
async fn requests_inside_the_window_read_no_report1() {
    let rig = Rig::new("spacing-no-read");
    let (worker, slots) = (start(&rig), slots(&rig));
    submit(&rig, &worker, &slots, 2).await.unwrap();
    let before = report1_reads(&rig);

    // A write that waits, a request that replaces it, and a request that needs no write.
    let first = worker.request_profile(request(&slots, 3));
    let second = worker.request_profile(request(&slots, 4));
    assert!(matches!(first.await.unwrap(), Err(Failure::Superseded)));
    second.await.unwrap().unwrap();
    let kept = worker.request_profile(request(&slots, 4)).await.unwrap();

    assert!(matches!(kept.unwrap().decision, Decision::Keep));
    assert_eq!(rig.sim.report3()[747], 4);
    assert_eq!(report1_reads(&rig), before);
}

#[tokio::test(start_paused = true)]
async fn only_newest_waiting_request_is_written() {
    let rig = Rig::new("spacing-newest");
    let (worker, slots) = (start(&rig), slots(&rig));
    let start = Instant::now();
    submit(&rig, &worker, &slots, 2).await.unwrap();

    let mut waiting = Vec::new();
    for slot in [3, 1, 4] {
        sleep(Duration::from_millis(100)).await;
        let ticket = worker.request_profile(request(&slots, slot));
        waiting.push(tokio::spawn(async move {
            ticket.await.unwrap().map(|_| Instant::now())
        }));
    }
    let results = futures_util::future::join_all(waiting).await;

    // The two older requests are dropped. The newest goes out 1.5 s after the first write.
    assert!(matches!(results[0], Ok(Err(Failure::Superseded))));
    assert!(matches!(results[1], Ok(Err(Failure::Superseded))));
    let done = *results[2].as_ref().unwrap().as_ref().unwrap();
    assert_eq!(done - start, Duration::from_millis(1500));
    assert_eq!(writes(&rig), 2);
    assert_eq!(rig.sim.report3()[747], 4);
}

#[tokio::test(start_paused = true)]
async fn a_request_that_needs_no_write_drops_the_waiting_request() {
    let rig = Rig::new("spacing-keep");
    let (worker, slots) = (start(&rig), slots(&rig));
    submit(&rig, &worker, &slots, 2).await.unwrap();
    let waiting = worker.request_profile(request(&slots, 3));
    sleep(Duration::from_millis(200)).await;

    // The device holds slot 2 already: this request keeps it.
    let kept = worker.request_profile(request(&slots, 2)).await.unwrap();

    assert!(matches!(waiting.await.unwrap(), Err(Failure::Superseded)));
    assert!(matches!(kept.unwrap().decision, Decision::Keep));
    sleep(Duration::from_secs(10)).await;
    assert_eq!(writes(&rig), 1);
}

#[tokio::test(start_paused = true)]
async fn a_request_that_fails_drops_the_waiting_request() {
    let rig = Rig::new("spacing-fail");
    let (worker, slots) = (start(&rig), slots(&rig));
    submit(&rig, &worker, &slots, 2).await.unwrap();
    std::fs::remove_file(slot_file(&rig, 4)).unwrap();
    let waiting = worker.request_profile(request(&slots, 3));
    sleep(Duration::from_millis(200)).await;

    let failed = worker.request_profile(request(&slots, 4)).await.unwrap();

    assert!(matches!(failed, Err(Failure::NoSlot)));
    assert!(matches!(waiting.await.unwrap(), Err(Failure::Superseded)));
    sleep(Duration::from_secs(10)).await;
    assert_eq!(writes(&rig), 1);
    assert_eq!(rig.sim.report3()[747], 2);
}

#[tokio::test(start_paused = true)]
async fn a_cancel_drops_the_waiting_request() {
    let rig = Rig::new("spacing-cancel");
    let (worker, slots) = (start(&rig), slots(&rig));
    submit(&rig, &worker, &slots, 2).await.unwrap();
    let waiting = worker.request_profile(request(&slots, 3));
    sleep(Duration::from_millis(200)).await;

    // A newer activation that has no slot sends no request.
    worker.cancel_profile();

    assert!(matches!(waiting.await.unwrap(), Err(Failure::Superseded)));
    sleep(Duration::from_secs(10)).await;
    assert_eq!(writes(&rig), 1);
    assert_eq!(rig.sim.report3()[747], 2);
}

#[tokio::test(start_paused = true)]
async fn a_waiting_request_writes_the_slot_as_it_is_when_the_write_goes_out() {
    let rig = Rig::new("spacing-fresh-slot");
    let (worker, slots) = (start(&rig), slots(&rig));
    submit(&rig, &worker, &slots, 2).await.unwrap();
    let waiting = worker.request_profile(request(&slots, 3));
    sleep(Duration::from_millis(200)).await;

    // The slot changes while the request waits.
    let mut report = Report3::decode(&std::fs::read(slot_file(&rig, 3)).unwrap()).unwrap();
    report.set_bytes(DISPLAY_PAGE_TIME, &[0x01, 0x2C]);
    std::fs::write(slot_file(&rig, 3), report.encode()).unwrap();
    waiting.await.unwrap().unwrap();

    assert_eq!(&rig.sim.report3()[DISPLAY_PAGE_TIME..][..2], [0x01, 0x2C]);
    assert_eq!(rig.sim.report3()[747], 3);
}

#[tokio::test(start_paused = true)]
async fn a_factory_reset_runs_after_the_waiting_profile_write() {
    let rig = Rig::new("spacing-reset");
    let (worker, slots) = (start(&rig), slots(&rig));
    let start = Instant::now();
    submit(&rig, &worker, &slots, 2).await.unwrap();
    let waiting = worker.request_profile(request(&slots, 3));
    sleep(Duration::from_millis(200)).await;

    let report1 = rig.io.read_report1().unwrap();
    worker.submit(Job::FactoryReset { report1 }).await.unwrap();
    let done = Instant::now();

    // The Profile write keeps its time, 1.5 s after the first write. The reset comes after it.
    assert!(waiting.await.unwrap().is_ok());
    assert_eq!(done - start, Duration::from_millis(1500));
    let order: Vec<&str> = rig
        .sim
        .requests()
        .iter()
        .filter_map(|r| match r {
            Request::SetFeature { .. } => Some("write"),
            Request::WriteOutput(_) => Some("command"),
            _ => None,
        })
        .collect();
    assert_eq!(order, ["write", "write", "command"]);
}

#[tokio::test(start_paused = true)]
async fn save_settings_4s_after_last_profile_write() {
    let rig = Rig::new("spacing-save");
    let (worker, slots) = (start(&rig), slots(&rig));
    submit(&rig, &worker, &slots, 2).await.unwrap();
    // The second write comes after the gap of 2 s, 2.5 s after the first.
    sleep(Duration::from_millis(2500)).await;
    submit(&rig, &worker, &slots, 3).await.unwrap();

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

/// The HID side of the simulated device with a slow read of report 1, as on the real device: it
/// sends report 1 every 500 ms, and a read that finds the queue empty waits for the next one.
/// It records the time of each read of report 1 and each report 3 write.
struct SlowReport1 {
    sim: SimDevice,
    delay: Duration,
    reads: Mutex<Vec<std::time::Instant>>,
    writes: Mutex<Vec<std::time::Instant>>,
}

impl HidTransport for SlowReport1 {
    fn read_input(&self, timeout: Duration) -> Result<Vec<u8>, DeviceError> {
        self.reads.lock().unwrap().push(std::time::Instant::now());
        std::thread::sleep(self.delay);
        self.sim.read_input(timeout)
    }
    fn get_feature(&self, id: u8, len: usize) -> Result<Vec<u8>, DeviceError> {
        self.sim.get_feature(id, len)
    }
    fn set_feature(&self, report: &[u8]) -> Result<(), DeviceError> {
        self.writes.lock().unwrap().push(std::time::Instant::now());
        self.sim.set_feature(report)
    }
    fn write_output(&self, report: &[u8]) -> Result<(), DeviceError> {
        self.sim.write_output(report)
    }
}

/// Real time: a blocking read does not move the paused clock of Tokio.
#[tokio::test]
async fn a_slow_report1_read_does_not_delay_the_waiting_write() {
    let rig = Rig::new("spacing-slow-read");
    let hid = Arc::new(SlowReport1 {
        sim: rig.sim.clone(),
        delay: Duration::from_millis(400),
        reads: Mutex::new(Vec::new()),
        writes: Mutex::new(Vec::new()),
    });
    let bulk: Arc<dyn BulkTransport> = Arc::new(rig.sim.clone());
    let worker = start_with(&rig, DeviceIo::new(hid.clone(), bulk));
    let slots = slots(&rig);

    // Three activations in one burst, as three calls of the D-Bus method do.
    let tickets: Vec<_> = [2, 3, 4]
        .map(|slot| worker.request_profile(request(&slots, slot)))
        .into();
    let mut results = Vec::new();
    for ticket in tickets {
        results.push(ticket.await.unwrap());
    }

    assert!(results[0].is_ok());
    assert!(matches!(results[1], Err(Failure::Superseded)));
    assert!(results[2].is_ok());
    assert_eq!(rig.sim.report3()[747], 4);
    let writes = hid.writes.lock().unwrap();
    assert_eq!(writes.len(), 2);
    // No read of report 1 sits between the two writes. A time bound would fail on a loaded test
    // host: the backup of each write goes to disk.
    let reads = hid.reads.lock().unwrap();
    assert!(
        !reads
            .iter()
            .any(|read| *read > writes[0] && *read < writes[1]),
        "a read of report 1 came between the two writes"
    );
    // The spacing counts from the end of the first write, so the device never gets the second
    // one sooner than 1.5 s after the first, whatever the backup costs.
    let gap = writes[1] - writes[0];
    assert!(
        gap >= Duration::from_millis(1500),
        "the second write came {gap:?} after the first"
    );
}
