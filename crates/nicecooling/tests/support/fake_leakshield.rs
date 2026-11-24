//! The Leakshield interface of the fake daemon: a small device with stored fields, events,
//! backups and 12 Software sensor slots. Each write call is recorded in `Shared::calls`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use zbus::object_server::SignalEmitter;
use zbus::{OwnedObjectPath, fdo, interface};
use zbus::{OwnedVariant as OwnedValue, Variant as Value};

use super::fake_daemon::{LEAKSHIELD, Shared, path};

const NOT_SUPPORTED: &str = "io.github.cappyt.NiceCooling1.Error.NotSupported";

#[derive(Clone, Debug, PartialEq)]
pub enum Fv {
    /// A scaled field: `d`.
    Num(f64),
    /// An integer field: `x`.
    Int(i64),
    /// An enum field: `s`.
    Text(String),
    /// A scaled field with several elements: `ad`.
    List(Vec<f64>),
    /// An integer field with several elements: `ax`.
    Ints(Vec<i64>),
    /// An enum field with several elements: `as`.
    TextList(Vec<String>),
}

#[derive(Clone)]
struct Field {
    id: &'static str,
    risk: &'static str,
    value: Fv,
    writable: bool,
    reason: &'static str,
}

struct Backup {
    id: String,
    time_us: u64,
    reason: &'static str,
}

struct Device {
    fields: Vec<Field>,
    mode: String,
    action: String,
    backups: Vec<Backup>,
    sensors: Vec<Option<String>>,
    next_backup: u32,
}

/// What a test can change on the fake device.
#[derive(Default, Clone, Copy)]
pub struct DeviceOptions {
    /// The registry of this firmware has no `cancel_action`, `reset_alarm`,
    /// `reset_refill_history` and `reset_service_counter`.
    pub no_reset_commands: bool,
    /// The Feed is on and nothing was sent yet: `FeedState` is `waiting`, `LastFeed` is zero.
    pub feed_waiting: bool,
}

/// What a test changes on the properties of the fake Leakshield while it runs.
pub struct Live {
    pub active_slot: u8,
    pub failed_write_backup: String,
    pub last_feed: (u64, f64, f64),
    /// While it is set, `GetFields` waits until the gate has a permit.
    pub gate: Option<Arc<tokio::sync::Semaphore>>,
    pub connected: bool,
    /// The stored settings of each slot. A slot that is not here has no stored report 3.
    pub stored: std::collections::BTreeMap<u8, HashMap<String, Fv>>,
}

pub struct Leakshield {
    pub live: Mutex<Live>,
    firmware: u16,
    read_only_reason: String,
    options: DeviceOptions,
    shared: Arc<Shared>,
    device: Mutex<Device>,
}

fn num(id: &'static str, value: f64) -> Field {
    field(id, "low", Fv::Num(value), true)
}

fn int(id: &'static str, value: i64) -> Field {
    field(id, "low", Fv::Int(value), true)
}

fn ints(value: &[i64]) -> Fv {
    Fv::Ints(value.to_vec())
}

fn texts(value: &[&str]) -> Fv {
    Fv::TextList(value.iter().map(|v| (*v).to_owned()).collect())
}

fn field(id: &'static str, risk: &'static str, value: Fv, writable: bool) -> Field {
    Field {
        id,
        risk,
        value,
        writable,
        reason: if writable { "" } else { "not-verified" },
    }
}

fn safety(id: &'static str, value: Fv) -> Field {
    field(id, "safety", value, true)
}

fn locked(id: &'static str, value: Fv) -> Field {
    field(id, "safety", value, false)
}

fn text(value: &str) -> Fv {
    Fv::Text(value.to_owned())
}

fn default_fields() -> Vec<Field> {
    let pages = |per_page: usize, fill: i64| Fv::Ints(vec![fill; 10 * per_page]);
    vec![
        safety("default_mode", text("shield")),
        safety("test_pressure", Fv::Num(300.0)),
        safety("deaeration_pressure", Fv::Num(300.0)),
        safety("refill_pressure", Fv::Num(300.0)),
        locked("shield_pressure_low", Fv::Num(250.0)),
        locked("shield_pressure_working_range", Fv::Num(50.0)),
        locked("shield_pressure_max_range", Fv::Num(50.0)),
        safety("shield_pressure_low_standby", Fv::Num(80.0)),
        locked("monitor_pressure_low", Fv::Num(100.0)),
        locked("monitor_pressure_working_range", Fv::Num(20.0)),
        locked("monitor_pressure_max_range", Fv::Num(20.0)),
        safety("shield_flags", Fv::Int(127)),
        safety("vacuum_timer_start_hour", ints(&[9, 18])),
        safety("vacuum_timer_start_minute", ints(&[0, 0])),
        safety("vacuum_timer_end_hour", ints(&[9, 18])),
        safety("vacuum_timer_end_minute", ints(&[1, 1])),
        safety("reservoir_model", text("custom")),
        locked("reservoir_total_volume", Fv::Int(270)),
        safety("reservoir_air_volume_full", Fv::Int(65)),
        safety("reservoir_air_volume_empty", Fv::Int(260)),
        safety("pump_model", text("d5")),
        safety("pump_max_speed", Fv::Int(4800)),
        safety("pump_max_pressure", Fv::Num(375.0)),
        safety("pump_start_speed", Fv::Int(2500)),
        safety("pump_start_flow", Fv::Num(130.0)),
        safety("alarm_enable", Fv::Int(15)),
        safety("alarm_buzzer", Fv::Int(1)),
        safety("fill_level_warning_threshold", Fv::Num(60.0)),
        safety("fill_level_alarm_threshold", Fv::Num(20.0)),
        safety("leak_rate_warning_threshold", Fv::Int(150)),
        safety("fill_level_warning_response_action", text("none")),
        safety("fill_level_alarm_response_action", text("none")),
        safety("leak_rate_warning_response_action", text("none")),
        safety("leak_rate_alarm_1_response_action", text("none")),
        safety("leak_rate_alarm_2_response_action", text("signal-long")),
        locked("system_alarm_response_action", text("none")),
        safety("standby_conditions", Fv::Int(11)),
        safety("daily_pump_time_warning", Fv::Int(5)),
        field("sensor_log_interval", "low", texts(&["1s"; 20]), true),
        field("sensor_unit", "low", texts(&["none"; 20]), true),
        num("display_page_time", 10.0),
        field("display_brightness_active", "low", text("low"), true),
        field("display_brightness_idle", "low", text("low"), true),
        int("display_idle_delay", 2400),
        int("display_flags", 0),
        safety("clock_flags", Fv::Int(1)),
        safety("clock_timezone", Fv::Num(1.0)),
        field(
            "page_template",
            "low",
            texts(&[
                "leakshield-state",
                "values-1",
                "chart",
                "values-2",
                "values-2",
                "chart",
                "time",
                "none",
                "none",
                "none",
            ]),
            true,
        ),
        field("page_options", "low", pages(1, 1), true),
        field("page_sources", "low", pages(8, 255), true),
        field("page_precisions", "low", pages(8, 0), true),
        field("page_icons", "low", pages(4, 255), true),
        num("led_strip_brightness", 60.0),
        int("led_strip_flags", 4),
        field(
            "led_controller_mode",
            "low",
            texts(&["rainbow", "fixed-color"]),
            true,
        ),
        field("led_controller_first_led", "low", ints(&[0, 2]), true),
        field("led_controller_led_count", "low", ints(&[6, 3]), true),
        field("led_controller_source", "low", ints(&[-1, -1]), true),
        field(
            "led_controller_parameters",
            "low",
            Fv::Ints([50, 100, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].repeat(2)),
            true,
        ),
        field(
            "led_controller_color_hue",
            "low",
            Fv::Ints(vec![0; 12]),
            true,
        ),
        field(
            "led_controller_color_saturation",
            "low",
            Fv::Ints(vec![255; 12]),
            true,
        ),
        field(
            "led_controller_color_value",
            "low",
            Fv::Ints(vec![255; 12]),
            true,
        ),
        int("device_profile", 1),
    ]
}

impl Leakshield {
    pub fn new(
        firmware: u16,
        read_only_reason: &str,
        options: DeviceOptions,
        shared: Arc<Shared>,
    ) -> Self {
        let mut fields = default_fields();
        if !read_only_reason.is_empty() {
            for field in &mut fields {
                field.writable = false;
                field.reason = "unknown-firmware";
            }
        }
        let thresholds = |warning: f64| -> HashMap<String, Fv> {
            HashMap::from([
                ("fill_level_warning_threshold".to_owned(), Fv::Num(warning)),
                ("fill_level_alarm_threshold".to_owned(), Fv::Num(20.0)),
                ("leak_rate_warning_threshold".to_owned(), Fv::Num(150.0)),
            ])
        };
        let last_feed = if options.feed_waiting {
            (0, 0.0, 0.0)
        } else {
            (1_700_000_000_000_000, 2840.0, 115.2)
        };
        Self {
            live: Mutex::new(Live {
                active_slot: 1,
                failed_write_backup: String::new(),
                last_feed,
                gate: None,
                connected: true,
                // Slot 3 has no stored report. Slot 2 has other thresholds than the device.
                stored: [(1, 60.0), (2, 50.0), (4, 60.0)]
                    .into_iter()
                    .map(|(slot, warning)| (slot, thresholds(warning)))
                    .collect(),
            }),
            firmware,
            read_only_reason: read_only_reason.to_owned(),
            options,
            shared,
            device: Mutex::new(Device {
                fields,
                mode: "shield".into(),
                action: String::new(),
                backups: vec![Backup {
                    id: "backup_1".into(),
                    time_us: 1_700_000_000_000_000,
                    reason: "before-write",
                }],
                sensors: vec![None; 12],
                next_backup: 2,
            }),
        }
    }

    pub fn set_device_mode(&self, mode: &str) {
        self.device.lock().expect("device").mode = mode.to_owned();
    }

    /// The value of a stored field, for a test.
    pub fn stored(&self, id: &str) -> Option<Fv> {
        let device = self.device.lock().expect("device");
        device
            .fields
            .iter()
            .find(|field| field.id == id)
            .map(|field| field.value.clone())
    }

    fn not_supported() -> fdo::Error {
        fdo::Error::Failed(NOT_SUPPORTED.into())
    }
}

fn to_value(value: &Fv) -> OwnedValue {
    let value = match value {
        Fv::Num(number) => Value::F64(*number),
        Fv::Int(number) => Value::I64(*number),
        Fv::Text(text) => Value::from(text.clone()),
        Fv::List(list) => Value::from(list.clone()),
        Fv::Ints(list) => Value::from(list.clone()),
        Fv::TextList(list) => Value::from(list.clone()),
    };
    OwnedValue::try_from(value).expect("value")
}

fn whole(value: &Value<'_>) -> Option<i64> {
    Some(match value {
        Value::U8(v) => i64::from(*v),
        Value::I16(v) => i64::from(*v),
        Value::U16(v) => i64::from(*v),
        Value::I32(v) => i64::from(*v),
        Value::U32(v) => i64::from(*v),
        Value::I64(v) => *v,
        Value::U64(v) => i64::try_from(*v).ok()?,
        _ => return None,
    })
}

/// The value as the wire types of `docs/api.md` (Leakshield fields) carry it: `d`, an integer
/// type, `s`, or an array of one of them.
fn from_value(value: &OwnedValue) -> Option<Fv> {
    let mut value: &Value<'_> = value;
    while let Value::Variant(inner) = value {
        value = inner;
    }
    Some(match value {
        Value::F64(number) => Fv::Num(*number),
        Value::Str(text) => Fv::Text(text.to_string()),
        Value::Array(items) => {
            let items: Vec<&Value<'_>> = items.iter().collect();
            match items.first() {
                Some(Value::F64(_)) => Fv::List(
                    items
                        .iter()
                        .map(|item| match item {
                            Value::F64(number) => Some(*number),
                            _ => None,
                        })
                        .collect::<Option<_>>()?,
                ),
                Some(Value::Str(_)) => Fv::TextList(
                    items
                        .iter()
                        .map(|item| match item {
                            Value::Str(text) => Some(text.to_string()),
                            _ => None,
                        })
                        .collect::<Option<_>>()?,
                ),
                _ => Fv::Ints(
                    items
                        .iter()
                        .map(|item| whole(item))
                        .collect::<Option<_>>()?,
                ),
            }
        }
        other => Fv::Int(whole(other)?),
    })
}

const REGISTRY: &str = include_str!("../../../nicecooling-protocol/registry/leakshield.toml");

/// The names of the enum field `id` in the registry, as the daemon checks them.
fn registry_names(id: &str) -> Vec<String> {
    static TABLE: std::sync::OnceLock<toml::Table> = std::sync::OnceLock::new();
    let table = TABLE.get_or_init(|| REGISTRY.parse().expect("registry"));
    table["field"]
        .as_array()
        .expect("fields")
        .iter()
        .find(|f| f["id"].as_str() == Some(id))
        .and_then(|f| f.get("names"))
        .and_then(|names| names.as_array())
        .map(|names| {
            names
                .iter()
                .filter_map(|n| n.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn len(value: &Fv) -> usize {
    match value {
        Fv::TextList(list) => list.len(),
        Fv::List(list) => list.len(),
        Fv::Ints(list) => list.len(),
        _ => 1,
    }
}

/// What the daemon does with a value for a stored field: a listed name goes to an enum field only,
/// a number goes to a scaled field, and an integer field takes an integer or a whole `d`. A list
/// keeps the length of the field.
fn fit(id: &str, stored: &Fv, value: Fv) -> Option<Fv> {
    let whole_number = |n: f64| (n.fract() == 0.0).then_some(n as i64);
    if len(stored) != len(&value) {
        return None;
    }
    let listed = |name: &String| registry_names(id).contains(name);
    match (stored, value) {
        (Fv::Text(_), Fv::Text(name)) => listed(&name).then_some(Fv::Text(name)),
        (Fv::TextList(_), Fv::TextList(names)) => {
            names.iter().all(listed).then_some(Fv::TextList(names))
        }
        (Fv::Num(_), Fv::Num(n)) => Some(Fv::Num(n)),
        (Fv::Num(_), Fv::Int(n)) => Some(Fv::Num(n as f64)),
        (Fv::List(_), value @ Fv::List(_)) => Some(value),
        (Fv::List(_), Fv::Ints(list)) => Some(Fv::List(list.iter().map(|n| *n as f64).collect())),
        (Fv::Int(_), value @ Fv::Int(_)) | (Fv::Ints(_), value @ Fv::Ints(_)) => Some(value),
        (Fv::Int(_), Fv::Num(n)) => whole_number(n).map(Fv::Int),
        (Fv::Ints(_), Fv::List(list)) => list
            .iter()
            .map(|n| whole_number(*n))
            .collect::<Option<_>>()
            .map(Fv::Ints),
        _ => None,
    }
}

/// The alarm events with a Device response: event suffix and the field of the response.
const ALARMS: [(&str, &str); 6] = [
    ("leak-warning", "leak_rate_warning_response_action"),
    ("leak-alarm-small", "leak_rate_alarm_1_response_action"),
    ("leak-alarm", "leak_rate_alarm_2_response_action"),
    (
        "fill-level-low-warning",
        "fill_level_warning_response_action",
    ),
    ("fill-level-low-alarm", "fill_level_alarm_response_action"),
    ("system-alarm", "system_alarm_response_action"),
];

#[interface(name = "io.github.cappyt.NiceCooling1.Leakshield")]
impl Leakshield {
    #[zbus(property)]
    fn id(&self) -> String {
        LEAKSHIELD.into()
    }

    #[zbus(property)]
    fn serial(&self) -> String {
        "20417-31072".into()
    }

    #[zbus(property)]
    fn firmware(&self) -> u16 {
        self.firmware
    }

    #[zbus(property)]
    fn structure(&self) -> u16 {
        100
    }

    #[zbus(property)]
    fn connected(&self) -> bool {
        self.live.lock().expect("live").connected
    }

    #[zbus(property)]
    fn writable(&self) -> bool {
        self.read_only_reason.is_empty()
    }

    #[zbus(property)]
    fn read_only_reason(&self) -> String {
        self.read_only_reason.clone()
    }

    #[zbus(property)]
    fn mode(&self) -> String {
        self.device.lock().expect("device").mode.clone()
    }

    #[zbus(property)]
    fn running_action(&self) -> String {
        self.device.lock().expect("device").action.clone()
    }

    #[zbus(property)]
    fn clock_offset(&self) -> i64 {
        -42
    }

    #[zbus(property)]
    fn software_sensor_slots(&self) -> u16 {
        12
    }

    #[zbus(property)]
    fn feed_state(&self) -> String {
        if self.options.feed_waiting {
            "waiting"
        } else {
            "ok"
        }
        .into()
    }

    #[zbus(property)]
    fn last_feed(&self) -> (u64, f64, f64) {
        self.live.lock().expect("live").last_feed
    }

    #[zbus(property)]
    fn active_slot(&self) -> u8 {
        self.live.lock().expect("live").active_slot
    }

    #[zbus(property)]
    fn failed_write_backup(&self) -> String {
        self.live.lock().expect("live").failed_write_backup.clone()
    }

    /// (field id, risk class, value, writable, reason)
    async fn get_fields(&self) -> Vec<(String, String, OwnedValue, bool, String)> {
        let gate = self.live.lock().expect("live").gate.clone();
        if let Some(gate) = gate {
            gate.acquire().await.expect("gate").forget();
        }
        let device = self.device.lock().expect("device");
        device
            .fields
            .iter()
            .map(|field| {
                (
                    field.id.to_owned(),
                    field.risk.to_owned(),
                    to_value(&field.value),
                    field.writable,
                    field.reason.to_owned(),
                )
            })
            .collect()
    }

    async fn set_fields(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        fields: HashMap<String, OwnedValue>,
    ) -> fdo::Result<()> {
        let mut names: Vec<&String> = fields.keys().collect();
        names.sort();
        let list: Vec<String> = names
            .iter()
            .map(|name| {
                let value = from_value(&fields[*name]);
                match value {
                    Some(Fv::Num(number)) => format!("{name}={number}"),
                    Some(Fv::Int(number)) => format!("{name}={number}"),
                    Some(Fv::Text(text)) => format!("{name}={text}"),
                    Some(Fv::List(list)) => format!("{name}={list:?}"),
                    Some(Fv::Ints(list)) => format!("{name}={list:?}"),
                    Some(Fv::TextList(list)) => format!("{name}={list:?}"),
                    None => format!("{name}=?"),
                }
            })
            .collect();
        self.shared
            .record(&format!("SetFields({})", list.join(",")), &header)?;
        let mut device = self.device.lock().expect("device");
        for (name, value) in &fields {
            let Some(stored) = device.fields.iter().find(|field| field.id == name) else {
                return Err(fdo::Error::Failed(
                    "io.github.cappyt.NiceCooling1.Error.InvalidArgument".into(),
                ));
            };
            if !stored.writable {
                return Err(fdo::Error::Failed(
                    "io.github.cappyt.NiceCooling1.Error.NotVerified".into(),
                ));
            }
            if from_value(value)
                .and_then(|v| fit(name, &stored.value, v))
                .is_none()
            {
                return Err(fdo::Error::Failed(
                    "io.github.cappyt.NiceCooling1.Error.InvalidArgument".into(),
                ));
            }
        }
        for (name, value) in &fields {
            if let Some(stored) = device.fields.iter_mut().find(|field| field.id == name) {
                stored.value = from_value(value)
                    .and_then(|v| fit(name, &stored.value, v))
                    .expect("checked");
            }
        }
        Ok(())
    }

    /// The stored settings of a slot. A slot without a stored report 3 is `NotFound`.
    async fn get_stored_fields(&self, slot: u8) -> fdo::Result<HashMap<String, OwnedValue>> {
        let error =
            |name: &str| fdo::Error::Failed(format!("io.github.cappyt.NiceCooling1.Error.{name}"));
        if !(1..=4).contains(&slot) {
            return Err(error("InvalidArgument"));
        }
        let live = self.live.lock().expect("live");
        let stored = live.stored.get(&slot).ok_or_else(|| error("NotFound"))?;
        Ok(stored
            .iter()
            .map(|(id, value)| (id.clone(), to_value(value)))
            .collect())
    }

    /// The active slot and a slot without a stored report are refused, as the daemon does.
    async fn set_stored_fields(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        slot: u8,
        fields: HashMap<String, OwnedValue>,
    ) -> fdo::Result<()> {
        let mut names: Vec<&String> = fields.keys().collect();
        names.sort();
        let list: Vec<&str> = names.iter().map(|name| name.as_str()).collect();
        self.shared.record(
            &format!("SetStoredFields({slot},{})", list.join(",")),
            &header,
        )?;
        let error =
            |name: &str| fdo::Error::Failed(format!("io.github.cappyt.NiceCooling1.Error.{name}"));
        let mut live = self.live.lock().expect("live");
        if slot == live.active_slot {
            return Err(error("InvalidArgument"));
        }
        let stored = live
            .stored
            .get_mut(&slot)
            .ok_or_else(|| error("NotFound"))?;
        for (id, value) in &fields {
            if let Some(value) = from_value(value) {
                stored.insert(id.clone(), value);
            }
        }
        Ok(())
    }

    async fn set_mode(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        mode: &str,
    ) -> fdo::Result<()> {
        self.shared.record(&format!("SetMode({mode})"), &header)?;
        self.device.lock().expect("device").mode = mode.to_owned();
        self.mode_changed(&emitter).await?;
        Ok(())
    }

    async fn start_action(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        action: &str,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("StartAction({action})"), &header)?;
        self.device.lock().expect("device").action = action.to_owned();
        self.running_action_changed(&emitter).await?;
        Ok(())
    }

    async fn cancel_action(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        self.shared.record("CancelAction()", &header)?;
        if self.options.no_reset_commands {
            return Err(Self::not_supported());
        }
        self.device.lock().expect("device").action.clear();
        self.running_action_changed(&emitter).await?;
        Ok(())
    }

    async fn sync_clock(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> fdo::Result<()> {
        self.shared.record("SyncClock()", &header)
    }

    async fn reset_alarms(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> fdo::Result<()> {
        self.shared.record("ResetAlarms()", &header)?;
        if self.options.no_reset_commands {
            return Err(Self::not_supported());
        }
        Ok(())
    }

    async fn reset_refill_history(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> fdo::Result<()> {
        self.shared.record("ResetRefillHistory()", &header)?;
        if self.options.no_reset_commands {
            return Err(Self::not_supported());
        }
        Ok(())
    }

    async fn reset_service_counter(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> fdo::Result<()> {
        self.shared.record("ResetServiceCounter()", &header)?;
        if self.options.no_reset_commands {
            return Err(Self::not_supported());
        }
        Ok(())
    }

    /// (device time us, event identifier, details)
    async fn get_events(&self) -> Vec<(u64, String, HashMap<String, OwnedValue>)> {
        let detail = |pairs: &[(&str, Value<'_>)]| {
            pairs
                .iter()
                .map(|(key, value)| {
                    (
                        (*key).to_owned(),
                        OwnedValue::try_from(value.try_clone().expect("clone")).expect("value"),
                    )
                })
                .collect::<HashMap<_, _>>()
        };
        vec![
            (
                1_700_000_100_000_000,
                "mode-changed".into(),
                detail(&[
                    ("from", Value::from("monitor")),
                    ("to", Value::from("shield")),
                ]),
            ),
            (
                1_700_000_050_000_000,
                "vacuum-pump-started".into(),
                detail(&[("pressure", Value::F64(281.0))]),
            ),
            (1_700_000_000_000_000, "system-start".into(), HashMap::new()),
        ]
    }

    async fn get_software_sensors(&self) -> Vec<(u16, OwnedObjectPath)> {
        let device = self.device.lock().expect("device");
        device
            .sensors
            .iter()
            .enumerate()
            .map(|(index, sensor)| {
                (
                    index as u16 + 1,
                    sensor
                        .as_ref()
                        .map_or_else(path_none, |id| path(&format!("sensor/{id}"))),
                )
            })
            .collect()
    }

    async fn set_software_sensor(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        slot: u16,
        sensor: OwnedObjectPath,
    ) -> fdo::Result<()> {
        let id = sensor.as_str().rsplit('/').next().unwrap_or_default();
        let shown = if sensor.as_str() == "/" { "" } else { id };
        self.shared
            .record(&format!("SetSoftwareSensor({slot},{shown})"), &header)?;
        let mut device = self.device.lock().expect("device");
        let entry = device
            .sensors
            .get_mut(usize::from(slot).wrapping_sub(1))
            .ok_or_else(|| {
                fdo::Error::Failed("io.github.cappyt.NiceCooling1.Error.InvalidArgument".into())
            })?;
        *entry = (sensor.as_str() != "/").then(|| id.to_owned());
        Ok(())
    }

    /// (backup id, time us, sha256 hex, firmware, reason)
    async fn list_backups(&self) -> Vec<(String, u64, String, u16, String)> {
        let device = self.device.lock().expect("device");
        device
            .backups
            .iter()
            .map(|backup| {
                (
                    backup.id.clone(),
                    backup.time_us,
                    "ab".repeat(32),
                    self.firmware,
                    backup.reason.to_owned(),
                )
            })
            .collect()
    }

    async fn create_backup(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> fdo::Result<String> {
        self.shared.record("CreateBackup()", &header)?;
        let mut device = self.device.lock().expect("device");
        let id = format!("backup_{}", device.next_backup);
        device.next_backup += 1;
        let time_us = 1_700_000_200_000_000 + u64::from(device.next_backup) * 1_000_000;
        device.backups.push(Backup {
            id: id.clone(),
            time_us,
            reason: "manual",
        });
        Ok(id)
    }

    async fn get_backup(&self, backup: &str) -> fdo::Result<(u16, Vec<u8>, Vec<u8>, Vec<u8>)> {
        let device = self.device.lock().expect("device");
        if device.backups.iter().any(|b| b.id == backup) {
            Ok((1022, vec![3, 0, 100], vec![5, 1], vec![8, 2]))
        } else {
            Err(fdo::Error::Failed(
                "io.github.cappyt.NiceCooling1.Error.NotFound".into(),
            ))
        }
    }

    async fn import_backup(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        firmware: u16,
        report3: Vec<u8>,
        report5: Vec<u8>,
        report8: Vec<u8>,
    ) -> fdo::Result<String> {
        self.shared.record(
            &format!(
                "ImportBackup({firmware},{},{},{})",
                report3.len(),
                report5.len(),
                report8.len()
            ),
            &header,
        )?;
        let mut device = self.device.lock().expect("device");
        let id = format!("backup_{}", device.next_backup);
        device.next_backup += 1;
        let time_us = 1_700_000_300_000_000 + u64::from(device.next_backup) * 1_000_000;
        device.backups.push(Backup {
            id: id.clone(),
            time_us,
            reason: "import",
        });
        Ok(id)
    }

    async fn restore_backup(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        backup: &str,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("RestoreBackup({LEAKSHIELD},{backup})"), &header)
    }

    async fn factory_reset(
        &self,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> fdo::Result<()> {
        self.shared.record("FactoryReset()", &header)
    }

    /// (device alarm id, current Device response, proposed Device response, overlap warning id)
    async fn get_device_response_proposals(&self) -> Vec<(String, String, String, String)> {
        let device = self.device.lock().expect("device");
        let current = |response_field: &str| match device
            .fields
            .iter()
            .find(|field| field.id == response_field)
            .map(|field| &field.value)
        {
            Some(Fv::Text(name)) => name.clone(),
            _ => "none".to_owned(),
        };
        ALARMS
            .iter()
            .map(|(alarm, response_field)| {
                let now = current(response_field);
                let (proposed, overlap) = match *alarm {
                    "system-alarm" => ("signal-long".to_owned(), String::new()),
                    // The fake daemon powers off after a large leak, so a signal overlaps it.
                    "leak-alarm" if now.starts_with("signal") => {
                        ("none".to_owned(), "double-poweroff".to_owned())
                    }
                    _ if now == "key-sequence" => ("none".to_owned(), String::new()),
                    _ => (now.clone(), String::new()),
                };
                (format!("leakshield.{alarm}"), now, proposed, overlap)
            })
            .collect()
    }

    #[zbus(signal)]
    pub async fn external_change(
        emitter: &SignalEmitter<'_>,
        fields: Vec<String>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn write_failed(
        emitter: &SignalEmitter<'_>,
        error: &str,
        backup: &str,
    ) -> zbus::Result<()>;
}

fn path_none() -> OwnedObjectPath {
    OwnedObjectPath::try_from("/").expect("path")
}
