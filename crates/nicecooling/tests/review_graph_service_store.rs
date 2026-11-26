//! Review of the Graph store: what happens to `graphs.json`, `graph-state.json` and
//! `secrets.toml` when a file is bad, a write fails, or a path is odd.

mod support;

use std::os::unix::fs::PermissionsExt as _;
use std::time::Duration;

use nicecooling::graphs::secrets::{PasswordEdit, Secrets};
use nicecooling::graphs::store::GraphStore;
use nicecooling_graph::model::StoredGraph;
use support::graphs::{Dir, constant_graph, persistent_graph};

const STATE: &str = r#"{"schema_version":1,"values":{"graph_1/4":7.0},"responses":{"graph_3/5":{"state":"run","last_run_us":1766000000000000}},"pause":{"count":2,"quiet_s":40,"paused":false}}"#;

fn write(path: &std::path::Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

/// `graphs.json` cannot be used (a higher schema version, or a hand edit that broke the JSON).
/// The store keeps no Graph in memory. The stored values and the run state of the Graphs must
/// survive: when the user repairs `graphs.json`, a response whose input stayed true must not run
/// again, and a persistent storage node must keep its value.
fn state_survives(graphs_text: &str, name: &str) {
    let dir = Dir::new(name);
    let paths = dir.graph_paths();
    write(&paths.graphs, graphs_text);
    write(&paths.state, STATE);

    let mut store = GraphStore::open(paths.clone());
    assert!(store.file_error().is_some());
    // The first pass of the service and the stop of the app both call this.
    let _ = store.write_state(Duration::from_secs(1), false);
    let _ = store.write_state(Duration::from_secs(2), true);

    let after: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&paths.state).unwrap()).unwrap();
    assert_eq!(
        after["values"]["graph_1/4"], 7.0,
        "the stored value is gone: {after}"
    );
    assert_eq!(
        after["responses"]["graph_3/5"]["state"], "run",
        "the run state is gone: {after}"
    );
}

#[test]
fn higher_schema_graph_file_keeps_the_state_file() {
    state_survives(r#"{"schema_version":2,"graphs":[]}"#, "rv-state-higher");
}

#[test]
fn unparsable_graph_file_keeps_the_state_file() {
    state_survives("{ not json", "rv-state-unparsable");
}

/// A Graph that parses but breaks a rule of the validator (two end nodes) must keep its stored
/// text when the store writes the file for another Graph.
#[test]
fn validator_invalid_graph_keeps_its_bytes() {
    let dir = Dir::new("rv-invalid-bytes");
    let paths = dir.graph_paths();
    let invalid = r#"{"id":"graph_2","name":"Two ends","enabled":true,"sensor_id":null,"note":"kept by hand","nodes":[{"id":1,"kind":"constant","label":null,"params":{"value":5},"x":0,"y":0},{"id":2,"kind":"sensor_end","label":null,"params":{"unit":"degC"},"x":10,"y":0},{"id":3,"kind":"sensor_end","label":null,"params":{"unit":"degC"},"x":20,"y":0}],"connections":[{"from":{"node":1,"pin":"out"},"to":{"node":2,"pin":"a"}}]}"#;
    write(
        &paths.graphs,
        &format!(r#"{{"schema_version":1,"graphs":[{invalid}]}}"#),
    );

    let mut store = GraphStore::open(paths.clone());
    assert!(matches!(&store.entries()[0], StoredGraph::Invalid(_)));
    store.put(constant_graph("graph_1", 1.0)).unwrap();
    let text = std::fs::read_to_string(&paths.graphs).unwrap();
    assert!(
        text.contains(invalid),
        "the text of the invalid Graph changed:\n{text}"
    );
}

/// A write into a read-only directory fails, and the Graphs in memory stay as in the file.
#[test]
fn read_only_directory_keeps_memory_and_file() {
    let dir = Dir::new("rv-read-only");
    let paths = dir.graph_paths();
    let mut store = GraphStore::open(paths.clone());
    store.put(constant_graph("graph_1", 1.0)).unwrap();
    let before = std::fs::read_to_string(&paths.graphs).unwrap();
    let config = paths.graphs.parent().unwrap().to_owned();
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o555)).unwrap();

    let result = store.put(constant_graph("graph_2", 2.0));
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err());
    assert_eq!(store.graphs().len(), 1);
    assert_eq!(std::fs::read_to_string(&paths.graphs).unwrap(), before);
}

/// The directories do not exist yet: the first write creates them.
#[test]
fn missing_directories_are_created() {
    let dir = Dir::new("rv-missing-dirs");
    let paths = nicecooling::graphs::store::GraphPaths::in_dirs(
        &dir.path().join("a/b/config"),
        &dir.path().join("c/d/data"),
    );
    let mut store = GraphStore::open(paths.clone());
    store.put(persistent_graph("graph_1")).unwrap();
    store.state_mut().values.insert("graph_1/4".into(), 1.0);
    assert!(store.write_state(Duration::ZERO, true).unwrap());
    assert!(paths.graphs.is_file() && paths.state.is_file());
}

/// A crash left a temporary file with a wide mode, or a symbolic link, at the name of the
/// temporary file of `secrets.toml`. The next write gives a file of mode 0600 and leaves the
/// target of the link alone.
#[test]
fn secrets_write_over_a_left_temporary_file() {
    let dir = Dir::new("rv-secrets-tmp");
    let path = dir.graph_paths().secrets;
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let victim = dir.path().join("victim");
    std::fs::write(&victim, "keep").unwrap();
    std::os::unix::fs::symlink(&victim, path.with_file_name("secrets.toml.tmp")).unwrap();

    let mut secrets = Secrets::open(&path);
    secrets.edit_smtp(PasswordEdit::Set("hunter2".into()));
    secrets.save().unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
    // The link is a file of the user: the write leaves it where it is.
    assert!(path.with_file_name("secrets.toml.tmp").is_symlink());
}

/// No error text of the secrets file holds the password: a file that does not parse, and a write
/// that fails.
#[test]
fn password_is_in_no_error_text() {
    let dir = Dir::new("rv-secrets-errors");
    let path = dir.graph_paths().secrets;
    write(&path, "schema_version = 1\n[smtp]\npassword = \"hunter2\n");
    let secrets = Secrets::open(&path);
    let error = secrets.error.clone().expect("an error");
    assert!(!error.contains("hunter2"), "{error}");
    assert!(!format!("{secrets:?}").contains("hunter2"));

    // The directory of the file is a file: the write fails.
    let other = Dir::new("rv-secrets-errors-2");
    write(&other.path().join("blocker"), "");
    let mut blocked = Secrets::open(&other.path().join("blocker/secrets.toml"));
    blocked.edit_smtp(PasswordEdit::Set("hunter2".into()));
    let error = blocked.save().expect_err("the write fails").to_string();
    assert!(!error.contains("hunter2"), "{error}");
}

/// A file write that fails in the middle (the disk is full). The process runs under a file size
/// limit of 4 KiB with SIGXFSZ ignored, so a write past the limit fails with EFBIG. The old
/// `graphs.json` must stay complete and the Graphs in memory must stay as in the file.
#[test]
fn full_disk_keeps_the_old_graph_file() {
    if std::env::var_os("REVIEW_FULL_DISK_CHILD").is_some() {
        let dir = std::path::PathBuf::from(std::env::var_os("REVIEW_DIR").unwrap());
        let paths = nicecooling::graphs::store::GraphPaths::in_dirs(&dir, &dir);
        let mut store = GraphStore::open(paths.clone());
        assert_eq!(store.graphs().len(), 1);
        let mut big = constant_graph("graph_2", 2.0);
        big.name = "x".repeat(60);
        for id in 3..200 {
            big.nodes.push(nicecooling_graph::model::Node::new(
                id,
                nicecooling_graph::model::Kind::Comment,
            ));
        }
        assert!(store.put(big).is_err(), "the write must fail");
        assert_eq!(store.graphs().len(), 1);
        return;
    }
    let dir = Dir::new("rv-full-disk");
    let paths = nicecooling::graphs::store::GraphPaths::in_dirs(dir.path(), dir.path());
    let mut store = GraphStore::open(paths.clone());
    store.put(constant_graph("graph_1", 1.0)).unwrap();
    let before = std::fs::read_to_string(&paths.graphs).unwrap();
    assert!(before.len() < 4096);

    let exe = std::env::current_exe().unwrap();
    let output = std::process::Command::new("sh")
        .arg("-c")
        .arg("trap '' XFSZ; exec prlimit --fsize=4096 \"$0\" --exact full_disk_keeps_the_old_graph_file --test-threads 1")
        .arg(&exe)
        .env("REVIEW_FULL_DISK_CHILD", "1")
        .env("REVIEW_DIR", dir.path())
        // The size limit also applies to the output of the child: a log file as stdout fails it.
        .output()
        .expect("the child test");
    assert!(
        output.status.success(),
        "the child test failed: {}\n{}{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(std::fs::read_to_string(&paths.graphs).unwrap(), before);
    let store = GraphStore::open(paths);
    assert_eq!(store.error(), None);
    assert_eq!(store.graphs().len(), 1);
}

/// A symbolic link at the path of `graphs.json` (a dotfile manager). The write gives a complete
/// file at the path, and the target of the link is not corrupted. Whether the link stays a link
/// is a reading that the spec does not state: the test prints it.
#[test]
fn symlinked_graph_file_is_not_corrupted() {
    let dir = Dir::new("rv-symlink");
    let paths = dir.graph_paths();
    std::fs::create_dir_all(paths.graphs.parent().unwrap()).unwrap();
    let target = dir.path().join("dotfiles-graphs.json");
    std::fs::write(&target, r#"{"schema_version":1,"graphs":[]}"#).unwrap();
    std::os::unix::fs::symlink(&target, &paths.graphs).unwrap();

    let mut store = GraphStore::open(paths.clone());
    store.put(constant_graph("graph_1", 1.0)).unwrap();
    let link = std::fs::symlink_metadata(&paths.graphs).unwrap();
    eprintln!(
        "graphs.json is a link after the write: {}; the target holds graph_1: {}",
        link.file_type().is_symlink(),
        std::fs::read_to_string(&target)
            .unwrap()
            .contains("graph_1")
    );
    assert!(GraphStore::open(paths).graph("graph_1").is_some());
    let target_text = std::fs::read_to_string(&target).unwrap();
    assert!(serde_json::from_str::<serde_json::Value>(&target_text).is_ok());
}

/// The app stores at most 64 Graphs. A Graph that this version cannot read (an invalid Graph)
/// is still a Graph of the file, so 63 valid Graphs, one invalid Graph and a new Graph are 65.
#[test]
fn invalid_graph_counts_for_the_limit_of_64() {
    let dir = Dir::new("rv-limit-64");
    let paths = dir.graph_paths();
    let odd = r#"{"id":"graph_100","name":"Odd","enabled":true,"sensor_id":null,"nodes":[{"id":1,"kind":"quantum","label":null,"params":{},"x":0.0,"y":0.0}],"connections":[]}"#;
    let valid: Vec<String> = (1..=63)
        .map(|n| serde_json::to_string(&constant_graph(&format!("graph_{n}"), 1.0)).unwrap())
        .collect();
    write(
        &paths.graphs,
        &format!(
            r#"{{"schema_version":1,"graphs":[{},{odd}]}}"#,
            valid.join(",")
        ),
    );
    let store = GraphStore::open(paths);
    assert_eq!(store.entries().len(), 64);
    let refusals = store.validate(&constant_graph("graph_64", 1.0));
    assert!(
        refusals
            .iter()
            .any(|r| r.code == nicecooling_graph::validate::RefusalCode::LimitGraphs),
        "the 65th Graph of the file passes: {refusals:?}"
    );
}
