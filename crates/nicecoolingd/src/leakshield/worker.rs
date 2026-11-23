//! The write worker: one task, a queue of one, and the timer of the save-settings command.

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report1::Report1;
use tokio::sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot};
use tokio::time::Instant;

use super::backup::Backups;
use super::external::LastKnown;
use super::factory_reset::factory_reset;
use super::save_settings;
use super::write::{FieldEdit, WriteEnv, WriteError, Written, restore, write_fields};
use super::{DeviceEvent, DeviceIo, blocking};

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
}

struct Message {
    job: Job,
    reply: oneshot::Sender<Result<Written, WriteError>>,
    /// Held until the job is done. It makes the queue hold one write.
    permit: OwnedSemaphorePermit,
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
    queue: mpsc::UnboundedSender<Message>,
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
        let message = Message { job, reply, permit };
        if self.queue.send(message).is_err() {
            return Err(WriteError::Disconnected { backup: None });
        }
        result
            .await
            .unwrap_or(Err(WriteError::Disconnected { backup: None }))
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

async fn run(parts: WorkerParts, mut queue: mpsc::UnboundedReceiver<Message>) {
    // The time of the save-settings command, and the backup that a failure points at.
    let mut pending: Option<(Instant, String)> = None;
    loop {
        let message = match &pending {
            Some((deadline, _)) => tokio::select! {
                message = queue.recv() => message,
                () = tokio::time::sleep_until(*deadline) => {
                    if let Some((_, backup)) = pending.take() {
                        blocking(|| send_save_settings(&parts, &backup));
                    }
                    continue;
                }
            },
            None => queue.recv().await,
        };
        let Some(Message { job, reply, permit }) = message else {
            // The last handle is gone (daemon stop, detach). The command is still due.
            if let Some((deadline, backup)) = pending.take() {
                tokio::time::sleep_until(deadline).await;
                blocking(|| send_save_settings(&parts, &backup));
            }
            return;
        };
        // The factory reset takes the settings from the device: no save-settings command follows.
        let saves = !matches!(job, Job::FactoryReset { .. });
        let result = blocking(|| execute(&parts, job));
        // A write that failed its read-back still changed report 3 on the device.
        let backup = match &result {
            Ok(written) => Some(&written.backup),
            Err(WriteError::VerifyFailed { backup, .. }) => Some(backup),
            Err(_) => None,
        };
        if let Some(backup) = backup
            && saves
        {
            pending = Some((Instant::now() + parts.save_delay, backup.clone()));
        }
        // The queue is free before the caller sees the result.
        drop(permit);
        let _ = reply.send(result);
    }
}
