//! The reconnect wait: 1 s, doubled after each failure up to 60 s, and back to 1 s after a
//! connection that lasted 30 s or more. The type has no clock: the caller passes the time.

use std::time::Duration;

pub const FIRST_WAIT: Duration = Duration::from_secs(1);
pub const MAX_WAIT: Duration = Duration::from_secs(60);
pub const STABLE_CONNECTION: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backoff {
    next: Duration,
}

impl Default for Backoff {
    fn default() -> Self {
        Self { next: FIRST_WAIT }
    }
}

impl Backoff {
    pub fn new() -> Self {
        Self::default()
    }

    /// The wait before the next attempt. The following wait is twice as long, up to 60 s.
    pub fn next_wait(&mut self) -> Duration {
        let wait = self.next;
        self.next = (wait * 2).min(MAX_WAIT);
        wait
    }

    /// A connection ended after `lasted`. A connection of 30 s or more resets the wait.
    /// An attempt that never connected passes `Duration::ZERO`.
    pub fn connection_ended(&mut self, lasted: Duration) {
        if lasted >= STABLE_CONNECTION {
            self.next = FIRST_WAIT;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(seconds: u64) -> Duration {
        Duration::from_secs(seconds)
    }

    #[test]
    fn waits_are_1_2_4_8_16() {
        let mut backoff = Backoff::new();
        let waits: Vec<Duration> = (0..5)
            .map(|_| {
                backoff.connection_ended(Duration::ZERO);
                backoff.next_wait()
            })
            .collect();
        assert_eq!(waits, [secs(1), secs(2), secs(4), secs(8), secs(16)]);
    }

    #[test]
    fn wait_is_capped_at_60() {
        let mut backoff = Backoff::new();
        let waits: Vec<Duration> = (0..10).map(|_| backoff.next_wait()).collect();
        assert!(waits.iter().all(|wait| *wait <= secs(60)));
        assert_eq!(
            waits[5..],
            [secs(32), secs(60), secs(60), secs(60), secs(60)]
        );
    }

    #[test]
    fn wait_resets_after_a_30s_connection() {
        let mut backoff = Backoff::new();
        for _ in 0..4 {
            backoff.next_wait();
        }
        backoff.connection_ended(secs(29));
        assert_eq!(backoff.next_wait(), secs(16));
        backoff.connection_ended(secs(40));
        assert_eq!(backoff.next_wait(), secs(1));
        assert_eq!(backoff.next_wait(), secs(2));
        backoff.connection_ended(secs(30));
        assert_eq!(backoff.next_wait(), secs(1));
    }
}
