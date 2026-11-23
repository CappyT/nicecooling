//! The Warnings and Alarms that report 1 encodes in `alarm_bits` (`docs/protocol/leakshield.md`,
//! section Warnings and Alarms). The bits of one Device response share one event.

use super::Severity;

/// One event of the daemon and the bits of `alarm_bits` that raise it.
#[derive(Clone, Copy, Debug)]
pub struct Group {
    pub id: &'static str,
    pub severity: Severity,
    pub bits: u32,
    /// The field of report 3 that holds the Device response of the event. The pump idle time
    /// Warning has none.
    pub response_field: Option<&'static str>,
}

/// The events of the Leakshield. The Warnings and the fill level Alarm have one bit each. The
/// pump idle time Warning (bit 3) has no Device response, so it keeps its own event. The leak
/// Alarms and the system Alarm are groups, as the Alarms page of aquasuite shows them.
pub const GROUPS: [Group; 7] = [
    Group {
        id: "leakshield.fill-level-low-warning",
        severity: Severity::Warning,
        bits: 1 << 0,
        response_field: Some("fill_level_warning_response_action"),
    },
    Group {
        id: "leakshield.fill-level-low-alarm",
        severity: Severity::Alarm,
        bits: 1 << 1,
        response_field: Some("fill_level_alarm_response_action"),
    },
    Group {
        id: "leakshield.leak-warning",
        severity: Severity::Warning,
        bits: 1 << 2,
        response_field: Some("leak_rate_warning_response_action"),
    },
    Group {
        id: "leakshield.pump-idle-time-warning",
        severity: Severity::Warning,
        bits: 1 << 3,
        response_field: None,
    },
    Group {
        id: "leakshield.leak-alarm-small",
        severity: Severity::Alarm,
        bits: 1 << 4,
        response_field: Some("leak_rate_alarm_1_response_action"),
    },
    Group {
        id: "leakshield.leak-alarm",
        severity: Severity::Alarm,
        bits: (1 << 5) | (1 << 6),
        response_field: Some("leak_rate_alarm_2_response_action"),
    },
    Group {
        id: "leakshield.system-alarm",
        severity: Severity::Alarm,
        bits: 0b1111 << 7,
        response_field: Some("system_alarm_response_action"),
    },
];

/// The events that have a bit set, each with the set bits of its group, as in `alarm_bits`. A
/// bit that the document does not name is ignored.
pub fn decode(alarm_bits: u32) -> impl Iterator<Item = (&'static str, Severity, u32)> {
    GROUPS.into_iter().filter_map(move |group| {
        let set = alarm_bits & group.bits;
        (set != 0).then_some((group.id, group.severity, set))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The group of each bit, from `docs/protocol/leakshield.md`.
    const BIT_EVENTS: [&str; 11] = [
        "fill-level-low-warning",
        "fill-level-low-alarm",
        "leak-warning",
        "pump-idle-time-warning",
        "leak-alarm-small",
        "leak-alarm",
        "leak-alarm",
        "system-alarm",
        "system-alarm",
        "system-alarm",
        "system-alarm",
    ];

    #[test]
    fn every_bit_raises_its_group() {
        for (bit, name) in BIT_EVENTS.into_iter().enumerate() {
            let events: Vec<_> = decode(1 << bit).collect();
            assert_eq!(events.len(), 1, "bit {bit}");
            assert_eq!(events[0].0, format!("leakshield.{name}"), "bit {bit}");
            assert_eq!(events[0].2, 1 << bit, "bit {bit}");
            // The code names bits 0, 2 and 3 "warning" and the others "alarm".
            let severity = if [0, 2, 3].contains(&bit) {
                Severity::Warning
            } else {
                Severity::Alarm
            };
            assert_eq!(events[0].1, severity, "bit {bit}");
        }
    }

    #[test]
    fn the_bits_of_one_group_raise_one_event() {
        let events: Vec<_> = decode((1 << 5) | (1 << 6)).collect();
        assert_eq!(
            events,
            [("leakshield.leak-alarm", Severity::Alarm, 0b110_0000)]
        );
        let events: Vec<_> = decode(0b111 << 7).collect();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].2, 0b111 << 7);
    }

    #[test]
    fn several_groups_and_unnamed_bits() {
        assert_eq!(decode(0).count(), 0);
        assert_eq!(decode(0b101).count(), 2);
        assert_eq!(decode(1 << 11).count(), 0);
        assert_eq!(decode(u32::MAX).count(), GROUPS.len());
        // A bit that no group names stays out of the details.
        let all: Vec<_> = decode(u32::MAX).collect();
        assert!(all.iter().all(|e| e.2 < 1 << 11));
    }
}
