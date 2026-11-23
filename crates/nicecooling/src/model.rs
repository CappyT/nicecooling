//! The data the app holds about the daemon, and the rules that update it from events.

use std::collections::BTreeMap;

/// The state of the link to the daemon.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Link {
    /// The start: no answer yet. The window shows "Connecting" for at most 2 s.
    #[default]
    Connecting,
    Online,
    /// The daemon is not on the bus.
    Offline,
    /// `nicecoolingd.service` is `failed` with `Result` `start-limit-hit`.
    Down,
}

/// `ActiveState` and `Result` of `nicecoolingd.service`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnitState {
    pub active_state: String,
    pub result: String,
}

impl UnitState {
    /// The link state when the daemon is not on the bus.
    pub fn link(&self) -> Link {
        if self.active_state == "failed" && self.result == "start-limit-hit" {
            Link::Down
        } else {
            Link::Offline
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Warning,
    Alarm,
}

impl Severity {
    pub fn parse(text: &str) -> Self {
        if text == "alarm" {
            Self::Alarm
        } else {
            Self::Warning
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    None,
    Notify,
    Poweroff,
}

impl Response {
    pub fn parse(text: &str) -> Self {
        match text {
            "poweroff" => Self::Poweroff,
            "notify" => Self::Notify,
            _ => Self::None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AlarmEntry {
    pub instance: String,
    pub event: String,
    pub severity: Severity,
    pub source: String,
    pub response: Response,
    pub raised_us: u64,
    /// The time of the poweroff in microseconds since the Unix epoch, or 0.
    pub poweroff_at_us: u64,
}

impl AlarmEntry {
    /// Whole seconds until the poweroff, rounded up. `None` without a pending poweroff.
    pub fn poweroff_in_s(&self, now_us: u64) -> Option<u64> {
        (self.poweroff_at_us != 0).then(|| {
            self.poweroff_at_us
                .saturating_sub(now_us)
                .div_ceil(1_000_000)
        })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cause {
    #[default]
    Default,
    Rule,
    Manual,
}

impl Cause {
    pub fn parse(text: &str) -> Self {
        match text {
            "rule" => Self::Rule,
            "manual" => Self::Manual,
            _ => Self::Default,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LoopInfo {
    pub id: String,
    pub name: String,
    pub leakshield: Option<String>,
    pub flow: Option<String>,
    pub water_temperature: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeakshieldInfo {
    pub id: String,
    pub serial: String,
    pub firmware: u16,
    pub connected: bool,
    /// Empty when the device is writable.
    pub read_only_reason: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SensorInfo {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub unit: String,
}

/// Everything the app reads from the daemon in one pass.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Snapshot {
    pub active_profile: Option<ProfileInfo>,
    pub profiles: Vec<ProfileInfo>,
    pub cause: Cause,
    pub immediate_switching: bool,
    pub alarms: Vec<AlarmEntry>,
    pub loops: Vec<LoopInfo>,
    pub leakshields: Vec<LeakshieldInfo>,
    pub sensors: BTreeMap<String, SensorInfo>,
    /// Channel ids with their user names.
    pub channels: BTreeMap<String, String>,
}

/// One `SensorsUpdated` signal: `(id, value, flags)` for each sensor and Channel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SensorSample {
    pub time_us: u64,
    pub sensors: Vec<(String, f64, u8)>,
    pub channels: Vec<(String, f64, u8)>,
}

pub const FLAG_NO_VALUE: u8 = 1;

impl SensorSample {
    /// The value of a sensor, `None` when it has no value.
    pub fn sensor_value(&self, id: &str) -> Option<f64> {
        self.sensors
            .iter()
            .find(|(name, _, _)| name == id)
            .and_then(|(_, value, flags)| (flags & FLAG_NO_VALUE == 0).then_some(*value))
    }
}

/// What the daemon client reports to the rest of the app.
#[derive(Clone, Debug, PartialEq)]
pub enum DaemonEvent {
    Link(Link),
    /// A full read. It implies `Link::Online`.
    Snapshot(Box<Snapshot>),
    Sensors(SensorSample),
    AlarmRaised(AlarmEntry),
    AlarmCleared {
        instance: String,
    },
    PoweroffCancelled {
        instance: String,
    },
    PoweroffFailed {
        instance: String,
        error: String,
    },
    ProfileActivated {
        profile: String,
        cause: Cause,
        state: String,
    },
    ExternalChange {
        leakshield: String,
        fields: Vec<String>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct AppState {
    pub link: Link,
    pub snapshot: Snapshot,
    pub sensors: Option<SensorSample>,
}

impl AppState {
    pub fn apply(&mut self, event: &DaemonEvent) {
        match event {
            DaemonEvent::Link(link) => {
                self.link = *link;
                if *link != Link::Online {
                    self.sensors = None;
                }
            }
            DaemonEvent::Snapshot(snapshot) => {
                self.link = Link::Online;
                self.snapshot = (**snapshot).clone();
            }
            DaemonEvent::Sensors(sample) => self.sensors = Some(sample.clone()),
            DaemonEvent::AlarmRaised(entry) => {
                // The signal has no raise time. A full read that came first has it.
                let known = self
                    .snapshot
                    .alarms
                    .iter()
                    .find(|alarm| alarm.instance == entry.instance)
                    .map(|alarm| alarm.raised_us);
                let raised_us = match known {
                    Some(raised_us) if entry.raised_us == 0 => raised_us,
                    _ => entry.raised_us,
                };
                self.snapshot
                    .alarms
                    .retain(|alarm| alarm.instance != entry.instance);
                self.snapshot.alarms.push(AlarmEntry {
                    raised_us,
                    ..entry.clone()
                });
            }
            DaemonEvent::AlarmCleared { instance } => {
                self.snapshot
                    .alarms
                    .retain(|alarm| &alarm.instance != instance);
            }
            DaemonEvent::PoweroffCancelled { instance } => {
                if let Some(alarm) = self
                    .snapshot
                    .alarms
                    .iter_mut()
                    .find(|a| &a.instance == instance)
                {
                    alarm.poweroff_at_us = 0;
                }
            }
            DaemonEvent::ProfileActivated { profile, cause, .. } => {
                self.snapshot.cause = *cause;
                if let Some(info) = self.snapshot.profiles.iter().find(|p| &p.id == profile) {
                    self.snapshot.active_profile = Some(info.clone());
                }
            }
            DaemonEvent::PoweroffFailed { .. } | DaemonEvent::ExternalChange { .. } => {}
        }
    }

    pub fn is_online(&self) -> bool {
        self.link == Link::Online
    }

    pub fn alarm_count(&self) -> usize {
        self.count(Severity::Alarm)
    }

    pub fn warning_count(&self) -> usize {
        self.count(Severity::Warning)
    }

    fn count(&self, severity: Severity) -> usize {
        self.snapshot
            .alarms
            .iter()
            .filter(|a| a.severity == severity)
            .count()
    }

    /// The Alarm with the earliest pending poweroff.
    pub fn next_poweroff(&self) -> Option<&AlarmEntry> {
        self.snapshot
            .alarms
            .iter()
            .filter(|alarm| alarm.poweroff_at_us != 0)
            .min_by_key(|alarm| alarm.poweroff_at_us)
    }

    /// The Loop that a source belongs to: the Loop itself, or the Loop of a Leakshield.
    pub fn source_loop(&self, source: &str) -> Option<&LoopInfo> {
        let loops = &self.snapshot.loops;
        loops.iter().find(|l| l.id == source).or_else(|| {
            loops
                .iter()
                .find(|l| l.leakshield.as_deref() == Some(source))
        })
    }

    /// The display name of an event source: a Loop, a Leakshield, a Channel or a sensor.
    pub fn source_name(&self, source: &str, leakshield_label: &str) -> String {
        let snapshot = &self.snapshot;
        if let Some(found) = snapshot.loops.iter().find(|l| l.id == source) {
            return found.name.clone();
        }
        if snapshot.leakshields.iter().any(|l| l.id == source) {
            return leakshield_label.to_owned();
        }
        if let Some(name) = snapshot.channels.get(source) {
            return name.clone();
        }
        if let Some(sensor) = snapshot.sensors.get(source) {
            return sensor.name.clone();
        }
        source.to_owned()
    }

    /// Like `source_name`, with the Loop after a Leakshield: "Leakshield, Main loop".
    pub fn source_with_loop(&self, source: &str, leakshield_label: &str) -> String {
        let name = self.source_name(source, leakshield_label);
        let is_leakshield = self.snapshot.leakshields.iter().any(|l| l.id == source);
        match self.source_loop(source) {
            Some(found) if is_leakshield => format!("{name}, {}", found.name),
            _ => name,
        }
    }
}

/// `m:ss` for a count of seconds.
pub fn format_countdown(seconds: u64) -> String {
    format!("{}:{:02}", seconds / 60, seconds % 60)
}
