//! Tests of the canvas in a window on the GPUI test platform. They need no display.

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    Context, Entity, IntoElement, Modifiers, MouseButton, Render, SharedString, TestAppContext,
    VisualTestContext, Window, div, point, prelude::*, px,
};
use nicecooling_flow::{
    Connection, FlowEdge, FlowEvent, FlowGraph, FlowNode, FlowPoint, FlowState, HandleDef,
    HandlePosition,
};

/// A header above the canvas, as in the editor: the canvas does not start at the window origin.
const HEADER: f32 = 46.0;

struct Host {
    graph: Entity<FlowGraph>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(div().h(px(HEADER)))
            .child(div().w(px(600.0)).h(px(400.0)).child(self.graph.clone()))
    }
}

fn nodes() -> Vec<FlowNode> {
    vec![
        FlowNode::new("a", 100.0, 100.0)
            .label("A")
            .size(100.0, 40.0)
            .handles(vec![HandleDef::source(HandlePosition::Right).id("out")]),
        FlowNode::new("b", 300.0, 100.0)
            .label("B")
            .size(100.0, 40.0)
            .handles(vec![HandleDef::target(HandlePosition::Left).id("in")]),
        FlowNode::new("c", 100.0, 250.0)
            .label("C")
            .size(100.0, 40.0)
            .handles(vec![HandleDef::source(HandlePosition::Right).id("out")]),
    ]
}

/// The pin `in` of `b` has the connection from `c`.
fn edges() -> Vec<FlowEdge> {
    vec![
        FlowEdge::new("e0", "c", "b")
            .source_handle("out")
            .target_handle("in"),
    ]
}

const INPUT_OCCUPIED: &str = "input already connected";

fn occupied(connection: &Connection, state: &FlowState) -> Result<(), SharedString> {
    let used = state.edges.iter().any(|edge| {
        edge.target == connection.target && edge.target_handle == connection.target_handle
    });
    if used {
        Err(INPUT_OCCUPIED.into())
    } else {
        Ok(())
    }
}

fn rig(cx: &mut TestAppContext) -> (Entity<FlowState>, &mut VisualTestContext) {
    let (state, _graph, cx) = rig_with(cx, FlowState::new(nodes(), edges()).with_rule(occupied));
    (state, cx)
}

fn rig_with(
    cx: &mut TestAppContext,
    state: FlowState,
) -> (Entity<FlowState>, Entity<FlowGraph>, &mut VisualTestContext) {
    let state = cx.new(|_| state);
    let graph_state = state.clone();
    let (host, cx) = cx.add_window_view(|_, cx| Host {
        graph: cx.new(|cx| FlowGraph::new(graph_state, cx)),
    });
    let graph = host.read_with(cx, |host, _| host.graph.clone());
    draw(cx);
    (state, graph, cx)
}

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

/// A point of the canvas in window coordinates.
fn at(x: f32, y: f32) -> gpui::Point<gpui::Pixels> {
    point(px(x), px(HEADER + y))
}

fn node_position(state: &Entity<FlowState>, cx: &VisualTestContext, id: &str) -> FlowPoint {
    cx.read(|cx| {
        state
            .read(cx)
            .get_node(&id.to_string().into())
            .unwrap()
            .position
    })
}

fn dragging(state: &Entity<FlowState>, cx: &VisualTestContext) -> bool {
    cx.read(|cx| state.read(cx).drag_state.is_some())
}

#[gpui::test]
fn release_outside_the_canvas_ends_a_node_drag(cx: &mut TestAppContext) {
    let (state, cx) = rig(cx);
    cx.simulate_mouse_down(at(120.0, 110.0), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(at(220.0, 160.0), MouseButton::Left, Modifiers::none());
    assert!(dragging(&state, cx));
    assert_eq!(node_position(&state, cx, "a"), FlowPoint::new(200.0, 150.0));
    // The pointer is over the header, outside the canvas.
    cx.simulate_mouse_up(
        point(px(220.0), px(10.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    assert!(!dragging(&state, cx));
    assert!(cx.read(|cx| state.read(cx).nodes.iter().all(|node| !node.dragging)));
}

#[gpui::test]
fn release_after_a_key_press_ends_a_node_drag(cx: &mut TestAppContext) {
    let (state, cx) = rig(cx);
    cx.simulate_mouse_down(at(120.0, 110.0), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(at(140.0, 130.0), MouseButton::Left, Modifiers::none());
    cx.simulate_keystrokes("shift");
    // No pointer motion between the key and the release.
    cx.simulate_mouse_up(at(140.0, 130.0), MouseButton::Left, Modifiers::none());
    assert!(!dragging(&state, cx));
}

#[gpui::test]
fn release_outside_the_canvas_ends_a_pan(cx: &mut TestAppContext) {
    let (state, cx) = rig(cx);
    cx.simulate_mouse_down(at(500.0, 300.0), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(at(450.0, 280.0), MouseButton::Left, Modifiers::none());
    assert_eq!(cx.read(|cx| state.read(cx).viewport.x), -50.0);
    cx.simulate_mouse_up(
        point(px(800.0), px(700.0)),
        MouseButton::Left,
        Modifiers::none(),
    );
    assert!(cx.read(|cx| state.read(cx).pan_drag.is_none()));
}

#[gpui::test]
fn escape_cancels_a_node_drag_and_puts_the_node_back(cx: &mut TestAppContext) {
    let (state, cx) = rig(cx);
    cx.simulate_mouse_down(at(120.0, 110.0), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(at(220.0, 160.0), MouseButton::Left, Modifiers::none());
    assert_ne!(node_position(&state, cx, "a"), FlowPoint::new(100.0, 100.0));
    cx.simulate_keystrokes("escape");
    assert!(!dragging(&state, cx));
    assert_eq!(node_position(&state, cx, "a"), FlowPoint::new(100.0, 100.0));
    // The release that follows changes nothing.
    cx.simulate_mouse_up(at(220.0, 160.0), MouseButton::Left, Modifiers::none());
    assert_eq!(node_position(&state, cx, "a"), FlowPoint::new(100.0, 100.0));
}

/// The center of a pin in window coordinates.
fn pin_point(
    state: &Entity<FlowState>,
    cx: &VisualTestContext,
    node: &str,
    handle: &str,
    side: HandlePosition,
) -> gpui::Point<gpui::Pixels> {
    let (x, y) = cx.read(|cx| {
        state
            .read(cx)
            .find_handle_center(
                &node.to_string().into(),
                &Some(handle.to_string().into()),
                side,
            )
            .unwrap()
    });
    at(x, y)
}

#[gpui::test]
fn a_drop_on_an_occupied_pin_sends_the_reason_to_a_subscriber(cx: &mut TestAppContext) {
    let (state, cx) = rig(cx);
    let events: Rc<RefCell<Vec<FlowEvent>>> = Rc::default();
    let log = events.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&state, move |_, event: &FlowEvent, _| {
            log.borrow_mut().push(event.clone())
        })
    });
    let out = pin_point(&state, cx, "a", "out", HandlePosition::Right);
    let input = pin_point(&state, cx, "b", "in", HandlePosition::Left);
    cx.simulate_mouse_down(out, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(input, MouseButton::Left, Modifiers::none());
    // The draft shows the refusal before the release.
    let refusal = cx.read(|cx| {
        let draft = state.read(cx).connecting.clone().unwrap();
        draft.snap_target.unwrap().refusal
    });
    assert_eq!(refusal, Some(INPUT_OCCUPIED.into()));
    assert!(events.borrow().is_empty());
    cx.simulate_mouse_up(input, MouseButton::Left, Modifiers::none());
    assert_eq!(
        *events.borrow(),
        vec![FlowEvent::ConnectionRefused {
            connection: Connection {
                source: "a".into(),
                target: "b".into(),
                source_handle: Some("out".into()),
                target_handle: Some("in".into()),
            },
            reason: INPUT_OCCUPIED.into(),
        }]
    );
    assert_eq!(cx.read(|cx| state.read(cx).edges.len()), 1);
}

#[gpui::test]
fn the_canvas_binds_no_undo_key(cx: &mut TestAppContext) {
    let (state, cx) = rig(cx);
    cx.simulate_mouse_down(at(120.0, 110.0), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(at(220.0, 160.0), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at(220.0, 160.0), MouseButton::Left, Modifiers::none());
    let moved = FlowPoint::new(200.0, 150.0);
    assert_eq!(node_position(&state, cx, "a"), moved);
    cx.simulate_keystrokes("ctrl-z super-z ctrl-shift-z super-shift-z");
    assert_eq!(node_position(&state, cx, "a"), moved);
}

#[gpui::test]
fn events_reach_a_subscriber_in_the_order_of_the_inputs(cx: &mut TestAppContext) {
    let (state, cx) = rig(cx);
    let events: Rc<RefCell<Vec<FlowEvent>>> = Rc::default();
    let log = events.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&state, move |_, event: &FlowEvent, _| {
            log.borrow_mut().push(event.clone())
        })
    });
    cx.simulate_mouse_down(at(120.0, 110.0), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(at(220.0, 160.0), MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_up(at(220.0, 160.0), MouseButton::Left, Modifiers::none());
    cx.simulate_keystrokes("delete");
    let events = events.borrow();
    let [
        FlowEvent::SelectionChanged { selection: Some(_) },
        FlowEvent::NodeMoved { from, to, .. },
        FlowEvent::NodeRemoved { id },
        FlowEvent::SelectionChanged { selection: None },
    ] = events.as_slice()
    else {
        panic!("unexpected events: {events:?}");
    };
    assert_eq!(*from, FlowPoint::new(100.0, 100.0));
    assert_eq!(*to, FlowPoint::new(200.0, 150.0));
    assert_eq!(id, "a");
}

/// A click on the empty canvas: the canvas takes the focus and the selection is empty.
fn click_empty_canvas(cx: &mut VisualTestContext) {
    cx.simulate_click(at(500.0, 350.0), Modifiers::none());
}

fn selection(state: &Entity<FlowState>, cx: &VisualTestContext) -> Option<String> {
    cx.read(|cx| match state.read(cx).selection() {
        Some(nicecooling_flow::Selection::Node(id)) => Some(id.to_string()),
        _ => None,
    })
}

#[gpui::test]
fn a_press_gives_the_canvas_the_focus_and_it_is_in_the_tab_order(cx: &mut TestAppContext) {
    let (_state, graph, cx) = rig_with(cx, FlowState::new(nodes(), edges()));
    let focused = |cx: &mut VisualTestContext| {
        cx.update(|window, cx| graph.read(cx).focus_handle().is_focused(window))
    };
    assert!(!focused(cx));
    cx.update(|window, cx| window.focus_next(cx));
    assert!(focused(cx), "the canvas is a tab stop");
    cx.update(|window, cx| window.blur(cx));
    assert!(!focused(cx));
    click_empty_canvas(cx);
    assert!(focused(cx), "a press focuses the canvas");
}

#[gpui::test]
fn the_keys_do_nothing_without_the_focus(cx: &mut TestAppContext) {
    let (state, cx) = rig(cx);
    cx.simulate_keystrokes("tab");
    assert_eq!(selection(&state, cx), None);
    click_empty_canvas(cx);
    cx.simulate_keystrokes("tab");
    assert_eq!(selection(&state, cx), Some("a".into()));
}

#[gpui::test]
fn tab_and_shift_tab_move_the_selection(cx: &mut TestAppContext) {
    let (state, cx) = rig(cx);
    click_empty_canvas(cx);
    cx.simulate_keystrokes("tab");
    assert_eq!(selection(&state, cx), Some("a".into()));
    cx.simulate_keystrokes("tab");
    assert_eq!(selection(&state, cx), Some("b".into()));
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(selection(&state, cx), Some("a".into()));
    cx.simulate_keystrokes("shift-tab");
    assert_eq!(selection(&state, cx), Some("c".into()));
}

#[gpui::test]
fn the_arrow_keys_move_the_selected_node_by_one_grid_step(cx: &mut TestAppContext) {
    let (state, cx) = rig(cx);
    click_empty_canvas(cx);
    cx.simulate_keystrokes("tab right right down");
    assert_eq!(node_position(&state, cx, "a"), FlowPoint::new(140.0, 120.0));
    cx.simulate_keystrokes("left up");
    assert_eq!(node_position(&state, cx, "a"), FlowPoint::new(120.0, 100.0));
}

#[gpui::test]
fn delete_asks_its_user_and_the_user_can_refuse(cx: &mut TestAppContext) {
    let state = FlowState::new(nodes(), edges()).with_delete_guard(|selection, _| {
        *selection != nicecooling_flow::Selection::Node("b".into())
    });
    let (state, _graph, cx) = rig_with(cx, state);
    click_empty_canvas(cx);
    cx.simulate_keystrokes("tab tab delete");
    assert_eq!(selection(&state, cx), Some("b".into()));
    assert_eq!(cx.read(|cx| state.read(cx).nodes.len()), 3);
    cx.simulate_keystrokes("shift-tab delete");
    assert_eq!(cx.read(|cx| state.read(cx).nodes.len()), 2);
    assert_eq!(selection(&state, cx), None);
}

#[gpui::test]
fn plus_minus_and_zero_change_the_zoom(cx: &mut TestAppContext) {
    let (state, cx) = rig(cx);
    let zoom = |cx: &VisualTestContext| cx.read(|cx| state.read(cx).viewport.zoom);
    click_empty_canvas(cx);
    cx.simulate_keystrokes("=");
    assert!((zoom(cx) - 1.2).abs() < 1e-4);
    cx.simulate_keystrokes("= =");
    assert!((zoom(cx) - 1.728).abs() < 1e-3);
    cx.simulate_keystrokes("-");
    assert!((zoom(cx) - 1.44).abs() < 1e-3);
    cx.simulate_keystrokes("0");
    assert_eq!(zoom(cx), 1.0);
    for _ in 0..30 {
        cx.simulate_keystrokes("-");
    }
    assert_eq!(zoom(cx), 0.25);
}

#[gpui::test]
fn a_node_with_the_dashed_flag_has_a_dashed_outline(cx: &mut TestAppContext) {
    let nodes = vec![
        FlowNode::new("plain", 100.0, 100.0)
            .label("P")
            .size(100.0, 40.0),
        FlowNode::new("draft", 300.0, 100.0)
            .label("D")
            .size(100.0, 40.0)
            .dashed(true),
    ];
    let (state, _graph, cx) = rig_with(cx, FlowState::new(nodes, Vec::new()));
    // The scene lists the same node more than once. Count the places, not the quads.
    let dashed = |cx: &mut VisualTestContext| {
        cx.update(|window, _| {
            window
                .painted_quads()
                .iter()
                .filter(|quad| quad.border_style == gpui::BorderStyle::Dashed)
                .map(|quad| format!("{:?}", quad.bounds.origin))
                .collect::<std::collections::BTreeSet<_>>()
                .len()
        })
    };
    assert_eq!(dashed(cx), 1);
    // The flag follows the state that the app sets.
    cx.update(|_, cx| {
        state.update(cx, |state, cx| {
            let mut nodes = state.nodes.clone();
            nodes[1].dashed = false;
            state.set_graph(nodes, Vec::new());
            cx.notify();
        })
    });
    draw(cx);
    assert_eq!(dashed(cx), 0);
}

#[gpui::test]
fn the_draft_line_draws_while_a_connection_drag_is_over_a_free_and_a_refused_pin(
    cx: &mut TestAppContext,
) {
    // The test window has no accessor for the paths of a frame: this checks that the dashed
    // draft line draws without a panic in both looks.
    let (state, cx) = rig(cx);
    let out = pin_point(&state, cx, "a", "out", HandlePosition::Right);
    cx.simulate_mouse_down(out, MouseButton::Left, Modifiers::none());
    cx.simulate_mouse_move(at(250.0, 300.0), MouseButton::Left, Modifiers::none());
    draw(cx);
    let input = pin_point(&state, cx, "b", "in", HandlePosition::Left);
    cx.simulate_mouse_move(input, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(cx.read(|cx| state.read(cx).connecting.is_some()));
    cx.simulate_mouse_up(input, MouseButton::Left, Modifiers::none());
}
