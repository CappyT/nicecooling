//! Alarms: the events that the tick evaluates in stage 3 (`docs/api.md`, section Event
//! identifiers). The Warnings of nicecooling live in `warnings.rs`.

use std::collections::BTreeMap;
use std::time::Duration;

pub mod leakshield;
pub mod proposals;
pub mod pump_stop;
pub mod unreachable;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Alarm,
}

impl Severity {
    /// The name that `docs/api.md` uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Warning => "warning",
            Self::Alarm => "alarm",
        }
    }
}

/// The active Alarms, and the Warnings of the Leakshield, each with an event id and a source id.
#[derive(Clone, Debug, Default)]
pub struct ActiveAlarms {
    active: BTreeMap<(String, String), (Severity, u32)>,
}

impl ActiveAlarms {
    /// Returns true when the event was not active before.
    pub fn raise(&mut self, event: &str, source: &str, severity: Severity) -> bool {
        self.raise_bits(event, source, severity, 0)
    }

    /// Like `raise`, for an event that the bits of a device report set. An event that is active
    /// takes the new bits.
    pub fn raise_bits(&mut self, event: &str, source: &str, severity: Severity, bits: u32) -> bool {
        self.active
            .insert((event.to_owned(), source.to_owned()), (severity, bits))
            .is_none()
    }

    /// The device bits of an active event, 0 when it has none or is not active.
    pub fn bits(&self, event: &str, source: &str) -> u32 {
        self.active
            .get(&(event.to_owned(), source.to_owned()))
            .map_or(0, |&(_, bits)| bits)
    }

    /// Returns true when the event was active.
    pub fn clear(&mut self, event: &str, source: &str) -> bool {
        self.active
            .remove(&(event.to_owned(), source.to_owned()))
            .is_some()
    }

    pub fn is_active(&self, event: &str, source: &str) -> bool {
        self.active
            .contains_key(&(event.to_owned(), source.to_owned()))
    }

    /// True when the event is active for any source.
    pub fn any_active(&self, event: &str) -> bool {
        self.active.keys().any(|(e, _)| e == event)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&str, &str) -> bool) {
        self.active.retain(|(e, s), _| keep(e, s));
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str, Severity)> {
        self.active
            .iter()
            .map(|((e, s), (severity, _))| (e.as_str(), s.as_str(), *severity))
    }
}

/// What the tick knows of one Leakshield. The sources of the daemon fill it in.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeakshieldStatus {
    /// `alarm_bits` of the last report 1.
    pub alarm_bits: u32,
    /// When the last report 1 arrived. `None` when none arrived yet.
    pub report1_at: Option<Duration>,
    /// When the current run of failed Feed messages began. `None` while the Feed message works.
    pub feed_failing_since: Option<Duration>,
}
