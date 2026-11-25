//! `Manager.SetGraphSensorValues`: what the value table, the tick and `SensorsUpdated` do with the
//! numbers of the app.

mod support;

use std::fs;
use std::time::Duration;

use futures_util::StreamExt;
use nicecooling_api::channel::ChannelProxy;
use nicecooling_api::ids::Kind;
use nicecooling_api::manager::ManagerProxy;
use nicecooling_api::sensor::SensorProxy;
use nicecooling_core::tick::{ValueSource, percent_to_raw};
use support::{Rig, error_name, path};
use tokio::time::timeout;

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";
const CURVE: [(f64, f64); 2] = [(40.0, 20.0), (80.0, 100.0)];

async fn manager(rig: &Rig) -> ManagerProxy<'static> {
    ManagerProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

async fn create(rig: &Rig, unit: &str) {
    manager(rig)
        .await
        .create_graph_sensor("Water", unit)
        .await
        .unwrap();
}

async fn send(rig: &Rig, entries: &[(&str, bool, f64)]) -> zbus::Result<()> {
    let entries: Vec<(String, bool, f64)> = entries
        .iter()
        .map(|(id, has, value)| ((*id).to_owned(), *has, *value))
        .collect();
    manager(rig).await.set_graph_sensor_values(&entries).await
}

/// A rig on a simulated clock with `graph_1` (degrees Celsius) as the Input of the Curve of
/// `nct6798_pwm1`: no smoothing and no step limit, so the Duty follows the Input at once.
async fn curve_rig() -> Rig {
    let mut rig = Rig::start_with_clock().await;
    create(&rig, "degC").await;
    let pwm1 = ChannelProxy::builder(&rig.client)
        .path(path(Kind::Channel, "nct6798_pwm1"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    pwm1.set_curve(&path(Kind::Sensor, "graph_1"), &CURVE, 0.0, 100.0)
        .await
        .unwrap();
    // The first tick takes the new configuration, the second one runs it.
    rig.step(2).await;
    rig
}

fn pwm1_raw(rig: &Rig) -> String {
    fs::read_to_string(rig.fake.attr_path("nct6798", "pwm1"))
        .unwrap()
        .trim()
        .to_owned()
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn values_call_sets_values_and_no_value() {
    let mut rig = Rig::start_with_clock().await;
    create(&rig, "degC").await;
    create(&rig, "W").await;
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), None);

    send(&rig, &[("graph_1", true, 55.0), ("graph_2", false, 0.0)])
        .await
        .unwrap();
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), Some(55.0));
    assert_eq!(tick_value(&mut rig, "graph_2"), None);
    let sensor = |id: &str| {
        let client = rig.client.clone();
        let path = path(Kind::Sensor, id);
        async move {
            SensorProxy::builder(&client)
                .path(path)
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .unwrap()
        }
    };
    let one = sensor("graph_1").await;
    assert!(one.has_value().await.unwrap());
    assert_eq!(one.value().await.unwrap(), 55.0);
    assert!(!sensor("graph_2").await.has_value().await.unwrap());

    // The next call replaces both entries.
    send(&rig, &[("graph_1", false, 0.0), ("graph_2", true, 120.5)])
        .await
        .unwrap();
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), None);
    assert_eq!(tick_value(&mut rig, "graph_2"), Some(120.5));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_value_entry_acts_in_the_next_tick() {
    let mut rig = curve_rig().await;
    send(&rig, &[("graph_1", true, 50.0)]).await.unwrap();
    rig.step(1).await;
    // 50 on the Curve (40, 20) to (80, 100) is 40%.
    assert_eq!(pwm1_raw(&rig), percent_to_raw(40.0).to_string());
    assert!(!lost_input(&mut rig));

    // One second later the value is 1 s old: only the entry takes it away, not the age.
    send(&rig, &[("graph_1", false, 0.0)]).await.unwrap();
    rig.step(1).await;
    assert!(lost_input(&mut rig));
    assert_eq!(pwm1_raw(&rig), percent_to_raw(100.0).to_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_finite_fails_the_whole_call() {
    let mut rig = Rig::start_with_clock().await;
    create(&rig, "degC").await;
    create(&rig, "degC").await;
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let e = send(&rig, &[("graph_1", true, 55.0), ("graph_2", true, bad)])
            .await
            .unwrap_err();
        assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"), "{bad}");
    }
    // A number that is not finite fails the call also in an entry without a value, and in an entry
    // for an id that the daemon skips.
    for entries in [
        [("graph_1", false, f64::NAN)],
        [("nct6798_temp1", true, f64::INFINITY)],
    ] {
        let e = send(&rig, &entries).await.unwrap_err();
        assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument"));
    }
    rig.step(1).await;
    // The good entry of the failed calls changed nothing.
    assert_eq!(tick_value(&mut rig, "graph_1"), None);
    assert_eq!(tick_value(&mut rig, "graph_2"), None);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unknown_id_is_skipped() {
    let mut rig = Rig::start_with_clock().await;
    create(&rig, "degC").await;
    create(&rig, "degC").await;
    let removed = nicecooling_api::graph_sensor::GraphSensorProxy::builder(&rig.client)
        .path(path(Kind::Sensor, "graph_2"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    removed.delete().await.unwrap();

    send(
        &rig,
        &[
            ("graph_2", true, 77.0),
            ("graph_99", true, 88.0),
            ("graph_1", true, 55.0),
            ("", true, 1.0),
        ],
    )
    .await
    .unwrap();
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), Some(55.0));
    assert_eq!(tick_value(&mut rig, "graph_2"), None);
    assert_eq!(tick_value(&mut rig, "graph_99"), None);
    let table = rig.daemon.as_ref().unwrap().values.snapshot();
    assert!(!table.contains_key("graph_2"));
    assert!(!table.contains_key("graph_99"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entry_for_a_measured_sensor_is_skipped() {
    let mut rig = Rig::start_with_clock().await;
    rig.step(1).await;
    send(
        &rig,
        &[
            ("nct6798_temp1", true, 99.0),
            ("k10temp_temp1", false, 0.0),
            ("nct6798_fan1", true, 5.0),
        ],
    )
    .await
    .unwrap();
    // The sources republish every 50 ms: the table is read in the same moment as the call.
    let table = rig.daemon.as_ref().unwrap().values.snapshot();
    assert_ne!(table["nct6798_temp1"].value, 99.0);
    assert_ne!(table["nct6798_fan1"].value, 5.0);
    assert!(table.contains_key("k10temp_temp1"));
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "nct6798_temp1"), Some(30.0));
    assert_eq!(tick_value(&mut rig, "k10temp_temp1"), Some(61.875));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entry_for_a_derived_sensor_is_skipped() {
    let mut rig = Rig::start_with_clock().await;
    let sources = [
        path(Kind::Sensor, "k10temp_temp1"),
        path(Kind::Sensor, "nct6798_temp1"),
    ];
    manager(&rig)
        .await
        .create_derived_sensor("Hottest", "maximum", &sources)
        .await
        .unwrap();
    rig.step(2).await;
    let before = tick_value(&mut rig, "derived_1");
    assert_eq!(before, Some(61.875));
    let flow = tick_value(&mut rig, "loop_1_flow");
    assert!(flow.is_some());

    send(
        &rig,
        &[("derived_1", true, 5.0), ("loop_1_flow", true, 7.0)],
    )
    .await
    .unwrap();
    let table = rig.daemon.as_ref().unwrap().values.snapshot();
    assert!(!table.contains_key("derived_1"));
    assert!(!table.contains_key("loop_1_flow"));
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "derived_1"), before);
    assert_eq!(tick_value(&mut rig, "loop_1_flow"), flow);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn value_goes_stale_after_3s() {
    let mut rig = curve_rig().await;
    send(&rig, &[("graph_1", true, 60.0)]).await.unwrap();
    // The value is 3 s old at the third tick after the call: it still counts.
    rig.step(3).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), Some(60.0));
    assert!(!lost_input(&mut rig));
    assert_eq!(pwm1_raw(&rig), percent_to_raw(60.0).to_string());
    // At 4 s it is stale: no value, Lost Input and 100% Duty.
    rig.step(1).await;
    assert_eq!(tick_value(&mut rig, "graph_1"), None);
    assert!(lost_input(&mut rig));
    assert_eq!(pwm1_raw(&rig), percent_to_raw(100.0).to_string());
    // A new value ends it.
    send(&rig, &[("graph_1", true, 60.0)]).await.unwrap();
    rig.step(1).await;
    assert!(!lost_input(&mut rig));
    assert_eq!(pwm1_raw(&rig), percent_to_raw(60.0).to_string());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn curve_follows_a_graph_sensor() {
    let mut rig = curve_rig().await;
    // Before the first value the Channel is on Lost Input.
    assert!(lost_input(&mut rig));
    assert_eq!(pwm1_raw(&rig), percent_to_raw(100.0).to_string());
    for (input, duty) in [(60.0, 60.0), (45.0, 30.0), (80.0, 100.0), (40.0, 20.0)] {
        send(&rig, &[("graph_1", true, input)]).await.unwrap();
        rig.step(1).await;
        assert!(!lost_input(&mut rig), "{input}");
        assert_eq!(pwm1_raw(&rig), percent_to_raw(duty).to_string(), "{input}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sensors_updated_flags_a_graph_sensor_without_value() {
    let mut rig = Rig::start_with_clock().await;
    create(&rig, "degC").await;
    let mut signals = manager(&rig).await.receive_sensors_updated().await.unwrap();
    let mut graph_entry = async |rig: &mut Rig| {
        rig.step(1).await;
        let mut last = None;
        while let Ok(Some(signal)) = timeout(Duration::from_millis(300), signals.next()).await {
            let args = signal.args().unwrap();
            last = args.sensors.iter().find(|s| s.0 == "graph_1").cloned();
        }
        last.expect("a SensorsUpdated entry for graph_1")
    };
    // No value: flag bit 0 and a value of 0.
    assert_eq!(graph_entry(&mut rig).await, ("graph_1".to_owned(), 0.0, 1));
    send(&rig, &[("graph_1", true, 55.0)]).await.unwrap();
    assert_eq!(graph_entry(&mut rig).await, ("graph_1".to_owned(), 55.0, 0));
    send(&rig, &[("graph_1", false, 0.0)]).await.unwrap();
    assert_eq!(graph_entry(&mut rig).await, ("graph_1".to_owned(), 0.0, 1));
}
