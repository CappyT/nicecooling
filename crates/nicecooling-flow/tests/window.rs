//! Tests of the canvas in a window on the GPUI test platform. They need no display.

use gpui::{
    Context, Entity, IntoElement, Modifiers, MouseButton, Render, TestAppContext,
    VisualTestContext, Window, div, point, prelude::*, px,
};
use nicecooling_flow::{FlowGraph, FlowNode, FlowPoint, FlowState, HandleDef, HandlePosition};

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
    ]
}

fn rig(cx: &mut TestAppContext) -> (Entity<FlowState>, &mut VisualTestContext) {
    let state = cx.new(|_| FlowState::new(nodes(), Vec::new()));
    let graph_state = state.clone();
    let (_host, cx) = cx.add_window_view(|_, cx| Host {
        graph: cx.new(|cx| FlowGraph::new(graph_state, cx)),
    });
    draw(cx);
    (state, cx)
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
