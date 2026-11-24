use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Two values closer than this are equal.
pub const EQ_TOLERANCE: f64 = 1e-9;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Comparison {
    Lt,
    Le,
    Eq,
    Ge,
    Gt,
    Ne,
    InRange,
}

impl Comparison {
    pub fn parse(text: &str) -> Option<Self> {
        Some(match text {
            "lt" => Self::Lt,
            "le" => Self::Le,
            "eq" => Self::Eq,
            "ge" => Self::Ge,
            "gt" => Self::Gt,
            "ne" => Self::Ne,
            "in_range" => Self::InRange,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Lt => "lt",
            Self::Le => "le",
            Self::Eq => "eq",
            Self::Ge => "ge",
            Self::Gt => "gt",
            Self::Ne => "ne",
            Self::InRange => "in_range",
        }
    }
}

/// A sensor compared with a value. The comparison must hold for `hold_s` before the rule holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SensorRule {
    pub sensor: String,
    pub comparison: Comparison,
    /// For `in_range`, the lower bound.
    pub value: f64,
    /// The upper bound. Only for `in_range`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value_max: Option<f64>,
    pub hold_s: u32,
}

impl SensorRule {
    pub fn validate(&self) -> Result<(), String> {
        if !self.value.is_finite() {
            return Err("value is not finite".into());
        }
        match (self.comparison, self.value_max) {
            (Comparison::InRange, Some(max)) if max.is_finite() && max >= self.value => Ok(()),
            (Comparison::InRange, Some(_)) => {
                Err("value_max is finite and not less than value".into())
            }
            (Comparison::InRange, None) => Err("in_range needs value_max".into()),
            (_, Some(_)) => Err("value_max is only for in_range".into()),
            (_, None) => Ok(()),
        }
    }

    pub fn compares(&self, reading: f64) -> bool {
        compare(self.comparison, reading, self.value, self.value_max)
    }
}

/// Whether `reading` passes the comparison against `value`. `in_range` also needs `value_max`:
/// without it nothing is in range. A Profile rule and a Graph node share this function, so both
/// agree on a reading.
pub fn compare(comparison: Comparison, reading: f64, value: f64, value_max: Option<f64>) -> bool {
    match comparison {
        Comparison::Lt => reading < value,
        Comparison::Le => reading <= value,
        Comparison::Eq => (reading - value).abs() <= EQ_TOLERANCE,
        Comparison::Ge => reading >= value,
        Comparison::Gt => reading > value,
        Comparison::Ne => (reading - value).abs() > EQ_TOLERANCE,
        Comparison::InRange => {
            reading >= value && reading <= value_max.unwrap_or(f64::NEG_INFINITY)
        }
    }
}

/// A condition that must stay true for a time. A false condition and a condition with no value
/// start the time again.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hold {
    since: Option<Duration>,
}

impl Hold {
    /// `condition` is `None` when the value of the condition is not known. It returns true when
    /// the condition has held for `hold` at `now`.
    pub fn holds(&mut self, condition: Option<bool>, hold: Duration, now: Duration) -> bool {
        if condition != Some(true) {
            self.since = None;
            return false;
        }
        let since = *self.since.get_or_insert(now);
        now.saturating_sub(since) >= hold
    }

    /// Forgets the time that the condition held.
    pub fn reset(&mut self) {
        self.since = None;
    }
}

/// The hold time of one sensor rule.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SensorState {
    hold: Hold,
}

impl SensorState {
    /// A sensor with no value makes the rule false.
    pub fn holds(&mut self, rule: &SensorRule, reading: Option<f64>, now: Duration) -> bool {
        let condition = reading.map(|r| rule.compares(r));
        self.hold
            .holds(condition, Duration::from_secs(u64::from(rule.hold_s)), now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(comparison: Comparison, value: f64) -> SensorRule {
        SensorRule {
            sensor: "k10temp_temp1".into(),
            comparison,
            value,
            value_max: None,
            hold_s: 0,
        }
    }

    #[test]
    fn sensor_comparisons_lt_le_eq_ge_gt_ne() {
        let cases = [
            (Comparison::Lt, [true, false, false]),
            (Comparison::Le, [true, true, false]),
            (Comparison::Eq, [false, true, false]),
            (Comparison::Ge, [false, true, true]),
            (Comparison::Gt, [false, false, true]),
            (Comparison::Ne, [true, false, true]),
        ];
        // The readings 49, 50 and 51 against the value 50.
        for (comparison, expected) in cases {
            let rule = rule(comparison, 50.0);
            let got = [49.0, 50.0, 51.0].map(|r| rule.compares(r));
            assert_eq!(got, expected, "{comparison:?}");
        }
    }

    #[test]
    fn sensor_in_range() {
        let rule = SensorRule {
            value_max: Some(40.0),
            ..rule(Comparison::InRange, 30.0)
        };
        assert_eq!(rule.validate(), Ok(()));
        let got = [29.9, 30.0, 35.0, 40.0, 40.1].map(|r| rule.compares(r));
        assert_eq!(got, [false, true, true, true, false]);
        // The upper bound is not less than the lower bound, and only `in_range` has one.
        let backwards = SensorRule {
            value_max: Some(20.0),
            ..rule.clone()
        };
        assert!(backwards.validate().is_err());
        assert!(
            SensorRule {
                value_max: None,
                ..rule
            }
            .validate()
            .is_err()
        );
        let stray = SensorRule {
            value_max: Some(1.0),
            ..self::rule(Comparison::Gt, 0.0)
        };
        assert!(stray.validate().is_err());
    }

    #[test]
    fn sensor_hold_time_resets_when_comparison_leaves() {
        let rule = SensorRule {
            hold_s: 60,
            ..rule(Comparison::Gt, 50.0)
        };
        let mut state = SensorState::default();
        let secs = Duration::from_secs;
        // 20 s above the value: the rule does not hold yet.
        for t in 0..=20 {
            assert!(!state.holds(&rule, Some(55.0), secs(t)), "t={t}");
        }
        // The comparison leaves: the timer resets.
        assert!(!state.holds(&rule, Some(45.0), secs(21)));
        // It must hold for 60 s again from the new start.
        assert!(!state.holds(&rule, Some(55.0), secs(22)));
        assert!(!state.holds(&rule, Some(55.0), secs(81)));
        assert!(state.holds(&rule, Some(55.0), secs(82)));
        assert!(state.holds(&rule, Some(55.0), secs(83)));
    }

    #[test]
    fn sensor_without_value_does_not_hold() {
        let rule = rule(Comparison::Gt, 50.0);
        let mut state = SensorState::default();
        assert!(state.holds(&rule, Some(60.0), Duration::ZERO));
        // The sensor loses its value: the rule is false at once and the timer resets.
        assert!(!state.holds(&rule, None, Duration::from_secs(1)));
        let timed = SensorRule { hold_s: 5, ..rule };
        let mut state = SensorState::default();
        assert!(!state.holds(&timed, Some(60.0), Duration::ZERO));
        assert!(!state.holds(&timed, None, Duration::from_secs(4)));
        assert!(!state.holds(&timed, Some(60.0), Duration::from_secs(5)));
    }

    #[test]
    fn compare_equal_tolerance_1e_9() {
        assert!(compare(Comparison::Eq, 50.000_000_000_1, 50.0, None));
        assert!(compare(Comparison::Eq, 0.1 + 0.2, 0.3, None));
        assert!(!compare(Comparison::Ne, 50.000_000_000_1, 50.0, None));
        assert_eq!(EQ_TOLERANCE, 1e-9);
    }

    #[test]
    fn compare_not_equal_outside_tolerance() {
        assert!(!compare(Comparison::Eq, 5.0, 5.000_001, None));
        assert!(compare(Comparison::Ne, 5.0, 5.000_001, None));
        assert!(!compare(Comparison::Ne, 5.0, 5.0, None));
        assert!(!compare(Comparison::InRange, 5.0, 1.0, None));
        assert!(compare(Comparison::InRange, 5.0, 1.0, Some(5.0)));
    }

    #[test]
    fn hold_resets_on_false() {
        let secs = Duration::from_secs;
        let mut hold = Hold::default();
        assert!(!hold.holds(Some(true), secs(3), secs(10)));
        assert!(!hold.holds(Some(true), secs(3), secs(12)));
        assert!(!hold.holds(Some(false), secs(3), secs(13)));
        // The time starts again at second 14.
        assert!(!hold.holds(Some(true), secs(3), secs(14)));
        assert!(!hold.holds(Some(true), secs(3), secs(16)));
        assert!(hold.holds(Some(true), secs(3), secs(17)));
    }

    #[test]
    fn hold_resets_on_no_value() {
        let secs = Duration::from_secs;
        let mut hold = Hold::default();
        assert!(!hold.holds(Some(true), secs(3), secs(10)));
        assert!(!hold.holds(None, secs(3), secs(12)));
        assert!(!hold.holds(Some(true), secs(3), secs(13)));
        assert!(!hold.holds(Some(true), secs(3), secs(15)));
        assert!(hold.holds(Some(true), secs(3), secs(16)));
        hold.reset();
        assert!(!hold.holds(Some(true), secs(3), secs(17)));
    }

    #[test]
    fn hold_reaches_after_seconds() {
        let secs = Duration::from_secs;
        let mut hold = Hold::default();
        for t in 10..13 {
            assert!(!hold.holds(Some(true), secs(3), secs(t)), "t={t}");
        }
        for t in 13..20 {
            assert!(hold.holds(Some(true), secs(3), secs(t)), "t={t}");
        }
        // A hold of 0 s holds at once.
        assert!(Hold::default().holds(Some(true), Duration::ZERO, secs(1)));
    }
}
