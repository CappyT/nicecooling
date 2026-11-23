//! The write worker: one task, a queue of one, and the timer of the save-settings command.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nicecooling_core::profile::Slot;
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report1::Report1;
use nicecooling_protocol::report3::Report3;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio::time::Instant;

use super::backup::Backups;
use super::external::LastKnown;
use super::factory_reset::factory_reset;
use super::save_settings;
use super::write::{FieldEdit, WriteEnv, WriteError, Written, restore, write_fields, write_stored};
use super::{DeviceEvent, DeviceIo, blocking};
use crate::profiles::activate::{Decision, Failure, decide};
use crate::profiles::spacing::{Decision as Timing, Spacing};
use crate::state::SlotStore;

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
    },
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

/// How a Profile request ended. `report1` is the one that the decision read.
#[derive(Debug)]
pub struct ProfileDone {
    pub decision: Decision,
    pub report1: Report1,
}

/// The result of a Profile request. A newer request that replaces it gives `Superseded`.
pub type ProfileTicket = oneshot::Receiver<Result<ProfileDone, Failure>>;

struct ProfileMessage {
    request: ProfileRequest,
    reply: oneshot::Sender<Result<ProfileDone, Failure>>,
}

enum Queued {
    Job(Box<Message>),
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
        Job::ProfileWrite { report1, report3 } => write_stored(&env, &report1, &report3, now_us()),
    };
    if let Ok(written) = &result
        && let Err(e) = last.set(written.report3.clone())
    {
        eprintln!("nicecoolingd: last known report 3 not saved: {e}");
    }
    result
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
    let result = blocking(|| execute(parts, job));
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

/// Reads report 1 and the stored slot now, and decides what the request does to the device.
fn decide_profile(
    parts: &WorkerParts,
    request: &ProfileRequest,
) -> Result<(Report1, Decision), Failure> {
    let report1 =
        blocking(|| parts.io.read_report1()).map_err(|e| Failure::Unreachable(e.to_string()))?;
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

/// Writes the report 3 that a decision chose, and answers the request.
fn write_profile(
    parts: &WorkerParts,
    reply: oneshot::Sender<Result<ProfileDone, Failure>>,
    report1: Report1,
    report3: Box<Report3>,
    save: &mut Option<(Instant, String)>,
    spacing: &mut Spacing,
) {
    spacing.wrote(Instant::now());
    let job = Job::ProfileWrite {
        report1: report1.clone(),
        report3: (*report3).clone(),
    };
    let result = run_job(parts, job, save)
        .map(|_| ProfileDone {
            decision: Decision::Write(report3),
            report1,
        })
        .map_err(Failure::Write);
    let _ = reply.send(result);
}

/// Ends a Profile request that is due: decides again, so that the write has the slot as it is
/// now.
fn finish_profile(
    parts: &WorkerParts,
    message: ProfileMessage,
    save: &mut Option<(Instant, String)>,
    spacing: &mut Spacing,
) {
    let ProfileMessage { request, reply } = message;
    match decide_profile(parts, &request) {
        Ok((report1, Decision::Write(report3))) => {
            write_profile(parts, reply, report1, report3, save, spacing);
        }
        Ok((report1, decision)) => {
            let _ = reply.send(Ok(ProfileDone { decision, report1 }));
        }
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
    spacing: &mut Spacing,
) {
    if let Some((_, old)) = waiting.take() {
        let _ = old.reply.send(Err(Failure::Superseded));
    }
    let now = Instant::now();
    match decide_profile(parts, &message.request) {
        Ok((report1, Decision::Write(report3))) => match spacing.request(now) {
            Timing::WriteAt(at) if at > now => *waiting = Some((at, message)),
            _ => write_profile(parts, message.reply, report1, report3, save, spacing),
        },
        Ok((report1, decision)) => {
            let _ = message.reply.send(Ok(ProfileDone { decision, report1 }));
        }
        Err(failure) => {
            let _ = message.reply.send(Err(failure));
        }
    }
}

async fn run(parts: WorkerParts, mut queue: mpsc::UnboundedReceiver<Queued>) {
    // The time of the save-settings command, and the backup that a failure points at.
    let mut save: Option<(Instant, String)> = None;
    // The spacing of the Profile writes, and the newest Profile request that waits.
    let mut spacing = Spacing::default();
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
                        finish_profile(&parts, message, &mut save, &mut spacing);
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
                    finish_profile(&parts, old, &mut save, &mut spacing);
                }
                run_message(&parts, *message, &mut save);
            }
            Queued::Profile(message) => {
                start_profile(&parts, message, &mut waiting, &mut save, &mut spacing);
            }
            Queued::CancelProfile => {
                if let Some((_, old)) = waiting.take() {
                    let _ = old.reply.send(Err(Failure::Superseded));
                }
            }
        }
    }
}
