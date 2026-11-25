use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling_core::alarms::LeakshieldStatus;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_core::tick::{Reading, ValueSource};

/// The latest timestamped value of each sensor, and what the sources know of each Leakshield.
/// Source tasks publish, the tick reads. The locks are held only to copy values, never during
/// device I/O.
#[derive(Clone, Default)]
pub struct ValueTable {
    values: Arc<Mutex<BTreeMap<String, Reading>>>,
    leakshields: Arc<Mutex<BTreeMap<String, LeakshieldStatus>>>,
    processes: Arc<Mutex<ProcessList>>,
}

impl ValueTable {
    pub fn publish(&self, sensor: &str, value: f64, at: Duration) {
        self.values
            .lock()
            .unwrap()
            .insert(sensor.to_owned(), Reading { value, at });
    }

    /// Removes the value of a sensor: the next tick reads no value.
    pub fn clear(&self, sensor: &str) {
        self.values.lock().unwrap().remove(sensor);
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
