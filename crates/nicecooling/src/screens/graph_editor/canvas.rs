//! The canvas of the editor: what each node shows, where its pins are, and what an input of the
//! user on the canvas does to the draft.
//!
//! The canvas element is `nicecooling_flow`. The draft owns the Graph: the canvas reports each
//! input as a `FlowEvent`, the editor applies it to the draft and gives the canvas the new state.

use std::collections::BTreeSet;

use gpui::SharedString;
use nicecooling_flow::{
    ALREADY_CONNECTED, Connection as FlowConnection, FlowEdge, FlowEvent, FlowNode, FlowPoint,
    FlowState, HandleDef, HandlePosition, SAME_NODE, Selection, Viewport,
};
use nicecooling_graph::eval::EndValue;
use nicecooling_graph::funcs::is_true;
use nicecooling_graph::model::{Connection, Graph, Kind, Node, OUTPUT_PIN};
use nicecooling_graph::response::RunState;
use nicecooling_graph::validate::RefusalCode;

use super::rules::{connection_reason, kind_name};
use super::{EditorUi, Selected, number_text};
use crate::i18n::Locale;
use crate::screens::ScreenInput;
use crate::screens::format::{clock, unit_text};

/// The sizes of a node, in units of the canvas. One unit is 1 px at a zoom of 100%.
pub const NODE_WIDTH: f32 = 152.0;
/// The outline of 1 unit and the padding of 8 units around the rows of a node.
pub const NODE_INSET: f32 = 9.0;
pub const TITLE_ROW: f32 = 20.0;
pub const SUBJECT_ROW: f32 = 16.0;
pub const CAPTION_ROW: f32 = 16.0;
pub const VALUE_ROW: f32 = 22.0;
pub const GUIDE_ROW: f32 = 10.0;
pub const PIN_ROW: f32 = 20.0;
/// The dots of the ground are 24 units apart.
pub const GRID: f32 = 24.0;
/// Alt with an arrow key moves the selected node by this many units.
pub const NUDGE: f64 = 12.0;
/// How long the status line shows the reason of a refused connection.
pub const REASON_TIME_US: u64 = 5_000_000;
/// The canvas shows a node at this distance from the origin at most, in units.
pub const PLACE_LIMIT: f64 = 100_000.0;

/// A coordinate of a stored node as the canvas takes it. The store keeps any finite `f64`, and a
/// large one is not finite as the `f32` of the canvas.
pub fn canvas_place(value: f64) -> f32 {
    value.clamp(-PLACE_LIMIT, PLACE_LIMIT) as f32
}

/// The kinds that show a subject row under their title.
pub const fn has_subject(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::SensorSource
            | Kind::AlarmEventSource
            | Kind::Process
            | Kind::SensorEnd
            | Kind::ResponseEnd
    )
}

/// The kinds whose output is 1 or 0: the node shows "true" or "false" in the place of a unit.
pub const fn is_logic(kind: Kind) -> bool {
    matches!(
        kind,
        Kind::AlarmEventSource
            | Kind::Process
            | Kind::Less
            | Kind::LessThanOrEqual
            | Kind::Equal
            | Kind::GreaterThanOrEqual
            | Kind::Greater
            | Kind::Different
            | Kind::And
            | Kind::Or
            | Kind::Not
            | Kind::HasValue
            | Kind::Trigger
            | Kind::RiseDelay
            | Kind::FallDelay
            | Kind::ResponseEnd
    )
}

/// The height of the rows above the value row of a node. `note` is the caption row of an unbound
/// source.
fn rows_above_value(kind: Kind, note: bool) -> f32 {
    let mut height = NODE_INSET + TITLE_ROW;
    if has_subject(kind) {
        height += SUBJECT_ROW;
    }
    if note {
        height += CAPTION_ROW;
    }
    height
}

/// The height of a node that is no comment node.
pub fn node_height(kind: Kind, note: bool) -> f32 {
    let mut height = rows_above_value(kind, note) + VALUE_ROW + NODE_INSET;
    if kind == Kind::ResponseEnd {
        height += CAPTION_ROW;
    }
    if kind == Kind::Limiter {
        height += GUIDE_ROW;
    }
    height + PIN_ROW * kind.inputs().len() as f32
}

/// The width and the height of a new node of `kind`.
pub fn new_node_size(kind: Kind) -> (f32, f32) {
    if kind == Kind::Comment {
        let node = Node::new(0, kind);
        (node.number("width") as f32, node.number("height") as f32)
    } else {
        (NODE_WIDTH, node_height(kind, false))
    }
}

/// One input pin of a node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PinFace {
    /// The name of the pin in the model: `a`, `enable`.
    pub pin: &'static str,
    /// The name that the node shows: "A", "Enable".
    pub name: String,
    pub connected: bool,
}

/// The band of a Limiter node, with the value of its input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub min: f64,
    pub max: f64,
    pub input: Option<f64>,
}

impl Band {
    /// Where `value` is on the scale, from 0 at the left to 1 at the right. The scale has a
    /// margin of a fifth of the band at each side.
    pub fn place(&self, value: f64) -> f32 {
        let width = (self.max - self.min).abs().max(f64::EPSILON);
        let start = self.min - width / 5.0;
        (((value - start) / (width * 1.4)).clamp(0.0, 1.0)) as f32
    }
}

/// What one node shows on the canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeFace {
    pub id: u32,
    pub kind: Kind,
    pub x: f32,
    pub y: f32,
    /// The name of the kind, or "End node".
    pub title: String,
    /// The main parameter after the name: "10 s", "20 to 60".
    pub detail: Option<String>,
    /// A tag on the title row: "Test" on a Sensor node with a test value.
    pub tag: Option<String>,
    /// The title row has the Warning icon: the sensor of the source does not exist.
    pub warns: bool,
    pub subject: Option<String>,
    /// The subject is an id: it shows in mono text.
    pub subject_mono: bool,
    /// A caption row under the subject: "Sensor not found".
    pub note: Option<String>,
    pub label: Option<String>,
    /// The value, or "--".
    pub value: String,
    /// The unit of the value, or "true" and "false" for a logic output.
    pub unit: Option<String>,
    /// The value shows in `fg-dim`.
    pub dim: bool,
    /// A caption row under the value: the state of a response end node.
    pub state: Option<String>,
    pub band: Option<Band>,
    pub pins: Vec<PinFace>,
    pub has_output: bool,
    /// The node exists only in the draft: it has a dashed outline.
    pub draft_only: bool,
    pub selected: bool,
    /// The node breaks a rule of the validator: it has the Warning outline.
    pub breaks_rule: bool,
    /// The text of a comment node. A comment node has no rows.
    pub comment: Option<String>,
    pub width: f32,
    pub height: f32,
}

impl NodeFace {
    /// The distance from the top of the node to the middle of its output pin: the middle of the
    /// value row.
    pub fn output_offset(&self) -> f32 {
        rows_above_value(self.kind, self.note.is_some()) + VALUE_ROW / 2.0
    }

    /// The distance from the top of the node to the middle of the input pin `index`.
    pub fn pin_offset(&self, index: usize) -> f32 {
        let pins = PIN_ROW * self.pins.len() as f32;
        self.height - NODE_INSET - pins + PIN_ROW * (index as f32 + 0.5)
    }
}

/// One connection on the canvas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EdgeFace {
    pub from: u32,
    pub to: u32,
    pub pin: String,
    pub selected: bool,
}

/// A value on a node: 4 significant digits at most, and no trailing zero.
pub fn value_text(locale: Locale, value: f64) -> String {
    if !value.is_finite() {
        return "--".to_owned();
    }
    if value == 0.0 {
        return "0".to_owned();
    }
    let magnitude = value.abs().log10().floor() as i32;
    let decimals = (3 - magnitude).clamp(0, 8) as usize;
    let rounded: f64 = format!("{value:.decimals$}").parse().unwrap_or(value);
    number_text(locale, rounded)
}

/// The main parameter of a node, for its title row.
pub(crate) fn detail_of(node: &Node, locale: Locale) -> Option<String> {
    let seconds = |name: &str| {
        locale.fmt(
            "graph_editor.title.seconds",
            &[("seconds", &number_text(locale, node.number(name)))],
        )
    };
    match node.kind {
        Kind::Random | Kind::Limiter => Some(locale.fmt(
            "graph_editor.title.range",
            &[
                ("min", &number_text(locale, node.number("min"))),
                ("max", &number_text(locale, node.number("max"))),
            ],
        )),
        Kind::Round => Some(locale.plural(
            "graph_editor.title.decimals",
            node.number("decimals").max(0.0) as usize,
        )),
        Kind::Table => Some(locale.plural("graph_editor.title.pairs", node.pairs().len())),
        Kind::RiseDelay | Kind::FallDelay => Some(seconds("delay_s")),
        kind if kind.is_window() => Some(seconds("span_s")),
        _ => None,
    }
}

/// The text of an empty subject: the node has no sensor, event or process yet.
const NO_SUBJECT: &str = "--";

fn or_dashes(text: String) -> String {
    if text.is_empty() {
        NO_SUBJECT.to_owned()
    } else {
        text
    }
}

/// The face of one node of the draft. `stored` is the Graph that the app stores.
fn face_of(
    input: &ScreenInput<'_>,
    ui: &EditorUi,
    node: &Node,
    rule_node: Option<u32>,
) -> NodeFace {
    let locale = input.locale;
    let graph = ui.draft.graph();
    let sensors = &input.state.snapshot.sensors;
    let mut face = NodeFace {
        id: node.id,
        kind: node.kind,
        x: canvas_place(node.x),
        y: canvas_place(node.y),
        title: if node.kind.is_end() {
            locale.tr("graph_editor.canvas.end_title")
        } else {
            kind_name(node.kind, locale)
        },
        detail: detail_of(node, locale),
        tag: None,
        warns: false,
        subject: None,
        subject_mono: false,
        note: None,
        label: node.label.clone(),
        value: "--".to_owned(),
        unit: None,
        dim: false,
        state: None,
        band: None,
        pins: node
            .kind
            .inputs()
            .iter()
            .map(|pin| PinFace {
                pin: pin.name,
                name: locale.tr(&format!("graph_editor.pin.{}", pin.name)),
                connected: graph.connection_to(node.id, pin.name).is_some(),
            })
            .collect(),
        has_output: node.kind.has_output(),
        draft_only: ui
            .draft
            .stored()
            .is_none_or(|stored| stored.node(node.id).is_none()),
        selected: ui.selected == Some(Selected::Node(node.id)),
        breaks_rule: rule_node == Some(node.id),
        comment: None,
        width: NODE_WIDTH,
        height: 0.0,
    };
    match node.kind {
        Kind::SensorSource => {
            if ui.has_test_value(node.id) {
                face.tag = Some(locale.tr("graph_editor.canvas.test_tag"));
            }
            let id = node.text("sensor");
            match sensors.get(&id) {
                Some(sensor) => {
                    face.subject = Some(sensor.name.clone());
                    face.unit = Some(unit_text(&sensor.unit).to_owned());
                }
                // The sensor list is known only while the daemon is online.
                None if !id.is_empty() && input.state.is_online() => {
                    face.subject = Some(id);
                    face.subject_mono = true;
                    face.warns = true;
                    face.note = Some(locale.tr("graph_editor.canvas.sensor_not_found"));
                }
                None => face.subject = Some(or_dashes(id)),
            }
        }
        Kind::AlarmEventSource => {
            let (event, source) = (node.text("event"), node.text("source"));
            face.subject = Some(if event.is_empty() {
                NO_SUBJECT.to_owned()
            } else if source.is_empty() {
                locale.event_title(&event)
            } else {
                let leakshield = locale.tr("nav.leakshield");
                format!(
                    "{}, {}",
                    locale.event_title(&event),
                    input.state.source_name(&source, &leakshield)
                )
            });
        }
        Kind::Process => face.subject = Some(or_dashes(node.text("name"))),
        Kind::SensorEnd => {
            face.subject = Some(locale.tr("graph_editor.canvas.graph_sensor"));
            face.unit = Some(unit_text(&node.text("unit")).to_owned());
        }
        Kind::ResponseEnd => {
            let kind = node.response_kind();
            face.subject = Some(locale.tr(&format!("graphs.kind.{}", kind.as_str())));
            face.state = Some(response_state(ui, locale));
        }
        Kind::Limiter => {
            face.band = Some(Band {
                min: node.number("min"),
                max: node.number("max"),
                input: pin_value(ui, node.id, "a"),
            });
        }
        Kind::Comment => {
            face.comment = Some(node.text("text"));
            face.width = node.number("width") as f32;
        }
        _ => {}
    }
    face.height = if node.kind == Kind::Comment {
        node.number("height") as f32
    } else {
        node_height(node.kind, face.note.is_some())
    };
    // The daemon is offline, or the read of the values failed: the values are dim.
    face.dim = !input.state.is_online() || ui.live.failed;
    let value = shown_value(ui, node, face.draft_only);
    // A response end node shows its input as a logic value.
    let end = match &ui.live.trying {
        Some(trying) if ui.shows_try() => &trying.end,
        _ => &ui.live.end,
    };
    let value = match (node.kind, end) {
        (Kind::ResponseEnd, EndValue::Response(state)) if value.is_some() => {
            state.map(|on| if on { 1.0 } else { 0.0 })
        }
        _ => value,
    };
    if let Some(value) = value {
        face.value = value_text(locale, value);
        if is_logic(node.kind) {
            face.unit = Some(locale.tr(if is_true(value) {
                "graph_editor.canvas.logic_true"
            } else {
                "graph_editor.canvas.logic_false"
            }));
        }
    }
    face
}

/// The value that a node shows: the value of the applied Graph in the last read. A node that
/// exists only in the draft has none before "Apply". While a try runs, every node shows the
/// value of the try.
fn shown_value(ui: &EditorUi, node: &Node, draft_only: bool) -> Option<f64> {
    if let Some(trying) = &ui.live.trying
        && ui.shows_try()
    {
        return trying.values.get(&node.id).copied().flatten();
    }
    if draft_only {
        return None;
    }
    ui.live.value(node.id)
}

/// The value on an input pin of a node: the value of the node whose output the pin takes.
pub(crate) fn pin_value(ui: &EditorUi, node: u32, pin: &str) -> Option<f64> {
    let graph = ui.draft.graph();
    let from = graph.connection_to(node, pin)?.from.node;
    let source = graph.node(from)?;
    let draft_only = ui
        .draft
        .stored()
        .is_none_or(|stored| stored.node(from).is_none());
    shown_value(ui, source, draft_only)
}

/// The state of the response end node: "Fired 14:05:09" or "Not fired". A try runs no
/// response.
fn response_state(ui: &EditorUi, locale: Locale) -> String {
    if ui.tries.running.is_some() {
        return locale.tr("graph_editor.canvas.not_run_in_try");
    }
    match ui.live.run {
        Some(run) if run.state == RunState::Run && !ui.live.failed => match run.last_run_us {
            Some(us) => locale.fmt("graph_editor.canvas.fired", &[("time", &clock(us))]),
            None => locale.tr("graphs.fired"),
        },
        _ => locale.tr("graphs.not_fired"),
    }
}

/// The faces of the nodes of the draft, in the order of the draft.
pub fn node_faces(input: &ScreenInput<'_>, ui: &EditorUi, rule_node: Option<u32>) -> Vec<NodeFace> {
    ui.draft
        .graph()
        .nodes
        .iter()
        .map(|node| face_of(input, ui, node, rule_node))
        .collect()
}

/// The connections of the draft.
pub fn edge_faces(ui: &EditorUi) -> Vec<EdgeFace> {
    ui.draft
        .graph()
        .connections
        .iter()
        .map(|c| EdgeFace {
            from: c.from.node,
            to: c.to.node,
            pin: c.to.pin.clone(),
            selected: ui.selected
                == Some(Selected::Connection {
                    to: c.to.node,
                    pin: c.to.pin.clone(),
                }),
        })
        .collect()
}

/// What the canvas holds of one node: its place, its size and its pins. A value is not part of
/// it, so a new value does not set the canvas again.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeShape {
    pub id: u32,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    /// The input pins with the distance from the top of the node to their middle.
    pub pins: Vec<(&'static str, f32)>,
    /// The distance from the top of the node to the middle of the output pin.
    pub output: Option<f32>,
    pub dashed: bool,
    pub deletable: bool,
}

/// What the canvas holds of the draft.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shape {
    pub nodes: Vec<NodeShape>,
    /// The node of the output pin, the node of the input pin, and the input pin.
    pub edges: Vec<(u32, u32, String)>,
}

pub fn shape_of(nodes: &[NodeFace], edges: &[EdgeFace]) -> Shape {
    Shape {
        nodes: nodes
            .iter()
            .map(|face| NodeShape {
                id: face.id,
                x: face.x,
                y: face.y,
                width: face.width,
                height: face.height,
                pins: face
                    .pins
                    .iter()
                    .enumerate()
                    .map(|(index, pin)| (pin.pin, face.pin_offset(index)))
                    .collect(),
                output: face.has_output.then(|| face.output_offset()),
                dashed: face.draft_only,
                // A Graph keeps its end node.
                deletable: !face.kind.is_end(),
            })
            .collect(),
        edges: edges
            .iter()
            .map(|edge| (edge.from, edge.to, edge.pin.clone()))
            .collect(),
    }
}

/// The id of a connection on the canvas: an input pin takes one connection.
pub fn edge_id(to: u32, pin: &str) -> String {
    format!("{to}:{pin}")
}

/// The nodes of the canvas for a shape.
pub fn flow_nodes(shape: &Shape) -> Vec<FlowNode> {
    shape
        .nodes
        .iter()
        .map(|node| {
            let mut handles: Vec<HandleDef> = node
                .pins
                .iter()
                .map(|(pin, offset)| {
                    HandleDef::target(HandlePosition::Left)
                        .id(*pin)
                        .offset(*offset)
                })
                .collect();
            if let Some(offset) = node.output {
                handles.push(
                    HandleDef::source(HandlePosition::Right)
                        .id(OUTPUT_PIN)
                        .offset(offset),
                );
            }
            let mut flow = FlowNode::new(node.id.to_string(), node.x, node.y)
                .size(node.width, node.height)
                .handles(handles)
                .dashed(node.dashed);
            flow.deletable = node.deletable;
            flow
        })
        .collect()
}

/// The connections of the canvas for a shape: a line of 1.5 px.
pub fn flow_edges(shape: &Shape) -> Vec<FlowEdge> {
    shape
        .edges
        .iter()
        .map(|(from, to, pin)| {
            FlowEdge::new(edge_id(*to, pin), from.to_string(), to.to_string())
                .source_handle(OUTPUT_PIN)
                .target_handle(pin.clone())
                .stroke_width(1.5)
        })
        .collect()
}

/// A connection of the canvas as a connection of the model. `None` for a connection that does
/// not join an output pin to a named input pin of a node of the draft.
pub fn model_connection(connection: &FlowConnection) -> Option<Connection> {
    let from = connection.source.parse::<u32>().ok()?;
    let to = connection.target.parse::<u32>().ok()?;
    let pin = connection.target_handle.as_ref()?;
    Some(Connection::new(from, to, pin))
}

/// Why the validator refuses the connection `new` next to the connections `existing`: the two
/// ends are one node, the input pin has a connection, or the connection closes a loop. `None`
/// when the validator accepts it.
pub fn connection_refusal(existing: &[Connection], new: &Connection) -> Option<RefusalCode> {
    if new.from.node == new.to.node {
        return Some(RefusalCode::SameNode);
    }
    if existing.iter().any(|c| c.to == new.to) {
        return Some(RefusalCode::PinOccupied);
    }
    // A path from the node of the input pin back to the node of the output pin closes a loop.
    let mut seen: BTreeSet<u32> = BTreeSet::new();
    let mut stack = vec![new.to.node];
    while let Some(node) = stack.pop() {
        if node == new.from.node {
            return Some(RefusalCode::Loop);
        }
        if seen.insert(node) {
            stack.extend(
                existing
                    .iter()
                    .filter(|c| c.from.node == node)
                    .map(|c| c.to.node),
            );
        }
    }
    None
}

/// The rule of the editor for a connection drag: the rules of the validator. The reason is the
/// name of the refusal code.
pub fn connect_rule(connection: &FlowConnection, state: &FlowState) -> Result<(), SharedString> {
    let existing: Vec<Connection> = state
        .edges
        .iter()
        .filter_map(|edge| model_connection(&edge.connection()))
        .collect();
    let Some(new) = model_connection(connection) else {
        return Err(RefusalCode::UnknownPin.as_str().into());
    };
    match connection_refusal(&existing, &new) {
        Some(code) => Err(code.as_str().into()),
        None => Ok(()),
    }
}

/// A new state of the canvas, with the rule of the editor. The wheel zooms, and the dots of the
/// ground are 24 units apart.
pub fn new_flow_state(shape: &Shape) -> FlowState {
    let mut state = FlowState::new(flow_nodes(shape), flow_edges(shape)).with_rule(connect_rule);
    state.zoom_on_scroll = true;
    state.snap_grid = (GRID, GRID);
    state
}

/// The text of the reason that the canvas gives for a refused connection.
pub fn reason_text(reason: &str, locale: Locale) -> String {
    let code = match reason {
        SAME_NODE => RefusalCode::SameNode,
        ALREADY_CONNECTED => RefusalCode::PinOccupied,
        other => [
            RefusalCode::PinOccupied,
            RefusalCode::SameDirection,
            RefusalCode::SameNode,
            RefusalCode::Loop,
        ]
        .into_iter()
        .find(|code| code.as_str() == other)
        .unwrap_or(RefusalCode::SameDirection),
    };
    connection_reason(code, locale)
}

/// The selection of the canvas as the selection of the editor.
fn selected_of(selection: &Selection) -> Option<Selected> {
    match selection {
        Selection::Node(id) => id.parse().ok().map(Selected::Node),
        Selection::Connection(id) => {
            let (to, pin) = id.split_once(':')?;
            Some(Selected::Connection {
                to: to.parse().ok()?,
                pin: pin.to_owned(),
            })
        }
    }
}

/// The selection of the editor as the selection of the canvas.
pub fn flow_selection(selected: Option<&Selected>) -> Option<Selection> {
    selected.map(|selected| match selected {
        Selected::Node(id) => Selection::Node(id.to_string().into()),
        Selected::Connection { to, pin } => Selection::Connection(edge_id(*to, pin).into()),
    })
}

/// The middle of the view of the canvas, in units of the canvas.
pub fn view_centre(state: &FlowState) -> (f64, f64) {
    let centre = state
        .viewport
        .screen_to_flow(state.size.0 / 2.0, state.size.1 / 2.0);
    (f64::from(centre.x), f64::from(centre.y))
}

/// The view of a canvas that opens: the whole Graph, at 100% at most. A Graph that has its end
/// node only shows that node at the right of the view. `size` is the size of the canvas in px.
pub fn opening_view(state: &mut FlowState, size: (f32, f32)) {
    if let [only] = state.nodes.as_slice() {
        let width = only.measured_width.map_or(NODE_WIDTH, f32::from);
        let height = only.measured_height.map_or(0.0, f32::from);
        state.viewport = Viewport {
            x: size.0 - width - 2.0 * GRID - only.position.x,
            y: (size.1 - height) / 2.0 - only.position.y,
            zoom: 1.0,
        };
        return;
    }
    state.fit_view(GRID, size.0, size.1);
    if state.viewport.zoom > 1.0 {
        // The middle of the Graph stays in the middle of the view.
        state.zoom_at((size.0 / 2.0, size.1 / 2.0), 1.0);
    }
}

/// The node nearest to `from` in a direction: `(1, 0)` is right, `(0, -1)` is up. `None` when no
/// node is on that side.
pub fn nearest_node(graph: &Graph, from: u32, direction: (i32, i32)) -> Option<u32> {
    let origin = graph.node(from)?;
    graph
        .nodes
        .iter()
        .filter(|node| node.id != from)
        .filter_map(|node| {
            let (dx, dy) = (node.x - origin.x, node.y - origin.y);
            let along = dx * f64::from(direction.0) + dy * f64::from(direction.1);
            let across = dx * f64::from(direction.1) - dy * f64::from(direction.0);
            // A node counts when it is more along the direction than across it.
            (along > 0.0 && along >= across.abs()).then_some((along + across.abs(), node.id))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, id)| id)
}

impl EditorUi {
    /// Apply what the user did on the canvas. `events` are the events of one input: they are
    /// one undo step, so a node that goes takes its connections with it in that step.
    pub fn canvas_events(&mut self, events: &[FlowEvent], now_us: u64) {
        let before = self.draft.graph().clone();
        let mut graph = before.clone();
        for event in events {
            match event {
                FlowEvent::NodeMoved { id, to, .. } => {
                    if let Ok(id) = id.parse::<u32>()
                        && let Some(node) = graph.nodes.iter_mut().find(|node| node.id == id)
                    {
                        node.x = f64::from(to.x);
                        node.y = f64::from(to.y);
                    }
                }
                FlowEvent::ConnectionAdded { connection, .. } => {
                    if let Some(new) = model_connection(connection) {
                        graph.connections.retain(|c| c.to != new.to);
                        graph.connections.push(new);
                    }
                }
                FlowEvent::ConnectionRemoved { connection, .. } => {
                    if let Some(gone) = model_connection(connection) {
                        graph.connections.retain(|c| c.to != gone.to);
                    }
                }
                FlowEvent::NodeRemoved { id } => {
                    if let Ok(id) = id.parse::<u32>()
                        && graph.node(id).is_some_and(|node| !node.kind.is_end())
                    {
                        graph.nodes.retain(|node| node.id != id);
                        graph
                            .connections
                            .retain(|c| c.from.node != id && c.to.node != id);
                    }
                }
                FlowEvent::SelectionChanged { selection } => {
                    self.selected = selection.as_ref().and_then(selected_of);
                }
                FlowEvent::ConnectionRefused { reason, .. } => {
                    self.refused = Some((reason.to_string(), now_us + REASON_TIME_US));
                }
            }
        }
        if graph != before {
            self.draft.change(|draft| *draft = graph);
        }
        self.drop_lost_selection();
    }

    /// Delete: remove the selected node with its connections, or the selected connection. The
    /// end node stays. Returns whether something went.
    pub fn delete_selected(&mut self) -> bool {
        match self.selected.clone() {
            Some(Selected::Node(id)) => self.remove_node(id),
            Some(Selected::Connection { to, pin }) => {
                let done = self.draft.disconnect(to, &pin);
                self.drop_lost_selection();
                done
            }
            None => false,
        }
    }

    /// An arrow key on the canvas: select the nearest node in that direction. With no selection
    /// it selects the first node.
    pub fn select_towards(&mut self, direction: (i32, i32)) -> bool {
        let graph = self.draft.graph();
        let next = match &self.selected {
            Some(Selected::Node(id)) => nearest_node(graph, *id, direction),
            _ => graph.nodes.first().map(|node| node.id),
        };
        match next {
            Some(id) => {
                self.selected = Some(Selected::Node(id));
                true
            }
            None => false,
        }
    }

    /// Alt with an arrow key: move the selected node by 12 units, as one undo step.
    pub fn nudge_selected(&mut self, direction: (i32, i32)) -> bool {
        let Some(Selected::Node(id)) = self.selected else {
            return false;
        };
        let Some((x, y)) = self.draft.graph().node(id).map(|node| (node.x, node.y)) else {
            return false;
        };
        self.draft.move_node(
            id,
            x + NUDGE * f64::from(direction.0),
            y + NUDGE * f64::from(direction.1),
        )
    }

    /// Where a new node of `kind` goes so that its middle is at `centre`. A place that a node
    /// has already moves by 12 units, so two new nodes do not hide each other.
    fn free_place(&self, kind: Kind, centre: (f64, f64)) -> (f64, f64) {
        let (width, height) = new_node_size(kind);
        let mut place = (
            (centre.0 - f64::from(width) / 2.0).round(),
            (centre.1 - f64::from(height) / 2.0).round(),
        );
        let taken = |place: (f64, f64)| {
            self.draft
                .graph()
                .nodes
                .iter()
                .any(|node| (node.x, node.y) == place)
        };
        while taken(place) {
            place = (place.0 + NUDGE, place.1 + NUDGE);
        }
        place
    }

    /// Add a node of `kind` with its middle at `centre`, and select it. An end kind adds
    /// nothing: the Graph has its end node.
    pub fn add_node_at(&mut self, kind: Kind, centre: (f64, f64)) -> Option<u32> {
        if kind.is_end() {
            return None;
        }
        let (x, y) = self.free_place(kind, centre);
        let id = self.draft.add_node(kind, x, y);
        self.selected = Some(Selected::Node(id));
        Some(id)
    }

    /// The reason of the last refused connection, while the status line shows it.
    pub fn refusal_reason(&self, now_us: u64, locale: Locale) -> Option<String> {
        let (reason, until_us) = self.refused.as_ref()?;
        (now_us < *until_us).then(|| {
            locale.fmt(
                "graph_editor.canvas.not_connected",
                &[("reason", &reason_text(reason, locale))],
            )
        })
    }
}

/// The point of the canvas under a point of the window, in units of the canvas.
pub fn canvas_point(state: &FlowState, position: gpui::Point<gpui::Pixels>) -> (f64, f64) {
    let (x, y) = state.to_canvas(position);
    let point: FlowPoint = state.viewport.screen_to_flow(x, y);
    (f64::from(point.x), f64::from(point.y))
}
