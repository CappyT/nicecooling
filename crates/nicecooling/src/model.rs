//! The data the app holds about the daemon, and the rules that update it from events.

use std::collections::BTreeMap;

use crate::bitmap::Bitmap;

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

/// One row of `GetResponses`. An empty `source` is the default of the event. A Leakshield id is
/// the resolved response of that Leakshield, and `overridden` tells that an override exists.
#[derive(Clone, Debug, PartialEq)]
pub struct ResponseRule {
    pub event: String,
    pub source: String,
    pub response: Response,
    pub delay_s: u32,
    pub overridden: bool,
}

/// What happened to an Alarm or a Warning: the `outcome` of `GetHistory`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    None,
    Notified,
    PoweroffPending,
    PoweroffCancelled,
    /// The Alarm closed before the call to logind.
    PoweroffStopped,
    PoweroffRequested,
    PoweroffFailed,
}

impl Outcome {
    pub fn parse(text: &str) -> Self {
        match text {
            "notified" => Self::Notified,
            "poweroff-pending" => Self::PoweroffPending,
            "poweroff-cancelled" => Self::PoweroffCancelled,
            "poweroff-stopped" => Self::PoweroffStopped,
            "poweroff-requested" => Self::PoweroffRequested,
            "poweroff-failed" => Self::PoweroffFailed,
            _ => Self::None,
        }
    }
}

/// One row of `GetHistory`.
#[derive(Clone, Debug, PartialEq)]
pub struct AlarmRecord {
    pub instance: String,
    pub event: String,
    pub severity: Severity,
    pub source: String,
    pub response: Response,
    pub raised_us: u64,
    /// `None` while the instance is active.
    pub cleared_us: Option<u64>,
    pub outcome: Outcome,
    /// A daemon restart closed the row.
    pub closed_by_restart: bool,
    pub details: AlarmDetails,
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
    /// The state of the Feed that the daemon sends to this Leakshield.
    pub feed_state: FeedState,
    /// The last good Feed send.
    pub last_feed: LastFeed,
    /// The slot of report 1, 0 when the daemon does not know it.
    pub active_slot: u8,
    /// The backup id of the last Profile write that failed. Empty when none.
    pub failed_write_backup: String,
    /// `(fields, read-only fields)` from `GetFields`, when the daemon answered.
    pub field_counts: Option<(u32, u32)>,
}

/// `Leakshield.FeedState`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FeedState {
    #[default]
    Off,
    Waiting,
    Ok,
    Failing,
}

impl FeedState {
    pub fn parse(text: &str) -> Self {
        match text {
            "waiting" => Self::Waiting,
            "ok" => Self::Ok,
            "failing" => Self::Failing,
            _ => Self::Off,
        }
    }
}

/// `Leakshield.LastFeed`: the time of the last good send and the values that it carried.
/// The time is 0 before the first good send.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LastFeed {
    pub time_us: u64,
    pub feed_pump: f64,
    pub flow_lph: f64,
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

/// One row of `GetRefillHistory`: a refill that the daemon keeps.
#[derive(Clone, Debug, PartialEq)]
pub struct RefillEntry {
    /// The device time, as in `DeviceEvent`.
    pub time_us: u64,
    pub duration_s: u32,
    pub pump_turns: u32,
    pub volume_ml: f64,
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

/// `GetDisplayAssets`: what report 8 holds for the display pages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DisplayAssets {
    /// The 40 names: 0 to 19 device sensors, 20 to 34 page texts, 35 to 39 lighting.
    pub names: Vec<String>,
    /// The 16 icons of 23 x 23 pixels.
    pub icons: Vec<Bitmap>,
    /// The start image of 128 x 64 pixels.
    pub logo: Bitmap,
}

/// The last answer of `GetDisplayPreview` for one page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PagePreview {
    Image(Bitmap),
    /// The call failed. The text is the last element of the error name: `PageNotShown`,
    /// `ReadOnly`, `Disconnected`, `NotSupported`, `UnknownMethod`.
    Failed(String),
}

/// What the app reads from one Leakshield beyond its properties: the calls of the interface
/// that have no signal. A call that the daemon does not support leaves its part empty.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LeakshieldData {
    pub fields: Vec<FieldEntry>,
    /// Newest first.
    pub events: Vec<DeviceEvent>,
    /// Oldest first.
    pub refill_history: Vec<RefillEntry>,
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
    /// `GetStoredFields` of each slot that has stored settings.
    pub stored: BTreeMap<u8, BTreeMap<String, FieldValue>>,
    /// `(slot, scale, offset)` of `GetSoftwareSensorScales`. A slot that is not here has scale 1
    /// and offset 0.
    pub software_scales: Vec<(u16, f64, f64)>,
    /// The sensors that a slot takes (`GetSoftwareSensorSources`). `None` when the daemon gave
    /// no list.
    pub software_sources: Option<Vec<String>>,
    /// `GetCounters`. `None` when the daemon gave no counters.
    pub counters: Option<BTreeMap<String, u64>>,
    /// `GetDisplayAssets`. `None` when the daemon gave no assets.
    pub display_assets: Option<DisplayAssets>,
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
    pub bios_snapshot: BiosSnapshot,
    /// The speed sensor of the Channel. `None` when it has none.
    pub fan_sensor: Option<String>,
}

/// `Channel.BiosSnapshot`: the time of the snapshot, 0 when the daemon holds none, and the
/// attributes of the Channel as sysfs text, in restore order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BiosSnapshot {
    pub time_us: u64,
    pub attributes: Vec<(String, String)>,
}

impl ChannelInfo {
    /// The Hardware curve of the snapshot as `(temperature in degC, Duty in percent)` rows, from
    /// `pwm<N>_auto_point<K>_temp` (millidegree) and `pwm<N>_auto_point<K>_pwm` (0 to 255). Empty
    /// when the snapshot has no such attributes.
    pub fn hardware_curve(&self) -> Vec<(f64, f64)> {
        let Some((_, number)) = self.id.rsplit_once("_pwm") else {
            return Vec::new();
        };
        let prefix = format!("pwm{number}_auto_point");
        let mut points: BTreeMap<u32, (Option<f64>, Option<f64>)> = BTreeMap::new();
        for (name, value) in &self.bios_snapshot.attributes {
            let Some(rest) = name.strip_prefix(&prefix) else {
                continue;
            };
            let Some((point, kind)) = rest.split_once('_') else {
                continue;
            };
            let (Ok(point), Ok(value)) = (point.parse::<u32>(), value.trim().parse::<f64>()) else {
                continue;
            };
            let entry = points.entry(point).or_default();
            match kind {
                "temp" => entry.0 = Some(value / 1000.0),
                "pwm" => entry.1 = Some(value / 255.0 * 100.0),
                _ => {}
            }
        }
        points
            .into_values()
            .filter_map(|(temp, duty)| Some((temp?, duty?)))
            .collect()
    }
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
    /// The end of the running Profile dwell, 0 when none runs.
    pub dwell_until_us: u64,
    pub rule_groups: Vec<RuleGroup>,
    /// `Profiles.RuleGroupState`: group id, holds, since.
    pub rule_group_state: Vec<(String, bool, u64)>,
    /// Every row of `GetResponses`.
    pub responses: Vec<ResponseRule>,
    /// `GetHistory`, newest raise first.
    pub alarm_history: Vec<AlarmRecord>,
    /// `GetContent` of each Profile, by Profile id.
    pub profile_contents: BTreeMap<String, ProfileContent>,
    /// What the app reads from each connected Leakshield, by Leakshield id.
    pub leakshield_data: BTreeMap<String, LeakshieldData>,
    pub daemon_version: String,
}

/// One `SensorsUpdated` signal. A sensor row is `(id, value, flags)`. A Channel row is
/// `(id, Duty, smoothed Input, Curve target, flags)`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SensorSample {
    pub time_us: u64,
    pub sensors: Vec<(String, f64, u8)>,
    pub channels: Vec<(String, f64, f64, f64, u8)>,
}

/// `sensors` flags: bit 0 no value, bit 1 above the range of its Calibration table.
pub const FLAG_NO_VALUE: u8 = 1;
pub const FLAG_OUT_OF_RANGE: u8 = 2;
/// `channels` flags: bit 0 override active, bit 1 on its Hardware curve, bit 2 the smoothed Input
/// and the Curve target are valid.
pub const FLAG_OVERRIDE: u8 = 1;
pub const FLAG_ON_HARDWARE: u8 = 2;
pub const FLAG_READOUT_VALID: u8 = 4;

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
            .find(|row| row.0 == id)
            .map(|row| row.1)
    }

    pub fn channel_flags(&self, id: &str) -> u8 {
        self.channels
            .iter()
            .find(|row| row.0 == id)
            .map_or(0, |row| row.4)
    }

    /// The smoothed Input and the Curve target of a Channel, `None` without flag bit 2.
    pub fn channel_readout(&self, id: &str) -> Option<(f64, f64)> {
        self.channels
            .iter()
            .find(|row| row.0 == id && row.4 & FLAG_READOUT_VALID != 0)
            .map(|row| (row.2, row.3))
    }
}

/// A `PropertiesChanged` signal: the object, the interface and the changed values.
#[derive(Clone, Debug, PartialEq)]
pub struct PropertyChange {
    pub path: String,
    pub interface: String,
    pub changed: std::collections::HashMap<String, zbus::OwnedVariant>,
}

/// Parts of the snapshot that need a call and not only a property. A part that is `None` did not
/// change.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Derived {
    pub profile_contents: Option<BTreeMap<String, ProfileContent>>,
    pub rule_groups: Option<Vec<RuleGroup>>,
    pub calibration_tables: Option<Vec<CalibrationTableInfo>>,
    /// The Leakshield id and what the app reads from it.
    pub leakshield: Option<(String, LeakshieldData)>,
}

/// What the daemon client reports to the rest of the app.
#[derive(Clone, Debug, PartialEq)]
pub enum DaemonEvent {
    Link(Link),
    /// A full read. It implies `Link::Online`.
    Snapshot(Box<Snapshot>),
    Sensors(SensorSample),
    /// `GetHistory` was read again.
    History(Vec<AlarmRecord>),
    /// `GetResponses` was read again: the daemon sends no signal for a response.
    Responses(Vec<ResponseRule>),
    /// A `PropertiesChanged`: only the properties that the signal carries change.
    Properties(PropertyChange),
    /// What the app read again because a property changed.
    Derived(Box<Derived>),
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
    /// The answer of `GetDisplayPreview` for a page (0 to 9).
    DisplayPreview {
        leakshield: String,
        page: u8,
        preview: PagePreview,
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
    /// Alarm instance -> the time of its cancelled poweroff.
    pub cancelled: BTreeMap<String, u64>,
    pub write_failure: Option<WriteFailure>,
    /// The fields of the last `ExternalChange`: a Leakshield change that nicecooling did not make.
    pub external_change: Option<Vec<String>>,
    /// The last display preview of each page, by Leakshield id and page (0 to 9).
    pub display_previews: BTreeMap<(String, u8), PagePreview>,
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
            DaemonEvent::History(rows) => self.snapshot.alarm_history = rows.clone(),
            DaemonEvent::Responses(rows) => self.snapshot.responses = rows.clone(),
            DaemonEvent::Properties(change) => {
                crate::daemon_client::apply_properties(&mut self.snapshot, change);
            }
            DaemonEvent::Derived(derived) => {
                crate::daemon_client::apply_derived(&mut self.snapshot, derived)
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
                self.snapshot.alarms.push(entry);
            }
            DaemonEvent::AlarmCleared { instance } => {
                self.snapshot
                    .alarms
                    .retain(|alarm| &alarm.instance != instance);
                self.cancelled.remove(instance);
            }
            DaemonEvent::PoweroffCancelled { instance } => {
                self.cancelled
                    .insert(instance.clone(), crate::app::now_us());
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
            DaemonEvent::DisplayPreview {
                leakshield,
                page,
                preview,
            } => {
                self.display_previews
                    .insert((leakshield.clone(), *page), preview.clone());
            }
            DaemonEvent::PoweroffFailed { .. } => {}
        }
    }

    pub fn is_online(&self) -> bool {
        self.link == Link::Online
    }

    /// The target, minimum and maximum pressure of a Leakshield, from its three limit sensors.
    pub fn leakshield_pressure_limits(&self, leakshield: &str) -> Option<PressureLimits> {
        let sample = self.sensors.as_ref().or(self.last_sensors.as_ref())?;
        let value = |name: &str| sample.sensor_value(&format!("{leakshield}_pressure_{name}"));
        Some(PressureLimits {
            target: value("setpoint")?,
            min: value("low")?,
            max: value("high")?,
        })
    }

    /// The row that the daemon resolves for an event and a Leakshield. Without it, the default
    /// of the event.
    pub fn leakshield_response(&self, event: &str, leakshield: &str) -> Option<&ResponseRule> {
        self.snapshot
            .responses
            .iter()
            .find(|rule| rule.event == event && rule.source == leakshield)
            .or_else(|| self.default_response(event))
    }

    /// The row of an event that holds its default: source `""` in `GetResponses`.
    pub fn default_response(&self, event: &str) -> Option<&ResponseRule> {
        self.snapshot
            .responses
            .iter()
            .find(|rule| rule.event == event && rule.source.is_empty())
    }

    /// The speed sensor of a Channel, from `Channel.FanSensor`.
    pub fn fan_sensor(&self, channel: &str) -> Option<String> {
        self.snapshot.channels.get(channel)?.fan_sensor.clone()
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
