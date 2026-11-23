use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Two values closer than this are equal.
const EQ_TOLERANCE: f64 = 1e-9;

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
        let value = self.value;
        match self.comparison {
            Comparison::Lt => reading < value,
            Comparison::Le => reading <= value,
            Comparison::Eq => (reading - value).abs() <= EQ_TOLERANCE,
            Comparison::Ge => reading >= value,
            Comparison::Gt => reading > value,
            Comparison::Ne => (reading - value).abs() > EQ_TOLERANCE,
            Comparison::InRange => {
                reading >= value && reading <= self.value_max.unwrap_or(f64::NEG_INFINITY)
            }
        }
    }
}

/// The hold time of one sensor rule.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SensorState {
    since: Option<Duration>,
}

impl SensorState {
    /// A sensor with no value makes the rule false.
    pub fn holds(&mut self, rule: &SensorRule, reading: Option<f64>, now: Duration) -> bool {
        if reading.is_none_or(|r| !rule.compares(r)) {
            self.since = None;
            return false;
        }
        let since = *self.since.get_or_insert(now);
        now.saturating_sub(since) >= Duration::from_secs(u64::from(rule.hold_s))
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
}
