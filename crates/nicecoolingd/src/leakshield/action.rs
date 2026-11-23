//! Actions: temporary procedures that the device starts with a code of 4 to 7.

use super::mode::Mode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    MeasureFillLevel,
    Deaerate,
    Test,
    Refill,
}

impl Action {
    pub const ALL: [Self; 4] = [
        Self::MeasureFillLevel,
        Self::Deaerate,
        Self::Test,
        Self::Refill,
    ];

    pub fn code(self) -> u32 {
        match self {
            Self::MeasureFillLevel => 4,
            Self::Deaerate => 5,
            Self::Test => 6,
            Self::Refill => 7,
        }
    }

    /// The identifiers of the `RunningAction` property and of `StartAction`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MeasureFillLevel => "measure-fill-level",
            Self::Deaerate => "deaerate",
            Self::Test => "test",
            Self::Refill => "refill",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.as_str() == text)
    }

    pub fn from_code(code: i16) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.code() as i16 == code)
    }
}

/// What report 1 `mode` says: the Mode, or the Action that runs now. While an Action runs, the
/// report holds its code, so the Mode is the last one seen.
pub fn interpret(report_mode: i16, last_mode: Option<Mode>) -> (Option<Mode>, Option<Action>) {
    match Mode::from_code(report_mode) {
        Some(mode) => (Some(mode), None),
        None => (last_mode, Action::from_code(report_mode)),
    }
}

/// A Loop as the Refill check sees it: its Leakshield and its Pump channels.
pub struct LoopPumps<'a> {
    pub leakshield: Option<&'a str>,
    pub pump_channels: &'a [String],
}

/// True while a Pump channel of a Loop of this Leakshield reads above 0 rpm or has no tach value
/// (fail closed). A Pump channel `<chip>_pwmN` reads as the sensor `<chip>_fanN`. `value_of`
/// gives the value of a sensor, or `None` when it has no value. Pumps of other Loops do not count.
pub fn refill_blocked<'a>(
    leakshield: &str,
    loops: impl IntoIterator<Item = LoopPumps<'a>>,
    value_of: impl Fn(&str) -> Option<f64>,
) -> bool {
    loops
        .into_iter()
        .filter(|l| l.leakshield == Some(leakshield))
        .flat_map(|l| l.pump_channels)
        .any(|channel| {
            super::feed::tach_sensor(channel)
                .and_then(|tach| value_of(&tach))
                .is_none_or(|rpm| rpm > 0.0)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn running_action_keeps_the_last_mode() {
        assert_eq!(interpret(2, None), (Some(Mode::Monitor), None));
        assert_eq!(
            interpret(6, Some(Mode::Monitor)),
            (Some(Mode::Monitor), Some(Action::Test))
        );
        assert_eq!(interpret(6, None), (None, Some(Action::Test)));
        assert_eq!(interpret(0, Some(Mode::Shield)), (Some(Mode::Shield), None));
    }

    #[test]
    fn action_names_and_codes() {
        let all: Vec<_> = Action::ALL.iter().map(|a| (a.as_str(), a.code())).collect();
        assert_eq!(
            all,
            [
                ("measure-fill-level", 4),
                ("deaerate", 5),
                ("test", 6),
                ("refill", 7)
            ]
        );
        assert_eq!(Action::parse("Refill"), None);
    }
}
