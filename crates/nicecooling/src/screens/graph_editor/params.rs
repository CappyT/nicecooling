//! The parameter panel of the editor: the parameters of the selected node, built from the
//! parameter tables of `nicecooling-graph`, and the section "Inputs" with one select per input
//! pin. The section "Inputs" is the keyboard path for a connection.
//!
//! A parameter edit commits on Enter or when the field loses the focus. One committed edit is
//! one undo step. A value outside its range does not commit: the panel shows the range under
//! the field.

use nicecooling_core::graph_sensor::GRAPH_SENSOR_UNITS;
use nicecooling_graph::model::{
    Connection, Kind, MAX_ARG_CHARS, MAX_ARGS, MAX_PAIRS, MIN_PAIRS, Node, ParamSpec, ParamType,
    ResponseKind,
};
use nicecooling_graph::validate::MAX_LABEL_CHARS;
use serde_json::Value;

use super::canvas::{Band, REASON_TIME_US, connection_refusal, detail_of, pin_value, value_text};
use super::palette::family_key;
use super::response_panel::{ResponsePanel, TIMING, response_panel};
use super::rules::{node_name, param_name, range_text};
use super::try_run::{TestRows, test_rows};
use super::{EditorUi, Selected, number_text};
use crate::graphs::view::GraphView;
use crate::i18n::Locale;
use crate::screens::ScreenInput;
use crate::screens::format::optional_value;
use crate::screens::sensors::{SensorUse, sensor_users};

/// The time that "Repeat every" gets when the user turns it on, in seconds.
pub const REPEAT_DEFAULT_S: u32 = 60;

/// One entry of a select: its value, its text, and whether it is the current one.
pub type Choice = (String, String, bool);

/// The key of the field of a parameter.
pub fn param_key(node: u32, name: &str) -> String {
    format!("param:{node}:{name}")
}

/// The key of the field "Label".
pub fn label_key(node: u32) -> String {
    format!("label:{node}")
}

/// The key of one number of a pair of a Table node: `column` is 0 for the input and 1 for the
/// output.
pub fn pair_key(node: u32, index: usize, column: usize) -> String {
    format!("pair:{node}:{index}:{column}")
}

/// The key of one argument of a command.
pub fn arg_key(node: u32, index: usize) -> String {
    format!("arg:{node}:{index}")
}

/// One sensor of the sensor picker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SensorChoice {
    pub id: String,
    pub name: String,
    /// The driver, or the origin of a sensor that has none: `derived`, `graph`.
    pub caption: String,
    /// The live value, or "--".
    pub value: String,
    pub selected: bool,
}

/// The control of one parameter.
#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    /// A text or number field. `unit` shows after the value.
    Field {
        text: String,
        unit: Option<String>,
    },
    Select {
        shown: String,
        choices: Vec<Choice>,
    },
    /// The sensor picker: every sensor, Derived sensors and Graph sensors included.
    Sensor {
        shown: String,
        choices: Vec<SensorChoice>,
    },
    /// The pairs of a Table node, as the texts of their two numbers.
    Pairs {
        pairs: Vec<(String, String)>,
        can_add: bool,
        can_remove: bool,
    },
    /// The arguments of a command, one field each.
    Args {
        args: Vec<String>,
        can_add: bool,
    },
    /// A switch and a field of seconds: off stores no value.
    Optional {
        on: bool,
        text: String,
    },
}

/// One row of the panel: the name of a parameter and its control.
#[derive(Clone, Debug, PartialEq)]
pub struct ParamRow {
    /// The name of the parameter in the model.
    pub name: &'static str,
    pub label: String,
    pub control: Control,
    /// The range of the parameter, while its field holds a value that did not commit.
    pub error: Option<String>,
}

/// One row of the section "Inputs": an input pin and the node whose output it takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputRow {
    pub pin: &'static str,
    pub label: String,
    /// The text of the current entry: "None, 0", or the title of a node.
    pub shown: String,
    /// "None", then each other node that has an output pin. The value of an entry is the id of
    /// its node, and it is empty for "None".
    pub choices: Vec<Choice>,
}

/// The field "Label" of a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelRow {
    pub label: String,
    pub text: String,
    pub caption: String,
    pub error: Option<String>,
}

/// The Graph sensor of a sensor end node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndSensor {
    /// "Graph sensor".
    pub label: String,
    /// The id of the sensor, or "Created at Apply".
    pub id: String,
    pub has_id: bool,
    /// "Used by".
    pub used_label: String,
    /// The Curves, Loops and Derived sensors that read the sensor, or "Not used".
    pub used: Vec<String>,
}

/// The guide of a kind that has one.
#[derive(Clone, Debug, PartialEq)]
pub enum Guide {
    /// The band of a Limiter, with the texts under its ends and above its marker.
    Band {
        band: Band,
        minimum: String,
        maximum: String,
        input: Option<String>,
    },
    /// The pairs of a Table as a line, with a dot at the input A.
    Table {
        pairs: Vec<(f64, f64)>,
        step: bool,
        input: Option<f64>,
    },
}

/// The segmented control of an end node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EndSwitch {
    pub sensor: String,
    pub response: String,
    /// The end node is a sensor end node.
    pub is_sensor: bool,
}

/// The parameter panel of one node.
#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub node: u32,
    pub kind: Kind,
    /// The name of the kind, or "End node".
    pub title: String,
    pub family: Option<String>,
    pub help: String,
    pub end: Option<EndSwitch>,
    /// `None` for an end node: it has no label.
    pub label: Option<LabelRow>,
    pub rows: Vec<ParamRow>,
    pub guide: Option<Guide>,
    pub sensor: Option<EndSensor>,
    /// "Inputs".
    pub inputs_title: String,
    pub inputs: Vec<InputRow>,
    /// "Test value" and "For", on a Sensor node only.
    pub tests: Option<TestRows>,
    /// The warning, "Run once" and the last runs, on a response end node only.
    pub response: Option<ResponsePanel>,
    /// "Delete node". `None` for an end node: a Graph keeps its end node.
    pub delete: Option<String>,
}

/// A number as the user types it: a comma or a point.
fn parse_number(text: &str) -> Option<f64> {
    text.trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
}

/// A number as the Graph file holds it: a whole number has no decimals.
fn number_json(value: f64) -> Value {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        Value::from(value as i64)
    } else {
        Value::from(value)
    }
}

fn pairs_json(pairs: &[(f64, f64)]) -> Value {
    Value::Array(
        pairs
            .iter()
            .map(|(input, output)| Value::Array(vec![number_json(*input), number_json(*output)]))
            .collect(),
    )
}

/// The value of a parameter for the text of its field. `None` for a text that is no value of
/// the type or that is outside the range.
pub fn typed_value(ty: ParamType, text: &str) -> Option<Value> {
    let value = match ty {
        ParamType::Number { .. } => number_json(parse_number(text)?),
        ParamType::Seconds { .. } | ParamType::Whole { .. } | ParamType::OptionalSeconds { .. } => {
            let number = parse_number(text)?;
            if number.fract() != 0.0 {
                return None;
            }
            number_json(number)
        }
        ParamType::Text { .. }
        | ParamType::Bytes { .. }
        | ParamType::Choice(_)
        | ParamType::Unit => Value::from(text),
        ParamType::AbsolutePath | ParamType::Email | ParamType::Topic => Value::from(text.trim()),
        ParamType::Pairs | ParamType::Args => return None,
    };
    ty.accepts(&value).then_some(value)
}

/// The text of a parameter in its field.
fn param_text(node: &Node, spec: &ParamSpec, locale: Locale) -> String {
    match spec.ty {
        ParamType::Number { .. }
        | ParamType::Seconds { .. }
        | ParamType::Whole { .. }
        | ParamType::OptionalSeconds { .. } => match node.param(spec.name) {
            Some(Value::Null) | None => String::new(),
            Some(_) => number_text(locale, node.number(spec.name)),
        },
        _ => node.text(spec.name),
    }
}

/// The sensors of the sensor picker: every sensor of the daemon, Derived sensors and Graph
/// sensors included, with their live value.
pub fn sensor_choices(input: &ScreenInput<'_>, selected: &str) -> Vec<SensorChoice> {
    let state = input.state;
    state
        .snapshot
        .sensors
        .values()
        .map(|sensor| SensorChoice {
            id: sensor.id.clone(),
            name: sensor.name.clone(),
            caption: if sensor.driver.is_empty() {
                sensor.origin.clone()
            } else {
                sensor.driver.clone()
            },
            value: optional_value(input.locale, state.sensor_value(&sensor.id), &sensor.unit),
            selected: sensor.id == selected,
        })
        .collect()
}

/// The entries of a select with the stored value first when the list does not hold it, so a
/// value that this machine does not know still shows.
fn with_stored(mut choices: Vec<Choice>, stored: &str) -> Vec<Choice> {
    if !stored.is_empty() && !choices.iter().any(|(value, _, _)| value == stored) {
        choices.insert(0, (stored.to_owned(), stored.to_owned(), true));
    }
    choices
}

fn shown_of(choices: &[Choice], fallback: String) -> String {
    choices
        .iter()
        .find(|(_, _, selected)| *selected)
        .map_or(fallback, |(_, text, _)| text.clone())
}

/// The control of one parameter of a node.
fn control_of(input: &ScreenInput<'_>, node: &Node, spec: &ParamSpec) -> Control {
    let locale = input.locale;
    let state = input.state;
    let choose = locale.tr("graph_editor.panel.choose");
    let stored = node.text(spec.name);
    let select = |choices: Vec<Choice>| Control::Select {
        shown: shown_of(&choices, choose.clone()),
        choices,
    };
    match (node.kind, spec.name, spec.ty) {
        (Kind::SensorSource, "sensor", _) => {
            let choices = sensor_choices(input, &stored);
            let shown = choices
                .iter()
                .find(|choice| choice.selected)
                .map(|choice| choice.name.clone())
                .unwrap_or_else(|| {
                    if stored.is_empty() {
                        choose.clone()
                    } else {
                        stored.clone()
                    }
                });
            Control::Sensor { shown, choices }
        }
        (Kind::AlarmEventSource, "event", _) => {
            let mut events: Vec<&str> = Vec::new();
            for rule in &state.snapshot.responses {
                if !events.contains(&rule.event.as_str()) {
                    events.push(&rule.event);
                }
            }
            let choices = events
                .into_iter()
                .map(|event| (event.to_owned(), locale.event_title(event), event == stored))
                .collect();
            select(with_stored(choices, &stored))
        }
        (Kind::AlarmEventSource, "source", _) => {
            let leakshield = locale.tr("nav.leakshield");
            let loops = state
                .snapshot
                .loops
                .iter()
                .map(|info| (info.id.clone(), info.name.clone()));
            let devices = state.snapshot.leakshields.iter().map(|device| {
                (
                    device.id.clone(),
                    state.source_name(&device.id, &leakshield),
                )
            });
            let choices = loops
                .chain(devices)
                .map(|(id, name)| {
                    let selected = id == stored;
                    (id, name, selected)
                })
                .collect();
            select(with_stored(choices, &stored))
        }
        (_, _, ParamType::Choice(options)) => select(
            options
                .iter()
                .map(|option| {
                    let key = if spec.name == "kind" {
                        format!("graphs.kind.{option}")
                    } else {
                        format!("graph_editor.choice.{option}")
                    };
                    ((*option).to_owned(), locale.tr(&key), *option == stored)
                })
                .collect(),
        ),
        (_, _, ParamType::Unit) => select(
            GRAPH_SENSOR_UNITS
                .iter()
                .map(|unit| {
                    (
                        unit.code.to_owned(),
                        unit.display.to_owned(),
                        unit.code == stored,
                    )
                })
                .collect(),
        ),
        (_, _, ParamType::Pairs) => {
            let pairs = node.pairs();
            Control::Pairs {
                can_add: pairs.len() < MAX_PAIRS,
                can_remove: pairs.len() > MIN_PAIRS,
                pairs: pairs
                    .iter()
                    .map(|(a, b)| (number_text(locale, *a), number_text(locale, *b)))
                    .collect(),
            }
        }
        (_, _, ParamType::Args) => {
            let args: Vec<String> = node
                .param(spec.name)
                .and_then(|value| value.as_array().cloned())
                .unwrap_or_default()
                .iter()
                .map(|arg| arg.as_str().unwrap_or_default().to_owned())
                .collect();
            Control::Args {
                can_add: args.len() < MAX_ARGS,
                args,
            }
        }
        (_, _, ParamType::OptionalSeconds { .. }) => {
            let text = param_text(node, spec, locale);
            Control::Optional {
                on: !text.is_empty(),
                text,
            }
        }
        (_, _, ty) => Control::Field {
            text: param_text(node, spec, locale),
            unit: ty
                .is_time()
                .then(|| locale.tr("graph_editor.panel.seconds")),
        },
    }
}

/// The title of a node in a select of the section "Inputs": the name of its kind, its main
/// parameter, and its label when it has one. "On delay 10 s", "Sensor cpu".
pub fn entry_title(node: &Node, locale: Locale) -> String {
    let mut title = node_name(node, locale);
    for part in [detail_of(node, locale), node.label.clone()]
        .into_iter()
        .flatten()
    {
        title.push(' ');
        title.push_str(&part);
    }
    title
}

/// The rows of the section "Inputs" of a node.
fn input_rows(ui: &EditorUi, node: &Node, locale: Locale) -> Vec<InputRow> {
    let graph = ui.draft.graph();
    node.kind
        .inputs()
        .iter()
        .map(|pin| {
            let from = graph
                .connection_to(node.id, pin.name)
                .map(|connection| connection.from.node);
            let none = match pin.default {
                // The default of the pin B of a Logarithm node is the number e.
                Some(default) if default == std::f64::consts::E => {
                    locale.fmt("graph_editor.panel.none_default", &[("value", "e")])
                }
                Some(default) => locale.fmt(
                    "graph_editor.panel.none_default",
                    &[("value", &number_text(locale, default))],
                ),
                None => locale.tr("graph_editor.panel.none"),
            };
            let mut choices = vec![(String::new(), none.clone(), from.is_none())];
            choices.extend(
                graph
                    .nodes
                    .iter()
                    .filter(|other| other.id != node.id && other.kind.has_output())
                    .map(|other| {
                        (
                            other.id.to_string(),
                            entry_title(other, locale),
                            from == Some(other.id),
                        )
                    }),
            );
            InputRow {
                pin: pin.name,
                label: locale.tr(&format!("graph_editor.pin.{}", pin.name)),
                shown: shown_of(&choices, none),
                choices,
            }
        })
        .collect()
}

/// The Graph sensor of the sensor end node, and what reads it.
fn end_sensor(input: &ScreenInput<'_>, ui: &EditorUi) -> EndSensor {
    let locale = input.locale;
    let id = ui.draft.candidate().sensor_id;
    let used: Vec<String> = id
        .as_deref()
        .map(|id| sensor_users(input.state, id))
        .unwrap_or_default()
        .into_iter()
        .map(|user| match user {
            SensorUse::Curve { name, .. } => {
                locale.fmt("graph_editor.panel.used.curve", &[("name", &name)])
            }
            SensorUse::Loop { name, .. } => {
                locale.fmt("graph_editor.panel.used.loop", &[("name", &name)])
            }
            SensorUse::Derived { name, .. } => {
                locale.fmt("graph_editor.panel.used.derived", &[("name", &name)])
            }
        })
        .collect();
    EndSensor {
        label: locale.tr("graph_editor.canvas.graph_sensor"),
        has_id: id.is_some(),
        id: id.unwrap_or_else(|| locale.tr("graph_editor.panel.created_at_apply")),
        used_label: locale.tr("graph_editor.panel.used_by"),
        used: if used.is_empty() {
            vec![locale.tr("graph_editor.panel.not_used")]
        } else {
            used
        },
    }
}

/// The node that the panel shows: the selected node, or the end node with no selection.
pub fn panel_node(ui: &EditorUi) -> Option<&Node> {
    let graph = ui.draft.graph();
    match &ui.selected {
        Some(Selected::Node(id)) => graph.node(*id),
        _ => None,
    }
    .or_else(|| graph.end_nodes().next())
}

/// The parameter panel. `None` for a draft that has no end node and no selection. `view` is what
/// the Graph service shows: the response log is in it.
pub fn panel(input: &ScreenInput<'_>, view: &GraphView, ui: &EditorUi) -> Option<Panel> {
    let locale = input.locale;
    let node = panel_node(ui)?;
    let error_of = |key: String| {
        ui.field_error
            .as_ref()
            .filter(|(field, _)| *field == key)
            .map(|(_, range)| range.clone())
    };
    let is_end = node.kind.is_end();
    let mut specs = node.param_specs();
    if node.kind == Kind::ResponseEnd {
        // The hold time, the repeat and the minimum interval come after the parameters of the
        // kind.
        specs.sort_by_key(|spec| TIMING.contains(&spec.name));
    }
    let rows: Vec<ParamRow> = specs
        .iter()
        .map(|spec| ParamRow {
            name: spec.name,
            label: param_name(spec.name, locale),
            control: control_of(input, node, spec),
            error: error_of(param_key(node.id, spec.name)),
        })
        .collect();
    let guide = match node.kind {
        Kind::Limiter => {
            let (min, max) = (node.number("min"), node.number("max"));
            let input = pin_value(ui, node.id, "a");
            Some(Guide::Band {
                band: Band { min, max, input },
                minimum: locale.fmt(
                    "graph_editor.panel.guide.minimum",
                    &[("value", &number_text(locale, min))],
                ),
                maximum: locale.fmt(
                    "graph_editor.panel.guide.maximum",
                    &[("value", &number_text(locale, max))],
                ),
                input: input.map(|value| {
                    locale.fmt(
                        "graph_editor.panel.guide.input",
                        &[("value", &value_text(locale, value))],
                    )
                }),
            })
        }
        Kind::Table => {
            let mut pairs = node.pairs();
            pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
            Some(Guide::Table {
                pairs,
                step: node.text("mode") == "step",
                input: pin_value(ui, node.id, "a"),
            })
        }
        _ => None,
    };
    Some(Panel {
        node: node.id,
        kind: node.kind,
        title: node_name(node, locale),
        family: (!is_end).then(|| {
            locale.tr(&format!(
                "graph_editor.family.{}",
                family_key(node.kind.family())
            ))
        }),
        help: locale.tr(&format!("graph_editor.kind.{}.help", node.kind.as_str())),
        end: is_end.then(|| EndSwitch {
            sensor: locale.tr("graphs.end.sensor"),
            response: locale.tr("graphs.end.response"),
            is_sensor: node.kind == Kind::SensorEnd,
        }),
        label: (!is_end).then(|| LabelRow {
            label: param_name("label", locale),
            text: node.label.clone().unwrap_or_default(),
            caption: locale.tr("graph_editor.panel.label_caption"),
            error: error_of(label_key(node.id)),
        }),
        rows,
        guide,
        sensor: (node.kind == Kind::SensorEnd).then(|| end_sensor(input, ui)),
        inputs_title: locale.tr("graph_editor.panel.inputs"),
        inputs: input_rows(ui, node, locale),
        tests: test_rows(ui, node.id, locale),
        response: response_panel(input, view, ui, node),
        delete: (!is_end).then(|| locale.tr("graph_editor.panel.delete_node")),
    })
}

/// Whether a text is a label: 1 to 32 characters of letters, digits and "_".
fn is_label(text: &str) -> bool {
    (1..=MAX_LABEL_CHARS).contains(&text.chars().count())
        && text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl EditorUi {
    /// The parameter of a node, with its type.
    fn spec_of(&self, node: u32, name: &str) -> Option<ParamSpec> {
        self.draft
            .graph()
            .node(node)?
            .param_specs()
            .into_iter()
            .find(|spec| spec.name == name)
    }

    /// The field `key` does not commit: it shows `range` under it.
    fn refuse_field(&mut self, key: String, range: String) -> bool {
        self.field_error = Some((key, range));
        false
    }

    /// Commit the text of the field of a parameter: Enter, or the field lost the focus. A value
    /// in the range is one undo step. A value outside the range does not commit: the panel
    /// shows the range, and the function returns false.
    pub fn commit_param(&mut self, node: u32, name: &str, text: &str, locale: Locale) -> bool {
        let Some(spec) = self.spec_of(node, name) else {
            return false;
        };
        match typed_value(spec.ty, text) {
            Some(value) => {
                self.field_error = None;
                self.draft.set_param(node, name, value);
                true
            }
            None => self.refuse_field(param_key(node, name), range_text(spec.ty, locale)),
        }
    }

    /// Commit the field "Label". An empty text removes the label.
    pub fn commit_label(&mut self, node: u32, text: &str, locale: Locale) -> bool {
        let text = text.trim();
        if !text.is_empty() && !is_label(text) {
            return self.refuse_field(label_key(node), locale.tr("graph_editor.range.label"));
        }
        self.field_error = None;
        self.draft.set_label(node, text);
        true
    }

    /// Choose an entry of a select of a parameter. The kind of a response gives the node the
    /// parameters of the new kind.
    pub fn choose_param(&mut self, node: u32, name: &str, value: &str) -> bool {
        let Some(spec) = self.spec_of(node, name) else {
            return false;
        };
        self.field_error = None;
        if name == "kind"
            && let Some(kind) = ResponseKind::parse(value)
        {
            return self.draft.set_response_kind(kind);
        }
        let value = Value::from(value);
        // A select of a text parameter holds the entries of this machine: any entry commits.
        if !matches!(spec.ty, ParamType::Text { .. }) && !spec.ty.accepts(&value) {
            return false;
        }
        self.draft.set_param(node, name, value)
    }

    /// The switch of a parameter that can be off: on gives it 60 s, and off stores no value.
    pub fn switch_param(&mut self, node: u32, name: &str, on: bool) -> bool {
        self.field_error = None;
        let value = if on {
            Value::from(REPEAT_DEFAULT_S)
        } else {
            Value::Null
        };
        self.draft.set_param(node, name, value)
    }

    /// Commit one number of a pair of a Table node.
    pub fn commit_pair(
        &mut self,
        node: u32,
        index: usize,
        column: usize,
        text: &str,
        locale: Locale,
    ) -> bool {
        let Some(mut pairs) = self.draft.graph().node(node).map(Node::pairs) else {
            return false;
        };
        let (Some(number), Some(pair)) = (parse_number(text), pairs.get_mut(index)) else {
            return self.refuse_field(
                pair_key(node, index, column),
                locale.tr("graph_editor.range.any_number"),
            );
        };
        if column == 0 {
            pair.0 = number;
        } else {
            pair.1 = number;
        }
        self.field_error = None;
        self.draft.set_param(node, "pairs", pairs_json(&pairs));
        true
    }

    /// Add a pair after the last one. A Table node takes 64 pairs at most.
    pub fn add_pair(&mut self, node: u32) -> bool {
        let Some(mut pairs) = self.draft.graph().node(node).map(Node::pairs) else {
            return false;
        };
        if pairs.len() >= MAX_PAIRS {
            return false;
        }
        // The new input is above every input, so the table has no input twice.
        let highest = pairs.iter().map(|pair| pair.0).fold(f64::MIN, f64::max);
        let output = pairs.last().map_or(0.0, |pair| pair.1);
        pairs.push((if pairs.is_empty() { 0.0 } else { highest + 1.0 }, output));
        self.draft.set_param(node, "pairs", pairs_json(&pairs))
    }

    /// Remove a pair. A Table node keeps 2 pairs at least.
    pub fn remove_pair(&mut self, node: u32, index: usize) -> bool {
        let Some(mut pairs) = self.draft.graph().node(node).map(Node::pairs) else {
            return false;
        };
        if pairs.len() <= MIN_PAIRS || index >= pairs.len() {
            return false;
        }
        pairs.remove(index);
        self.field_error = None;
        self.draft.set_param(node, "pairs", pairs_json(&pairs))
    }

    /// The arguments of the command of a node.
    fn args_of(&self, node: u32) -> Option<Vec<Value>> {
        let node = self.draft.graph().node(node)?;
        Some(
            node.param("args")
                .and_then(|value| value.as_array().cloned())
                .unwrap_or_default(),
        )
    }

    /// Commit one argument of a command: 1024 characters at most.
    pub fn commit_arg(&mut self, node: u32, index: usize, text: &str, locale: Locale) -> bool {
        let Some(mut args) = self.args_of(node) else {
            return false;
        };
        let Some(slot) = args.get_mut(index) else {
            return false;
        };
        if text.chars().count() > MAX_ARG_CHARS {
            return self.refuse_field(arg_key(node, index), locale.tr("graph_editor.range.args"));
        }
        *slot = Value::from(text);
        self.field_error = None;
        self.draft.set_param(node, "args", Value::Array(args));
        true
    }

    /// Add an empty argument. A command takes 32 arguments at most.
    pub fn add_arg(&mut self, node: u32) -> bool {
        let Some(mut args) = self.args_of(node) else {
            return false;
        };
        if args.len() >= MAX_ARGS {
            return false;
        }
        args.push(Value::from(""));
        self.draft.set_param(node, "args", Value::Array(args))
    }

    pub fn remove_arg(&mut self, node: u32, index: usize) -> bool {
        let Some(mut args) = self.args_of(node) else {
            return false;
        };
        if index >= args.len() {
            return false;
        }
        args.remove(index);
        self.field_error = None;
        self.draft.set_param(node, "args", Value::Array(args))
    }

    /// Commit the text of a field of the panel by its key. Returns false for a value that did
    /// not commit.
    pub fn commit_field(&mut self, key: &str, text: &str, locale: Locale) -> bool {
        let parts: Vec<&str> = key.splitn(4, ':').collect();
        let number = |index: usize| parts.get(index).and_then(|part| part.parse::<usize>().ok());
        let Some(node) = number(1).map(|node| node as u32) else {
            return false;
        };
        match (parts.first().copied(), parts.get(2).copied()) {
            (Some("param"), Some(name)) => self.commit_param(node, name, text, locale),
            (Some("label"), None) => self.commit_label(node, text, locale),
            (Some("test"), None) => self.commit_test_value(node, text),
            (Some("for"), None) => self.commit_test_seconds(node, text, locale),
            (Some("pair"), Some(_)) => match (number(2), number(3)) {
                (Some(index), Some(column)) => self.commit_pair(node, index, column, text, locale),
                _ => false,
            },
            (Some("arg"), Some(_)) => match number(2) {
                Some(index) => self.commit_arg(node, index, text, locale),
                None => false,
            },
            _ => false,
        }
    }

    /// Escape in a field: the parameter keeps its earlier value, and the range text goes.
    pub fn cancel_field(&mut self) {
        self.field_error = None;
    }

    /// Choose an entry of a select of the section "Inputs". A node connects its output to the
    /// pin, in the place of the connection that the pin has, as one undo step. `None` removes
    /// the connection. A choice that closes a loop does not connect: the status line shows the
    /// reason for 5 s, and the function returns false.
    pub fn choose_input(&mut self, node: u32, pin: &str, from: Option<u32>, now_us: u64) -> bool {
        let Some(from) = from else {
            return self.draft.disconnect(node, pin);
        };
        let graph = self.draft.graph();
        let known = graph
            .node(from)
            .is_some_and(|other| other.kind.has_output())
            && graph
                .node(node)
                .is_some_and(|target| target.kind.input(pin).is_some());
        if !known {
            return false;
        }
        let new = Connection::new(from, node, pin);
        // The choice takes the place of the connection of the pin.
        let others: Vec<Connection> = graph
            .connections
            .iter()
            .filter(|connection| connection.to != new.to)
            .cloned()
            .collect();
        match connection_refusal(&others, &new) {
            Some(code) => {
                self.refused = Some((code.as_str().to_owned(), now_us + REASON_TIME_US));
                false
            }
            None => self.draft.connect(from, node, pin),
        }
    }
}
