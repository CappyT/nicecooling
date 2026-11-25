//! Tests of the canvas logic. They need no window.

use gpui::SharedString;
use nicecooling_flow::{
    ALREADY_CONNECTED, ConnectOutcome, Connection, FlowEdge, FlowEvent, FlowNode, FlowPoint,
    FlowState, HandleDef, HandlePosition, HandleType, MAX_ZOOM, MIN_ZOOM, SAME_NODE, Selection,
};

/// `src` has the output pin `out`. `dst` has the input pins `a` and `b`.
fn pins() -> FlowState {
    let src = FlowNode::new("src", 0.0, 0.0)
        .size(100.0, 60.0)
        .handles(vec![
            HandleDef::source(HandlePosition::Right)
                .id("out")
                .offset(30.0),
        ]);
    let src2 = FlowNode::new("src2", 0.0, 200.0)
        .size(100.0, 60.0)
        .handles(vec![
            HandleDef::source(HandlePosition::Right)
                .id("out")
                .offset(30.0),
        ]);
    let dst = FlowNode::new("dst", 300.0, 0.0)
        .size(100.0, 60.0)
        .handles(vec![
            HandleDef::target(HandlePosition::Left).id("a").offset(20.0),
            HandleDef::target(HandlePosition::Left).id("b").offset(40.0),
        ]);
    FlowState::new(vec![src, src2, dst], Vec::new())
}

/// Drag from the output pin of `from` to the input pin `pin` of `dst`, then release.
fn connect(state: &mut FlowState, from: &str, pin: &str) -> ConnectOutcome {
    let from_y = if from == "src" { 30.0 } else { 230.0 };
    state.begin_connection(
        &from.to_string().into(),
        &Some("out".into()),
        HandleType::Source,
        HandlePosition::Right,
        (100.0, from_y),
    );
    let to_y = if pin == "a" { 20.0 } else { 40.0 };
    assert!(state.drag_connection((300.0, to_y)));
    state.end_connection()
}

#[test]
fn connect_to_a_free_pin() {
    let mut state = pins();
    let outcome = connect(&mut state, "src", "a");
    assert_eq!(
        outcome,
        ConnectOutcome::Added(Connection {
            source: "src".into(),
            target: "dst".into(),
            source_handle: Some("out".into()),
            target_handle: Some("a".into()),
        })
    );
    assert_eq!(state.edges.len(), 1);
    assert!(state.connecting.is_none());
}

pub const INPUT_OCCUPIED: &str = "input already connected";

fn occupied(connection: &Connection, state: &FlowState) -> Result<(), SharedString> {
    let used = state.edges.iter().any(|edge: &FlowEdge| {
        edge.target == connection.target && edge.target_handle == connection.target_handle
    });
    if used {
        Err(INPUT_OCCUPIED.into())
    } else {
        Ok(())
    }
}

#[test]
fn refuse_an_occupied_pin() {
    let mut state = pins().with_rule(occupied);
    assert!(matches!(
        connect(&mut state, "src", "a"),
        ConnectOutcome::Added(_)
    ));
    assert!(matches!(
        connect(&mut state, "src2", "a"),
        ConnectOutcome::Refused { .. }
    ));
    assert_eq!(state.edges.len(), 1);
    // The pin b is still free.
    assert!(matches!(
        connect(&mut state, "src2", "b"),
        ConnectOutcome::Added(_)
    ));
    assert_eq!(state.edges.len(), 2);
}

#[test]
fn zoom_stays_between_25_and_400_percent() {
    let mut state = pins();
    assert_eq!((state.min_zoom, state.max_zoom), (MIN_ZOOM, MAX_ZOOM));
    for _ in 0..40 {
        state.zoom_out(600.0, 400.0);
    }
    assert_eq!(state.viewport.zoom, 0.25);
    for _ in 0..40 {
        state.zoom_in(600.0, 400.0);
    }
    assert_eq!(state.viewport.zoom, 4.0);
    state.zoom_steps_at((10.0, 10.0), -100.0);
    assert_eq!(state.viewport.zoom, 0.25);
}

#[test]
fn zoom_keeps_the_point_under_the_pointer() {
    let mut state = pins();
    state.viewport.x = 30.0;
    state.viewport.y = -20.0;
    let focus = (200.0, 150.0);
    let before = state.viewport.screen_to_flow(focus.0, focus.1);
    state.zoom_at(focus, 2.0);
    let after = state.viewport.screen_to_flow(focus.0, focus.1);
    assert!((before.x - after.x).abs() < 1e-3 && (before.y - after.y).abs() < 1e-3);
}

#[test]
fn pan_moves_the_view() {
    let mut state = pins();
    state.start_pan((100.0, 100.0));
    assert!(state.drag_pan((40.0, 150.0)));
    assert_eq!((state.viewport.x, state.viewport.y), (-60.0, 50.0));
    state.end_pan();
    assert!(!state.drag_pan((0.0, 0.0)));
    assert_eq!((state.viewport.x, state.viewport.y), (-60.0, 50.0));
}

#[test]
fn a_release_on_an_occupied_pin_gives_the_reason() {
    let mut state = pins().with_rule(occupied);
    connect(&mut state, "src", "a");
    state.take_events();
    let outcome = connect(&mut state, "src2", "a");
    let ConnectOutcome::Refused { connection, reason } = outcome else {
        panic!("expected a refusal, got {outcome:?}");
    };
    assert_eq!(reason, INPUT_OCCUPIED);
    assert_eq!(connection.source, "src2");
    assert_eq!(
        state.take_events(),
        vec![FlowEvent::ConnectionRefused { connection, reason }]
    );
    assert!(state.take_events().is_empty());
}

#[test]
fn the_draft_over_an_occupied_pin_has_the_reason_before_the_release() {
    let mut state = pins().with_rule(occupied);
    connect(&mut state, "src", "a");
    state.begin_connection(
        &"src2".into(),
        &Some("out".into()),
        HandleType::Source,
        HandlePosition::Right,
        (100.0, 230.0),
    );
    state.drag_connection((300.0, 20.0));
    let snap = state
        .connecting
        .as_ref()
        .unwrap()
        .snap_target
        .clone()
        .unwrap();
    assert_eq!(snap.handle_id, Some("a".into()));
    assert_eq!(snap.refusal, Some(INPUT_OCCUPIED.into()));
    // The free pin b has no refusal.
    state.drag_connection((300.0, 40.0));
    let snap = state
        .connecting
        .as_ref()
        .unwrap()
        .snap_target
        .clone()
        .unwrap();
    assert_eq!(snap.handle_id, Some("b".into()));
    assert_eq!(snap.refusal, None);
}

#[test]
fn the_default_rules_give_a_reason() {
    let mut state = pins();
    connect(&mut state, "src", "a");
    let again = Connection {
        source: "src".into(),
        target: "dst".into(),
        source_handle: Some("out".into()),
        target_handle: Some("a".into()),
    };
    assert_eq!(state.check(&again), Err(ALREADY_CONNECTED.into()));
    let to_self = Connection {
        target: "src".into(),
        ..again
    };
    assert_eq!(state.check(&to_self), Err(SAME_NODE.into()));
}

fn node(id: &str) -> Selection {
    Selection::Node(id.to_string().into())
}

#[test]
fn a_drag_sends_one_node_moved_event() {
    let mut state = pins();
    state.press_node(&"src".into(), (10.0, 10.0));
    assert_eq!(
        state.take_events(),
        vec![FlowEvent::SelectionChanged {
            selection: Some(node("src"))
        }]
    );
    for step in 1..=5 {
        let offset = 10.0 * step as f32;
        assert!(state.pointer_moved((10.0 + offset, 10.0 + offset / 2.0)));
    }
    // The moves send nothing: the app learns the end of the drag.
    assert!(state.take_events().is_empty());
    assert!(state.pointer_released());
    assert_eq!(
        state.take_events(),
        vec![FlowEvent::NodeMoved {
            id: "src".into(),
            from: FlowPoint::new(0.0, 0.0),
            to: FlowPoint::new(50.0, 25.0),
        }]
    );
}

#[test]
fn a_click_without_a_move_sends_no_node_moved() {
    let mut state = pins();
    state.press_node(&"src".into(), (10.0, 10.0));
    state.take_events();
    state.pointer_released();
    assert!(state.take_events().is_empty());
}

#[test]
fn a_cancelled_drag_sends_no_node_moved() {
    let mut state = pins();
    state.press_node(&"src".into(), (10.0, 10.0));
    state.take_events();
    state.pointer_moved((60.0, 60.0));
    assert!(state.cancel_drag());
    state.pointer_released();
    assert!(state.take_events().is_empty());
}

#[test]
fn a_connection_sends_connection_added() {
    let mut state = pins();
    connect(&mut state, "src", "a");
    let events = state.take_events();
    let [FlowEvent::ConnectionAdded { id, connection }] = events.as_slice() else {
        panic!("expected one ConnectionAdded, got {events:?}");
    };
    assert_eq!(connection.target_handle, Some("a".into()));
    assert_eq!(state.edges[0].id, *id);
}

#[test]
fn the_canvas_keeps_one_selection() {
    let mut state = pins();
    state.press_node(&"src".into(), (0.0, 0.0));
    state.pointer_released();
    state.press_node(&"dst".into(), (0.0, 0.0));
    state.pointer_released();
    assert_eq!(state.selection(), Some(node("dst")));
    assert_eq!(state.nodes.iter().filter(|node| node.selected).count(), 1);
    assert_eq!(
        state.take_events(),
        vec![
            FlowEvent::SelectionChanged {
                selection: Some(node("src"))
            },
            FlowEvent::SelectionChanged {
                selection: Some(node("dst"))
            },
        ]
    );
    // A press on the empty canvas clears the selection.
    state.press_canvas((500.0, 500.0));
    assert_eq!(state.selection(), None);
    assert_eq!(
        state.take_events(),
        vec![FlowEvent::SelectionChanged { selection: None }]
    );
    // A press on the selected node again changes nothing.
    state.press_node(&"dst".into(), (0.0, 0.0));
    state.take_events();
    state.press_node(&"dst".into(), (0.0, 0.0));
    assert!(state.take_events().is_empty());
}

#[test]
fn delete_removes_a_node_and_its_connections_and_tells_the_app() {
    let mut state = pins();
    connect(&mut state, "src", "a");
    connect(&mut state, "src2", "b");
    state.take_events();
    let id: SharedString = state.edges[0].id.clone();
    state.press_node(&"src".into(), (0.0, 0.0));
    state.pointer_released();
    state.take_events();
    assert!(state.delete_selection());
    assert_eq!(state.nodes.len(), 2);
    assert_eq!(state.edges.len(), 1);
    let events = state.take_events();
    assert!(matches!(
        events.as_slice(),
        [
            FlowEvent::ConnectionRemoved { id: removed, .. },
            FlowEvent::NodeRemoved { id: node },
            FlowEvent::SelectionChanged { selection: None },
        ] if *removed == id && node == "src"
    ));
}

#[test]
fn delete_removes_a_selected_connection() {
    let mut state = pins();
    connect(&mut state, "src", "a");
    let id = state.edges[0].id.clone();
    state.select(Some(Selection::Connection(id.clone())));
    state.take_events();
    assert!(state.delete_selection());
    assert!(state.edges.is_empty());
    assert_eq!(state.nodes.len(), 3);
    assert!(matches!(
        state.take_events().as_slice(),
        [
            FlowEvent::ConnectionRemoved { id: removed, .. },
            FlowEvent::SelectionChanged { selection: None },
        ] if *removed == id
    ));
}

#[test]
fn delete_leaves_a_node_that_is_not_deletable() {
    let mut state = pins();
    state.nodes[2].deletable = false;
    state.select(Some(node("dst")));
    state.take_events();
    assert!(!state.delete_selection());
    assert_eq!(state.nodes.len(), 3);
    assert!(state.take_events().is_empty());
}

#[test]
fn set_graph_sends_no_event_and_keeps_the_measured_sizes() {
    let mut state = pins();
    let mut moved = state.nodes.clone();
    moved[0].position = FlowPoint::new(7.0, 8.0);
    moved[0].measured_width = None;
    moved[0].selected = true;
    state.set_graph(moved, Vec::new());
    assert!(state.take_events().is_empty());
    let first = state.get_node(&"src".into()).unwrap();
    assert_eq!(first.position, FlowPoint::new(7.0, 8.0));
    assert_eq!(first.measured_width, Some(gpui::px(100.0)));
    assert_eq!(state.selection(), Some(node("src")));
    // The new state sets the selection too, with no event.
    state.set_selection(None);
    assert!(state.take_events().is_empty());
}

#[test]
fn tab_selects_the_nodes_in_a_stable_order() {
    let mut state = pins();
    let order = |state: &mut FlowState, backwards| {
        assert!(state.select_next(backwards));
        state.selection()
    };
    assert_eq!(order(&mut state, false), Some(node("src")));
    assert_eq!(order(&mut state, false), Some(node("src2")));
    assert_eq!(order(&mut state, false), Some(node("dst")));
    // The list goes on at its start.
    assert_eq!(order(&mut state, false), Some(node("src")));
    // Shift+Tab goes the other way, also over the start.
    assert_eq!(order(&mut state, true), Some(node("dst")));
    assert_eq!(order(&mut state, true), Some(node("src2")));
    // The same selection from the same state gives the same node.
    state.set_selection(None);
    assert_eq!(order(&mut state, true), Some(node("dst")));
    assert_eq!(state.nodes.iter().filter(|node| node.selected).count(), 1);
}

#[test]
fn tab_skips_a_hidden_node_and_a_node_that_is_not_selectable() {
    let mut state = pins();
    state.nodes[1].hidden = true;
    state.nodes[2].selectable = false;
    assert!(state.select_next(false));
    assert_eq!(state.selection(), Some(node("src")));
    assert!(state.select_next(false));
    assert_eq!(state.selection(), Some(node("src")));
    let mut none = FlowState::new(Vec::new(), Vec::new());
    assert!(!none.select_next(false));
}

#[test]
fn an_arrow_key_moves_the_selected_node_by_one_grid_step() {
    let mut state = pins();
    assert!(!state.nudge_selection((1, 0)), "nothing is selected");
    state.select(Some(node("dst")));
    state.take_events();
    state.snap_grid = (24.0, 24.0);
    assert!(state.nudge_selection((1, 0)));
    assert!(state.nudge_selection((0, -1)));
    assert_eq!(
        state.get_node(&"dst".into()).unwrap().position,
        FlowPoint::new(324.0, -24.0)
    );
    assert_eq!(
        state.take_events(),
        vec![
            FlowEvent::NodeMoved {
                id: "dst".into(),
                from: FlowPoint::new(300.0, 0.0),
                to: FlowPoint::new(324.0, 0.0),
            },
            FlowEvent::NodeMoved {
                id: "dst".into(),
                from: FlowPoint::new(324.0, 0.0),
                to: FlowPoint::new(324.0, -24.0),
            },
        ]
    );
}

#[test]
fn the_delete_guard_can_refuse() {
    let mut state = pins().with_delete_guard(|selection, _| *selection != node("dst"));
    state.select(Some(node("dst")));
    state.take_events();
    assert!(!state.delete_selection());
    assert_eq!(state.nodes.len(), 3);
    assert!(state.take_events().is_empty());
    assert_eq!(state.selection(), Some(node("dst")));
    state.select(Some(node("src")));
    state.take_events();
    assert!(state.delete_selection());
    assert_eq!(state.nodes.len(), 2);
}

#[test]
fn the_zero_key_goes_back_to_100_percent_around_the_center() {
    let mut state = pins();
    state.size = (600.0, 400.0);
    let center = state.viewport.screen_to_flow(300.0, 200.0);
    state.zoom_in(600.0, 400.0);
    state.zoom_in(600.0, 400.0);
    assert!(state.viewport.zoom > 1.4);
    state.reset_zoom();
    assert_eq!(state.viewport.zoom, 1.0);
    let after = state.viewport.screen_to_flow(300.0, 200.0);
    assert!((center.x - after.x).abs() < 1e-3 && (center.y - after.y).abs() < 1e-3);
}
