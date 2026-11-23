//! The device settings that nicecooling proposes for the alarms of a Leakshield, and the warnings
//! when a device setting overlaps with an alarm response of the daemon (ADR-0007). The functions
//! are pure: the daemon never writes a proposal on its own.

use super::Severity;
use super::leakshield::GROUPS;

/// The Device response of an event: what the Leakshield does by itself (`*_response_action` in
/// report 3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeviceResponse {
    None,
    SignalShort,
    SignalLong,
    KeySequence,
}

impl DeviceResponse {
    /// The value of the field: 0 none, 1 power switch short, 2 power switch long, 3 USB key
    /// sequence.
    pub fn from_code(code: f64) -> Option<Self> {
        match code {
            0.0 => Some(Self::None),
            1.0 => Some(Self::SignalShort),
            2.0 => Some(Self::SignalLong),
            3.0 => Some(Self::KeySequence),
            _ => None,
        }
    }

    /// The name that the API and the app use.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::SignalShort => "signal-short",
            Self::SignalLong => "signal-long",
            Self::KeySequence => "key-sequence",
        }
    }

    fn is_signal(self) -> bool {
        matches!(self, Self::SignalShort | Self::SignalLong)
    }
}

/// A device setting that overlaps with an alarm response of the daemon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Overlap {
    /// The Leakshield types a key sequence into the focused window.
    KeySequenceOn,
    /// The signal output cuts the power, and the alarm response of the daemon powers off too.
    DoublePoweroff,
}

impl Overlap {
    /// The identifier of `GetDeviceResponseProposals`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::KeySequenceOn => "key-sequence-on",
            Self::DoublePoweroff => "double-poweroff",
        }
    }
}

/// One row of `GetDeviceResponseProposals`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Proposal {
    /// The event, with the prefix `leakshield.`.
    pub event: &'static str,
    pub current: DeviceResponse,
    pub proposed: DeviceResponse,
    pub overlap: Option<Overlap>,
}

/// The proposals for every event that has a Device response. `current` gives the Device
/// response that report 3 holds in a field, `None` when the daemon cannot read it: that event
/// has no row. `poweroff` tells whether the alarm response of an event is poweroff.
///
/// The key sequence goes off on every event, and the signal output stays a long press on the
/// system Alarm. A setting that the user chose and that overlaps with nothing stays as it is.
pub fn propose(
    current: impl Fn(&str) -> Option<DeviceResponse>,
    poweroff: impl Fn(&str) -> bool,
) -> Vec<Proposal> {
    GROUPS
        .into_iter()
        .filter_map(|group| {
            let field = group.response_field?;
            let now = current(field)?;
            let system = group.id == "leakshield.system-alarm";
            let overlap = if now == DeviceResponse::KeySequence {
                Some(Overlap::KeySequenceOn)
            } else if now.is_signal() && group.severity == Severity::Alarm && poweroff(group.id) {
                Some(Overlap::DoublePoweroff)
            } else {
                None
            };
            let proposed = match overlap {
                Some(Overlap::DoublePoweroff) => DeviceResponse::None,
                _ if system => DeviceResponse::SignalLong,
                Some(Overlap::KeySequenceOn) => DeviceResponse::None,
                None => now,
            };
            Some(Proposal {
                event: group.id,
                current: now,
                proposed,
                overlap,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use DeviceResponse::*;

    const SYSTEM: &str = "leakshield.system-alarm";

    /// Every event has the Device response `all`, except the fields in `except`.
    fn run(
        all: DeviceResponse,
        except: &[(&str, DeviceResponse)],
        poweroff: &[&str],
    ) -> Vec<Proposal> {
        propose(
            |field| {
                Some(
                    except
                        .iter()
                        .find(|(f, _)| *f == field)
                        .map_or(all, |(_, r)| *r),
                )
            },
            |event| poweroff.contains(&event),
        )
    }

    fn of<'a>(list: &'a [Proposal], event: &str) -> &'a Proposal {
        list.iter()
            .find(|p| p.event == format!("leakshield.{event}") || p.event == event)
            .unwrap()
    }

    #[test]
    fn proposes_key_sequence_off() {
        let list = run(KeySequence, &[], &[]);
        // Six events have a Device response.
        assert_eq!(list.len(), 6);
        for proposal in list.iter().filter(|p| p.event != SYSTEM) {
            assert_eq!(proposal.proposed, None, "{}", proposal.event);
        }
        // The system Alarm leaves the key sequence for the long press.
        assert_eq!(of(&list, SYSTEM).proposed, SignalLong);
        // A setting that is not the key sequence stays.
        let list = run(
            None,
            &[("leak_rate_warning_response_action", SignalShort)],
            &[],
        );
        assert_eq!(of(&list, "leak-warning").proposed, SignalShort);
        assert_eq!(of(&list, "leak-alarm").proposed, None);
    }

    #[test]
    fn proposes_long_press_on_system_alarm_only() {
        for current in [None, SignalShort, SignalLong, KeySequence] {
            let list = run(current, &[], &[]);
            let long: Vec<_> = list
                .iter()
                .filter(|p| p.proposed == SignalLong && p.current != SignalLong)
                .map(|p| p.event)
                .collect();
            let expected: &[&str] = if current == SignalLong {
                &[]
            } else {
                &[SYSTEM]
            };
            assert_eq!(long, expected, "{current:?}");
        }
        // A long press on a leak Alarm that no poweroff overlaps is the user's choice.
        let list = run(
            None,
            &[("leak_rate_alarm_1_response_action", SignalLong)],
            &[],
        );
        let small = of(&list, "leak-alarm-small");
        assert_eq!((small.proposed, small.overlap), (SignalLong, Option::None));
    }

    #[test]
    fn warns_key_sequence_on() {
        let list = run(
            None,
            &[("leak_rate_alarm_1_response_action", KeySequence)],
            &[],
        );
        for proposal in &list {
            let expected =
                (proposal.event == "leakshield.leak-alarm-small").then_some(Overlap::KeySequenceOn);
            assert_eq!(proposal.overlap, expected, "{}", proposal.event);
        }
        assert_eq!(Overlap::KeySequenceOn.as_str(), "key-sequence-on");
    }

    #[test]
    fn warns_double_poweroff() {
        let large = "leakshield.leak-alarm";
        // A signal on the large leak Alarm, and an alarm response poweroff: the two overlap.
        for signal in [SignalShort, SignalLong] {
            let list = run(
                None,
                &[("leak_rate_alarm_2_response_action", signal)],
                &[large],
            );
            let p = of(&list, "leak-alarm");
            assert_eq!(p.overlap, Some(Overlap::DoublePoweroff));
            assert_eq!(p.proposed, None);
        }
        // No poweroff, no signal, or another event: no warning.
        let signal = [("leak_rate_alarm_2_response_action", SignalLong)];
        assert!(run(None, &signal, &[]).iter().all(|p| p.overlap.is_none()));
        assert!(run(None, &[], &[large]).iter().all(|p| p.overlap.is_none()));
        let list = run(None, &signal, &["leakshield.leak-alarm-small"]);
        assert!(list.iter().all(|p| p.overlap.is_none()));
        // The system Alarm with a signal and poweroff overlaps as well.
        let system = [("system_alarm_response_action", SignalLong)];
        let list = run(None, &system, &[SYSTEM]);
        assert_eq!(of(&list, SYSTEM).overlap, Some(Overlap::DoublePoweroff));
        assert_eq!(Overlap::DoublePoweroff.as_str(), "double-poweroff");
    }

    #[test]
    fn no_double_poweroff_on_a_warning() {
        let warning = "leakshield.leak-warning";
        let signal = [("leak_rate_warning_response_action", SignalLong)];
        let list = run(None, &signal, &[warning]);
        let p = of(&list, "leak-warning");
        assert_eq!((p.overlap, p.proposed), (Option::None, SignalLong));
    }

    #[test]
    fn an_unreadable_field_has_no_row() {
        let list = propose(|_| Option::None, |_| false);
        assert!(list.is_empty());
        assert_eq!(DeviceResponse::from_code(4.0), Option::None);
        assert_eq!(DeviceResponse::from_code(2.0), Some(SignalLong));
    }
}
