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

/// The `details` of an Alarm (`docs/api.md`): a key is absent when it does not apply.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlarmDetails {
    pub value: Option<f64>,
    pub limit: Option<f64>,
    pub unit: String,
    /// The identifier of the sensor that caused the event.
    pub sensor: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AlarmEntry {
    pub instance: String,
    pub event: String,
    pub severity: Severity,
    pub source: String,
    pub response: Response,
    pub raised_us: u64,
    /// The time of the poweroff in microseconds since the Unix epoch, or 0.
    pub poweroff_at_us: u64,
    pub details: AlarmDetails,
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
    /// The calibrated flow sensor that the daemon makes for the Loop.
    pub flow: Option<String>,
    pub water_temperature: Option<String>,
    pub pump_channels: Vec<String>,
    /// The raw flow sensor, with the id of its Calibration table.
    pub flow_sensor: Option<String>,
    pub calibration_table: String,
    pub pump_floor: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeakshieldInfo {
    pub id: String,
    pub serial: String,
    pub firmware: u16,
    pub connected: bool,
    /// Empty when the device is writable.
    pub read_only_reason: String,
    /// `shield`, `monitor` or `release`; empty while unknown.
    pub mode: String,
    /// `test`, `measure-fill-level`, `deaerate` or `refill`; empty when no Action runs.
    pub running_action: String,
    /// The pressure limits of the Mode, when the app knows them.
    pub pressure: Option<PressureLimits>,
}

/// The target and the limits of the Leakshield pressure in mbar.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PressureLimits {
    pub target: f64,
    pub min: f64,
    pub max: f64,
}

/// The sources and the function of a Derived sensor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DerivedDef {
    /// `maximum`, `mean` or `difference`.
    pub function: String,
    pub sources: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SensorInfo {
    pub id: String,
    pub name: String,
    pub default_name: String,
    pub kind: String,
    pub unit: String,
    /// `hwmon`, `leakshield`, `derived` or `calibrated`.
    pub origin: String,
    /// The hwmon `name` attribute; empty when the origin is not hwmon.
    pub driver: String,
    pub derived: Option<DerivedDef>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Control {
    #[default]
    Hardware,
    Curve,
    Fixed,
}

impl Control {
    pub fn parse(text: &str) -> Self {
        match text {
            "curve" => Self::Curve,
            "fixed" => Self::Fixed,
            _ => Self::Hardware,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Hardware => "hardware",
            Self::Curve => "curve",
            Self::Fixed => "fixed",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChannelOverride {
    #[default]
    None,
    LostInput,
    PumpStop,
}

impl ChannelOverride {
    pub fn parse(text: &str) -> Self {
        match text {
            "lost-input" => Self::LostInput,
            "pump-stop" => Self::PumpStop,
            _ => Self::None,
        }
    }
}

/// The Curve of a Channel: Input sensor, points as (Input, Duty), smoothing and step limit.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CurveSpec {
    pub input: Option<String>,
    pub points: Vec<(f64, f64)>,
    pub smoothing_s: f64,
    pub step_limit: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub default_name: String,
    pub control: Control,
    pub override_state: ChannelOverride,
    pub is_pump: bool,
    pub manual_at_snapshot: bool,
    pub curve: CurveSpec,
    pub fixed_duty: f64,
}

/// A Calibration table: a preset of the package or a user table.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CalibrationTableInfo {
    pub id: String,
    pub name: String,
    pub preset: bool,
    /// `(raw rpm, flow L/h)`.
    pub points: Vec<(f64, f64)>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BoardInfo {
    pub vendor: String,
    pub model: String,
    /// Empty for an unknown Board.
    pub board_file: String,
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
    pub channels: BTreeMap<String, ChannelInfo>,
    pub calibration_tables: Vec<CalibrationTableInfo>,
    pub board: Option<BoardInfo>,
}

/// One `SensorsUpdated` signal: `(id, value, flags)` for each sensor and Channel.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SensorSample {
    pub time_us: u64,
    pub sensors: Vec<(String, f64, u8)>,
    pub channels: Vec<(String, f64, u8)>,
}

/// `sensors` flags: bit 0 no value, bit 1 above the range of its Calibration table.
pub const FLAG_NO_VALUE: u8 = 1;
pub const FLAG_OUT_OF_RANGE: u8 = 2;
/// `channels` flags: bit 0 override active, bit 1 on its Hardware curve.
pub const FLAG_OVERRIDE: u8 = 1;
pub const FLAG_ON_HARDWARE: u8 = 2;

impl SensorSample {
    /// The value of a sensor, `None` when it has no value.
    pub fn sensor_value(&self, id: &str) -> Option<f64> {
        self.sensors
            .iter()
            .find(|(name, _, _)| name == id)
            .and_then(|(_, value, flags)| (flags & FLAG_NO_VALUE == 0).then_some(*value))
    }

    pub fn sensor_flags(&self, id: &str) -> u8 {
        self.sensors
            .iter()
            .find(|(name, _, _)| name == id)
            .map_or(0, |(_, _, flags)| *flags)
    }

    /// The Duty of a Channel in percent.
    pub fn channel_duty(&self, id: &str) -> Option<f64> {
        self.channels
            .iter()
            .find(|(name, _, _)| name == id)
            .map(|(_, value, _)| *value)
    }

    pub fn channel_flags(&self, id: &str) -> u8 {
        self.channels
            .iter()
            .find(|(name, _, _)| name == id)
            .map_or(0, |(_, _, flags)| *flags)
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
    /// The live values. `None` while the daemon is not online.
    pub sensors: Option<SensorSample>,
    /// The last values that arrived. They stay while the daemon is away.
    pub last_sensors: Option<SensorSample>,
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
            DaemonEvent::Sensors(sample) => {
                self.sensors = Some(sample.clone());
                self.last_sensors = Some(sample.clone());
            }
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
        if let Some(channel) = snapshot.channels.get(source) {
            return channel.name.clone();
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
