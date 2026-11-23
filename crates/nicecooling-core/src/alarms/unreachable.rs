//! The Alarm "Leakshield unreachable" (design D13).

use std::time::Duration;

use super::LeakshieldStatus;
use crate::tick::STALE_AFTER;

/// Report 1 is missing, or the Feed message fails, this long before the Alarm rises.
pub const RAISE_AFTER: Duration = Duration::from_secs(10);
/// Report 1 arrives and the Feed message is accepted this long in a row before the Alarm closes.
pub const CLOSE_AFTER: Duration = Duration::from_secs(10);

/// The state of one Leakshield.
#[derive(Debug, Default)]
pub struct Unreachable {
    first_seen: Option<Duration>,
    active: bool,
    good_since: Option<Duration>,
}

impl Unreachable {
    /// Feeds one tick. Returns true while the Alarm is active.
    /// A Leakshield that sent no report 1 yet counts as missing since the first tick.
    pub fn update(&mut self, now: Duration, status: &LeakshieldStatus) -> bool {
        let first = *self.first_seen.get_or_insert(now);
        let missing_since = status.report1_at.unwrap_or(first);
        let missing_for = now.saturating_sub(missing_since);
        let feed_failing = status.feed_failing_since.is_some();
        if self.active {
            if missing_for > STALE_AFTER || feed_failing {
                self.good_since = None;
            } else {
                let since = *self.good_since.get_or_insert(now);
                if now.saturating_sub(since) >= CLOSE_AFTER {
                    self.active = false;
                    self.good_since = None;
                }
            }
        } else {
            let feed_for = status
                .feed_failing_since
                .map_or(Duration::ZERO, |at| now.saturating_sub(at));
            if missing_for >= RAISE_AFTER || feed_for >= RAISE_AFTER {
                self.active = true;
                self.good_since = None;
            }
        }
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    fn status(report1_at: Option<u32>, feed_failing_since: Option<u32>) -> LeakshieldStatus {
        LeakshieldStatus {
            alarm_bits: 0,
            alarm_bits_latched: 0,
            report1_at: report1_at.map(|s| SECOND * s),
            feed_failing_since: feed_failing_since.map(|s| SECOND * s),
        }
    }

    #[test]
    fn disconnect_10s_raises() {
        let mut rule = Unreachable::default();
        // The last report 1 came at t = 0.
        for t in 0..10 {
            assert!(!rule.update(SECOND * t, &status(Some(0), None)), "t = {t}");
        }
        assert!(rule.update(SECOND * 10, &status(Some(0), None)));
        // A Leakshield that never sent a report 1 counts from the first tick.
        let mut rule = Unreachable::default();
        for t in 5..15 {
            assert!(!rule.update(SECOND * t, &status(None, None)), "t = {t}");
        }
        assert!(rule.update(SECOND * 15, &status(None, None)));
    }

    #[test]
    fn failed_feed_10s_raises() {
        let mut rule = Unreachable::default();
        // The reports keep coming. The Feed message fails from t = 3.
        for t in 0..13 {
            assert!(
                !rule.update(SECOND * t, &status(Some(t), Some(3))),
                "t = {t}"
            );
        }
        assert!(rule.update(SECOND * 13, &status(Some(13), Some(3))));
    }

    #[test]
    fn gap_4s_raises_nothing() {
        let mut rule = Unreachable::default();
        for t in 0..=5 {
            assert!(!rule.update(SECOND * t, &status(Some(t), None)));
        }
        // No report 1 from t = 6 to t = 9.
        for t in 6..10 {
            assert!(!rule.update(SECOND * t, &status(Some(5), None)), "t = {t}");
        }
        for t in 10..60 {
            assert!(!rule.update(SECOND * t, &status(Some(t), None)), "t = {t}");
        }
        // A Feed message that fails for 4 s raises nothing either.
        for t in 60..64 {
            assert!(!rule.update(SECOND * t, &status(Some(t), Some(60))));
        }
        assert!(!rule.update(SECOND * 64, &status(Some(64), None)));
    }

    #[test]
    fn closes_after_10s_of_good_reports() {
        let mut rule = Unreachable::default();
        assert!(rule.update(SECOND * 10, &status(Some(0), None)));
        // The device answers again at t = 20. The Alarm closes at t = 30.
        for t in 20..30 {
            assert!(rule.update(SECOND * t, &status(Some(t), None)), "t = {t}");
        }
        assert!(!rule.update(SECOND * 30, &status(Some(30), None)));
        // A failed Feed message inside the 10 s starts them again.
        let mut rule = Unreachable::default();
        assert!(rule.update(SECOND * 10, &status(Some(0), None)));
        for t in 20..25 {
            assert!(rule.update(SECOND * t, &status(Some(t), None)));
        }
        assert!(rule.update(SECOND * 25, &status(Some(25), Some(25))));
        for t in 26..36 {
            assert!(rule.update(SECOND * t, &status(Some(t), None)), "t = {t}");
        }
        assert!(!rule.update(SECOND * 36, &status(Some(36), None)));
    }
}
