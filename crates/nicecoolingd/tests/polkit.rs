//! `PolkitAuthorizer` against a mock of `org.freedesktop.PolicyKit1` on the private bus.

mod support;

use std::sync::atomic::Ordering;
use std::time::Duration;

use futures_util::StreamExt;
use nicecooling_api::channel::ChannelProxy;
use nicecooling_api::graph_sensor::GraphSensorProxy;
use nicecooling_api::ids::Kind;
use nicecooling_api::manager::ManagerProxy;
use nicecooling_api::sensor::SensorProxy;
use support::{Rig, error_name, path};
use tokio::time::timeout;
use zbus::fdo::ObjectManagerProxy;

const WRITE: &str = "io.github.cappyt.NiceCooling1.write";
const DENIED: &str = "org.freedesktop.DBus.Error.AccessDenied";

fn running_as_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .is_some_and(|l| l.split_whitespace().next() == Some("0"))
}

async fn pwm1(rig: &Rig) -> ChannelProxy<'static> {
    ChannelProxy::builder(&rig.client)
        .path(path(Kind::Channel, "nct6798_pwm1"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn polkit_denial_returns_access_denied() {
    if running_as_root() {
        return; // uid 0 needs no check
    }
    let mut rig = Rig::start_with_polkit().await;
    rig.tick_in_background();
    let pwm1 = pwm1(&rig).await;
    pwm1.set_fixed_duty(50.0).await.unwrap();
    let polkit = rig.polkit.as_ref().unwrap();
    polkit.deny.store(true, Ordering::SeqCst);
    let e = pwm1.set_fixed_duty(60.0).await.unwrap_err();
    assert_eq!(error_name(&e), "org.freedesktop.DBus.Error.AccessDenied");
    assert_eq!(pwm1.fixed_duty().await.unwrap(), 50.0);
    // A read method asks polkit nothing.
    let calls = polkit.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert!(
        calls
            .iter()
            .all(|c| c.0 == "io.github.cappyt.NiceCooling1.write")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interactive_flag_passed_only_when_set() {
    if running_as_root() {
        return;
    }
    let mut rig = Rig::start_with_polkit().await;
    rig.tick_in_background();
    let pwm1 = pwm1(&rig).await;
    let proxy = zbus::Proxy::new(
        &rig.client,
        nicecooling_api::BUS_NAME,
        path(Kind::Channel, "nct6798_pwm1"),
        "io.github.cappyt.NiceCooling1.Channel",
    )
    .await
    .unwrap();
    pwm1.set_fixed_duty(10.0).await.unwrap();
    proxy
        .call_with_flags::<_, _, ()>(
            "SetFixedDuty",
            zbus::proxy::MethodFlags::ALLOW_INTERACTIVE_AUTH,
            &(20.0f64,),
        )
        .await
        .unwrap();
    // The polkit flag ALLOW_USER_INTERACTION is 1.
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
    assert_eq!(flags, [0, 1]);
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

async fn sensor(rig: &Rig, id: &str) -> SensorProxy<'static> {
    SensorProxy::builder(&rig.client)
        .path(path(Kind::Sensor, id))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

fn deny(rig: &Rig, denied: bool) {
    rig.polkit
        .as_ref()
        .unwrap()
        .deny
        .store(denied, Ordering::SeqCst);
}

/// The action ids and the flags that the mock polkit has seen since the last call.
fn take_calls(rig: &Rig) -> Vec<(String, u32)> {
    std::mem::take(&mut *rig.polkit.as_ref().unwrap().calls.lock().unwrap())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn graph_sensor_members_need_write() {
    if running_as_root() {
        return;
    }
    let mut rig = Rig::start_with_polkit().await;
    rig.tick_in_background();
    let manager = manager(&rig).await;
    let objects = ObjectManagerProxy::builder(&rig.client)
        .destination(nicecooling_api::BUS_NAME)
        .path(nicecooling_api::ROOT_PATH)
        .build()
        .await
        .unwrap();
    let mut added = objects.receive_interfaces_added().await.unwrap();

    // Allowed: each of the four members asks polkit once, for `write`.
    take_calls(&rig);
    manager.create_graph_sensor("Water", "degC").await.unwrap();
    assert_eq!(take_calls(&rig), [(WRITE.to_owned(), 0)]);
    manager
        .set_graph_sensor_values(&[("graph_1".to_owned(), true, 55.0)])
        .await
        .unwrap();
    assert_eq!(take_calls(&rig), [(WRITE.to_owned(), 0)]);
    graph(&rig, "graph_1")
        .await
        .set_definition("Hot water", "degC")
        .await
        .unwrap();
    assert_eq!(take_calls(&rig), [(WRITE.to_owned(), 0)]);
    // After the reply the object goes and comes back: wait for its two interfaces.
    let mut returned = 0;
    while returned < 2 {
        let signal = timeout(Duration::from_secs(5), added.next())
            .await
            .unwrap()
            .unwrap();
        let args = signal.args().unwrap();
        if args.object_path.as_str().ends_with("/graph_1") {
            returned += args.interfaces_and_properties.len();
        }
    }

    // Denied: each member fails with AccessDenied and changes nothing.
    deny(&rig, true);
    take_calls(&rig);
    let e = manager
        .create_graph_sensor("Two", "degC")
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), DENIED);
    let e = manager
        .set_graph_sensor_values(&[("graph_1".to_owned(), false, 0.0)])
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), DENIED);
    let e = graph(&rig, "graph_1")
        .await
        .set_definition("Other", "rpm")
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), DENIED);
    let e = graph(&rig, "graph_1").await.delete().await.unwrap_err();
    assert_eq!(error_name(&e), DENIED);
    let calls = take_calls(&rig);
    assert_eq!(calls.len(), 4);
    assert!(calls.iter().all(|c| c.0 == WRITE), "{calls:?}");

    // The sensor is as it was, and no second sensor exists.
    deny(&rig, false);
    let one = sensor(&rig, "graph_1").await;
    assert_eq!(one.name().await.unwrap(), "Hot water");
    assert_eq!(one.unit().await.unwrap(), "degC");
    let mut has_value = false;
    for _ in 0..40 {
        has_value = one.has_value().await.unwrap();
        if has_value {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert!(has_value, "the denied call did not remove the value");
    let objects = objects.get_managed_objects().await.unwrap();
    assert!(
        objects
            .keys()
            .all(|p| !p.as_str().ends_with("/graph_2") && !p.as_str().ends_with("/graph_3"))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn values_call_asks_polkit_every_time() {
    if running_as_root() {
        return;
    }
    let mut rig = Rig::start_with_polkit().await;
    rig.tick_in_background();
    let manager = manager(&rig).await;
    manager.create_graph_sensor("Water", "degC").await.unwrap();
    take_calls(&rig);

    // Every call asks, with no interactive flag: the daemon keeps no authorization of a sender.
    for n in 0..5 {
        manager
            .set_graph_sensor_values(&[("graph_1".to_owned(), true, f64::from(n))])
            .await
            .unwrap();
    }
    assert_eq!(take_calls(&rig), vec![(WRITE.to_owned(), 0); 5]);

    // A denial after five allowed calls is a denial at once.
    deny(&rig, true);
    let e = manager
        .set_graph_sensor_values(&[("graph_1".to_owned(), true, 99.0)])
        .await
        .unwrap_err();
    assert_eq!(error_name(&e), DENIED);
    assert_eq!(take_calls(&rig), [(WRITE.to_owned(), 0)]);

    // And an allowance after the denial is an allowance at once.
    deny(&rig, false);
    manager
        .set_graph_sensor_values(&[("graph_1".to_owned(), true, 7.0)])
        .await
        .unwrap();
    assert_eq!(take_calls(&rig), [(WRITE.to_owned(), 0)]);
}
