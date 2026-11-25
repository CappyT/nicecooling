//! Review of export and import: hostile files, ids that collide with stored Graphs, and the
//! files next to the export path.

mod support;

use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::transfer::{
    ImportError, MAX_IMPORT_BYTES, export_file, export_text, import_file, import_text,
};
use nicecooling_graph::model::{Kind, Node, StoredGraph};
use nicecooling_graph::validate::RefusalCode;
use support::graphs::{Dir, constant_graph};

fn refused_with(result: Result<nicecooling_graph::model::Graph, ImportError>, code: RefusalCode) {
    match result {
        Err(ImportError::Invalid(refusals)) => {
            assert!(refusals.iter().any(|r| r.code == code), "{refusals:?}");
        }
        other => panic!("expected a refusal {code:?}, got {other:?}"),
    }
}

/// `graphs.json` holds a Graph with a node kind of a newer version (`graph_2`), and a valid
/// Graph `graph_1`. An import must not take the id of the Graph that this version cannot read:
/// the store would then replace its text, and the Graph of the newer version would be lost.
#[test]
fn import_id_does_not_collide_with_an_invalid_graph() {
    let dir = Dir::new("rv-import-collision");
    let paths = dir.graph_paths();
    std::fs::create_dir_all(paths.graphs.parent().unwrap()).unwrap();
    let odd = r#"{"id":"graph_2","name":"Odd","enabled":true,"sensor_id":null,"nodes":[{"id":1,"kind":"quantum","label":null,"params":{},"x":0.0,"y":0.0}],"connections":[]}"#;
    let valid = serde_json::to_string(&constant_graph("graph_1", 1.0)).unwrap();
    std::fs::write(
        &paths.graphs,
        format!(r#"{{"schema_version":1,"graphs":[{valid},{odd}]}}"#),
    )
    .unwrap();

    let mut store = GraphStore::open(paths.clone());
    let imported = import_text(&export_text(&constant_graph("x", 3.0)), &store.graphs()).unwrap();
    // The window applies the imported Graph as it applies any Graph.
    store.put(imported.clone()).unwrap();
    let text = std::fs::read_to_string(&paths.graphs).unwrap();
    assert!(
        text.contains(odd),
        "the Graph of the newer version is gone after the import of {}:\n{text}",
        imported.id
    );
    assert_eq!(store.entries().len(), 3);
}

/// 1 MiB is the limit. One byte more is refused before the parse.
#[test]
fn one_byte_over_the_limit_is_refused() {
    let dir = Dir::new("rv-import-size");
    let path = dir.path().join("big.json");
    std::fs::write(&path, vec![b' '; MAX_IMPORT_BYTES as usize + 1]).unwrap();
    assert_eq!(import_file(&path, &[]), Err(ImportError::TooLarge));
}

/// Deep nesting gives a message and no stack overflow.
#[test]
fn deeply_nested_json_is_refused() {
    let depth = 100_000;
    let text = format!(
        r#"{{"format":"nicecooling-graph","format_version":1,"graph":{{"name":"Deep","nodes":[{{"id":1,"kind":"comment","label":null,"params":{{"text":{}{}}},"x":0,"y":0}}],"connections":[]}}}}"#,
        "[".repeat(depth),
        "]".repeat(depth)
    );
    assert!(matches!(
        import_text(&text, &[]),
        Err(ImportError::Format(_))
    ));
}

/// A Graph of 257 nodes is refused with `limit-nodes`.
#[test]
fn import_of_257_nodes_is_refused() {
    let mut graph = constant_graph("graph_1", 1.0);
    for id in 3..=257 {
        graph.nodes.push(Node::new(id, Kind::Comment));
    }
    assert_eq!(graph.nodes.len(), 257);
    refused_with(
        import_text(&export_text(&graph), &[]),
        RefusalCode::LimitNodes,
    );
}

/// The 65th Graph is refused with `limit-graphs`.
#[test]
fn import_of_the_65th_graph_is_refused() {
    let stored: Vec<_> = (1..=64)
        .map(|n| constant_graph(&format!("graph_{n}"), 1.0))
        .collect();
    refused_with(
        import_text(&export_text(&constant_graph("x", 1.0)), &stored),
        RefusalCode::LimitGraphs,
    );
}

/// A file with a `sensor_id`, an `enabled` flag and a Graph id: the import drops all three.
#[test]
fn file_sensor_id_and_enabled_are_dropped() {
    let text = export_text(&constant_graph("graph_1", 1.0)).replacen(
        "\"graph\": {",
        "\"graph\": {\"id\": \"graph_1\", \"sensor_id\": \"graph_1\", \"enabled\": true,",
        1,
    );
    let stored = vec![constant_graph("graph_1", 1.0)];
    let graph = import_text(&text, &stored).unwrap();
    assert_eq!(graph.sensor_id, None);
    assert!(!graph.enabled);
    assert_ne!(graph.id, "graph_1");
}

/// Bytes that are not UTF-8 give a read message, and the message does not echo the bytes.
#[test]
fn non_utf8_file_is_refused() {
    let dir = Dir::new("rv-import-utf8");
    let path = dir.path().join("bad.json");
    let mut bytes =
        br#"{"format":"nicecooling-graph","format_version":1,"graph":{"name":""#.to_vec();
    bytes.extend_from_slice(&[0xff, 0xfe, 0xfd]);
    bytes.extend_from_slice(br#"","nodes":[],"connections":[]}}"#);
    std::fs::write(&path, bytes).unwrap();
    assert!(matches!(import_file(&path, &[]), Err(ImportError::Read(_))));
}

/// The stored Graphs stay as they are after an import that fails.
#[test]
fn failed_import_changes_no_stored_graph() {
    let dir = Dir::new("rv-import-failed");
    let paths = dir.graph_paths();
    let mut store = GraphStore::open(paths.clone());
    store.put(constant_graph("graph_1", 1.0)).unwrap();
    let before = std::fs::read_to_string(&paths.graphs).unwrap();
    assert!(import_text("{", &store.graphs()).is_err());
    assert_eq!(std::fs::read_to_string(&paths.graphs).unwrap(), before);
    assert!(matches!(store.entries()[0], StoredGraph::Valid(_)));
}
