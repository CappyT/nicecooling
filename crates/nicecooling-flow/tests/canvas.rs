//! Tests of the canvas logic. They need no window.

use nicecooling_flow::{
    ConnectOutcome, Connection, FlowEdge, FlowNode, FlowState, HandleDef, HandlePosition,
    HandleType, MAX_ZOOM, MIN_ZOOM,
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

fn occupied(connection: &Connection, state: &FlowState) -> bool {
    !state.edges.iter().any(|edge: &FlowEdge| {
        edge.target == connection.target && edge.target_handle == connection.target_handle
    })
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
        ConnectOutcome::Refused(_)
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
