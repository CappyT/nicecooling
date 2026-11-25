mod support;

use std::os::unix::fs::PermissionsExt as _;
use std::time::Duration;

use nicecooling::graphs::secrets::{PasswordEdit, Secrets};
use nicecooling::graphs::store::{GraphStore, StateFile};
use nicecooling_graph::model::{Kind, Node, StoredGraph};
use nicecooling_graph::response::{RunRecord, RunState};
use nicecooling_graph::validate::RefusalCode;
use support::graphs::{Dir, constant_graph, persistent_graph};

fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

#[test]
fn graphs_survive_a_restart() {
    let dir = Dir::new("restart");
    let mut store = GraphStore::open(dir.graph_paths());
    let mut graph = constant_graph("graph_1", 5.0);
    graph.sensor_id = Some("graph_1".into());
    store.put(graph.clone()).unwrap();
    store.put(constant_graph("graph_2", 7.0)).unwrap();
    drop(store);

    let store = GraphStore::open(dir.graph_paths());
    assert_eq!(store.error(), None);
    assert_eq!(store.graph("graph_1"), Some(&graph));
    assert_eq!(store.graphs().len(), 2);
}

#[test]
fn invalid_graph_is_listed_and_not_run() {
    let dir = Dir::new("invalid");
    let mut store = GraphStore::open(dir.graph_paths());
    store.put(constant_graph("graph_1", 1.0)).unwrap();
    // The file of a Graph with two end nodes is a hand edit: the app never stores it.
    let mut broken = constant_graph("graph_2", 1.0);
    broken.nodes.push(Node::new(3, Kind::SensorEnd));
    store.put(broken).unwrap();
    store.put(constant_graph("graph_3", 3.0)).unwrap();
    drop(store);

    let store = GraphStore::open(dir.graph_paths());
    assert_eq!(store.entries().len(), 3);
    let runs: Vec<String> = store.graphs().into_iter().map(|g| g.id).collect();
    assert_eq!(runs, ["graph_1", "graph_3"]);
    assert!(
        matches!(&store.entries()[1], StoredGraph::Invalid(g) if g.id.as_deref() == Some("graph_2"))
    );
}

#[test]
fn unknown_kind_graph_text_survives_a_write() {
    let dir = Dir::new("unknown-kind");
    let paths = dir.graph_paths();
    std::fs::create_dir_all(paths.graphs.parent().unwrap()).unwrap();
    let odd = r#"{"id":"graph_9","name":"Odd","enabled":true,"sensor_id":null,"nodes":[{"id":1,"kind":"quantum","label":null,"params":{"flux":3},"x":0.0,"y":0.0}],"connections":[]}"#;
    let file = format!(r#"{{"schema_version":1,"graphs":[{odd}]}}"#);
    std::fs::write(&paths.graphs, file).unwrap();

    let mut store = GraphStore::open(paths.clone());
    assert!(store.graphs().is_empty());
    store.put(constant_graph("graph_1", 2.0)).unwrap();
    let text = std::fs::read_to_string(&paths.graphs).unwrap();
    assert!(text.contains(r#""kind":"quantum""#), "{text}");
    assert!(text.contains(r#""flux":3"#), "{text}");
    assert!(text.contains(r#""id": "graph_1""#), "{text}");
}

#[test]
fn unparsable_file_stays_unchanged_with_an_error() {
    let dir = Dir::new("unparsable");
    let paths = dir.graph_paths();
    std::fs::create_dir_all(paths.graphs.parent().unwrap()).unwrap();
    std::fs::write(&paths.graphs, "{ not json").unwrap();

    let mut store = GraphStore::open(paths.clone());
    assert!(store.error().is_some());
    assert!(store.put(constant_graph("graph_1", 1.0)).is_err());
    assert!(
        store.entries().is_empty(),
        "the failed write leaves no entry"
    );
    assert_eq!(
        std::fs::read_to_string(&paths.graphs).unwrap(),
        "{ not json"
    );
}

#[test]
fn higher_schema_version_is_not_written() {
    let dir = Dir::new("higher");
    let paths = dir.graph_paths();
    std::fs::create_dir_all(paths.graphs.parent().unwrap()).unwrap();
    let text = r#"{"schema_version":2,"graphs":[]}"#;
    std::fs::write(&paths.graphs, text).unwrap();

    let mut store = GraphStore::open(paths.clone());
    assert!(
        store
            .error()
            .is_some_and(|e| e.contains("schema version 2"))
    );
    assert!(store.put(constant_graph("graph_1", 1.0)).is_err());
    assert_eq!(std::fs::read_to_string(&paths.graphs).unwrap(), text);

    // The state file follows the same rule.
    std::fs::create_dir_all(paths.state.parent().unwrap()).unwrap();
    let state = r#"{"schema_version":2,"values":{}}"#;
    std::fs::write(&paths.state, state).unwrap();
    let mut store = GraphStore::open(paths.clone());
    store.state_mut().values.insert("graph_1/4".into(), 1.0);
    assert!(store.write_state(secs(100), true).is_err());
    assert_eq!(std::fs::read_to_string(&paths.state).unwrap(), state);
}

#[test]
fn state_written_at_most_every_30s() {
    let dir = Dir::new("rate");
    let mut store = GraphStore::open(dir.graph_paths());
    store.put(persistent_graph("graph_1")).unwrap();
    let mut writes = 0;
    for t in 0..60 {
        store
            .state_mut()
            .values
            .insert("graph_1/4".into(), f64::from(t as u32));
        if store.write_state(secs(t), false).unwrap() {
            writes += 1;
        }
    }
    assert!((1..=3).contains(&writes), "{writes} writes in 60 s");
    // The last value waits for the next write. After it, a change of the app run time alone
    // starts no write.
    assert!(store.write_state(secs(100), false).unwrap());
    store.state_mut().pause.quiet_s = 99;
    assert!(!store.write_state(secs(200), false).unwrap());
}

#[test]
fn state_written_at_stop() {
    let dir = Dir::new("stop");
    let paths = dir.graph_paths();
    let mut store = GraphStore::open(paths.clone());
    store.put(persistent_graph("graph_1")).unwrap();
    store.state_mut().values.insert("graph_1/4".into(), 1.0);
    assert!(store.write_state(secs(0), false).unwrap());
    // Inside the 30 s, a new value waits.
    store.state_mut().values.insert("graph_1/4".into(), 2.0);
    assert!(!store.write_state(secs(5), false).unwrap());
    // The stop writes it, and the app run time with it.
    store.state_mut().pause.quiet_s = 5;
    assert!(store.write_state(secs(6), true).unwrap());

    let stored: StateFile =
        serde_json::from_str(&std::fs::read_to_string(&paths.state).unwrap()).unwrap();
    assert_eq!(stored.values["graph_1/4"], 2.0);
    assert_eq!(stored.pause.quiet_s, 5);
    // Nothing changed since: the stop writes no second time.
    assert!(!store.write_state(secs(7), true).unwrap());
}

#[test]
fn removed_node_value_is_deleted() {
    let dir = Dir::new("prune");
    let mut store = GraphStore::open(dir.graph_paths());
    store.put(persistent_graph("graph_1")).unwrap();
    store.state_mut().values.insert("graph_1/4".into(), 7.0);
    store.state_mut().values.insert("graph_2/4".into(), 8.0);
    store.state_mut().responses.insert(
        "graph_2/5".into(),
        RunRecord {
            state: RunState::Run,
            last_run_us: Some(1),
        },
    );

    // Apply of the Graph without node 4.
    store.put(constant_graph("graph_1", 1.0)).unwrap();
    assert!(store.state().values.is_empty(), "{:?}", store.state());
    assert!(store.state().responses.is_empty());
}

#[test]
fn stored_values_keyed_graph_slash_node() {
    let dir = Dir::new("keys");
    let paths = dir.graph_paths();
    let mut store = GraphStore::open(paths.clone());
    store.put(persistent_graph("graph_2")).unwrap();
    store.state_mut().values.insert("graph_2/4".into(), 7.0);
    assert!(store.write_state(secs(0), true).unwrap());
    let json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&paths.state).unwrap()).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert_eq!(json["values"]["graph_2/4"], 7.0);

    let store = GraphStore::open(paths);
    assert_eq!(store.state().values["graph_2/4"], 7.0);
}

fn mode_of(path: &std::path::Path) -> u32 {
    std::fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn secrets_file_is_created_with_mode_0600() {
    let dir = Dir::new("mode");
    let path = dir.graph_paths().secrets;
    let mut secrets = Secrets::open(&path);
    secrets.edit_mqtt(PasswordEdit::Set("hunter2".into()));
    secrets.save().unwrap();
    assert_eq!(mode_of(&path), 0o600);
    // A second write keeps the mode, also over a file that someone opened up.
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    secrets.edit_smtp(PasswordEdit::Set("pw".into()));
    secrets.save().unwrap();
    assert_eq!(mode_of(&path), 0o600);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("schema_version = 1"), "{text}");
    assert!(text.contains("[mqtt]") && text.contains("[smtp]"), "{text}");
}

#[test]
fn untouched_field_keeps_the_password() {
    let dir = Dir::new("untouched");
    let path = dir.graph_paths().secrets;
    let mut secrets = Secrets::open(&path);
    secrets.edit_mqtt(PasswordEdit::Set("hunter2".into()));
    secrets.save().unwrap();

    let mut secrets = Secrets::open(&path);
    secrets.edit_mqtt(PasswordEdit::Untouched);
    secrets.save().unwrap();
    assert_eq!(Secrets::open(&path).mqtt.expose(), "hunter2");
}

#[test]
fn empty_field_clears_the_password() {
    let dir = Dir::new("clear");
    let path = dir.graph_paths().secrets;
    let mut secrets = Secrets::open(&path);
    secrets.edit_mqtt(PasswordEdit::Set("hunter2".into()));
    secrets.edit_smtp(PasswordEdit::Set("other".into()));
    secrets.save().unwrap();

    let mut secrets = Secrets::open(&path);
    secrets.edit_mqtt(PasswordEdit::Set(String::new()));
    secrets.save().unwrap();
    let secrets = Secrets::open(&path);
    assert!(!secrets.mqtt.is_set());
    assert_eq!(secrets.smtp.expose(), "other");
}

#[test]
fn password_is_not_in_debug_output() {
    let dir = Dir::new("debug");
    let mut secrets = Secrets::open(&dir.graph_paths().secrets);
    secrets.edit_mqtt(PasswordEdit::Set("hunter2".into()));
    let edit = PasswordEdit::Set("hunter2".into());
    let text = format!("{secrets:?} {edit:?} {:?}", secrets.mqtt);
    assert!(!text.contains("hunter2"), "{text}");
}

const ODD: &str = r#"{"id":"graph_2","name":"Odd","enabled":true,"sensor_id":null,"nodes":[{"id":1,"kind":"quantum","label":null,"params":{},"x":0.0,"y":0.0}],"connections":[]}"#;

/// `graph_1` is valid and `graph_2` has a node kind of a newer version.
fn store_with_odd_graph(name: &str) -> (Dir, GraphStore) {
    let dir = Dir::new(name);
    let paths = dir.graph_paths();
    std::fs::create_dir_all(paths.graphs.parent().unwrap()).unwrap();
    let valid = serde_json::to_string(&constant_graph("graph_1", 1.0)).unwrap();
    std::fs::write(
        &paths.graphs,
        format!(r#"{{"schema_version":1,"graphs":[{valid},{ODD}]}}"#),
    )
    .unwrap();
    let store = GraphStore::open(paths);
    (dir, store)
}

#[test]
fn put_never_replaces_an_invalid_graph() {
    let (dir, mut store) = store_with_odd_graph("put-odd");
    assert!(store.put(constant_graph("graph_2", 9.0)).is_err());
    assert_eq!(store.entries().len(), 2);
    let text = std::fs::read_to_string(dir.graph_paths().graphs).unwrap();
    assert!(text.contains(ODD), "{text}");
}

#[test]
fn validator_refuses_the_id_of_an_invalid_graph() {
    let (_dir, store) = store_with_odd_graph("validate-odd");
    let refusals = store.validate(&constant_graph("graph_2", 1.0));
    assert!(
        refusals
            .iter()
            .any(|r| r.code == RefusalCode::BadParameter && r.parameter.as_deref() == Some("id")),
        "{refusals:?}"
    );
    // The id of a valid Graph is the stored version of it: no refusal.
    assert!(store.validate(&constant_graph("graph_1", 2.0)).is_empty());
}

#[test]
fn invalid_graph_counts_for_the_limit_of_64_graphs() {
    let dir = Dir::new("limit-odd");
    let paths = dir.graph_paths();
    std::fs::create_dir_all(paths.graphs.parent().unwrap()).unwrap();
    let odd = ODD.replace("graph_2", "graph_100");
    let valid: Vec<String> = (1..=63)
        .map(|n| serde_json::to_string(&constant_graph(&format!("graph_{n}"), 1.0)).unwrap())
        .collect();
    std::fs::write(
        &paths.graphs,
        format!(
            r#"{{"schema_version":1,"graphs":[{},{odd}]}}"#,
            valid.join(",")
        ),
    )
    .unwrap();
    let store = GraphStore::open(paths);
    assert_eq!(store.graphs().len(), 63);
    let refusals = store.validate(&constant_graph("graph_64", 1.0));
    assert!(
        refusals.iter().any(|r| r.code == RefusalCode::LimitGraphs),
        "{refusals:?}"
    );
    // A Graph that is stored already is not a new one.
    assert!(store.validate(&constant_graph("graph_5", 1.0)).is_empty());
}

#[test]
fn a_file_that_broke_after_the_open_is_not_overwritten() {
    let dir = Dir::new("broke-later");
    let paths = dir.graph_paths();
    let mut store = GraphStore::open(paths.clone());
    store.put(constant_graph("graph_1", 1.0)).unwrap();
    std::fs::write(&paths.graphs, "{ not json").unwrap();

    assert!(store.put(constant_graph("graph_2", 2.0)).is_err());
    assert_eq!(
        std::fs::read_to_string(&paths.graphs).unwrap(),
        "{ not json"
    );
    assert!(store.file_error().is_some());
}

#[test]
fn a_remove_edits_the_file_that_another_writer_changed() {
    let dir = Dir::new("other-writer");
    let paths = dir.graph_paths();
    let mut first = GraphStore::open(paths.clone());
    let mut second = GraphStore::open(paths.clone());
    first.put(constant_graph("graph_1", 1.0)).unwrap();
    second.put(constant_graph("graph_2", 2.0)).unwrap();
    // `first` has not read `graph_2`. Its remove of `graph_1` must not bring `graph_1` or lose
    // `graph_2`.
    first.remove("graph_1").unwrap();
    let ids: Vec<String> = GraphStore::open(paths)
        .graphs()
        .into_iter()
        .map(|g| g.id)
        .collect();
    assert_eq!(ids, ["graph_2"]);
}

/// An edit that fails to write is not kept in memory, and the next edit works on the file.
#[test]
fn a_failed_write_leaves_no_edit_behind() {
    let dir = Dir::new("failed-edit");
    let paths = dir.graph_paths();
    let mut store = GraphStore::open(paths.clone());
    store.put(constant_graph("graph_1", 1.0)).unwrap();
    let config = paths.graphs.parent().unwrap().to_owned();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o555)).unwrap();
    let failed = store.put(constant_graph("graph_2", 2.0));
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(failed.is_err());

    store.put(constant_graph("graph_3", 3.0)).unwrap();
    let ids =
        |store: &GraphStore| -> Vec<String> { store.graphs().into_iter().map(|g| g.id).collect() };
    assert_eq!(ids(&store), ["graph_1", "graph_3"]);
    assert_eq!(ids(&GraphStore::open(paths)), ["graph_1", "graph_3"]);
}
