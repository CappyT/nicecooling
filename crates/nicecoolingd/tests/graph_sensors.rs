//! The Graph sensor in the hub and on the bus: ids, units, definition, delete and the limit.

mod support;

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use nicecooling_api::channel::ChannelProxy;
use nicecooling_api::derived_sensor::DerivedSensorProxy;
use nicecooling_api::graph_sensor::GraphSensorProxy;
use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_api::loops::LoopProxy;
use nicecooling_api::manager::ManagerProxy;
use nicecooling_api::profiles::ProfilesProxy;
use nicecooling_api::sensor::SensorProxy;
use support::{LEAKSHIELD, Rig, STATE, error_name, path};
use tokio::time::timeout;
use zbus::fdo::ObjectManagerProxy;
use zbus::{OwnedObjectPath, Variant};

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";

async fn manager(rig: &Rig) -> ManagerProxy<'static> {
    ManagerProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

async fn create(rig: &Rig, name: &str, unit: &str) -> OwnedObjectPath {
    manager(rig)
        .await
        .create_graph_sensor(name, unit)
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

async fn sensor(rig: &Rig, id: &str) -> SensorProxy<'static> {
    SensorProxy::builder(&rig.client)
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

async fn leakshield(rig: &Rig) -> LeakshieldProxy<'static> {
    LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

/// The text of the error of a failed call.
fn error_text(error: &zbus::Error) -> String {
    match error {
        zbus::Error::MethodError(_, Some(text), _) => text.clone(),
        other => format!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn first_sensor_is_graph_1() {
    let rig = Rig::start().await;
    let created = create(&rig, "Hot water", "degC").await;
    assert_eq!(created, path(Kind::Sensor, "graph_1"));
    assert_eq!(
        created.as_str(),
        "/io/github/cappyt/NiceCooling1/sensor/graph_1"
    );
    let proxy = sensor(&rig, "graph_1").await;
    assert_eq!(proxy.id().await.unwrap(), "graph_1");
    assert_eq!(proxy.origin().await.unwrap(), "graph");
    assert_eq!(proxy.name().await.unwrap(), "Hot water");
    assert_eq!(proxy.default_name().await.unwrap(), "Hot water");
    assert!(!proxy.has_value().await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_returns_graph_5_path() {
    let rig = Rig::start_with(|_, state| {
        let text = STATE.replace(
            "\"schema_version\": 1,",
            "\"schema_version\": 1, \"graph_sensor_next\": 5,",
        );
        std::fs::write(state.join("state.json"), text).unwrap();
    })
    .await;
    assert_eq!(
        create(&rig, "A", "degC").await,
        path(Kind::Sensor, "graph_5")
    );
    // The counter is 6 now.
    assert_eq!(
        create(&rig, "B", "degC").await,
        path(Kind::Sensor, "graph_6")
    );
    // An id after a delete is not used again.
    graph(&rig, "graph_6").await.delete().await.unwrap();
    assert_eq!(
        create(&rig, "C", "degC").await,
        path(Kind::Sensor, "graph_7")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unit_gives_the_kind() {
    let rig = Rig::start().await;
    for (n, (unit, kind)) in [
        ("degC", "temperature"),
        ("rpm", "fan"),
        ("%", "other"),
        ("V", "voltage"),
        ("A", "other"),
        ("W", "other"),
        ("l/h", "flow"),
        ("mbar", "pressure"),
    ]
    .into_iter()
    .enumerate()
    {
        create(&rig, "Sensor", unit).await;
        let proxy = sensor(&rig, &format!("graph_{}", n + 1)).await;
        assert_eq!(proxy.unit().await.unwrap(), unit);
        assert_eq!(proxy.kind().await.unwrap(), kind, "{unit}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unit_not_allowed_is_refused() {
    let rig = Rig::start().await;
    for unit in ["bar", "", "mbar/min", "DEGC"] {
        let e = manager(&rig)
            .await
            .create_graph_sensor("Pressure", unit)
            .await
            .unwrap_err();
        assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"), "{unit:?}");
    }
    // The refusals took no number.
    assert_eq!(
        create(&rig, "Ok", "mbar").await,
        path(Kind::Sensor, "graph_1")
    );
    let e = graph(&rig, "graph_1")
        .await
        .set_definition("Ok", "bar")
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_definition_keeps_the_id_and_replaces_the_object() {
    let rig = Rig::start().await;
    let objects = ObjectManagerProxy::builder(&rig.client)
        .destination(nicecooling_api::BUS_NAME)
        .path(nicecooling_api::ROOT_PATH)
        .build()
        .await
        .unwrap();
    create(&rig, "Water", "degC").await;
    let mut added = objects.receive_interfaces_added().await.unwrap();
    let mut removed = objects.receive_interfaces_removed().await.unwrap();

    graph(&rig, "graph_1")
        .await
        .set_definition("Flow", "l/h")
        .await
        .unwrap();

    let gone = timeout(Duration::from_secs(5), removed.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        gone.args().unwrap().object_path,
        path(Kind::Sensor, "graph_1").as_ref()
    );
    // The two interfaces come back, one signal for each.
    let mut interfaces = Vec::new();
    while interfaces.len() < 2 {
        let back = timeout(Duration::from_secs(5), added.next())
            .await
            .unwrap()
            .unwrap();
        let args = back.args().unwrap();
        assert_eq!(args.object_path, path(Kind::Sensor, "graph_1").as_ref());
        interfaces.extend(args.interfaces_and_properties.keys().map(|k| k.to_string()));
    }
    interfaces.sort();
    assert_eq!(
        interfaces,
        [
            "io.github.cappyt.NiceCooling1.GraphSensor",
            "io.github.cappyt.NiceCooling1.Sensor"
        ]
    );
    let proxy = sensor(&rig, "graph_1").await;
    assert_eq!(proxy.id().await.unwrap(), "graph_1");
    assert_eq!(proxy.name().await.unwrap(), "Flow");
    assert_eq!(proxy.unit().await.unwrap(), "l/h");
    assert_eq!(proxy.kind().await.unwrap(), "flow");
    // The new object takes calls again.
    graph(&rig, "graph_1").await.delete().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_name_wins_over_the_default_name() {
    let rig = Rig::start().await;
    let objects = ObjectManagerProxy::builder(&rig.client)
        .destination(nicecooling_api::BUS_NAME)
        .path(nicecooling_api::ROOT_PATH)
        .build()
        .await
        .unwrap();
    create(&rig, "Water", "degC").await;
    sensor(&rig, "graph_1")
        .await
        .set_name("Mine")
        .await
        .unwrap();
    let mut added = objects.receive_interfaces_added().await.unwrap();
    graph(&rig, "graph_1")
        .await
        .set_definition("Hot water", "degC")
        .await
        .unwrap();
    timeout(Duration::from_secs(5), added.next())
        .await
        .unwrap()
        .unwrap();
    let proxy = sensor(&rig, "graph_1").await;
    assert_eq!(proxy.name().await.unwrap(), "Mine");
    assert_eq!(proxy.default_name().await.unwrap(), "Hot water");
    // An empty name gives the default name back.
    proxy.set_name("").await.unwrap();
    assert_eq!(proxy.name().await.unwrap(), "Hot water");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unit_change_in_a_slot_is_in_use() {
    let rig = Rig::start().await;
    let sensor_path = create(&rig, "Water", "degC").await;
    leakshield(&rig)
        .await
        .set_software_sensor(4, &sensor_path)
        .await
        .unwrap();
    let proxy = graph(&rig, "graph_1").await;
    let e = proxy.set_definition("Water", "l/h").await.unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InUse"));
    assert!(error_text(&e).contains("slot 4"), "{}", error_text(&e));
    assert_eq!(sensor(&rig, "graph_1").await.unit().await.unwrap(), "degC");
    // A sensor in no slot takes the new unit.
    leakshield(&rig)
        .await
        .set_software_sensor(4, &support::none_path())
        .await
        .unwrap();
    proxy.set_definition("Water", "l/h").await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_in_use_by_curve() {
    let rig = Rig::start().await;
    let input = create(&rig, "Water", "degC").await;
    let pwm1 = pwm1(&rig).await;
    pwm1.set_curve(&input, &[(20.0, 10.0), (40.0, 50.0)], 0.0, 100.0)
        .await
        .unwrap();
    let proxy = graph(&rig, "graph_1").await;
    let e = proxy.delete().await.unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InUse"));
    assert!(
        error_text(&e).contains("nct6798_pwm1"),
        "{}",
        error_text(&e)
    );
    // The sensor stays.
    assert_eq!(
        sensor(&rig, "graph_1").await.origin().await.unwrap(),
        "graph"
    );
    pwm1.set_fixed_duty(50.0).await.unwrap();
    proxy.delete().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_in_use_by_rule() {
    let rig = Rig::start().await;
    let input = create(&rig, "Water", "degC").await;
    let profiles = ProfilesProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let default = path(Kind::Profile, "profile_default");
    let target = profiles.create_profile("Hot", &default).await.unwrap();
    let rule = (
        "sensor",
        HashMap::from([
            ("sensor", Variant::from(input.clone())),
            ("comparison", Variant::from("gt")),
            ("value", Variant::F64(50.0)),
            ("hold_s", Variant::U32(10)),
        ]),
    );
    profiles
        .set_rule_groups(vec![("", &target, true, vec![rule])])
        .await
        .unwrap();
    let proxy = graph(&rig, "graph_1").await;
    let e = proxy.delete().await.unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InUse"));
    assert!(error_text(&e).contains("group_1"), "{}", error_text(&e));
    profiles.set_rule_groups(vec![]).await.unwrap();
    proxy.delete().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_in_use_by_software_sensor_slot() {
    let rig = Rig::start().await;
    let input = create(&rig, "Water", "degC").await;
    let device = leakshield(&rig).await;
    device.set_software_sensor(3, &input).await.unwrap();
    let proxy = graph(&rig, "graph_1").await;
    let e = proxy.delete().await.unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InUse"));
    assert!(error_text(&e).contains("slot 3"), "{}", error_text(&e));
    device
        .set_software_sensor(3, &support::none_path())
        .await
        .unwrap();
    proxy.delete().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn derived_sensor_in_a_slot_cannot_be_deleted() {
    let rig = Rig::start().await;
    let derived = manager(&rig)
        .await
        .create_derived_sensor("Hottest", "maximum", &[path(Kind::Sensor, "k10temp_temp1")])
        .await
        .unwrap();
    let device = leakshield(&rig).await;
    device.set_software_sensor(5, &derived).await.unwrap();
    let proxy = DerivedSensorProxy::builder(&rig.client)
        .path(derived)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let e = proxy.delete().await.unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InUse"));
    assert!(error_text(&e).contains("slot 5"), "{}", error_text(&e));
    device
        .set_software_sensor(5, &support::none_path())
        .await
        .unwrap();
    proxy.delete().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loop_with_a_calibrated_flow_in_a_slot_cannot_be_deleted() {
    let rig = Rig::start().await;
    let flow = path(Kind::Sensor, "loop_1_flow");
    let device = leakshield(&rig).await;
    device.set_software_sensor(6, &flow).await.unwrap();
    let loop_1 = LoopProxy::builder(&rig.client)
        .path(path(Kind::Loop, "loop_1"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let e = loop_1.delete().await.unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InUse"));
    assert!(error_text(&e).contains("slot 6"), "{}", error_text(&e));
    device
        .set_software_sensor(6, &support::none_path())
        .await
        .unwrap();
    loop_1.delete().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sixty_fifth_sensor_is_refused() {
    let rig = Rig::start().await;
    for n in 1..=64 {
        let created = create(&rig, "Sensor", "degC").await;
        assert_eq!(created, path(Kind::Sensor, &format!("graph_{n}")));
    }
    let e = manager(&rig)
        .await
        .create_graph_sensor("One more", "degC")
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
    // A delete makes room, and the new sensor takes the next number.
    graph(&rig, "graph_10").await.delete().await.unwrap();
    assert_eq!(
        create(&rig, "Again", "degC").await,
        path(Kind::Sensor, "graph_65")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restart_keeps_sensors_and_counter() {
    let mut rig = Rig::start().await;
    create(&rig, "Water", "degC").await;
    create(&rig, "Flow", "l/h").await;
    create(&rig, "Gone", "W").await;
    graph(&rig, "graph_3").await.delete().await.unwrap();
    sensor(&rig, "graph_2")
        .await
        .set_name("Mine")
        .await
        .unwrap();

    rig.restart_daemon().await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let mut ids = hub.graph_sensor_ids();
    ids.sort();
    assert_eq!(ids, ["graph_1", "graph_2"]);
    let two = hub.sensor("graph_2").unwrap();
    assert_eq!(two.origin, "graph");
    assert_eq!(two.unit, "l/h");
    assert_eq!(two.kind, "flow");
    assert_eq!(two.name, "Mine");
    assert_eq!(two.default_name, "Flow");
    assert_eq!(two.value, None);
    // The counter stays: graph_3 is not used again.
    assert_eq!(hub.create_graph_sensor("New", "degC").unwrap(), "graph_4");
}
