//! The Graph editor: the draft, the canvas, the node list, the parameter panel, the live values,
//! the try, Apply and the response panel. The tests read the state and the model of the editor.

mod support;

use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::SharedString;
use nicecooling::command::Command;
use nicecooling::graphs::channel::{GraphCommand, TryRequest};
use nicecooling::graphs::service::{Env, GraphService, NoPower, SensorLink};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::values::SensorError;
use nicecooling::graphs::view::{GraphRow, GraphView, RowState, SharedView, shared_view};
use nicecooling::i18n::{EN, IT};
use nicecooling::model::{AppState, DaemonEvent, Link, SensorSample};
use nicecooling::notify::Plan;
use nicecooling::screens::graph_editor::canvas::{self, new_flow_state, shape_of};
use nicecooling::screens::graph_editor::draft::UNDO_LIMIT;
use nicecooling::screens::graph_editor::live::ValueSource;
use nicecooling::screens::graph_editor::palette::{ListForm, ListKey};
use nicecooling::screens::graph_editor::params::{
    Control, Guide, InputRow, Panel, label_key, pair_key, param_key,
};
use nicecooling::screens::graph_editor::try_run::{for_key, test_key};
use nicecooling::screens::graph_editor::{EditorKey, EditorModel, EditorUi, Selected, editor_key};
use nicecooling::screens::{Feedback, ScreenInput};
use nicecooling_core::clock::{SimClock, SimWallClock};
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_flow::{
    ConnectOutcome, FlowNodeId, FlowState, HandlePosition, HandleType, Selection, Viewport,
};
use nicecooling_graph::eval::EndValue;
use nicecooling_graph::model::{Connection, Family, Graph, Kind, Node, ResponseKind, StoredGraph};
use serde_json::json;
use support::fixtures::{full, online, sensor};
use support::graphs::{Dir, constant_graph};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

/// The wall clock of the tests: 2025-12-17 19:33:20 UTC.
const NOW_US: u64 = 1_766_000_000_000_000;

/// The row of a stored Graph, as the Graph service shows it before its first pass.
fn row_of(graph: &Graph) -> GraphRow {
    GraphRow {
        id: graph.id.clone(),
        name: graph.name.clone(),
        enabled: graph.enabled,
        state: if graph.enabled {
            RowState::Running
        } else {
            RowState::Disabled
        },
        values: BTreeMap::new(),
        end: EndValue::None,
        stored: StoredGraph::Valid(graph.clone()),
        run: None,
    }
}

/// What the Graph service shows for the stored Graphs `graphs`.
fn view_of(graphs: &[Graph]) -> GraphView {
    GraphView {
        rows: graphs.iter().map(row_of).collect(),
        ..GraphView::default()
    }
}

/// The model of the editor with the daemon state `state`, at the wall clock time `now_us`.
fn model_at(state: &AppState, view: &GraphView, ui: &EditorUi, now_us: u64) -> EditorModel {
    let feedback = Feedback::default();
    EditorModel::build(
        &ScreenInput {
            state,
            now_us,
            locale: EN,
            down_since_us: None,
            feedback: &feedback,
        },
        view,
        ui,
    )
}

fn model_with(state: &AppState, view: &GraphView, ui: &EditorUi) -> EditorModel {
    model_at(state, view, ui, NOW_US)
}

fn model(view: &GraphView, ui: &EditorUi) -> EditorModel {
    model_with(&online(), view, ui)
}

/// A Graph with a sensor end node: the sensor `x` goes through a limiter to the end node. It
/// holds the Graph sensor `graph_3`.
fn limiter_graph(id: &str) -> Graph {
    let mut graph = Graph::new(id, "Render temperature");
    graph.nodes = vec![
        Node::new(1, Kind::SensorSource).with_param("sensor", "x"),
        Node::new(2, Kind::Limiter)
            .with_param("min", 20)
            .with_param("max", 60),
        Node::new(3, Kind::SensorEnd).with_param("unit", "degC"),
    ];
    graph.connections = vec![Connection::new(1, 2, "a"), Connection::new(2, 3, "a")];
    graph.sensor_id = Some("graph_3".to_owned());
    graph
}

/// The editor on a stored Graph, and the view that holds it.
fn open(graph: Graph) -> (EditorUi, GraphView) {
    let view = view_of(std::slice::from_ref(&graph));
    (EditorUi::open(graph, &view), view)
}

fn place(ui: &EditorUi, node: u32) -> (f64, f64) {
    let node = ui.draft.graph().node(node).expect("the node");
    (node.x, node.y)
}

#[test]
fn undo_a_node_move() {
    let (mut ui, view) = open(limiter_graph("graph_1"));
    assert_eq!(place(&ui, 2), (0.0, 0.0));

    // One drag is one call, and one undo step.
    assert!(ui.draft.move_node(2, 120.0, 48.0));
    assert_eq!(place(&ui, 2), (120.0, 48.0));
    assert_eq!(ui.draft.undo_steps(), 1);
    assert!(model(&view, &ui).toolbar.undo.enabled);

    // Ctrl+Z puts the node back.
    let key = editor_key("z", true, false, false).expect("undo");
    assert!(ui.press(key));
    assert_eq!(place(&ui, 2), (0.0, 0.0));
    assert!(!ui.draft.is_dirty());
    assert!(!model(&view, &ui).toolbar.undo.enabled);
    assert!(model(&view, &ui).toolbar.redo.enabled);

    // A move to the same place is no step.
    assert!(!ui.draft.move_node(2, 0.0, 0.0));
    assert_eq!(ui.draft.undo_steps(), 0);
    // A move of a node that the draft does not have changes nothing.
    assert!(!ui.draft.move_node(99, 1.0, 1.0));
}

#[test]
fn redo_a_delete() {
    let (mut ui, _view) = open(limiter_graph("graph_1"));
    ui.selected = Some(Selected::Node(2));

    // The node goes with its two connections, as one step.
    assert!(ui.remove_node(2));
    assert!(ui.draft.graph().node(2).is_none());
    assert!(ui.draft.graph().connections.is_empty());
    assert_eq!(ui.draft.undo_steps(), 1);
    assert_eq!(ui.selected, None);

    assert!(ui.undo());
    assert!(ui.draft.graph().node(2).is_some());
    assert_eq!(ui.draft.graph().connections.len(), 2);

    // Ctrl+Shift+Z deletes the node again.
    let key = editor_key("z", true, true, false).expect("redo");
    assert_eq!(key, EditorKey::Redo);
    assert!(ui.press(key));
    assert!(ui.draft.graph().node(2).is_none());
    assert!(ui.draft.graph().connections.is_empty());
    assert!(!ui.redo(), "nothing is left to redo");

    // A new change drops the redo steps.
    assert!(ui.undo());
    assert!(ui.draft.move_node(1, 5.0, 5.0));
    assert!(!ui.draft.can_redo());
}

#[test]
fn undo_stack_stops_at_100_steps() {
    let (mut ui, _view) = open(limiter_graph("graph_1"));
    for step in 1..=150 {
        assert!(ui.draft.move_node(2, f64::from(step), 0.0));
    }
    assert_eq!(UNDO_LIMIT, 100);
    assert_eq!(ui.draft.undo_steps(), 100);

    let mut undone = 0;
    while ui.undo() {
        undone += 1;
    }
    assert_eq!(undone, 100);
    // The 50 oldest steps are gone: the node is where the move 50 put it.
    assert_eq!(place(&ui, 2), (50.0, 0.0));

    let mut redone = 0;
    while ui.redo() {
        redone += 1;
    }
    assert_eq!(redone, 100);
    assert_eq!(place(&ui, 2), (150.0, 0.0));
    assert_eq!(ui.draft.undo_steps(), 100);
}

#[test]
fn one_parameter_edit_is_one_step() {
    let (mut ui, _view) = open(constant_graph("graph_1", 5.0));
    assert!(ui.draft.set_param(1, "value", json!(42)));
    assert_eq!(ui.draft.undo_steps(), 1);
    assert_eq!(ui.draft.graph().node(1).unwrap().number("value"), 42.0);

    // The same value again is no step.
    assert!(!ui.draft.set_param(1, "value", json!(42)));
    assert_eq!(ui.draft.undo_steps(), 1);

    // A label and the name of the Graph are one step each.
    assert!(ui.draft.set_label(1, "five"));
    assert!(ui.draft.set_name("Six"));
    assert_eq!(ui.draft.undo_steps(), 3);

    assert!(ui.undo());
    assert!(ui.undo());
    assert!(ui.undo());
    let node = ui.draft.graph().node(1).unwrap();
    assert_eq!(node.number("value"), 5.0);
    assert_eq!(node.label, None);
    assert_eq!(ui.draft.graph().name, "Five");
    assert!(!ui.draft.is_dirty());
}

#[test]
fn two_end_nodes_message() {
    let (mut ui, view) = open(constant_graph("graph_1", 5.0));
    assert_eq!(model(&view, &ui).toolbar.rule, None);

    ui.draft.add_node(Kind::SensorEnd, 300.0, 0.0);
    let toolbar = model(&view, &ui).toolbar;
    assert_eq!(toolbar.rule.as_deref(), Some("A Graph has one end node"));
    assert!(!toolbar.apply.enabled, "a broken rule turns Apply off");
    assert!(!toolbar.try_run.enabled, "a broken rule turns Try off");

    // A Graph with no end node breaks the same rule.
    let mut none = Graph::new("graph_2", "No end");
    none.nodes = vec![Node::new(1, Kind::Constant)];
    let ui = EditorUi::open(none, &view);
    let toolbar = model(&view, &ui).toolbar;
    assert_eq!(toolbar.rule.as_deref(), Some("A Graph has one end node"));

    // A rule with a parameter names the node, the parameter and the range.
    let (mut ui, view) = open(constant_graph("graph_1", 5.0));
    let delay = ui.draft.add_node(Kind::RiseDelay, 0.0, 100.0);
    ui.draft.set_param(delay, "delay_s", json!(3601));
    let toolbar = model(&view, &ui).toolbar;
    assert_eq!(
        toolbar.rule.as_deref(),
        Some("On delay: Delay is 0 to 3600 s")
    );
    assert_eq!(toolbar.rule_node, Some(delay));
}

#[test]
fn apply_is_off_when_unchanged() {
    let (mut ui, view) = open(limiter_graph("graph_1"));
    let toolbar = model(&view, &ui).toolbar;
    assert_eq!(toolbar.apply.label, "Apply");
    assert!(!toolbar.apply.enabled);
    assert_eq!(toolbar.draft_state, None);
    assert!(toolbar.try_run.enabled);
    assert_eq!(toolbar.name, "Render temperature");
    assert_eq!(toolbar.back, "Graphs");

    // A change turns Apply on, and the undo of the change turns it off again.
    ui.draft.move_node(2, 10.0, 0.0);
    let toolbar = model(&view, &ui).toolbar;
    assert!(toolbar.apply.enabled);
    assert_eq!(toolbar.draft_state.as_deref(), Some("Not applied"));
    ui.undo();
    assert!(!model(&view, &ui).toolbar.apply.enabled);

    // A Graph that the app does not store yet is not applied.
    let mut fresh = Graph::new("graph_2", "Graph 2");
    fresh.nodes = vec![Node::new(1, Kind::SensorEnd)];
    let ui = EditorUi::open(fresh, &view);
    let toolbar = model(&view, &ui).toolbar;
    assert!(toolbar.apply.enabled);
    assert_eq!(toolbar.draft_state.as_deref(), Some("Not applied"));
}

#[test]
fn shortcut_keys_undo_and_redo() {
    assert_eq!(editor_key("z", true, false, false), Some(EditorKey::Undo));
    assert_eq!(editor_key("z", true, true, false), Some(EditorKey::Redo));
    assert_eq!(editor_key("Z", true, true, false), Some(EditorKey::Redo));
    assert_eq!(editor_key("z", false, false, false), None);
    assert_eq!(editor_key("y", true, false, false), None);
    // A text field with an open edit keeps the keys.
    assert_eq!(editor_key("z", true, false, true), None);
    assert_eq!(editor_key("z", true, true, true), None);

    let (mut ui, _view) = open(constant_graph("graph_1", 5.0));
    ui.draft.set_param(1, "value", json!(7));
    ui.draft.move_node(1, 24.0, 24.0);

    assert!(ui.press(EditorKey::Undo));
    assert_eq!(place(&ui, 1), (0.0, 0.0));
    assert_eq!(ui.draft.graph().node(1).unwrap().number("value"), 7.0);
    assert!(ui.press(EditorKey::Undo));
    assert_eq!(ui.draft.graph().node(1).unwrap().number("value"), 5.0);
    assert!(!ui.press(EditorKey::Undo), "no step is left");

    assert!(ui.press(EditorKey::Redo));
    assert!(ui.press(EditorKey::Redo));
    assert_eq!(place(&ui, 1), (24.0, 24.0));
    assert!(!ui.press(EditorKey::Redo));
}

/// `graph` with its nodes in a row, 220 units apart, so that no two pins are at one place.
fn spread(mut graph: Graph) -> Graph {
    for (index, node) in graph.nodes.iter_mut().enumerate() {
        node.x = 220.0 * index as f64;
    }
    graph
}

/// The canvas with the nodes and the connections of the draft, as the window gives them to it.
fn canvas_of(view: &GraphView, ui: &EditorUi) -> FlowState {
    let shown = model(view, ui);
    new_flow_state(&shape_of(&shown.nodes, &shown.edges))
}

/// Drag from the output pin of `from` to the input pin `pin` of `to` and let go. The events of
/// the canvas go to the editor, as in the window.
fn drag_connection(
    ui: &mut EditorUi,
    view: &GraphView,
    from: u32,
    to: u32,
    pin: &str,
) -> ConnectOutcome {
    let mut state = canvas_of(view, ui);
    let source: FlowNodeId = from.to_string().into();
    let target: FlowNodeId = to.to_string().into();
    let out: Option<SharedString> = Some("out".into());
    let input: Option<SharedString> = Some(pin.to_owned().into());
    let start = state
        .find_handle_center(&source, &out, HandlePosition::Right)
        .expect("the output pin");
    state.begin_connection(
        &source,
        &out,
        HandleType::Source,
        HandlePosition::Right,
        start,
    );
    let end = state
        .find_handle_center(&target, &input, HandlePosition::Left)
        .expect("the input pin");
    assert!(state.drag_connection(end));
    let outcome = state.end_connection();
    ui.canvas_events(&state.take_events(), NOW_US);
    outcome
}

/// The kinds of the draft, in the order of its nodes.
fn kinds_of(ui: &EditorUi) -> Vec<Kind> {
    ui.draft.graph().nodes.iter().map(|n| n.kind).collect()
}

#[test]
fn connect_by_drag() {
    let mut graph = spread(limiter_graph("graph_1"));
    graph.connections = vec![Connection::new(2, 3, "a")];
    let (mut ui, view) = open(graph);
    assert!(!model(&view, &ui).nodes[1].pins[0].connected);

    let outcome = drag_connection(&mut ui, &view, 1, 2, "a");
    assert!(matches!(outcome, ConnectOutcome::Added(_)));
    assert_eq!(
        ui.draft.graph().connections,
        vec![Connection::new(2, 3, "a"), Connection::new(1, 2, "a")]
    );
    assert_eq!(ui.draft.undo_steps(), 1, "one connection is one undo step");
    let shown = model(&view, &ui);
    assert_eq!(shown.edges.len(), 2);
    assert!(shown.nodes[1].pins[0].connected);

    // An output pin takes any number of connections.
    let add = ui.draft.add_node(Kind::Add, 220.0, 240.0);
    let outcome = drag_connection(&mut ui, &view, 1, add, "b");
    assert!(matches!(outcome, ConnectOutcome::Added(_)));
    assert_eq!(
        ui.draft.graph().connection_to(add, "b"),
        Some(&Connection::new(1, add, "b"))
    );

    // Ctrl+Z removes the connection again.
    assert!(ui.undo());
    assert_eq!(ui.draft.graph().connection_to(add, "b"), None);
}

#[test]
fn refusal_reason_is_input_already_connected() {
    let (mut ui, view) = open(spread(limiter_graph("graph_1")));
    let state = online();

    // The pin A of the end node has the connection from the limiter.
    let outcome = drag_connection(&mut ui, &view, 1, 3, "a");
    assert!(matches!(outcome, ConnectOutcome::Refused { .. }));
    assert_eq!(ui.draft.undo_steps(), 0, "the canvas draws nothing");
    assert_eq!(ui.draft.graph().connections.len(), 2);
    assert_eq!(
        model(&view, &ui).status.text.as_deref(),
        Some("Not connected: input already connected")
    );
    // The status line shows the reason for 5 s.
    let later = model_at(&state, &view, &ui, NOW_US + 4_900_000);
    assert!(later.status.text.is_some());
    let gone = model_at(&state, &view, &ui, NOW_US + 5_000_000);
    assert_eq!(gone.status.text, None);

    // A connection that closes a loop has its own reason.
    let mut graph = spread(limiter_graph("graph_1"));
    graph.connections = vec![Connection::new(2, 3, "a")];
    let (mut ui, view) = open(graph);
    let add = ui.draft.add_node(Kind::Add, 220.0, 240.0);
    ui.draft.connect(2, add, "a");
    let outcome = drag_connection(&mut ui, &view, add, 2, "a");
    assert!(matches!(outcome, ConnectOutcome::Refused { .. }));
    assert_eq!(
        model(&view, &ui).status.text.as_deref(),
        Some("Not connected: the connection closes a loop")
    );
    assert_eq!(ui.draft.graph().connection_to(2, "a"), None);
}

#[test]
fn end_node_cannot_be_deleted() {
    let (mut ui, view) = open(spread(limiter_graph("graph_1")));

    // The Delete key of the editor.
    ui.selected = Some(Selected::Node(3));
    assert!(!ui.delete_selected());
    assert!(!ui.draft.remove_node(3));
    assert!(ui.draft.graph().node(3).is_some());
    assert_eq!(ui.draft.undo_steps(), 0);

    // The Delete key of the canvas: the end node stays there too.
    let mut state = canvas_of(&view, &ui);
    state.set_selection(Some(Selection::Node("3".into())));
    assert!(!state.delete_selection());
    assert!(state.take_events().is_empty());

    // Another node goes with its two connections, as one undo step.
    state.set_selection(Some(Selection::Node("2".into())));
    assert!(state.delete_selection());
    ui.canvas_events(&state.take_events(), NOW_US);
    assert_eq!(kinds_of(&ui), [Kind::SensorSource, Kind::SensorEnd]);
    assert!(ui.draft.graph().connections.is_empty());
    assert_eq!(ui.draft.undo_steps(), 1);
    assert_eq!(ui.selected, None);
    assert!(ui.undo());
    assert_eq!(ui.draft.graph().connections.len(), 2);

    // A selected connection goes alone.
    ui.selected = Some(Selected::Connection {
        to: 3,
        pin: "a".to_owned(),
    });
    assert!(ui.delete_selected());
    assert_eq!(ui.draft.graph().connections, [Connection::new(1, 2, "a")]);
    assert_eq!(ui.selected, None);
}

#[test]
fn palette_search_finds_the_delay_nodes() {
    let (mut ui, view) = open(constant_graph("graph_1", 5.0));
    for text in ["delay", "DELAY", "  Delay "] {
        ui.list.search = text.to_owned();
        let list = model(&view, &ui).list;
        assert_eq!(list.families.len(), 1, "{text}");
        let family = &list.families[0];
        assert_eq!(family.family, Family::StateAndTime);
        assert_eq!((family.name.as_str(), family.count), ("State and time", 2));
        assert!(family.open);
        let names: Vec<&str> = family.rows.iter().map(|row| row.name.as_str()).collect();
        assert_eq!(names, ["On delay", "Off delay"]);
        assert_eq!(list.empty, None);
    }

    // A folded family opens for a search.
    ui.list.search.clear();
    ui.toggle_family(Family::StateAndTime);
    assert!(!model(&view, &ui).list.families[3].open);
    ui.list.search = "delay".to_owned();
    assert!(model(&view, &ui).list.families[0].open);

    ui.list.search = "zzz".to_owned();
    let list = model(&view, &ui).list;
    assert!(list.families.is_empty());
    assert_eq!(list.empty.as_deref(), Some("No node kind has this name."));
}

#[test]
fn double_click_adds_a_node_at_the_view_centre() {
    let (mut ui, view) = open(constant_graph("graph_1", 5.0));
    // The canvas is 620 x 500 px, at 200%, with the view moved by 100 and 50 px.
    let mut state = canvas_of(&view, &ui);
    state.size = (620.0, 500.0);
    state.viewport = Viewport {
        x: 100.0,
        y: 50.0,
        zoom: 2.0,
    };
    ui.view_centre = canvas::view_centre(&state);
    assert_eq!(ui.view_centre, (105.0, 100.0));

    let id = ui.list_double_click(Kind::Constant).expect("a node");
    assert_eq!(ui.draft.graph().node(id).unwrap().kind, Kind::Constant);
    // The middle of the node is at the middle of the view.
    let (width, height) = canvas::new_node_size(Kind::Constant);
    assert_eq!((width, height), (152.0, 60.0));
    assert_eq!(place(&ui, id), (105.0 - 76.0, 100.0 - 30.0));
    assert_eq!(ui.selected, Some(Selected::Node(id)));
    assert_eq!(ui.draft.undo_steps(), 1);

    // A second node at the same place moves by 12 units.
    let second = ui.list_double_click(Kind::Constant).expect("a node");
    assert_eq!(place(&ui, second), (29.0 + 12.0, 70.0 + 12.0));
    assert_ne!(second, id);
}

#[test]
fn palette_disables_the_end_kinds() {
    let (mut ui, view) = open(constant_graph("graph_1", 5.0));
    let list = model(&view, &ui).list;
    let rows: Vec<_> = list.families.iter().flat_map(|f| f.rows.iter()).collect();
    for row in &rows {
        let end = matches!(row.kind, Kind::SensorEnd | Kind::ResponseEnd);
        assert_eq!(row.enabled, !end, "{:?}", row.kind);
        if end {
            assert_eq!(row.tooltip, "The Graph has its end node.");
            assert!(!row.draggable);
        }
    }
    let limiter = rows.iter().find(|row| row.kind == Kind::Limiter).unwrap();
    assert_eq!(
        limiter.tooltip,
        "Limiter: Holds A between the minimum and the maximum."
    );

    // A double click, Enter and a drag of an end kind add nothing.
    assert_eq!(ui.list_double_click(Kind::SensorEnd), None);
    assert_eq!(ui.list_double_click(Kind::ResponseEnd), None);
    assert_eq!(ui.drop_from_list(Kind::ResponseEnd, (10.0, 10.0)), None);
    ui.list.cursor = Some(Kind::SensorEnd);
    assert_eq!(ui.list_key(ListKey::Enter, EN), None);
    assert_eq!(kinds_of(&ui), [Kind::Constant, Kind::SensorEnd]);
    assert_eq!(ui.draft.undo_steps(), 0);
}

#[test]
fn palette_lists_51_kinds() {
    let (ui, view) = open(constant_graph("graph_1", 5.0));
    let list = model(&view, &ui).list;
    let names: Vec<&str> = list.families.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Sources",
            "Math",
            "Logic",
            "State and time",
            "Filters",
            "Generators",
            "Other"
        ]
    );
    let counts: Vec<usize> = list.families.iter().map(|f| f.count).collect();
    assert_eq!(counts, [5, 16, 11, 6, 5, 5, 3]);
    let rows: Vec<_> = list.families.iter().flat_map(|f| f.rows.iter()).collect();
    assert_eq!(rows.len(), 51);
    assert_eq!(rows.iter().filter(|row| row.kind.is_end()).count(), 2);
    assert!(list.families.iter().all(|family| family.open));
    // Every kind has a name and a help text of its own key.
    for row in &rows {
        assert!(!row.name.contains("graph_editor."), "{:?}", row.kind);
        assert!(!row.tooltip.contains("graph_editor."), "{:?}", row.kind);
    }
    assert_eq!(rows[0].name, "Sensor");
    assert_eq!(rows[50].name, "Response end node");
}

#[test]
fn node_list_is_a_column_at_1280_px() {
    let mut fresh = Graph::new("graph_2", "Graph 2");
    fresh.nodes = vec![Node::new(1, Kind::SensorEnd)];
    let view = view_of(&[]);
    let mut ui = EditorUi::open(fresh, &view);
    ui.set_width(1280.0);
    let shown = model(&view, &ui);
    assert_eq!(shown.list.form, ListForm::Column);
    assert!(shown.list.open);
    assert_eq!(shown.toolbar.add_node, None);
    assert_eq!(shown.list.search, "Search nodes");
    // A Graph with its end node only says how to add a node.
    assert_eq!(
        shown.hint.as_deref(),
        Some("Double-click a node in the list to add it.")
    );
    ui.set_width(1920.0);
    assert_eq!(model(&view, &ui).list.form, ListForm::Column);

    // The button "Add node" does nothing while the list is a column.
    ui.toggle_list();
    assert!(!ui.list.open);
    ui.list_double_click(Kind::Constant);
    assert_eq!(model(&view, &ui).hint, None);
}

#[test]
fn drag_from_the_column_adds_a_node_at_the_pointer() {
    let (mut ui, view) = open(constant_graph("graph_1", 5.0));
    ui.set_width(1280.0);
    let list = model(&view, &ui).list;
    let constant = list.families[0]
        .rows
        .iter()
        .find(|row| row.kind == Kind::Constant)
        .unwrap();
    assert!(constant.draggable);

    // The canvas starts at 388, 92 of the window. The pointer lets go at 700, 400.
    let mut state = canvas_of(&view, &ui);
    state.origin = (388.0, 92.0);
    let point = canvas::canvas_point(&state, gpui::point(gpui::px(700.0), gpui::px(400.0)));
    assert_eq!(point, (312.0, 308.0));
    let id = ui.drop_from_list(Kind::Constant, point).expect("a node");
    assert_eq!(ui.draft.graph().node(id).unwrap().kind, Kind::Constant);
    assert_eq!(place(&ui, id), (312.0 - 76.0, 308.0 - 30.0));
    assert_eq!(ui.draft.undo_steps(), 1);

    // At 50% the same pointer is twice as far in the units of the canvas.
    state.viewport.zoom = 0.5;
    let point = canvas::canvas_point(&state, gpui::point(gpui::px(700.0), gpui::px(400.0)));
    assert_eq!(point, (624.0, 616.0));
}

/// The editor on a stored Graph in a window of 1100 px, with the popover open.
fn open_popover() -> (EditorUi, GraphView) {
    let (mut ui, view) = open(constant_graph("graph_1", 5.0));
    ui.set_width(1100.0);
    ui.view_centre = (400.0, 300.0);
    ui.toggle_list();
    assert!(ui.list.open);
    (ui, view)
}

#[test]
fn add_node_button_replaces_the_column_at_1279_px() {
    let mut fresh = Graph::new("graph_2", "Graph 2");
    fresh.nodes = vec![Node::new(1, Kind::ResponseEnd)];
    let view = view_of(&[]);
    let mut ui = EditorUi::open(fresh, &view);
    ui.set_width(1279.0);
    let shown = model(&view, &ui);
    assert_eq!(shown.list.form, ListForm::Popover);
    assert!(!shown.list.open, "no node list column");
    let button = shown.toolbar.add_node.expect("the button");
    assert_eq!(button.label, "Add node");
    assert!(button.enabled);
    assert_eq!(shown.hint.as_deref(), Some("Press Add node to add a node."));

    // The button opens the popover, and closes it again.
    ui.toggle_list();
    assert!(model(&view, &ui).list.open);
    ui.toggle_list();
    assert!(!model(&view, &ui).list.open);

    // A window width of 1280 px or more closes the popover.
    ui.toggle_list();
    ui.set_width(1280.0);
    assert!(!ui.list.open);
    assert_eq!(model(&view, &ui).toolbar.add_node, None);
}

#[test]
fn add_node_popover_has_the_search_and_the_7_families() {
    let (ui, view) = open_popover();
    // The search field has the focus when the popover opens.
    assert!(ui.list.focus_search);
    let list = model(&view, &ui).list;
    assert_eq!(list.form, ListForm::Popover);
    assert!(list.open);
    assert_eq!(list.search, "Search nodes");
    assert_eq!(list.families.len(), 7);
    assert!(list.families.iter().all(|family| family.open));
    let rows: usize = list.families.iter().map(|family| family.rows.len()).sum();
    assert_eq!(rows, 51);
}

#[test]
fn enter_in_the_popover_adds_a_node_at_the_view_centre() {
    let (mut ui, view) = open_popover();
    // Enter with no marked row adds nothing.
    assert_eq!(ui.list_key(ListKey::Enter, EN), None);

    // Down marks "Sensor", "Alarm event", then "Constant".
    for _ in 0..3 {
        assert_eq!(ui.list_key(ListKey::Down, EN), None);
    }
    assert_eq!(ui.list.cursor, Some(Kind::Constant));
    let list = model(&view, &ui).list;
    let marked: Vec<Kind> = list
        .families
        .iter()
        .flat_map(|family| family.rows.iter())
        .filter(|row| row.marked)
        .map(|row| row.kind)
        .collect();
    assert_eq!(marked, [Kind::Constant]);

    let id = ui.list_key(ListKey::Enter, EN).expect("a node");
    assert_eq!(ui.draft.graph().node(id).unwrap().kind, Kind::Constant);
    assert_eq!(place(&ui, id), (400.0 - 76.0, 300.0 - 30.0));
    assert!(ui.list.open, "the popover stays open");

    // Up goes back, and stops at the first row.
    for _ in 0..5 {
        ui.list_key(ListKey::Up, EN);
    }
    assert_eq!(ui.list.cursor, Some(Kind::SensorSource));

    // Left folds the family of the marked row, and Right unfolds it.
    ui.list_key(ListKey::Left, EN);
    assert!(!model(&view, &ui).list.families[0].open);
    assert_eq!(
        ui.list_key(ListKey::Enter, EN),
        None,
        "the row does not show"
    );
    ui.list_key(ListKey::Right, EN);
    assert!(model(&view, &ui).list.families[0].open);
    assert_eq!(ListKey::parse("enter"), Some(ListKey::Enter));
    assert_eq!(ListKey::parse("a"), None);
}

#[test]
fn double_click_in_the_popover_adds_a_node_at_the_view_centre() {
    let (mut ui, _view) = open_popover();
    let id = ui.list_double_click(Kind::Constant).expect("a node");
    assert_eq!(ui.draft.graph().node(id).unwrap().kind, Kind::Constant);
    assert_eq!(place(&ui, id), (400.0 - 76.0, 300.0 - 30.0));
    assert_eq!(ui.draft.undo_steps(), 1);
}

#[test]
fn popover_stays_open_after_an_add() {
    let (mut ui, view) = open_popover();
    ui.list_double_click(Kind::Constant).expect("a node");
    assert!(ui.list.open);
    assert!(model(&view, &ui).list.open);
    ui.list.cursor = Some(Kind::Add);
    ui.list_key(ListKey::Enter, EN).expect("a node");
    assert!(ui.list.open);
    assert_eq!(
        kinds_of(&ui),
        [Kind::Constant, Kind::SensorEnd, Kind::Constant, Kind::Add]
    );
}

#[test]
fn popover_closes_on_escape() {
    let (mut ui, view) = open_popover();
    let before = ui.draft.graph().clone();
    assert_eq!(ui.list_key(ListKey::Escape, EN), None);
    assert!(!ui.list.open);
    assert!(!model(&view, &ui).list.open);
    // The focus goes back to the button "Add node".
    assert!(ui.list.focus_button);
    assert_eq!(ui.draft.graph(), &before);
    assert_eq!(ui.draft.undo_steps(), 0);
}

#[test]
fn popover_closes_on_a_press_outside() {
    let (mut ui, view) = open_popover();
    let before = ui.draft.graph().clone();
    ui.press_outside_list();
    assert!(!ui.list.open);
    assert!(!model(&view, &ui).list.open);
    assert_eq!(ui.draft.graph(), &before);
    assert_eq!(ui.draft.undo_steps(), 0);
}

#[test]
fn drag_from_the_popover_adds_no_node() {
    let (mut ui, view) = open_popover();
    let list = model(&view, &ui).list;
    assert!(
        list.families
            .iter()
            .flat_map(|family| family.rows.iter())
            .all(|row| !row.draggable)
    );
    assert_eq!(ui.drop_from_list(Kind::Constant, (300.0, 200.0)), None);
    assert_eq!(kinds_of(&ui), [Kind::Constant, Kind::SensorEnd]);
    assert_eq!(ui.draft.undo_steps(), 0);
    assert!(ui.list.open);
}

/// The parameter panel of the editor.
fn panel_of(view: &GraphView, ui: &EditorUi) -> Panel {
    model(view, ui).panel.expect("the panel")
}

/// The editor on a Graph with one node of `kind` as node 1. Every kind but an end kind gets a
/// sensor end node as node 2. Node 1 is selected.
fn open_kind(kind: Kind) -> (EditorUi, GraphView) {
    let mut graph = Graph::new("graph_1", "Kinds");
    graph.nodes = vec![Node::new(1, kind)];
    if !kind.is_end() {
        graph.nodes.push(Node::new(2, Kind::SensorEnd));
    }
    let view = view_of(&[]);
    let mut ui = EditorUi::open(graph, &view);
    ui.selected = Some(Selected::Node(1));
    (ui, view)
}

/// The range that the panel shows under the field of the parameter `name`.
fn error_of(view: &GraphView, ui: &EditorUi, name: &str) -> Option<String> {
    let panel = panel_of(view, ui);
    let row = panel.rows.iter().find(|row| row.name == name);
    row.expect("the parameter").error.clone()
}

#[test]
fn every_kind_has_a_panel() {
    assert_eq!(Kind::ALL.len(), 51);
    for kind in Kind::ALL.iter().copied() {
        let (ui, view) = open_kind(kind);
        let panel = panel_of(&view, &ui);
        assert_eq!((panel.node, panel.kind), (1, kind));
        assert!(!panel.title.is_empty(), "{kind}");
        assert!(!panel.title.contains("graph_editor."), "{kind}");
        assert!(!panel.help.contains("graph_editor."), "{kind}");
        // One row for each parameter of the table of the engine, in its order.
        let specs = Node::new(1, kind).param_specs();
        let names: Vec<&str> = panel.rows.iter().map(|row| row.name).collect();
        let expected: Vec<&str> = specs.iter().map(|spec| spec.name).collect();
        assert_eq!(names, expected, "{kind}");
        for row in &panel.rows {
            assert!(!row.label.contains("graph_editor."), "{kind} {}", row.name);
            assert_eq!(row.error, None);
        }
        assert_eq!(panel.inputs.len(), kind.inputs().len(), "{kind}");
        assert_eq!(panel.inputs_title, "Inputs");
        // An end node has no label and no "Delete node".
        assert_eq!(panel.label.is_some(), !kind.is_end(), "{kind}");
        assert_eq!(panel.delete.is_some(), !kind.is_end(), "{kind}");
        assert_eq!(panel.end.is_some(), kind.is_end(), "{kind}");
        assert_eq!(panel.family.is_some(), !kind.is_end(), "{kind}");
        assert_eq!(panel.sensor.is_some(), kind == Kind::SensorEnd, "{kind}");
        let guided = matches!(kind, Kind::Limiter | Kind::Table);
        assert_eq!(panel.guide.is_some(), guided, "{kind}");
    }

    // The header of a node and of an end node.
    let (mut ui, view) = open_kind(Kind::Limiter);
    let panel = panel_of(&view, &ui);
    assert_eq!(panel.title, "Limiter");
    assert_eq!(panel.family.as_deref(), Some("Math"));
    assert_eq!(panel.help, "Holds A between the minimum and the maximum.");
    assert_eq!(panel.delete.as_deref(), Some("Delete node"));
    let label = panel.label.expect("the label");
    assert_eq!(label.label, "Label");
    assert_eq!(
        label.caption,
        "A label names the node in a message: {value:water}."
    );
    let labels: Vec<&str> = panel.rows.iter().map(|row| row.label.as_str()).collect();
    assert_eq!(labels, ["Minimum", "Maximum"]);
    assert_eq!(
        panel.rows[0].control,
        Control::Field {
            text: "-100".to_owned(),
            unit: None
        }
    );
    match panel.guide {
        Some(Guide::Band {
            minimum, maximum, ..
        }) => assert_eq!(
            (minimum.as_str(), maximum.as_str()),
            ("Minimum -100", "Maximum 100")
        ),
        other => panic!("no band guide: {other:?}"),
    }
    // With no selection the panel shows the end node.
    ui.selected = None;
    let panel = panel_of(&view, &ui);
    assert_eq!((panel.node, panel.title.as_str()), (2, "End node"));
    let end = panel.end.expect("the end switch");
    assert_eq!(
        (end.sensor.as_str(), end.response.as_str()),
        ("Sensor", "Response")
    );
    assert!(end.is_sensor);
    let sensor = panel.sensor.expect("the Graph sensor");
    assert_eq!(
        (sensor.label.as_str(), sensor.id.as_str()),
        ("Graph sensor", "Created at Apply")
    );
    assert!(!sensor.has_id);
    assert_eq!(sensor.used, ["Not used"]);
    match &panel.rows[0].control {
        Control::Select { shown, choices } => {
            assert_eq!(shown, "°C");
            let units: Vec<&str> = choices.iter().map(|(_, text, _)| text.as_str()).collect();
            assert_eq!(units, ["°C", "rpm", "%", "V", "A", "W", "L/h", "mbar"]);
        }
        other => panic!("the unit is no select: {other:?}"),
    }

    // The segmented control changes the kind of the end node. Each of the 8 response kinds has
    // the rows of its parameters.
    assert!(ui.draft.set_end_kind(Kind::ResponseEnd));
    for kind in ResponseKind::ALL {
        ui.choose_param(2, "kind", kind.as_str());
        let panel = panel_of(&view, &ui);
        let node = ui.draft.graph().node(2).unwrap();
        assert_eq!(node.response_kind(), kind);
        let names: Vec<&str> = panel.rows.iter().map(|row| row.name).collect();
        let specs = node.param_specs();
        let expected: Vec<&str> = specs.iter().map(|spec| spec.name).collect();
        assert_eq!(names, expected, "{}", kind.as_str());
        for row in &panel.rows {
            assert!(!row.label.contains("graph_editor."), "{}", row.name);
        }
    }

    // The controls of the other parameter types.
    let (ui, view) = open_kind(Kind::Round);
    match &panel_of(&view, &ui).rows[1].control {
        Control::Select { shown, choices } => {
            assert_eq!(shown, "Nearest");
            let texts: Vec<&str> = choices.iter().map(|(_, text, _)| text.as_str()).collect();
            assert_eq!(texts, ["Nearest", "Down", "Up"]);
        }
        other => panic!("the mode is no select: {other:?}"),
    }
    let (ui, view) = open_kind(Kind::RiseDelay);
    assert_eq!(
        panel_of(&view, &ui).rows[0].control,
        Control::Field {
            text: "1".to_owned(),
            unit: Some("s".to_owned())
        }
    );
}

#[test]
fn fields_enforce_the_ranges() {
    let (mut ui, view) = open_kind(Kind::RiseDelay);
    for text in ["3601", "-1", "1.5", "abc", ""] {
        assert!(!ui.commit_param(1, "delay_s", text, EN), "{text}");
        // The value does not commit, and the panel shows the range under the field.
        assert_eq!(ui.draft.graph().node(1).unwrap().number("delay_s"), 1.0);
        assert_eq!(ui.draft.undo_steps(), 0);
        assert_eq!(
            error_of(&view, &ui, "delay_s").as_deref(),
            Some("0 to 3600 s")
        );
    }
    assert!(ui.commit_param(1, "delay_s", "3600", EN));
    assert_eq!(ui.draft.graph().node(1).unwrap().number("delay_s"), 3600.0);
    assert_eq!(error_of(&view, &ui, "delay_s"), None);
    assert_eq!(ui.draft.undo_steps(), 1);
    // The field of a parameter commits by its key, as the window does it.
    assert!(ui.commit_field(&param_key(1, "delay_s"), "20", EN));
    assert_eq!(ui.draft.graph().node(1).unwrap().number("delay_s"), 20.0);
    assert!(!ui.commit_field(&param_key(1, "delay_s"), "99999", EN));

    // A number with decimals takes a comma or a point.
    let (mut ui, view) = open_kind(Kind::LowPass);
    assert!(!ui.commit_param(1, "damping", "100", EN));
    assert_eq!(
        error_of(&view, &ui, "damping").as_deref(),
        Some("0 to 99.99")
    );
    assert!(ui.commit_param(1, "damping", "99,5", EN));
    assert_eq!(ui.draft.graph().node(1).unwrap().number("damping"), 99.5);
    assert!(!ui.commit_param(1, "span_s", "1801", EN));
    assert_eq!(
        error_of(&view, &ui, "span_s").as_deref(),
        Some("1 to 1800 s")
    );

    let (mut ui, view) = open_kind(Kind::Round);
    assert!(!ui.commit_param(1, "decimals", "9", EN));
    assert_eq!(error_of(&view, &ui, "decimals").as_deref(), Some("0 to 8"));
    assert!(ui.commit_param(1, "decimals", "8", EN));

    // A number without a range takes any finite number.
    let (mut ui, view) = open_kind(Kind::Constant);
    assert!(!ui.commit_param(1, "value", "1e400", EN));
    assert_eq!(error_of(&view, &ui, "value").as_deref(), Some("a number"));
    assert!(ui.commit_param(1, "value", "-3.25", EN));
    assert_eq!(ui.draft.graph().node(1).unwrap().number("value"), -3.25);

    let (mut ui, view) = open_kind(Kind::Comment);
    assert!(!ui.commit_param(1, "width", "59", EN));
    assert_eq!(error_of(&view, &ui, "width").as_deref(), Some("60 to 800"));

    // A text has a count of characters.
    let (mut ui, view) = open_kind(Kind::Process);
    assert!(!ui.commit_param(1, "name", "", EN));
    assert_eq!(
        error_of(&view, &ui, "name").as_deref(),
        Some("1 to 255 characters")
    );
    assert!(ui.commit_param(1, "name", "blender", EN));

    // The label: 1 to 32 characters of letters, digits and "_". An empty text removes it.
    assert!(!ui.commit_field(&label_key(1), "two words", EN));
    let label = panel_of(&view, &ui).label.expect("the label");
    assert_eq!(
        label.error.as_deref(),
        Some("1 to 32 letters, digits and _")
    );
    assert_eq!(ui.draft.graph().node(1).unwrap().label, None);
    assert!(ui.commit_field(&label_key(1), "render_1", EN));
    assert_eq!(
        ui.draft.graph().node(1).unwrap().label.as_deref(),
        Some("render_1")
    );
    assert!(ui.commit_label(1, "", EN));
    assert_eq!(ui.draft.graph().node(1).unwrap().label, None);

    // The countdown of a power response: 10 to 3600 s.
    let (mut ui, view) = open_kind(Kind::ResponseEnd);
    ui.choose_param(1, "kind", "poweroff");
    assert!(!ui.commit_param(1, "countdown_s", "5", EN));
    assert_eq!(
        error_of(&view, &ui, "countdown_s").as_deref(),
        Some("10 to 3600 s")
    );
    assert!(ui.commit_param(1, "countdown_s", "10", EN));
}

#[test]
fn table_pairs_editor_takes_2_to_64_pairs() {
    let (mut ui, view) = open_kind(Kind::Table);
    let pairs_of = |view: &GraphView, ui: &EditorUi| match &panel_of(view, ui).rows[1].control {
        Control::Pairs {
            pairs,
            can_add,
            can_remove,
        } => (pairs.clone(), *can_add, *can_remove),
        other => panic!("the pairs are no pair editor: {other:?}"),
    };
    // A new Table has the pairs (-100, -100) and (100, 100): none can go.
    let (pairs, can_add, can_remove) = pairs_of(&view, &ui);
    let text = |a: &str, b: &str| (a.to_owned(), b.to_owned());
    assert_eq!(pairs, [text("-100", "-100"), text("100", "100")]);
    assert!(can_add && !can_remove);
    assert!(!ui.remove_pair(1, 0));
    assert_eq!(ui.draft.undo_steps(), 0);

    // One number of a pair is one undo step.
    assert!(ui.commit_field(&pair_key(1, 1, 1), "42,5", EN));
    assert_eq!(
        ui.draft.graph().node(1).unwrap().pairs(),
        [(-100.0, -100.0), (100.0, 42.5)]
    );
    assert_eq!(ui.draft.undo_steps(), 1);
    assert!(!ui.commit_field(&pair_key(1, 0, 0), "low", EN));
    assert_eq!(ui.draft.undo_steps(), 1);

    // A new pair has an input above every input. The table takes 64 pairs.
    assert!(ui.add_pair(1));
    assert_eq!(ui.draft.graph().node(1).unwrap().pairs()[2], (101.0, 42.5));
    assert!(ui.remove_pair(1, 2));
    for _ in 0..62 {
        assert!(ui.add_pair(1));
    }
    assert_eq!(ui.draft.graph().node(1).unwrap().pairs().len(), 64);
    let (pairs, can_add, can_remove) = pairs_of(&view, &ui);
    assert_eq!(pairs.len(), 64);
    assert!(!can_add && can_remove);
    assert!(!ui.add_pair(1), "the 65th pair");
    assert_eq!(model(&view, &ui).toolbar.rule, None, "64 pairs are valid");
    assert_eq!(
        model(&view, &ui).nodes[0].detail.as_deref(),
        Some("64 pairs")
    );

    // The guide of the panel has the pairs in the order of their inputs.
    match panel_of(&view, &ui).guide {
        Some(Guide::Table { pairs, step, .. }) => {
            assert_eq!(pairs.len(), 64);
            assert!(pairs.windows(2).all(|pair| pair[0].0 < pair[1].0));
            assert!(!step);
        }
        other => panic!("no table guide: {other:?}"),
    }
}

#[test]
fn sensor_picker_lists_graph_sensors() {
    let mut state = full();
    for (id, name, origin) in [
        ("graph_1", "Render temperature", "graph"),
        ("derived_1", "Hottest core", "derived"),
    ] {
        state.snapshot.sensors.insert(
            id.to_owned(),
            sensor(id, name, "temperature", "degC", origin, ""),
        );
    }
    let (mut ui, view) = open_kind(Kind::SensorSource);
    let choices_of =
        |ui: &EditorUi| match &model_with(&state, &view, ui).panel.unwrap().rows[0].control {
            Control::Sensor { shown, choices } => (shown.clone(), choices.clone()),
            other => panic!("the sensor is no picker: {other:?}"),
        };

    // Every sensor of the daemon: measured, Derived and Graph sensors.
    let (shown, choices) = choices_of(&ui);
    assert_eq!(shown, "Choose");
    assert_eq!(choices.len(), state.snapshot.sensors.len());
    let graph_sensor = choices.iter().find(|c| c.id == "graph_1").unwrap();
    assert_eq!(graph_sensor.name, "Render temperature");
    assert_eq!(graph_sensor.caption, "graph");
    assert_eq!(graph_sensor.value, "--");
    assert!(!graph_sensor.selected);
    let derived = choices.iter().find(|c| c.id == "derived_1").unwrap();
    assert_eq!(derived.caption, "derived");
    assert!(choices.iter().any(|c| c.id == "nct6798_temp1"));

    // A choice is one undo step, and the node shows the name of the sensor.
    assert!(ui.choose_param(1, "sensor", "graph_1"));
    assert_eq!(ui.draft.undo_steps(), 1);
    assert_eq!(ui.draft.graph().node(1).unwrap().text("sensor"), "graph_1");
    let (shown, choices) = choices_of(&ui);
    assert_eq!(shown, "Render temperature");
    assert!(choices.iter().find(|c| c.id == "graph_1").unwrap().selected);
    let face = &model_with(&state, &view, &ui).nodes[0];
    assert_eq!(face.subject.as_deref(), Some("Render temperature"));
    assert_eq!(face.unit.as_deref(), Some("°C"));
}

#[test]
fn escape_keeps_the_old_value() {
    let (mut ui, view) = open_kind(Kind::FallDelay);
    assert!(ui.commit_param(1, "delay_s", "10", EN));

    // A value that did not commit: the field keeps it, and the draft keeps the earlier one.
    assert!(!ui.commit_param(1, "delay_s", "5000", EN));
    assert!(error_of(&view, &ui, "delay_s").is_some());
    assert_eq!(ui.draft.graph().node(1).unwrap().number("delay_s"), 10.0);

    // Escape: the range text goes, and the field shows the earlier value again.
    ui.cancel_field();
    assert_eq!(error_of(&view, &ui, "delay_s"), None);
    assert_eq!(
        panel_of(&view, &ui).rows[0].control,
        Control::Field {
            text: "10".to_owned(),
            unit: Some("s".to_owned())
        }
    );
    assert_eq!(ui.draft.undo_steps(), 1, "Escape is no undo step");
    assert_eq!(model(&view, &ui).nodes[0].detail.as_deref(), Some("10 s"));
}

/// The Graph of the spec scenario: a sensor source node, a switch node, a limiter node, a
/// comment node and an end node, with no connection. The limiter is selected.
fn open_inputs() -> (EditorUi, GraphView) {
    let mut graph = Graph::new("graph_1", "Inputs");
    graph.nodes = vec![
        Node::new(1, Kind::SensorSource)
            .with_param("sensor", "x")
            .with_label("cpu"),
        Node::new(2, Kind::Switch),
        Node::new(3, Kind::Limiter),
        Node::new(4, Kind::Comment),
        Node::new(5, Kind::SensorEnd),
    ];
    let (mut ui, view) = open(spread(graph));
    ui.selected = Some(Selected::Node(3));
    (ui, view)
}

/// The rows of the section "Inputs" of the panel.
fn inputs_of(view: &GraphView, ui: &EditorUi) -> Vec<InputRow> {
    panel_of(view, ui).inputs
}

#[test]
fn inputs_section_has_one_select_per_input_pin() {
    let (mut ui, view) = open_inputs();
    let labels = |ui: &EditorUi| -> Vec<String> {
        inputs_of(&view, ui)
            .iter()
            .map(|row| row.label.clone())
            .collect()
    };
    assert_eq!(labels(&ui), ["A"]);
    ui.selected = Some(Selected::Node(2));
    assert_eq!(labels(&ui), ["S", "A", "B"]);
    // A source has no input pin, and a comment has no pin at all.
    ui.selected = Some(Selected::Node(1));
    assert!(labels(&ui).is_empty());
    ui.selected = Some(Selected::Node(4));
    assert!(labels(&ui).is_empty());
    // The pin of an end node has a select too. It has no default.
    ui.selected = Some(Selected::Node(5));
    let rows = inputs_of(&view, &ui);
    assert_eq!(rows.len(), 1);
    assert_eq!((rows[0].pin, rows[0].shown.as_str()), ("a", "None"));

    // A pin without a connection shows "None" and its default.
    let counter = ui.draft.add_node(Kind::Counter, 0.0, 300.0);
    ui.selected = Some(Selected::Node(counter));
    let rows = inputs_of(&view, &ui);
    let shown: Vec<(&str, &str)> = rows
        .iter()
        .map(|row| (row.label.as_str(), row.shown.as_str()))
        .collect();
    assert_eq!(
        shown,
        [
            ("Count", "None, 0"),
            ("Enable", "None, 1"),
            ("Reset", "None, 0"),
            ("Period", "None, 10")
        ]
    );
    let logarithm = ui.draft.add_node(Kind::Logarithm, 0.0, 500.0);
    ui.selected = Some(Selected::Node(logarithm));
    assert_eq!(inputs_of(&view, &ui)[1].shown, "None, e");
}

#[test]
fn inputs_select_lists_none_and_each_other_node_with_an_output() {
    let (mut ui, view) = open_inputs();
    let rows = inputs_of(&view, &ui);
    assert_eq!(rows.len(), 1);
    // "None", the sensor source node and the switch node: not the limiter itself, not the
    // comment node and not the end node.
    assert_eq!(
        rows[0].choices,
        [
            (String::new(), "None, 0".to_owned(), true),
            ("1".to_owned(), "Sensor cpu".to_owned(), false),
            ("2".to_owned(), "Switch".to_owned(), false),
        ]
    );
    assert_eq!(rows[0].shown, "None, 0");

    // An entry has the title of the node: the name of its kind with its main parameter.
    let delay = ui.draft.add_node(Kind::RiseDelay, 0.0, 300.0);
    ui.draft.set_param(delay, "delay_s", json!(10));
    let texts: Vec<String> = inputs_of(&view, &ui)[0]
        .choices
        .iter()
        .map(|(_, text, _)| text.clone())
        .collect();
    assert_eq!(texts, ["None, 0", "Sensor cpu", "Switch", "On delay 10 s"]);
}

#[test]
fn inputs_choice_connects_as_one_undo_step() {
    let (mut ui, view) = open_inputs();
    assert!(ui.choose_input(3, "a", Some(2), NOW_US));
    assert_eq!(ui.draft.graph().connections, [Connection::new(2, 3, "a")]);
    assert_eq!(ui.draft.undo_steps(), 1);
    let row = &inputs_of(&view, &ui)[0];
    assert_eq!(row.shown, "Switch");
    assert!(row.choices[2].2);
    assert!(model(&view, &ui).nodes[2].pins[0].connected);

    // One Ctrl+Z removes the connection.
    assert!(ui.press(EditorKey::Undo));
    assert!(ui.draft.graph().connections.is_empty());
    assert_eq!(inputs_of(&view, &ui)[0].shown, "None, 0");
}

#[test]
fn inputs_choice_replaces_the_connection_as_one_undo_step() {
    let (mut ui, view) = open_inputs();
    ui.choose_input(3, "a", Some(2), NOW_US);
    assert!(ui.choose_input(3, "a", Some(1), NOW_US));
    // The pin A has only the connection from the sensor source node.
    assert_eq!(ui.draft.graph().connections, [Connection::new(1, 3, "a")]);
    assert_eq!(ui.draft.undo_steps(), 2);
    assert_eq!(inputs_of(&view, &ui)[0].shown, "Sensor cpu");

    // One Ctrl+Z gives back the connection from the switch.
    assert!(ui.press(EditorKey::Undo));
    assert_eq!(ui.draft.graph().connections, [Connection::new(2, 3, "a")]);
    // The choice of the entry that the pin has is no step.
    assert!(!ui.choose_input(3, "a", Some(2), NOW_US));
    assert_eq!(ui.draft.undo_steps(), 1);
}

#[test]
fn inputs_choice_that_closes_a_loop_is_refused_with_the_reason() {
    let (mut ui, view) = open_inputs();
    // The limiter takes its input from the switch.
    ui.choose_input(3, "a", Some(2), NOW_US);
    let before = ui.draft.graph().clone();

    // The limiter in the select of the pin A of the switch closes a loop.
    assert!(!ui.choose_input(2, "a", Some(3), NOW_US));
    assert_eq!(ui.draft.graph(), &before);
    assert_eq!(ui.draft.undo_steps(), 1);
    assert_eq!(
        model(&view, &ui).status.text.as_deref(),
        Some("Not connected: the connection closes a loop")
    );
    // The select keeps its earlier entry, and the reason goes after 5 s.
    ui.selected = Some(Selected::Node(2));
    assert_eq!(inputs_of(&view, &ui)[1].shown, "None, 0");
    let later = model_at(&online(), &view, &ui, NOW_US + 5_000_000);
    assert_eq!(later.status.text, None);

    // A node cannot take its own output, and an end node gives none.
    assert!(!ui.choose_input(2, "a", Some(2), NOW_US));
    assert!(!ui.choose_input(2, "a", Some(5), NOW_US));
    assert!(!ui.choose_input(2, "a", Some(4), NOW_US));
    assert!(!ui.choose_input(2, "x", Some(1), NOW_US));
    assert_eq!(ui.draft.graph(), &before);
}

#[test]
fn inputs_none_removes_the_connection() {
    let (mut ui, view) = open_inputs();
    ui.choose_input(3, "a", Some(2), NOW_US);
    assert!(ui.choose_input(3, "a", None, NOW_US));
    // The limiter takes the default 0 on the pin.
    assert_eq!(ui.draft.graph().connection_to(3, "a"), None);
    assert_eq!(inputs_of(&view, &ui)[0].shown, "None, 0");
    assert!(inputs_of(&view, &ui)[0].choices[0].2);
    assert_eq!(ui.draft.undo_steps(), 2);
    assert!(ui.undo());
    assert_eq!(ui.draft.graph().connections, [Connection::new(2, 3, "a")]);
    // "None" on a pin without a connection is no step.
    let (mut ui, _view) = open_inputs();
    assert!(!ui.choose_input(3, "a", None, NOW_US));
    assert_eq!(ui.draft.undo_steps(), 0);
}

#[test]
fn inputs_of_an_end_node_connect_the_same_way() {
    let (mut ui, view) = open_inputs();
    // With no selection the panel shows the end node.
    ui.selected = None;
    let rows = inputs_of(&view, &ui);
    assert_eq!(panel_of(&view, &ui).node, 5);
    let texts: Vec<&str> = rows[0].choices.iter().map(|c| c.1.as_str()).collect();
    assert_eq!(
        texts,
        ["None", "Sensor cpu", "Switch", "Limiter -100 to 100"]
    );

    assert!(ui.choose_input(5, "a", Some(3), NOW_US));
    assert_eq!(ui.draft.graph().connections, [Connection::new(3, 5, "a")]);
    assert_eq!(inputs_of(&view, &ui)[0].shown, "Limiter -100 to 100");
    assert_eq!(ui.draft.undo_steps(), 1);
    assert!(ui.press(EditorKey::Undo));
    assert!(ui.draft.graph().connections.is_empty());
}

#[test]
fn inputs_select_works_with_the_keyboard() {
    let (mut ui, view) = open_inputs();
    let row = inputs_of(&view, &ui).remove(0);
    // The select opens with the mark on its current entry: "None".
    let mut mark = row.choices.iter().position(|c| c.2).expect("the entry");
    assert_eq!(mark, 0);
    // Down moves the mark to the sensor source node, then to the switch node. It stops at the
    // last entry.
    for _ in 0..2 {
        mark = (mark + 1).min(row.choices.len() - 1);
    }
    assert_eq!(row.choices[mark].1, "Switch");
    // Enter picks the marked entry: the value of an entry is the id of its node.
    let from = row.choices[mark].0.parse::<u32>().ok();
    assert_eq!(from, Some(2));
    assert!(ui.choose_input(3, row.pin, from, NOW_US));
    assert_eq!(ui.draft.graph().connections, [Connection::new(2, 3, "a")]);

    // The entry "None" has an empty value: Enter on it removes the connection.
    let row = inputs_of(&view, &ui).remove(0);
    assert_eq!(row.choices[0].0.parse::<u32>().ok(), None);
    assert!(ui.choose_input(3, row.pin, None, NOW_US));
    assert!(ui.draft.graph().connections.is_empty());
}

struct NoScan;

impl ProcessSource for NoScan {
    fn scan(&self) -> ProcessList {
        ProcessList::default()
    }
}

/// A daemon that makes, renames and removes Graph sensors, and that can refuse each call.
#[derive(Clone, Default)]
struct FakeLink {
    /// The values of each pass, as the service sends them to the daemon.
    values: Arc<Mutex<Vec<Vec<(String, Option<f64>)>>>>,
    /// The name and the unit of each sensor that the service asked for.
    created: Arc<Mutex<Vec<(String, String)>>>,
    /// The id, the name and the unit of each definition that the service set.
    renamed: Arc<Mutex<Vec<(String, String, String)>>>,
    deleted: Arc<Mutex<Vec<String>>>,
    /// The answer to a create, a set definition and a delete. `None`: the call works.
    no_create: Arc<Mutex<Option<SensorError>>>,
    no_rename: Arc<Mutex<Option<SensorError>>>,
    no_delete: Arc<Mutex<Option<SensorError>>>,
}

impl SensorLink for FakeLink {
    fn values(&mut self, values: &[(String, Option<f64>)]) {
        self.values.lock().unwrap().push(values.to_vec());
    }

    fn create_sensor(&mut self, name: &str, unit: &str) -> Result<String, SensorError> {
        match self.no_create.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => {
                let mut created = self.created.lock().unwrap();
                created.push((name.to_owned(), unit.to_owned()));
                Ok(format!("graph_{}", 8 + created.len()))
            }
        }
    }

    fn rename_sensor(&mut self, id: &str, name: &str, unit: &str) -> Result<(), SensorError> {
        match self.no_rename.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => {
                let call = (id.to_owned(), name.to_owned(), unit.to_owned());
                self.renamed.lock().unwrap().push(call);
                Ok(())
            }
        }
    }

    fn delete_sensor(&mut self, id: &str) -> Result<(), SensorError> {
        match self.no_delete.lock().unwrap().clone() {
            Some(error) => Err(error),
            None => {
                self.deleted.lock().unwrap().push(id.to_owned());
                Ok(())
            }
        }
    }
}

/// The Graph service with a simulated clock and the fake daemon link.
struct Rig {
    service: GraphService,
    clock: SimClock,
    view: SharedView,
    link: FakeLink,
    _dir: Dir,
    _notifications: UnboundedReceiver<Plan>,
}

impl Rig {
    fn new(name: &str, graphs: Vec<Graph>) -> Self {
        let dir = Dir::new(name);
        let mut store = GraphStore::open(dir.graph_paths());
        for graph in graphs {
            store.put(graph).unwrap();
        }
        let clock = SimClock::default();
        let view = shared_view();
        let link = FakeLink::default();
        let (notify, notifications) = unbounded_channel();
        let service = GraphService::new(
            store,
            Env {
                clock: Box::new(clock.clone()),
                link: Box::new(link.clone()),
                scan: Box::new(NoScan),
                power: Box::new(NoPower),
                notify,
                locale: EN,
                view: view.clone(),
            },
        )
        .with_wall_clock(Box::new(SimWallClock::new(NOW_US)));
        let mut rig = Self {
            service,
            clock,
            view,
            link,
            _dir: dir,
            _notifications: notifications,
        };
        rig.service.apply_event(&DaemonEvent::Link(Link::Online));
        rig
    }

    /// The sensor `x` of the daemon has the value `value`.
    fn feed(&mut self, value: f64) {
        self.service
            .apply_event(&DaemonEvent::Sensors(SensorSample {
                time_us: 1,
                sensors: vec![("x".into(), value, 0)],
                channels: Vec::new(),
            }));
    }

    fn ticks(&mut self, seconds: u32) {
        for _ in 0..seconds {
            self.clock.advance(Duration::from_secs(1));
            self.service.poll();
        }
    }

    /// Give a command of the editor to the service, as the app does.
    fn run(&mut self, command: Command) {
        match command {
            Command::Graph(command) => self.service.handle(command),
            other => panic!("not a Graph command: {other:?}"),
        }
    }

    /// What the service shows now.
    fn shown(&self) -> GraphView {
        self.view.lock().unwrap().clone()
    }

    /// The editor on the stored Graph `id`.
    fn open(&self, id: &str) -> EditorUi {
        let view = self.shown();
        let graph = view.row(id).and_then(GraphRow::graph).expect("the Graph");
        EditorUi::open(graph.clone(), &view)
    }
}

/// The daemon state of the tests of the live values: online, with the sensor `x` in degrees
/// Celsius.
fn state_with_x() -> AppState {
    let mut state = online();
    state.snapshot.sensors.insert(
        "x".to_owned(),
        sensor(
            "x",
            "Water temperature",
            "temperature",
            "degC",
            "hwmon",
            "nct6798",
        ),
    );
    state
}

/// The value and the unit that each node shows, in the order of the draft.
fn values_of(shown: &EditorModel) -> Vec<(String, Option<String>)> {
    shown
        .nodes
        .iter()
        .map(|face| (face.value.clone(), face.unit.clone()))
        .collect()
}

fn text(value: &str, unit: Option<&str>) -> (String, Option<String>) {
    (value.to_owned(), unit.map(str::to_owned))
}

#[test]
fn value_shows_within_2s() {
    let mut rig = Rig::new("editor-live", vec![spread(limiter_graph("graph_1"))]);
    rig.ticks(1);
    let state = state_with_x();
    let mut ui = rig.open("graph_1");
    let view = rig.shown();

    // Loading: the nodes show "--" before the first values arrive.
    let shown = model_with(&state, &view, &ui);
    assert_eq!(shown.nodes[0].value, "--");
    assert!(!ui.live.loaded);

    // The editor opens: the first read is at once. The sensor has no value yet.
    assert!(ui.live.poll("graph_1", NOW_US, &rig.view));
    assert!(ui.live.loaded);
    let dashes = [
        text("--", Some("°C")),
        text("--", None),
        text("--", Some("°C")),
    ];
    assert_eq!(values_of(&model_with(&state, &view, &ui)), dashes);

    // The sensor gives 46.2. The pass of the next second has the value, and the read of that
    // second shows it: 1 s after the node gave it.
    rig.feed(46.2);
    rig.ticks(1);
    assert!(!ui.live.poll("graph_1", NOW_US + 900_000, &rig.view));
    assert_eq!(values_of(&model_with(&state, &view, &ui)), dashes);
    assert!(ui.live.poll("graph_1", NOW_US + 1_000_000, &rig.view));
    let shown = model_at(&state, &view, &ui, NOW_US + 1_000_000);
    assert_eq!(
        values_of(&shown),
        [
            text("46.2", Some("°C")),
            text("46.2", None),
            text("46.2", Some("°C"))
        ]
    );
    assert!(shown.nodes.iter().all(|face| !face.dim && !face.draft_only));
    // The value that the node shows is the value that the service sends to the daemon.
    let sent = rig.link.values.lock().unwrap().last().cloned();
    assert_eq!(sent, Some(vec![("graph_3".to_owned(), Some(46.2))]));
    // The band of the Limiter has the value of its input A, on the node and in the panel.
    assert_eq!(shown.nodes[1].band.unwrap().input, Some(46.2));
    ui.selected = Some(Selected::Node(2));
    match panel_of(&view, &ui).guide {
        Some(Guide::Band { band, input, .. }) => {
            assert_eq!(band.input, Some(46.2));
            assert_eq!(input.as_deref(), Some("A 46.2"));
        }
        other => panic!("no band guide: {other:?}"),
    }

    // A value shows 4 significant digits at most, with the decimal mark of the language.
    for (value, shown) in [
        (46.2, "46.2"),
        (182.0, "182"),
        (1234.56, "1235"),
        (0.5, "0.5"),
        (-3.0, "-3"),
        (0.0, "0"),
    ] {
        assert_eq!(canvas::value_text(EN, value), shown);
    }
    assert_eq!(canvas::value_text(IT, 46.2), "46,2");
    assert_eq!(canvas::value_text(EN, f64::NAN), "--");
}

#[test]
fn no_value_shows_dashes() {
    // A comparison after the limiter: a node after a node with no value has no value too.
    let mut graph = spread(limiter_graph("graph_1"));
    graph.nodes.push(Node::new(4, Kind::Greater));
    graph.connections.push(Connection::new(2, 4, "a"));
    let mut rig = Rig::new("editor-no-value", vec![graph]);
    let state = state_with_x();
    let mut ui = rig.open("graph_1");
    let view = rig.shown();

    // The source has no value: no node has one, and no logic node says "true" or "false".
    rig.ticks(1);
    ui.live.poll("graph_1", NOW_US, &rig.view);
    let shown = model_with(&state, &view, &ui);
    assert!(shown.nodes.iter().all(|face| face.value == "--"));
    assert_eq!(shown.nodes[3].unit, None);
    assert_eq!(shown.nodes[1].band.unwrap().input, None);
    assert_eq!(shown.status.text, None);

    // With a value the comparison shows 1 and "true".
    rig.feed(46.2);
    rig.ticks(1);
    ui.live.poll("graph_1", NOW_US + 1_000_000, &rig.view);
    let shown = model_with(&state, &view, &ui);
    assert_eq!(values_of(&shown)[3], text("1", Some("true")));

    // The daemon is offline: the values are dim.
    let mut offline = state_with_x();
    offline.link = Link::Offline;
    assert!(
        model_with(&offline, &view, &ui)
            .nodes
            .iter()
            .all(|face| face.dim)
    );

    // A disabled Graph is not evaluated: every node shows "--", and the status line says why.
    rig.run(Command::Graph(GraphCommand::Enable {
        graph: "graph_1".to_owned(),
        enabled: false,
    }));
    rig.ticks(1);
    ui.live.poll("graph_1", NOW_US + 2_000_000, &rig.view);
    let shown = model_with(&state, &view, &ui);
    assert!(shown.nodes.iter().all(|face| face.value == "--"));
    assert_eq!(
        shown.status.text.as_deref(),
        Some("The Graph is disabled. Turn it on in the list.")
    );
    assert_eq!(shown.status.retry, None);
}

#[test]
fn draft_only_node_shows_dashes_until_apply() {
    let mut rig = Rig::new("editor-draft-only", vec![constant_graph("graph_1", 5.0)]);
    rig.ticks(1);
    let state = online();
    let mut ui = rig.open("graph_1");
    ui.live.poll("graph_1", NOW_US, &rig.view);

    // A new node exists only in the draft: a dashed outline and no value.
    let id = ui.draft.add_node(Kind::Constant, 0.0, 200.0);
    ui.draft.set_param(id, "value", json!(7));
    let view = rig.shown();
    let shown = model_with(&state, &view, &ui);
    let face = shown.nodes.iter().find(|face| face.id == id).unwrap();
    assert!(face.draft_only);
    assert_eq!(face.value, "--");
    // The nodes of the applied Graph keep their values.
    assert_eq!(shown.nodes[0].value, "5");
    assert!(!shown.nodes[0].draft_only);
    // A changed parameter of an applied node still shows the value of the applied Graph.
    ui.draft.set_param(1, "value", json!(6));
    assert_eq!(model_with(&state, &view, &ui).nodes[0].value, "5");
    let shape = shape_of(&shown.nodes, &shown.edges);
    assert!(
        shape
            .nodes
            .iter()
            .find(|node| node.id == id)
            .unwrap()
            .dashed
    );

    // A read does not give the new node a value: the service does not know it.
    rig.ticks(1);
    ui.live.poll("graph_1", NOW_US + 1_000_000, &rig.view);
    let shown = model_with(&state, &rig.shown(), &ui);
    assert_eq!(shown.nodes.iter().find(|f| f.id == id).unwrap().value, "--");

    // Apply stores the draft. The node gets a solid outline and its value.
    rig.run(Command::Graph(GraphCommand::Apply(Box::new(
        ui.draft.candidate(),
    ))));
    rig.ticks(1);
    let view = rig.shown();
    ui.draft
        .set_stored(view.row("graph_1").and_then(GraphRow::graph).cloned());
    ui.live.poll("graph_1", NOW_US + 2_000_000, &rig.view);
    let shown = model_with(&state, &view, &ui);
    let face = shown.nodes.iter().find(|face| face.id == id).unwrap();
    assert!(!face.draft_only);
    assert_eq!(face.value, "7");
    assert_eq!(shown.nodes[0].value, "6");
}

/// A source of node values that counts its reads.
struct Counting {
    view: SharedView,
    reads: Cell<u32>,
}

impl ValueSource for Counting {
    fn read(&self) -> Option<GraphView> {
        self.reads.set(self.reads.get() + 1);
        self.view.read()
    }
}

#[test]
fn no_reads_after_close() {
    let mut rig = Rig::new("editor-close", vec![constant_graph("graph_1", 5.0)]);
    rig.ticks(1);
    let mut ui = rig.open("graph_1");
    let source = Counting {
        view: rig.view.clone(),
        reads: Cell::new(0),
    };
    assert!(ui.live.poll("graph_1", NOW_US, &source));
    assert!(ui.live.poll("graph_1", NOW_US + 1_000_000, &source));
    assert_eq!(source.reads.get(), 2);

    // The editor closes: no read follows, however long the time.
    ui.live.close();
    for seconds in 2..=30 {
        assert!(
            !ui.live
                .poll("graph_1", NOW_US + seconds * 1_000_000, &source)
        );
    }
    assert_eq!(source.reads.get(), 2);
    assert_eq!(ui.live.reads(), 2);
}

#[test]
fn reads_once_per_second() {
    let mut rig = Rig::new("editor-reads", vec![constant_graph("graph_1", 5.0)]);
    rig.ticks(1);
    let mut ui = rig.open("graph_1");
    let source = Counting {
        view: rig.view.clone(),
        reads: Cell::new(0),
    };
    // A frame every 100 ms for 5 s: one read at the start, then one per second.
    for frame in 0..=50 {
        let read = ui.live.poll("graph_1", NOW_US + frame * 100_000, &source);
        assert_eq!(read, frame % 10 == 0, "frame {frame}");
    }
    assert_eq!(source.reads.get(), 6);
    assert_eq!(ui.live.reads(), 6);
    assert!(!ui.live.poll("graph_1", NOW_US + 5_999_999, &source));
    assert!(ui.live.poll("graph_1", NOW_US + 6_000_000, &source));

    // "Retry" reads at the next frame.
    ui.live.retry();
    assert!(ui.live.poll("graph_1", NOW_US + 6_000_001, &source));
    assert_eq!(source.reads.get(), 8);
}

/// A Graph whose notification response fires while the sensor `x` is above 0.
fn response_graph(id: &str) -> Graph {
    let mut graph = Graph::new(id, "Water too warm");
    graph.nodes = vec![
        Node::new(1, Kind::SensorSource).with_param("sensor", "x"),
        Node::new(2, Kind::ResponseEnd)
            .with_param("kind", "notification")
            .with_param("title", "Hot"),
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    spread(graph)
}

/// The service with the limiter Graph, the sensor `x` at 46.2, and the editor on the Graph after
/// its first read. The editor gives the source the test value 80 for `seconds`.
fn try_rig(name: &str, seconds: &str) -> (Rig, EditorUi) {
    let mut rig = Rig::new(name, vec![spread(limiter_graph("graph_1"))]);
    rig.feed(46.2);
    rig.ticks(1);
    let mut ui = rig.open("graph_1");
    assert!(ui.live.poll("graph_1", NOW_US, &rig.view));
    assert!(ui.commit_field(&test_key(1), "80", EN));
    assert!(ui.commit_field(&for_key(1), seconds, EN));
    (rig, ui)
}

/// The values that the nodes show, without their units.
fn numbers_of(shown: &EditorModel) -> Vec<&str> {
    shown.nodes.iter().map(|face| face.value.as_str()).collect()
}

#[test]
fn try_with_80_for_30s_shows_the_banner() {
    let mut rig = Rig::new("editor-try", vec![spread(limiter_graph("graph_1"))]);
    rig.feed(46.2);
    rig.ticks(1);
    let state = state_with_x();
    let mut ui = rig.open("graph_1");
    ui.live.poll("graph_1", NOW_US, &rig.view);
    let view = rig.shown();

    // A Sensor node has the fields "Test value", empty at first, and "For", 60 s at first.
    ui.selected = Some(Selected::Node(1));
    let tests = panel_of(&view, &ui).tests.expect("the test fields");
    assert_eq!(
        (
            tests.title.as_str(),
            tests.value_label.as_str(),
            tests.value.as_str()
        ),
        ("Try", "Test value", "")
    );
    assert_eq!(
        (
            tests.seconds_label.as_str(),
            tests.seconds.as_str(),
            tests.unit.as_str()
        ),
        ("For", "60", "s")
    );
    ui.selected = Some(Selected::Node(2));
    assert_eq!(
        panel_of(&view, &ui).tests,
        None,
        "a Limiter has no test value"
    );

    // The source gets 80 for 30 s. The time is 1 to 600 s.
    ui.selected = Some(Selected::Node(1));
    assert!(ui.commit_field(&test_key(1), "80", EN));
    assert!(!ui.commit_field(&test_key(1), "warm", EN));
    for text in ["0", "601", "1.5"] {
        assert!(!ui.commit_field(&for_key(1), text, EN), "{text}");
        let tests = panel_of(&view, &ui).tests.unwrap();
        assert_eq!(tests.error.as_deref(), Some("1 to 600 s"));
    }
    assert!(ui.commit_field(&for_key(1), "30", EN));
    let tests = panel_of(&view, &ui).tests.unwrap();
    assert_eq!((tests.value.as_str(), tests.seconds.as_str()), ("80", "30"));
    assert_eq!(tests.error, None);
    // A test value is no part of the draft, and the node has the tag "Test".
    assert!(!ui.draft.is_dirty());
    let shown = model_with(&state, &view, &ui);
    assert_eq!(shown.nodes[0].tag.as_deref(), Some("Test"));
    assert_eq!(shown.nodes[1].tag, None);
    assert_eq!(shown.try_banner, None);
    assert!(shown.toolbar.try_run.enabled);

    // A node that exists only in the draft has a value in a try.
    let extra = ui.draft.add_node(Kind::Constant, 0.0, 300.0);
    ui.draft.set_param(extra, "value", json!(3));

    // "Try" sends the draft with the test value and the time.
    let command = ui.start_try().expect("the command");
    assert_eq!(
        command,
        Command::Graph(GraphCommand::Try(Box::new(TryRequest {
            graph: ui.draft.candidate(),
            tests: vec![(1, 80.0)],
            seconds: 30,
        })))
    );
    // The banner shows at once, with the whole time, and "Try" is off while the try runs.
    let shown = model_with(&state, &view, &ui);
    let banner = shown.try_banner.expect("the banner");
    assert_eq!(banner.text, "Try running, nothing is published");
    assert_eq!(
        (banner.left.as_str(), banner.stop.as_str()),
        ("30 s left", "Stop")
    );
    assert!(!shown.toolbar.try_run.enabled);
    assert_eq!(ui.start_try(), None);

    // The service evaluates the draft: the nodes show the values of the try.
    rig.run(command);
    assert!(ui.live.poll("graph_1", NOW_US + 1, &rig.view));
    assert_eq!(ui.follow_try(), None);
    let shown = model_with(&state, &view, &ui);
    assert_eq!(numbers_of(&shown), ["80", "60", "60", "3"]);
    assert_eq!(shown.nodes[0].unit.as_deref(), Some("°C"));
    assert!(shown.nodes[3].draft_only);
    assert_eq!(shown.try_banner.unwrap().left, "30 s left");

    // The service counts the time.
    rig.ticks(6);
    assert!(ui.live.poll("graph_1", NOW_US + 6_000_000, &rig.view));
    assert_eq!(ui.follow_try(), None);
    let shown = model_with(&state, &view, &ui);
    assert_eq!(shown.try_banner.unwrap().left, "24 s left");
    assert_eq!(numbers_of(&shown), ["80", "60", "60", "3"]);
}

#[test]
fn stop_returns_to_the_live_values() {
    let (mut rig, mut ui) = try_rig("editor-try-stop", "30");
    let state = state_with_x();
    let view = rig.shown();
    let command = ui.start_try().expect("the command");
    rig.run(command);
    ui.live.poll("graph_1", NOW_US + 1, &rig.view);
    assert_eq!(
        numbers_of(&model_with(&state, &view, &ui)),
        ["80", "60", "60"]
    );

    // "Stop": the banner goes at once, and the nodes show the live values again.
    let stop = ui.stop_try().expect("the command");
    assert_eq!(stop, Command::Graph(GraphCommand::StopTry));
    let shown = model_with(&state, &view, &ui);
    assert_eq!(shown.try_banner, None);
    assert_eq!(numbers_of(&shown), ["46.2", "46.2", "46.2"]);
    rig.run(stop);
    assert_eq!(rig.shown().trying, None);
    assert!(ui.live.poll("graph_1", NOW_US + 2, &rig.view));
    assert_eq!(ui.follow_try(), None);
    let shown = model_with(&state, &view, &ui);
    assert_eq!(numbers_of(&shown), ["46.2", "46.2", "46.2"]);
    assert!(shown.toolbar.try_run.enabled);
    assert_eq!(ui.stop_try(), None, "no try runs");

    // A change of the draft ends a try: the editor stops it.
    let command = ui.start_try().expect("the command");
    rig.run(command);
    ui.live.poll("graph_1", NOW_US + 3, &rig.view);
    assert_eq!(ui.follow_try(), None);
    ui.draft.move_node(2, 300.0, 40.0);
    assert_eq!(ui.follow_try(), Some(Command::Graph(GraphCommand::StopTry)));
    assert_eq!(model_with(&state, &view, &ui).try_banner, None);
}

#[test]
fn banner_goes_after_the_time() {
    let (mut rig, mut ui) = try_rig("editor-try-time", "2");
    let state = state_with_x();
    let view = rig.shown();
    let command = ui.start_try().expect("the command");
    rig.run(command);
    ui.live.poll("graph_1", NOW_US + 1, &rig.view);
    assert_eq!(ui.follow_try(), None);
    assert_eq!(
        model_with(&state, &view, &ui).try_banner.unwrap().left,
        "2 s left"
    );

    // The time of the test value is over: the service ends the try.
    rig.ticks(3);
    assert_eq!(rig.shown().trying, None);
    assert!(ui.live.poll("graph_1", NOW_US + 3_000_000, &rig.view));
    assert_eq!(ui.follow_try(), None);
    let shown = model_with(&state, &view, &ui);
    assert_eq!(shown.try_banner, None);
    assert_eq!(numbers_of(&shown), ["46.2", "46.2", "46.2"]);
    assert!(shown.toolbar.try_run.enabled);

    // A try without a test value lasts 60 s.
    assert!(ui.commit_field(&test_key(1), "", EN));
    match ui.start_try() {
        Some(Command::Graph(GraphCommand::Try(request))) => {
            assert!(request.tests.is_empty());
            assert_eq!(request.seconds, 60);
        }
        other => panic!("no try: {other:?}"),
    }
    // The service does not start this try: after two reads the banner goes.
    assert!(model_with(&state, &view, &ui).try_banner.is_some());
    ui.live.poll("graph_1", NOW_US + 4_000_000, &rig.view);
    assert_eq!(ui.follow_try(), None);
    assert!(model_with(&state, &view, &ui).try_banner.is_some());
    ui.live.poll("graph_1", NOW_US + 5_000_000, &rig.view);
    assert_eq!(ui.follow_try(), None);
    assert_eq!(model_with(&state, &view, &ui).try_banner, None);
}

#[test]
fn try_does_not_change_the_values_sent() {
    let (mut rig, mut ui) = try_rig("editor-try-values", "30");
    let command = ui.start_try().expect("the command");
    rig.run(command);
    rig.ticks(3);
    // The try gives 60. The Graph sensor keeps the value of the applied Graph in every pass.
    assert_eq!(
        rig.shown().trying.map(|trying| trying.values[&3]),
        Some(Some(60.0))
    );
    let sent = rig.link.values.lock().unwrap().clone();
    assert_eq!(sent.len(), 4);
    for pass in &sent {
        assert_eq!(pass, &vec![("graph_3".to_owned(), Some(46.2))]);
    }
    let row = rig.shown().row("graph_1").cloned().expect("the row");
    assert_eq!(row.end, EndValue::Sensor(Some(46.2)));
    assert!(rig.link.created.lock().unwrap().is_empty());

    // A try of a Graph with a response end node runs no response.
    let mut rig = Rig::new("editor-try-response", vec![response_graph("graph_2")]);
    rig.feed(0.0);
    rig.ticks(1);
    let state = state_with_x();
    let mut ui = rig.open("graph_2");
    ui.live.poll("graph_2", NOW_US, &rig.view);
    let view = rig.shown();
    assert_eq!(
        model_with(&state, &view, &ui).nodes[1].state.as_deref(),
        Some("Not fired")
    );
    assert!(ui.commit_field(&test_key(1), "1", EN));
    let command = ui.start_try().expect("the command");
    rig.run(command);
    rig.ticks(3);
    ui.live.poll("graph_2", NOW_US + 3_000_000, &rig.view);
    let shown = model_with(&state, &view, &ui);
    // The end node shows the input of the try, and says that the response does not run.
    assert_eq!(values_of(&shown)[1], text("1", Some("true")));
    assert_eq!(shown.nodes[1].state.as_deref(), Some("Not run in a try"));
    assert!(rig.shown().log.is_empty(), "no response ran");
    assert_eq!(rig.shown().row("graph_2").unwrap().run, None);
}
