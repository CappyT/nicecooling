//! The event list of report 1: 15 entries that the device keeps. Codes 0 to 27 are the events of
//! the device, codes 100 to 110 are the Warnings and Alarms in the order of the `alarm_bits`
//! (`docs/protocol/leakshield.md`, section Event list).

use nicecooling_protocol::feed::NO_VALUE;
use nicecooling_protocol::report1::{EventEntry, Report1};

use super::clock::DEVICE_EPOCH_UNIX;

/// Event code of an empty entry.
const EMPTY: u8 = 255;

const DEVICE_EVENTS: [&str; 28] = [
    "system-start",
    "mode-shield",
    "mode-monitor",
    "mode-release",
    "action-measure-fill-level",
    "action-deaerate",
    "action-test",
    "action-refill",
    "release",
    "sensor-calibration",
    "release-run",
    "fill-level-measurement-start",
    "fill-level-measurement-error",
    "fill-level-measurement-finished",
    "deaeration-finished",
    "test-fill-error",
    "test-leak",
    "test-finished",
    "refill-finished",
    "shield-setpoint-changed",
    "shield-pressure-low",
    "shield-pressure-high",
    "shield-refill",
    "shield-pump-cool-down",
    "shield-pump-cool-down-stopped",
    "shield-pump-finished",
    "shield-pump-maximum-runtime",
    "test-leak-prepare",
];

/// The names of the bits of `alarm_bits`, which `docs/api.md` uses as event identifiers.
const ALARM_EVENTS: [&str; 11] = [
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

const FIRST_ALARM_CODE: u8 = 100;

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    /// The device time as microseconds since the Unix epoch, not corrected by the clock offset.
    pub time_us: u64,
    /// `leakshield.<name>`.
    pub id: String,
    /// The numbers of the entry. A value that the device marks as "no value" is absent.
    pub details: Vec<(&'static str, f64)>,
}

fn name(code: u8) -> Option<&'static str> {
    match code {
        0..=27 => DEVICE_EVENTS.get(usize::from(code)),
        FIRST_ALARM_CODE..=110 => ALARM_EVENTS.get(usize::from(code - FIRST_ALARM_CODE)),
        _ => None,
    }
    .copied()
}

fn decode(entry: &EventEntry) -> Event {
    let id = match name(entry.event_code) {
        Some(name) => format!("leakshield.{name}"),
        None => format!("leakshield.event-{}", entry.event_code),
    };
    let numbers = [
        ("mode", entry.mode, 1.0),
        ("procedure_state", entry.procedure_state, 1.0),
        ("pressure", entry.pressure, 0.1),
        ("pressure_leak_rate", entry.pressure_leak_rate, 1.0),
        ("volume_leak_rate", entry.volume_leak_rate, 1.0),
        ("value_1", entry.value_1, 1.0),
        ("value_2", entry.value_2, 1.0),
    ];
    let mut details = vec![("entry_id", f64::from(entry.entry_id))];
    details.extend(
        numbers
            .into_iter()
            .filter(|&(_, raw, _)| raw != NO_VALUE)
            .map(|(key, raw, scale)| (key, f64::from(raw) * scale)),
    );
    Event {
        time_us: (u64::from(entry.time) + DEVICE_EPOCH_UNIX as u64) * 1_000_000,
        id,
        details,
    }
}

/// The events of report 1, oldest first. Empty entries are left out.
pub fn decode_events(report1: &Report1) -> Vec<Event> {
    let mut events: Vec<(u32, u8, Event)> = report1
        .event_list
        .iter()
        .filter(|e| e.event_code != EMPTY)
        .map(|e| (e.time, e.entry_id, decode(e)))
        .collect();
    events.sort_by_key(|&(time, entry_id, _)| (time, entry_id));
    events.into_iter().map(|(_, _, event)| event).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_tables_follow_the_protocol_document() {
        assert_eq!(name(0), Some("system-start"));
        assert_eq!(name(27), Some("test-leak-prepare"));
        assert_eq!(name(100), Some("fill-level-low-warning"));
        assert_eq!(name(110), Some("pump-low-volume"));
        assert_eq!(name(28), None);
        assert_eq!(name(111), None);
        assert_eq!(name(EMPTY), None);
    }
}
