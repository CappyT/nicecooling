//! Review reproducers: the load of `graph_sensors` from a `state.json` that the daemon did not
//! write, for example a file edited by hand.

mod support;

use std::fs;

use nicecooling_api::ids::Kind;
use nicecooling_api::manager::ManagerProxy;
use nicecooling_api::sensor::SensorProxy;
use nicecooling_core::graph_sensor::MAX_GRAPH_SENSORS;
use support::{Rig, STATE, error_name, path};

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";

async fn manager(rig: &Rig) -> ManagerProxy<'static> {
    ManagerProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

/// `STATE` with more keys after `schema_version`.
fn state_with(keys: &str) -> String {
    STATE.replace(
        "\"schema_version\": 1,",
        &format!("\"schema_version\": 1, {keys},"),
    )
}

/// Spec: "At most 64 SHALL exist." The load of `state.json` takes every entry of
/// `graph_sensors`: 10 000 entries, the last two with one id.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn state_load_keeps_the_limit_and_unique_ids() {
    let entries: Vec<String> = (1..=10_000)
        .map(|n| n.min(9_999))
        .map(|n| format!(r#"{{"id": "graph_{n}", "name": "G{n}", "unit": "degC"}}"#))
        .collect();
    let start = std::time::Instant::now();
    let rig = Rig::start_with(move |_, state| {
        let keys = format!(
            "\"graph_sensors\": [{}], \"graph_sensor_next\": 10000",
            entries.join(", ")
        );
        fs::write(state.join("state.json"), state_with(&keys)).unwrap();
    })
    .await;
    eprintln!("start with 10 000 Graph sensors: {:?}", start.elapsed());
    let mut ids = rig.daemon.as_ref().unwrap().hub.graph_sensor_ids();
    let count = ids.len();
    ids.sort();
    ids.dedup();
    let mut problems = Vec::new();
    if count > MAX_GRAPH_SENSORS {
        problems.push(format!("{count} Graph sensors exist after the load"));
    }
    if ids.len() != count {
        problems.push(format!("{} entries share an id", count - ids.len()));
    }
    // The limit holds for a create.
    let e = manager(&rig)
        .await
        .create_graph_sensor("More", "degC")
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
    assert!(problems.is_empty(), "{problems:#?}");
}

/// A `graph_sensors` entry with an id that is not a path element (a state edited by hand). The
/// registration of the objects calls `object()`, which expects a valid id: the daemon must not
/// panic at its start.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn state_with_a_bad_graph_sensor_id_starts() {
    let rig = Rig::start_with(|_, state| {
        let keys = r#""graph_sensors": [{"id": "graph-1", "name": "Bad", "unit": "degC"},
                       {"id": "graph_2", "name": "Good", "unit": "degC"}],
                       "graph_sensor_next": 3"#;
        fs::write(state.join("state.json"), state_with(keys)).unwrap();
    })
    .await;
    let version = manager(&rig).await.version().await.unwrap();
    assert!(!version.is_empty());
    let good = SensorProxy::builder(&rig.client)
        .path(path(Kind::Sensor, "graph_2"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    assert_eq!(good.origin().await.unwrap(), "graph");
}

/// The same check for a Derived sensor id. This path exists before this change: the result tells
/// whether the panic above is a new class or a copy of an existing one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pre_existing_state_with_a_bad_derived_sensor_id_starts() {
    let rig = Rig::start_with(|_, state| {
        let keys = r#""derived_sensors": [{"id": "derived-1", "name": "Bad", "type": "maximum",
                       "sources": ["k10temp_temp1", "nct6798_temp1"]}]"#;
        fs::write(state.join("state.json"), state_with(keys)).unwrap();
    })
    .await;
    let version = manager(&rig).await.version().await.unwrap();
    assert!(!version.is_empty());
}
