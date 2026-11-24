//! The write worker: one task, a queue of one, and the timer of the save-settings command.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nicecooling_core::profile::Slot;
use nicecooling_protocol::access::WriteRefusal;
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report1::Report1;
use nicecooling_protocol::report3::Report3;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot, watch};
use tokio::time::Instant;

use super::attached::Info;
use super::backup::Backups;
use super::external::LastKnown;
use super::factory_reset::factory_reset;
use super::fields::{self, FieldValue};
use super::save_settings;
use super::write::{
    FieldEdit, WriteEnv, WriteError, Written, edit_stored, restore, write_edited, write_fields,
    write_stored,
};
use super::{DeviceEvent, DeviceIo, blocking};
use crate::profiles::activate::{Decision, Failure, decide};
use crate::profiles::spacing::{Decision as Timing, Spacing};
use crate::state::SlotStore;
use crate::state::slots::SlotMeta;

pub enum Job {
    /// `report1` is the latest report 1: it gives the firmware and the structure.
    Write {
        report1: Report1,
        edits: Vec<FieldEdit>,
    },
    Restore {
        report1: Report1,
        backup: String,
    },
    FactoryReset {
        report1: Report1,
    },
    /// The write of a Profile request that the worker decided on (`ProfileRequest`).
    ProfileWrite {
        report1: Report1,
        report3: Report3,
        /// The slot is edited: back up, write and read back (`write_edited`).
        verified: bool,
    },
}

/// A read or a change of the stored settings of a slot. It sends nothing to the device.
pub struct StoredRequest {
    pub slots: SlotStore,
    /// The id of the Leakshield: the name of its slot directory.
    pub leakshield: String,
    pub slot: Slot,
    /// `None` reads the slot. A value is `None` when the caller could not decode it. The job
    /// checks the fields after the slot checks, so the order of the errors follows design D18.
    pub values: Option<Vec<(String, Option<FieldValue>)>>,
}

pub enum StoredReply {
    Fields(Vec<(String, FieldValue)>),
    Written,
}

/// A Profile activation for one Leakshield (design D14). The worker decides on it when it arrives
/// and reads the slot again when the write goes out, so a change of the slot while the request
/// waits is not lost.
pub struct ProfileRequest {
    pub slot: Slot,
    pub slots: SlotStore,
    /// The id of the Leakshield: the name of its slot directory.
    pub leakshield: String,
    pub serial: String,
}

/// How a Profile request ended. `report1` is the one that the decision used.
#[derive(Debug)]
pub struct ProfileDone {
    pub decision: Decision,
    pub report1: Report1,
    /// The decision read `report1` from the device. Else it is the report that the worker knew,
    /// and only its `active_profile` is current.
    pub read: bool,
}

/// The result of a Profile request. A newer request that replaces it gives `Superseded`.
pub type ProfileTicket = oneshot::Receiver<Result<ProfileDone, Failure>>;

struct ProfileMessage {
    request: ProfileRequest,
    reply: oneshot::Sender<Result<ProfileDone, Failure>>,
}

struct StoredMessage {
    request: StoredRequest,
    reply: oneshot::Sender<Result<StoredReply, WriteError>>,
}

enum Queued {
    Job(Box<Message>),
    Stored(StoredMessage),
    Profile(ProfileMessage),
    /// A newer activation that sends no request: it still drops the request that waits.
    CancelProfile,
}

struct Message {
    job: Job,
    reply: oneshot::Sender<Result<Written, WriteError>>,
    /// Held until the job is done. It makes the queue hold one write. A Profile write has none.
    permit: Option<OwnedSemaphorePermit>,
}

pub struct WorkerParts {
    pub io: DeviceIo,
    pub registry: Arc<Registry>,
    pub backups: Backups,
    pub last_known: Arc<LastKnown>,
    pub events: mpsc::UnboundedSender<DeviceEvent>,
    /// `SAVE_SETTINGS_DELAY` in the daemon.
    pub save_delay: Duration,
    /// What the properties show. The worker sets `failed_write_backup` and `active_slot`.
    pub info: Arc<watch::Sender<Info>>,
}

/// Handle of the worker task. The task ends when the last handle is dropped.
#[derive(Clone)]
pub struct WriteWorker {
    queue: mpsc::UnboundedSender<Queued>,
    slot: Arc<Semaphore>,
}

impl WriteWorker {
    pub fn spawn(parts: WorkerParts) -> Self {
        let (queue, rx) = mpsc::unbounded_channel();
        tokio::spawn(run(parts, rx));
        Self {
            queue,
            slot: Arc::new(Semaphore::new(1)),
        }
    }

    /// Returns `Busy` at once when a write is queued or running. Else it waits for the result.
    /// The write runs to its end even when the caller drops this future.
    pub async fn submit(&self, job: Job) -> Result<Written, WriteError> {
        let permit = self
            .slot
            .clone()
            .try_acquire_owned()
            .map_err(|_| WriteError::Busy)?;
        let (reply, result) = oneshot::channel();
        let message = Message {
            job,
            reply,
            permit: Some(permit),
        };
        if self.queue.send(Queued::Job(Box::new(message))).is_err() {
            return Err(WriteError::Disconnected { backup: None });
        }
        result
            .await
            .unwrap_or(Err(WriteError::Disconnected { backup: None }))
    }
}

impl WriteWorker {
    /// Runs a read or a change of a stored slot in the order of the queue. A Profile write that
    /// waits goes first.
    pub async fn stored(&self, request: StoredRequest) -> Result<StoredReply, WriteError> {
        let (reply, result) = oneshot::channel();
        let message = Queued::Stored(StoredMessage { request, reply });
        if self.queue.send(message).is_err() {
            return Err(WriteError::Disconnected { backup: None });
        }
        result
            .await
            .unwrap_or(Err(WriteError::Disconnected { backup: None }))
    }

    /// Drops the Profile request that waits, in the order of the queue. The request ends with
    /// `Superseded`. Use it when a newer activation has no request to send.
    pub fn cancel_profile(&self) {
        let _ = self.queue.send(Queued::CancelProfile);
    }

    /// Queues a Profile request. The queue keeps the order of the calls, so the request of the
    /// newest activation is the last one. A request that is still waiting for its time is dropped
    /// when a newer one arrives, whatever the newer one does.
    pub fn request_profile(&self, request: ProfileRequest) -> ProfileTicket {
        let (reply, ticket) = oneshot::channel();
        let message = Queued::Profile(ProfileMessage { request, reply });
        if let Err(mpsc::error::SendError(Queued::Profile(lost))) = self.queue.send(message) {
            let _ = lost
                .reply
                .send(Err(Failure::Write(WriteError::Disconnected {
                    backup: None,
                })));
        }
        ticket
    }
}

fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64)
}

fn execute(parts: &WorkerParts, job: Job) -> Result<Written, WriteError> {
    let env = WriteEnv {
        io: &parts.io,
        registry: &parts.registry,
        backups: &parts.backups,
    };
    let mut last = parts.last_known.lock();
    let result = match job {
        Job::Write { report1, edits } => {
            let base = match last.get() {
                Some(base) => base.clone(),
                None => parts
                    .io
                    .read_report3()
                    .map_err(|error| WriteError::Device {
                        error,
                        backup: None,
                    })?,
            };
            write_fields(&env, &report1, &base, &edits, now_us())
        }
        Job::Restore { report1, backup } => restore(&env, &report1, &backup, now_us()),
        Job::FactoryReset { report1 } => factory_reset(&env, &report1, now_us()),
        Job::ProfileWrite {
            report1,
            report3,
            verified: false,
        } => write_stored(&env, &report1, &report3, now_us()),
        Job::ProfileWrite {
            report1,
            report3,
            verified: true,
        } => write_edited(&env, &report1, &report3, now_us()),
    };
    if let Ok(written) = &result
        && let Err(e) = last.set(written.report3.clone())
    {
        eprintln!("nicecoolingd: last known report 3 not saved: {e}");
    }
    result
}

fn note_failed_backup(parts: &WorkerParts, backup: &str) {
    parts.info.send_if_modified(|info| {
        let changed = info.failed_write_backup != backup;
        backup.clone_into(&mut info.failed_write_backup);
        changed
    });
}

/// The device runs the slot that a good Profile write put on it, until report 1 says otherwise.
fn note_active_slot(parts: &WorkerParts, slot: u8) {
    parts.info.send_if_modified(|info| {
        let changed = info.active_slot != slot;
        info.active_slot = slot;
        changed
    });
}

/// The edits of the values of a `SetStoredFields`, one field after the other: an unknown field is
/// refused first, then a value that does not fit it.
fn decode_edits(
    registry: &Registry,
    values: Vec<(String, Option<FieldValue>)>,
) -> Result<Vec<FieldEdit>, WriteError> {
    let mut decoded = Vec::with_capacity(values.len());
    for (id, value) in values {
        if registry.field(&id).is_none() {
            return Err(WriteError::Refused(WriteRefusal::UnknownField(id)));
        }
        let value = value.ok_or_else(|| WriteError::BadValue { field: id.clone() })?;
        decoded.push((id, value));
    }
    fields::edits(registry, decoded).map_err(|e| match e {
        fields::EditError::NoSuchField(id) => WriteError::Refused(WriteRefusal::UnknownField(id)),
        fields::EditError::BadValue(field) => WriteError::BadValue { field },
    })
}

/// Reads or changes a stored slot. The checks use the firmware and the structure of the slot and
/// the last report 1: the device is not read.
fn run_stored(parts: &WorkerParts, request: StoredRequest) -> Result<StoredReply, WriteError> {
    let registry = &parts.registry;
    let stored = request
        .slots
        .load(&request.leakshield, request.slot)
        .map_err(WriteError::Backup)?
        .ok_or(WriteError::NoStoredSlot)?;
    if stored.meta.firmware != registry.firmware
        || stored.meta.structure != registry.structure
        || stored.report3.structure_number() != registry.structure
    {
        return Err(WriteError::NotSupported);
    }
    let Some(values) = request.values else {
        return Ok(StoredReply::Fields(fields::values(
            registry,
            &stored.report3,
        )));
    };
    if parts.info.borrow().active_slot == request.slot.get() {
        return Err(WriteError::ActiveSlot);
    }
    if let Some((id, _)) = values.iter().find(|(id, _)| id == "device_profile") {
        return Err(WriteError::FieldNotEditable(id.clone()));
    }
    let edits = decode_edits(registry, values)?;
    let report3 = edit_stored(
        registry,
        stored.meta.firmware,
        stored.meta.structure,
        &stored.report3,
        &edits,
    )?;
    if !edits.is_empty() {
        let meta = SlotMeta {
            edited: true,
            ..stored.meta
        };
        request
            .slots
            .store(registry, &request.leakshield, request.slot, &report3, &meta)
            .map_err(WriteError::Backup)?;
    }
    Ok(StoredReply::Written)
}

fn send_save_settings(parts: &WorkerParts, backup: &str) {
    let failure = match save_settings::command(&parts.registry) {
        None => Some("save_settings is not in the registry".to_owned()),
        Some(command) => parts.io.send_command(command).err().map(|e| e.to_string()),
    };
    if let Some(error) = failure {
        let _ = parts.events.send(DeviceEvent::WriteFailed {
            error,
            backup: backup.to_owned(),
        });
    }
}

/// Runs a job and plans the save-settings command that follows a write.
fn run_job(
    parts: &WorkerParts,
    job: Job,
    save: &mut Option<(Instant, String)>,
) -> Result<Written, WriteError> {
    // The factory reset takes the settings from the device: no save-settings command follows.
    let saves = !matches!(job, Job::FactoryReset { .. });
    let restores = matches!(job, Job::Restore { .. });
    let result = blocking(|| execute(parts, job));
    if restores && result.is_ok() {
        note_failed_backup(parts, "");
    }
    // A write that failed its read-back still changed report 3 on the device.
    let backup = match &result {
        Ok(written) => Some(&written.backup),
        Err(WriteError::VerifyFailed { backup, .. }) => Some(backup),
        Err(_) => None,
    };
    if let Some(backup) = backup
        && saves
    {
        *save = Some((Instant::now() + parts.save_delay, backup.clone()));
    }
    result
}

fn run_message(parts: &WorkerParts, message: Message, save: &mut Option<(Instant, String)>) {
    let Message { job, reply, permit } = message;
    let result = run_job(parts, job, save);
    // The queue is free before the caller sees the result.
    drop(permit);
    let _ = reply.send(result);
}

/// What the worker keeps between Profile requests: the spacing, and the report 1 that it knows.
#[derive(Default)]
struct ProfileState {
    spacing: Spacing,
    /// Report 1 as the worker last read it, or as its last good Profile write left it. Inside the
    /// spacing window the worker decides on this one and reads no report 1: a read waits for the
    /// next report of the device (every 500 ms) after any reader of the same node drained the
    /// queue, and that wait delays the write that waits for its time. A job that is not a
    /// Profile write changes the device without a decision, so it drops this report.
    report1: Option<Report1>,
    /// The last decision read `report1` from the device.
    read: bool,
}

/// Reads the stored slot, and report 1 unless `ProfileState` holds one that is valid now. Decides
/// what the request does to the device.
fn decide_profile(
    parts: &WorkerParts,
    request: &ProfileRequest,
    state: &mut ProfileState,
    in_window: bool,
) -> Result<(Report1, Decision), Failure> {
    let report1 = match state.report1.clone().filter(|_| in_window) {
        Some(known) => {
            state.read = false;
            known
        }
        None => {
            let fresh = blocking(|| parts.io.read_report1())
                .map_err(|e| Failure::Unreachable(e.to_string()))?;
            state.report1 = Some(fresh.clone());
            state.read = true;
            fresh
        }
    };
    let stored = request
        .slots
        .load(&request.leakshield, request.slot)
        .ok()
        .flatten();
    let decision = decide(
        &parts.registry,
        &request.serial,
        &report1,
        request.slot,
        stored,
    )?;
    Ok((report1, decision))
}

/// A decision that writes: the report 3, and whether the write reads it back.
fn writes(decision: Decision) -> Result<(Box<Report3>, bool), Decision> {
    match decision {
        Decision::Write(report3) => Ok((report3, false)),
        Decision::WriteVerified(report3) => Ok((report3, true)),
        other => Err(other),
    }
}

/// Writes the report 3 that a decision chose, and answers the request.
fn write_profile(
    parts: &WorkerParts,
    request: &ProfileRequest,
    reply: oneshot::Sender<Result<ProfileDone, Failure>>,
    report1: Report1,
    (report3, verified): (Box<Report3>, bool),
    save: &mut Option<(Instant, String)>,
    state: &mut ProfileState,
) {
    let job = Job::ProfileWrite {
        report1: report1.clone(),
        report3: (*report3).clone(),
        verified,
    };
    let result = run_job(parts, job, save);
    // The spacing counts from the end of the write: a slow backup before the report 3 write must
    // not bring the next write closer than 1.5 s to the device.
    state.spacing.wrote(Instant::now());
    match &result {
        Ok(_) => {
            note_failed_backup(parts, "");
            note_active_slot(parts, request.slot.get());
            // The read-back matched: the slot is as the device holds it.
            if verified
                && let Err(e) = request
                    .slots
                    .set_edited(&request.leakshield, request.slot, false)
            {
                eprintln!(
                    "nicecoolingd: the edited mark of slot {} of {} stays: {e}",
                    request.slot, request.leakshield
                );
            }
        }
        Err(error) => {
            if let Some(backup) = error.backup_id() {
                note_failed_backup(parts, backup);
            }
        }
    }
    let read = state.read;
    let result = result
        .map(|_| ProfileDone {
            decision: if verified {
                Decision::WriteVerified(report3)
            } else {
                Decision::Write(report3)
            },
            // The report 1 of the decision is older than the write: the device runs the new slot.
            report1: Report1 {
                active_profile: request.slot.get(),
                ..report1
            },
            read,
        })
        .map_err(Failure::Write);
    // A failed write may have changed the device: the next decision reads report 1.
    state.report1 = result.as_ref().ok().map(|done| done.report1.clone());
    let _ = reply.send(result);
}

/// Ends a Profile request that is due: decides again, so that the write has the slot as it is
/// now. The time is inside the spacing window, so the decision reads no report 1.
fn finish_profile(
    parts: &WorkerParts,
    message: ProfileMessage,
    save: &mut Option<(Instant, String)>,
    state: &mut ProfileState,
) {
    let ProfileMessage { request, reply } = message;
    match decide_profile(parts, &request, state, true) {
        Ok((report1, decision)) => match writes(decision) {
            Ok(write) => write_profile(parts, &request, reply, report1, write, save, state),
            Err(decision) => {
                let _ = reply.send(Ok(ProfileDone {
                    decision,
                    report1,
                    read: state.read,
                }));
            }
        },
        Err(failure) => {
            let _ = reply.send(Err(failure));
        }
    }
}

/// A request arrives. The older request that waits goes first, so that a request that fails or
/// needs no write still cancels it. A write goes out at once or waits (`profiles::spacing`).
fn start_profile(
    parts: &WorkerParts,
    message: ProfileMessage,
    waiting: &mut Option<(Instant, ProfileMessage)>,
    save: &mut Option<(Instant, String)>,
    state: &mut ProfileState,
) {
    if let Some((_, old)) = waiting.take() {
        let _ = old.reply.send(Err(Failure::Superseded));
    }
    let now = Instant::now();
    let timing = state.spacing.request(now);
    let in_window = matches!(timing, Timing::WriteAt(_));
    match decide_profile(parts, &message.request, state, in_window) {
        Ok((report1, decision)) => match writes(decision) {
            Ok(write) => match timing {
                Timing::WriteAt(at) if at > now => *waiting = Some((at, message)),
                _ => {
                    let ProfileMessage { request, reply } = message;
                    write_profile(parts, &request, reply, report1, write, save, state);
                }
            },
            Err(decision) => {
                let _ = message.reply.send(Ok(ProfileDone {
                    decision,
                    report1,
                    read: state.read,
                }));
            }
        },
        Err(failure) => {
            let _ = message.reply.send(Err(failure));
        }
    }
}

async fn run(parts: WorkerParts, mut queue: mpsc::UnboundedReceiver<Queued>) {
    // The time of the save-settings command, and the backup that a failure points at.
    let mut save: Option<(Instant, String)> = None;
    // The spacing of the Profile writes, and the newest Profile request that waits.
    let mut profiles = ProfileState::default();
    let mut waiting: Option<(Instant, ProfileMessage)> = None;
    loop {
        let wake = [save.as_ref().map(|s| s.0), waiting.as_ref().map(|w| w.0)]
            .into_iter()
            .flatten()
            .min();
        let message = match wake {
            Some(at) => tokio::select! {
                message = queue.recv() => message,
                () = tokio::time::sleep_until(at) => {
                    let now = Instant::now();
                    // A Profile write first: it moves the save-settings command back.
                    if waiting.as_ref().is_some_and(|w| w.0 <= now)
                        && let Some((_, message)) = waiting.take()
                    {
                        finish_profile(&parts, message, &mut save, &mut profiles);
                    }
                    if save.as_ref().is_some_and(|s| s.0 <= now)
                        && let Some((_, backup)) = save.take()
                    {
                        blocking(|| send_save_settings(&parts, &backup));
                    }
                    continue;
                }
            },
            None => queue.recv().await,
        };
        let Some(message) = message else {
            // The last handle is gone (daemon stop, detach). The command is still due. A Profile
            // write that waits goes with the device.
            if let Some((deadline, backup)) = save.take() {
                tokio::time::sleep_until(deadline).await;
                blocking(|| send_save_settings(&parts, &backup));
            }
            return;
        };
        match message {
            Queued::Job(message) => {
                // A Profile write that waits goes first, at its time. The job then works on
                // the slot that the Profile write put on the device, and its read-back goes
                // into that slot.
                if let Some((at, old)) = waiting.take() {
                    tokio::time::sleep_until(at).await;
                    finish_profile(&parts, old, &mut save, &mut profiles);
                }
                run_message(&parts, *message, &mut save);
                profiles.report1 = None;
            }
            Queued::Stored(StoredMessage { request, reply }) => {
                // As for a job: the Profile write that waits goes first.
                if let Some((at, old)) = waiting.take() {
                    tokio::time::sleep_until(at).await;
                    finish_profile(&parts, old, &mut save, &mut profiles);
                }
                let _ = reply.send(run_stored(&parts, request));
            }
            Queued::Profile(message) => {
                start_profile(&parts, message, &mut waiting, &mut save, &mut profiles);
            }
            Queued::CancelProfile => {
                if let Some((_, old)) = waiting.take() {
                    let _ = old.reply.send(Err(Failure::Superseded));
                }
            }
        }
    }
}
