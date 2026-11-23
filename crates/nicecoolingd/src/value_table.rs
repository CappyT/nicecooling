use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling_core::tick::{Reading, ValueSource};

/// The latest timestamped value of each sensor. Source tasks publish, the tick reads.
/// The lock is held only to copy values, never during device I/O.
#[derive(Clone, Default)]
pub struct ValueTable(Arc<Mutex<BTreeMap<String, Reading>>>);

impl ValueTable {
    pub fn publish(&self, sensor: &str, value: f64, at: Duration) {
        self.0
            .lock()
            .unwrap()
            .insert(sensor.to_owned(), Reading { value, at });
    }
}

impl ValueSource for ValueTable {
    fn snapshot(&self) -> BTreeMap<String, Reading> {
        self.0.lock().unwrap().clone()
    }
}
