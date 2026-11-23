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
    pub fn id(self) -> &'static str {
        match self {
            Self::Poweroff => "poweroff",
            Self::Notify => "notify",
            Self::None => "none",
        }
    }

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
    /// The bits of `alarm_bits_latched` that the Leakshield holds for the event, 0 when none.
    pub latched_bits: u32,
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
    pub is_default: bool,
    /// The Profile sets Mode RELEASE on its Leakshield slot.
    pub contains_release: bool,
    /// The Leakshield slot, 1 to 4.
    pub slot: u8,
}

/// A value of a Leakshield field in the stored settings of a slot.
#[derive(Clone, Debug, PartialEq)]
pub enum FieldValue {
    Flag(bool),
    Number(f64),
    /// The name of an enum value (`docs/api.md`, Leakshield fields).
    Text(String),
    /// A field that the device repeats (display pages, LED controllers): one number each.
    List(Vec<f64>),
    /// An enum field that the device repeats: one name each.
    TextList(Vec<String>),
}

/// What a Profile holds for one Channel (`docs/storage.md`, Profile file).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfileChannel {
    pub control: Control,
    pub input: Option<String>,
    pub points: Vec<(f64, f64)>,
    pub fixed_duty: f64,
}

/// `GetContent` of a Profile: the Curves and the Leakshield settings of its slot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfileContent {
    pub channels: BTreeMap<String, ProfileChannel>,
    /// For each Leakshield id, the decoded fields of the stored settings of the slot.
    pub leakshield: BTreeMap<String, BTreeMap<String, FieldValue>>,
}

/// A condition of a Rule group. The keys of the parameters are in `docs/api.md`.
#[derive(Clone, Debug, PartialEq)]
pub enum Rule {
    Sensor {
        sensor: String,
        /// `lt`, `le`, `eq`, `ge`, `gt`, `ne` or `in_range`.
        comparison: String,
        value: f64,
        /// The upper bound of `in_range`.
        value_max: f64,
        hold_s: u32,
    },
    Process {
        names: Vec<String>,
        /// `any`, `all` or `none`.
        match_kind: String,
        /// `name` or `cmdline`.
        match_on: String,
    },
    Time {
        /// ISO weekdays, 1 Monday to 7 Sunday.
        days: Vec<u8>,
        /// Minutes after midnight.
        start: u16,
        end: u16,
    },
    Interval {
        interval_s: u32,
        delay_s: u32,
        duration_s: u32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuleGroup {
    /// Empty for a group that the daemon has not stored yet.
    pub id: String,
    pub profile: String,
    pub enabled: bool,
    pub rules: Vec<Rule>,
    pub holds: bool,
    /// When the group started to hold, or 0.
    pub since_us: u64,
}

/// The alarm response of one event type: `GetResponses`.
#[derive(Clone, Debug, PartialEq)]
pub struct ResponseRule {
    pub event: String,
    pub response: Response,
    pub delay_s: u32,
}

/// What happened to an Alarm or a Warning, for the History card of the Alarms screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The Alarm is still active.
    Active,
    Notified,
    PoweroffCancelled,
    None,
}

/// One row of the history that the app keeps while it runs.
#[derive(Clone, Debug, PartialEq)]
pub struct AlarmRecord {
    pub instance: String,
    pub event: String,
    pub severity: Severity,
    pub source: String,
    pub response: Response,
    pub raised_us: u64,
    pub cleared_us: Option<u64>,
    pub outcome: Outcome,
}

/// A failed Leakshield write after a Profile activation: the backup that the restore uses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteFailure {
    pub leakshield: String,
    pub error: String,
    pub backup: String,
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
    /// `(fields, read-only fields)` from `GetFields`, when the daemon answered.
    pub field_counts: Option<(u32, u32)>,
}

/// The target and the limits of the Leakshield pressure in mbar.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PressureLimits {
    pub target: f64,
    pub min: f64,
    pub max: f64,
}

/// The risk class of a Leakshield field (`docs/api.md`, `GetFields`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldRisk {
    Safety,
    Low,
}

impl FieldRisk {
    pub fn parse(text: &str) -> Self {
        if text == "low" {
            Self::Low
        } else {
            Self::Safety
        }
    }
}

/// One row of `GetFields`.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldEntry {
    pub id: String,
    pub risk: FieldRisk,
    pub value: FieldValue,
    pub writable: bool,
    /// The read-only reason identifier, empty for a writable field.
    pub reason: String,
}

/// One row of `GetEvents`: the time on the device clock, the identifier and the details as text.
#[derive(Clone, Debug, PartialEq)]
pub struct DeviceEvent {
    pub time_us: u64,
    pub id: String,
    pub details: BTreeMap<String, String>,
}

/// One row of `ListBackups`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupEntry {
    pub id: String,
    pub time_us: u64,
    pub sha256: String,
    pub firmware: u16,
    /// `first`, `before-write`, `manual` or `import`.
    pub reason: String,
}

/// One row of `GetDeviceResponseProposals`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseProposal {
    /// The event identifier without the prefix `leakshield.`: `leak-warning`.
    pub alarm: String,
    /// `none`, `signal-short`, `signal-long` or `key-sequence`.
    pub current: String,
    pub proposed: String,
    /// The identifier of the overlap warning, empty without one.
    pub overlap: String,
}

/// What the app reads from one Leakshield beyond its properties: the calls of the interface
/// that have no signal. A call that the daemon does not support leaves its part empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeakshieldData {
    pub fields: Vec<FieldEntry>,
    /// Newest first.
    pub events: Vec<DeviceEvent>,
    /// Newest first.
    pub backups: Vec<BackupEntry>,
    /// `(slot, sensor)`: the slots 1 to 12, with the sensor that fills each.
    pub software_sensors: Vec<(u16, Option<String>)>,
    pub proposals: Vec<ResponseProposal>,
    pub structure: u16,
    pub writable: bool,
    /// Device time minus PC time, in seconds.
    pub clock_offset_s: i64,
    pub software_sensor_slots: u16,
}

impl LeakshieldData {
    pub fn field(&self, id: &str) -> Option<&FieldEntry> {
        self.fields.iter().find(|field| field.id == id)
    }

    pub fn number(&self, id: &str) -> Option<f64> {
        match &self.field(id)?.value {
            FieldValue::Number(value) => Some(*value),
            FieldValue::Flag(on) => Some(f64::from(u8::from(*on))),
            _ => None,
        }
    }

    pub fn list(&self, id: &str) -> Option<&[f64]> {
        match &self.field(id)?.value {
            FieldValue::List(values) => Some(values),
            _ => None,
        }
    }

    pub fn text_list(&self, id: &str) -> Option<&[String]> {
        match &self.field(id)?.value {
            FieldValue::TextList(names) => Some(names),
            _ => None,
        }
    }

    /// The minimum, target and maximum pressure of a Mode in mbar, from its limit fields: the
    /// lower limit, the lower limit plus the working range, and the setpoint plus the upper range.
    pub fn pressure_limits(&self, mode: &str) -> Option<PressureLimits> {
        let prefix = if mode == "monitor" {
            "monitor"
        } else {
            "shield"
        };
        let low = self.number(&format!("{prefix}_pressure_low"))?;
        let working = self.number(&format!("{prefix}_pressure_working_range"))?;
        let upper = self.number(&format!("{prefix}_pressure_max_range"))?;
        Some(PressureLimits {
            min: low,
            target: low + working,
            max: low + working + upper,
        })
    }
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
    /// `applied` or `leakshield-failed`.
    pub profile_state: String,
    pub rule_delay_s: u32,
    pub dwell_s: u32,
    pub rule_groups: Vec<RuleGroup>,
    pub responses: Vec<ResponseRule>,
    /// `GetContent` of each Profile, by Profile id.
    pub profile_contents: BTreeMap<String, ProfileContent>,
    /// What the app reads from each connected Leakshield, by Leakshield id.
    pub leakshield_data: BTreeMap<String, LeakshieldData>,
    pub daemon_version: String,
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
    WriteFailed {
        leakshield: String,
        error: String,
        backup: String,
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
    /// The Alarms and Warnings that the app saw since it started, newest last. The daemon keeps
    /// no history of them.
    pub alarm_history: Vec<AlarmRecord>,
    /// Alarm instance -> the time of its cancelled poweroff.
    pub cancelled: BTreeMap<String, u64>,
    pub write_failure: Option<WriteFailure>,
    /// The fields of the last `ExternalChange`: a Leakshield change that nicecooling did not make.
    pub external_change: Option<Vec<String>>,
}

/// The history keeps this many rows.
const HISTORY_LIMIT: usize = 200;

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
                // An Alarm that was active before the app started has no row yet.
                let active = self.snapshot.alarms.clone();
                for alarm in &active {
                    self.record_raised(alarm);
                }
                // An Alarm that closed while the app was away.
                for record in &mut self.alarm_history {
                    if record.cleared_us.is_none()
                        && !active.iter().any(|a| a.instance == record.instance)
                    {
                        record.cleared_us = Some(crate::app::now_us().max(record.raised_us));
                        if record.outcome == Outcome::Active {
                            record.outcome = Outcome::Notified;
                        }
                    }
                }
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
                let entry = AlarmEntry {
                    raised_us,
                    ..entry.clone()
                };
                self.record_raised(&entry);
                self.snapshot.alarms.push(entry);
            }
            DaemonEvent::AlarmCleared { instance } => {
                self.snapshot
                    .alarms
                    .retain(|alarm| &alarm.instance != instance);
                self.cancelled.remove(instance);
                if let Some(record) = self
                    .alarm_history
                    .iter_mut()
                    .rev()
                    .find(|r| &r.instance == instance && r.cleared_us.is_none())
                {
                    record.cleared_us = Some(crate::app::now_us().max(record.raised_us));
                    if record.outcome == Outcome::Active {
                        record.outcome = match record.response {
                            Response::None => Outcome::None,
                            _ => Outcome::Notified,
                        };
                    }
                }
            }
            DaemonEvent::PoweroffCancelled { instance } => {
                self.cancelled
                    .insert(instance.clone(), crate::app::now_us());
                if let Some(record) = self
                    .alarm_history
                    .iter_mut()
                    .rev()
                    .find(|r| &r.instance == instance)
                {
                    record.outcome = Outcome::PoweroffCancelled;
                }
                if let Some(alarm) = self
                    .snapshot
                    .alarms
                    .iter_mut()
                    .find(|a| &a.instance == instance)
                {
                    alarm.poweroff_at_us = 0;
                }
            }
            DaemonEvent::ProfileActivated {
                profile,
                cause,
                state,
            } => {
                self.snapshot.cause = *cause;
                self.snapshot.profile_state = state.clone();
                // A new activation ends the old write failure.
                if state != "leakshield-failed" {
                    self.write_failure = None;
                }
                self.external_change = None;
                if let Some(info) = self.snapshot.profiles.iter().find(|p| &p.id == profile) {
                    self.snapshot.active_profile = Some(info.clone());
                }
            }
            DaemonEvent::WriteFailed {
                leakshield,
                error,
                backup,
            } => {
                self.write_failure = Some(WriteFailure {
                    leakshield: leakshield.clone(),
                    error: error.clone(),
                    backup: backup.clone(),
                });
            }
            DaemonEvent::ExternalChange { fields, .. } => {
                self.external_change = Some(fields.clone());
            }
            DaemonEvent::PoweroffFailed { .. } => {}
        }
    }

    fn record_raised(&mut self, alarm: &AlarmEntry) {
        if self
            .alarm_history
            .iter()
            .any(|r| r.instance == alarm.instance && r.cleared_us.is_none())
        {
            return;
        }
        self.alarm_history.push(AlarmRecord {
            instance: alarm.instance.clone(),
            event: alarm.event.clone(),
            severity: alarm.severity,
            source: alarm.source.clone(),
            response: alarm.response,
            raised_us: if alarm.raised_us == 0 {
                crate::app::now_us()
            } else {
                alarm.raised_us
            },
            cleared_us: None,
            outcome: Outcome::Active,
        });
        if self.alarm_history.len() > HISTORY_LIMIT {
            self.alarm_history.remove(0);
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
