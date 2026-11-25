//! The Graph editor: the draft, the canvas, the node list, the parameter panel, the live values,
//! the try, Apply and the response panel. The tests read the state and the model of the editor.

mod support;

use std::collections::BTreeMap;

use nicecooling::graphs::view::{GraphRow, GraphView, RowState};
use nicecooling::i18n::EN;
use nicecooling::model::AppState;
use nicecooling::screens::graph_editor::draft::UNDO_LIMIT;
use nicecooling::screens::graph_editor::{EditorKey, EditorModel, EditorUi, Selected, editor_key};
use nicecooling::screens::{Feedback, ScreenInput};
use nicecooling_graph::eval::EndValue;
use nicecooling_graph::model::{Connection, Graph, Kind, Node, StoredGraph};
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

/// The model of the editor with the daemon state `state`.
fn model_with(state: &AppState, view: &GraphView, ui: &EditorUi) -> EditorModel {
    let feedback = Feedback::default();
    EditorModel::build(
        &ScreenInput {
            state,
            now_us: NOW_US,
            locale: EN,
            down_since_us: None,
            feedback: &feedback,
        },
        view,
        ui,
    )
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
