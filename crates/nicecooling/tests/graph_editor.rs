//! The Graph editor: the draft, the canvas, the node list, the parameter panel, the live values,
//! the try, Apply and the response panel. The tests read the state and the model of the editor.

mod support;

use std::collections::BTreeMap;

use gpui::SharedString;
use nicecooling::graphs::view::{GraphRow, GraphView, RowState};
use nicecooling::i18n::EN;
use nicecooling::model::AppState;
use nicecooling::screens::graph_editor::canvas::{self, new_flow_state, shape_of};
use nicecooling::screens::graph_editor::draft::UNDO_LIMIT;
use nicecooling::screens::graph_editor::palette::{ListForm, ListKey};
use nicecooling::screens::graph_editor::{EditorKey, EditorModel, EditorUi, Selected, editor_key};
use nicecooling::screens::{Feedback, ScreenInput};
use nicecooling_flow::{
    ConnectOutcome, FlowNodeId, FlowState, HandlePosition, HandleType, Selection, Viewport,
};
use nicecooling_graph::eval::EndValue;
use nicecooling_graph::model::{Connection, Family, Graph, Kind, Node, StoredGraph};
use serde_json::json;
use support::fixtures::online;
use support::graphs::constant_graph;

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
