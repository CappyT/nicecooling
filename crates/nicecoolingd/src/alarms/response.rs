//! The alarm response of each Warning and Alarm: none, notify, or poweroff after a delay.

use std::collections::BTreeMap;

use nicecooling_api::errors::{Error, Result};
use nicecooling_core::alarms::Severity;
use nicecooling_core::alarms::leakshield::EVENTS;

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
const NICECOOLING_EVENTS: [(&str, Severity); 7] = [
    ("pump-stop", Severity::Alarm),
    ("leakshield-unreachable", Severity::Alarm),
    ("lost-input", Severity::Warning),
    ("another-writer", Severity::Warning),
    ("manual-at-snapshot", Severity::Warning),
    ("no-bios-snapshot", Severity::Warning),
    ("state-fallback", Severity::Warning),
];

/// The events that power off by default.
const POWEROFF_BY_DEFAULT: [&str; 1] = ["pump-stop"];

/// Every event that has a row in the response table, nicecooling first.
pub fn events() -> impl Iterator<Item = (&'static str, Severity)> {
    NICECOOLING_EVENTS.into_iter().chain(EVENTS)
}

pub fn default_response(event: &str) -> (Response, u32) {
    if POWEROFF_BY_DEFAULT.contains(&event) {
        (Response::Poweroff, DEFAULT_DELAY_S)
    } else {
        (Response::Notify, DEFAULT_DELAY_S)
    }
}

/// The response of an event. An event without an entry, or with an entry that no response name
/// matches (a state file of another version), uses its default.
pub fn resolve(table: &BTreeMap<String, AlarmResponse>, event: &str) -> (Response, u32) {
    let default = default_response(event);
    match table.get(event) {
        Some(entry) => (
            Response::parse(&entry.response).unwrap_or(default.0),
            entry.delay_s,
        ),
        None => default,
    }
}

/// The rows of `GetResponses`: one for each event.
pub fn rows(table: &BTreeMap<String, AlarmResponse>) -> Vec<(String, String, u32)> {
    events()
        .map(|(event, _)| {
            let (response, delay) = resolve(table, event);
            (event.to_owned(), response.as_str().to_owned(), delay)
        })
        .collect()
}

/// Checks the arguments of `SetResponse`.
pub fn validate(event: &str, response: &str, delay_s: u32) -> Result<AlarmResponse> {
    if !events().any(|(known, _)| known == event) {
        return Err(Error::not_found(format!("{event} is not an event")));
    }
    let parsed = Response::parse(response).ok_or_else(|| {
        Error::invalid(format!(
            "the alarm response is none, notify or poweroff, not {response}"
        ))
    })?;
    Ok(AlarmResponse {
        response: parsed.as_str().to_owned(),
        delay_s,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_responses() {
        let table = BTreeMap::new();
        let rows = rows(&table);
        assert_eq!(rows.len(), 7 + 11);
        for (event, response, delay) in &rows {
            let expected = if event == "pump-stop" {
                "poweroff"
            } else {
                "notify"
            };
            assert_eq!(response, expected, "{event}");
            assert_eq!(*delay, 30, "{event}");
        }
        let unreachable = rows
            .iter()
            .find(|r| r.0 == "leakshield-unreachable")
            .unwrap();
        assert_eq!(unreachable.1, "notify");
    }

    #[test]
    fn a_user_entry_wins_and_a_bad_entry_falls_back() {
        let mut table = BTreeMap::new();
        table.insert(
            "pump-stop".to_owned(),
            AlarmResponse {
                response: "none".into(),
                delay_s: 5,
            },
        );
        table.insert(
            "lost-input".to_owned(),
            AlarmResponse {
                response: "reboot".into(),
                delay_s: 9,
            },
        );
        assert_eq!(resolve(&table, "pump-stop"), (Response::None, 5));
        assert_eq!(resolve(&table, "lost-input"), (Response::Notify, 9));
    }

    #[test]
    fn set_response_checks_its_arguments() {
        assert_eq!(validate("pump-stop", "none", 0).unwrap().response, "none");
        assert!(matches!(
            validate("nonsense", "none", 0),
            Err(Error::NotFound(_))
        ));
        assert!(matches!(
            validate("pump-stop", "reboot", 0),
            Err(Error::InvalidArgument(_))
        ));
    }
}
