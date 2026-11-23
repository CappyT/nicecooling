use std::time::Duration;

use serde::{Deserialize, Serialize};

/// A timer: the rule holds from `delay_s` to `delay_s + duration_s` after the start of each
/// period of `interval_s`. A window that ends after the period is cut at the period end.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IntervalRule {
    pub interval_s: u32,
    pub delay_s: u32,
    pub duration_s: u32,
}

impl IntervalRule {
    pub fn validate(&self) -> Result<(), String> {
        if self.interval_s == 0 || self.duration_s == 0 {
            return Err("interval_s and duration_s are at least 1".into());
        }
        Ok(())
    }
}

/// The start of the first period: the start of the daemon, or the save of the rule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IntervalState {
    start: Duration,
}

impl IntervalState {
    pub fn new(start: Duration) -> Self {
        Self { start }
    }

    pub fn holds(&self, rule: &IntervalRule, now: Duration) -> bool {
        if rule.interval_s == 0 {
            return false;
        }
        let elapsed = now.saturating_sub(self.start).as_secs();
        let position = elapsed % u64::from(rule.interval_s);
        let window_end =
            (u64::from(rule.delay_s) + u64::from(rule.duration_s)).min(u64::from(rule.interval_s));
        (u64::from(rule.delay_s)..window_end).contains(&position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    #[test]
    fn interval_holds_from_delay_for_duration() {
        let rule = IntervalRule {
            interval_s: 600,
            delay_s: 60,
            duration_s: 120,
        };
        let state = IntervalState::new(secs(0));
        assert!(!state.holds(&rule, secs(59)));
        assert!(state.holds(&rule, secs(60)));
        assert!(state.holds(&rule, secs(179)));
        assert!(!state.holds(&rule, secs(180)));
        assert!(!state.holds(&rule, secs(599)));
        // The first period starts at the start time that the state holds.
        let late = IntervalState::new(secs(1000));
        assert!(!late.holds(&rule, secs(1059)));
        assert!(late.holds(&rule, secs(1060)));
    }

    #[test]
    fn interval_window_cut_at_period_end() {
        let rule = IntervalRule {
            interval_s: 100,
            delay_s: 80,
            duration_s: 50,
        };
        let state = IntervalState::new(secs(0));
        assert!(state.holds(&rule, secs(80)));
        assert!(state.holds(&rule, secs(99)));
        // The next period starts: the window does not run on.
        assert!(!state.holds(&rule, secs(100)));
        assert!(!state.holds(&rule, secs(120)));
        assert!(state.holds(&rule, secs(180)));
    }

    #[test]
    fn interval_period_restarts() {
        let rule = IntervalRule {
            interval_s: 10,
            delay_s: 2,
            duration_s: 3,
        };
        let state = IntervalState::new(secs(0));
        let pattern: Vec<bool> = (0..30).map(|t| state.holds(&rule, secs(t))).collect();
        let period: Vec<bool> = (0..10).map(|t| (2..5).contains(&t)).collect();
        assert_eq!(pattern[0..10], period[..]);
        assert_eq!(pattern[10..20], period[..]);
        assert_eq!(pattern[20..30], period[..]);
        assert!(rule.validate().is_ok());
        assert!(
            IntervalRule {
                interval_s: 0,
                ..rule.clone()
            }
            .validate()
            .is_err()
        );
        assert!(
            IntervalRule {
                duration_s: 0,
                ..rule
            }
            .validate()
            .is_err()
        );
    }
}
