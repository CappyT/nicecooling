//! Review checks: what the root daemon does with the arguments of an unprivileged caller in the
//! four Graph sensor members. A `reading_` test pins a behavior that the spec does not state.

mod support;

use std::time::{Duration, Instant};

use nicecooling_api::graph_sensor::GraphSensorProxy;
use nicecooling_api::ids::Kind;
use nicecooling_api::manager::ManagerProxy;
use nicecooling_api::sensor::SensorProxy;
use nicecooling_core::tick::ValueSource;
use support::{Rig, error_name, path};

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";

async fn manager(rig: &Rig) -> ManagerProxy<'static> {
    ManagerProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

async fn graph_at(rig: &Rig, path: zbus::OwnedObjectPath) -> GraphSensorProxy<'static> {
    GraphSensorProxy::builder(&rig.client)
        .path(path)
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

fn entries(list: &[(&str, bool, f64)]) -> Vec<(String, bool, f64)> {
    list.iter()
        .map(|(id, has, value)| ((*id).to_owned(), *has, *value))
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

/// Arrays of 0, 1, 65 and 100 000 entries. The large call must end in a short time, and a tick
/// that runs while the daemon handles it must not wait for it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn values_call_takes_0_1_65_and_100000_entries() {
    let mut rig = Rig::start_with_clock().await;
    let manager = manager(&rig).await;
    for _ in 0..64 {
        manager.create_graph_sensor("G", "degC").await.unwrap();
    }
    manager.set_graph_sensor_values(&[]).await.unwrap();
    manager
        .set_graph_sensor_values(&entries(&[("graph_1", true, 1.0)]))
        .await
        .unwrap();
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), Some(1.0));

    let sixty_five: Vec<(String, bool, f64)> = (1..=65)
        .map(|n| (format!("graph_{n}"), true, f64::from(n)))
        .collect();
    manager.set_graph_sensor_values(&sixty_five).await.unwrap();
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_64"), Some(64.0));
    assert!(
        !rig.daemon
            .as_ref()
            .unwrap()
            .values
            .snapshot()
            .contains_key("graph_65")
    );

    let big: Vec<(String, bool, f64)> = (0..100_000u32)
        .map(|n| {
            let id = if n % 2 == 0 {
                format!("graph_{}", n % 64 + 1)
            } else {
                format!("unknown_{n}")
            };
            (id, true, f64::from(n))
        })
        .collect();
    let call = {
        let manager = manager.clone();
        tokio::spawn(async move {
            let start = Instant::now();
            manager.set_graph_sensor_values(&big).await.unwrap();
            start.elapsed()
        })
    };
    let mut slowest = Duration::ZERO;
    while !call.is_finished() {
        let start = Instant::now();
        rig.daemon.as_mut().unwrap().runner.tick();
        slowest = slowest.max(start.elapsed());
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let took = call.await.unwrap();
    eprintln!("100 000 entries: the call took {took:?}, the slowest tick meanwhile {slowest:?}");
    assert!(took < Duration::from_secs(5), "{took:?}");
    assert!(slowest < Duration::from_millis(200), "{slowest:?}");
    // The value of an id is the number of its last entry.
    let table = rig.daemon.as_ref().unwrap().values.snapshot();
    let last_for_1 = (0..100_000u32)
        .rev()
        .find(|n| n % 2 == 0 && n % 64 == 0)
        .unwrap();
    assert_eq!(table["graph_1"].value, f64::from(last_for_1));
    assert!(!table.keys().any(|k| k.starts_with("unknown_")));
}

/// Ids that are not the id of a Graph sensor: path characters, an object path, other case, a
/// line break, a leading zero and an id of 1 MiB. The daemon skips them all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ids_with_path_characters_are_skipped() {
    let mut rig = Rig::start_with_clock().await;
    let manager = manager(&rig).await;
    manager.create_graph_sensor("G", "degC").await.unwrap();
    let long = "g".repeat(1 << 20);
    let odd = [
        "../graph_1",
        "graph_1/",
        "/io/github/cappyt/NiceCooling1/sensor/graph_1",
        "GRAPH_1",
        "graph_1\n",
        "graph_01",
        " graph_1",
        long.as_str(),
    ];
    let list: Vec<(String, bool, f64)> =
        odd.iter().map(|id| ((*id).to_owned(), true, 9.0)).collect();
    manager.set_graph_sensor_values(&list).await.unwrap();
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), None);
    let table = rig.daemon.as_ref().unwrap().values.snapshot();
    for id in odd {
        assert!(!table.contains_key(id), "{:.40}", id);
    }
}

/// Spec: one entry per Graph sensor. Pins the current result of two entries for one id, which the
/// spec does not state: the last entry wins.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reading_duplicate_ids_last_entry_wins() {
    let mut rig = Rig::start_with_clock().await;
    let manager = manager(&rig).await;
    manager.create_graph_sensor("G", "degC").await.unwrap();
    manager
        .set_graph_sensor_values(&entries(&[
            ("graph_1", true, 55.0),
            ("graph_1", false, 0.0),
        ]))
        .await
        .unwrap();
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), None);
    manager
        .set_graph_sensor_values(&entries(&[
            ("graph_1", false, 0.0),
            ("graph_1", true, 55.0),
        ]))
        .await
        .unwrap();
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), Some(55.0));
}

/// Spec: "A name SHALL have up to 64 characters." Characters, not bytes: 64 characters of 4 bytes
/// pass, 65 fail, and a name of 1 MiB fails. A unit outside the table of 8 fails in any spelling.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn names_and_units_at_the_edges() {
    let rig = Rig::start().await;
    let manager = manager(&rig).await;
    let wide = "\u{1F600}".repeat(64);
    let created = manager.create_graph_sensor(&wide, "degC").await.unwrap();
    assert_eq!(sensor(&rig, "graph_1").await.name().await.unwrap(), wide);
    for name in ["\u{1F600}".repeat(65), "n".repeat(1 << 20)] {
        let e = manager
            .create_graph_sensor(&name, "degC")
            .await
            .unwrap_err();
        assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
        let e = graph_at(&rig, created.clone())
            .await
            .set_definition(&name, "degC")
            .await
            .unwrap_err();
        assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
    }
    for unit in ["degc", "degC ", "", "\u{b0}C", "L/h", "bar", "%%"] {
        let e = manager.create_graph_sensor("G", unit).await.unwrap_err();
        assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"), "{unit:?}");
        let e = graph_at(&rig, created.clone())
            .await
            .set_definition("G", unit)
            .await
            .unwrap_err();
        assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"), "{unit:?}");
    }
    assert_eq!(sensor(&rig, "graph_1").await.unit().await.unwrap(), "degC");
    assert_eq!(
        rig.daemon.as_ref().unwrap().hub.graph_sensor_ids(),
        ["graph_1"]
    );
}

/// Pins the current result for names that the spec does not limit beyond the length: control
/// characters pass as they are, and an empty name gives the id as the default name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reading_control_characters_and_empty_names_pass() {
    let rig = Rig::start().await;
    let manager = manager(&rig).await;
    let control = "a\u{1b}[2J\nb\tc\u{7f}\u{202e}";
    manager.create_graph_sensor(control, "degC").await.unwrap();
    assert_eq!(sensor(&rig, "graph_1").await.name().await.unwrap(), control);
    manager.create_graph_sensor("", "degC").await.unwrap();
    assert_eq!(
        sensor(&rig, "graph_2").await.name().await.unwrap(),
        "graph_2"
    );
}

/// `GraphSensor.SetDefinition` and `GraphSensor.Delete` on a path that is not a Graph sensor: a
/// measured sensor, a Derived sensor, a calibrated flow, the root and a path with no object. Each
/// call fails, and the sensor stays.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn graph_sensor_members_on_other_paths_fail() {
    let rig = Rig::start().await;
    let sources = [
        path(Kind::Sensor, "k10temp_temp1"),
        path(Kind::Sensor, "nct6798_temp1"),
    ];
    manager(&rig)
        .await
        .create_derived_sensor("Hottest", "maximum", &sources)
        .await
        .unwrap();
    let paths = [
        path(Kind::Sensor, "nct6798_temp1"),
        path(Kind::Sensor, "derived_1"),
        path(Kind::Sensor, "loop_1_flow"),
        support::root(),
        path(Kind::Sensor, "graph_1"),
    ];
    for target in paths {
        let proxy = graph_at(&rig, target.clone()).await;
        let e = proxy.set_definition("X", "degC").await.unwrap_err();
        eprintln!("SetDefinition on {target}: {}", error_name(&e));
        assert!(error_name(&e).starts_with("org.freedesktop.DBus.Error.Unknown"));
        let e = proxy.delete().await.unwrap_err();
        assert!(error_name(&e).starts_with("org.freedesktop.DBus.Error.Unknown"));
    }
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    for id in ["nct6798_temp1", "derived_1", "loop_1_flow"] {
        assert!(hub.sensor(id).is_some(), "{id}");
    }
    assert!(hub.graph_sensor_ids().is_empty());
}

/// A sensor that is deleted takes no value, and a sensor created after it starts with no value and
/// a new id.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn deleted_id_takes_no_value_and_the_new_one_starts_without_one() {
    let mut rig = Rig::start_with_clock().await;
    let manager = manager(&rig).await;
    let first = manager.create_graph_sensor("A", "degC").await.unwrap();
    manager
        .set_graph_sensor_values(&entries(&[("graph_1", true, 50.0)]))
        .await
        .unwrap();
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), Some(50.0));
    graph_at(&rig, first).await.delete().await.unwrap();
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), None);
    let second = manager.create_graph_sensor("B", "degC").await.unwrap();
    assert_eq!(second, path(Kind::Sensor, "graph_2"));
    manager
        .set_graph_sensor_values(&entries(&[("graph_1", true, 60.0)]))
        .await
        .unwrap();
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), None);
    assert_eq!(tick_value(&mut rig, "graph_2"), None);
    assert!(!sensor(&rig, "graph_2").await.has_value().await.unwrap());
}
