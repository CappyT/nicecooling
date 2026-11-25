//! The Graph model: Graph, node, connection, the 51 kinds with their pins and parameters, and the
//! JSON forms of `graphs.json` and of the export file.

use std::fmt;

use nicecooling_core::graph_sensor::graph_sensor_unit;
use serde::de::{self, Deserializer};
use serde::ser::Serializer;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use serde_json::{Map, Value};

/// The `schema_version` of `graphs.json`.
pub const SCHEMA_VERSION: u32 = 1;
/// The `format` of an export file.
pub const EXPORT_FORMAT: &str = "nicecooling-graph";
/// The `format_version` of an export file.
pub const EXPORT_FORMAT_VERSION: u32 = 1;
/// The name of the output pin of every node that has one.
pub const OUTPUT_PIN: &str = "out";

macro_rules! kinds {
    ($($variant:ident => $name:literal,)*) => {
        /// The kind of a node. The JSON name of each kind is fixed by the spec.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Kind {
            $($variant,)*
        }

        impl Kind {
            /// Every kind, in the order of the node list.
            pub const ALL: &'static [Kind] = &[$(Kind::$variant,)*];

            /// The JSON name of the kind.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Kind::$variant => $name,)*
                }
            }

            pub fn parse(text: &str) -> Option<Kind> {
                match text {
                    $($name => Some(Kind::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

kinds! {
    SensorSource => "sensor_source",
    AlarmEventSource => "alarm_event_source",
    Constant => "constant",
    Random => "random",
    Process => "process",
    Add => "add",
    Subtract => "subtract",
    Multiply => "multiply",
    Divide => "divide",
    Minimum => "minimum",
    Maximum => "maximum",
    Average => "average",
    Logarithm => "logarithm",
    Power => "power",
    Sine => "sine",
    Cosine => "cosine",
    Tangent => "tangent",
    SquareRoot => "square_root",
    Round => "round",
    Limiter => "limiter",
    Table => "table",
    Less => "less",
    LessThanOrEqual => "less_equal",
    Equal => "equal",
    GreaterThanOrEqual => "greater_equal",
    Greater => "greater",
    Different => "not_equal",
    And => "and",
    Or => "or",
    Not => "not",
    Switch => "switch",
    HasValue => "has_value",
    Memory => "memory",
    PersistentStorage => "persistent_storage",
    Trigger => "trigger",
    RiseDelay => "on_delay",
    FallDelay => "off_delay",
    Counter => "counter",
    Peak => "peak",
    WindowMinimum => "window_minimum",
    WindowAverage => "window_average",
    LowPass => "low_pass",
    Trend => "trend",
    LinearGen => "generator_linear",
    SawtoothGen => "generator_sawtooth",
    TriangleGen => "generator_triangle",
    SineGen => "generator_sine",
    RectangleGen => "generator_rectangle",
    Comment => "comment",
    SensorEnd => "sensor_end",
    ResponseEnd => "response_end",
}

impl Serialize for Kind {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Kind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Kind::parse(&text).ok_or_else(|| de::Error::custom(format!("unknown kind {text:?}")))
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The groups of the node list of the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Sources,
    Math,
    Logic,
    StateAndTime,
    Filters,
    Generators,
    Other,
}

/// One input pin of a kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PinSpec {
    pub name: &'static str,
    /// The value of the pin while it has no connection. `None` for an end node, which has no
    /// default, and for the node "has value", where a pin with no connection has no value.
    pub default: Option<f64>,
}

const fn pin(name: &'static str, default: f64) -> PinSpec {
    PinSpec {
        name,
        default: Some(default),
    }
}

const NO_PIN: [PinSpec; 0] = [];
const PINS_A: [PinSpec; 1] = [pin("a", 0.0)];
const PINS_AB: [PinSpec; 2] = [pin("a", 0.0), pin("b", 0.0)];
const PINS_LOGARITHM: [PinSpec; 2] = [pin("a", 0.0), pin("b", std::f64::consts::E)];
const PINS_POWER: [PinSpec; 2] = [pin("a", 0.0), pin("b", 2.0)];
const PINS_SWITCH: [PinSpec; 3] = [pin("s", 0.0), pin("a", 0.0), pin("b", 0.0)];
const PINS_MEMORY: [PinSpec; 2] = [pin("s", 0.0), pin("a", 0.0)];
const PINS_COUNTER: [PinSpec; 4] = [
    pin("count", 0.0),
    pin("enable", 1.0),
    pin("reset", 0.0),
    pin("period", 10.0),
];
const PINS_GENERATOR: [PinSpec; 3] = [pin("enable", 1.0), pin("reset", 0.0), pin("period", 10.0)];
const PINS_NO_DEFAULT: [PinSpec; 1] = [PinSpec {
    name: "a",
    default: None,
}];

impl Kind {
    /// The input pins, in the order of the node.
    pub fn inputs(self) -> &'static [PinSpec] {
        use Kind::*;
        match self {
            SensorSource | AlarmEventSource | Constant | Random | Process | Comment => &NO_PIN,
            Add | Subtract | Multiply | Divide | Minimum | Maximum | Average | Less
            | LessThanOrEqual | Equal | GreaterThanOrEqual | Greater | Different | And | Or => {
                &PINS_AB
            }
            Logarithm => &PINS_LOGARITHM,
            Power => &PINS_POWER,
            Sine | Cosine | Tangent | SquareRoot | Round | Limiter | Table | Not | Trigger
            | RiseDelay | FallDelay | Peak | WindowMinimum | WindowAverage | LowPass | Trend => {
                &PINS_A
            }
            Switch => &PINS_SWITCH,
            HasValue | SensorEnd | ResponseEnd => &PINS_NO_DEFAULT,
            Memory | PersistentStorage => &PINS_MEMORY,
            Counter => &PINS_COUNTER,
            LinearGen | SawtoothGen | TriangleGen | SineGen | RectangleGen => &PINS_GENERATOR,
        }
    }

    pub fn input(self, name: &str) -> Option<&'static PinSpec> {
        self.inputs().iter().find(|p| p.name == name)
    }

    /// An end node and the comment node have no output pin.
    pub const fn has_output(self) -> bool {
        !matches!(self, Kind::SensorEnd | Kind::ResponseEnd | Kind::Comment)
    }

    pub const fn is_end(self) -> bool {
        matches!(self, Kind::SensorEnd | Kind::ResponseEnd)
    }

    pub const fn is_window(self) -> bool {
        matches!(
            self,
            Kind::Peak | Kind::WindowMinimum | Kind::WindowAverage | Kind::LowPass | Kind::Trend
        )
    }

    pub const fn family(self) -> Family {
        use Kind::*;
        match self {
            SensorSource | AlarmEventSource | Constant | Random | Process => Family::Sources,
            Add | Subtract | Multiply | Divide | Minimum | Maximum | Average | Logarithm
            | Power | Sine | Cosine | Tangent | SquareRoot | Round | Limiter | Table => {
                Family::Math
            }
            Less | LessThanOrEqual | Equal | GreaterThanOrEqual | Greater | Different | And
            | Or | Not | Switch | HasValue => Family::Logic,
            Memory | PersistentStorage | Trigger | RiseDelay | FallDelay | Counter => {
                Family::StateAndTime
            }
            Peak | WindowMinimum | WindowAverage | LowPass | Trend => Family::Filters,
            LinearGen | SawtoothGen | TriangleGen | SineGen | RectangleGen => Family::Generators,
            Comment | SensorEnd | ResponseEnd => Family::Other,
        }
    }
}

/// The kinds of a response end node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResponseKind {
    Notification,
    Sound,
    Command,
    Email,
    Mqtt,
    Poweroff,
    Reboot,
    Suspend,
}

impl ResponseKind {
    pub const ALL: [ResponseKind; 8] = [
        Self::Notification,
        Self::Sound,
        Self::Command,
        Self::Email,
        Self::Mqtt,
        Self::Poweroff,
        Self::Reboot,
        Self::Suspend,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Notification => "notification",
            Self::Sound => "sound",
            Self::Command => "command",
            Self::Email => "email",
            Self::Mqtt => "mqtt",
            Self::Poweroff => "poweroff",
            Self::Reboot => "reboot",
            Self::Suspend => "suspend",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.as_str() == text)
    }

    /// Poweroff, reboot and suspend.
    pub const fn is_power(self) -> bool {
        matches!(self, Self::Poweroff | Self::Reboot | Self::Suspend)
    }

    /// The default minimum interval between two runs, in seconds.
    pub const fn default_min_interval_s(self) -> u32 {
        if self.is_power() { 600 } else { 0 }
    }
}

/// The type and the range of one parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamType {
    /// A finite number in the range.
    Number { min: f64, max: f64 },
    /// A whole number of seconds in the range.
    Seconds { min: u32, max: u32 },
    /// A whole number in the range that is no time (the decimals of the round node).
    Whole { min: u32, max: u32 },
    /// `null` or a whole number of seconds in the range.
    OptionalSeconds { min: u32, max: u32 },
    /// A text of `min` to `max` characters.
    Text { min: usize, max: usize },
    /// A text of at most `max` bytes.
    Bytes { max: usize },
    /// One of the listed texts.
    Choice(&'static [&'static str]),
    /// 2 to 64 pairs of an input and an output.
    Pairs,
    /// At most 32 arguments of at most 1024 characters.
    Args,
    /// An absolute path.
    AbsolutePath,
    /// An e-mail address with one `@`.
    Email,
    /// An MQTT topic below the base topic.
    Topic,
    /// The unit of a Graph sensor.
    Unit,
}

/// The pairs of a table node, at least and at most.
pub const MIN_PAIRS: usize = 2;
pub const MAX_PAIRS: usize = 64;
/// The most arguments of a command, and the longest one.
pub const MAX_ARGS: usize = 32;
pub const MAX_ARG_CHARS: usize = 1024;

impl ParamType {
    /// Whether `value` has this type and is in the range.
    pub fn accepts(self, value: &Value) -> bool {
        match self {
            Self::Number { min, max } => value
                .as_f64()
                .is_some_and(|v| v.is_finite() && v >= min && v <= max),
            Self::Seconds { min, max } | Self::Whole { min, max } => whole_in(value, min, max),
            Self::OptionalSeconds { min, max } => value.is_null() || whole_in(value, min, max),
            Self::Text { min, max } => value
                .as_str()
                .is_some_and(|t| (min..=max).contains(&t.chars().count())),
            Self::Bytes { max } => value.as_str().is_some_and(|t| t.len() <= max),
            Self::Choice(options) => value.as_str().is_some_and(|t| options.contains(&t)),
            Self::Pairs => {
                parse_pairs(value).is_some_and(|p| (MIN_PAIRS..=MAX_PAIRS).contains(&p.len()))
            }
            Self::Args => value.as_array().is_some_and(|args| {
                args.len() <= MAX_ARGS
                    && args.iter().all(|a| {
                        a.as_str()
                            .is_some_and(|t| t.chars().count() <= MAX_ARG_CHARS)
                    })
            }),
            Self::AbsolutePath => value
                .as_str()
                .is_some_and(|t| t.starts_with('/') && t.len() <= 4096 && !t.contains('\0')),
            Self::Email => value.as_str().is_some_and(is_email),
            Self::Topic => value.as_str().is_some_and(is_topic),
            Self::Unit => value
                .as_str()
                .is_some_and(|u| graph_sensor_unit(u).is_some()),
        }
    }

    /// A type whose value is a count of seconds.
    pub const fn is_time(self) -> bool {
        matches!(self, Self::Seconds { .. } | Self::OptionalSeconds { .. })
    }
}

fn whole_in(value: &Value, min: u32, max: u32) -> bool {
    value.as_f64().is_some_and(|v| {
        v.is_finite() && v.fract() == 0.0 && v >= f64::from(min) && v <= f64::from(max)
    })
}

/// The pairs of a `pairs` value, or `None` when the value is not a list of `[input, output]`
/// with finite numbers.
pub fn parse_pairs(value: &Value) -> Option<Vec<(f64, f64)>> {
    value
        .as_array()?
        .iter()
        .map(|pair| {
            let pair = pair.as_array()?;
            let [a, b] = pair.as_slice() else {
                return None;
            };
            let (a, b) = (a.as_f64()?, b.as_f64()?);
            (a.is_finite() && b.is_finite()).then_some((a, b))
        })
        .collect()
}

fn is_email(text: &str) -> bool {
    if text.chars().count() > 254 || text.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return false;
    }
    let mut parts = text.split('@');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(local), Some(domain), None) if !local.is_empty() && !domain.is_empty()
    )
}

fn is_topic(text: &str) -> bool {
    let chars = text.chars().count();
    (1..=128).contains(&chars)
        && !text.chars().any(|c| matches!(c, '+' | '#' | '\0'))
        && !text.starts_with('/')
        && !text.ends_with('/')
        && !text.contains("//")
}

/// The default of a parameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ParamDefault {
    Number(f64),
    Text(&'static str),
    Null,
    EmptyList,
    DefaultPairs,
}

/// The pairs of a new table node.
pub const DEFAULT_PAIRS: [(f64, f64); 2] = [(-100.0, -100.0), (100.0, 100.0)];

fn number_value(v: f64) -> Value {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        Value::from(v as i64)
    } else {
        Value::from(v)
    }
}

impl ParamDefault {
    pub fn to_value(self) -> Value {
        match self {
            Self::Number(v) => number_value(v),
            Self::Text(t) => Value::from(t),
            Self::Null => Value::Null,
            Self::EmptyList => Value::Array(Vec::new()),
            Self::DefaultPairs => Value::Array(
                DEFAULT_PAIRS
                    .iter()
                    .map(|(a, b)| Value::Array(vec![number_value(*a), number_value(*b)]))
                    .collect(),
            ),
        }
    }
}

/// One parameter of a kind.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ParamSpec {
    pub name: &'static str,
    pub ty: ParamType,
    pub default: ParamDefault,
}

const ANY_NUMBER: ParamType = ParamType::Number {
    min: f64::NEG_INFINITY,
    max: f64::INFINITY,
};

const fn spec(name: &'static str, ty: ParamType, default: ParamDefault) -> ParamSpec {
    ParamSpec { name, ty, default }
}

const fn secs(name: &'static str, min: u32, max: u32, default: f64) -> ParamSpec {
    spec(
        name,
        ParamType::Seconds { min, max },
        ParamDefault::Number(default),
    )
}

const fn text(name: &'static str, min: usize, max: usize) -> ParamSpec {
    spec(name, ParamType::Text { min, max }, ParamDefault::Text(""))
}

const MODES: &[&str] = &["nearest", "down", "up"];
const TABLE_MODES: &[&str] = &["linear", "step"];
const URGENCIES: &[&str] = &["low", "normal", "critical"];
const RESPONSE_KINDS: &[&str] = &[
    "notification",
    "sound",
    "command",
    "email",
    "mqtt",
    "poweroff",
    "reboot",
    "suspend",
];

const SENSOR_SOURCE: [ParamSpec; 1] = [text("sensor", 0, 128)];
const ALARM_SOURCE: [ParamSpec; 2] = [text("event", 0, 128), text("source", 0, 128)];
const CONSTANT: [ParamSpec; 1] = [spec("value", ANY_NUMBER, ParamDefault::Number(0.0))];
const RANDOM: [ParamSpec; 2] = [
    spec("min", ANY_NUMBER, ParamDefault::Number(-100.0)),
    spec("max", ANY_NUMBER, ParamDefault::Number(100.0)),
];
const PROCESS: [ParamSpec; 1] = [text("name", 1, 255)];
const ROUND: [ParamSpec; 2] = [
    spec(
        "decimals",
        ParamType::Whole { min: 0, max: 8 },
        ParamDefault::Number(0.0),
    ),
    spec(
        "mode",
        ParamType::Choice(MODES),
        ParamDefault::Text("nearest"),
    ),
];
const TABLE: [ParamSpec; 2] = [
    spec(
        "mode",
        ParamType::Choice(TABLE_MODES),
        ParamDefault::Text("linear"),
    ),
    spec("pairs", ParamType::Pairs, ParamDefault::DefaultPairs),
];
const DELAY: [ParamSpec; 1] = [secs("delay_s", 0, 3600, 1.0)];
const SPAN: [ParamSpec; 1] = [secs("span_s", 1, 1800, 10.0)];
const LOW_PASS: [ParamSpec; 2] = [
    secs("span_s", 1, 1800, 10.0),
    spec(
        "damping",
        ParamType::Number {
            min: 0.0,
            max: 99.99,
        },
        ParamDefault::Number(25.0),
    ),
];
const COMMENT: [ParamSpec; 3] = [
    text("text", 0, 1024),
    spec(
        "width",
        ParamType::Number {
            min: 60.0,
            max: 800.0,
        },
        ParamDefault::Number(180.0),
    ),
    spec(
        "height",
        ParamType::Number {
            min: 20.0,
            max: 600.0,
        },
        ParamDefault::Number(60.0),
    ),
];
const SENSOR_END: [ParamSpec; 1] = [spec("unit", ParamType::Unit, ParamDefault::Text("degC"))];
const NO_PARAMS: [ParamSpec; 0] = [];

/// The hold time of a response end node, in seconds.
pub const MAX_HOLD_S: u32 = 3600;

/// The parameters of a response end node of one kind, the common ones first.
fn response_specs(kind: ResponseKind) -> Vec<ParamSpec> {
    let mut specs = vec![
        spec(
            "kind",
            ParamType::Choice(RESPONSE_KINDS),
            ParamDefault::Text(kind.as_str()),
        ),
        secs("hold_s", 0, MAX_HOLD_S, 0.0),
        spec(
            "repeat_s",
            ParamType::OptionalSeconds {
                min: 1,
                max: 86_400,
            },
            ParamDefault::Null,
        ),
        secs(
            "min_interval_s",
            0,
            86_400,
            f64::from(kind.default_min_interval_s()),
        ),
    ];
    match kind {
        ResponseKind::Notification => specs.extend([
            text("title", 1, 64),
            text("text", 0, 1024),
            spec(
                "urgency",
                ParamType::Choice(URGENCIES),
                ParamDefault::Text("normal"),
            ),
        ]),
        ResponseKind::Sound => specs.push(text("sound", 1, 64)),
        ResponseKind::Command => specs.extend([
            spec("path", ParamType::AbsolutePath, ParamDefault::Text("")),
            spec("args", ParamType::Args, ParamDefault::EmptyList),
            secs("time_limit_s", 1, 3600, 30.0),
        ]),
        ResponseKind::Email => specs.extend([
            spec("to", ParamType::Email, ParamDefault::Text("")),
            text("subject", 0, 128),
            text("text", 0, 4096),
        ]),
        ResponseKind::Mqtt => specs.extend([
            spec("topic", ParamType::Topic, ParamDefault::Text("")),
            spec(
                "payload",
                ParamType::Bytes { max: 4096 },
                ParamDefault::Text(""),
            ),
        ]),
        ResponseKind::Poweroff | ResponseKind::Reboot | ResponseKind::Suspend => {
            specs.push(secs("countdown_s", 10, 3600, 30.0));
        }
    }
    specs
}

/// The parameters of a kind. For a response end node `response` is its response kind.
pub fn param_specs(kind: Kind, response: ResponseKind) -> Vec<ParamSpec> {
    use Kind::*;
    let fixed: &[ParamSpec] = match kind {
        SensorSource => &SENSOR_SOURCE,
        AlarmEventSource => &ALARM_SOURCE,
        Constant => &CONSTANT,
        Random | Limiter => &RANDOM,
        Process => &PROCESS,
        Round => &ROUND,
        Table => &TABLE,
        RiseDelay | FallDelay => &DELAY,
        Peak | WindowMinimum | WindowAverage | Trend => &SPAN,
        LowPass => &LOW_PASS,
        Comment => &COMMENT,
        SensorEnd => &SENSOR_END,
        ResponseEnd => return response_specs(response),
        _ => &NO_PARAMS,
    };
    fixed.to_vec()
}

/// The parameters of a new node of this kind, all with their defaults. A response end node
/// starts as a notification.
pub fn default_params(kind: Kind) -> Map<String, Value> {
    default_params_for(kind, ResponseKind::Notification)
}

/// As `default_params`, for a response end node of the given response kind.
pub fn default_params_for(kind: Kind, response: ResponseKind) -> Map<String, Value> {
    param_specs(kind, response)
        .into_iter()
        .map(|s| (s.name.to_owned(), s.default.to_value()))
        .collect()
}

/// One end of a connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub node: u32,
    pub pin: String,
}

/// A connection from an output pin (`from`) to an input pin (`to`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    pub from: Endpoint,
    pub to: Endpoint,
}

impl Connection {
    pub fn new(from: u32, to: u32, to_pin: &str) -> Self {
        Self {
            from: Endpoint {
                node: from,
                pin: OUTPUT_PIN.to_owned(),
            },
            to: Endpoint {
                node: to,
                pin: to_pin.to_owned(),
            },
        }
    }
}

/// One step of a Graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: u32,
    pub kind: Kind,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub params: Map<String, Value>,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
}

impl Node {
    /// A node with the default parameters, at the origin.
    pub fn new(id: u32, kind: Kind) -> Self {
        Self {
            id,
            kind,
            label: None,
            params: default_params(kind),
            x: 0.0,
            y: 0.0,
        }
    }

    pub fn with_param(mut self, name: &str, value: impl Into<Value>) -> Self {
        self.params.insert(name.to_owned(), value.into());
        self
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_owned());
        self
    }

    /// The response kind of a response end node. A missing or unknown value gives the default,
    /// a notification.
    pub fn response_kind(&self) -> ResponseKind {
        self.params
            .get("kind")
            .and_then(Value::as_str)
            .and_then(ResponseKind::parse)
            .unwrap_or(ResponseKind::Notification)
    }

    /// The parameters of this node, with the response kind of the node.
    pub fn param_specs(&self) -> Vec<ParamSpec> {
        param_specs(self.kind, self.response_kind())
    }

    /// The stored value of a parameter, or its default when the node has none.
    pub fn param(&self, name: &str) -> Option<Value> {
        if let Some(v) = self.params.get(name) {
            return Some(v.clone());
        }
        self.param_specs()
            .into_iter()
            .find(|s| s.name == name)
            .map(|s| s.default.to_value())
    }

    /// A number parameter. A value that is not a number gives the default, then 0.
    pub fn number(&self, name: &str) -> f64 {
        let stored = self.params.get(name).and_then(Value::as_f64);
        stored
            .or_else(|| self.param(name).and_then(|v| v.as_f64()))
            .unwrap_or(0.0)
    }

    /// A text parameter. A value that is not a text gives the default, then "".
    pub fn text(&self, name: &str) -> String {
        match self.params.get(name).and_then(Value::as_str) {
            Some(t) => t.to_owned(),
            None => self
                .param(name)
                .and_then(|v| v.as_str().map(str::to_owned))
                .unwrap_or_default(),
        }
    }

    /// The pairs of a table node, as stored. Not sorted.
    pub fn pairs(&self) -> Vec<(f64, f64)> {
        self.param("pairs")
            .as_ref()
            .and_then(parse_pairs)
            .unwrap_or_else(|| DEFAULT_PAIRS.to_vec())
    }
}

/// A named set of nodes and connections.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// `graph_<n>`, the Graph sensor of a Graph with a sensor end node.
    #[serde(default)]
    pub sensor_id: Option<String>,
    pub nodes: Vec<Node>,
    pub connections: Vec<Connection>,
}

impl Graph {
    pub fn new(id: &str, name: &str) -> Self {
        Self {
            id: id.to_owned(),
            name: name.to_owned(),
            enabled: true,
            sensor_id: None,
            nodes: Vec::new(),
            connections: Vec::new(),
        }
    }

    pub fn node(&self, id: u32) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    /// The connection that ends on an input pin.
    pub fn connection_to(&self, node: u32, pin: &str) -> Option<&Connection> {
        self.connections
            .iter()
            .find(|c| c.to.node == node && c.to.pin == pin)
    }

    pub fn end_nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.iter().filter(|n| n.kind.is_end())
    }

    /// The sensor end node, when the Graph has one.
    pub fn sensor_end(&self) -> Option<&Node> {
        self.nodes.iter().find(|n| n.kind == Kind::SensorEnd)
    }

    pub fn response_end(&self) -> Option<&Node> {
        self.nodes.iter().find(|n| n.kind == Kind::ResponseEnd)
    }

    /// The ids of the sensors that the source nodes read.
    pub fn source_sensors(&self) -> impl Iterator<Item = String> + '_ {
        self.nodes
            .iter()
            .filter(|n| n.kind == Kind::SensorSource)
            .map(|n| n.text("sensor"))
    }
}

/// The number `n` of a sensor id `graph_<n>`. A leading zero, a sign and a zero are not allowed.
pub fn parse_sensor_id(id: &str) -> Option<u32> {
    let digits = id.strip_prefix("graph_")?;
    if digits.is_empty() || digits.starts_with('0') || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

pub fn format_sensor_id(number: u32) -> String {
    format!("graph_{number}")
}

/// A Graph of `graphs.json` that could not be parsed. Its stored text stays as it was.
#[derive(Clone, Debug)]
pub struct InvalidGraph {
    pub id: Option<String>,
    pub name: Option<String>,
    /// Why the Graph does not parse.
    pub problem: ParseProblem,
    /// The text of the Graph in the file.
    pub text: Box<RawValue>,
}

impl PartialEq for InvalidGraph {
    fn eq(&self, other: &Self) -> bool {
        self.text.get() == other.text.get()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseProblem {
    /// A node has a kind that this version does not know.
    UnknownKind(String),
    /// The Graph is not in the form of the spec.
    Malformed(String),
}

impl fmt::Display for ParseProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownKind(kind) => write!(f, "unknown node kind {kind:?}"),
            Self::Malformed(why) => write!(f, "malformed Graph: {why}"),
        }
    }
}

/// One Graph of `graphs.json`.
#[derive(Clone, Debug, PartialEq)]
pub enum StoredGraph {
    Valid(Graph),
    Invalid(InvalidGraph),
}

impl StoredGraph {
    pub fn id(&self) -> Option<&str> {
        match self {
            Self::Valid(g) => Some(&g.id),
            Self::Invalid(g) => g.id.as_deref(),
        }
    }
}

/// Why `graphs.json` cannot be used. The app leaves the file as it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileError {
    NotJson(String),
    /// The file is JSON and not an object with a `graphs` list.
    NotAGraphList,
    HigherSchemaVersion(u32),
}

impl fmt::Display for FileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotJson(why) => write!(f, "the file is not JSON: {why}"),
            Self::NotAGraphList => f.write_str("the file is not a list of Graphs"),
            Self::HigherSchemaVersion(v) => write!(
                f,
                "the file has the schema version {v}, and this version knows {SCHEMA_VERSION}"
            ),
        }
    }
}

impl std::error::Error for FileError {}

/// The content of `graphs.json`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphFile {
    pub graphs: Vec<StoredGraph>,
}

#[derive(Deserialize)]
struct RawFile {
    graphs: Vec<Box<RawValue>>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum OutGraph<'a> {
    Graph(&'a Graph),
    Raw(&'a RawValue),
}

#[derive(Serialize)]
struct OutFile<'a> {
    schema_version: u32,
    graphs: Vec<OutGraph<'a>>,
}

impl GraphFile {
    /// Parses the file one Graph at a time. A Graph that does not parse, or that has a node of an
    /// unknown kind, stays in the list as invalid with its text, and the others load.
    pub fn parse(text: &str) -> Result<Self, FileError> {
        let value: Value =
            serde_json::from_str(text).map_err(|e| FileError::NotJson(e.to_string()))?;
        let Some(object) = value.as_object() else {
            return Err(FileError::NotAGraphList);
        };
        let version = object
            .get("schema_version")
            .and_then(Value::as_u64)
            .ok_or(FileError::NotAGraphList)?;
        if version > u64::from(SCHEMA_VERSION) {
            return Err(FileError::HigherSchemaVersion(
                u32::try_from(version).unwrap_or(u32::MAX),
            ));
        }
        let raw: RawFile = serde_json::from_str(text).map_err(|_| FileError::NotAGraphList)?;
        Ok(Self {
            graphs: raw.graphs.into_iter().map(parse_graph).collect(),
        })
    }

    /// The text of the file. An invalid Graph keeps the text that it had.
    pub fn to_json(&self) -> String {
        let out = OutFile {
            schema_version: SCHEMA_VERSION,
            graphs: self
                .graphs
                .iter()
                .map(|g| match g {
                    StoredGraph::Valid(g) => OutGraph::Graph(g),
                    StoredGraph::Invalid(g) => OutGraph::Raw(&g.text),
                })
                .collect(),
        };
        serde_json::to_string_pretty(&out).unwrap_or_default()
    }
}

fn parse_graph(raw: Box<RawValue>) -> StoredGraph {
    let value: Value = serde_json::from_str(raw.get()).unwrap_or(Value::Null);
    let id = value.get("id").and_then(Value::as_str).map(str::to_owned);
    let name = value.get("name").and_then(Value::as_str).map(str::to_owned);
    let unknown_kind = value
        .get("nodes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|n| n.get("kind").and_then(Value::as_str))
        .find(|k| Kind::parse(k).is_none());
    let problem = if let Some(kind) = unknown_kind {
        ParseProblem::UnknownKind(kind.to_owned())
    } else {
        match serde_json::from_value::<Graph>(value) {
            Ok(graph) => return StoredGraph::Valid(graph),
            Err(e) => ParseProblem::Malformed(e.to_string()),
        }
    };
    StoredGraph::Invalid(InvalidGraph {
        id,
        name,
        problem,
        text: raw,
    })
}

/// Why an export file is refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportError {
    NotJson(String),
    /// The `format` is not `nicecooling-graph`.
    WrongFormat,
    HigherFormatVersion(u32),
    Malformed(String),
}

impl fmt::Display for ExportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotJson(why) => write!(f, "the file is not JSON: {why}"),
            Self::WrongFormat => f.write_str("the file is not a nicecooling Graph"),
            Self::HigherFormatVersion(v) => write!(
                f,
                "the file has the format version {v}, and this version knows {EXPORT_FORMAT_VERSION}"
            ),
            Self::Malformed(why) => write!(f, "the Graph in the file is malformed: {why}"),
        }
    }
}

impl std::error::Error for ExportError {}

/// The Graph of an export file.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExportGraph {
    pub name: String,
    pub nodes: Vec<Node>,
    pub connections: Vec<Connection>,
}

/// An export file: one Graph, with no `sensor_id`, no stored value, no run state and no password.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Export {
    pub format: String,
    pub format_version: u32,
    pub graph: ExportGraph,
}

impl Export {
    pub fn from_graph(graph: &Graph) -> Self {
        Self {
            format: EXPORT_FORMAT.to_owned(),
            format_version: EXPORT_FORMAT_VERSION,
            graph: ExportGraph {
                name: graph.name.clone(),
                nodes: graph.nodes.clone(),
                connections: graph.connections.clone(),
            },
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Parses an export file. A higher format version is refused before the Graph is read.
    pub fn parse(text: &str) -> Result<Self, ExportError> {
        let value: Value =
            serde_json::from_str(text).map_err(|e| ExportError::NotJson(e.to_string()))?;
        if value.get("format").and_then(Value::as_str) != Some(EXPORT_FORMAT) {
            return Err(ExportError::WrongFormat);
        }
        let version = value
            .get("format_version")
            .and_then(Value::as_u64)
            .ok_or(ExportError::WrongFormat)?;
        if version > u64::from(EXPORT_FORMAT_VERSION) {
            return Err(ExportError::HigherFormatVersion(
                u32::try_from(version).unwrap_or(u32::MAX),
            ));
        }
        if let Some(kind) = value
            .pointer("/graph/nodes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|n| n.get("kind").and_then(Value::as_str))
            .find(|k| Kind::parse(k).is_none())
        {
            return Err(ExportError::Malformed(format!(
                "unknown node kind {kind:?}"
            )));
        }
        serde_json::from_value(value).map_err(|e| ExportError::Malformed(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 51 names of the spec, in its order.
    const SPEC_NAMES: [&str; 51] = [
        "sensor_source",
        "alarm_event_source",
        "constant",
        "random",
        "process",
        "add",
        "subtract",
        "multiply",
        "divide",
        "minimum",
        "maximum",
        "average",
        "logarithm",
        "power",
        "sine",
        "cosine",
        "tangent",
        "square_root",
        "round",
        "limiter",
        "table",
        "less",
        "less_equal",
        "equal",
        "greater_equal",
        "greater",
        "not_equal",
        "and",
        "or",
        "not",
        "switch",
        "has_value",
        "memory",
        "persistent_storage",
        "trigger",
        "on_delay",
        "off_delay",
        "counter",
        "peak",
        "window_minimum",
        "window_average",
        "low_pass",
        "trend",
        "generator_linear",
        "generator_sawtooth",
        "generator_triangle",
        "generator_sine",
        "generator_rectangle",
        "comment",
        "sensor_end",
        "response_end",
    ];

    fn one_of_each() -> Graph {
        let mut graph = Graph::new("graph_1", "All kinds");
        graph.sensor_id = Some("graph_1".into());
        for (i, kind) in Kind::ALL.iter().enumerate() {
            let mut node = Node::new(u32::try_from(i).unwrap() + 1, *kind);
            node.x = i as f64 * 10.5;
            node.y = -(i as f64);
            if i % 2 == 0 {
                node.label = Some(format!("n{i}"));
            }
            graph.nodes.push(node);
        }
        graph.connections.push(Connection::new(3, 6, "a"));
        graph
    }

    #[test]
    fn kind_count_is_51() {
        assert_eq!(Kind::ALL.len(), 51);
        let mut seen = std::collections::BTreeSet::new();
        assert!(Kind::ALL.iter().all(|k| seen.insert(*k)));
    }

    #[test]
    fn kind_json_names_match_spec() {
        let names: Vec<&str> = Kind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(names, SPEC_NAMES);
        for kind in Kind::ALL {
            assert_eq!(Kind::parse(kind.as_str()), Some(*kind));
            assert_eq!(serde_json::to_string(kind).unwrap(), format!("\"{kind}\""));
        }
        assert_eq!(Kind::parse("quantum"), None);
        assert!(serde_json::from_str::<Kind>("\"Add\"").is_err());
        // 2 end nodes and the comment node have no output.
        let no_output: Vec<_> = Kind::ALL.iter().filter(|k| !k.has_output()).collect();
        assert_eq!(no_output.len(), 3);
    }

    #[test]
    fn end_nodes_have_no_output_pin() {
        assert!(!Kind::SensorEnd.has_output());
        assert!(!Kind::ResponseEnd.has_output());
        assert!(!Kind::Comment.has_output());
        assert_eq!(Kind::ALL.iter().filter(|k| k.is_end()).count(), 2);
        for kind in Kind::ALL
            .iter()
            .filter(|k| !k.is_end() && **k != Kind::Comment)
        {
            assert!(kind.has_output(), "{kind}");
        }
        assert!(Kind::Comment.inputs().is_empty());
        assert_eq!(Kind::SensorEnd.inputs().len(), 1);
        assert_eq!(Kind::ResponseEnd.inputs().len(), 1);
    }

    fn defaults(kind: Kind) -> Vec<(&'static str, Option<f64>)> {
        kind.inputs().iter().map(|p| (p.name, p.default)).collect()
    }

    #[test]
    fn pin_default_table() {
        let zero_ab = vec![("a", Some(0.0)), ("b", Some(0.0))];
        for kind in [
            Kind::Add,
            Kind::Subtract,
            Kind::Multiply,
            Kind::Divide,
            Kind::Minimum,
            Kind::Maximum,
            Kind::Average,
            Kind::Less,
            Kind::LessThanOrEqual,
            Kind::Equal,
            Kind::GreaterThanOrEqual,
            Kind::Greater,
            Kind::Different,
            Kind::And,
            Kind::Or,
        ] {
            assert_eq!(defaults(kind), zero_ab, "{kind}");
        }
        assert_eq!(
            defaults(Kind::Logarithm),
            vec![("a", Some(0.0)), ("b", Some(std::f64::consts::E))]
        );
        assert_eq!(
            defaults(Kind::Power),
            vec![("a", Some(0.0)), ("b", Some(2.0))]
        );
        assert_eq!(
            defaults(Kind::Switch),
            vec![("s", Some(0.0)), ("a", Some(0.0)), ("b", Some(0.0))]
        );
        assert_eq!(
            defaults(Kind::Memory),
            vec![("s", Some(0.0)), ("a", Some(0.0))]
        );
        assert_eq!(defaults(Kind::PersistentStorage), defaults(Kind::Memory));
        assert_eq!(
            defaults(Kind::Counter),
            vec![
                ("count", Some(0.0)),
                ("enable", Some(1.0)),
                ("reset", Some(0.0)),
                ("period", Some(10.0)),
            ]
        );
        for kind in [
            Kind::LinearGen,
            Kind::SawtoothGen,
            Kind::TriangleGen,
            Kind::SineGen,
            Kind::RectangleGen,
        ] {
            assert_eq!(
                defaults(kind),
                vec![
                    ("enable", Some(1.0)),
                    ("reset", Some(0.0)),
                    ("period", Some(10.0)),
                ],
                "{kind}"
            );
        }
        for kind in [
            Kind::Sine,
            Kind::Cosine,
            Kind::Tangent,
            Kind::SquareRoot,
            Kind::Round,
            Kind::Limiter,
            Kind::Table,
            Kind::Not,
            Kind::Trigger,
            Kind::RiseDelay,
            Kind::FallDelay,
            Kind::Peak,
            Kind::WindowMinimum,
            Kind::WindowAverage,
            Kind::LowPass,
            Kind::Trend,
        ] {
            assert_eq!(defaults(kind), vec![("a", Some(0.0))], "{kind}");
        }
        for kind in [
            Kind::SensorSource,
            Kind::AlarmEventSource,
            Kind::Constant,
            Kind::Random,
            Kind::Process,
            Kind::Comment,
        ] {
            assert!(kind.inputs().is_empty(), "{kind}");
        }
        // With no connection the node "has value" sees no value.
        assert_eq!(defaults(Kind::HasValue), vec![("a", None)]);
    }

    #[test]
    fn end_node_inputs_have_no_default() {
        for kind in [Kind::SensorEnd, Kind::ResponseEnd] {
            assert_eq!(defaults(kind), vec![("a", None)]);
        }
    }

    fn defaults_of(kind: Kind) -> Map<String, Value> {
        default_params(kind)
    }

    #[test]
    fn parameter_defaults_per_kind() {
        use serde_json::json;
        assert_eq!(defaults_of(Kind::Constant)["value"], json!(0));
        assert_eq!(defaults_of(Kind::Random)["min"], json!(-100));
        assert_eq!(defaults_of(Kind::Random)["max"], json!(100));
        assert_eq!(defaults_of(Kind::Limiter)["min"], json!(-100));
        assert_eq!(defaults_of(Kind::Limiter)["max"], json!(100));
        assert_eq!(defaults_of(Kind::Round)["decimals"], json!(0));
        assert_eq!(defaults_of(Kind::Round)["mode"], json!("nearest"));
        assert_eq!(defaults_of(Kind::Table)["mode"], json!("linear"));
        assert_eq!(
            defaults_of(Kind::Table)["pairs"],
            json!([[-100, -100], [100, 100]])
        );
        assert_eq!(defaults_of(Kind::RiseDelay)["delay_s"], json!(1));
        assert_eq!(defaults_of(Kind::FallDelay)["delay_s"], json!(1));
        for kind in [
            Kind::Peak,
            Kind::WindowMinimum,
            Kind::WindowAverage,
            Kind::Trend,
        ] {
            assert_eq!(defaults_of(kind)["span_s"], json!(10), "{kind}");
        }
        assert_eq!(defaults_of(Kind::LowPass)["span_s"], json!(10));
        assert_eq!(defaults_of(Kind::LowPass)["damping"], json!(25));
        assert_eq!(defaults_of(Kind::Comment)["width"], json!(180));
        assert_eq!(defaults_of(Kind::Comment)["height"], json!(60));
        assert_eq!(defaults_of(Kind::SensorEnd)["unit"], json!("degC"));
        assert!(defaults_of(Kind::Add).is_empty());
        // A notification: hold 0 s, repeat off, minimum interval 0 s.
        let response = defaults_of(Kind::ResponseEnd);
        assert_eq!(response["kind"], json!("notification"));
        assert_eq!(response["hold_s"], json!(0));
        assert_eq!(response["repeat_s"], Value::Null);
        assert_eq!(response["min_interval_s"], json!(0));
        assert_eq!(response["urgency"], json!("normal"));
        // The three power kinds default to 600 s and a countdown of 30 s.
        for kind in [
            ResponseKind::Poweroff,
            ResponseKind::Reboot,
            ResponseKind::Suspend,
        ] {
            let p = default_params_for(Kind::ResponseEnd, kind);
            assert_eq!(p["min_interval_s"], json!(600), "{kind:?}");
            assert_eq!(p["countdown_s"], json!(30));
        }
        let command = default_params_for(Kind::ResponseEnd, ResponseKind::Command);
        assert_eq!(command["time_limit_s"], json!(30));
        assert_eq!(command["args"], json!([]));
        // A node reads a missing parameter as its default.
        let mut node = Node::new(1, Kind::LowPass);
        node.params.clear();
        assert_eq!(node.number("damping"), 25.0);
        assert_eq!(node.number("span_s"), 10.0);
        let mut table = Node::new(2, Kind::Table);
        table.params.clear();
        assert_eq!(table.pairs(), DEFAULT_PAIRS.to_vec());
        assert_eq!(table.text("mode"), "linear");
    }

    #[test]
    fn parameter_ranges_refused() {
        use serde_json::json;
        let spec = |kind: Kind, name: &str| {
            param_specs(kind, ResponseKind::Notification)
                .into_iter()
                .find(|s| s.name == name)
                .unwrap()
        };
        let ok = |kind, name, value: Value| spec(kind, name).ty.accepts(&value);
        assert!(ok(Kind::Round, "decimals", json!(8)));
        assert!(!ok(Kind::Round, "decimals", json!(9)));
        assert!(!ok(Kind::Round, "decimals", json!(-1)));
        assert!(!ok(Kind::Round, "mode", json!("truncate")));
        assert!(ok(Kind::LowPass, "damping", json!(99.99)));
        assert!(!ok(Kind::LowPass, "damping", json!(100)));
        assert!(ok(Kind::Peak, "span_s", json!(1800)));
        assert!(!ok(Kind::Peak, "span_s", json!(1801)));
        assert!(!ok(Kind::Peak, "span_s", json!(0)));
        assert!(ok(Kind::RiseDelay, "delay_s", json!(0)));
        assert!(!ok(Kind::RiseDelay, "delay_s", json!(3601)));
        assert!(!ok(Kind::Constant, "value", json!("5")));
        assert!(!ok(Kind::Comment, "width", json!(900)));
        assert!(ok(Kind::Comment, "width", json!(60)));
        assert!(!ok(Kind::Comment, "height", json!(19)));
        assert!(!ok(Kind::SensorEnd, "unit", json!("bar")));
        assert!(ok(Kind::SensorEnd, "unit", json!("mbar")));
        assert!(!ok(Kind::Process, "name", json!("")));
        assert!(ok(Kind::Table, "pairs", json!([[0, 0], [1, 1]])));
        assert!(!ok(Kind::Table, "pairs", json!([[0, 0]])));
        assert!(!ok(Kind::Table, "pairs", json!([[0, 0, 1], [1, 1]])));
        let sixty_five: Vec<Value> = (0..65).map(|i| json!([i, i])).collect();
        assert!(!ok(Kind::Table, "pairs", Value::Array(sixty_five.clone())));
        assert!(ok(
            Kind::Table,
            "pairs",
            Value::Array(sixty_five[..64].to_vec())
        ));
        let response = |name: &str, kind: ResponseKind, value: Value| {
            param_specs(Kind::ResponseEnd, kind)
                .into_iter()
                .find(|s| s.name == name)
                .unwrap()
                .ty
                .accepts(&value)
        };
        use ResponseKind::*;
        assert!(!response("hold_s", Notification, json!(2.5)));
        assert!(response("hold_s", Notification, json!(2.0)));
        assert!(!response("hold_s", Notification, json!(3601)));
        assert!(response("repeat_s", Notification, Value::Null));
        assert!(response("repeat_s", Notification, json!(86_400)));
        assert!(!response("repeat_s", Notification, json!(0)));
        assert!(!response("min_interval_s", Poweroff, json!(86_401)));
        assert!(!response("countdown_s", Poweroff, json!(9)));
        assert!(response("countdown_s", Poweroff, json!(10)));
        assert!(!response("countdown_s", Reboot, json!(3601)));
        assert!(!response("title", Notification, json!("")));
        assert!(!response("title", Notification, json!("x".repeat(65))));
        assert!(!response("urgency", Notification, json!("loud")));
        assert!(!response("sound", Sound, json!("")));
        assert!(!response("path", Command, json!("notify-send")));
        assert!(response("path", Command, json!("/usr/bin/notify-send")));
        assert!(!response("time_limit_s", Command, json!(0)));
        assert!(!response("args", Command, json!(vec!["a"; 33])));
        assert!(response("args", Command, json!(vec!["a"; 32])));
        assert!(!response("to", Email, json!("owner.example.org")));
        assert!(!response("to", Email, json!("a@b@c")));
        assert!(!response("to", Email, json!("a@b.org\r\nBcc: x@y.org")));
        assert!(response("to", Email, json!("owner@example.org")));
        assert!(!response("text", Email, json!("x".repeat(4097))));
        for topic in ["alerts/#", "a/+/b", "/a", "a/", "a//b", ""] {
            assert!(!response("topic", Mqtt, json!(topic)), "{topic:?}");
        }
        assert!(response("topic", Mqtt, json!("alerts/water")));
        assert!(!response("payload", Mqtt, json!("é".repeat(2049))));
    }

    #[test]
    fn time_parameters_are_whole_seconds() {
        let mut checked = 0;
        for kind in Kind::ALL {
            for response in ResponseKind::ALL {
                for spec in param_specs(*kind, response) {
                    if spec.name.ends_with("_s") {
                        assert!(spec.ty.is_time(), "{kind} {}", spec.name);
                        assert!(!spec.ty.accepts(&serde_json::json!(1.5)), "{}", spec.name);
                        checked += 1;
                    } else {
                        assert!(!spec.ty.is_time(), "{kind} {}", spec.name);
                    }
                }
            }
        }
        assert!(checked >= 7, "{checked}");
    }

    #[test]
    fn graphs_json_roundtrip_all_kinds() {
        let graph = one_of_each();
        assert_eq!(graph.nodes.len(), 51);
        let file = GraphFile {
            graphs: vec![StoredGraph::Valid(graph.clone())],
        };
        let text = file.to_json();
        assert!(text.contains("\"schema_version\": 1"));
        let back = GraphFile::parse(&text).unwrap();
        assert_eq!(back, file);
        // The documented example.
        let mut five = Graph::new("graph_1", "Five");
        five.sensor_id = Some("graph_1".into());
        five.nodes = vec![
            Node::new(1, Kind::Constant).with_param("value", 5),
            Node::new(2, Kind::SensorEnd),
        ];
        five.connections = vec![Connection::new(1, 2, "a")];
        let value = serde_json::to_value(&five).unwrap();
        assert_eq!(value["id"], "graph_1");
        assert_eq!(value["enabled"], true);
        assert_eq!(value["sensor_id"], "graph_1");
        assert_eq!(
            value["connections"],
            serde_json::json!([{"from": {"node": 1, "pin": "out"}, "to": {"node": 2, "pin": "a"}}])
        );
        let node = &value["nodes"][0];
        for key in ["id", "kind", "label", "params", "x", "y"] {
            assert!(node.get(key).is_some(), "{key}");
        }
    }

    #[test]
    fn export_json_roundtrip() {
        let mut graph = one_of_each();
        graph.nodes.truncate(10);
        graph.connections.clear();
        let export = Export::from_graph(&graph);
        assert_eq!(export.format, "nicecooling-graph");
        assert_eq!(export.format_version, 1);
        let text = export.to_json();
        assert!(!text.contains("sensor_id"));
        assert!(!text.contains("enabled"));
        let back = Export::parse(&text).unwrap();
        assert_eq!(back, export);
        assert_eq!(back.graph.nodes.len(), 10);
        assert_eq!(back.graph.name, "All kinds");
    }

    #[test]
    fn higher_format_version_refused() {
        let mut export = serde_json::to_value(Export::from_graph(&Graph::new("g", "G"))).unwrap();
        export["format_version"] = 2.into();
        assert_eq!(
            Export::parse(&export.to_string()),
            Err(ExportError::HigherFormatVersion(2))
        );
        export["format_version"] = 1.into();
        export["format"] = "other".into();
        assert_eq!(
            Export::parse(&export.to_string()),
            Err(ExportError::WrongFormat)
        );
        assert!(matches!(Export::parse("{"), Err(ExportError::NotJson(_))));
        // The Graph file has its own version.
        assert_eq!(
            GraphFile::parse(r#"{"schema_version":2,"graphs":[]}"#),
            Err(FileError::HigherSchemaVersion(2))
        );
        assert_eq!(GraphFile::parse("[]"), Err(FileError::NotAGraphList));
        assert_eq!(
            GraphFile::parse(r#"{"schema_version":1}"#),
            Err(FileError::NotAGraphList)
        );
        assert!(matches!(
            GraphFile::parse("not json"),
            Err(FileError::NotJson(_))
        ));
        assert!(GraphFile::parse(r#"{"schema_version":1,"graphs":[]}"#).is_ok());
    }

    #[test]
    fn sensor_id_link() {
        assert_eq!(parse_sensor_id("graph_3"), Some(3));
        assert_eq!(parse_sensor_id("graph_64"), Some(64));
        for bad in [
            "graph_0", "graph_03", "graph_", "graph_x", "graph_-1", "Graph_1", "nct_1", "",
        ] {
            assert_eq!(parse_sensor_id(bad), None, "{bad:?}");
        }
        assert_eq!(format_sensor_id(5), "graph_5");
        // The Graph keeps its link over a roundtrip, and none stays none.
        let mut graph = Graph::new("graph_9", "Linked");
        graph.sensor_id = Some("graph_3".into());
        graph.nodes.push(Node::new(1, Kind::SensorEnd));
        let back: Graph = serde_json::from_str(&serde_json::to_string(&graph).unwrap()).unwrap();
        assert_eq!(back.sensor_id.as_deref(), Some("graph_3"));
        assert_eq!(back.sensor_end().map(|n| n.id), Some(1));
        let none: Graph = serde_json::from_str(
            r#"{"id":"a","name":"b","enabled":false,"nodes":[],"connections":[]}"#,
        )
        .unwrap();
        assert_eq!(none.sensor_id, None);
    }

    fn good_graph(id: &str) -> String {
        format!(
            r#"{{"id":"{id}","name":"G {id}","enabled":true,"sensor_id":null,
            "nodes":[{{"id":1,"kind":"constant","label":null,"params":{{"value":5}},"x":0,"y":0}}],
            "connections":[]}}"#
        )
    }

    #[test]
    fn unknown_kind_graph_is_invalid_and_the_others_load() {
        let quantum = r#"{"id":"graph_2","name":"New","enabled":true,"sensor_id":null,
            "nodes":[{"id":1,"kind":"quantum","label":null,"params":{},"x":0,"y":0}],
            "connections":[]}"#;
        let text = format!(
            r#"{{"schema_version":1,"graphs":[{},{quantum},{}]}}"#,
            good_graph("graph_1"),
            good_graph("graph_3")
        );
        let file = GraphFile::parse(&text).unwrap();
        assert_eq!(file.graphs.len(), 3);
        assert!(matches!(file.graphs[0], StoredGraph::Valid(_)));
        assert!(matches!(file.graphs[2], StoredGraph::Valid(_)));
        let StoredGraph::Invalid(invalid) = &file.graphs[1] else {
            panic!("the Graph with the kind quantum is invalid");
        };
        assert_eq!(invalid.problem, ParseProblem::UnknownKind("quantum".into()));
        assert_eq!(invalid.id.as_deref(), Some("graph_2"));
        assert_eq!(invalid.name.as_deref(), Some("New"));
        // A Graph that is not an object, and one without nodes, are invalid too.
        let broken = r#"{"schema_version":1,"graphs":[42, {"id":"x"}]}"#;
        let file = GraphFile::parse(broken).unwrap();
        assert!(
            file.graphs
                .iter()
                .all(|g| matches!(g, StoredGraph::Invalid(_)))
        );
        assert_eq!(file.graphs[1].id(), Some("x"));
    }

    #[test]
    fn invalid_graph_keeps_its_stored_text() {
        let quantum = r#"{ "id" : "graph_2", "future":{"a":[1,2]},
            "nodes":[{"id":1,"kind":"quantum"}] }"#;
        let text = format!(
            r#"{{"schema_version":1,"graphs":[{},{quantum}]}}"#,
            good_graph("graph_1")
        );
        let file = GraphFile::parse(&text).unwrap();
        let written = file.to_json();
        // The text of the invalid Graph is in the written file byte for byte.
        assert!(written.contains(quantum), "{written}");
        let again = GraphFile::parse(&written).unwrap();
        assert_eq!(again, file);
        let StoredGraph::Invalid(invalid) = &again.graphs[1] else {
            panic!("still invalid");
        };
        assert_eq!(invalid.text.get(), quantum);
    }
}
