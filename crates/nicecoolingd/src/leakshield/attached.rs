//! One attached Leakshield: its device access, the state that report 1 gives and the operations
//! of the `Leakshield` interface. The D-Bus layer only authorizes and converts.

use std::fmt;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nicecooling_api::Error;
use nicecooling_api::errors::Result;
use nicecooling_api::ids;
use nicecooling_core::profile::Slot;
use nicecooling_protocol::access::{Access, WriteRefusal};
use nicecooling_protocol::device::DeviceError;
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report1::Report1;
use nicecooling_protocol::report2::Report2;
use nicecooling_protocol::report3::Report3;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use super::action::{self, Action};
use super::backup::{BackupEntry, BackupSet, Backups, ImportError, Reason};
use super::events::{Event, decode_events};
use super::external::{ExternalDetector, LastKnown};
use super::feed::FeedSent;
use super::fields::{self, EditError, FieldRow, FieldValue};
use super::mode::{self, COMMAND_DELAY, Mode};
use super::software_sensors::SoftwareSlots;
use super::worker::{
    Job, ProfileRequest, ProfileTicket, StoredReply, StoredRequest, WorkerParts, WriteWorker,
};
use super::write::{FieldEdit, WriteEnv, WriteError, out_of_range_field, take_backup};
use super::{DeviceEvent, DeviceIo, blocking, clock, counters, reset_alarm};
use crate::profiles::activate::{Decision, Failure};
use crate::state::slots::SlotMeta;
use crate::state::{SlotStore, StateStore};
use crate::value_table::ValueTable;

/// The automatic clock sync runs at most this often, so a device that ignores it is not flooded.
const AUTO_CLOCK_GAP: Duration = Duration::from_secs(60);

#[derive(Clone)]
pub struct AttachParams {
    /// `/var/lib/nicecooling`.
    pub state_dir: PathBuf,
    pub registry: Arc<Registry>,
    /// The time between the last report 3 write and the save-settings command.
    pub save_delay: Duration,
}

/// What the properties of the interface show. A change of it emits `PropertiesChanged`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    pub firmware: u16,
    pub structure: u16,
    pub connected: bool,
    /// Empty when the Leakshield is writable.
    pub read_only_reason: String,
    /// `shield`, `monitor`, `release`, or empty while unknown.
    pub mode: String,
    /// `test`, `measure-fill-level`, `deaerate`, `refill`, or empty.
    pub running_action: String,
    /// `off`, `waiting`, `ok` or `failing` (`docs/api.md`, Feed state).
    pub feed_state: &'static str,
    /// The time of the last good send in microseconds, the pump speed and the flow it carried.
    pub last_feed: (u64, f64, f64),
    /// `active_profile` of report 1.
    pub active_slot: u8,
    /// The backup id of the last Profile write that failed, or empty. The worker sets it.
    pub failed_write_backup: String,
}

/// What the Feed message of one Leakshield did.
#[derive(Default)]
struct FeedStatus {
    /// A Loop links the Leakshield.
    linked: bool,
    /// `None` until the first send result after the Feed went on. `Some(true)`: the send worked.
    result: Option<bool>,
    last: (u64, f64, f64),
}

struct State {
    report1: Report1,
    connected: bool,
    mode: Option<Mode>,
    running: Option<Action>,
    clock_offset: i64,
    feed: FeedStatus,
}

pub struct Attached {
    id: String,
    serial: String,
    io: DeviceIo,
    registry: Arc<Registry>,
    backups: Backups,
    last_known: Arc<LastKnown>,
    worker: WriteWorker,
    state: Mutex<State>,
    info: Arc<watch::Sender<Info>>,
    events: mpsc::UnboundedSender<DeviceEvent>,
    events_rx: Mutex<Option<mpsc::UnboundedReceiver<DeviceEvent>>>,
    last_auto_clock: Mutex<Option<Instant>>,
    software: Arc<SoftwareSlots>,
    /// The task that waits for the device to store the Mode that the last `SetMode` set.
    mode_store: Mutex<Option<JoinHandle<()>>>,
    /// The runtime of `open`. A D-Bus method runs on the executor of zbus, where `tokio::spawn`
    /// has no runtime.
    runtime: tokio::runtime::Handle,
}

impl fmt::Debug for Attached {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Attached").field("id", &self.id).finish()
    }
}

pub fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64)
}

fn failed(message: impl Into<String>) -> Error {
    Error::ZBus(zbus::fdo::Error::Failed(message.into()).into())
}

fn device_error(error: DeviceError) -> Error {
    match error {
        DeviceError::Disconnected => Error::Disconnected("the Leakshield is not connected".into()),
        other => failed(other.to_string()),
    }
}

fn io_error(error: io::Error) -> Error {
    match error.kind() {
        io::ErrorKind::NotFound => Error::not_found("no such backup"),
        io::ErrorKind::InvalidInput => Error::invalid(error.to_string()),
        _ => failed(error.to_string()),
    }
}

/// The reply of a failed write (`docs/api.md`, Errors).
fn write_error(error: WriteError) -> Error {
    match error {
        WriteError::Refused(WriteRefusal::ReadOnly(reason)) => Error::read_only(reason.as_str()),
        WriteError::Refused(WriteRefusal::NotVerified(field)) => Error::NotVerified(field),
        WriteError::Refused(WriteRefusal::UnknownField(field)) => {
            Error::not_found(format!("no field {field}"))
        }
        WriteError::OutOfRange { field } => {
            Error::invalid(format!("the value of {field} is out of range"))
        }
        WriteError::Busy => Error::Busy("a write is queued".into()),
        WriteError::DeviceChanged => {
            Error::DeviceChanged("report 3 changed since the first read".into())
        }
        WriteError::VerifyFailed { backup, .. } => Error::VerifyFailed(backup),
        WriteError::Disconnected { .. } => {
            Error::Disconnected("the Leakshield is not connected".into())
        }
        WriteError::NoSuchBackup => Error::not_found("no such backup"),
        WriteError::BackupMismatch(why) => {
            Error::invalid(format!("the backup does not fit: {why}"))
        }
        WriteError::Backup(e) => io_error(e),
        WriteError::Device { error, .. } => device_error(error),
        WriteError::NotSupported => {
            Error::not_supported("the registry of this firmware lacks the command")
        }
        WriteError::NoStoredSlot => Error::not_found("the slot has no stored report 3"),
        WriteError::ActiveSlot => {
            Error::invalid("the slot is the active slot of the device: use SetFields")
        }
        WriteError::FieldNotEditable(field) => {
            Error::invalid(format!("{field} is not an editable field"))
        }
        WriteError::BadValue { field } => {
            Error::invalid(format!("the value of {field} does not fit the field"))
        }
    }
}

fn info_of(registry: &Registry, state: &State) -> Info {
    let read_only_reason = if !state.connected {
        "disconnected"
    } else {
        Access::new(registry, &state.report1)
            .device_reason()
            .map_or("", |r| r.as_str())
    };
    let feed_state = if !state.connected
        || !state.feed.linked
        || Access::new(registry, &state.report1)
            .device_reason()
            .is_some()
    {
        "off"
    } else {
        match state.feed.result {
            None => "waiting",
            Some(true) => "ok",
            Some(false) => "failing",
        }
    };
    Info {
        feed_state,
        last_feed: state.feed.last,
        active_slot: state.report1.active_profile,
        failed_write_backup: String::new(),
        firmware: state.report1.firmware_version,
        structure: state.report1.structure_number,
        connected: state.connected,
        read_only_reason: read_only_reason.to_owned(),
        mode: state.mode.map_or("", Mode::as_str).to_owned(),
        running_action: state.running.map_or("", Action::as_str).to_owned(),
    }
}

/// Shows `next` on the watch. The worker owns `failed_write_backup`, so it stays as it is.
fn publish(sender: &watch::Sender<Info>, mut next: Info) {
    sender.send_if_modified(|current| {
        next.failed_write_backup
            .clone_from(&current.failed_write_backup);
        let changed = *current != next;
        *current = next;
        changed
    });
}

/// The first read of a Leakshield: a device that the registry covers gets its report 3 copied
/// into the four slots, each with its own number. This reads the device and writes nothing to it.
fn init_slots(io: &DeviceIo, params: &AttachParams, id: &str, serial: &str, report1: &Report1) {
    let covered = Access::new(&params.registry, report1)
        .device_reason()
        .is_none();
    let slots = SlotStore::new(Arc::new(StateStore::new(&params.state_dir)));
    if !covered || Slot::all().all(|slot| slots.exists(id, slot)) {
        return;
    }
    let meta = SlotMeta::new(report1.firmware_version, report1.structure_number, serial);
    let result = blocking(|| io.read_report3())
        .map_err(|e| e.to_string())
        .and_then(|report3| {
            slots
                .init(&params.registry, id, &report3, &meta)
                .map_err(|e| e.to_string())
        });
    if let Err(e) = result {
        eprintln!("nicecoolingd: the slots of {id} are not created: {e}");
    }
}

impl Attached {
    /// Reads report 1 once, for the serial. Call it inside the runtime: it starts the write worker.
    pub fn open(
        io: DeviceIo,
        params: &AttachParams,
    ) -> std::result::Result<Arc<Self>, DeviceError> {
        let report1 = blocking(|| io.read_report1())?;
        let serial = report1.serial_text();
        let id = ids::leakshield_id(&serial);
        init_slots(&io, params, &id, &serial, &report1);
        let (events, events_rx) = mpsc::unbounded_channel();
        let last_known = Arc::new(LastKnown::open(&params.state_dir, &id));
        let (mode, running) = action::interpret(report1.mode, None);
        let state = State {
            clock_offset: clock::offset(&report1, now_unix()),
            report1,
            connected: true,
            mode,
            running,
            feed: FeedStatus::default(),
        };
        let (info, _) = watch::channel(info_of(&params.registry, &state));
        let info = Arc::new(info);
        let worker = WriteWorker::spawn(WorkerParts {
            io: io.clone(),
            registry: Arc::clone(&params.registry),
            backups: Backups::new(&params.state_dir, &serial),
            last_known: Arc::clone(&last_known),
            events: events.clone(),
            save_delay: params.save_delay,
            info: Arc::clone(&info),
        });
        Ok(Arc::new(Self {
            backups: Backups::new(&params.state_dir, &serial),
            id,
            serial,
            io,
            registry: Arc::clone(&params.registry),
            last_known,
            worker,
            state: Mutex::new(state),
            info,
            events,
            events_rx: Mutex::new(Some(events_rx)),
            last_auto_clock: Mutex::new(None),
            software: Arc::default(),
            mode_store: Mutex::new(None),
            runtime: tokio::runtime::Handle::current(),
        }))
    }

    pub fn id(&self) -> String {
        self.id.clone()
    }

    pub fn serial(&self) -> &str {
        &self.serial
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn io(&self) -> &DeviceIo {
        &self.io
    }

    /// The Software sensor slots. The Feed stage reads them.
    pub fn software_slots(&self) -> Arc<SoftwareSlots> {
        Arc::clone(&self.software)
    }

    /// The device vanished: its transports close and the Leakshield shows as disconnected.
    pub fn detach(&self) {
        self.io.detach();
        self.mark_disconnected();
    }

    /// The device came back with new transports. It reads report 1 at once, so that the state
    /// shows the device again before the next period of the source.
    pub fn reattach(&self, io: &DeviceIo) {
        self.io.replace(io);
        self.poll(now_unix());
    }

    pub fn info(&self) -> Info {
        self.info.borrow().clone()
    }

    pub fn subscribe(&self) -> watch::Receiver<Info> {
        self.info.subscribe()
    }

    /// Device time minus PC time at the last report 1, in seconds.
    pub fn clock_offset(&self) -> i64 {
        self.state.lock().unwrap().clock_offset
    }

    /// The receiving end of the device events, for the signals. The first call gets it.
    pub fn take_events(&self) -> Option<mpsc::UnboundedReceiver<DeviceEvent>> {
        self.events_rx.lock().unwrap().take()
    }

    /// Checks report 3 every `period` and sends `ExternalChange` events.
    pub fn spawn_external_detection(&self, period: Duration) -> JoinHandle<()> {
        let detector = ExternalDetector::new(
            self.io.clone(),
            Arc::clone(&self.registry),
            Arc::clone(&self.last_known),
        );
        tokio::spawn(detector.run(period, self.events.clone()))
    }

    /// Changes the state and shows the new `Info`. A Feed that is off forgets its send result, so
    /// it waits for a new one when it goes on.
    fn refresh(&self, change: impl FnOnce(&mut State)) {
        let mut state = self.state.lock().unwrap();
        change(&mut state);
        let info = info_of(&self.registry, &state);
        if info.feed_state == "off" {
            state.feed.result = None;
        }
        drop(state);
        publish(&self.info, info);
    }

    fn apply(&self, report1: Report1, now: i64) {
        self.refresh(|state| {
            (state.mode, state.running) = action::interpret(report1.mode, state.mode);
            state.clock_offset = clock::offset(&report1, now);
            state.report1 = report1;
            state.connected = true;
        });
    }

    fn mark_disconnected(&self) {
        self.refresh(|state| state.connected = false);
    }

    /// The result of a Feed message send. A result that arrives while the Feed is off does not
    /// count: the send started before the Feed went off, and its failure must not bring back the
    /// failing time that the change to `off` reset. `counted` runs, under the state lock, only
    /// for a result that counts. A vanished device shows at once.
    pub fn feed_result(
        &self,
        result: &std::result::Result<FeedSent, DeviceError>,
        counted: impl FnOnce(bool),
    ) {
        let mut state = self.state.lock().unwrap();
        if info_of(&self.registry, &state).feed_state == "off" {
            return;
        }
        counted(result.is_ok());
        match result {
            Ok(sent) => {
                state.feed.result = Some(true);
                state.feed.last = (now_us(), sent.pump_speed_rpm, sent.flow_lph);
            }
            Err(DeviceError::Disconnected) => state.connected = false,
            Err(_) => state.feed.result = Some(false),
        }
        let info = info_of(&self.registry, &state);
        if info.feed_state == "off" {
            state.feed.result = None;
        }
        drop(state);
        publish(&self.info, info);
    }

    /// A Loop links the Leakshield, or no Loop does. Without a Loop the Feed is off.
    pub fn set_feed_linked(&self, linked: bool) {
        if self.state.lock().unwrap().feed.linked != linked {
            self.refresh(|state| state.feed.linked = linked);
        }
    }

    /// Resets the failing time of the value table whenever the Feed goes off.
    pub fn spawn_feed_reset(&self, table: ValueTable) -> JoinHandle<()> {
        let (mut info, id) = (self.info.subscribe(), self.id.clone());
        tokio::spawn(async move {
            loop {
                if info.borrow_and_update().feed_state == "off" {
                    table.feed_off(&id);
                }
                if info.changed().await.is_err() {
                    return;
                }
            }
        })
    }

    /// Reads report 1, updates the state and runs the automatic clock sync. Blocks on the device.
    /// Returns the report, or `None` when the device did not answer.
    pub fn poll(&self, now: i64) -> Option<Report1> {
        match blocking(|| self.io.read_report1()) {
            Ok(report1) => {
                self.apply(report1.clone(), now);
                self.auto_clock(&report1, now);
                Some(report1)
            }
            Err(DeviceError::Disconnected) => {
                self.mark_disconnected();
                None
            }
            Err(_) => None,
        }
    }

    fn auto_clock(&self, report1: &Report1, now: i64) {
        if Access::new(&self.registry, report1)
            .device_reason()
            .is_some()
        {
            return;
        }
        let mut last = self.last_auto_clock.lock().unwrap();
        if last.is_some_and(|t| t.elapsed() < AUTO_CLOCK_GAP) {
            return;
        }
        match blocking(|| clock::auto_sync(&self.io, &self.registry, report1, now)) {
            Ok(true) => *last = Some(Instant::now()),
            Ok(false) => {}
            Err(e) => eprintln!(
                "nicecoolingd: the automatic clock sync of {} failed: {e}",
                self.id
            ),
        }
    }

    /// Reads report 1 now and updates the state.
    pub fn fresh_report1(&self) -> Result<Report1> {
        match blocking(|| self.io.read_report1()) {
            Ok(report1) => {
                self.apply(report1.clone(), now_unix());
                Ok(report1)
            }
            Err(e) => {
                if matches!(e, DeviceError::Disconnected) {
                    self.mark_disconnected();
                }
                Err(device_error(e))
            }
        }
    }

    fn require_writable(&self, report1: &Report1) -> Result<()> {
        match Access::new(&self.registry, report1).device_reason() {
            Some(reason) => Err(Error::read_only(reason.as_str())),
            None => Ok(()),
        }
    }

    /// Sends one report 2 command. A registry without it gives `NotSupported`.
    fn send(&self, command: Option<Report2>, delay: Duration) -> Result<()> {
        self.send_late(command.is_some(), delay, || command)
    }

    /// Like `send`, but `build` makes the command at the send, after the report 1 read and the
    /// delay. `supported` is the answer for a registry without the command, before any I/O.
    fn send_late(
        &self,
        supported: bool,
        delay: Duration,
        build: impl FnOnce() -> Option<Report2>,
    ) -> Result<()> {
        if !supported {
            return Err(Error::not_supported(
                "the registry of this firmware lacks the command",
            ));
        }
        let report1 = self.fresh_report1()?;
        self.require_writable(&report1)?;
        blocking(|| {
            std::thread::sleep(delay);
            match build() {
                Some(command) => self.io.send_command(command).map(|()| true),
                None => Ok(false),
            }
        })
        .map_err(device_error)?
        .then_some(())
        .ok_or_else(|| Error::not_supported("the device clock is out of range"))?;
        // The device shows the result in its next report 1.
        let _ = self.fresh_report1();
        Ok(())
    }

    /// Sets the Mode. The device stores it in report 3 some time later. The daemon then stores it
    /// in the slot that the device runs (`mode::follow_default_mode`).
    pub fn set_mode(&self, mode: &str, slots: &SlotStore) -> Result<()> {
        let mode = Mode::parse(mode)
            .ok_or_else(|| Error::invalid(format!("{mode:?} is not shield, monitor or release")))?;
        let mut task = self.mode_store.lock().unwrap();
        self.send(
            mode::set_mode_command(&self.registry, mode.code()),
            COMMAND_DELAY,
        )?;
        let code = mode.code() as u8;
        let store = mode::ModeStore {
            io: self.io.clone(),
            registry: Arc::clone(&self.registry),
            last_known: Arc::clone(&self.last_known),
            slots: slots.clone(),
            leakshield: self.id(),
        };
        // A newer Mode replaces the wait for the older one.
        if let Some(older) = task.take() {
            older.abort();
        }
        self.last_known.expect_mode(Some(code));
        *task = Some(self.runtime.spawn(mode::follow_default_mode(store, code)));
        Ok(())
    }

    /// `refill_blocked` tells whether a Pump channel of the Loop of this Leakshield runs. It is
    /// asked only for a Refill.
    pub fn start_action(&self, action: &str, refill_blocked: impl FnOnce() -> bool) -> Result<()> {
        let action = Action::parse(action).ok_or_else(|| {
            Error::invalid(format!(
                "{action:?} is not test, measure-fill-level, deaerate or refill"
            ))
        })?;
        if action == Action::Refill && refill_blocked() {
            return Err(Error::PumpRunning(
                "a Pump channel of the Loop reads above 0 rpm or has no tach value: start the Refill with the device button while the PC is in standby"
                    .into(),
            ));
        }
        self.send(
            mode::set_mode_command(&self.registry, action.code()),
            COMMAND_DELAY,
        )
    }

    pub fn cancel_action(&self) -> Result<()> {
        self.send(mode::cancel_command(&self.registry), COMMAND_DELAY)
    }

    /// Sets the device clock from the PC clock, whatever the offset.
    pub fn sync_clock(&self) -> Result<()> {
        let supported = self.registry.command("set_clock").is_some();
        self.send_late(supported, Duration::ZERO, || {
            clock::command(&self.registry, now_unix())
        })
    }

    /// True when the registry covers the firmware and the structure of the device. The Feed goes
    /// out only then.
    pub fn feed_covered(&self) -> bool {
        let state = self.state.lock().unwrap();
        Access::new(&self.registry, &state.report1)
            .device_reason()
            .is_none()
    }

    pub fn reset_refill_history(&self) -> Result<()> {
        self.send(
            counters::reset_refill_history(&self.registry),
            COMMAND_DELAY,
        )
    }

    /// Sends `reset_alarm`. The device clears its latched bits; the next report 1 shows the result.
    pub fn reset_alarms(&self) -> Result<()> {
        self.send(
            reset_alarm::command(&self.registry),
            reset_alarm::COMMAND_DELAY,
        )
    }

    pub fn reset_service_counter(&self) -> Result<()> {
        let serial = self.state.lock().unwrap().report1.serial;
        self.send(
            counters::reset_service_counter(&self.registry, serial),
            COMMAND_DELAY,
        )
    }

    /// The events of report 1, oldest first.
    pub fn get_events(&self) -> Result<Vec<Event>> {
        Ok(decode_events(&self.fresh_report1()?))
    }

    pub fn get_fields(&self) -> Result<Vec<FieldRow>> {
        let report1 = self.fresh_report1()?;
        let report3 = blocking(|| self.io.read_report3()).map_err(device_error)?;
        Ok(fields::rows(&self.registry, &report1, &report3))
    }

    fn edits(&self, values: Vec<(String, FieldValue)>) -> Result<Vec<FieldEdit>> {
        fields::edits(&self.registry, values).map_err(|e| match e {
            EditError::NoSuchField(id) => Error::not_found(format!("no field {id}")),
            EditError::BadValue(id) => {
                Error::invalid(format!("the value of {id} does not fit the field"))
            }
        })
    }

    async fn stored(
        &self,
        slot: u8,
        slots: &SlotStore,
        values: Option<Vec<(String, Option<FieldValue>)>>,
    ) -> Result<StoredReply> {
        let slot = Slot::new(u32::from(slot))
            .ok_or_else(|| Error::invalid("a slot is a number from 1 to 4"))?;
        self.worker
            .stored(StoredRequest {
                slots: slots.clone(),
                leakshield: self.id(),
                slot,
                values,
            })
            .await
            .map_err(write_error)
    }

    /// The stored settings of a slot (design D18). The device is not read.
    pub async fn get_stored_fields(
        &self,
        slot: u8,
        slots: &SlotStore,
    ) -> Result<Vec<(String, FieldValue)>> {
        match self.stored(slot, slots, None).await? {
            StoredReply::Fields(fields) => Ok(fields),
            StoredReply::Written => Err(failed("the worker answered a read with a write")),
        }
    }

    /// Changes the stored report 3 of another slot. The device gets nothing. A value that the
    /// caller could not decode is `None`: the job refuses it after the slot checks.
    pub async fn set_stored_fields(
        &self,
        slot: u8,
        values: Vec<(String, Option<FieldValue>)>,
        slots: &SlotStore,
    ) -> Result<()> {
        self.stored(slot, slots, Some(values)).await.map(|_| ())
    }

    /// One write path run for all fields. Returns after the read-back, with the report 3 that the
    /// read-back returned, or `None` when there was nothing to write.
    pub async fn set_fields(&self, values: Vec<(String, FieldValue)>) -> Result<Option<Report3>> {
        let edits = self.edits(values)?;
        if edits.is_empty() {
            return Ok(None);
        }
        let report1 = self.fresh_report1()?;
        let written = self
            .worker
            .submit(Job::Write { report1, edits })
            .await
            .map_err(write_error)?;
        Ok(Some(written.report3))
    }

    /// A Profile activation (design D14). It queues the request now, so that the requests of
    /// two activations reach the worker in the order of the calls. The worker decides, and reads
    /// the slot when the write goes out.
    pub fn request_profile(&self, slot: Slot, slots: &SlotStore) -> ProfileTicket {
        self.worker.request_profile(ProfileRequest {
            slot,
            slots: slots.clone(),
            leakshield: self.id(),
            serial: self.serial().to_owned(),
        })
    }

    /// A newer activation has no slot to write: the request that waits is dropped.
    pub fn cancel_profile(&self) {
        self.worker.cancel_profile();
    }

    /// Waits for the end of a Profile request and takes the report 1 that its decision read.
    pub async fn finish_profile(
        &self,
        ticket: ProfileTicket,
    ) -> std::result::Result<Decision, Failure> {
        match ticket.await {
            Ok(Ok(done)) => {
                if done.read {
                    self.apply(done.report1, now_unix());
                } else {
                    // The rest of the report is older than the last poll.
                    self.refresh(|state| {
                        state.report1.active_profile = done.report1.active_profile
                    });
                }
                Ok(done.decision)
            }
            Ok(Err(failure)) => Err(failure),
            Err(_) => Err(Failure::Write(WriteError::Disconnected { backup: None })),
        }
    }

    /// Returns the report 3 that the read-back returned.
    pub async fn restore_backup(&self, backup: &str) -> Result<Report3> {
        let report1 = self.fresh_report1()?;
        let written = self
            .worker
            .submit(Job::Restore {
                report1,
                backup: backup.to_owned(),
            })
            .await
            .map_err(write_error)?;
        Ok(written.report3)
    }

    /// Backs up the settings, sends the factory defaults command and reads report 3 back.
    pub async fn factory_reset(&self) -> Result<()> {
        let report1 = self.fresh_report1()?;
        self.worker
            .submit(Job::FactoryReset { report1 })
            .await
            .map_err(write_error)?;
        Ok(())
    }

    pub fn list_backups(&self) -> Result<Vec<BackupEntry>> {
        self.backups.list().map_err(io_error)
    }

    /// Backs up reports 3, 5 and 8. It waits for a write that runs.
    pub fn create_backup(&self) -> Result<String> {
        let report1 = self.fresh_report1()?;
        let env = WriteEnv {
            io: &self.io,
            registry: &self.registry,
            backups: &self.backups,
        };
        blocking(|| {
            let _exclusive = self.last_known.lock();
            take_backup(&env, &report1, Reason::Manual, now_us())
        })
        .map_err(write_error)
    }

    pub fn get_backup(&self, backup: &str) -> Result<BackupSet> {
        self.backups.get(backup).map_err(io_error)
    }

    /// The firmware that a backup was taken with.
    pub fn backup_firmware(&self, backup: &str) -> Result<u16> {
        self.backups.firmware(backup).map_err(io_error)
    }

    /// Saves reports of a client as a backup. It needs the structure id of the device and a
    /// firmware that the registry covers.
    pub fn import_backup(&self, firmware: u16, set: &BackupSet) -> Result<String> {
        if firmware != self.registry.firmware {
            return Err(Error::not_supported(format!(
                "the registry does not cover firmware {firmware}"
            )));
        }
        let structure = self.state.lock().unwrap().report1.structure_number;
        if let Ok(report3) = Report3::decode(&set.report3)
            && let Some(field) = out_of_range_field(&self.registry, &report3)
        {
            return Err(Error::invalid(format!(
                "the value of {field} in report 3 is out of its range"
            )));
        }
        self.backups
            .import(set, structure, firmware, now_us())
            .map(|saved| saved.id)
            .map_err(|e| match e {
                ImportError::Io(e) => io_error(e),
                other => Error::invalid(other.to_string()),
            })
    }
}
