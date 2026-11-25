//! Review checks: the authorization order of the four Graph sensor members, the in-use rule for a
//! Curve of a Profile that is not active, and the Derived sensors at the update.

mod support;

use std::fs;
use std::sync::atomic::Ordering;

use nicecooling_api::channel::ChannelProxy;
use nicecooling_api::graph_sensor::GraphSensorProxy;
use nicecooling_api::ids::Kind;
use nicecooling_api::manager::ManagerProxy;
use nicecooling_api::profiles::ProfilesProxy;
use support::{Rig, STATE, error_name, path};

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";
const WRITE: &str = "io.github.cappyt.NiceCooling1.write";
const DENIED: &str = "org.freedesktop.DBus.Error.AccessDenied";

fn running_as_root() -> bool {
    fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .is_some_and(|l| l.split_whitespace().next() == Some("0"))
}

async fn manager(rig: &Rig) -> ManagerProxy<'static> {
    ManagerProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

async fn graph(rig: &Rig, id: &str) -> GraphSensorProxy<'static> {
    GraphSensorProxy::builder(&rig.client)
        .path(path(Kind::Sensor, id))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

async fn pwm1(rig: &Rig) -> ChannelProxy<'static> {
    ChannelProxy::builder(&rig.client)
        .path(path(Kind::Channel, "nct6798_pwm1"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

/// A denied caller gets `AccessDenied` before any check of its arguments: a value that is not
/// finite, a unit outside the table, a name that is too long, a delete of a sensor in use and an
/// empty values call. Each call asks polkit once, for `write`, without the interactive flag.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn denied_caller_learns_nothing_from_validation() {
    if running_as_root() {
        return;
    }
    let mut rig = Rig::start_with_polkit().await;
    rig.tick_in_background();
    let manager = manager(&rig).await;
    manager.create_graph_sensor("Water", "degC").await.unwrap();
    pwm1(&rig)
        .await
        .set_curve(
            &path(Kind::Sensor, "graph_1"),
            &[(40.0, 20.0), (80.0, 100.0)],
            0.0,
            100.0,
        )
        .await
        .unwrap();
    let polkit = rig.polkit.as_ref().unwrap();
    polkit.deny.store(true, Ordering::SeqCst);
    polkit.calls.lock().unwrap().clear();

    let graph = graph(&rig, "graph_1").await;
    let results = [
        manager
            .set_graph_sensor_values(&[("graph_1".to_owned(), true, f64::NAN)])
            .await,
        manager
            .create_graph_sensor("Water", "bar")
            .await
            .map(|_| ()),
        graph.set_definition(&"n".repeat(65), "degC").await,
        graph.delete().await,
        manager.set_graph_sensor_values(&[]).await,
    ];
    for result in &results {
        assert_eq!(error_name(result.as_ref().unwrap_err()), DENIED);
    }
    let calls = polkit.calls.lock().unwrap().clone();
    assert_eq!(calls, vec![(WRITE.to_owned(), 0); 5]);
}

/// Spec: the values call never allows a polkit dialog. The daemon asks polkit at each call without
/// the interactive flag, whatever flag the message carries.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn values_call_never_asks_polkit_with_the_dialog_flag() {
    if running_as_root() {
        return;
    }
    let mut rig = Rig::start_with_polkit().await;
    rig.tick_in_background();
    let raw = zbus::Proxy::new(
        &rig.client,
        nicecooling_api::BUS_NAME,
        support::root(),
        "io.github.cappyt.NiceCooling1.Manager",
    )
    .await
    .unwrap();
    let entries: Vec<(String, bool, f64)> = Vec::new();
    rig.polkit.as_ref().unwrap().calls.lock().unwrap().clear();
    for interactive in [false, true, false, true, false] {
        if interactive {
            raw.call_with_flags::<_, _, ()>(
                "SetGraphSensorValues",
                zbus::proxy::MethodFlags::ALLOW_INTERACTIVE_AUTH,
                &(entries.clone(),),
            )
            .await
            .unwrap();
        } else {
            raw.call::<_, _, ()>("SetGraphSensorValues", &(entries.clone(),))
                .await
                .unwrap();
        }
    }
    let flags: Vec<u32> = rig
        .polkit
        .as_ref()
        .unwrap()
        .calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.1)
        .collect();
    assert_eq!(flags, [0; 5]);
}

/// Spec: the delete of a Graph sensor that a Curve uses fails with `InUse`. The Curve is in a
/// Profile that is not active.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn curve_in_an_inactive_profile_keeps_the_sensor() {
    let rig = Rig::start().await;
    manager(&rig)
        .await
        .create_graph_sensor("Water", "degC")
        .await
        .unwrap();
    let pwm1 = pwm1(&rig).await;
    pwm1.set_curve(
        &path(Kind::Sensor, "graph_1"),
        &[(40.0, 20.0), (80.0, 100.0)],
        0.0,
        100.0,
    )
    .await
    .unwrap();
    let profiles = ProfilesProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let copy = profiles
        .create_profile("Copy", &path(Kind::Profile, "profile_default"))
        .await
        .unwrap();
    // The active Default Profile no longer reads the sensor. The copy does.
    pwm1.set_fixed_duty(50.0).await.unwrap();
    let e = graph(&rig, "graph_1").await.delete().await.unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InUse"));
    let text = format!("{e}");
    let copy_id = copy.as_str().rsplit('/').next().unwrap();
    assert!(text.contains(copy_id), "{text}");
}

/// Spec, Derived sensors unchanged: a daemon that starts with 3 stored Derived sensors has the
/// same 3 with the same ids, no Graph sensor, and `CreateDerivedSensor` works as before.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stored_derived_sensors_stay_after_the_update() {
    let rig = Rig::start_with(|_, state| {
        let derived = r#""derived_sensors": [
            {"id": "derived_1", "name": "Hottest", "type": "maximum",
             "sources": ["k10temp_temp1", "nct6798_temp1"]},
            {"id": "derived_2", "name": "Mean", "type": "mean",
             "sources": ["k10temp_temp1", "nct6798_temp1"]},
            {"id": "derived_4", "name": "Delta", "type": "difference",
             "sources": ["k10temp_temp1", "nct6798_temp1"]}]"#;
        let text = STATE.replace(
            "\"schema_version\": 1,",
            &format!("\"schema_version\": 1, {derived},"),
        );
        fs::write(state.join("state.json"), text).unwrap();
    })
    .await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let mut ids = hub.derived_ids();
    ids.sort();
    assert_eq!(ids, ["derived_1", "derived_2", "derived_4"]);
    assert!(hub.graph_sensor_ids().is_empty());
    let created = manager(&rig)
        .await
        .create_derived_sensor(
            "More",
            "maximum",
            &[
                path(Kind::Sensor, "k10temp_temp1"),
                path(Kind::Sensor, "nct6798_temp1"),
            ],
        )
        .await
        .unwrap();
    assert_eq!(created, path(Kind::Sensor, "derived_5"));
    assert!(hub.graph_sensor_ids().is_empty());
}
