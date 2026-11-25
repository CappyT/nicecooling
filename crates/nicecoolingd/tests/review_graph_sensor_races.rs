//! Review reproducers: the object swap after `GraphSensor.SetDefinition` against other calls, and
//! values from two senders. A `reading_` test pins a behavior that the spec does not state.

mod support;

use std::collections::BTreeSet;
use std::fs;
use std::time::Duration;

use nicecooling_api::channel::ChannelProxy;
use nicecooling_api::graph_sensor::GraphSensorProxy;
use nicecooling_api::ids::Kind;
use nicecooling_api::manager::ManagerProxy;
use nicecooling_api::sensor::SensorProxy;
use nicecooling_core::tick::{ValueSource, percent_to_raw};
use support::{Rig, error_name, path};
use zbus::Connection;
use zbus::fdo::ObjectManagerProxy;

const SENSOR: &str = "io.github.cappyt.NiceCooling1.Sensor";
const GRAPH: &str = "io.github.cappyt.NiceCooling1.GraphSensor";

async fn manager_on(connection: &Connection) -> ManagerProxy<'static> {
    ManagerProxy::builder(connection)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

async fn graph_on(connection: &Connection, id: &str) -> GraphSensorProxy<'static> {
    GraphSensorProxy::builder(connection)
        .path(path(Kind::Sensor, id))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

/// The interfaces on the path of each sensor, from `GetManagedObjects`.
async fn sensor_objects(rig: &Rig) -> Vec<(String, BTreeSet<String>)> {
    let objects = ObjectManagerProxy::builder(&rig.client)
        .destination(nicecooling_api::BUS_NAME)
        .path(nicecooling_api::ROOT_PATH)
        .build()
        .await
        .unwrap();
    objects
        .get_managed_objects()
        .await
        .unwrap()
        .into_iter()
        .filter_map(|(object, interfaces)| {
            let id = object
                .as_str()
                .strip_prefix("/io/github/cappyt/NiceCooling1/sensor/")?;
            Some((
                id.to_owned(),
                interfaces
                    .keys()
                    .map(|name| name.as_str().to_owned())
                    .collect(),
            ))
        })
        .collect()
}

fn tick_value(rig: &mut Rig, id: &str) -> Option<f64> {
    rig.daemon
        .as_mut()
        .unwrap()
        .runner
        .context()
        .values
        .get(id)
        .copied()
}

fn pwm1_raw(rig: &Rig) -> String {
    fs::read_to_string(rig.fake.attr_path("nct6798", "pwm1"))
        .unwrap()
        .trim()
        .to_owned()
}

/// `SetDefinition` and `Delete` of one Graph sensor at the same time, from two clients, 200 times.
/// After both calls settle, a removed sensor has no object, and a sensor that stays has its two
/// interfaces. The swap task of `SetDefinition` registers `Sensor` without a check that the sensor
/// still exists, so a `Delete` between its two steps can leave an object for a sensor that is gone.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn set_definition_and_delete_leave_no_ghost_object() {
    let rig = Rig::start().await;
    let other = rig.bus.connect().await;
    let manager = manager_on(&rig.client).await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let mut problems = Vec::new();
    for n in 1..=200 {
        manager.create_graph_sensor("R", "degC").await.unwrap();
        let id = format!("graph_{n}");
        let one = graph_on(&rig.client, &id).await;
        let two = graph_on(&other, &id).await;
        let late = async {
            for _ in 0..n % 8 {
                tokio::task::yield_now().await;
            }
            two.delete().await
        };
        let (set, delete) = tokio::join!(one.set_definition("S", "rpm"), late);
        tokio::time::sleep(Duration::from_millis(40)).await;
        let exists = hub.graph_sensor_ids().contains(&id);
        let objects = sensor_objects(&rig).await;
        let interfaces = objects
            .iter()
            .find(|(object, _)| *object == id)
            .map(|(_, i)| i.clone());
        let outcome = format!(
            "{id}: SetDefinition {:?}, Delete {:?}",
            set.as_ref().map_err(error_name),
            delete.as_ref().map_err(error_name)
        );
        match (exists, interfaces) {
            (false, Some(interfaces)) => {
                problems.push(format!(
                    "{outcome}: removed, object left with {interfaces:?}"
                ));
            }
            (true, interfaces) if interfaces != Some([GRAPH, SENSOR].map(str::to_owned).into()) => {
                problems.push(format!("{outcome}: exists, object has {interfaces:?}"));
            }
            _ => {}
        }
        if exists {
            let _ = graph_on(&rig.client, &id).await.delete().await;
        }
    }
    assert!(problems.is_empty(), "{problems:#?}");
}

/// Values calls every 5 ms while the object of their sensor goes and comes back 20 times. The
/// values call names ids, not objects: no call fails and the last value is in the tick.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn values_calls_during_set_definition_all_succeed() {
    let mut rig = Rig::start_with_clock().await;
    let manager = manager_on(&rig.client).await;
    manager.create_graph_sensor("A", "degC").await.unwrap();
    let sender = {
        let manager = manager_on(&rig.bus.connect().await).await;
        tokio::spawn(async move {
            let mut failures = Vec::new();
            for n in 0..200u32 {
                let entry = vec![("graph_1".to_owned(), true, f64::from(n))];
                if let Err(e) = manager.set_graph_sensor_values(&entry).await {
                    failures.push(error_name(&e));
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            failures
        })
    };
    let graph = graph_on(&rig.client, "graph_1").await;
    let mut gaps = 0;
    for n in 0..20 {
        match graph.set_definition(&format!("A{n}"), "degC").await {
            Ok(()) => {}
            Err(e) if error_name(&e).starts_with("org.freedesktop.DBus.Error.Unknown") => gaps += 1,
            Err(e) => panic!("{e}"),
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let failures = sender.await.unwrap();
    eprintln!("SetDefinition calls that met the swap gap of the call before: {gaps}");
    assert!(failures.is_empty(), "{failures:?}");
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), Some(199.0));
}

/// A client reads `Sensor.Unit` at once after the reply of `SetDefinition`. Pins the measured
/// count of reads that meet the gap between `InterfacesRemoved` and `InterfacesAdded`, which the
/// spec does not state. The test prints the count and checks only the end state.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reading_a_reader_can_meet_the_swap_gap() {
    let rig = Rig::start().await;
    manager_on(&rig.client)
        .await
        .create_graph_sensor("A", "degC")
        .await
        .unwrap();
    let graph = graph_on(&rig.client, "graph_1").await;
    let reader = SensorProxy::builder(&rig.client)
        .path(path(Kind::Sensor, "graph_1"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let (mut old, mut new, mut gap) = (0, 0, 0);
    for n in 0..40 {
        let unit = if n % 2 == 0 { "rpm" } else { "degC" };
        graph.set_definition("A", unit).await.unwrap();
        match reader.unit().await {
            Ok(u) if u == unit => new += 1,
            Ok(_) => old += 1,
            Err(_) => gap += 1,
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    eprintln!("reads after the reply: new unit {new}, old unit {old}, no object {gap}");
    assert_eq!(reader.unit().await.unwrap(), "degC");
}

/// Two apps (two users, or two instances) send values for one id. Pins the current result, which
/// the spec does not state: the daemon binds no sensor to a sender, and the last call wins.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reading_two_senders_last_call_wins() {
    let mut rig = Rig::start_with_clock().await;
    let first = manager_on(&rig.client).await;
    let second = manager_on(&rig.bus.connect().await).await;
    first.create_graph_sensor("A", "degC").await.unwrap();
    for (sender, value) in [
        (&first, 40.0),
        (&second, 90.0),
        (&first, 41.0),
        (&second, 91.0),
    ] {
        sender
            .set_graph_sensor_values(&[("graph_1".to_owned(), true, value)])
            .await
            .unwrap();
        rig.step(1).await;
        assert_eq!(tick_value(&mut rig, "graph_1"), Some(value));
    }
    // A no-value entry of one sender takes away the value of the other.
    second
        .set_graph_sensor_values(&[("graph_1".to_owned(), false, 0.0)])
        .await
        .unwrap();
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), None);
}

/// `SetDefinition` with a new unit between two values calls, while a Curve reads the sensor.
/// Spec: a unit change clears the value at once and stays allowed while a Curve uses the sensor
/// (it is refused only for a Software sensor slot). The Curve keeps its Input, and the sensor has
/// no value until the next call.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unit_change_clears_the_value_and_keeps_the_curve() {
    let mut rig = Rig::start_with_clock().await;
    manager_on(&rig.client)
        .await
        .create_graph_sensor("Water", "degC")
        .await
        .unwrap();
    ChannelProxy::builder(&rig.client)
        .path(path(Kind::Channel, "nct6798_pwm1"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
        .set_curve(
            &path(Kind::Sensor, "graph_1"),
            &[(40.0, 20.0), (80.0, 100.0)],
            0.0,
            100.0,
        )
        .await
        .unwrap();
    rig.step(2).await;
    manager_on(&rig.client)
        .await
        .set_graph_sensor_values(&[("graph_1".to_owned(), true, 60.0)])
        .await
        .unwrap();
    rig.step(1).await;
    assert_eq!(pwm1_raw(&rig), percent_to_raw(60.0).to_string());

    graph_on(&rig.client, "graph_1")
        .await
        .set_definition("Load", "%")
        .await
        .unwrap();
    rig.step(1).await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    assert_eq!(hub.sensor("graph_1").unwrap().unit, "%");
    assert_eq!(tick_value(&mut rig, "graph_1"), None);
    assert_eq!(hub.channel("nct6798_pwm1").unwrap().curve_input, "graph_1");

    manager_on(&rig.client)
        .await
        .set_graph_sensor_values(&[("graph_1".to_owned(), true, 45.0)])
        .await
        .unwrap();
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), Some(45.0));
}

/// A values call and a `Delete` of one of its ids at the same time. The call checks the ids
/// against a copy that it takes before its loop over the entries, and the `Delete` clears the
/// value. After both calls, the deleted id has no value in the table. The test measures the time
/// `t` of one call and tries 40 delays of the `Delete` from 0.3 t to t.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn values_call_during_delete_leaves_no_value() {
    let rig = Rig::start().await;
    let manager = manager_on(&rig.client).await;
    let other = manager_on(&rig.bus.connect().await).await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let table = rig.daemon.as_ref().unwrap().values.clone();
    let filler: Vec<(String, bool, f64)> =
        (0..200_000).map(|k| (format!("x{k}"), true, 1.0)).collect();
    let start = std::time::Instant::now();
    other.set_graph_sensor_values(&filler).await.unwrap();
    let t = start.elapsed();
    let mut left = Vec::new();
    for n in 1..=40u32 {
        manager.create_graph_sensor("R", "degC").await.unwrap();
        let id = format!("graph_{n}");
        let mut big = filler.clone();
        big.push((id.clone(), true, 50.0));
        let graph = graph_on(&rig.client, &id).await;
        let delay = t.mul_f64(0.3 + f64::from(n) * 0.7 / 40.0);
        let delete = async {
            tokio::time::sleep(delay).await;
            graph.delete().await
        };
        let (values, deleted) = tokio::join!(other.set_graph_sensor_values(&big), delete);
        values.unwrap();
        deleted.unwrap();
        tokio::time::sleep(Duration::from_millis(20)).await;
        if !hub.graph_sensor_ids().contains(&id) && table.snapshot().contains_key(&id) {
            left.push(format!("{id} (delay {delay:?})"));
        }
    }
    eprintln!("one call of 200 000 entries: {t:?}");
    assert!(
        left.is_empty(),
        "deleted ids with a value in the table: {left:?}"
    );
}
