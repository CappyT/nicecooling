//! External change detection: every 10 s the daemon reads report 3 and compares it with the last
//! known value outside the volatile mask. The last known value is on disk, so a restart keeps it.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use nicecooling_protocol::device::DeviceError;
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report3::Report3;
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;

use super::{DeviceEvent, DeviceIo, blocking};
use crate::state::atomic::{RealFs, create_dir_all_with, write_atomic};

pub const POLL_PERIOD: Duration = Duration::from_secs(10);

/// Id of a difference in the structure id bytes, which no registry field covers.
const STRUCTURE_ID: &str = "structure_number";

/// The last report 3 that the daemon read or wrote. The lock makes a write and a poll exclusive,
/// so a poll never sees the device between a write and the update of this value.
pub struct LastKnown {
    path: PathBuf,
    value: Mutex<Option<Report3>>,
}

pub struct LastKnownGuard<'a> {
    owner: &'a LastKnown,
    value: MutexGuard<'a, Option<Report3>>,
}

impl LastKnown {
    /// `<state dir>/leakshield/<leakshield id>/last-known-report3.bin`. A missing or invalid
    /// file gives no value.
    pub fn open(state_dir: &std::path::Path, leakshield_id: &str) -> Self {
        let path = state_dir
            .join("leakshield")
            .join(leakshield_id)
            .join("last-known-report3.bin");
        let value = fs::read(&path).ok().and_then(|b| Report3::decode(&b).ok());
        Self {
            path,
            value: Mutex::new(value),
        }
    }

    pub fn lock(&self) -> LastKnownGuard<'_> {
        LastKnownGuard {
            owner: self,
            value: self.value.lock().unwrap(),
        }
    }
}

impl LastKnownGuard<'_> {
    pub fn get(&self) -> Option<&Report3> {
        self.value.as_ref()
    }

    /// Keeps the value in memory and writes it to disk. The error is the disk error.
    pub fn set(&mut self, report: Report3) -> io::Result<()> {
        let write = (|| {
            if let Some(dir) = self.owner.path.parent() {
                create_dir_all_with(&RealFs, dir)?;
            }
            write_atomic(&self.owner.path, &report.encode())
        })();
        *self.value = Some(report);
        write
    }
}

/// Ids of the registry fields that cover the offsets, in registry order.
pub fn field_ids(registry: &Registry, offsets: &[usize]) -> Vec<String> {
    let mut ids = Vec::new();
    for field in &registry.field {
        let size = field.value_type.scalar.size();
        let hit = field
            .element_offsets()
            .iter()
            .any(|&start| offsets.iter().any(|&o| o >= start && o < start + size));
        if hit {
            ids.push(field.id.clone());
        }
    }
    if offsets.iter().any(|&o| o < 3) {
        ids.push(STRUCTURE_ID.to_owned());
    }
    ids
}

pub struct ExternalDetector {
    io: DeviceIo,
    registry: Arc<Registry>,
    last_known: Arc<LastKnown>,
}

impl ExternalDetector {
    pub fn new(io: DeviceIo, registry: Arc<Registry>, last_known: Arc<LastKnown>) -> Self {
        Self {
            io,
            registry,
            last_known,
        }
    }

    /// Reads report 3. Returns the ids of the changed fields, or `None` when nothing changed
    /// outside the volatile mask. A change becomes the last known value. Without a last known
    /// value, the first read becomes it. This writes nothing to the device.
    pub fn poll(&self) -> Result<Option<Vec<String>>, DeviceError> {
        let mut last = self.last_known.lock();
        let current = self.io.read_report3()?;
        let changed = match last.get() {
            None => None,
            Some(known) => {
                let offsets =
                    known.differing_offsets(&current, &self.registry.report3.volatile_mask);
                (!offsets.is_empty()).then(|| field_ids(&self.registry, &offsets))
            }
        };
        if (changed.is_some() || last.get().is_none())
            && let Err(e) = last.set(current)
        {
            eprintln!("nicecoolingd: last known report 3 not saved: {e}");
        }
        Ok(changed)
    }

    /// Polls every `period` and sends `ExternalChange`. A failed read waits for the next period.
    pub async fn run(self, period: Duration, events: mpsc::UnboundedSender<DeviceEvent>) {
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            if let Ok(Some(fields)) = blocking(|| self.poll())
                && events.send(DeviceEvent::ExternalChange { fields }).is_err()
            {
                return;
            }
        }
    }
}
