//! The alarm response of each Warning and Alarm: none, notify, or poweroff after a delay.

use std::collections::BTreeMap;

use nicecooling_api::errors::{Error, Result};
use nicecooling_core::alarms::Severity;
use nicecooling_core::alarms::leakshield::GROUPS;

use crate::state::AlarmResponse;

/// The delay that a row shows until the user sets one.
pub const DEFAULT_DELAY_S: u32 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    None,
    Notify,
    Poweroff,
}

impl Response {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Notify => "notify",
            Self::Poweroff => "poweroff",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "none" => Some(Self::None),
            "notify" => Some(Self::Notify),
            "poweroff" => Some(Self::Poweroff),
            _ => None,
        }
    }
}

/// The events of nicecooling (`docs/api.md`, section Event identifiers).
const NICECOOLING_EVENTS: [(&str, Severity); 8] = [
    ("pump-stop", Severity::Alarm),
    ("leakshield-unreachable", Severity::Alarm),
    ("leakshield-unknown-firmware", Severity::Warning),
    ("lost-input", Severity::Warning),
    ("another-writer", Severity::Warning),
    ("manual-at-snapshot", Severity::Warning),
    ("no-bios-snapshot", Severity::Warning),
    ("state-fallback", Severity::Warning),
];

/// The events that power off by default: the pump stop and the large leak.
const POWEROFF_BY_DEFAULT: [&str; 2] = ["pump-stop", "leakshield.leak-alarm"];

/// Every event that has a row in the response table, nicecooling first. The Leakshield has one
/// row for each Warning, for the fill level Alarm and for each Alarm group, not for each bit.
pub fn events() -> impl Iterator<Item = (&'static str, Severity)> {
    NICECOOLING_EVENTS
        .into_iter()
        .chain(GROUPS.into_iter().map(|g| (g.id, g.severity)))
}

pub fn default_response(event: &str) -> (Response, u32) {
    if POWEROFF_BY_DEFAULT.contains(&event) {
        (Response::Poweroff, DEFAULT_DELAY_S)
    } else {
        (Response::Notify, DEFAULT_DELAY_S)
    }
}

/// The key of the override of `event` for the Leakshield `source` in `alarm_responses`.
pub fn override_key(event: &str, source: &str) -> String {
    format!("{event}@{source}")
}

/// An override exists only for an event of the Leakshield and the id of a Leakshield.
fn is_leakshield_event(event: &str) -> bool {
    event.starts_with("leakshield.")
}

/// The entry of `key`, with a response that the daemon knows.
fn entry(table: &BTreeMap<String, AlarmResponse>, key: &str) -> Option<(Response, u32)> {
    let entry = table.get(key)?;
    Some((Response::parse(&entry.response)?, entry.delay_s))
}

/// The response of an instance that rises for `event` and `source`, first match wins: the
/// forced `notify` of a firmware update, the override of (event, Leakshield), the default of the
/// event, the built-in default. An entry that no response name matches (a state file of another
/// version) counts as missing. `source` `""` is the default of the event.
pub fn resolve(
    table: &BTreeMap<String, AlarmResponse>,
    event: &str,
    source: &str,
    forced_notify: bool,
) -> (Response, u32) {
    let (built_in, built_in_delay) = default_response(event);
    if forced_notify {
        let delay = entry(table, event).map_or(built_in_delay, |e| e.1);
        return (Response::Notify, delay);
    }
    if !source.is_empty()
        && is_leakshield_event(event)
        && let Some(found) = entry(table, &override_key(event, source))
    {
        return found;
    }
    // A default entry with a bad response keeps its delay, as before.
    match table.get(event) {
        Some(default) => (
            Response::parse(&default.response).unwrap_or(built_in),
            default.delay_s,
        ),
        None => (built_in, built_in_delay),
    }
}

/// A row of `GetResponses`: event, source, response, delay, override.
pub type Row = (String, String, String, u32, bool);

/// The rows of `GetResponses`, in the event order of `docs/api.md`: one row for each event with
/// source `""` (override true when the default of the event differs from the built-in default,
/// in the response or in the delay), then, for each event of the Leakshield, one resolved row
/// for each of `leakshields` (override true when an override exists).
pub fn rows(table: &BTreeMap<String, AlarmResponse>, leakshields: &[String]) -> Vec<Row> {
    let defaults = events().map(|(event, _)| {
        let (response, delay) = resolve(table, event, "", false);
        (
            event.to_owned(),
            String::new(),
            response.as_str().to_owned(),
            delay,
            (response, delay) != default_response(event),
        )
    });
    let per_device = events()
        .filter(|(event, _)| is_leakshield_event(event))
        .flat_map(|(event, _)| {
            leakshields.iter().map(move |source| {
                let (response, delay) = resolve(table, event, source, false);
                let overridden = entry(table, &override_key(event, source)).is_some();
                (
                    event.to_owned(),
                    source.clone(),
                    response.as_str().to_owned(),
                    delay,
                    overridden,
                )
            })
        });
    defaults.chain(per_device).collect()
}

/// The resolved response of each event for one Leakshield: `(event, response, delay)`.
pub fn resolved_for(
    table: &BTreeMap<String, AlarmResponse>,
    leakshield: &str,
) -> Vec<(String, String, u32)> {
    events()
        .map(|(event, _)| {
            let (response, delay) = resolve(table, event, leakshield, false);
            (event.to_owned(), response.as_str().to_owned(), delay)
        })
        .collect()
}

fn check_event(event: &str) -> Result<()> {
    if events().any(|(known, _)| known == event) {
        Ok(())
    } else {
        Err(Error::not_found(format!("{event} is not an event")))
    }
}

/// Checks the arguments of `SetResponse`. `known` tells whether `source` is a known Leakshield.
/// Returns the key of `alarm_responses` and its entry.
pub fn validate(
    event: &str,
    source: &str,
    response: &str,
    delay_s: u32,
    known: impl Fn(&str) -> bool,
) -> Result<(String, AlarmResponse)> {
    check_event(event)?;
    let parsed = Response::parse(response).ok_or_else(|| {
        Error::invalid(format!(
            "the alarm response is none, notify or poweroff, not {response}"
        ))
    })?;
    let key = if source.is_empty() {
        event.to_owned()
    } else if !is_leakshield_event(event) {
        return Err(Error::invalid(format!(
            "{event} has no response for a source: only the events of a Leakshield have one"
        )));
    } else if !known(source) {
        return Err(Error::not_found(format!("{source} is not a Leakshield")));
    } else {
        override_key(event, source)
    };
    Ok((
        key,
        AlarmResponse {
            response: parsed.as_str().to_owned(),
            delay_s,
        },
    ))
}

/// Checks the arguments of `ResetResponse` and returns the key to remove. Any source is
/// accepted, also the id of a Leakshield that no longer exists.
pub fn reset_key(event: &str, source: &str) -> Result<String> {
    check_event(event)?;
    Ok(if source.is_empty() {
        event.to_owned()
    } else {
        override_key(event, source)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const LARGE: &str = "leakshield.leak-alarm";
    const SMALL: &str = "leakshield.leak-alarm-small";
    const DEVICE: &str = "leakshield_20417_31072";

    fn entry(response: &str, delay_s: u32) -> AlarmResponse {
        AlarmResponse {
            response: response.into(),
            delay_s,
        }
    }

    #[test]
    fn default_responses() {
        let table = BTreeMap::new();
        let rows = rows(&table, &[]);
        assert_eq!(rows.len(), 8 + 7);
        for (event, source, response, delay, overridden) in &rows {
            let expected = if event == "pump-stop" || event == LARGE {
                "poweroff"
            } else {
                "notify"
            };
            assert_eq!(response, expected, "{event}");
            assert_eq!((*delay, source.as_str(), *overridden), (30, "", false));
        }
        let unreachable = rows
            .iter()
            .find(|r| r.0 == "leakshield-unreachable")
            .unwrap();
        assert_eq!(unreachable.2, "notify");
    }

    #[test]
    fn default_row_is_overridden_when_it_differs_from_builtin() {
        let mut table = BTreeMap::new();
        let flags = |table: &BTreeMap<String, AlarmResponse>| {
            rows(table, &[])
                .into_iter()
                .filter(|r| r.4)
                .map(|r| r.0)
                .collect::<Vec<_>>()
        };
        assert!(flags(&table).is_empty());
        table.insert("pump-stop".to_owned(), entry("poweroff", 30));
        assert!(
            flags(&table).is_empty(),
            "the built-in values are no override"
        );
        table.insert("pump-stop".to_owned(), entry("poweroff", 5));
        table.insert("lost-input".to_owned(), entry("none", 30));
        table.insert("leakshield-unreachable".to_owned(), entry("notify", 5));
        assert_eq!(
            flags(&table),
            ["pump-stop", "leakshield-unreachable", "lost-input"]
        );
    }

    #[test]
    fn the_unknown_firmware_warning_has_a_notify_row() {
        let ids: Vec<String> = rows(&BTreeMap::new(), &[])
            .into_iter()
            .map(|r| r.0)
            .collect();
        assert!(ids.iter().any(|e| e == "leakshield-unknown-firmware"));
        assert_eq!(
            resolve(&BTreeMap::new(), "leakshield-unknown-firmware", "", false),
            (Response::Notify, DEFAULT_DELAY_S)
        );
        assert!(validate("leakshield-unknown-firmware", "", "none", 0, |_| false).is_ok());
    }

    #[test]
    fn a_user_entry_wins_and_a_bad_entry_falls_back() {
        let table = BTreeMap::from([
            ("pump-stop".to_owned(), entry("none", 5)),
            ("lost-input".to_owned(), entry("reboot", 9)),
        ]);
        assert_eq!(
            resolve(&table, "pump-stop", "loop_1", false),
            (Response::None, 5)
        );
        assert_eq!(
            resolve(&table, "lost-input", "", false),
            (Response::Notify, 9)
        );
    }

    #[test]
    fn override_wins_over_default() {
        let table = BTreeMap::from([
            (LARGE.to_owned(), entry("notify", 10)),
            (override_key(LARGE, DEVICE), entry("poweroff", 3)),
        ]);
        assert_eq!(
            resolve(&table, LARGE, DEVICE, false),
            (Response::Poweroff, 3)
        );
        // Another Leakshield and the row of the event keep the default.
        assert_eq!(
            resolve(&table, LARGE, "leakshield_1_2", false),
            (Response::Notify, 10)
        );
        assert_eq!(resolve(&table, LARGE, "", false), (Response::Notify, 10));
    }

    #[test]
    fn default_wins_over_builtin() {
        let table = BTreeMap::from([(SMALL.to_owned(), entry("poweroff", 7))]);
        assert_eq!(
            resolve(&table, SMALL, DEVICE, false),
            (Response::Poweroff, 7)
        );
        // Without an entry, the built-in default applies: notify, and poweroff for the large leak.
        assert_eq!(
            resolve(&BTreeMap::new(), SMALL, DEVICE, false),
            (Response::Notify, 30)
        );
        assert_eq!(
            resolve(&BTreeMap::new(), LARGE, DEVICE, false),
            (Response::Poweroff, 30)
        );
    }

    #[test]
    fn forced_notify_wins_over_override() {
        let table = BTreeMap::from([
            (LARGE.to_owned(), entry("poweroff", 10)),
            (override_key(LARGE, DEVICE), entry("poweroff", 3)),
        ]);
        assert_eq!(resolve(&table, LARGE, DEVICE, true).0, Response::Notify);
        assert_eq!(resolve(&table, LARGE, DEVICE, false).0, Response::Poweroff);
    }

    #[test]
    fn rows_in_event_order_with_resolved_leakshield_rows() {
        let table = BTreeMap::from([(override_key(LARGE, "leakshield_b"), entry("none", 0))]);
        let devices = ["leakshield_a".to_owned(), "leakshield_b".to_owned()];
        let rows = rows(&table, &devices);
        // 15 rows of events, then 7 events of the Leakshield for each of the two devices.
        assert_eq!(rows.len(), 15 + 7 * 2);
        assert!(rows[..15].iter().all(|r| r.1.is_empty() && !r.4));
        let order: Vec<&str> = rows[..15].iter().map(|r| r.0.as_str()).collect();
        assert_eq!(order, events().map(|(event, _)| event).collect::<Vec<_>>());
        let per_device = &rows[15..];
        assert!(per_device.iter().all(|r| r.0.starts_with("leakshield.")));
        // Event order, then device order inside an event.
        assert_eq!(
            (per_device[0].0.as_str(), per_device[0].1.as_str()),
            ("leakshield.fill-level-low-warning", "leakshield_a")
        );
        assert_eq!(per_device[1].1, "leakshield_b");
        let large_b = per_device
            .iter()
            .find(|r| r.0 == LARGE && r.1 == "leakshield_b")
            .unwrap();
        assert_eq!(
            (large_b.2.as_str(), large_b.3, large_b.4),
            ("none", 0, true)
        );
        let large_a = per_device
            .iter()
            .find(|r| r.0 == LARGE && r.1 == "leakshield_a")
            .unwrap();
        assert_eq!(
            (large_a.2.as_str(), large_a.3, large_a.4),
            ("poweroff", 30, false)
        );
    }

    #[test]
    fn set_response_checks_its_arguments() {
        let known = |source: &str| source == DEVICE;
        assert_eq!(
            validate("pump-stop", "", "none", 0, known)
                .unwrap()
                .1
                .response,
            "none"
        );
        assert!(matches!(
            validate("nonsense", "", "none", 0, known),
            Err(Error::NotFound(_))
        ));
        assert!(matches!(
            validate("pump-stop", "", "reboot", 0, known),
            Err(Error::InvalidArgument(_))
        ));
        assert_eq!(
            validate(LARGE, DEVICE, "none", 0, known).unwrap().0,
            "leakshield.leak-alarm@leakshield_20417_31072"
        );
        assert!(matches!(
            validate("pump-stop", "loop_1", "none", 0, known),
            Err(Error::InvalidArgument(_))
        ));
        assert!(matches!(
            validate(LARGE, "leakshield_9_9", "none", 0, known),
            Err(Error::NotFound(_))
        ));
    }
}
