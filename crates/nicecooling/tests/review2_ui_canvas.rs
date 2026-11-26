//! Review reproducers: the node renderer of the canvas. The Graph editor registers its node
//! card with `FlowGraph::default_renderer`. The canvas must draw each node with it.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    AppContext as _, Context, Entity, IntoElement, Render, TestAppContext, Window, div,
    prelude::*, px,
};
use nicecooling_flow::{FlowGraph, FlowNode, FlowState};

struct Host {
    graph: Entity<FlowGraph>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(div().w(px(600.0)).h(px(400.0)).child(self.graph.clone()))
    }
}

/// graph-editor.md, "Canvas": "A node is a box 152 units wide ... Its rows, from the top: Title,
/// Subject, ...". The editor draws that box in the function that it gives to
/// `default_renderer`. The canvas ignores that function and draws the node id as plain text.
#[gpui::test]
fn the_canvas_draws_a_node_with_the_default_renderer(cx: &mut TestAppContext) {
    let calls = Rc::new(Cell::new(0_usize));
    let state = cx.new(|_| {
        FlowState::new(
            vec![
                FlowNode::new("1", 40.0, 40.0).size(152.0, 80.0),
                FlowNode::new("2", 260.0, 40.0).size(152.0, 80.0),
            ],
            Vec::new(),
        )
    });
    let seen = calls.clone();
    let (_host, cx) = cx.add_window_view(|_, cx| Host {
        graph: cx.new(|cx| {
            FlowGraph::new(state.clone(), cx).no_node_chrome().default_renderer(
                move |_, _, _| {
                    seen.set(seen.get() + 1);
                    div().w(px(152.0)).h(px(80.0)).into_any_element()
                },
            )
        }),
    });
    cx.update(|window, cx| window.draw(cx).clear(cx));
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
    assert!(
        calls.get() >= 2,
        "the default renderer drew {} nodes of 2",
        calls.get()
    );
}
