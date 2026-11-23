//! The Alarm history (design D18, `docs/storage.md`): one row for each instance, with the outcome
//! of its poweroff. It has its own writer, and it controls nothing: a damaged file loads the copy,
//! else an empty history, and the daemon only logs it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::poweroff::{Effect, Row};
use crate::state::{Source, StateStore};

pub const HISTORY_FILE: &str = "alarm-history.json";
/// The rows that each severity keeps.
pub const KEEP: usize = 1000;
/// The writer saves at most this often.
pub const SAVE_EVERY: Duration = Duration::from_secs(1);

pub const NONE: &str = "none";
pub const NOTIFIED: &str = "notified";
pub const PENDING: &str = "poweroff-pending";
pub const CANCELLED: &str = "poweroff-cancelled";
pub const STOPPED: &str = "poweroff-stopped";
pub const REQUESTED: &str = "poweroff-requested";
pub const FAILED: &str = "poweroff-failed";

/// A value of `details`, with the signature of its D-Bus type as the tag: `{"u": 3}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Detail {
    #[serde(rename = "d")]
    Double(f64),
    #[serde(rename = "u")]
    Unsigned(u32),
    #[serde(rename = "s")]
    Text(String),
    #[serde(rename = "o")]
    Object(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HistoryRow {
    pub instance: String,
    pub event: String,
    pub severity: String,
    pub source: String,
    pub response: String,
    pub raised_us: u64,
    /// 0 while the instance is active.
    pub cleared_us: u64,
    pub outcome: String,
    /// The daemon closed the row at its start, not the end of the instance.
    #[serde(default)]
    pub closed_by_restart: bool,
    #[serde(default)]
    pub details: BTreeMap<String, Detail>,
}

#[derive(Debug, Serialize, Deserialize)]
struct File {
    schema_version: u32,
    rows: Vec<HistoryRow>,
}

impl Default for File {
    fn default() -> Self {
        Self {
            schema_version: 1,
            rows: Vec::new(),
        }
    }
}

fn details_of(row: &Row) -> BTreeMap<String, Detail> {
    [("bits", row.bits), ("latched_bits", row.latched)]
        .into_iter()
        .filter(|(_, value)| *value != 0)
        .map(|(key, value)| (key.to_owned(), Detail::Unsigned(value)))
        .collect()
}

/// The rows and the changes that the alarm manager makes to them. No I/O.
#[derive(Debug, Default)]
pub struct History {
    /// Oldest raise first.
    rows: Vec<HistoryRow>,
    dirty: bool,
}

impl History {
    pub fn from_rows(rows: Vec<HistoryRow>) -> Self {
        Self { rows, dirty: false }
    }

    pub fn rows(&self) -> &[HistoryRow] {
        &self.rows
    }

    /// Closes the rows of an earlier start that are still open: the instance ended with the
    /// daemon, at an unknown time.
    pub fn close_open_rows(&mut self, started_us: u64) {
        for row in self.rows.iter_mut().filter(|r| r.cleared_us == 0) {
            row.cleared_us = started_us;
            row.closed_by_restart = true;
            self.dirty = true;
        }
    }

    /// A new row. Each severity keeps its last `KEEP` rows.
    pub fn raise(&mut self, row: &Row) {
        let outcome = match row.response {
            "none" => NONE,
            "notify" => NOTIFIED,
            _ => PENDING,
        };
        self.rows.push(HistoryRow {
            instance: row.instance.clone(),
            event: row.event.clone(),
            severity: row.severity.to_owned(),
            source: row.source.clone(),
            response: row.response.to_owned(),
            raised_us: row.raised_us,
            cleared_us: 0,
            outcome: outcome.to_owned(),
            closed_by_restart: false,
            details: details_of(row),
        });
        let same = self.rows.iter().filter(|r| r.severity == row.severity);
        if same.clone().count() > KEEP
            && let Some(oldest) = same.map(|r| r.instance.clone()).next()
        {
            self.rows.retain(|r| r.instance != oldest);
        }
        self.dirty = true;
    }

    fn find(&mut self, instance: &str) -> Option<&mut HistoryRow> {
        self.rows.iter_mut().rev().find(|r| r.instance == instance)
    }

    /// Moves a row to `to` when its outcome is one of `from`. Returns true when it moved.
    fn go(&mut self, instance: &str, from: &[&str], to: &str) -> bool {
        let Some(row) = self.find(instance) else {
            return false;
        };
        if !from.contains(&row.outcome.as_str()) {
            return false;
        }
        to.clone_into(&mut row.outcome);
        self.dirty = true;
        true
    }

    /// The instance ends. A poweroff that waits, or that waits for a retry, ends with it.
    pub fn clear(&mut self, instance: &str, at_us: u64) {
        self.go(instance, &[PENDING, FAILED], STOPPED);
        if let Some(row) = self.find(instance)
            && row.cleared_us == 0
        {
            row.cleared_us = at_us;
            self.dirty = true;
        }
    }

    /// Applies one effect of the alarm manager. `rows` are the active instances after it.
    /// Returns true when the change must reach the disk before anything else happens: the call to
    /// logind follows.
    pub fn apply(&mut self, effect: &Effect, rows: &[Row], at_us: u64) -> bool {
        match effect {
            Effect::Raised(row) => self.raise(row),
            Effect::Updated(instance) => {
                if let Some(active) = rows.iter().find(|r| &r.instance == instance) {
                    let details = details_of(active);
                    if let Some(row) = self.find(instance)
                        && row.details != details
                    {
                        row.details = details;
                        self.dirty = true;
                    }
                }
            }
            Effect::Cleared(instance) => self.clear(instance, at_us),
            Effect::PoweroffCancelled(instance) => {
                self.go(instance, &[PENDING, FAILED], CANCELLED);
            }
            Effect::PoweroffFailed { instance, .. } => {
                self.go(instance, &[REQUESTED], FAILED);
            }
            Effect::CallPoweroff(instance) => {
                return self.go(instance, &[PENDING, FAILED], REQUESTED);
            }
        }
        false
    }

    /// The rows that were active at or after `since_us`, newest raise first. `limit` 0 gives all.
    pub fn query(&self, since_us: u64, limit: u32) -> Vec<HistoryRow> {
        let rows = self
            .rows
            .iter()
            .rev()
            .filter(|r| r.cleared_us == 0 || r.cleared_us >= since_us)
            .cloned();
        if limit == 0 {
            rows.collect()
        } else {
            rows.take(limit as usize).collect()
        }
    }

    fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }
}

/// Saves when told, at most once in `period`.
struct Writer {
    changed: Arc<Notify>,
    task: JoinHandle<()>,
}

impl Writer {
    fn spawn(period: Duration, mut flush: impl FnMut() + Send + 'static) -> Self {
        let changed = Arc::new(Notify::new());
        let task = tokio::spawn({
            let changed = changed.clone();
            async move {
                let mut last: Option<Instant> = None;
                loop {
                    changed.notified().await;
                    if let Some(last) = last {
                        tokio::time::sleep_until(last + period).await;
                    }
                    flush();
                    last = Some(Instant::now());
                }
            }
        });
        Self { changed, task }
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct Shared {
    history: Mutex<History>,
    store: Arc<StateStore>,
    /// One save at a time, so that an old copy never replaces a new one.
    saving: Mutex<()>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Shared {
    fn save(&self) {
        let _saving = lock(&self.saving);
        let rows = {
            let mut history = lock(&self.history);
            if !history.take_dirty() {
                return;
            }
            history.rows.clone()
        };
        let file = File {
            schema_version: 1,
            rows,
        };
        if let Err(e) = self.store.save(HISTORY_FILE, &file) {
            eprintln!("nicecoolingd: {HISTORY_FILE} is not saved: {e}");
            lock(&self.history).dirty = true;
        }
    }
}

/// The history of the daemon: the rows, and the writer that saves them.
pub struct HistoryStore {
    shared: Arc<Shared>,
    writer: Writer,
}

impl HistoryStore {
    /// Loads the file and closes the rows of an earlier start. Call it inside the runtime.
    pub fn open(store: Arc<StateStore>, started_us: u64, period: Duration) -> Self {
        let loaded = store.load::<File>(HISTORY_FILE);
        match (loaded.source, loaded.fallback) {
            (Source::LastKnownGood, _) => {
                eprintln!("nicecoolingd: {HISTORY_FILE} is damaged: loaded its copy");
            }
            (Source::Defaults, true) => {
                eprintln!("nicecoolingd: {HISTORY_FILE} is damaged: the history is empty");
            }
            _ => {}
        }
        let mut history = History::from_rows(loaded.value.rows);
        history.close_open_rows(started_us);
        let dirty = history.dirty;
        let shared = Arc::new(Shared {
            history: Mutex::new(history),
            store,
            saving: Mutex::new(()),
        });
        let writer = Writer::spawn(period, {
            let shared = shared.clone();
            move || shared.save()
        });
        if dirty {
            writer.changed.notify_one();
        }
        Self { shared, writer }
    }

    /// Records the effects of the alarm manager. A change to `poweroff-requested` is on disk when
    /// this returns.
    pub fn apply(&self, effects: &[Effect], rows: &[Row], at_us: u64) {
        let mut urgent = false;
        {
            let mut history = lock(&self.shared.history);
            for effect in effects {
                urgent |= history.apply(effect, rows, at_us);
            }
        }
        if urgent {
            self.shared.save();
        } else if !effects.is_empty() {
            self.writer.changed.notify_one();
        }
    }

    pub fn query(&self, since_us: u64, limit: u32) -> Vec<HistoryRow> {
        lock(&self.shared.history).query(since_us, limit)
    }

    /// Saves now. The daemon calls it at its stop.
    pub fn save_now(&self) {
        self.shared.save();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nicecooling_core::alarms::Severity;
    use std::path::PathBuf;

    fn row(n: u64, severity: Severity, response: &'static str) -> Row {
        Row {
            instance: format!("alarm_1_{n}"),
            event: "pump-stop".into(),
            severity: severity.as_str(),
            source: "loop_1".into(),
            response,
            raised_us: n * 1000,
            poweroff_at_us: 0,
            bits: 0,
            latched: 0,
        }
    }

    fn outcome(history: &History, n: u64) -> String {
        history.rows()[(n - 1) as usize].outcome.clone()
    }

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("nicecooling-history-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn outcome_state_machine() {
        let mut h = History::default();
        let rows = [];
        for (n, response, expected) in [
            (1, "none", NONE),
            (2, "notify", NOTIFIED),
            (3, "poweroff", PENDING),
        ] {
            h.apply(&Effect::Raised(row(n, Severity::Alarm, response)), &rows, 0);
            assert_eq!(outcome(&h, n), expected);
        }
        // A final outcome stays through every later effect.
        for n in [1, 2] {
            let id = format!("alarm_1_{n}");
            h.apply(&Effect::PoweroffCancelled(id.clone()), &rows, 0);
            h.apply(&Effect::CallPoweroff(id.clone()), &rows, 0);
            h.apply(&Effect::Cleared(id), &rows, 5);
        }
        assert_eq!(
            (outcome(&h, 1), outcome(&h, 2)),
            (NONE.into(), NOTIFIED.into())
        );

        // The call to logind: requested, and the caller must save at once.
        assert!(h.apply(&Effect::CallPoweroff("alarm_1_3".into()), &rows, 0));
        assert_eq!(outcome(&h, 3), REQUESTED);
        // A clear after the call keeps `requested`: the PC powers off.
        let failed = Effect::PoweroffFailed {
            instance: "alarm_1_3".into(),
            error: "blocked".into(),
        };
        h.apply(&failed, &rows, 0);
        assert_eq!(outcome(&h, 3), FAILED);
        assert!(h.apply(&Effect::CallPoweroff("alarm_1_3".into()), &rows, 0));
        assert_eq!(outcome(&h, 3), REQUESTED);
        h.apply(&Effect::Cleared("alarm_1_3".into()), &rows, 9);
        assert_eq!(outcome(&h, 3), REQUESTED);
        assert_eq!(h.rows()[2].cleared_us, 9);

        // A cancel gives `cancelled` from `pending` and from `failed`.
        for (n, via_failure) in [(4, false), (5, true)] {
            h.apply(
                &Effect::Raised(row(n, Severity::Alarm, "poweroff")),
                &rows,
                0,
            );
            let id = format!("alarm_1_{n}");
            if via_failure {
                h.apply(&Effect::CallPoweroff(id.clone()), &rows, 0);
                h.apply(
                    &Effect::PoweroffFailed {
                        instance: id.clone(),
                        error: String::new(),
                    },
                    &rows,
                    0,
                );
            }
            h.apply(&Effect::PoweroffCancelled(id), &rows, 0);
            assert_eq!(outcome(&h, n), CANCELLED);
        }
        // A clear gives `stopped` from `pending` and from `failed`.
        for (n, via_failure) in [(6, false), (7, true)] {
            h.apply(
                &Effect::Raised(row(n, Severity::Alarm, "poweroff")),
                &rows,
                0,
            );
            let id = format!("alarm_1_{n}");
            if via_failure {
                h.apply(&Effect::CallPoweroff(id.clone()), &rows, 0);
                h.apply(
                    &Effect::PoweroffFailed {
                        instance: id.clone(),
                        error: String::new(),
                    },
                    &rows,
                    0,
                );
            }
            h.apply(&Effect::Cleared(id), &rows, 7);
            assert_eq!(outcome(&h, n), STOPPED);
        }
        // `failed` only follows `requested`.
        h.apply(
            &Effect::Raised(row(8, Severity::Alarm, "poweroff")),
            &rows,
            0,
        );
        h.apply(
            &Effect::PoweroffFailed {
                instance: "alarm_1_8".into(),
                error: String::new(),
            },
            &rows,
            0,
        );
        assert_eq!(outcome(&h, 8), PENDING);
    }

    #[test]
    fn keeps_1000_alarm_and_1000_warning_rows() {
        let mut h = History::default();
        h.raise(&row(1, Severity::Alarm, "notify"));
        for n in 2..=1500 {
            h.raise(&row(n, Severity::Warning, "notify"));
        }
        for n in 1501..=2500 {
            h.raise(&row(n, Severity::Alarm, "notify"));
        }
        let count = |severity: &str| h.rows().iter().filter(|r| r.severity == severity).count();
        assert_eq!((count("alarm"), count("warning")), (1000, 1000));
        // Each group drops its oldest row: the first Alarm and the first 499 Warnings.
        assert!(h.rows().iter().all(|r| r.instance != "alarm_1_1"));
        assert!(h.rows().iter().all(|r| r.instance != "alarm_1_500"));
        assert!(h.rows().iter().any(|r| r.instance == "alarm_1_501"));
        assert!(h.rows().iter().any(|r| r.instance == "alarm_1_2500"));
    }

    fn open(dir: &PathBuf, started_us: u64) -> HistoryStore {
        HistoryStore::open(Arc::new(StateStore::new(dir)), started_us, SAVE_EVERY)
    }

    #[tokio::test]
    async fn damaged_file_loads_lkg() {
        let dir = temp("lkg");
        let first = open(&dir, 1);
        first.apply(&[Effect::Raised(row(1, Severity::Alarm, "notify"))], &[], 0);
        first.save_now();
        // A second open writes the copy, then the file breaks.
        drop(open(&dir, 2));
        std::fs::write(dir.join(HISTORY_FILE), b"{ not json").unwrap();

        let loaded = open(&dir, 3);

        let rows = loaded.query(0, 0);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].instance, "alarm_1_1");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn damaged_file_and_lkg_load_empty() {
        let dir = temp("empty");
        std::fs::write(dir.join(HISTORY_FILE), b"{ not json").unwrap();
        std::fs::write(dir.join(format!("{HISTORY_FILE}.lkg")), b"also not json").unwrap();

        let loaded = open(&dir, 1);

        assert!(loaded.query(0, 0).is_empty());
        // The history still saves, and the next start reads it.
        loaded.apply(&[Effect::Raised(row(1, Severity::Alarm, "notify"))], &[], 0);
        loaded.save_now();
        assert_eq!(open(&dir, 2).query(0, 0).len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn since_includes_rows_active_in_range() {
        let mut h = History::default();
        for n in 1..=4 {
            h.raise(&row(n, Severity::Warning, "notify"));
        }
        // Row 1 cleared at 2000, row 2 at 3000, row 3 stays active, row 4 cleared at 5000.
        h.clear("alarm_1_1", 2000);
        h.clear("alarm_1_2", 3000);
        h.clear("alarm_1_4", 5000);

        let ids =
            |since| -> Vec<String> { h.query(since, 0).into_iter().map(|r| r.instance).collect() };
        assert_eq!(ids(0), ["alarm_1_4", "alarm_1_3", "alarm_1_2", "alarm_1_1"]);
        // A row that cleared at `since` is in range, one before it is not.
        assert_eq!(ids(3000), ["alarm_1_4", "alarm_1_3", "alarm_1_2"]);
        assert_eq!(ids(3001), ["alarm_1_4", "alarm_1_3"]);
        assert_eq!(ids(9000), ["alarm_1_3"]);
    }

    #[test]
    fn limit_zero_returns_all() {
        let mut h = History::default();
        for n in 1..=5 {
            h.raise(&row(n, Severity::Warning, "notify"));
        }
        assert_eq!(h.query(0, 0).len(), 5);
        assert_eq!(h.query(0, 2).len(), 2);
        // The newest raise comes first.
        assert_eq!(h.query(0, 2)[0].instance, "alarm_1_5");
        assert_eq!(h.query(0, 99).len(), 5);
    }
}
