mod support;

use nicecooling::graphs::secrets::{PasswordEdit, Secrets};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::transfer::{
    ImportError, MAX_IMPORT_BYTES, export_file, export_text, import_file, import_text,
};
use nicecooling_graph::model::{Connection, ExportError, Graph, Kind, Node};
use support::graphs::{Dir, constant_graph, persistent_graph};

/// A Graph with a response end node that sends an e-mail. Its node ids are not 1 and 2.
fn mail_graph() -> Graph {
    let mut graph = Graph::new("graph_7", "Mail");
    graph.sensor_id = Some("graph_7".into());
    graph.nodes = vec![
        Node::new(10, Kind::Constant).with_param("value", 1.0),
        Node::new(20, Kind::ResponseEnd)
            .with_param("kind", "email")
            .with_param("to", "owner@example.org")
            .with_param("subject", "Water is hot")
            .with_param("text", "Check the loop."),
        Node::new(30, Kind::PersistentStorage),
    ];
    graph.connections = vec![Connection::new(10, 20, "a")];
    graph
}

#[test]
fn export_has_no_values_or_passwords() {
    let dir = Dir::new("export-secrets");
    let paths = dir.graph_paths();
    let mut graph = mail_graph();
    graph.id = "graph_1".into();
    graph.sensor_id = None;
    graph.nodes[2] = Node::new(4, Kind::PersistentStorage);
    let mut store = GraphStore::open(paths.clone());
    store.put(graph.clone()).unwrap();
    store.state_mut().values.insert("graph_1/4".into(), 4242.5);
    store.write_state(std::time::Duration::ZERO, true).unwrap();
    let mut secrets = Secrets::open(&paths.secrets);
    secrets.edit_smtp(PasswordEdit::Set("hunter2".into()));
    secrets.save().unwrap();

    let text = export_text(&graph);
    assert!(
        !text.contains("hunter2") && !text.contains("password"),
        "{text}"
    );
    assert!(!text.contains("4242") && !text.contains("values"), "{text}");
    assert!(!text.contains("last_run_us"), "{text}");
    for kept in ["owner@example.org", "Water is hot", "Check the loop."] {
        assert!(text.contains(kept), "{kept} in {text}");
    }
}

#[test]
fn export_has_no_sensor_id() {
    let mut graph = constant_graph("graph_3", 5.0);
    graph.sensor_id = Some("graph_3".into());
    let text = export_text(&graph);
    assert!(!text.contains("sensor_id"), "{text}");
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["format"], "nicecooling-graph");
    assert_eq!(json["format_version"], 1);
    assert_eq!(json["graph"]["name"], "Five");
    assert_eq!(json["graph"]["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(json["graph"]["connections"].as_array().unwrap().len(), 1);
    assert!(json["graph"].get("id").is_none() && json["graph"].get("enabled").is_none());

    // The file on disk is the same text.
    let dir = Dir::new("export-file");
    let path = dir.path().join("five.json");
    export_file(&path, &graph).unwrap();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
}

#[test]
fn import_twice_makes_two_graphs() {
    let text = export_text(&mail_graph());
    let first = import_text(&text, &[]).unwrap();
    let second = import_text(&text, std::slice::from_ref(&first)).unwrap();
    assert_ne!(first.id, second.id);
    assert_eq!(first.id, "graph_1");
    assert_eq!(second.id, "graph_2");
    // The nodes get new ids, and the connections follow.
    let ids: Vec<u32> = first.nodes.iter().map(|n| n.id).collect();
    assert_eq!(ids, [1, 2, 3]);
    assert_eq!(first.connections, [Connection::new(1, 2, "a")]);
    assert_eq!(first.node(2).unwrap().kind, Kind::ResponseEnd);
}

#[test]
fn import_unknown_sensor_stays_unbound() {
    let mut graph = constant_graph("graph_1", 1.0);
    graph.nodes[0] = Node::new(1, Kind::SensorSource).with_param("sensor", "nct9999_temp7");
    let imported = import_text(&export_text(&graph), &[]).unwrap();
    assert_eq!(imported.node(1).unwrap().text("sensor"), "nct9999_temp7");
    assert_eq!(
        imported.source_sensors().collect::<Vec<_>>(),
        ["nct9999_temp7"]
    );
}

#[test]
fn imported_graph_is_disabled() {
    let mut graph = mail_graph();
    graph.nodes[1] = Node::new(20, Kind::ResponseEnd)
        .with_param("kind", "command")
        .with_param("path", "/usr/bin/true");
    graph.enabled = true;
    let imported = import_text(&export_text(&graph), &[]).unwrap();
    assert!(!imported.enabled);
    assert_eq!(imported.sensor_id, None);
    assert_eq!(imported.name, "Mail");
}

#[test]
fn file_over_1_mib_is_refused() {
    let dir = Dir::new("big");
    let text = export_text(&constant_graph("graph_1", 1.0));
    // Spaces after the JSON value are valid. The file is exactly at the limit, then one byte over.
    let padded = |size: u64| format!("{text}{}", " ".repeat(size as usize - text.len()));
    let path = dir.path().join("graph.json");
    std::fs::write(&path, padded(MAX_IMPORT_BYTES)).unwrap();
    assert!(import_file(&path, &[]).is_ok());
    std::fs::write(&path, padded(MAX_IMPORT_BYTES + 1)).unwrap();
    assert_eq!(import_file(&path, &[]), Err(ImportError::TooLarge));
    assert!(ImportError::TooLarge.to_string().contains("1048576"));
    // A file that is not there has its own message.
    assert!(matches!(
        import_file(&dir.path().join("none.json"), &[]),
        Err(ImportError::Read(_))
    ));
}

#[test]
fn format_version_2_is_refused() {
    let text = export_text(&constant_graph("graph_1", 1.0))
        .replace("\"format_version\": 1", "\"format_version\": 2");
    let error = import_text(&text, &[]).unwrap_err();
    assert_eq!(
        error,
        ImportError::Format(ExportError::HigherFormatVersion(2))
    );
    assert!(error.to_string().contains("format version 2"), "{error}");
    // Text that is no Graph file at all.
    assert!(matches!(
        import_text("{\"hello\": 1}", &[]),
        Err(ImportError::Format(ExportError::WrongFormat))
    ));
}

#[test]
fn invalid_graph_is_refused_with_a_message() {
    let mut graph = constant_graph("graph_1", 1.0);
    graph.nodes.push(Node::new(3, Kind::SensorEnd));
    let error = import_text(&export_text(&graph), &[]).unwrap_err();
    assert!(
        matches!(&error, ImportError::Invalid(r) if !r.is_empty()),
        "{error:?}"
    );
    assert!(error.to_string().contains("two-end-nodes"), "{error}");
    // The persistent Graph of the store tests is fine.
    assert!(import_text(&export_text(&persistent_graph("graph_1")), &[]).is_ok());
}
