//! The pump stop rule of one Loop (design D7).

use std::time::Duration;

/// A Pump channel reads 0 rpm this long, with a Duty above zero, before the Alarm rises.
pub const STOP_AFTER: Duration = Duration::from_secs(5);
/// Every Pump channel reads more than 0 rpm this long before the Alarm closes.
pub const RUN_AFTER: Duration = Duration::from_secs(10);

/// One Pump channel in one tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PumpReading {
    /// The tachometer. `None` when the value is missing or stale.
    pub rpm: Option<f64>,
    /// The Duty is above zero. For a Channel on its Hardware curve this is the `pwmN` value.
    pub duty_positive: bool,
}

#[derive(Debug, Default)]
pub struct PumpStop {
    stopped_since: Option<Duration>,
    running_since: Option<Duration>,
    active: bool,
}

impl PumpStop {
    /// Feeds one tick. Returns true while the Alarm is active.
    /// A missing tachometer value is neither a stop nor a run: it resets both timers.
    pub fn update(&mut self, now: Duration, pumps: &[PumpReading]) -> bool {
        let stopping = pumps
            .iter()
            .any(|p| p.duty_positive && p.rpm.is_some_and(|r| r <= 0.0));
        let running = !pumps.is_empty() && pumps.iter().all(|p| p.rpm.is_some_and(|r| r > 0.0));
        if self.active {
            if running {
                let since = *self.running_since.get_or_insert(now);
                if now.saturating_sub(since) >= RUN_AFTER {
                    *self = Self::default();
                }
            } else {
                self.running_since = None;
            }
        } else if stopping {
            let since = *self.stopped_since.get_or_insert(now);
            if now.saturating_sub(since) >= STOP_AFTER {
                self.active = true;
                self.stopped_since = None;
                self.running_since = None;
            }
        } else {
            self.stopped_since = None;
        }
        self.active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    fn pump(rpm: f64, duty_positive: bool) -> PumpReading {
        PumpReading {
            rpm: Some(rpm),
            duty_positive,
        }
    }

    /// Feeds `readings` once a second from t = 0 and returns the first tick that is active.
    fn first_active(rule: &mut PumpStop, ticks: u64, readings: &[PumpReading]) -> Option<u64> {
        (0..ticks).find(|t| rule.update(SECOND * (*t as u32), readings))
    }

    #[test]
    fn pump_stop_after_5s() {
        let mut rule = PumpStop::default();
        assert_eq!(first_active(&mut rule, 20, &[pump(0.0, true)]), Some(5));
        // A run that is shorter than 5 s raises nothing.
        let mut rule = PumpStop::default();
        for t in 0..4 {
            assert!(!rule.update(SECOND * t, &[pump(0.0, true)]));
        }
        assert!(!rule.update(SECOND * 4, &[pump(3000.0, true)]));
        for t in 5..9 {
            assert!(!rule.update(SECOND * t, &[pump(0.0, true)]));
        }
    }

    #[test]
    fn pump_off_by_choice_no_alarm() {
        let mut rule = PumpStop::default();
        assert_eq!(first_active(&mut rule, 60, &[pump(0.0, false)]), None);
    }

    #[test]
    fn two_pump_channels_one_stopped() {
        let mut rule = PumpStop::default();
        let both = [pump(4800.0, true), pump(0.0, true)];
        assert_eq!(first_active(&mut rule, 20, &both), Some(5));
    }

    #[test]
    fn pump_on_hardware_curve_uses_current_pwm() {
        // The caller passes `pwmN > 0` as the Duty of a Channel on its Hardware curve.
        let mut rule = PumpStop::default();
        assert_eq!(first_active(&mut rule, 20, &[pump(0.0, true)]), Some(5));
        let mut rule = PumpStop::default();
        assert_eq!(first_active(&mut rule, 20, &[pump(0.0, false)]), None);
    }

    #[test]
    fn closes_after_10s_of_running_pumps() {
        let mut rule = PumpStop::default();
        for t in 0..=5 {
            rule.update(SECOND * t, &[pump(0.0, true)]);
        }
        let running = [pump(4000.0, true), pump(4000.0, true)];
        for t in 6..16 {
            assert!(rule.update(SECOND * t, &running), "tick {t}");
        }
        assert!(!rule.update(SECOND * 16, &running));
        // A stop inside the 10 s restarts them.
        let mut rule = PumpStop::default();
        for t in 0..=5 {
            rule.update(SECOND * t, &[pump(0.0, true)]);
        }
        for t in 6..12 {
            rule.update(SECOND * t, &running);
        }
        assert!(rule.update(SECOND * 12, &[pump(4000.0, true), pump(0.0, true)]));
        for t in 13..23 {
            assert!(rule.update(SECOND * t, &running), "tick {t}");
        }
        assert!(!rule.update(SECOND * 23, &running));
    }

    #[test]
    fn missing_tachometer_does_not_close() {
        let mut rule = PumpStop::default();
        for t in 0..=5 {
            rule.update(SECOND * t, &[pump(0.0, true)]);
        }
        let missing = [PumpReading {
            rpm: None,
            duty_positive: true,
        }];
        for t in 6..30 {
            assert!(rule.update(SECOND * t, &missing));
        }
    }
}
