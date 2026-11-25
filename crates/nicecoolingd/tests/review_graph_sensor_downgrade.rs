//! Review reproducer: the ids of the Graph sensors after a downgrade and a new update.

mod support;

use std::fs;
use std::path::Path;

use nicecooling_api::graph_sensor::GraphSensorProxy;
use nicecooling_api::manager::ManagerProxy;
use nicecooling_api::sensor::SensorProxy;
use nicecooling_core::tick::percent_to_raw;
use support::{LEAKSHIELD, Rig, STATE};

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

fn lost_input(rig: &mut Rig) -> bool {
    rig.daemon
        .as_mut()
        .unwrap()
        .runner
        .context()
        .warnings
        .is_active("lost-input", "nct6798_pwm1")
}

fn pwm1_raw(rig: &Rig) -> String {
    fs::read_to_string(rig.fake.attr_path("nct6798", "pwm1"))
        .unwrap()
        .trim()
        .to_owned()
}

/// The state that the older daemon leaves after a downgrade (`state_downgrade.rs`): it drops
/// `graph_sensors` and `graph_sensor_next`, and it keeps the user name of `graph_1`, the Software
/// sensor slot 4 that held `graph_1` and the Curve of `nct6798_pwm1` on `graph_1`.
fn after_downgrade(state: &Path) {
    let text = state_with(&format!(
        r#""names": {{"graph_1": "Mine"}}, "software_sensors": {{"{LEAKSHIELD}": {{"4": "graph_1"}}}}"#
    ));
    fs::write(state.join("state.json"), text).unwrap();
    let profile =
        support::PROFILE.replace("\"input\": \"k10temp_temp1\"", "\"input\": \"graph_1\"");
    assert!(profile.contains("\"graph_1\""));
    fs::write(state.join("profiles/profile_default/profile.json"), profile).unwrap();
}

/// Design, Migration and rollback: "Forward again after a downgrade: the app finds that the Graph
/// sensors of its Graphs do not exist and creates them again with new ids. The user picks the
/// Input of each such Curve again." Spec: "The counter SHALL never give an id twice."
///
/// The older daemon drops the counter, so the counter starts at 1 again and gives `graph_1` a
/// second time. The Curve, the slot and the user name of the old `graph_1` then take the new
/// sensor, which can be another Graph with another unit.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn downgrade_round_trip_gives_new_ids() {
    let mut rig = Rig::start_with_clock_and(|_, state| after_downgrade(state)).await;
    rig.step(2).await;
    assert!(
        lost_input(&mut rig),
        "no graph_1 yet: the Curve has a Lost Input"
    );

    // The app creates the sensor of another Graph first: a load in percent.
    let created = manager(&rig)
        .await
        .create_graph_sensor("Load", "%")
        .await
        .unwrap();
    let id = created.as_str().rsplit('/').next().unwrap().to_owned();
    manager(&rig)
        .await
        .set_graph_sensor_values(&[(id.clone(), true, 75.0)])
        .await
        .unwrap();
    rig.step(1).await;

    let mut problems = Vec::new();
    if id == "graph_1" {
        problems.push("the counter gave graph_1 a second time".to_owned());
    }
    if !lost_input(&mut rig) {
        problems.push(format!(
            "the Curve of nct6798_pwm1 runs on the new sensor {id} (the load 75%): raw {}, not {}",
            pwm1_raw(&rig),
            percent_to_raw(100.0)
        ));
    }
    let name = SensorProxy::builder(&rig.client)
        .path(created.clone())
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
        .name()
        .await
        .unwrap();
    if name != "Load" {
        problems.push(format!(
            "the new sensor shows the name {name:?} of the old one"
        ));
    }
    let graph = GraphSensorProxy::builder(&rig.client)
        .path(created)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    // Slot 4 of the Leakshield held the old graph_1 with the unit code of degC.
    if let Err(e) = graph.set_definition("Load", "rpm").await {
        problems.push(format!(
            "the slot of the old sensor holds the new one, a unit change fails: {e}"
        ));
    }
    let delete = graph.delete().await;
    if let Err(e) = delete {
        problems.push(format!(
            "the new sensor cannot be deleted, a use of the old one holds it: {e}"
        ));
    }
    assert!(problems.is_empty(), "{problems:#?}");
}
