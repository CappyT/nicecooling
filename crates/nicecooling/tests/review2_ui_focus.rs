//! Review reproducers: the keyboard focus of the dialogs of the Graphs screen.

mod support;

use gpui::{AppContext as _, TestAppContext, VisualTestContext};
use nicecooling::app::{self, Backend, StartMode};
use nicecooling::command::Command;
use nicecooling::graphs::channel::GraphCommand;
use nicecooling::graphs::view::{GraphRow, RowState, shared_view};
use nicecooling::i18n::Language;
use nicecooling::model::DaemonEvent;
use nicecooling::screen_view::ScreenView;
use nicecooling::shell::Page;
use nicecooling_graph::eval::EndValue;
use nicecooling_graph::model::{Graph, Kind, Node, StoredGraph};
use support::fixtures::full;
use tokio::sync::{broadcast, mpsc};

/// graphs.md, "Interactions": "'Delete' asks ... Buttons: 'Cancel', 'Delete Graph'. The default
/// focus is on 'Cancel'." Enter on the open question must then press "Cancel". The dialog takes
/// no focus, so Enter does nothing and the keyboard user cannot reach the buttons first.
#[gpui::test]
fn the_delete_question_has_the_focus_on_cancel(cx: &mut TestAppContext) {
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
    let mut graph = Graph::new("graph_1", "Water too warm");
    graph.nodes = vec![Node::new(1, Kind::ResponseEnd)];
    let shared = shared_view();
    shared.lock().unwrap().rows = vec![GraphRow {
        id: graph.id.clone(),
        name: graph.name.clone(),
        enabled: true,
        state: RowState::Running,
        values: Default::default(),
        end: EndValue::None,
        stored: StoredGraph::Valid(graph),
        run: None,
    }];
    let (screen_commands, mut sent) = mpsc::unbounded_channel();
    let window = cx.update(|cx| {
        cx.open_window(gpui::WindowOptions::default(), |_, cx| {
            cx.new(|cx| ScreenView::new(model.clone(), screen_commands.clone(), cx))
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
    let draw = |cx: &mut TestAppContext| {
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
        cx.run_until_parked();
    };
    draw(cx);
    window
        .update(cx, |view, _, _| {
            view.graphs_ui().delete = Some("graph_1".to_owned());
        })
        .unwrap();
    draw(cx);
    draw(cx);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_keystrokes("enter");
    draw(cx);
    let open = window
        .update(cx, |view, _, _| view.graphs_ui().delete.clone())
        .unwrap();
    let mut deleted = false;
    while let Ok(command) = sent.try_recv() {
        deleted |= matches!(command, Command::Graph(GraphCommand::Delete { .. }));
    }
    assert!(!deleted, "Enter deleted the Graph");
    assert_eq!(
        open, None,
        "Enter did not press Cancel: the question has no focus"
    );
    cx.update(|cx| cx.quit());
    drop(runtime);
}
