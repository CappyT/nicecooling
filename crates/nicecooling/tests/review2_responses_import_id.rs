//! Review of the id of a new Graph against another writer of `graphs.json`.
//!
//! The store reads `graphs.json` again before each edit, so the edit sees a Graph that another
//! writer added. The id of a new Graph (an import, a new draft) comes from the Graph view, which
//! holds the file as the app read it last. When another writer has added a Graph since then, the
//! new Graph gets the id of that Graph, and the store replaces it.

mod support;

use nicecooling::graphs::channel::GraphCommand;
use nicecooling::graphs::service::{Env, GraphService, NoLink, NoPower};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::transfer::{export_text, import_text, new_graph_id};
use nicecooling::graphs::view::{SharedView, shared_view};
use nicecooling::i18n::EN;
use nicecooling_core::clock::SimClock;
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};
use support::graphs::{Dir, constant_graph};
use tokio::sync::mpsc;

struct NoScan;

impl ProcessSource for NoScan {
    fn scan(&self) -> ProcessList {
        ProcessList::default()
    }
}

/// The app with `graph_1`, and another writer that then adds `graph_2` named "Theirs".
fn app_and_other_writer(dir: &Dir) -> (GraphService, SharedView) {
    let mut store = GraphStore::open(dir.graph_paths());
    store.put(constant_graph("graph_1", 1.0)).unwrap();
    let view = shared_view();
    let (notify, notifications) = mpsc::unbounded_channel();
    std::mem::forget(notifications);
    let service = GraphService::new(
        store,
        Env {
            clock: Box::new(SimClock::default()),
            link: Box::new(NoLink),
            scan: Box::new(NoScan),
            power: Box::new(NoPower),
            notify,
            locale: EN,
            view: view.clone(),
        },
    );

    let mut other = GraphStore::open(dir.graph_paths());
    let mut theirs = constant_graph("graph_2", 7.0);
    theirs.name = "Theirs".into();
    other.put(theirs).unwrap();
    (service, view)
}

fn stored(dir: &Dir) -> Vec<Graph> {
    GraphStore::open(dir.graph_paths()).graphs()
}

/// The spec (node-graph, Ids of stored Graphs): "The app SHALL give a new Graph (an import, a new
/// draft) an id that no stored entry holds". And (Graph storage, File changed while the app runs):
/// the file keeps the Graph of the other writer.
#[test]
fn import_keeps_the_graph_of_another_writer() {
    let dir = Dir::new("rv2-import-other-writer");
    let (mut service, view) = app_and_other_writer(&dir);

    // The Graphs screen imports a file with the Graphs of its view.
    let existing = view.lock().unwrap().graphs();
    let mut file_graph = constant_graph("graph_1", 3.0);
    file_graph.name = "Imported".into();
    let imported = import_text(&export_text(&file_graph), &existing).expect("a valid file");
    service.handle(GraphCommand::Import(Box::new(imported)));

    let graphs = stored(&dir);
    let names: Vec<(&str, &str)> = graphs
        .iter()
        .map(|g| (g.id.as_str(), g.name.as_str()))
        .collect();
    assert!(
        names.contains(&("graph_2", "Theirs")),
        "the Graph of the other writer is gone: {names:?}"
    );
    assert_eq!(graphs.len(), 3, "the file holds 3 Graphs: {names:?}");
}

/// The same for a new draft: the Graphs screen gives it `new_graph_id` of its view.
#[test]
fn new_draft_keeps_the_graph_of_another_writer() {
    let dir = Dir::new("rv2-draft-other-writer");
    let (mut service, view) = app_and_other_writer(&dir);

    // A draft with no sensor end node: its Apply needs no call to the daemon.
    let id = new_graph_id(&view.lock().unwrap().graphs());
    let mut draft = Graph::new(&id, "Mine");
    draft.nodes = vec![
        Node::new(1, Kind::Constant).with_param("value", 0.0),
        Node::new(2, Kind::ResponseEnd)
            .with_param("kind", "notification")
            .with_param("title", "Hot"),
    ];
    draft.connections = vec![Connection::new(1, 2, "a")];
    service.handle(GraphCommand::Apply(Box::new(draft)));

    let graphs = stored(&dir);
    let names: Vec<(&str, &str)> = graphs
        .iter()
        .map(|g| (g.id.as_str(), g.name.as_str()))
        .collect();
    assert!(
        names.contains(&("graph_2", "Theirs")),
        "the Graph of the other writer is gone: {names:?}"
    );
    assert_eq!(graphs.len(), 3, "the file holds 3 Graphs: {names:?}");
}
