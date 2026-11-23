//! A zbus server of the interface contract `io.github.cappyt.NiceCooling1`, with a small
//! simulated installation. It implements the properties and methods that the app uses.
//! `Alarms.Active` has the 8 fields of the XML: 5 strings, 2 times and the details.
// The signal `AlarmRaised` has 7 arguments, and the `interface` macro adds the emitter.
#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use nicecooling::daemon_client::{BUS_NAME, ROOT};
use zbus::OwnedObjectPath;
use zbus::message::Flags;
use zbus::object_server::{ObjectServer, SignalEmitter};
use zbus::{Connection, fdo, interface};
use zbus::{OwnedVariant as OwnedValue, Variant as Value};

use super::fake_leakshield::{DeviceOptions, Fv, Leakshield};

pub const LEAKSHIELD: &str = "leakshield_20417_31072";

type ActiveRow = (
    String,
    String,
    String,
    String,
    String,
    u64,
    u64,
    HashMap<String, OwnedValue>,
);

/// The `details` of an Alarm for the fake: a key that is `None` stays out of the dictionary.
#[derive(Default, Clone)]
pub struct Details {
    pub value: Option<f64>,
    pub limit: Option<f64>,
    pub unit: &'static str,
    pub sensor: Option<&'static str>,
    pub latched_bits: u32,
}

impl Details {
    fn to_map(&self) -> HashMap<String, OwnedValue> {
        let mut map = HashMap::new();
        let mut put = |key: &str, value: Value<'_>| {
            map.insert(key.to_owned(), OwnedValue::try_from(value).expect("value"));
        };
        if let Some(value) = self.value {
            put("value", Value::F64(value));
        }
        if let Some(limit) = self.limit {
            put("limit", Value::F64(limit));
        }
        if !self.unit.is_empty() {
            put("unit", Value::from(self.unit));
        }
        if self.latched_bits != 0 {
            put("latched_bits", Value::U32(self.latched_bits));
        }
        if let Some(sensor) = self.sensor {
            put("sensor", Value::from(path(&format!("sensor/{sensor}"))));
        }
        map
    }
}

type Table = (String, String, bool, Vec<(f64, f64)>);

/// One Rule group of the fake: id, target Profile, enabled, rules.
type Group = (
    String,
    OwnedObjectPath,
    bool,
    Vec<(String, HashMap<String, OwnedValue>)>,
);

pub struct Shared {
    /// `"<Method>(<args>) interactive=<bool>"` for each write call.
    pub calls: Mutex<Vec<String>>,
    pub deny_writes: Mutex<bool>,
    next_id: AtomicU32,
    next_profile: AtomicU32,
    next_group: AtomicU32,
    tables: Mutex<Vec<Table>>,
    /// Profile id -> Leakshield slot.
    slots: Mutex<HashMap<String, u8>>,
    groups: Mutex<Vec<Group>>,
    responses: Mutex<Vec<(String, String, u32)>>,
    /// The overrides per Leakshield: event, Leakshield id, response, delay.
    overrides: Mutex<Vec<(String, String, String, u32)>>,
    /// The rows of `GetHistory`, newest raise first.
    pub history: Mutex<Vec<nicecooling_api::alarms::HistoryRow>>,
}

impl Default for Shared {
    fn default() -> Self {
        Self {
            calls: Mutex::default(),
            deny_writes: Mutex::default(),
            next_id: AtomicU32::new(1),
            next_profile: AtomicU32::new(3),
            next_group: AtomicU32::new(1),
            slots: Mutex::new(HashMap::from([
                ("profile_default".to_owned(), 1),
                ("profile_2".to_owned(), 2),
            ])),
            groups: Mutex::default(),
            history: Mutex::default(),
            overrides: Mutex::default(),
            responses: Mutex::new(
                [
                    ("leakshield.leak-warning", "notify"),
                    ("leakshield.leak-alarm", "poweroff"),
                    ("leakshield.fill-level-low-warning", "notify"),
                    ("leakshield.fill-level-low-alarm", "notify"),
                    ("pump-stop", "poweroff"),
                    ("leakshield-unreachable", "notify"),
                    ("lost-input", "notify"),
                ]
                .into_iter()
                .map(|(event, response)| (event.to_owned(), response.to_owned(), 30))
                .collect(),
            ),
            tables: Mutex::new(vec![(
                "alphacool_17558".into(),
                "Alphacool 17558 flow sensor".into(),
                true,
                vec![(327.0, 40.0), (369.0, 50.0), (404.0, 60.0), (700.0, 120.0)],
            )]),
        }
    }
}

impl Shared {
    pub fn record(&self, call: &str, header: &zbus::message::Header<'_>) -> fdo::Result<()> {
        let interactive = header
            .primary()
            .flags()
            .contains(Flags::ALLOW_INTERACTIVE_AUTH);
        self.calls
            .lock()
            .expect("calls")
            .push(format!("{call} interactive={interactive}"));
        if *self.deny_writes.lock().expect("deny") {
            return Err(fdo::Error::AccessDenied("polkit denied the call".into()));
        }
        Ok(())
    }
}

pub fn path(suffix: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(format!("{ROOT}/{suffix}")).expect("object path")
}

struct Manager {
    shared: Arc<Shared>,
}

#[interface(name = "io.github.cappyt.NiceCooling1.Manager")]
impl Manager {
    #[zbus(property)]
    fn version(&self) -> String {
        "0.1.0".into()
    }

    #[zbus(property)]
    fn state_health(&self) -> String {
        "ok".into()
    }

    async fn create_derived_sensor(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
        name: &str,
        r#type: &str,
        sources: Vec<OwnedObjectPath>,
    ) -> fdo::Result<OwnedObjectPath> {
        let ids: Vec<&str> = sources
            .iter()
            .map(|p| p.as_str().rsplit('/').next().unwrap_or_default())
            .collect();
        self.shared.record(
            &format!("CreateDerivedSensor({name},{},[{}])", r#type, ids.join(",")),
            &header,
        )?;
        let id = format!(
            "derived_{}",
            self.shared.next_id.fetch_add(1, Ordering::Relaxed)
        );
        let object = path(&format!("sensor/{id}"));
        server
            .at(
                object.clone(),
                Sensor {
                    id: id.clone(),
                    name: name.into(),
                    default_name: name.into(),
                    kind: "temperature",
                    unit: "degC",
                    origin: "derived",
                    driver: String::new(),
                    shared: self.shared.clone(),
                },
            )
            .await?;
        server
            .at(
                object.clone(),
                DerivedSensor {
                    id,
                    function: r#type.into(),
                    sources,
                    shared: self.shared.clone(),
                },
            )
            .await?;
        Ok(object)
    }

    async fn create_loop(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
        name: &str,
    ) -> fdo::Result<OwnedObjectPath> {
        self.shared
            .record(&format!("CreateLoop({name})"), &header)?;
        let id = format!(
            "loop_{}",
            self.shared.next_id.fetch_add(1, Ordering::Relaxed) + 1
        );
        let object = path(&format!("loop/{id}"));
        server
            .at(
                object.clone(),
                Loop {
                    id,
                    name: name.into(),
                    pumps: Vec::new(),
                    flow_sensor: String::new(),
                    table: String::new(),
                    flow: String::new(),
                    water: String::new(),
                    leakshield: String::new(),
                    floor: 40.0,
                    shared: self.shared.clone(),
                },
            )
            .await?;
        Ok(object)
    }

    fn get_calibration_tables(&self) -> Vec<Table> {
        self.shared.tables.lock().expect("tables").clone()
    }

    fn set_calibration_table(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: &str,
        name: &str,
        points: Vec<(f64, f64)>,
    ) -> fdo::Result<String> {
        self.shared.record(
            &format!("SetCalibrationTable({id},{name},{points:?})"),
            &header,
        )?;
        let mut tables = self.shared.tables.lock().expect("tables");
        if tables.iter().any(|t| t.0 == id && t.2) {
            return Err(fdo::Error::Failed(
                "io.github.cappyt.NiceCooling1.Error.ReadOnly".into(),
            ));
        }
        let id = if id.is_empty() {
            format!(
                "table_{}",
                self.shared.next_id.fetch_add(1, Ordering::Relaxed)
            )
        } else {
            id.to_owned()
        };
        tables.retain(|t| t.0 != id);
        tables.push((id.clone(), name.into(), false, points));
        Ok(id)
    }

    fn delete_calibration_table(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        id: &str,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("DeleteCalibrationTable({id})"), &header)?;
        self.shared
            .tables
            .lock()
            .expect("tables")
            .retain(|t| t.0 != id);
        Ok(())
    }

    #[zbus(signal)]
    async fn sensors_updated(
        emitter: &SignalEmitter<'_>,
        time_us: u64,
        sensors: Vec<(String, f64, u8)>,
        channels: Vec<(String, f64, f64, f64, u8)>,
    ) -> zbus::Result<()>;
}

struct Board;

#[interface(name = "io.github.cappyt.NiceCooling1.Board")]
impl Board {
    #[zbus(property)]
    fn vendor(&self) -> String {
        "ASUSTeK COMPUTER INC.".into()
    }

    #[zbus(property)]
    fn model(&self) -> String {
        "ROG CROSSHAIR VIII DARK HERO".into()
    }

    #[zbus(property)]
    fn board_file(&self) -> String {
        "asus_rog_crosshair_viii_dark_hero".into()
    }
}

struct Profiles {
    active: String,
    cause: String,
    immediate: bool,
    rule_delay: u32,
    dwell: u32,
    /// The end of the running Profile dwell in microseconds, 0 when none runs.
    dwell_until: u64,
    shared: Arc<Shared>,
}

fn object_id_of(object: &OwnedObjectPath) -> String {
    object
        .as_str()
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_owned()
}

fn invalid() -> fdo::Error {
    fdo::Error::Failed("io.github.cappyt.NiceCooling1.Error.InvalidArgument".into())
}

#[interface(name = "io.github.cappyt.NiceCooling1.Profiles")]
impl Profiles {
    #[zbus(property)]
    fn active(&self) -> OwnedObjectPath {
        path(&format!("profile/{}", self.active))
    }

    #[zbus(property)]
    fn activation_cause(&self) -> String {
        self.cause.clone()
    }

    #[zbus(property)]
    fn active_state(&self) -> String {
        "applied".into()
    }

    #[zbus(property)]
    fn default(&self) -> OwnedObjectPath {
        path("profile/profile_default")
    }

    #[zbus(property)]
    fn immediate_switching(&self) -> bool {
        self.immediate
    }

    #[zbus(property)]
    fn rule_delay_seconds(&self) -> u32 {
        self.rule_delay
    }

    #[zbus(property)]
    fn dwell_seconds(&self) -> u32 {
        self.dwell
    }

    #[zbus(property)]
    fn dwell_until(&self) -> u64 {
        self.dwell_until
    }

    #[zbus(property)]
    fn rule_group_state(&self) -> Vec<(String, bool, u64)> {
        self.shared
            .groups
            .lock()
            .expect("groups")
            .iter()
            .map(|group| (group.0.clone(), false, 0))
            .collect()
    }

    async fn create_profile(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
        name: &str,
        copy_from: OwnedObjectPath,
    ) -> fdo::Result<OwnedObjectPath> {
        let from = if copy_from.as_str() == "/" {
            self.active.clone()
        } else {
            object_id_of(&copy_from)
        };
        self.shared
            .record(&format!("CreateProfile({name},{from})"), &header)?;
        let slot = self
            .shared
            .slots
            .lock()
            .expect("slots")
            .get(&from)
            .copied()
            .unwrap_or(1);
        let id = format!(
            "profile_{}",
            self.shared.next_profile.fetch_add(1, Ordering::Relaxed)
        );
        self.shared
            .slots
            .lock()
            .expect("slots")
            .insert(id.clone(), slot);
        let object = path(&format!("profile/{id}"));
        server
            .at(
                object.clone(),
                Profile {
                    id,
                    name: name.to_owned(),
                    is_default: false,
                    shared: self.shared.clone(),
                },
            )
            .await?;
        Ok(object)
    }

    async fn set_immediate_switching(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        enabled: bool,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("SetImmediateSwitching({enabled})"), &header)?;
        self.immediate = enabled;
        self.immediate_switching_changed(&emitter).await?;
        Ok(())
    }

    async fn set_rule_timing(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        rule_delay_s: u32,
        dwell_s: u32,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("SetRuleTiming({rule_delay_s},{dwell_s})"), &header)?;
        self.rule_delay = rule_delay_s;
        self.dwell = dwell_s;
        self.rule_delay_seconds_changed(&emitter).await?;
        self.dwell_seconds_changed(&emitter).await?;
        Ok(())
    }

    async fn get_rule_groups(&self) -> Vec<Group> {
        self.shared.groups.lock().expect("groups").clone()
    }

    async fn set_rule_groups(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        groups: Vec<Group>,
    ) -> fdo::Result<Vec<String>> {
        let summary: Vec<String> = groups
            .iter()
            .map(|(id, target, enabled, rules)| {
                let kinds: Vec<String> = rules
                    .iter()
                    .map(|(kind, params)| {
                        let mut keys: Vec<&str> = params.keys().map(String::as_str).collect();
                        keys.sort_unstable();
                        format!("{kind}:{}", keys.join("+"))
                    })
                    .collect();
                format!(
                    "{id}>{}>{enabled}>[{}]",
                    object_id_of(target),
                    kinds.join(",")
                )
            })
            .collect();
        self.shared
            .record(&format!("SetRuleGroups({})", summary.join(";")), &header)?;
        let missing = {
            let known = self.shared.slots.lock().expect("slots");
            groups
                .iter()
                .any(|group| !known.contains_key(&object_id_of(&group.1)))
        };
        if missing {
            return Err(fdo::Error::Failed(
                "io.github.cappyt.NiceCooling1.Error.NotFound".into(),
            ));
        }
        let mut stored = Vec::new();
        let mut ids = Vec::new();
        for (id, target, enabled, rules) in groups {
            let id = if id.is_empty() {
                format!(
                    "group_{}",
                    self.shared.next_group.fetch_add(1, Ordering::Relaxed)
                )
            } else {
                id
            };
            ids.push(id.clone());
            stored.push((id, target, enabled, rules));
        }
        *self.shared.groups.lock().expect("groups") = stored;
        self.rule_group_state_changed(&emitter).await?;
        Ok(ids)
    }

    async fn activate_profile(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        profile: OwnedObjectPath,
    ) -> fdo::Result<()> {
        let id = profile
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        self.shared
            .record(&format!("ActivateProfile({id})"), &header)?;
        self.active = id;
        self.cause = "manual".into();
        self.active_changed(&emitter).await?;
        self.activation_cause_changed(&emitter).await?;
        Ok(())
    }

    async fn return_to_automatic(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        self.shared.record("ReturnToAutomatic()", &header)?;
        self.active = "profile_default".into();
        self.cause = "default".into();
        self.active_changed(&emitter).await?;
        self.activation_cause_changed(&emitter).await?;
        Ok(())
    }

    #[zbus(signal)]
    async fn profile_activated(
        emitter: &SignalEmitter<'_>,
        profile: OwnedObjectPath,
        cause: &str,
        state: &str,
    ) -> zbus::Result<()>;
}

struct Alarms {
    active: Vec<ActiveRow>,
    shared: Arc<Shared>,
}

#[interface(name = "io.github.cappyt.NiceCooling1.Alarms")]
impl Alarms {
    #[zbus(property)]
    fn active(&self) -> Vec<ActiveRow> {
        self.active.clone()
    }

    async fn get_responses(&self) -> Vec<(String, String, String, u32, bool)> {
        let defaults = self.shared.responses.lock().expect("responses").clone();
        let overrides = self.shared.overrides.lock().expect("overrides").clone();
        let mut rows: Vec<_> = defaults
            .iter()
            .map(|(event, response, delay_s)| {
                (
                    event.clone(),
                    String::new(),
                    response.clone(),
                    *delay_s,
                    false,
                )
            })
            .collect();
        for (event, response, delay_s) in defaults
            .iter()
            .filter(|(event, ..)| event.starts_with("leakshield."))
        {
            let found = overrides
                .iter()
                .find(|row| &row.0 == event && row.1 == LEAKSHIELD);
            rows.push(match found {
                Some((_, _, response, delay_s)) => (
                    event.clone(),
                    LEAKSHIELD.to_owned(),
                    response.clone(),
                    *delay_s,
                    true,
                ),
                None => (
                    event.clone(),
                    LEAKSHIELD.to_owned(),
                    response.clone(),
                    *delay_s,
                    false,
                ),
            });
        }
        rows
    }

    async fn set_response(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        event: &str,
        source: &str,
        response: &str,
        delay_s: u32,
    ) -> fdo::Result<()> {
        let at = if source.is_empty() {
            String::new()
        } else {
            format!("@{source}")
        };
        self.shared.record(
            &format!("SetAlarmResponse({event}{at},{response},{delay_s})"),
            &header,
        )?;
        let not_found =
            || fdo::Error::Failed("io.github.cappyt.NiceCooling1.Error.NotFound".into());
        if !source.is_empty() {
            if !event.starts_with("leakshield.") {
                return Err(fdo::Error::Failed(
                    "io.github.cappyt.NiceCooling1.Error.InvalidArgument".into(),
                ));
            }
            if source != LEAKSHIELD {
                return Err(not_found());
            }
            let mut overrides = self.shared.overrides.lock().expect("overrides");
            overrides.retain(|row| !(row.0 == event && row.1 == source));
            overrides.push((
                event.to_owned(),
                source.to_owned(),
                response.to_owned(),
                delay_s,
            ));
            return Ok(());
        }
        let mut responses = self.shared.responses.lock().expect("responses");
        let row = responses
            .iter_mut()
            .find(|row| row.0 == event)
            .ok_or_else(not_found)?;
        row.1 = response.to_owned();
        row.2 = delay_s;
        Ok(())
    }

    async fn reset_response(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        event: &str,
        source: &str,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("ResetResponse({event},{source})"), &header)?;
        if source.is_empty() {
            let builtin = if matches!(event, "leakshield.leak-alarm" | "pump-stop") {
                "poweroff"
            } else {
                "notify"
            };
            let mut responses = self.shared.responses.lock().expect("responses");
            if let Some(row) = responses.iter_mut().find(|row| row.0 == event) {
                row.1 = builtin.to_owned();
                row.2 = 30;
            }
        } else {
            self.shared
                .overrides
                .lock()
                .expect("overrides")
                .retain(|row| !(row.0 == event && row.1 == source));
        }
        Ok(())
    }

    async fn get_history(
        &self,
        since_us: u64,
        limit: u32,
    ) -> Vec<nicecooling_api::alarms::HistoryRow> {
        let rows = self.shared.history.lock().expect("history");
        let rows = rows
            .iter()
            .filter(|row| row.6 == 0 || row.6 >= since_us)
            .cloned();
        if limit == 0 {
            rows.collect()
        } else {
            rows.take(limit as usize).collect()
        }
    }

    async fn cancel_poweroff(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        instance: &str,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("CancelPoweroff({instance})"), &header)?;
        let pending = self
            .active
            .iter_mut()
            .find(|row| row.0 == instance && row.6 != 0)
            .ok_or_else(|| {
                fdo::Error::Failed("io.github.cappyt.NiceCooling1.Error.NoPoweroff".into())
            })?;
        pending.6 = 0;
        for row in self.shared.history.lock().expect("history").iter_mut() {
            if row.0 == instance {
                row.7 = "poweroff-cancelled".into();
            }
        }
        self.active_changed(&emitter).await?;
        Self::poweroff_cancelled(&emitter, instance).await?;
        Ok(())
    }

    #[zbus(signal)]
    async fn alarm_raised(
        emitter: &SignalEmitter<'_>,
        instance: &str,
        event: &str,
        severity: &str,
        source: &str,
        response: &str,
        poweroff_at_us: u64,
        details: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn alarm_cleared(emitter: &SignalEmitter<'_>, instance: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn poweroff_cancelled(emitter: &SignalEmitter<'_>, instance: &str) -> zbus::Result<()>;
}

struct Profile {
    id: String,
    name: String,
    is_default: bool,
    shared: Arc<Shared>,
}

/// The Profile content of the fake: one Curve, one fixed Duty and the Leakshield settings of the slot.
fn content(slot: u8) -> HashMap<String, OwnedValue> {
    let own = |value: Value<'_>| OwnedValue::try_from(value).expect("value");
    let mut pump: HashMap<String, Value<'_>> = HashMap::new();
    pump.insert("control".into(), Value::from("curve"));
    pump.insert("input".into(), Value::from("nct6798_temp1"));
    pump.insert(
        "points".into(),
        Value::from(vec![(30.0f64, 40.0f64), (45.0, 70.0), (60.0, 100.0)]),
    );
    pump.insert("fixed_duty".into(), Value::F64(50.0));
    let mut chassis: HashMap<String, Value<'_>> = HashMap::new();
    chassis.insert("control".into(), Value::from("fixed"));
    chassis.insert("fixed_duty".into(), Value::F64(30.0));
    let mut channels: HashMap<String, Value<'_>> = HashMap::new();
    channels.insert("nct6798_pwm6".into(), Value::from(pump));
    channels.insert("nct6798_pwm2".into(), Value::from(chassis));
    let mut fields: HashMap<String, Value<'_>> = HashMap::new();
    fields.insert("default_mode".into(), Value::from("shield"));
    fields.insert("display_brightness_active".into(), Value::U8(3));
    fields.insert("display_brightness_idle".into(), Value::U8(1));
    fields.insert("led_strip_brightness".into(), Value::F64(60.0));
    fields.insert(
        "fill_level_warning_threshold".into(),
        Value::F64(f64::from(30 - slot)),
    );
    fields.insert("fill_level_alarm_threshold".into(), Value::F64(15.0));
    fields.insert("leak_rate_warning_threshold".into(), Value::F64(100.0));
    let mut devices: HashMap<String, Value<'_>> = HashMap::new();
    devices.insert(LEAKSHIELD.into(), Value::from(fields));
    let mut map = HashMap::new();
    map.insert("channels".to_owned(), own(Value::from(channels)));
    map.insert("slot".to_owned(), own(Value::U8(slot)));
    map.insert("leakshield_fields".to_owned(), own(Value::from(devices)));
    map
}

#[interface(name = "io.github.cappyt.NiceCooling1.Profile")]
impl Profile {
    #[zbus(property)]
    fn id(&self) -> String {
        self.id.clone()
    }

    #[zbus(property)]
    fn name(&self) -> String {
        self.name.clone()
    }

    #[zbus(property)]
    fn is_default(&self) -> bool {
        self.is_default
    }

    #[zbus(property)]
    fn contains_release(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn slot(&self) -> u8 {
        self.shared
            .slots
            .lock()
            .expect("slots")
            .get(&self.id)
            .copied()
            .unwrap_or(1)
    }

    async fn set_name(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        name: &str,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("SetProfileName({},{name})", self.id), &header)?;
        self.name = name.to_owned();
        self.name_changed(&emitter).await?;
        Ok(())
    }

    async fn set_slot(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        slot: u8,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("SetProfileSlot({},{slot})", self.id), &header)?;
        if self.is_default || !(1..=4).contains(&slot) {
            return Err(invalid());
        }
        self.shared
            .slots
            .lock()
            .expect("slots")
            .insert(self.id.clone(), slot);
        self.slot_changed(&emitter).await?;
        Ok(())
    }

    async fn delete(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("DeleteProfile({})", self.id), &header)?;
        if self.is_default {
            return Err(fdo::Error::Failed(
                "io.github.cappyt.NiceCooling1.Error.DefaultProfile".into(),
            ));
        }
        let used = self
            .shared
            .groups
            .lock()
            .expect("groups")
            .iter()
            .any(|group| object_id_of(&group.1) == self.id);
        if used {
            return Err(fdo::Error::Failed(
                "io.github.cappyt.NiceCooling1.Error.InUse".into(),
            ));
        }
        self.shared.slots.lock().expect("slots").remove(&self.id);
        server
            .remove::<Profile, _>(path(&format!("profile/{}", self.id)))
            .await?;
        Ok(())
    }

    async fn get_content(&self) -> HashMap<String, OwnedValue> {
        let slot = self
            .shared
            .slots
            .lock()
            .expect("slots")
            .get(&self.id)
            .copied()
            .unwrap_or(1);
        content(slot)
    }
}

struct Channel {
    id: String,
    name: String,
    default_name: String,
    control: String,
    is_pump: bool,
    curve: (OwnedObjectPath, Vec<(f64, f64)>, f64, f64),
    fixed: f64,
    bios_snapshot: (u64, Vec<(String, String)>),
    fan_sensor: OwnedObjectPath,
    shared: Arc<Shared>,
}

#[interface(name = "io.github.cappyt.NiceCooling1.Channel")]
impl Channel {
    #[zbus(property)]
    fn id(&self) -> String {
        self.id.clone()
    }

    #[zbus(property)]
    fn name(&self) -> String {
        self.name.clone()
    }

    #[zbus(property)]
    fn default_name(&self) -> String {
        self.default_name.clone()
    }

    #[zbus(property)]
    fn duty(&self) -> f64 {
        62.0
    }

    #[zbus(property)]
    fn raw(&self) -> u8 {
        158
    }

    #[zbus(property)]
    fn control(&self) -> String {
        self.control.clone()
    }

    #[zbus(property, name = "Override")]
    fn override_state(&self) -> String {
        "none".into()
    }

    #[zbus(property)]
    fn is_pump(&self) -> bool {
        self.is_pump
    }

    #[zbus(property)]
    fn manual_at_snapshot(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn curve(&self) -> (OwnedObjectPath, Vec<(f64, f64)>, f64, f64) {
        self.curve.clone()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn bios_snapshot(&self) -> (u64, Vec<(String, String)>) {
        self.bios_snapshot.clone()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn fan_sensor(&self) -> OwnedObjectPath {
        self.fan_sensor.clone()
    }

    #[zbus(property)]
    fn fixed_duty(&self) -> f64 {
        self.fixed
    }

    async fn set_name(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        name: &str,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("SetChannelName({},{name})", self.id), &header)?;
        self.name = if name.is_empty() {
            self.default_name.clone()
        } else {
            name.to_owned()
        };
        self.name_changed(&emitter).await?;
        Ok(())
    }

    async fn set_curve(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        input: OwnedObjectPath,
        points: Vec<(f64, f64)>,
        smoothing_s: f64,
        step_limit_pct_per_s: f64,
    ) -> fdo::Result<()> {
        let input_id = input
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        self.shared.record(
            &format!(
                "SetCurve({},{input_id},{points:?},{smoothing_s},{step_limit_pct_per_s})",
                self.id
            ),
            &header,
        )?;
        if points.len() < 2 {
            return Err(fdo::Error::Failed(
                "io.github.cappyt.NiceCooling1.Error.InvalidArgument".into(),
            ));
        }
        self.curve = (input, points, smoothing_s, step_limit_pct_per_s);
        self.control = "curve".into();
        self.curve_changed(&emitter).await?;
        self.control_changed(&emitter).await?;
        Ok(())
    }

    async fn set_fixed_duty(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        duty: f64,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("SetFixedDuty({},{duty})", self.id), &header)?;
        self.fixed = duty;
        self.control = "fixed".into();
        self.fixed_duty_changed(&emitter).await?;
        self.control_changed(&emitter).await?;
        Ok(())
    }

    async fn release_to_hardware(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("ReleaseToHardware({})", self.id), &header)?;
        self.control = "hardware".into();
        self.control_changed(&emitter).await?;
        Ok(())
    }
}

struct Sensor {
    id: String,
    name: String,
    default_name: String,
    kind: &'static str,
    unit: &'static str,
    origin: &'static str,
    driver: String,
    shared: Arc<Shared>,
}

#[interface(name = "io.github.cappyt.NiceCooling1.Sensor")]
impl Sensor {
    #[zbus(property)]
    fn id(&self) -> String {
        self.id.clone()
    }

    #[zbus(property)]
    fn name(&self) -> String {
        self.name.clone()
    }

    #[zbus(property)]
    fn default_name(&self) -> String {
        self.default_name.clone()
    }

    #[zbus(property)]
    fn kind(&self) -> String {
        self.kind.into()
    }

    #[zbus(property)]
    fn unit(&self) -> String {
        self.unit.into()
    }

    #[zbus(property)]
    fn origin(&self) -> String {
        self.origin.into()
    }

    #[zbus(property)]
    fn driver(&self) -> String {
        self.driver.clone()
    }

    async fn set_name(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        name: &str,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("SetSensorName({},{name})", self.id), &header)?;
        self.name = if name.is_empty() {
            self.default_name.clone()
        } else {
            name.to_owned()
        };
        self.name_changed(&emitter).await?;
        Ok(())
    }
}

struct DerivedSensor {
    id: String,
    function: String,
    sources: Vec<OwnedObjectPath>,
    shared: Arc<Shared>,
}

#[interface(name = "io.github.cappyt.NiceCooling1.DerivedSensor")]
impl DerivedSensor {
    #[zbus(property, name = "Type")]
    fn function(&self) -> String {
        self.function.clone()
    }

    #[zbus(property)]
    fn sources(&self) -> Vec<OwnedObjectPath> {
        self.sources.clone()
    }

    async fn set_definition(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        r#type: &str,
        sources: Vec<OwnedObjectPath>,
    ) -> fdo::Result<()> {
        let ids: Vec<&str> = sources
            .iter()
            .map(|p| p.as_str().rsplit('/').next().unwrap_or_default())
            .collect();
        self.shared.record(
            &format!(
                "SetDerivedDefinition({},{},[{}])",
                self.id,
                r#type,
                ids.join(",")
            ),
            &header,
        )?;
        self.function = r#type.into();
        self.sources = sources;
        self.type_changed(&emitter).await?;
        self.sources_changed(&emitter).await?;
        Ok(())
    }

    async fn delete(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("DeleteDerivedSensor({})", self.id), &header)?;
        let object = path(&format!("sensor/{}", self.id));
        server.remove::<DerivedSensor, _>(object.clone()).await?;
        server.remove::<Sensor, _>(object).await?;
        Ok(())
    }
}

struct Loop {
    id: String,
    name: String,
    pumps: Vec<String>,
    flow_sensor: String,
    table: String,
    flow: String,
    water: String,
    leakshield: String,
    floor: f64,
    shared: Arc<Shared>,
}

fn object_or_none(kind: &str, id: &str) -> OwnedObjectPath {
    if id.is_empty() {
        OwnedObjectPath::try_from("/").expect("path")
    } else {
        path(&format!("{kind}/{id}"))
    }
}

fn id_of(object: &OwnedObjectPath) -> String {
    if object.as_str() == "/" {
        String::new()
    } else {
        object
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned()
    }
}

#[interface(name = "io.github.cappyt.NiceCooling1.Loop")]
impl Loop {
    #[zbus(property)]
    fn id(&self) -> String {
        self.id.clone()
    }

    #[zbus(property)]
    fn name(&self) -> String {
        self.name.clone()
    }

    #[zbus(property)]
    fn pump_channels(&self) -> Vec<OwnedObjectPath> {
        self.pumps
            .iter()
            .map(|id| path(&format!("channel/{id}")))
            .collect()
    }

    #[zbus(property)]
    fn flow_sensor(&self) -> OwnedObjectPath {
        object_or_none("sensor", &self.flow_sensor)
    }

    #[zbus(property)]
    fn calibration_table(&self) -> String {
        self.table.clone()
    }

    #[zbus(property)]
    fn flow(&self) -> OwnedObjectPath {
        object_or_none("sensor", &self.flow)
    }

    #[zbus(property)]
    fn water_temperature(&self) -> OwnedObjectPath {
        object_or_none("sensor", &self.water)
    }

    #[zbus(property)]
    fn leakshield(&self) -> OwnedObjectPath {
        object_or_none("leakshield", &self.leakshield)
    }

    #[zbus(property)]
    fn pump_floor(&self) -> f64 {
        self.floor
    }

    async fn set_name(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        name: &str,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("SetLoopName({},{name})", self.id), &header)?;
        self.name = name.into();
        self.name_changed(&emitter).await?;
        Ok(())
    }

    async fn set_pump_channels(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        channels: Vec<OwnedObjectPath>,
    ) -> fdo::Result<()> {
        let ids: Vec<String> = channels.iter().map(id_of).collect();
        self.shared.record(
            &format!("SetPumpChannels({},[{}])", self.id, ids.join(",")),
            &header,
        )?;
        self.pumps = ids;
        self.pump_channels_changed(&emitter).await?;
        Ok(())
    }

    async fn set_flow_sensor(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        sensor: OwnedObjectPath,
        calibration_table: &str,
    ) -> fdo::Result<()> {
        let sensor = id_of(&sensor);
        self.shared.record(
            &format!("SetFlowSensor({},{sensor},{calibration_table})", self.id),
            &header,
        )?;
        self.flow_sensor = sensor;
        self.table = calibration_table.into();
        self.flow_sensor_changed(&emitter).await?;
        self.calibration_table_changed(&emitter).await?;
        Ok(())
    }

    async fn set_water_temperature(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        sensor: OwnedObjectPath,
    ) -> fdo::Result<()> {
        let sensor = id_of(&sensor);
        self.shared.record(
            &format!("SetWaterTemperature({},{sensor})", self.id),
            &header,
        )?;
        self.water = sensor;
        self.water_temperature_changed(&emitter).await?;
        Ok(())
    }

    async fn set_leakshield(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        leakshield: OwnedObjectPath,
    ) -> fdo::Result<()> {
        let leakshield = id_of(&leakshield);
        self.shared.record(
            &format!("SetLoopLeakshield({},{leakshield})", self.id),
            &header,
        )?;
        self.leakshield = leakshield;
        self.leakshield_changed(&emitter).await?;
        Ok(())
    }

    async fn set_pump_floor(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        duty: f64,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("SetPumpFloor({},{duty})", self.id), &header)?;
        self.floor = duty;
        self.pump_floor_changed(&emitter).await?;
        Ok(())
    }

    async fn delete(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("DeleteLoop({})", self.id), &header)?;
        server
            .remove::<Loop, _>(path(&format!("loop/{}", self.id)))
            .await?;
        Ok(())
    }
}

#[derive(Default)]
pub struct Options {
    /// A Leakshield with the firmware 1042 that the registry does not know.
    pub unknown_firmware: bool,
    /// The firmware of the Leakshield has no cancel and reset commands.
    pub no_reset_commands: bool,
    /// A Pump channel of the Loop runs: the Refill is refused.
    pub pump_running: bool,
    /// The Feed is on and nothing was sent yet.
    pub feed_waiting: bool,
}

pub struct FakeDaemon {
    conn: Connection,
    pub shared: Arc<Shared>,
}

impl FakeDaemon {
    pub async fn start(conn: Connection, options: Options) -> Self {
        let shared = Arc::new(Shared::default());
        let server = conn.object_server();
        server
            .at(ROOT, fdo::ObjectManager)
            .await
            .expect("object manager");
        server
            .at(
                ROOT,
                Manager {
                    shared: shared.clone(),
                },
            )
            .await
            .expect("manager");
        server.at(path("board"), Board).await.expect("board");
        server
            .at(
                ROOT,
                Profiles {
                    active: "profile_default".into(),
                    cause: "default".into(),
                    immediate: false,
                    rule_delay: 30,
                    dwell: 300,
                    dwell_until: 0,
                    shared: shared.clone(),
                },
            )
            .await
            .expect("profiles");
        server
            .at(
                ROOT,
                Alarms {
                    active: Vec::new(),
                    shared: shared.clone(),
                },
            )
            .await
            .expect("alarms");
        for (id, name) in [("profile_default", "Default"), ("profile_2", "Quiet")] {
            server
                .at(
                    path(&format!("profile/{id}")),
                    Profile {
                        id: id.into(),
                        name: name.into(),
                        is_default: id == "profile_default",
                        shared: shared.clone(),
                    },
                )
                .await
                .expect("profile");
        }
        server
            .at(
                path("channel/nct6798_pwm6"),
                Channel {
                    id: "nct6798_pwm6".into(),
                    name: "Waterpump".into(),
                    default_name: "Waterpump".into(),
                    control: "curve".into(),
                    is_pump: true,
                    curve: (
                        path("sensor/nct6798_temp1"),
                        vec![(30.0, 40.0), (45.0, 70.0), (60.0, 100.0)],
                        5.0,
                        5.0,
                    ),
                    fixed: 50.0,
                    bios_snapshot: (
                        1_700_000_000_000_000,
                        [
                            ("pwm6_auto_point1_temp", "30000"),
                            ("pwm6_auto_point1_pwm", "102"),
                            ("pwm6_auto_point2_temp", "60000"),
                            ("pwm6_auto_point2_pwm", "255"),
                            ("pwm6", "158"),
                            ("pwm6_enable", "5"),
                        ]
                        .map(|(name, value)| (name.to_owned(), value.to_owned()))
                        .to_vec(),
                    ),
                    fan_sensor: path("sensor/nct6798_fan6"),
                    shared: shared.clone(),
                },
            )
            .await
            .expect("channel");
        server
            .at(
                path("channel/nct6798_pwm2"),
                Channel {
                    id: "nct6798_pwm2".into(),
                    name: "Chassis Fan 1".into(),
                    default_name: "Chassis Fan 1".into(),
                    control: "hardware".into(),
                    is_pump: false,
                    curve: (
                        OwnedObjectPath::try_from("/").expect("path"),
                        Vec::new(),
                        5.0,
                        5.0,
                    ),
                    fixed: 50.0,
                    bios_snapshot: (1_700_000_000_000_000, Vec::new()),
                    fan_sensor: OwnedObjectPath::try_from("/").expect("path"),
                    shared: shared.clone(),
                },
            )
            .await
            .expect("channel");
        for (id, name, kind, unit, origin, driver) in [
            (
                "nct6798_temp1",
                "Water temperature",
                "temperature",
                "degC",
                "hwmon",
                "nct6798",
            ),
            (
                "k10temp_tctl",
                "CPU temperature",
                "temperature",
                "degC",
                "hwmon",
                "k10temp",
            ),
            (
                "nct6798_fan6",
                "Waterpump speed",
                "fan",
                "rpm",
                "hwmon",
                "nct6798",
            ),
            (
                "asusec_fan1",
                "Water flow",
                "flow",
                "rpm",
                "hwmon",
                "asusec",
            ),
            ("loop_1_flow", "Flow", "flow", "l/h", "calibrated", ""),
            (
                "leakshield_20417_31072_pressure",
                "Pressure",
                "pressure",
                "mbar",
                "leakshield",
                "",
            ),
        ] {
            server
                .at(
                    path(&format!("sensor/{id}")),
                    Sensor {
                        id: id.into(),
                        name: name.into(),
                        default_name: name.into(),
                        kind,
                        unit,
                        origin,
                        driver: driver.into(),
                        shared: shared.clone(),
                    },
                )
                .await
                .expect("sensor");
        }
        server
            .at(
                path("loop/loop_1"),
                Loop {
                    id: "loop_1".into(),
                    name: "Main loop".into(),
                    pumps: vec!["nct6798_pwm6".into()],
                    flow_sensor: "asusec_fan1".into(),
                    table: "alphacool_17558".into(),
                    flow: "loop_1_flow".into(),
                    water: "nct6798_temp1".into(),
                    leakshield: LEAKSHIELD.into(),
                    floor: 40.0,
                    shared: shared.clone(),
                },
            )
            .await
            .expect("loop");
        let (firmware, reason) = if options.unknown_firmware {
            (1042, "unknown-firmware")
        } else {
            (1040, "")
        };
        server
            .at(
                path(&format!("leakshield/{LEAKSHIELD}")),
                Leakshield::new(
                    firmware,
                    reason,
                    DeviceOptions {
                        no_reset_commands: options.no_reset_commands,
                        pump_running: options.pump_running,
                        feed_waiting: options.feed_waiting,
                    },
                    shared.clone(),
                ),
            )
            .await
            .expect("leakshield");
        conn.request_name(BUS_NAME).await.expect("daemon name");
        Self { conn, shared }
    }

    pub fn calls(&self) -> Vec<String> {
        self.shared.calls.lock().expect("calls").clone()
    }

    /// The value of a stored field of the fake Leakshield.
    pub async fn stored_field(&self, id: &str) -> Option<Fv> {
        let iface = self
            .conn
            .object_server()
            .interface::<_, Leakshield>(path(&format!("leakshield/{LEAKSHIELD}")))
            .await
            .expect("leakshield");
        iface.get().await.stored(id)
    }

    pub fn deny_writes(&self, deny: bool) {
        *self.shared.deny_writes.lock().expect("deny") = deny;
    }

    /// Raise an Alarm or a Warning, as the daemon does: the property changes and the signal goes out.
    pub async fn raise_alarm(
        &self,
        instance: &str,
        event: &str,
        severity: &str,
        source: &str,
        response: &str,
        poweroff_at_us: u64,
    ) {
        self.raise_alarm_with_details(
            instance,
            event,
            severity,
            source,
            response,
            poweroff_at_us,
            Details::default(),
        )
        .await;
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn raise_alarm_with_details(
        &self,
        instance: &str,
        event: &str,
        severity: &str,
        source: &str,
        response: &str,
        poweroff_at_us: u64,
        details: Details,
    ) {
        let iface = self
            .conn
            .object_server()
            .interface::<_, Alarms>(ROOT)
            .await
            .expect("alarms");
        iface
            .get()
            .await
            .shared
            .history
            .lock()
            .expect("history")
            .insert(
                0,
                (
                    instance.into(),
                    event.into(),
                    severity.into(),
                    source.into(),
                    response.into(),
                    1,
                    0,
                    match response {
                        "none" => "none",
                        "poweroff" => "poweroff-pending",
                        _ => "notified",
                    }
                    .into(),
                    details.to_map(),
                ),
            );
        iface.get_mut().await.active.push((
            instance.into(),
            event.into(),
            severity.into(),
            source.into(),
            response.into(),
            1,
            poweroff_at_us,
            details.to_map(),
        ));
        iface
            .get()
            .await
            .active_changed(iface.signal_emitter())
            .await
            .expect("changed");
        Alarms::alarm_raised(
            iface.signal_emitter(),
            instance,
            event,
            severity,
            source,
            response,
            poweroff_at_us,
            details.to_map(),
        )
        .await
        .expect("signal");
    }

    pub async fn clear_alarm(&self, instance: &str) {
        let iface = self
            .conn
            .object_server()
            .interface::<_, Alarms>(ROOT)
            .await
            .expect("alarms");
        for row in iface
            .get()
            .await
            .shared
            .history
            .lock()
            .expect("history")
            .iter_mut()
        {
            if row.0 == instance && row.6 == 0 {
                row.6 = 2;
                if row.7 == "poweroff-pending" {
                    row.7 = "poweroff-stopped".into();
                }
            }
        }
        iface.get_mut().await.active.retain(|row| row.0 != instance);
        iface
            .get()
            .await
            .active_changed(iface.signal_emitter())
            .await
            .expect("changed");
        Alarms::alarm_cleared(iface.signal_emitter(), instance)
            .await
            .expect("signal");
    }

    pub async fn emit_sensors(
        &self,
        time_us: u64,
        sensors: Vec<(String, f64, u8)>,
        channels: Vec<(String, f64, f64, f64, u8)>,
    ) {
        let iface = self
            .conn
            .object_server()
            .interface::<_, Manager>(ROOT)
            .await
            .expect("manager");
        Manager::sensors_updated(iface.signal_emitter(), time_us, sensors, channels)
            .await
            .expect("signal");
    }

    /// The end of the Profile dwell changes: the property changes and the signal goes out.
    pub async fn set_dwell_until(&self, until_us: u64) {
        let iface = self
            .conn
            .object_server()
            .interface::<_, Profiles>(ROOT)
            .await
            .expect("profiles");
        iface.get_mut().await.dwell_until = until_us;
        iface
            .get()
            .await
            .dwell_until_changed(iface.signal_emitter())
            .await
            .expect("changed");
    }

    async fn leakshield_iface(&self) -> zbus::object_server::InterfaceRef<Leakshield> {
        self.conn
            .object_server()
            .interface::<_, Leakshield>(path(&format!("leakshield/{LEAKSHIELD}")))
            .await
            .expect("leakshield")
    }

    /// The slot of report 1 changes.
    pub async fn set_active_slot(&self, slot: u8) {
        let iface = self.leakshield_iface().await;
        iface.get().await.live.lock().expect("live").active_slot = slot;
        iface
            .get()
            .await
            .active_slot_changed(iface.signal_emitter())
            .await
            .expect("changed");
    }

    /// The backup id of a failed Profile write: an empty id is a good write.
    pub async fn set_failed_write_backup(&self, backup: &str) {
        let iface = self.leakshield_iface().await;
        iface
            .get()
            .await
            .live
            .lock()
            .expect("live")
            .failed_write_backup = backup.to_owned();
        iface
            .get()
            .await
            .failed_write_backup_changed(iface.signal_emitter())
            .await
            .expect("changed");
    }

    /// A good Feed send: the time, the pump speed and the flow.
    pub async fn set_last_feed(&self, time_us: u64, pump: f64, flow: f64) {
        let iface = self.leakshield_iface().await;
        iface.get().await.live.lock().expect("live").last_feed = (time_us, pump, flow);
        iface
            .get()
            .await
            .last_feed_changed(iface.signal_emitter())
            .await
            .expect("changed");
    }

    /// A Leakshield write that failed after a Profile activation.
    pub async fn emit_write_failed(&self, error: &str, backup: &str) {
        let iface = self
            .conn
            .object_server()
            .interface::<_, Leakshield>(path(&format!("leakshield/{LEAKSHIELD}")))
            .await
            .expect("leakshield");
        Leakshield::write_failed(iface.signal_emitter(), error, backup)
            .await
            .expect("signal");
    }

    pub async fn emit_external_change(&self, fields: Vec<String>) {
        let iface = self
            .conn
            .object_server()
            .interface::<_, Leakshield>(path(&format!("leakshield/{LEAKSHIELD}")))
            .await
            .expect("leakshield");
        Leakshield::external_change(iface.signal_emitter(), fields)
            .await
            .expect("signal");
    }

    /// Leave the bus, as a stopped daemon does.
    pub async fn stop(self) {
        let _ = self.conn.release_name(BUS_NAME).await;
    }
}
