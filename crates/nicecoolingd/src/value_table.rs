use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling_core::alarms::LeakshieldStatus;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_core::tick::{Reading, ValueSource};
use tokio::sync::Notify;

/// The latest timestamped value of each sensor, and what the sources know of each Leakshield.
/// Source tasks publish, the tick reads. The locks are held only to copy values, never during
/// device I/O.
#[derive(Clone, Default)]
pub struct ValueTable {
    values: Arc<Mutex<BTreeMap<String, Reading>>>,
    /// The sensors whose value the app sends. No source reads them again after a pause.
    from_app: Arc<Mutex<BTreeSet<String>>>,
    published: Arc<Notify>,
    leakshields: Arc<Mutex<BTreeMap<String, LeakshieldStatus>>>,
    processes: Arc<Mutex<ProcessList>>,
}

impl ValueTable {
    pub fn publish(&self, sensor: &str, value: f64, at: Duration) {
        self.values
            .lock()
            .unwrap()
            .insert(sensor.to_owned(), Reading { value, at });
        self.published.notify_waiters();
    }

    /// Like `publish`, for a value that the app sends (a Graph sensor).
    pub fn publish_from_app(&self, sensor: &str, value: f64, at: Duration) {
        self.from_app.lock().unwrap().insert(sensor.to_owned());
        self.publish(sensor, value, at);
    }

    /// Removes the value of a sensor: the next tick reads no value.
    pub fn clear(&self, sensor: &str) {
        self.values.lock().unwrap().remove(sensor);
        self.from_app.lock().unwrap().remove(sensor);
    }

    /// Waits until a source has read again each sensor of `sensors` that a source reads: the
    /// table holds a value that is stamped at or after `since`. A sensor that does not read stops
    /// the wait only at `limit`. A value of the app and a sensor that the table does not hold
    /// do not count: nothing reads them again.
    pub async fn wait_for_reads<'a>(
        &self,
        sensors: impl IntoIterator<Item = &'a String>,
        since: Duration,
        limit: Duration,
    ) {
        let sensors: Vec<&String> = sensors.into_iter().collect();
        let read = || {
            let values = self.values.lock().unwrap();
            let from_app = self.from_app.lock().unwrap();
            sensors.iter().all(|id| {
                from_app.contains(*id) || values.get(*id).is_none_or(|reading| reading.at >= since)
            })
        };
        let wait = async {
            loop {
                let published = self.published.notified();
                tokio::pin!(published);
                // Before the check: a publish between the check and the wait is not lost.
                published.as_mut().enable();
                if read() {
                    return;
                }
                published.await;
            }
        };
        let _ = tokio::time::timeout(limit, wait).await;
    }

    /// The result of a scan of the process list.
    pub fn publish_processes(&self, list: ProcessList) {
        *self.processes.lock().unwrap() = list;
    }

    /// A report 1 of a Leakshield arrived.
    pub fn publish_report1(
        &self,
        leakshield: &str,
        alarm_bits: u32,
        alarm_bits_latched: u32,
        at: Duration,
    ) {
        let mut map = self.leakshields.lock().unwrap();
        let status = map.entry(leakshield.to_owned()).or_default();
        status.alarm_bits = alarm_bits;
        status.alarm_bits_latched = alarm_bits_latched;
        status.report1_at = Some(at);
    }

    /// The result of one Feed message. A run of failures counts from its first failure.
    pub fn publish_feed(&self, leakshield: &str, accepted: bool, at: Duration) {
        let mut map = self.leakshields.lock().unwrap();
        let status = map.entry(leakshield.to_owned()).or_default();
        if accepted {
            status.feed_failing_since = None;
        } else {
            status.feed_failing_since.get_or_insert(at);
        }
    }
}

impl ValueTable {
    /// The Feed of a Leakshield is off: a failing run ends with it.
    pub fn feed_off(&self, leakshield: &str) {
        if let Some(status) = self.leakshields.lock().unwrap().get_mut(leakshield) {
            status.feed_failing_since = None;
        }
    }
}

impl ValueSource for ValueTable {
    fn snapshot(&self) -> BTreeMap<String, Reading> {
        self.values.lock().unwrap().clone()
    }

    fn leakshields(&self) -> BTreeMap<String, LeakshieldStatus> {
        self.leakshields.lock().unwrap().clone()
    }

    fn processes(&self) -> ProcessList {
        self.processes.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    #[tokio::test(start_paused = true)]
    async fn wait_for_reads_ends_with_the_read_of_each_source_sensor() {
        let table = ValueTable::default();
        table.publish("a", 1.0, Duration::ZERO);
        table.publish("b", 1.0, Duration::ZERO);
        table.publish_from_app("graph_1", 1.0, Duration::ZERO);
        let ids = ["a", "b", "graph_1", "derived_1"].map(String::from);
        let since = 10 * SECOND;
        let start = tokio::time::Instant::now();
        let reader = table.clone();
        tokio::spawn(async move {
            tokio::time::sleep(SECOND).await;
            reader.publish("a", 2.0, since);
            tokio::time::sleep(SECOND).await;
            reader.publish("b", 2.0, since);
        });
        table.wait_for_reads(&ids, since, 5 * SECOND).await;
        assert_eq!(start.elapsed(), 2 * SECOND);
    }

    #[tokio::test(start_paused = true)]
    async fn wait_for_reads_gives_up_at_the_limit() {
        let table = ValueTable::default();
        table.publish("a", 1.0, Duration::ZERO);
        let start = tokio::time::Instant::now();
        table
            .wait_for_reads(&["a".to_owned()], SECOND, 2 * SECOND)
            .await;
        assert_eq!(start.elapsed(), 2 * SECOND);
    }
}
