//! Rediscovery. A hwmon chip can appear after the daemon starts (`asus_ec_sensors` loads late).
//! A new chip gets its source tasks and its sensors join the Inputs. A new Channel is not taken
//! under control: it has no entry in the BIOS snapshot, and the snapshot is taken only at the
//! start without a lease.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use nicecooling_core::clock::Clock;
use nicecooling_hwmon::board::BoardFile;
use nicecooling_hwmon::discovery::{Discovery, discover_with};
use nicecooling_hwmon::sysfs::Sysfs;
use tokio::task::JoinHandle;

use crate::dbus::{Hub, sensor_meta};
use crate::sources::hwmon::{chip_readers, spawn_source};
use crate::state::{Identity, identities_of, known_prefixes};
use crate::value_table::ValueTable;

/// The default time between two scans of `class/hwmon`.
pub const RESCAN_PERIOD: Duration = Duration::from_secs(10);

pub struct Rescan {
    pub sysfs: Sysfs,
    pub board: Option<BoardFile>,
    pub names: BTreeMap<String, String>,
    /// The identities that the daemon knows.
    pub identities: BTreeMap<String, Identity>,
    /// The sensor ids that have a source.
    pub sensors: BTreeSet<String>,
    pub hub: Arc<Hub>,
    pub values: ValueTable,
    pub clock: Arc<dyn Clock>,
    pub source_period: Duration,
    pub period: Duration,
}

/// Aborts the source tasks that the rescan started when the rescan task ends.
#[derive(Default)]
struct Sources(Vec<JoinHandle<()>>);

impl Drop for Sources {
    fn drop(&mut self) {
        for task in &self.0 {
            task.abort();
        }
    }
}

impl Rescan {
    pub fn spawn(mut self) -> JoinHandle<()> {
        tokio::spawn(async move {
            let mut sources = Sources::default();
            let mut interval =
                tokio::time::interval_at(tokio::time::Instant::now() + self.period, self.period);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                interval.tick().await;
                let (sysfs, board, names, known) = (
                    self.sysfs.clone(),
                    self.board.clone(),
                    self.names.clone(),
                    known_prefixes(&self.identities),
                );
                // Reading `class/hwmon` can block on a device, so it stays off the runtime.
                let Ok(Ok(found)) = tokio::task::spawn_blocking(move || {
                    discover_with(&sysfs, board.as_ref(), &names, &known)
                })
                .await
                else {
                    continue;
                };
                sources.0.extend(self.add(found));
            }
        })
    }

    /// Starts the sources of the sensors that are new. Returns their tasks.
    fn add(&mut self, found: Discovery) -> Vec<JoinHandle<()>> {
        let fresh = Discovery {
            channels: Vec::new(),
            sensors: found
                .sensors
                .iter()
                .filter(|s| !self.sensors.contains(&s.id))
                .cloned()
                .collect(),
        };
        let identities: BTreeMap<_, _> = identities_of(&found)
            .into_iter()
            .filter(|(id, _)| !self.identities.contains_key(id))
            .collect();
        if fresh.sensors.is_empty() && identities.is_empty() {
            return Vec::new();
        }
        let tasks = chip_readers(&fresh)
            .into_iter()
            .map(|reader| {
                spawn_source(
                    reader,
                    self.values.clone(),
                    self.clock.clone(),
                    self.source_period,
                )
                .task
            })
            .collect();
        self.sensors
            .extend(fresh.sensors.iter().map(|s| s.id.clone()));
        self.identities.extend(identities.clone());
        self.hub
            .add_sensors(fresh.sensors.iter().map(sensor_meta).collect(), identities);
        tasks
    }
}
