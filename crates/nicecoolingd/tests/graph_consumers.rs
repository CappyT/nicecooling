//! Where a Graph sensor can go: the Input of a Curve, a sensor Profile rule and a Software sensor
//! slot, and where it cannot: the source of a Derived sensor, and the flow or water temperature
//! sensor of a Loop.

mod support;

use std::collections::HashMap;

use nicecooling_api::channel::ChannelProxy;
use nicecooling_api::derived_sensor::DerivedSensorProxy;
use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_api::loops::LoopProxy;
use nicecooling_api::manager::ManagerProxy;
use nicecooling_api::profiles::ProfilesProxy;
use support::{LEAKSHIELD, Rig, error_name, path};
use zbus::{OwnedObjectPath, Variant};

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";

async fn manager(rig: &Rig) -> ManagerProxy<'static> {
    ManagerProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

/// A Graph sensor `graph_1` in degrees Celsius.
async fn graph_sensor(rig: &Rig) -> OwnedObjectPath {
    manager(rig)
        .await
        .create_graph_sensor("Water", "degC")
        .await
        .unwrap()
}

async fn loop_1(rig: &Rig) -> LoopProxy<'static> {
    LoopProxy::builder(&rig.client)
        .path(path(Kind::Loop, "loop_1"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn curve_input_accepts_graph_sensor() {
    let rig = Rig::start().await;
    let input = graph_sensor(&rig).await;
    let pwm1 = ChannelProxy::builder(&rig.client)
        .path(path(Kind::Channel, "nct6798_pwm1"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    pwm1.set_curve(&input, &[(20.0, 10.0), (60.0, 90.0)], 0.0, 100.0)
        .await
        .unwrap();
    assert_eq!(pwm1.curve().await.unwrap().0, input);
    assert_eq!(pwm1.control().await.unwrap(), "curve");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn profile_rule_accepts_graph_sensor() {
    let rig = Rig::start().await;
    let input = graph_sensor(&rig).await;
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
    let ids = profiles
        .set_rule_groups(vec![("", &target, true, vec![rule])])
        .await
        .unwrap();
    assert_eq!(ids, ["group_1"]);
    let groups = profiles.get_rule_groups().await.unwrap();
    let (_, _, _, rules) = &groups[0];
    assert_eq!(rules[0].0, "sensor");
    assert_eq!(
        OwnedObjectPath::try_from(rules[0].1["sensor"].try_clone().unwrap()).unwrap(),
        input
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn software_sensor_slot_accepts_graph_sensor() {
    let rig = Rig::start().await;
    let input = graph_sensor(&rig).await;
    let device = LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    assert!(
        device
            .get_software_sensor_sources()
            .await
            .unwrap()
            .contains(&input)
    );
    device.set_software_sensor(7, &input).await.unwrap();
    assert_eq!(device.get_software_sensors().await.unwrap()[6], (7, input));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn derived_create_refuses_graph_sensor() {
    let rig = Rig::start().await;
    let graph = graph_sensor(&rig).await;
    let measured = path(Kind::Sensor, "k10temp_temp1");
    for sources in [
        vec![graph.clone()],
        vec![measured.clone(), graph.clone()],
        vec![graph.clone(), measured.clone()],
    ] {
        let e = manager(&rig)
            .await
            .create_derived_sensor("Hottest", "maximum", &sources)
            .await
            .unwrap_err();
        assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
    }
    let e = manager(&rig)
        .await
        .create_derived_sensor("Delta", "difference", &[measured.clone(), graph])
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
    // The refusals took no Derived sensor: the first one is derived_1.
    let derived = manager(&rig)
        .await
        .create_derived_sensor("Hottest", "maximum", &[measured])
        .await
        .unwrap();
    assert_eq!(derived, path(Kind::Sensor, "derived_1"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn derived_set_definition_refuses_graph_sensor() {
    let rig = Rig::start().await;
    let graph = graph_sensor(&rig).await;
    let measured = path(Kind::Sensor, "k10temp_temp1");
    let derived = manager(&rig)
        .await
        .create_derived_sensor("Hottest", "maximum", std::slice::from_ref(&measured))
        .await
        .unwrap();
    let proxy = DerivedSensorProxy::builder(&rig.client)
        .path(derived)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let e = proxy
        .set_definition("maximum", &[measured.clone(), graph])
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
    assert_eq!(proxy.sources().await.unwrap(), [measured]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loop_flow_sensor_refuses_graph_sensor() {
    let rig = Rig::start().await;
    let graph = manager(&rig)
        .await
        .create_graph_sensor("Flow", "l/h")
        .await
        .unwrap();
    let proxy = loop_1(&rig).await;
    let e = proxy
        .set_flow_sensor(&graph, "alphacool_17558")
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
    // The Loop keeps its measured flow sensor.
    assert_eq!(
        proxy.flow_sensor().await.unwrap(),
        path(Kind::Sensor, "nct6798_fan1")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn loop_water_temperature_refuses_graph_sensor() {
    let rig = Rig::start().await;
    let graph = graph_sensor(&rig).await;
    let proxy = loop_1(&rig).await;
    let e = proxy.set_water_temperature(&graph).await.unwrap_err();
    assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
    assert_eq!(
        proxy.water_temperature().await.unwrap(),
        path(Kind::Sensor, "k10temp_temp1")
    );
}
