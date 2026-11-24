//! The refill history store. The device keeps 7 refill entries in report 1. The daemon keeps each
//! entry that has a time and pump revolutions, keyed by its time, for 90 days, so that the app can
//! chart the pump runtime per day (`docs/storage.md`, `refill-history.json`).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use nicecooling_protocol::report1::Report1;
use serde::{Deserialize, Serialize};

use super::clock::DEVICE_EPOCH_UNIX;
use crate::state::StateStore;

/// The daemon drops an entry that is older than this.
pub const RETENTION_S: i64 = 90 * 86_400;

/// One refill: the time is the device time in microseconds since the Unix epoch, as in
/// `GetEvents`. The volume is in 0.1 ml.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Refill {
    pub time_us: u64,
    pub duration_s: u32,
    pub pump_turns: u32,
    pub volume_tenth_ml: i32,
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    schema_version: u32,
    #[serde(default)]
    entries: Vec<Refill>,
}

pub struct RefillHistory {
    store: Arc<StateStore>,
    rel: String,
    /// By `time_us`.
    entries: Mutex<BTreeMap<u64, Refill>>,
}

fn cutoff_us(now_unix: i64) -> u64 {
    u64::try_from((now_unix - RETENTION_S).max(0)).unwrap_or(0) * 1_000_000
}

impl RefillHistory {
    /// Loads the file of a Leakshield. A missing or damaged file gives an empty history.
    pub fn open(store: Arc<StateStore>, leakshield: &str) -> Self {
        let rel = format!("leakshield/{leakshield}/refill-history.json");
        let loaded = store.load::<File>(&rel).value;
        Self {
            store,
            rel,
            entries: Mutex::new(loaded.entries.into_iter().map(|e| (e.time_us, e)).collect()),
        }
    }

    /// Takes the refill entries of `report1` that have a time and pump revolutions above 0, and
    /// drops what is older than the retention. An entry with a known time replaces the stored
    /// one. The file is written only when the history changed.
    pub fn ingest(&self, report1: &Report1, now_unix: i64) {
        let mut entries = self.entries.lock().unwrap();
        let before = entries.clone();
        for entry in &report1.refill_history {
            if entry.time == 0 || entry.pump_turns == 0 {
                continue;
            }
            let time_us = (u64::from(entry.time) + DEVICE_EPOCH_UNIX as u64) * 1_000_000;
            entries.insert(
                time_us,
                Refill {
                    time_us,
                    duration_s: entry.duration,
                    pump_turns: entry.pump_turns,
                    volume_tenth_ml: entry.volume,
                },
            );
        }
        let cutoff = cutoff_us(now_unix);
        entries.retain(|&time_us, _| time_us >= cutoff);
        if *entries == before {
            return;
        }
        let file = File {
            schema_version: 1,
            entries: entries.values().copied().collect(),
        };
        if let Err(e) = self.store.save(&self.rel, &file) {
            eprintln!("nicecoolingd: the refill history is not saved: {e}");
        }
    }

    /// The stored refills, oldest first.
    pub fn entries(&self) -> Vec<Refill> {
        self.entries.lock().unwrap().values().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nicecooling_protocol::report1::RefillEntry;

    const FW1022: &[u8] =
        include_bytes!("../../../../tests/fixtures/leakshield/fw1022/report1.bin");
    /// A device time that is 2026-11-22 in device seconds, and the Unix second of it.
    const DEVICE_TIME: u32 = 564_623_071;
    const NOW: i64 = DEVICE_TIME as i64 + DEVICE_EPOCH_UNIX + 60;

    fn dir(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("nicecooling-refill-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn history(dir: &std::path::Path) -> RefillHistory {
        RefillHistory::open(Arc::new(StateStore::new(dir)), "leakshield_1_2")
    }

    fn report1(entries: &[RefillEntry]) -> Report1 {
        let mut report1 = Report1::decode(FW1022).unwrap();
        report1.refill_history = Default::default();
        report1.refill_history[..entries.len()].copy_from_slice(entries);
        report1
    }

    fn entry(time: u32, duration: u32, turns: u32) -> RefillEntry {
        RefillEntry {
            time,
            duration,
            pump_turns: turns,
            volume: 1234,
        }
    }

    #[test]
    fn the_fixture_has_no_refill() {
        let dir = dir("fixture");
        let history = history(&dir);
        history.ingest(&Report1::decode(FW1022).unwrap(), NOW);
        assert!(history.entries().is_empty());
        assert!(!dir.join("leakshield").exists());
    }

    #[test]
    fn an_entry_needs_a_time_and_pump_revolutions() {
        let dir = dir("filter");
        let history = history(&dir);
        history.ingest(
            &report1(&[
                entry(0, 10, 50),
                entry(DEVICE_TIME - 100, 12, 0),
                entry(DEVICE_TIME - 200, 9, 40),
            ]),
            NOW,
        );
        let kept = history.entries();
        assert_eq!(kept.len(), 1);
        assert_eq!(
            kept[0],
            Refill {
                time_us: (u64::from(DEVICE_TIME - 200) + DEVICE_EPOCH_UNIX as u64) * 1_000_000,
                duration_s: 9,
                pump_turns: 40,
                volume_tenth_ml: 1234,
            }
        );
    }

    #[test]
    fn an_entry_with_the_same_time_replaces_the_stored_one() {
        let dir = dir("key");
        let history = history(&dir);
        history.ingest(&report1(&[entry(DEVICE_TIME - 100, 5, 20)]), NOW);
        history.ingest(&report1(&[entry(DEVICE_TIME - 100, 8, 30)]), NOW);
        let kept = history.entries();
        assert_eq!(kept.len(), 1);
        assert_eq!((kept[0].duration_s, kept[0].pump_turns), (8, 30));
    }

    #[test]
    fn entries_stay_when_the_device_forgets_them_and_leave_after_90_days() {
        let dir = dir("retention");
        let history = history(&dir);
        history.ingest(&report1(&[entry(DEVICE_TIME - 100, 5, 20)]), NOW);
        // The device list is empty now, for example after a reset of its history.
        history.ingest(&report1(&[]), NOW + 89 * 86_400);
        assert_eq!(history.entries().len(), 1);
        history.ingest(&report1(&[]), NOW + 91 * 86_400);
        assert!(history.entries().is_empty());
    }

    #[test]
    fn the_file_survives_a_restart() {
        let dir = dir("restart");
        history(&dir).ingest(&report1(&[entry(DEVICE_TIME - 100, 5, 20)]), NOW);
        let text =
            std::fs::read_to_string(dir.join("leakshield/leakshield_1_2/refill-history.json"))
                .unwrap();
        assert!(text.contains("\"schema_version\": 1"));
        assert_eq!(history(&dir).entries().len(), 1);
    }
}
