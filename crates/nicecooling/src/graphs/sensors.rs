//! The Graph sensors of the Graphs: which sensor a Graph needs, and the comparison of the Graphs
//! with the Graph sensors that the daemon holds.
//!
//! The calls themselves are in `values.rs`, and the service makes them at Apply, at Enable, at
//! Delete and at each new snapshot of the daemon.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use nicecooling_graph::model::{Graph, StoredGraph};

use super::view::ForeignSensor;
use crate::model::SensorInfo;

/// How long the service counts a sensor that it created as present while no snapshot lists it.
/// A snapshot that the daemon client read before the call can arrive after the call.
pub const CREATED_GRACE: Duration = Duration::from_secs(30);

/// The name and the unit that the daemon holds for the sensor of a Graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SensorSpec {
    pub name: String,
    pub unit: String,
}

/// The sensor of a Graph with a sensor end node: the name of the Graph and the unit of the node.
/// A Graph with a response end node has none.
pub fn spec_of(graph: &Graph) -> Option<SensorSpec> {
    let end = graph.sensor_end()?;
    Some(SensorSpec {
        name: graph.name.clone(),
        unit: end.text("unit"),
    })
}

/// The sensor ids that the stored Graphs hold. A Graph that does not parse can hold one too: its
/// stored text says which.
pub fn held_ids(entries: &[StoredGraph]) -> BTreeSet<String> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            StoredGraph::Valid(graph) => graph.sensor_id.clone(),
            StoredGraph::Invalid(invalid) => {
                let value: serde_json::Value = serde_json::from_str(invalid.text.get()).ok()?;
                value.get("sensor_id")?.as_str().map(str::to_owned)
            }
        })
        .collect()
}

/// The ids of the Graphs that hold a sensor which the daemon does not list. `listed` is the sensors
/// of the newest snapshot, and `created` the ids that the service made and no snapshot lists yet.
pub fn missing(
    graphs: &[Graph],
    listed: &BTreeMap<String, SensorInfo>,
    created: &BTreeMap<String, Duration>,
) -> Vec<String> {
    graphs
        .iter()
        .filter(|graph| graph.sensor_end().is_some())
        .filter(|graph| {
            graph
                .sensor_id
                .as_ref()
                .is_some_and(|id| !listed.contains_key(id) && !created.contains_key(id))
        })
        .map(|graph| graph.id.clone())
        .collect()
}

/// The Graph sensors of the daemon that no Graph holds. They can belong to the app of another
/// user, so the app leaves them and lists them.
pub fn foreign(
    entries: &[StoredGraph],
    listed: &BTreeMap<String, SensorInfo>,
) -> Vec<ForeignSensor> {
    let held = held_ids(entries);
    listed
        .values()
        .filter(|sensor| sensor.origin == "graph" && !held.contains(&sensor.id))
        .map(|sensor| ForeignSensor {
            id: sensor.id.clone(),
            name: sensor.default_name.clone(),
            unit: sensor.unit.clone(),
        })
        .collect()
}

/// Whether the daemon holds another name or unit for the sensor, or none.
pub fn differs(listed: &BTreeMap<String, SensorInfo>, id: &str, spec: &SensorSpec) -> bool {
    listed
        .get(id)
        .is_none_or(|info| info.default_name != spec.name || info.unit != spec.unit)
}
