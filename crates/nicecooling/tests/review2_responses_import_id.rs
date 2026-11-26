//! Review of the id of a new Graph against another writer of `graphs.json`.
//!
//! The store reads `graphs.json` again before each edit, so the edit sees a Graph that another
//! writer added. The id of a new Graph (an import, a new draft) comes from the Graph view, which
//! holds the file as the app read it last. When another writer has added a Graph since then, the
//! new Graph gets the id of that Graph, and the store replaces it.

mod support;

use nicecooling::command::Command;
use nicecooling::graphs::channel::GraphCommand;
use nicecooling::graphs::service::{Env, GraphService, NoLink, NoPower};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::transfer::{export_text, import_text, new_graph_id};
use nicecooling::graphs::view::{Applied, SharedView, shared_view};
use nicecooling::i18n::EN;
use nicecooling::screens::graph_editor::EditorUi;
use nicecooling::screens::graph_editor::apply::ApplyState;
use nicecooling::screens::graphs::GraphsUi;
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

/// A draft of the Graphs screen with a response end node: its Apply needs no call to the daemon.
fn draft_of(view: &SharedView) -> Graph {
    let id = new_graph_id(&view.lock().unwrap().graphs());
    let mut draft = Graph::new(&id, "Mine");
    draft.nodes = vec![
        Node::new(1, Kind::Constant).with_param("value", 0.0),
        Node::new(2, Kind::ResponseEnd)
            .with_param("kind", "notification")
            .with_param("title", "Hot"),
    ];
    draft.connections = vec![Connection::new(1, 2, "a")];
    draft
}

/// The store gives the id inside the edit, after it read the file again, and never replaces an
/// entry with a new Graph.
#[test]
fn store_gives_a_new_graph_its_id_after_the_reread() {
    let dir = Dir::new("rv2-store-add");
    let mut store = GraphStore::open(dir.graph_paths());
    store.put(constant_graph("graph_1", 1.0)).unwrap();
    let mut other = GraphStore::open(dir.graph_paths());
    other.put(constant_graph("graph_2", 7.0)).unwrap();

    // The caller proposes `graph_2`: its view is older than the file.
    let id = store.add(constant_graph("graph_2", 3.0)).unwrap();
    assert_eq!(id, "graph_3");
    let ids: Vec<String> = stored(&dir).into_iter().map(|g| g.id).collect();
    assert_eq!(ids, ["graph_1", "graph_2", "graph_3"]);
    assert_eq!(stored(&dir)[1], constant_graph("graph_2", 7.0));
    // The id of an entry that this version cannot run is taken too.
    assert!(store.holds("graph_2") && !store.holds("graph_4"));
}

/// The answer to the Import names the id of the command and the id of the store, and the mark
/// "imported" of the Graphs screen moves to the stored Graph.
#[test]
fn import_answer_names_the_id_of_the_store() {
    let dir = Dir::new("rv2-import-answer");
    let (mut service, view) = app_and_other_writer(&dir);
    let existing = view.lock().unwrap().graphs();
    let imported = import_text(&export_text(&constant_graph("graph_1", 3.0)), &existing)
        .expect("a valid file");
    assert_eq!(imported.id, "graph_2", "the id of the old view");
    let mut ui = GraphsUi::default();
    ui.imported.insert(imported.id.clone());
    service.handle(GraphCommand::Import(Box::new(imported)));

    let shown = view.lock().unwrap().clone();
    assert_eq!(
        shown.imported,
        Some(("graph_2".to_owned(), "graph_3".to_owned()))
    );
    ui.follow(&shown);
    assert_eq!(ui.imported.iter().collect::<Vec<_>>(), ["graph_3"]);
    assert!(shown.row("graph_3").is_some_and(|row| !row.enabled));
}

/// The answer to the Apply of a new draft names the id of the store. The editor takes it: the
/// draft is the stored Graph, and the next Apply changes this Graph and no other.
#[test]
fn editor_takes_the_id_that_the_store_gave() {
    let dir = Dir::new("rv2-draft-answer");
    let (mut service, view) = app_and_other_writer(&dir);
    let before = view.lock().unwrap().clone();
    let mut ui = EditorUi::open(draft_of(&view), &before);
    assert_eq!(ui.draft.graph().id, "graph_2");
    assert_eq!(ui.follow(&before, 1), None);

    let Some(Command::Graph(command)) = ui.apply(&before, true) else {
        panic!("the Apply of a new draft");
    };
    service.handle(command);
    let after = view.lock().unwrap().clone();
    assert_eq!(
        after.applied,
        Some(Applied {
            graph: "graph_2".to_owned(),
            result: Ok("graph_3".to_owned()),
        })
    );
    assert_eq!(ui.follow(&after, 2), None);
    assert_eq!(ui.apply.state, ApplyState::Applied { at_us: 2 });
    assert_eq!(ui.draft.graph().id, "graph_3");
    assert!(!ui.draft.is_dirty(), "the draft is the stored Graph");

    // A second Apply changes `graph_3`, and the Graph of the other writer stays.
    ui.draft.set_name("Mine 2");
    let Some(Command::Graph(command)) = ui.apply(&after, true) else {
        panic!("the Apply of the changed draft");
    };
    service.handle(command);
    let names: Vec<(String, String)> = stored(&dir).into_iter().map(|g| (g.id, g.name)).collect();
    assert_eq!(
        names,
        [
            ("graph_1".to_owned(), "Five".to_owned()),
            ("graph_2".to_owned(), "Theirs".to_owned()),
            ("graph_3".to_owned(), "Mine 2".to_owned()),
        ]
    );
}

/// The service reads the file for another command, so its view shows the Graph of the other
/// writer under the id of the open draft. The draft takes a free id: its Apply replaces nothing.
#[test]
fn draft_takes_a_free_id_when_another_graph_takes_its_id() {
    let dir = Dir::new("rv2-draft-id-taken");
    let (mut service, view) = app_and_other_writer(&dir);
    let before = view.lock().unwrap().clone();
    let mut ui = EditorUi::open(draft_of(&view), &before);
    assert_eq!(ui.follow(&before, 1), None);
    assert_eq!(ui.draft.graph().id, "graph_2");

    service.handle(GraphCommand::Enable {
        graph: "graph_1".into(),
        enabled: false,
    });
    let after = view.lock().unwrap().clone();
    assert!(after.row("graph_2").is_some(), "the view shows `Theirs`");
    assert_eq!(ui.follow(&after, 2), None);
    assert_eq!(ui.draft.graph().id, "graph_3");
    assert_eq!(ui.draft.stored(), None);

    let Some(Command::Graph(command)) = ui.apply(&after, true) else {
        panic!("the Apply of the draft");
    };
    service.handle(command);
    let graphs = stored(&dir);
    assert_eq!(graphs.len(), 3);
    assert_eq!(graphs[1].name, "Theirs");
    assert_eq!(graphs[2].name, "Mine");
}
