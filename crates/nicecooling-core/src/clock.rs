use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

/// The monotonic clock of the operating system (`CLOCK_MONOTONIC`). It starts at 0 when the
/// clock is made. It does not count the time of a suspend.
#[derive(Clone, Copy, Debug)]
pub struct SystemClock {
    start: Instant,
}

impl SystemClock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for SystemClock {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }
}

/// The wall clock: microseconds since the Unix epoch. It can jump when the user or the network
/// sets the time, so only a value that must survive a reboot uses it.
pub trait WallClock: Send + Sync {
    fn now_us(&self) -> u64;
}

/// The wall clock of the operating system.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemWallClock;

impl WallClock for SystemWallClock {
    fn now_us(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_micros()).unwrap_or(u64::MAX))
    }
}

/// A wall clock that tests set and advance by hand. Clones share the same time.
#[derive(Clone, Debug, Default)]
pub struct SimWallClock(Arc<Mutex<u64>>);

impl SimWallClock {
    pub fn new(start_us: u64) -> Self {
        Self(Arc::new(Mutex::new(start_us)))
    }

    pub fn advance(&self, by: Duration) {
        let by = u64::try_from(by.as_micros()).unwrap_or(u64::MAX);
        let mut now = self.0.lock().unwrap();
        *now = now.saturating_add(by);
    }

    pub fn set_us(&self, now_us: u64) {
        *self.0.lock().unwrap() = now_us;
    }
}

impl WallClock for SimWallClock {
    fn now_us(&self) -> u64 {
        *self.0.lock().unwrap()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sim_clock_clones_share_the_time() {
        let clock = SimClock::default();
        let other = clock.clone();
        clock.advance(Duration::from_secs(3));
        assert_eq!(other.now(), Duration::from_secs(3));
    }

    #[test]
    fn system_clock_never_goes_back() {
        let clock = SystemClock::new();
        let mut last = clock.now();
        for _ in 0..10_000 {
            let now = clock.now();
            assert!(now >= last);
            last = now;
        }
    }

    #[test]
    fn system_clock_advances_with_sleep() {
        let clock = SystemClock::new();
        let before = clock.now();
        std::thread::sleep(Duration::from_millis(30));
        let passed = clock.now() - before;
        assert!(passed >= Duration::from_millis(30), "{passed:?}");
        assert!(passed < Duration::from_secs(5), "{passed:?}");
    }

    #[test]
    fn sim_wall_clock_advances() {
        let clock = SimWallClock::new(1_766_000_000_000_000);
        let other = clock.clone();
        assert_eq!(clock.now_us(), 1_766_000_000_000_000);
        clock.advance(Duration::from_millis(1500));
        assert_eq!(other.now_us(), 1_766_000_001_500_000);
        other.set_us(5);
        assert_eq!(clock.now_us(), 5);
    }

    #[test]
    fn system_wall_clock_is_microseconds() {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let us = SystemWallClock.now_us();
        // The same instant in seconds, give or take the time that the test takes.
        assert!(
            (us / 1_000_000).abs_diff(seconds) <= 2,
            "{us} us against {seconds} s"
        );
        assert!(us > 1_700_000_000_000_000);
    }
}
