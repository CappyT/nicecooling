use std::collections::BTreeMap;

use super::db::{HistoryDb, MinuteRow, SampleRow};
use super::prune::prune;
use super::{BATCH_INTERVAL_S, PRUNE_INTERVAL_S};

#[derive(Default)]
struct MinuteAcc {
    unit: String,
    sum: f64,
    min: f64,
    max: f64,
    n: i64,
}

impl MinuteAcc {
    fn add(&mut self, value: Option<f64>) {
        let Some(value) = value else { return };
        if self.n == 0 {
            self.min = value;
            self.max = value;
        } else {
            self.min = self.min.min(value);
            self.max = self.max.max(value);
        }
        self.sum += value;
        self.n += 1;
    }
}

/// Buffers samples and writes them in one transaction every 10 s. The caller gives the time,
/// so a test drives it with a simulated clock.
pub struct HistoryWriter {
    db: HistoryDb,
    pending: Vec<SampleRow>,
    minutes: BTreeMap<(String, i64), MinuteAcc>,
    last_flush: Option<i64>,
    last_prune: Option<i64>,
    minute_floor: i64,
    transactions: u64,
}

impl HistoryWriter {
    pub fn new(db: HistoryDb) -> Self {
        Self {
            db,
            pending: Vec::new(),
            minutes: BTreeMap::new(),
            last_flush: None,
            last_prune: None,
            minute_floor: i64::MIN,
            transactions: 0,
        }
    }

    pub fn db(&self) -> &HistoryDb {
        &self.db
    }

    /// The number of committed write transactions.
    pub fn transactions(&self) -> u64 {
        self.transactions
    }

    /// Queue one sample. `t` is in Unix seconds.
    pub fn record(&mut self, t: i64, sensor: &str, unit: &str, value: Option<f64>) {
        let minute = t - t.rem_euclid(60);
        if minute >= self.minute_floor {
            self.minutes
                .entry((sensor.to_owned(), minute))
                .or_insert_with(|| MinuteAcc {
                    unit: unit.to_owned(),
                    ..MinuteAcc::default()
                })
                .add(value);
        }
        self.pending.push(SampleRow {
            sensor: sensor.to_owned(),
            unit: unit.to_owned(),
            t,
            value,
        });
    }

    /// Write the batch when 10 s passed since the last one, and prune when an hour passed.
    pub fn tick(&mut self, now: i64) -> rusqlite::Result<()> {
        let flush_due = self
            .last_flush
            .is_none_or(|last| now - last >= BATCH_INTERVAL_S);
        if flush_due {
            self.flush(now)?;
        }
        let prune_due = self
            .last_prune
            .is_none_or(|last| now - last >= PRUNE_INTERVAL_S);
        if prune_due {
            prune(&mut self.db, now)?;
            self.last_prune = Some(now);
        }
        Ok(())
    }

    fn flush(&mut self, now: i64) -> rusqlite::Result<()> {
        self.last_flush = Some(now);
        let done: Vec<(String, i64)> = self
            .minutes
            .keys()
            .filter(|(_, minute)| minute + 60 <= now)
            .cloned()
            .collect();
        let minute_rows: Vec<MinuteRow> = done
            .iter()
            .map(|key| {
                let acc = &self.minutes[key];
                let has_value = acc.n > 0;
                MinuteRow {
                    sensor: key.0.clone(),
                    unit: acc.unit.clone(),
                    t: key.1,
                    mean: has_value.then(|| acc.sum / acc.n as f64),
                    min: has_value.then_some(acc.min),
                    max: has_value.then_some(acc.max),
                    n: acc.n,
                }
            })
            .collect();
        if self.pending.is_empty() && minute_rows.is_empty() {
            return Ok(());
        }
        self.db.write_batch(&self.pending, &minute_rows)?;
        self.transactions += 1;
        self.pending.clear();
        for key in done {
            self.minutes.remove(&key);
        }
        self.minute_floor = self.minute_floor.max(now - now.rem_euclid(60));
        Ok(())
    }
}
