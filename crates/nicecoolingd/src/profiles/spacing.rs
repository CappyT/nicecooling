//! The spacing of the Leakshield writes of Profile switches, as aquasuite does it (design D14).
//! A request writes at once when no write waits and the last profile write is older than 2 s.
//! Else the worker keeps only the newest request and writes it 1.5 s after the last profile
//! write. The state machine holds no request: the worker keeps it.

use std::time::Duration;

use tokio::time::Instant;

/// A request writes at once when the last profile write is older than this.
pub const MIN_GAP: Duration = Duration::from_secs(2);
/// A request that has to wait writes this long after the last profile write.
pub const DELAYED_WRITE: Duration = Duration::from_millis(1500);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    WriteNow,
    /// Keep the request and write it at this time. A newer request replaces it.
    WriteAt(Instant),
}

#[derive(Debug, Default)]
pub struct Spacing {
    last_write: Option<Instant>,
}

impl Spacing {
    /// A request arrives.
    pub fn request(&mut self, now: Instant) -> Decision {
        let recent = self
            .last_write
            .filter(|last| now.saturating_duration_since(*last) <= MIN_GAP);
        match recent {
            // A time in the past means: write at once.
            Some(last) => Decision::WriteAt(last + DELAYED_WRITE),
            None => Decision::WriteNow,
        }
    }

    /// The worker writes: the waiting request, or a request that went through at once.
    pub fn wrote(&mut self, now: Instant) {
        self.last_write = Some(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn the_gap_is_2_s_and_the_delay_is_1500_ms() {
        let start = Instant::now();
        let mut spacing = Spacing::default();
        // No write before: at once.
        assert_eq!(spacing.request(start), Decision::WriteNow);
        spacing.wrote(start);
        // Within 2 s: 1.5 s after the last write.
        assert_eq!(
            spacing.request(start + Duration::from_millis(100)),
            Decision::WriteAt(start + DELAYED_WRITE)
        );
        // A newer request keeps the same time.
        assert_eq!(
            spacing.request(start + Duration::from_millis(900)),
            Decision::WriteAt(start + DELAYED_WRITE)
        );
        spacing.wrote(start + DELAYED_WRITE);
        // 1.9 s after the write at 1.5 s.
        let third = start + DELAYED_WRITE + Duration::from_secs(2);
        assert_eq!(
            spacing.request(third),
            Decision::WriteAt(start + DELAYED_WRITE * 2)
        );
        // Older than 2 s: at once.
        let later = start + DELAYED_WRITE + Duration::from_millis(2001);
        assert_eq!(spacing.request(later), Decision::WriteNow);
    }
}
