//! The Warnings and Alarms that report 1 encodes in `alarm_bits` (`docs/protocol/leakshield.md`,
//! section Warnings and Alarms).

use super::Severity;

/// The event identifier of each bit of `alarm_bits`, in bit order.
pub const EVENTS: [(&str, Severity); 11] = [
    ("leakshield.fill-level-low-warning", Severity::Warning),
    ("leakshield.fill-level-low-alarm", Severity::Alarm),
    ("leakshield.leak-warning", Severity::Warning),
    ("leakshield.pump-idle-time-warning", Severity::Warning),
    ("leakshield.pump-idle-time-alarm", Severity::Alarm),
    ("leakshield.pump-runtime-alarm", Severity::Alarm),
    ("leakshield.high-pressure-drop", Severity::Alarm),
    ("leakshield.supply-voltage", Severity::Alarm),
    ("leakshield.pump-stall", Severity::Alarm),
    ("leakshield.pump-overcurrent", Severity::Alarm),
    ("leakshield.pump-low-volume", Severity::Alarm),
];

/// The events of the bits that are set. A bit that the document does not name is ignored.
pub fn decode(alarm_bits: u32) -> impl Iterator<Item = (&'static str, Severity)> {
    EVENTS
        .into_iter()
        .enumerate()
        .filter(move |(bit, _)| alarm_bits & (1 << bit) != 0)
        .map(|(_, event)| event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_leakshield_alarm_decodes_to_its_identifier() {
        // The identifiers of `docs/protocol/leakshield.md`.
        let expected = [
            "fill-level-low-warning",
            "fill-level-low-alarm",
            "leak-warning",
            "pump-idle-time-warning",
            "pump-idle-time-alarm",
            "pump-runtime-alarm",
            "high-pressure-drop",
            "supply-voltage",
            "pump-stall",
            "pump-overcurrent",
            "pump-low-volume",
        ];
        for (bit, name) in expected.into_iter().enumerate() {
            let events: Vec<_> = decode(1 << bit).collect();
            assert_eq!(events.len(), 1, "bit {bit}");
            assert_eq!(events[0].0, format!("leakshield.{name}"), "bit {bit}");
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
    fn several_bits_and_unnamed_bits() {
        assert_eq!(decode(0).count(), 0);
        assert_eq!(decode(0b101).count(), 2);
        assert_eq!(decode(1 << 11).count(), 0);
        assert_eq!(decode(u32::MAX).count(), EVENTS.len());
    }
}
