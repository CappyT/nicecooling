//! The model behind the D-Bus objects: the persisted state, the live values of the last tick and
//! the checks of every write. A write changes the state, queues a new tick configuration and
//! asks the persister for a flush 2 s later. Locks are never held across an `await`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nicecooling_api::Error;
use nicecooling_api::errors::Result;
use nicecooling_api::ids::{self, Kind};
use nicecooling_core::calibration::{CalibrationTable, Preset};
use nicecooling_core::clock::Clock;
use nicecooling_core::curve::{Curve, DEFAULT_SMOOTHING_S, DEFAULT_STEP_LIMIT};
use nicecooling_core::derived::{DerivedKind, DerivedSensor};
use nicecooling_core::graph_sensor::{MAX_GRAPH_SENSORS, graph_sensor_kind};
use nicecooling_core::loops::{Loop, tach_sensor};
use nicecooling_core::tick::TickConfig;
use nicecooling_hwmon::identity::DeviceIdentity;
use nicecooling_hwmon::sysfs::Sysfs;
use tokio::sync::{mpsc, oneshot};

mod profiles;
mod rules;

pub use profiles::{ProfileContent, ProfileEvent, ProfileView};
pub use rules::HubRules;

use super::leakshield::LeakshieldInfo;
use crate::alarms::history::{HistoryRow, HistoryStore, SAVE_EVERY};
use crate::alarms::logind::Login1;
use crate::alarms::poweroff::{AlarmManager, CancelError, Effect, Event, Row};
use crate::alarms::response;
use crate::leakshield::software_sensors::Assignment;
use crate::snapshot::{self, BiosSnapshot};
use crate::state::profiles::DEFAULT_PROFILE;
use crate::state::{
    CalibrationDef, ControlKind, DaemonState, GraphSensorDef, PERSIST_DELAY, Persister,
    ProfileChannel, ProfileFile, ProfileSet, STATE_FILE, SlotScale, SlotStore, StateStore,
    profile_file, tick_config,
};
use crate::value_table::ValueTable;

/// The default Pump floor of a new Loop, in percent.
pub const DEFAULT_PUMP_FLOOR: f64 = 40.0;

/// The limits of the API (`docs/api.md`). A write above a limit is `InvalidArgument`.
pub const MIN_POINTS: usize = 2;
pub const MAX_CURVE_POINTS: usize = 8;
pub const MAX_TABLE_POINTS: usize = 64;
pub const MAX_NAME_CHARS: usize = 64;
pub const MAX_DERIVED_SENSORS: usize = 64;
pub const MAX_LOOPS: usize = 16;
pub const MAX_CALIBRATION_TABLES: usize = 32;
pub const MAX_RULE_GROUPS: usize = 32;
pub const MAX_RULES_PER_GROUP: usize = 16;

#[derive(Clone, Debug, Default)]
pub struct BoardInfo {
    pub vendor: String,
    pub model: String,
    /// The Board file id, empty for an unknown Board.
    pub board_file: String,
}

/// An hwmon sensor as discovery found it, with the default name.
#[derive(Clone, Debug)]
pub struct SensorMeta {
    pub id: String,
    pub name: String,
    pub kind: &'static str,
    pub unit: &'static str,
    /// The hwmon name.
    pub driver: String,
}

#[derive(Clone, Debug)]
pub struct ChannelMeta {
    pub id: String,
    pub default_name: String,
    pub device: DeviceIdentity,
    pub number: u32,
    pub dir: PathBuf,
}

/// What the tick shows of one Channel.
#[derive(Clone, Debug, Default)]
pub struct ChannelLive {
    pub duty: f64,
    pub raw: u8,
    /// `none`, `lost-input` or `pump-stop`.
    pub override_state: &'static str,
}

/// The values of the last tick.
#[derive(Clone, Debug, Default)]
pub struct Live {
    pub values: BTreeMap<String, f64>,
    pub out_of_range: BTreeSet<String>,
    pub channels: BTreeMap<String, ChannelLive>,
    pub manual_at_snapshot: BTreeSet<String>,
}

/// One `SensorsUpdated` signal. A flag is documented in `docs/api.md`.
#[derive(Clone, Debug)]
pub struct TickReport {
    pub time_us: u64,
    pub sensors: Vec<(String, f64, u8)>,
    /// (id, Duty, smoothed Input, Curve target, flags).
    pub channels: Vec<(String, f64, f64, f64, u8)>,
}

pub struct HubInit {
    pub store: StateStore,
    pub sysfs_root: PathBuf,
    pub state: DaemonState,
    pub profiles: ProfileSet,
    pub presets: Vec<Preset>,
    pub board: BoardInfo,
    pub sensors: Vec<SensorMeta>,
    pub channels: Vec<ChannelMeta>,
    pub leakshields: Vec<LeakshieldInfo>,
    pub state_health: String,
    /// The table that the tick reads. `SetGraphSensorValues` writes the Graph sensors into it.
    pub values: ValueTable,
    /// The clock of the tick: a value is as old as the tick sees it.
    pub clock: Arc<dyn Clock>,
}

pub struct SensorView {
    pub id: String,
    pub name: String,
    pub default_name: String,
    pub kind: String,
    pub unit: String,
    pub origin: &'static str,
    pub driver: String,
    pub value: Option<f64>,
}

pub struct ChannelView {
    pub id: String,
    pub name: String,
    pub default_name: String,
    pub duty: f64,
    pub raw: u8,
    pub control: &'static str,
    pub override_state: &'static str,
    pub is_pump: bool,
    pub manual_at_snapshot: bool,
    /// The Input sensor id, empty when the Channel has no Curve.
    pub curve_input: String,
    pub curve_points: Vec<(f64, f64)>,
    pub smoothing_s: f64,
    pub step_limit: f64,
    pub fixed_duty: f64,
    /// The snapshot time and the attributes of the Channel. `(0, [])` without a snapshot.
    pub bios_snapshot: (u64, Vec<(String, String)>),
    /// The fan sensor of the Channel, when it exists.
    pub fan_sensor: Option<String>,
}

pub struct LoopView {
    pub id: String,
    pub name: String,
    pub pump_channels: Vec<String>,
    pub flow_sensor: Option<String>,
    pub calibration_table: Option<String>,
    /// The id of the calibrated flow sensor, when the Loop has one.
    pub flow: Option<String>,
    pub water_temperature: Option<String>,
    pub leakshield: Option<String>,
    pub pump_floor: f64,
}

pub type CalibrationRow = (String, String, bool, Vec<(f64, f64)>);

struct Inner {
    state: DaemonState,
    profiles: ProfileSet,
    /// Profiles that a delete removed. The next flush removes their files.
    deleted: BTreeSet<String>,
    /// `applied` or `leakshield-failed`: how the last activation ended.
    active_state: &'static str,
    /// The number of the newest activation. Only the newest one reports its end.
    activation: u64,
    /// What the rules stage of the tick has not read yet.
    rule_queue: rules::RuleQueue,
    /// (group id, holds, since in microseconds), as the last tick published them.
    rule_states: Vec<(String, bool, u64)>,
    /// The end of the running Profile dwell in microseconds since the Unix epoch, 0 without one.
    dwell_until_us: u64,
    presets: Vec<Preset>,
    board: BoardInfo,
    sensors: Vec<SensorMeta>,
    channels: Vec<ChannelMeta>,
    leakshields: Vec<LeakshieldInfo>,
    state_health: String,
    /// The BIOS snapshot, read once at the start. The restore of a release uses it.
    snapshot: Option<BiosSnapshot>,
    live: Live,
}

type Release = (String, oneshot::Sender<std::result::Result<(), String>>);

pub struct Hub {
    inner: Arc<Mutex<Inner>>,
    store: Arc<StateStore>,
    slots: SlotStore,
    sysfs_root: PathBuf,
    persister: Persister,
    authz: OnceLock<crate::authz::Authz>,
    /// The bus connection that `dbus::serve` set, to register the objects of late sensors.
    connection: OnceLock<zbus::Connection>,
    pending: Mutex<Option<TickConfig>>,
    releases: Mutex<Vec<Release>>,
    reports: mpsc::Sender<TickReport>,
    report_rx: Mutex<Option<mpsc::Receiver<TickReport>>>,
    profile_events: mpsc::UnboundedSender<ProfileEvent>,
    profile_rx: Mutex<Option<mpsc::UnboundedReceiver<ProfileEvent>>>,
    alarms: Mutex<AlarmManager>,
    history: HistoryStore,
    alarm_effects: mpsc::UnboundedSender<Effect>,
    alarm_rx: Mutex<Option<mpsc::UnboundedReceiver<Effect>>>,
    login1: OnceLock<Arc<dyn Login1>>,
    values: ValueTable,
    clock: Arc<dyn Clock>,
    /// zbus runs the handlers on its own runtime: Tokio timers and tasks go through this handle.
    runtime: tokio::runtime::Handle,
}

fn wall_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64)
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn failed(message: impl Into<String>) -> Error {
    Error::ZBus(zbus::fdo::Error::Failed(message.into()).into())
}

fn sensor_id_of(path: &str) -> Result<String> {
    ids::id_from_path(Kind::Sensor, path)
        .map(str::to_owned)
        .ok_or_else(|| Error::invalid(format!("{path} is not a sensor object path")))
}

/// An optional object path: `/` means none.
fn optional(path: &str, kind: Kind) -> Result<Option<String>> {
    if path == "/" {
        return Ok(None);
    }
    ids::id_from_path(kind, path)
        .map(|id| Some(id.to_owned()))
        .ok_or_else(|| Error::invalid(format!("{path} is not a {} object path", kind.segment())))
}

fn derived_kind(name: &str) -> Result<DerivedKind> {
    match name {
        "maximum" => Ok(DerivedKind::Maximum),
        "mean" => Ok(DerivedKind::Mean),
        "difference" => Ok(DerivedKind::Difference),
        other => Err(Error::invalid(format!(
            "the Derived sensor type is maximum, mean or difference, not {other}"
        ))),
    }
}

pub fn derived_kind_name(kind: DerivedKind) -> &'static str {
    match kind {
        DerivedKind::Maximum => "maximum",
        DerivedKind::Mean => "mean",
        DerivedKind::Difference => "difference",
    }
}

fn check_name(name: &str) -> Result<()> {
    if name.chars().count() > MAX_NAME_CHARS {
        Err(Error::invalid(format!(
            "a name has at most {MAX_NAME_CHARS} characters"
        )))
    } else {
        Ok(())
    }
}

fn check_graph_unit(unit: &str) -> Result<()> {
    graph_sensor_kind(unit)
        .map(|_| ())
        .ok_or_else(|| Error::invalid(format!("{unit:?} is not a unit of a Graph sensor")))
}

fn check_point_count(what: &str, count: usize, max: usize) -> Result<()> {
    if (MIN_POINTS..=max).contains(&count) {
        Ok(())
    } else {
        Err(Error::invalid(format!(
            "{what} has {MIN_POINTS} to {max} points"
        )))
    }
}

fn check_count(what: &str, count: usize, max: usize) -> Result<()> {
    if count < max {
        Ok(())
    } else {
        Err(Error::invalid(format!("at most {max} {what}")))
    }
}

fn check_percent(what: &str, value: f64) -> Result<()> {
    if (0.0..=100.0).contains(&value) {
        Ok(())
    } else {
        Err(Error::invalid(format!("{what} is a percent from 0 to 100")))
    }
}

impl Inner {
    fn loop_flow_ids(&self) -> impl Iterator<Item = (&Loop, String)> {
        self.state
            .loops
            .iter()
            .filter(|l| l.flow_sensor.is_some() && l.calibration_table.is_some())
            .map(|l| (l, l.flow_id()))
    }

    fn sensor_ids(&self) -> Vec<String> {
        self.sensors
            .iter()
            .map(|s| s.id.clone())
            .chain(self.state.derived_sensors.iter().map(|d| d.id.clone()))
            .chain(self.state.graph_sensors.iter().map(|g| g.id.clone()))
            .chain(self.loop_flow_ids().map(|(_, id)| id))
            .collect()
    }

    fn sensor_exists(&self, id: &str) -> bool {
        self.sensors.iter().any(|s| s.id == id)
            || self.state.derived_sensors.iter().any(|d| d.id == id)
            || self.state.graph_sensors.iter().any(|g| g.id == id)
            || self.loop_flow_ids().any(|(_, f)| f == id)
    }

    fn is_graph_sensor(&self, id: &str) -> bool {
        self.state.graph_sensors.iter().any(|g| g.id == id)
    }

    /// `InvalidArgument` for a Graph sensor in a place that stays on measured values.
    fn refuse_graph_sensor(&self, id: &str, what: &str) -> Result<()> {
        if self.is_graph_sensor(id) {
            Err(Error::invalid(format!("{what} the Graph sensor {id}")))
        } else {
            Ok(())
        }
    }

    /// Every id that the stored state names for a sensor: the Graph sensors, the Curve Inputs of
    /// all Profiles, the sensors of the rules, the Software sensor slots and the user names. A
    /// downgrade drops the Graph sensors and the counter but keeps these, so a new Graph sensor
    /// must not take one of these ids.
    fn named_sensor_ids(&self) -> Vec<String> {
        let state = &self.state;
        let rules = state
            .rule_groups
            .iter()
            .flat_map(|g| &g.rules)
            .filter_map(|r| match r {
                nicecooling_core::rules::group::RuleDef::Sensor(s) => Some(s.sensor.clone()),
                _ => None,
            });
        let inputs = self
            .profiles
            .iter()
            .flat_map(|p| p.channels.values())
            .filter_map(|c| c.input.clone());
        let slots = state
            .software_sensors
            .values()
            .flat_map(|slots| slots.values().cloned());
        state
            .graph_sensors
            .iter()
            .map(|g| g.id.clone())
            .chain(inputs)
            .chain(rules)
            .chain(slots)
            .chain(state.names.keys().cloned())
            .collect()
    }

    /// The Software sensor slot that holds this sensor, or `None`.
    fn slot_user(&self, id: &str) -> Option<String> {
        self.state
            .software_sensors
            .iter()
            .find_map(|(device, slots)| {
                slots
                    .iter()
                    .find(|(_, sensor)| sensor.as_str() == id)
                    .map(|(slot, _)| format!("the Software sensor slot {slot} of {device}"))
            })
    }

    fn require_sensor(&self, id: &str) -> Result<()> {
        if self.sensor_exists(id) {
            Ok(())
        } else {
            Err(Error::not_found(format!("no sensor {id}")))
        }
    }

    fn require_channel(&self, id: &str) -> Result<&ChannelMeta> {
        self.channels
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| Error::not_found(format!("no Channel {id}")))
    }

    /// A Channel without a BIOS snapshot entry has no state to go back to. The daemon does not
    /// take it over. Without any snapshot the Channels are not told apart.
    fn has_snapshot_entry(&self, meta: &ChannelMeta) -> bool {
        self.snapshot
            .as_ref()
            .is_none_or(|s| s.channel(&meta.device, meta.number).is_some())
    }

    /// A Channel that a Curve or a fixed Duty can control.
    fn require_controllable(&self, id: &str) -> Result<&ChannelMeta> {
        let meta = self.require_channel(id)?;
        if self.has_snapshot_entry(meta) {
            Ok(meta)
        } else {
            Err(Error::read_only("no-bios-snapshot"))
        }
    }

    fn loop_mut(&mut self, id: &str) -> Result<&mut Loop> {
        self.state
            .loops
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or_else(|| Error::not_found(format!("no Loop {id}")))
    }

    fn table_exists(&self, id: &str) -> bool {
        self.presets.iter().any(|p| p.id == id)
            || self.state.calibration_tables.iter().any(|t| t.id == id)
    }

    /// What reads this sensor, or `None`.
    fn sensor_user(&self, id: &str) -> Option<String> {
        for profile in self.profiles.iter() {
            if let Some((channel, _)) = profile
                .channels
                .iter()
                .find(|(_, c)| c.control == ControlKind::Curve && c.input.as_deref() == Some(id))
            {
                return Some(format!("the Curve of {channel} in {}", profile.id));
            }
        }
        for l in &self.state.loops {
            if l.flow_sensor.as_deref() == Some(id) || l.water_temperature.as_deref() == Some(id) {
                return Some(format!("the Loop {}", l.id));
            }
        }
        if let Some(group) = self.state.rule_groups.iter().find(|g| {
            g.rules.iter().any(
                |r| matches!(r, nicecooling_core::rules::group::RuleDef::Sensor(s) if s.sensor == id),
            )
        }) {
            return Some(format!("a rule of the Rule group {}", group.id));
        }
        if let Some(derived) = self
            .state
            .derived_sensors
            .iter()
            .find(|d| d.sources.iter().any(|s| s == id))
        {
            return Some(format!("the Derived sensor {}", derived.id));
        }
        self.slot_user(id)
    }

    fn name_of(&self, id: &str, default: &str) -> String {
        self.state
            .names
            .get(id)
            .cloned()
            .unwrap_or_else(|| default.to_owned())
    }

    fn kind_unit(&self, id: &str, depth: u8) -> (String, String) {
        if let Some(s) = self.sensors.iter().find(|s| s.id == id) {
            return (s.kind.to_owned(), s.unit.to_owned());
        }
        if let Some(g) = self.state.graph_sensors.iter().find(|g| g.id == id) {
            let kind = graph_sensor_kind(&g.unit).unwrap_or("other");
            return (kind.to_owned(), g.unit.clone());
        }
        if let Some(d) = self.state.derived_sensors.iter().find(|d| d.id == id) {
            if let (Some(first), true) = (d.sources.first(), depth < 8) {
                return self.kind_unit(first, depth + 1);
            }
            return ("other".into(), String::new());
        }
        ("flow".into(), "l/h".into())
    }

    /// Puts the stored Software sensors into the slots of a Leakshield. A sensor that does not
    /// exist (now) or has a unit without a device unit code leaves its slot empty.
    fn restore_software_sensors(&self, device: &LeakshieldInfo) {
        let slots = device.software_slots();
        for (slot, scaling) in self
            .state
            .software_sensor_scales
            .get(&device.id())
            .into_iter()
            .flatten()
        {
            let _ = slots.set_scale(*slot, scaling.scale, scaling.offset);
        }
        let Some(stored) = self.state.software_sensors.get(&device.id()) else {
            return;
        };
        for (slot, sensor) in stored {
            let assignment = self
                .sensor_view(sensor)
                .and_then(|view| Assignment::new(sensor, &view.unit));
            if assignment.is_some() {
                let _ = slots.assign(*slot, assignment);
            }
        }
    }

    fn sensor_view(&self, id: &str) -> Option<SensorView> {
        let value = self.live.values.get(id).copied();
        if let Some(meta) = self.sensors.iter().find(|s| s.id == id) {
            return Some(SensorView {
                id: id.to_owned(),
                name: self.name_of(id, &meta.name),
                default_name: meta.name.clone(),
                kind: meta.kind.to_owned(),
                unit: meta.unit.to_owned(),
                origin: if meta.driver.is_empty() {
                    "leakshield"
                } else {
                    "hwmon"
                },
                driver: meta.driver.clone(),
                value,
            });
        }
        if let Some(derived) = self.state.derived_sensors.iter().find(|d| d.id == id) {
            let (kind, unit) = self.kind_unit(id, 0);
            return Some(SensorView {
                id: id.to_owned(),
                name: self.name_of(id, &derived.name),
                default_name: derived.name.clone(),
                kind,
                unit,
                origin: "derived",
                driver: String::new(),
                value,
            });
        }
        if let Some(graph) = self.state.graph_sensors.iter().find(|g| g.id == id) {
            let (kind, unit) = self.kind_unit(id, 0);
            return Some(SensorView {
                id: id.to_owned(),
                name: self.name_of(id, &graph.name),
                default_name: graph.name.clone(),
                kind,
                unit,
                origin: "graph",
                driver: String::new(),
                value,
            });
        }
        let (l, _) = self.loop_flow_ids().find(|(_, f)| f == id)?;
        let default_name = format!("{} flow", l.name);
        Some(SensorView {
            id: id.to_owned(),
            name: self.name_of(id, &default_name),
            default_name,
            kind: "flow".into(),
            unit: "l/h".into(),
            origin: "calibrated",
            driver: String::new(),
            value,
        })
    }

    /// The active Profile. A Curve change goes into it.
    fn profile(&self) -> &ProfileFile {
        self.profiles
            .get(&self.state.profiles.active)
            .unwrap_or_else(|| self.profiles.default_profile())
    }

    fn profile_mut(&mut self) -> &mut ProfileFile {
        let id = if self.profiles.contains(&self.state.profiles.active) {
            self.state.profiles.active.clone()
        } else {
            DEFAULT_PROFILE.to_owned()
        };
        self.profiles
            .get_mut(&id)
            .expect("the Default Profile always exists")
    }

    fn pump_floor(&self, l: &Loop) -> f64 {
        self.profile()
            .pump_floors
            .get(&l.id)
            .copied()
            .unwrap_or(l.pump_floor)
    }

    fn loop_view(&self, id: &str) -> Option<LoopView> {
        let l = self.state.loops.iter().find(|l| l.id == id)?;
        Some(LoopView {
            id: l.id.clone(),
            name: l.name.clone(),
            pump_channels: l.pump_channels.clone(),
            flow_sensor: l.flow_sensor.clone(),
            calibration_table: l.calibration_table.clone(),
            flow: (l.flow_sensor.is_some() && l.calibration_table.is_some()).then(|| l.flow_id()),
            water_temperature: l.water_temperature.clone(),
            leakshield: l.leakshield.clone(),
            pump_floor: self.pump_floor(l),
        })
    }

    fn channel_view(&self, id: &str) -> Option<ChannelView> {
        let meta = self.channels.iter().find(|c| c.id == id)?;
        let entry = self
            .profile()
            .channels
            .get(id)
            .filter(|_| self.has_snapshot_entry(meta));
        let control = match entry.map(|e| e.control) {
            Some(ControlKind::Curve) => "curve",
            Some(ControlKind::Fixed) => "fixed",
            _ => "hardware",
        };
        let live = self.live.channels.get(id).cloned().unwrap_or_default();
        let curve = entry.filter(|e| e.control == ControlKind::Curve);
        Some(ChannelView {
            id: id.to_owned(),
            name: self.name_of(id, &meta.default_name),
            default_name: meta.default_name.clone(),
            duty: live.duty,
            raw: live.raw,
            control,
            override_state: if live.override_state.is_empty() {
                "none"
            } else {
                live.override_state
            },
            is_pump: self
                .state
                .loops
                .iter()
                .any(|l| l.pump_channels.iter().any(|c| c == id)),
            manual_at_snapshot: self.live.manual_at_snapshot.contains(id),
            curve_input: curve.and_then(|c| c.input.clone()).unwrap_or_default(),
            curve_points: curve.map(|c| c.points.clone()).unwrap_or_default(),
            smoothing_s: curve
                .and_then(|c| c.smoothing_s)
                .unwrap_or(DEFAULT_SMOOTHING_S),
            step_limit: curve
                .and_then(|c| c.step_limit_pct_per_s)
                .unwrap_or(DEFAULT_STEP_LIMIT),
            fixed_duty: entry.and_then(|e| e.fixed_duty).unwrap_or(0.0),
            bios_snapshot: self.snapshot.as_ref().map_or((0, Vec::new()), |s| {
                (
                    s.taken_us,
                    s.channel(&meta.device, meta.number)
                        .map(|c| c.attributes.clone())
                        .unwrap_or_default(),
                )
            }),
            fan_sensor: tach_sensor(id).filter(|fan| self.sensor_exists(fan)),
        })
    }

    /// Orders the Derived sensors so that a sensor comes after the Derived sensors it reads.
    fn sort_derived(&mut self) {
        let mut sorted: Vec<DerivedSensor> = Vec::new();
        let mut rest = std::mem::take(&mut self.state.derived_sensors);
        while !rest.is_empty() {
            let ready = rest.iter().position(|d| {
                d.sources
                    .iter()
                    .all(|s| !rest.iter().any(|other| &other.id == s))
            });
            // A cycle cannot come in: the checks of a write refuse it.
            sorted.push(rest.remove(ready.unwrap_or(0)));
        }
        self.state.derived_sensors = sorted;
    }

    /// True when `from` reads `target`, directly or through other Derived sensors.
    fn derived_reads(&self, from: &str, target: &str, depth: u8) -> bool {
        if depth > 16 {
            return true;
        }
        self.state
            .derived_sensors
            .iter()
            .find(|d| d.id == from)
            .is_some_and(|d| {
                d.sources
                    .iter()
                    .any(|s| s == target || self.derived_reads(s, target, depth + 1))
            })
    }

    fn check_derived(
        &self,
        own_id: Option<&str>,
        kind: DerivedKind,
        sources: &[String],
    ) -> Result<()> {
        let count_ok = match kind {
            DerivedKind::Difference => sources.len() == 2,
            _ => !sources.is_empty(),
        };
        if !count_ok {
            return Err(Error::invalid(
                "a difference needs 2 sources, a maximum or a mean at least 1",
            ));
        }
        for source in sources {
            self.require_sensor(source)?;
            // A Graph could read this Derived sensor and close a loop that no validator sees.
            self.refuse_graph_sensor(source, "a Derived sensor cannot read")?;
            if let Some(own) = own_id
                && (source == own || self.derived_reads(source, own, 0))
            {
                return Err(Error::invalid("a Derived sensor cannot read itself"));
            }
        }
        Ok(())
    }
}

impl Hub {
    pub fn new(init: HubInit) -> Arc<Self> {
        let store = Arc::new(init.store);
        let inner = Arc::new(Mutex::new(Inner {
            state: init.state,
            profiles: init.profiles,
            deleted: BTreeSet::new(),
            active_state: "applied",
            activation: 0,
            rule_queue: rules::RuleQueue::first(),
            rule_states: Vec::new(),
            dwell_until_us: 0,
            presets: init.presets,
            board: init.board,
            sensors: init.sensors,
            channels: init.channels,
            leakshields: init.leakshields,
            state_health: init.state_health,
            snapshot: snapshot::read_snapshot(&store),
            live: Live::default(),
        }));
        {
            let inner = lock(&inner);
            for device in &inner.leakshields {
                inner.restore_software_sensors(device);
            }
        }
        let persister = Persister::spawn(PERSIST_DELAY, {
            let (inner, store) = (inner.clone(), store.clone());
            move || flush(&inner, &store)
        });
        let started_us = wall_us();
        let history = HistoryStore::open(store.clone(), started_us, SAVE_EVERY);
        let (reports, report_rx) = mpsc::channel(32);
        let (alarm_effects, alarm_rx) = mpsc::unbounded_channel();
        let (profile_events, profile_rx) = mpsc::unbounded_channel();
        Arc::new(Self {
            inner,
            slots: SlotStore::new(store.clone()),
            store,
            sysfs_root: init.sysfs_root,
            persister,
            authz: OnceLock::new(),
            connection: OnceLock::new(),
            pending: Mutex::new(None),
            releases: Mutex::new(Vec::new()),
            reports,
            report_rx: Mutex::new(Some(report_rx)),
            profile_events,
            profile_rx: Mutex::new(Some(profile_rx)),
            alarms: Mutex::new(AlarmManager::new(started_us)),
            history,
            alarm_effects,
            alarm_rx: Mutex::new(Some(alarm_rx)),
            login1: OnceLock::new(),
            values: init.values,
            clock: init.clock,
            runtime: tokio::runtime::Handle::current(),
        })
    }

    fn inner(&self) -> MutexGuard<'_, Inner> {
        lock(&self.inner)
    }

    /// Runs a write. When it succeeds, the tick configuration is rebuilt and the state is
    /// persisted 2 s after the last write.
    fn edit<R>(&self, f: impl FnOnce(&mut Inner) -> Result<R>) -> Result<R> {
        let mut inner = self.inner();
        let result = f(&mut inner)?;
        self.queue_config(&inner);
        drop(inner);
        self.persister.changed();
        Ok(result)
    }

    /// Runs a change of the state that needs no new tick configuration, and persists it.
    fn change<R>(&self, f: impl FnOnce(&mut Inner) -> R) -> R {
        let result = f(&mut self.inner());
        self.persister.changed();
        result
    }

    /// Builds the tick configuration of the active Profile and queues it for the tick.
    fn queue_config(&self, inner: &Inner) -> TickConfig {
        let mut problems = Vec::new();
        let config = tick_config(&inner.state, inner.profile(), &inner.presets, &mut problems);
        for problem in problems {
            eprintln!("nicecoolingd: {problem}");
        }
        *lock(&self.pending) = Some(config.clone());
        config
    }

    /// Adds the sensors of a chip that appeared after the start, remembers the ids of the chip,
    /// and registers their D-Bus objects when the API is on a bus.
    pub fn add_sensors(
        self: &Arc<Self>,
        sensors: Vec<SensorMeta>,
        identities: std::collections::BTreeMap<String, crate::state::Identity>,
    ) {
        let ids: Vec<String> = sensors.iter().map(|s| s.id.clone()).collect();
        {
            let mut inner = self.inner();
            inner.sensors.extend(sensors);
            inner.state.identities.extend(identities);
        }
        self.persister.changed();
        if let Some(connection) = self.connection.get() {
            let (hub, connection) = (self.clone(), connection.clone());
            self.runtime.spawn(async move {
                for id in ids {
                    if let Err(e) =
                        super::register_sensor(connection.object_server(), &hub, &id).await
                    {
                        eprintln!("nicecoolingd: cannot register the sensor {id}: {e}");
                    }
                }
            });
        }
    }

    /// Adds a Leakshield that attached after the start, with its sensors, and registers their
    /// D-Bus objects when the API is on a bus.
    pub fn add_leakshield(self: &Arc<Self>, device: LeakshieldInfo) {
        let sensors = crate::sources::leakshield::sensor_metas(&device.id());
        let ids: Vec<String> = sensors.iter().map(|s| s.id.clone()).collect();
        {
            let mut inner = self.inner();
            inner.sensors.extend(sensors);
            inner.leakshields.push(device.clone());
            inner.restore_software_sensors(&device);
        }
        if let Some(connection) = self.connection.get() {
            let (hub, connection) = (self.clone(), connection.clone());
            self.runtime.spawn(async move {
                for id in ids {
                    if let Err(e) =
                        super::register_sensor(connection.object_server(), &hub, &id).await
                    {
                        eprintln!("nicecoolingd: cannot register the sensor {id}: {e}");
                    }
                }
                if let Err(e) = super::leakshield::register(&connection, &hub, &device).await {
                    eprintln!(
                        "nicecoolingd: cannot register the Leakshield {}: {e}",
                        device.id()
                    );
                }
            });
        }
    }

    /// Records a Software sensor slot in `state.json`. `None` clears the slot.
    pub fn remember_software_sensor(&self, leakshield: &str, slot: u16, sensor: Option<&str>) {
        {
            let mut inner = self.inner();
            let slots = inner
                .state
                .software_sensors
                .entry(leakshield.to_owned())
                .or_default();
            match sensor {
                Some(sensor) => slots.insert(slot, sensor.to_owned()),
                None => slots.remove(&slot),
            };
            if slots.is_empty() {
                inner.state.software_sensors.remove(leakshield);
            }
        }
        self.persister.changed();
    }

    /// Records the scale and the offset of a Software sensor slot in `state.json`. The default
    /// (scale 1, offset 0) removes the entry.
    pub fn remember_software_scale(&self, leakshield: &str, slot: u16, scale: f64, offset: f64) {
        {
            let mut inner = self.inner();
            let slots = inner
                .state
                .software_sensor_scales
                .entry(leakshield.to_owned())
                .or_default();
            if scale == 1.0 && offset == 0.0 {
                slots.remove(&slot);
            } else {
                slots.insert(slot, SlotScale { scale, offset });
            }
            if slots.is_empty() {
                inner.state.software_sensor_scales.remove(leakshield);
            }
        }
        self.persister.changed();
    }

    /// The sensors that a Software sensor slot can carry: those whose unit has a device unit code.
    pub fn software_sensor_sources(&self) -> Vec<String> {
        let inner = self.inner();
        inner
            .sensor_ids()
            .into_iter()
            .filter(|id| {
                inner
                    .sensor_view(id)
                    .is_some_and(|view| Assignment::new(id, &view.unit).is_some())
            })
            .collect()
    }

    pub fn set_connection(&self, connection: zbus::Connection) {
        let _ = self.connection.set(connection);
    }

    /// Writes the state files now. The daemon calls it before it exits.
    pub fn flush_now(&self) {
        flush(&self.inner, &self.store);
        self.history.save_now();
    }

    pub fn spawn(&self, future: impl std::future::Future<Output = ()> + Send + 'static) {
        self.runtime.spawn(future);
    }

    pub fn set_authz(&self, authz: crate::authz::Authz) {
        let _ = self.authz.set(authz);
    }

    pub async fn authorize(
        &self,
        connection: &zbus::Connection,
        header: &zbus::message::Header<'_>,
        interface: &str,
        method: &str,
        mode: Option<&str>,
    ) -> Result<()> {
        let authz = self
            .authz
            .get()
            .ok_or_else(|| failed("the API has no authorizer"))?;
        authz
            .check(connection, header, interface, method, mode)
            .await
    }

    /// Like `authorize`, but polkit never shows a dialog, whatever flag the message has.
    pub async fn authorize_without_dialog(
        &self,
        connection: &zbus::Connection,
        header: &zbus::message::Header<'_>,
        interface: &str,
        method: &str,
    ) -> Result<()> {
        let authz = self
            .authz
            .get()
            .ok_or_else(|| failed("the API has no authorizer"))?;
        authz
            .check_without_dialog(connection, header, interface, method)
            .await
    }

    /// The logind that the poweroff calls. The first call wins.
    pub fn set_login1(&self, login1: Arc<dyn Login1>) {
        let _ = self.login1.set(login1);
    }

    pub fn login1(&self) -> Option<Arc<dyn Login1>> {
        self.login1.get().cloned()
    }

    pub fn take_alarm_receiver(&self) -> Option<mpsc::UnboundedReceiver<Effect>> {
        lock(&self.alarm_rx).take()
    }

    /// Feeds the Warnings and Alarms of a tick to the alarm manager.
    pub fn alarm_tick(&self, now: Duration, wall_us: u64, active: &[Event]) {
        let responses = self.inner().state.alarm_responses.clone();
        let effects = lock(&self.alarms).update(now, wall_us, active, &responses);
        self.run_alarm_effects(effects, wall_us);
    }

    /// Records the effects in the history, then hands them to the task that emits them. A change
    /// to `poweroff-requested` is on disk before the call to logind starts.
    fn run_alarm_effects(&self, effects: Vec<Effect>, wall_us: u64) {
        let rows = lock(&self.alarms).rows();
        self.history.apply(&effects, &rows, wall_us);
        for effect in effects {
            let _ = self.alarm_effects.send(effect);
        }
    }

    /// The history rows that were active at or after `since_us`, newest first (`GetHistory`).
    pub fn alarm_history(&self, since_us: u64, limit: u32) -> Vec<HistoryRow> {
        self.history.query(since_us, limit)
    }

    pub fn alarm_rows(&self) -> Vec<Row> {
        lock(&self.alarms).rows()
    }

    pub fn poweroff_result(&self, instance: &str, result: std::result::Result<(), String>) {
        let effects = lock(&self.alarms).poweroff_result(instance, result);
        self.run_alarm_effects(effects, wall_us());
    }

    /// The ids of the Leakshields that the daemon knows, connected or not, in id order.
    fn leakshield_ids(inner: &Inner) -> Vec<String> {
        let mut ids: Vec<String> = inner.leakshields.iter().map(|d| d.id()).collect();
        ids.sort();
        ids
    }

    /// The rows of `GetResponses`.
    pub fn alarm_responses(&self) -> Vec<response::Row> {
        let inner = self.inner();
        response::rows(&inner.state.alarm_responses, &Self::leakshield_ids(&inner))
    }

    /// The response of each event for one Leakshield, as an instance of it would get it now.
    pub fn alarm_responses_for(&self, leakshield: &str) -> Vec<(String, String, u32)> {
        response::resolved_for(&self.inner().state.alarm_responses, leakshield)
    }

    /// An alarm response applies to the Alarms that rise after it. Source `""` sets the default of
    /// the event.
    pub fn set_alarm_response(
        &self,
        event: &str,
        source: &str,
        response: &str,
        delay_s: u32,
    ) -> Result<()> {
        let mut inner = self.inner();
        let known = Self::leakshield_ids(&inner);
        let (key, entry) = response::validate(event, source, response, delay_s, |source| {
            known.iter().any(|id| id == source)
        })?;
        inner.state.alarm_responses.insert(key, entry);
        drop(inner);
        self.persister.changed();
        Ok(())
    }

    /// Source `""` sets the default of the event back to the built-in one. A source removes its
    /// override, also the one of a Leakshield that no longer exists.
    pub fn reset_alarm_response(&self, event: &str, source: &str) -> Result<()> {
        let key = response::reset_key(event, source)?;
        self.inner().state.alarm_responses.remove(&key);
        self.persister.changed();
        Ok(())
    }

    /// `NoPoweroff` when the instance has no pending poweroff, `PoweroffStarted` when the call
    /// to logind is under way or done.
    pub fn cancel_poweroff(&self, instance: &str) -> Result<()> {
        let effect = lock(&self.alarms).cancel(instance).map_err(|e| match e {
            CancelError::NoPoweroff => {
                Error::NoPoweroff(format!("{instance} has no pending poweroff"))
            }
            CancelError::PoweroffStarted => Error::PoweroffStarted(format!(
                "{instance} already called logind: the PC powers off"
            )),
        })?;
        self.run_alarm_effects(vec![effect], wall_us());
        Ok(())
    }

    pub fn take_profile_receiver(&self) -> Option<mpsc::UnboundedReceiver<ProfileEvent>> {
        lock(&self.profile_rx).take()
    }

    pub fn take_report_receiver(&self) -> Option<mpsc::Receiver<TickReport>> {
        lock(&self.report_rx).take()
    }

    // The tick side.

    pub fn take_pending_config(&self) -> Option<TickConfig> {
        lock(&self.pending).take()
    }

    pub fn take_releases(&self) -> Vec<Release> {
        std::mem::take(&mut *lock(&self.releases))
    }

    pub fn channel_metas(&self) -> Vec<ChannelMeta> {
        self.inner().channels.clone()
    }

    pub fn sensor_ids(&self) -> Vec<String> {
        self.inner().sensor_ids()
    }

    /// A report that finds the queue full is dropped: nobody reads the signals.
    pub fn publish(&self, live: Live, report: TickReport) {
        self.inner().live = live;
        let _ = self.reports.try_send(report);
    }

    /// Writes the BIOS snapshot attributes of one Channel back.
    fn restore_channel(&self, id: &str) -> std::result::Result<(), String> {
        let (meta, snapshot) = {
            let inner = self.inner();
            let meta = inner
                .require_channel(id)
                .map_err(|e| e.to_string())?
                .clone();
            (meta, inner.snapshot.clone())
        };
        let snapshot = snapshot.ok_or_else(|| "there is no BIOS snapshot".to_owned())?;
        let Some(channel) = snapshot.channel(&meta.device, meta.number) else {
            return Err(format!("the BIOS snapshot has no entry for {id}"));
        };
        let one = BiosSnapshot {
            chips: vec![snapshot::ChipSnapshot {
                hwmon_name: meta.device.hwmon_name.clone(),
                device_path: meta.device.device_path.clone(),
                channels: vec![channel.clone()],
            }],
            ..snapshot.clone()
        };
        let report = snapshot::restore(&Sysfs::new(&self.sysfs_root), &one);
        if report.matched {
            Ok(())
        } else {
            Err(report.problems.join("; "))
        }
    }

    // Reads.

    pub fn version(&self) -> &'static str {
        env!("CARGO_PKG_VERSION")
    }

    pub fn state_health(&self) -> String {
        self.inner().state_health.clone()
    }

    pub fn board(&self) -> BoardInfo {
        self.inner().board.clone()
    }

    pub fn sensor(&self, id: &str) -> Option<SensorView> {
        self.inner().sensor_view(id)
    }

    pub fn derived(&self, id: &str) -> Option<(&'static str, Vec<String>)> {
        self.inner()
            .state
            .derived_sensors
            .iter()
            .find(|d| d.id == id)
            .map(|d| (derived_kind_name(d.kind), d.sources.clone()))
    }

    pub fn derived_ids(&self) -> Vec<String> {
        self.inner()
            .state
            .derived_sensors
            .iter()
            .map(|d| d.id.clone())
            .collect()
    }

    pub fn graph_sensor_ids(&self) -> Vec<String> {
        self.inner()
            .state
            .graph_sensors
            .iter()
            .map(|g| g.id.clone())
            .collect()
    }

    pub fn is_graph_sensor(&self, id: &str) -> bool {
        self.inner().is_graph_sensor(id)
    }

    pub fn hwmon_sensor_ids(&self) -> Vec<String> {
        self.inner().sensors.iter().map(|s| s.id.clone()).collect()
    }

    pub fn flow_sensor_ids(&self) -> Vec<String> {
        self.inner().loop_flow_ids().map(|(_, id)| id).collect()
    }

    pub fn channel(&self, id: &str) -> Option<ChannelView> {
        self.inner().channel_view(id)
    }

    pub fn channel_ids(&self) -> Vec<String> {
        self.inner().channels.iter().map(|c| c.id.clone()).collect()
    }

    pub fn loop_view(&self, id: &str) -> Option<LoopView> {
        self.inner().loop_view(id)
    }

    pub fn loop_ids(&self) -> Vec<String> {
        self.inner()
            .state
            .loops
            .iter()
            .map(|l| l.id.clone())
            .collect()
    }

    pub fn leakshields(&self) -> Vec<LeakshieldInfo> {
        self.inner().leakshields.clone()
    }

    pub fn profiles_state(&self) -> crate::state::ProfilesState {
        self.inner().state.profiles.clone()
    }

    pub fn calibration_tables(&self) -> Vec<CalibrationRow> {
        let inner = self.inner();
        inner
            .presets
            .iter()
            .map(|p| {
                (
                    p.id.clone(),
                    p.name.clone(),
                    true,
                    p.table.points().to_vec(),
                )
            })
            .chain(
                inner
                    .state
                    .calibration_tables
                    .iter()
                    .map(|t| (t.id.clone(), t.name.clone(), false, t.points.clone())),
            )
            .collect()
    }

    // Writes.

    pub fn set_sensor_name(&self, id: &str, name: &str) -> Result<()> {
        check_name(name)?;
        self.edit(|i| {
            i.require_sensor(id)?;
            set_name(&mut i.state.names, id, name);
            Ok(())
        })
    }

    /// Returns the id of the new Derived sensor.
    pub fn create_derived(&self, name: &str, kind: &str, sources: &[String]) -> Result<String> {
        let kind = derived_kind(kind)?;
        check_name(name)?;
        self.edit(|i| {
            check_count(
                "Derived sensors",
                i.state.derived_sensors.len(),
                MAX_DERIVED_SENSORS,
            )?;
            i.check_derived(None, kind, sources)?;
            let id = ids::derived_sensor_id(ids::next_number(
                "derived_",
                i.state.derived_sensors.iter().map(|d| d.id.as_str()),
            ));
            i.state.derived_sensors.push(DerivedSensor {
                id: id.clone(),
                name: if name.is_empty() {
                    id.clone()
                } else {
                    name.to_owned()
                },
                kind,
                sources: sources.to_vec(),
            });
            i.sort_derived();
            Ok(id)
        })
    }

    pub fn set_derived_definition(&self, id: &str, kind: &str, sources: &[String]) -> Result<()> {
        let kind = derived_kind(kind)?;
        self.edit(|i| {
            if !i.state.derived_sensors.iter().any(|d| d.id == id) {
                return Err(Error::not_found(format!("no Derived sensor {id}")));
            }
            i.check_derived(Some(id), kind, sources)?;
            if let Some(d) = i.state.derived_sensors.iter_mut().find(|d| d.id == id) {
                d.kind = kind;
                d.sources = sources.to_vec();
            }
            i.sort_derived();
            Ok(())
        })
    }

    pub fn delete_derived(&self, id: &str) -> Result<()> {
        self.edit(|i| {
            if !i.state.derived_sensors.iter().any(|d| d.id == id) {
                return Err(Error::not_found(format!("no Derived sensor {id}")));
            }
            if let Some(user) = i.sensor_user(id) {
                return Err(Error::in_use(format!("{id} is read by {user}")));
            }
            i.state.derived_sensors.retain(|d| d.id != id);
            i.state.names.remove(id);
            Ok(())
        })
    }

    /// Returns the id of the new Graph sensor. An empty name gives the id as the default name.
    pub fn create_graph_sensor(&self, name: &str, unit: &str) -> Result<String> {
        check_name(name)?;
        check_graph_unit(unit)?;
        self.edit(|i| {
            check_count(
                "Graph sensors",
                i.state.graph_sensors.len(),
                MAX_GRAPH_SENSORS,
            )?;
            let named = i.named_sensor_ids();
            let number = ids::take_number(
                "graph_",
                &mut i.state.graph_sensor_next,
                named.iter().map(String::as_str),
            )
            .ok_or_else(|| Error::invalid("no Graph sensor id is left"))?;
            let id = ids::graph_sensor_id(number);
            i.state.graph_sensors.push(GraphSensorDef {
                id: id.clone(),
                name: if name.is_empty() {
                    id.clone()
                } else {
                    name.to_owned()
                },
                unit: unit.to_owned(),
            });
            Ok(id)
        })
    }

    /// Changes the default name and the unit. A name that the user set stays in `names`. A new unit
    /// clears the value: the app measured it in the old unit.
    pub fn set_graph_definition(&self, id: &str, name: &str, unit: &str) -> Result<()> {
        check_name(name)?;
        check_graph_unit(unit)?;
        let unit_changed = self.edit(|i| {
            let Some(current) = i.state.graph_sensors.iter().find(|g| g.id == id) else {
                return Err(Error::not_found(format!("no Graph sensor {id}")));
            };
            let unit_changed = current.unit != unit;
            if unit_changed && let Some(user) = i.slot_user(id) {
                // The slot holds the unit code of the device.
                return Err(Error::in_use(format!("{id} is held by {user}")));
            }
            if let Some(graph) = i.state.graph_sensors.iter_mut().find(|g| g.id == id) {
                graph.name = if name.is_empty() {
                    id.to_owned()
                } else {
                    name.to_owned()
                };
                graph.unit = unit.to_owned();
            }
            Ok(unit_changed)
        })?;
        if unit_changed {
            self.values.clear(id);
        }
        Ok(())
    }

    pub fn delete_graph_sensor(&self, id: &str) -> Result<()> {
        self.edit(|i| {
            if !i.is_graph_sensor(id) {
                return Err(Error::not_found(format!("no Graph sensor {id}")));
            }
            if let Some(user) = i.sensor_user(id) {
                return Err(Error::in_use(format!("{id} is read by {user}")));
            }
            i.state.graph_sensors.retain(|g| g.id != id);
            i.state.names.remove(id);
            Ok(())
        })?;
        self.values.clear(id);
        Ok(())
    }

    /// The values call of the app. An entry with a value puts it in the value table with the time
    /// of the tick clock. An entry without a value removes the value, so the next tick reads none.
    /// An entry for an id that is not a Graph sensor is skipped: the app cannot write the value of
    /// a measured, Derived or calibrated sensor. A number that is not finite fails the whole call
    /// before it changes anything, also in an entry that is skipped or has no value: a Curve never
    /// gets a number that no sensor measured.
    pub fn set_graph_sensor_values(&self, entries: &[(String, bool, f64)]) -> Result<()> {
        if let Some((id, _, value)) = entries.iter().find(|(_, _, value)| !value.is_finite()) {
            return Err(Error::invalid(format!(
                "the value {value} of {id} is not a finite number"
            )));
        }
        let graph_ids: BTreeSet<String> = self.graph_sensor_ids().into_iter().collect();
        let now = self.clock.now();
        for (id, has_value, value) in entries {
            if !graph_ids.contains(id) {
                continue;
            }
            if *has_value {
                self.values.publish_from_app(id, *value, now);
            } else {
                self.values.clear(id);
            }
        }
        // A Delete that ran during the loop cleared its value before a later publish of this call.
        let left: BTreeSet<String> = self.graph_sensor_ids().into_iter().collect();
        for id in graph_ids.difference(&left) {
            self.values.clear(id);
        }
        Ok(())
    }

    pub fn set_channel_name(&self, id: &str, name: &str) -> Result<()> {
        check_name(name)?;
        self.edit(|i| {
            i.require_channel(id)?;
            set_name(&mut i.state.names, id, name);
            Ok(())
        })
    }

    pub fn set_curve(
        &self,
        id: &str,
        input: &str,
        points: Vec<(f64, f64)>,
        smoothing_s: f64,
        step_limit: f64,
    ) -> Result<()> {
        let input = sensor_id_of(input)?;
        check_point_count("a Curve", points.len(), MAX_CURVE_POINTS)?;
        Curve::new(input.clone(), points.clone())
            .and_then(|mut curve| curve.set_limits(Some(smoothing_s), Some(step_limit)))
            .map_err(|e| Error::invalid(e.to_string()))?;
        self.edit(|i| {
            i.require_controllable(id)?;
            i.require_sensor(&input)?;
            i.profile_mut().channels.insert(
                id.to_owned(),
                ProfileChannel {
                    control: ControlKind::Curve,
                    input: Some(input),
                    points,
                    smoothing_s: Some(smoothing_s),
                    step_limit_pct_per_s: Some(step_limit),
                    fixed_duty: None,
                },
            );
            Ok(())
        })
    }

    pub fn set_fixed_duty(&self, id: &str, duty: f64) -> Result<()> {
        check_percent("a Duty", duty)?;
        self.edit(|i| {
            i.require_controllable(id)?;
            i.profile_mut().channels.insert(
                id.to_owned(),
                ProfileChannel {
                    control: ControlKind::Fixed,
                    input: None,
                    points: Vec::new(),
                    smoothing_s: None,
                    step_limit_pct_per_s: None,
                    fixed_duty: Some(duty),
                },
            );
            Ok(())
        })
    }

    /// Gives the Channel back to its Hardware curve. The tick restores the BIOS snapshot
    /// attributes and then commits the change of control: a failed restore leaves the control
    /// as it was.
    pub async fn release_channel(&self, id: &str) -> Result<()> {
        {
            let inner = self.inner();
            inner.require_channel(id)?;
            if inner
                .channel_view(id)
                .is_some_and(|v| v.control == "hardware")
            {
                return Ok(());
            }
        }
        let (tx, rx) = oneshot::channel();
        lock(&self.releases).push((id.to_owned(), tx));
        let wait = self
            .runtime
            .spawn(async move { tokio::time::timeout(Duration::from_secs(5), rx).await });
        match wait.await {
            Ok(Ok(Ok(Ok(())))) => Ok(()),
            Ok(Ok(Ok(Err(e)))) => Err(failed(format!(
                "the BIOS snapshot restore of {id} failed: {e}"
            ))),
            _ => Err(failed("the tick did not restore the Channel in time")),
        }
    }

    /// The tick side of `release_channel`: restores the Channel, then drops its control.
    pub fn release_now(&self, id: &str) -> std::result::Result<(), String> {
        self.restore_channel(id)?;
        self.edit(|i| {
            i.profile_mut().channels.remove(id);
            Ok(())
        })
        .map_err(|e| e.to_string())
    }

    pub fn create_loop(&self, name: &str) -> Result<String> {
        check_name(name)?;
        self.edit(|i| {
            check_count("Loops", i.state.loops.len(), MAX_LOOPS)?;
            let id = ids::loop_id(ids::next_number(
                "loop_",
                i.state.loops.iter().map(|l| l.id.as_str()),
            ));
            i.state.loops.push(Loop {
                id: id.clone(),
                name: if name.is_empty() {
                    id.clone()
                } else {
                    name.to_owned()
                },
                pump_channels: Vec::new(),
                flow_sensor: None,
                calibration_table: None,
                water_temperature: None,
                leakshield: None,
                pump_floor: DEFAULT_PUMP_FLOOR,
            });
            Ok(id)
        })
    }

    pub fn set_loop_name(&self, id: &str, name: &str) -> Result<()> {
        check_name(name)?;
        self.edit(|i| {
            i.loop_mut(id)?.name = name.to_owned();
            Ok(())
        })
    }

    pub fn set_pump_channels(&self, id: &str, channels: &[String]) -> Result<()> {
        self.edit(|i| {
            i.loop_mut(id)?;
            for (n, channel) in channels.iter().enumerate() {
                i.require_channel(channel)?;
                if channels[..n].contains(channel) {
                    return Err(Error::invalid(format!("{channel} is listed twice")));
                }
                if let Some(other) = i
                    .state
                    .loops
                    .iter()
                    .find(|l| l.id != id && l.pump_channels.contains(channel))
                {
                    return Err(Error::in_use(format!(
                        "{channel} drives the Loop {}",
                        other.id
                    )));
                }
            }
            i.loop_mut(id)?.pump_channels = channels.to_vec();
            Ok(())
        })
    }

    pub fn set_flow_sensor(&self, id: &str, sensor: &str, table: &str) -> Result<()> {
        let sensor = optional(sensor, Kind::Sensor)?;
        self.edit(|i| {
            i.loop_mut(id)?;
            let table = match (&sensor, table) {
                (None, "") => None,
                (None, _) => return Err(Error::invalid("a table needs a flow sensor")),
                (Some(_), "") => {
                    return Err(Error::invalid("a flow sensor needs a Calibration table"));
                }
                (Some(sensor), table) => {
                    i.require_sensor(sensor)?;
                    // The Feed message carries the flow, and it stays on measured values.
                    i.refuse_graph_sensor(sensor, "a Loop cannot take as its flow sensor")?;
                    if !i.table_exists(table) {
                        return Err(Error::not_found(format!("no Calibration table {table}")));
                    }
                    Some(table.to_owned())
                }
            };
            let current = i.loop_mut(id)?;
            let flow = (current.flow_sensor.is_some() && current.calibration_table.is_some())
                .then(|| current.flow_id());
            if let Some(flow) = flow
                && current.flow_sensor != sensor
                && let Some(user) = i.sensor_user(&flow)
            {
                return Err(Error::in_use(format!("{flow} is read by {user}")));
            }
            let l = i.loop_mut(id)?;
            l.flow_sensor = sensor;
            l.calibration_table = table;
            Ok(())
        })
    }

    pub fn set_water_temperature(&self, id: &str, sensor: &str) -> Result<()> {
        let sensor = optional(sensor, Kind::Sensor)?;
        self.edit(|i| {
            i.loop_mut(id)?;
            if let Some(sensor) = &sensor {
                i.require_sensor(sensor)?;
                i.refuse_graph_sensor(sensor, "a Loop cannot take as its water temperature")?;
            }
            i.loop_mut(id)?.water_temperature = sensor;
            Ok(())
        })
    }

    pub fn set_loop_leakshield(&self, id: &str, leakshield: &str) -> Result<()> {
        let leakshield = optional(leakshield, Kind::Leakshield)?;
        self.edit(|i| {
            i.loop_mut(id)?;
            if let Some(leakshield) = &leakshield
                && !i.leakshields.iter().any(|l| &l.id() == leakshield)
            {
                return Err(Error::not_found(format!("no Leakshield {leakshield}")));
            }
            i.loop_mut(id)?.leakshield = leakshield;
            Ok(())
        })
    }

    pub fn set_pump_floor(&self, id: &str, duty: f64) -> Result<()> {
        check_percent("the Pump floor", duty)?;
        self.edit(|i| {
            i.loop_mut(id)?;
            i.profile_mut().pump_floors.insert(id.to_owned(), duty);
            Ok(())
        })
    }

    pub fn delete_loop(&self, id: &str) -> Result<()> {
        self.edit(|i| {
            i.loop_mut(id)?;
            let flow = ids::flow_sensor_id(id);
            if let Some(user) = i.sensor_user(&flow) {
                return Err(Error::in_use(format!("{flow} is read by {user}")));
            }
            i.state.loops.retain(|l| l.id != id);
            for profile in i.profiles.iter_mut() {
                profile.pump_floors.remove(id);
            }
            i.state.names.remove(&flow);
            Ok(())
        })
    }

    pub fn set_calibration_table(
        &self,
        id: &str,
        name: &str,
        points: Vec<(f64, f64)>,
    ) -> Result<String> {
        check_name(name)?;
        check_point_count("a Calibration table", points.len(), MAX_TABLE_POINTS)?;
        CalibrationTable::new(points.clone()).map_err(|e| Error::invalid(e.to_string()))?;
        self.edit(|i| {
            if i.presets.iter().any(|p| p.id == id) {
                return Err(Error::read_only("preset"));
            }
            if id.is_empty() {
                check_count(
                    "Calibration tables",
                    i.state.calibration_tables.len(),
                    MAX_CALIBRATION_TABLES,
                )?;
                let id = ids::table_id(ids::next_number(
                    "table_",
                    i.state.calibration_tables.iter().map(|t| t.id.as_str()),
                ));
                i.state.calibration_tables.push(CalibrationDef {
                    id: id.clone(),
                    name: name.to_owned(),
                    points,
                });
                return Ok(id);
            }
            let table = i
                .state
                .calibration_tables
                .iter_mut()
                .find(|t| t.id == id)
                .ok_or_else(|| Error::not_found(format!("no Calibration table {id}")))?;
            table.name = name.to_owned();
            table.points = points;
            Ok(id.to_owned())
        })
    }

    pub fn delete_calibration_table(&self, id: &str) -> Result<()> {
        self.edit(|i| {
            if i.presets.iter().any(|p| p.id == id) {
                return Err(Error::read_only("preset"));
            }
            if !i.state.calibration_tables.iter().any(|t| t.id == id) {
                return Err(Error::not_found(format!("no Calibration table {id}")));
            }
            if let Some(l) = i
                .state
                .loops
                .iter()
                .find(|l| l.calibration_table.as_deref() == Some(id))
            {
                return Err(Error::in_use(format!("the Loop {} uses {id}", l.id)));
            }
            i.state.calibration_tables.retain(|t| t.id != id);
            Ok(())
        })
    }
}

fn set_name(names: &mut BTreeMap<String, String>, id: &str, name: &str) {
    if name.is_empty() {
        names.remove(id);
    } else {
        names.insert(id.to_owned(), name.to_owned());
    }
}

fn flush(inner: &Mutex<Inner>, store: &StateStore) {
    let (state, profiles, deleted) = {
        let mut inner = lock(inner);
        let deleted = std::mem::take(&mut inner.deleted);
        (inner.state.clone(), inner.profiles.clone(), deleted)
    };
    if let Err(e) = store.save(STATE_FILE, &state) {
        eprintln!("nicecoolingd: cannot write {STATE_FILE}: {e}");
    }
    for profile in profiles.iter() {
        let file = profile_file(&profile.id);
        if let Err(e) = store.save(&file, profile) {
            eprintln!("nicecoolingd: cannot write {file}: {e}");
        }
    }
    for id in deleted.iter().filter(|id| !profiles.contains(id)) {
        if let Err(e) = store.remove_profile(id) {
            eprintln!("nicecoolingd: cannot remove the Profile {id}: {e}");
        }
    }
}
