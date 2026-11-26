//! Review reproducers: the Graph editor opens on a Graph that the store accepts and that is odd
//! for the editor. The editor must draw it without a panic.

mod support;

use gpui::{AppContext as _, TestAppContext, VisualTestContext};
use nicecooling::app::{self, Backend, StartMode};
use nicecooling::graphs::view::{GraphRow, RowState, shared_view};
use nicecooling::i18n::Language;
use nicecooling::model::DaemonEvent;
use nicecooling::screen_view::ScreenView;
use nicecooling::shell::Page;
use nicecooling_graph::eval::EndValue;
use nicecooling_graph::model::{Connection, Graph, Kind, Node, StoredGraph};
use support::fixtures::full;
use tokio::sync::{broadcast, mpsc};

/// Open the editor on `graph` and draw the window three times.
fn draw_editor(cx: &mut TestAppContext, graph: Graph) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (events, events_rx) = broadcast::channel(64);
    let (commands, inbox) = mpsc::unbounded_channel();
    let backend = Backend {
        events: events_rx,
        commands: commands.clone(),
        inbox,
        client: None,
        graphs: None,
        runtime: runtime.handle().clone(),
        language: Language::English,
        settings: Default::default(),
    };
    let model = cx.update(|cx| app::init(cx, StartMode::Normal, backend));
    events
        .send(DaemonEvent::Snapshot(Box::new(full().snapshot.clone())))
        .unwrap();
    cx.run_until_parked();
    let shared = shared_view();
    shared.lock().unwrap().rows = vec![GraphRow {
        id: graph.id.clone(),
        name: graph.name.clone(),
        enabled: graph.enabled,
        state: RowState::Running,
        values: Default::default(),
        end: EndValue::None,
        stored: StoredGraph::Valid(graph.clone()),
        run: None,
    }];
    let window = cx.update(|cx| {
        cx.open_window(gpui::WindowOptions::default(), |_, cx| {
            cx.new(|cx| ScreenView::new(model.clone(), commands.clone(), cx))
        })
        .expect("a window")
    });
    cx.update_window(window.into(), |_, _, cx| {
        model.update(cx, |app, _| {
            app.graph_view = Some(shared.clone());
            app.page = Page::Graphs;
        });
    })
    .unwrap();
    window
        .update(cx, |view, _, _| view.graphs_ui().editor = Some(graph))
        .unwrap();
    for _ in 0..3 {
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
    }
    cx.update(|cx| cx.quit());
    drop(runtime);
}

fn placed(node: Node, x: f64, y: f64) -> Node {
    let mut node = node;
    node.x = x;
    node.y = y;
    node
}

/// The store keeps any finite position. 1e300 is finite in f64 and infinite in f32, the type
/// of the canvas.
#[gpui::test]
fn the_editor_draws_a_graph_with_a_node_far_outside_the_view(cx: &mut TestAppContext) {
    let mut graph = Graph::new("graph_1", "Far");
    graph.nodes = vec![
        placed(Node::new(1, Kind::Constant).with_param("value", 1), 1e300, -1e300),
        placed(Node::new(2, Kind::SensorEnd), 0.0, 0.0),
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    draw_editor(cx, graph);
}

/// A Graph without nodes breaks the rule "a Graph has one end node". graphs.md keeps "Edit" on
/// for a Graph that breaks a rule.
#[gpui::test]
fn the_editor_draws_an_empty_graph(cx: &mut TestAppContext) {
    draw_editor(cx, Graph::new("graph_1", "Empty"));
}

/// 2000 nodes break the node limit only. graphs.md keeps "Edit" on for that Graph.
#[gpui::test]
fn the_editor_draws_a_graph_with_2000_nodes(cx: &mut TestAppContext) {
    let mut graph = Graph::new("graph_1", "Large");
    graph.nodes = (1..=2000)
        .map(|id| {
            placed(
                Node::new(id, Kind::Add),
                f64::from(id % 50) * 200.0,
                f64::from(id / 50) * 120.0,
            )
        })
        .collect();
    graph.nodes.push(Node::new(2001, Kind::SensorEnd));
    graph.connections = (1..2000)
        .map(|id| Connection::new(id, id + 1, "a"))
        .chain(std::iter::once(Connection::new(2000, 2001, "a")))
        .collect();
    draw_editor(cx, graph);
}

/// A name of 64 characters of two bytes each, and a label of 32 characters.
#[gpui::test]
fn the_editor_draws_a_graph_with_a_name_of_64_characters(cx: &mut TestAppContext) {
    let mut graph = Graph::new("graph_1", &"\u{e9}".repeat(64));
    graph.nodes = vec![
        placed(
            Node::new(1, Kind::Comment).with_param("text", "\u{e9}".repeat(500)),
            0.0,
            0.0,
        ),
        placed(Node::new(2, Kind::Add).with_label(&"a".repeat(32)), 200.0, 0.0),
        placed(Node::new(3, Kind::SensorEnd), 400.0, 0.0),
    ];
    graph.connections = vec![Connection::new(2, 3, "a")];
    draw_editor(cx, graph);
}
