use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A monotonic clock. The value is the time since an arbitrary start.
pub trait Clock: Send + Sync {
    fn now(&self) -> Duration;
}

/// A clock that tests advance by hand. Clones share the same time.
#[derive(Clone, Debug, Default)]
pub struct SimClock(Arc<Mutex<Duration>>);

impl SimClock {
    pub fn advance(&self, by: Duration) {
        *self.0.lock().unwrap() += by;
    }
}

impl Clock for SimClock {
    fn now(&self) -> Duration {
        *self.0.lock().unwrap()
    }
}
