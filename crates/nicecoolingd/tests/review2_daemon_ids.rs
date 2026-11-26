//! Review 2, stored ids. A state file made by hand, or by a newer or an older daemon.

mod support;

use std::fs;
use std::time::Duration;

use nicecooling_api::ids::Kind;
use nicecooling_api::manager::ManagerProxy;
use support::{Rig, STATE, path};

fn state_with(keys: &str) -> String {
    STATE.replace(
        "\"schema_version\": 1,",
        &format!("\"schema_version\": 1, {keys},"),
    )
}

const LARGEST_DERIVED: &str = r#""derived_sensors": [{"id": "derived_4294967295", "name": "Top",
    "type": "maximum", "sources": ["k10temp_temp1", "nct6798_temp1"]}]"#;

/// `ids::next_number` adds 1 to the largest stored number without a check. With
/// `derived_4294967295` in `state.json` (a valid identifier, so the load keeps it), a debug build
/// panics in `CreateDerivedSensor`, and a release build wraps to `derived_0` twice.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_derived_sensor_at_the_largest_number_gives_unique_new_ids() {
    let rig = Rig::start_with(|_, state| {
        fs::write(state.join("state.json"), state_with(LARGEST_DERIVED)).unwrap();
    })
    .await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let sources = ["k10temp_temp1".to_owned(), "nct6798_temp1".to_owned()];
    let first = hub.create_derived("A", "maximum", &sources);
    let second = hub.create_derived("B", "maximum", &sources);
    match (first, second) {
        (Ok(a), Ok(b)) => assert_ne!(a, b, "two Derived sensors have one id"),
        (a, b) => {
            // A refusal is a valid answer: the family has no number left.
            assert!(a.is_err() || b.is_err());
        }
    }
}

/// The same call over the bus: the method call must get an answer.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_derived_sensor_answers_after_a_stored_largest_id() {
    let rig = Rig::start_with(|_, state| {
        fs::write(state.join("state.json"), state_with(LARGEST_DERIVED)).unwrap();
    })
    .await;
    let manager = ManagerProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let sources = [
        path(Kind::Sensor, "k10temp_temp1"),
        path(Kind::Sensor, "nct6798_temp1"),
    ];
    let answer = tokio::time::timeout(
        Duration::from_secs(5),
        manager.create_derived_sensor("A", "maximum", &sources),
    )
    .await;
    let version = tokio::time::timeout(Duration::from_secs(5), manager.version()).await;
    assert!(
        answer.is_ok() && matches!(version, Ok(Ok(_))),
        "CreateDerivedSensor answered: {}, Version after it: {version:?}",
        answer.is_ok()
    );
}

const CHILD_STATE: &str = "NICECOOLING_REVIEW2_CHILD_STATE";

/// The child half of the test below: it loads the state directory that the parent made.
#[test]
fn review2_child_load_state() {
    let Some(dir) = std::env::var_os(CHILD_STATE) else {
        return;
    };
    let store = nicecoolingd::state::store::StateStore::new(dir);
    let loaded = nicecoolingd::state::load_state(&store);
    assert!(loaded.state.graph_sensors.is_empty());
}

/// 10 000 Graph sensors with a bad id give one journal line each. The daemon runs under systemd,
/// so each line is a journal entry at each start.
#[test]
fn ten_thousand_bad_graph_sensor_ids_do_not_flood_the_journal() {
    let dir =
        std::env::temp_dir().join(format!("nicecooling-review2-flood-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let entries: Vec<String> = (0..10_000)
        .map(|n| format!(r#"{{"id": "graph-{n}", "name": "Bad", "unit": "degC"}}"#))
        .collect();
    let keys = format!("\"graph_sensors\": [{}]", entries.join(","));
    fs::write(dir.join("state.json"), state_with(&keys)).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "review2_child_load_state",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(CHILD_STATE, &dir)
        .output()
        .unwrap();
    let _ = fs::remove_dir_all(&dir);
    assert!(output.status.success(), "the child failed: {output:?}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines = stderr
        .lines()
        .filter(|l| l.contains("dropped the Graph sensor"))
        .count();
    assert!(
        lines < 10,
        "the load wrote {lines} journal lines for the dropped Graph sensors"
    );
}

/// Spec (node-graph, "Graph sensor id after a downgrade"): a new id is above every `graph_<n>`
/// that the state still names. `take_number` reads `graph_+4294967295` (a sign, a key that the
/// daemon never writes) as the largest number, so no Graph sensor can be created any more.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_user_name_key_with_a_sign_does_not_use_up_the_graph_sensor_ids() {
    let rig = Rig::start_with(|_, state| {
        let keys = r#""names": {"graph_+4294967295": "Hand made"}"#;
        fs::write(state.join("state.json"), state_with(keys)).unwrap();
    })
    .await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let created = hub.create_graph_sensor("Water", "degC");
    assert!(created.is_ok(), "{created:?}");
}
