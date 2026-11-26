//! Review 2, the MQTT publisher of the app. Test broker in this process only.

mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use nicecooling::graphs::secrets::Password;
use nicecooling::model::{DaemonEvent, SensorInfo, SensorSample, Snapshot};
use nicecooling::mqtt::client::{MqttConfig, MqttHandle};
use nicecooling::mqtt::publish::Publisher;
use nicecooling::mqtt::settings::MqttSettings;
use nicecooling_core::clock::SimWallClock;
use support::fixtures::sensor;
use support::mqtt_broker::{Behavior, MqttBroker};

const TIME_US: u64 = 1_766_000_000_000_000;

fn config(port: u16) -> MqttConfig {
    let settings = MqttSettings {
        host: "127.0.0.1".to_owned(),
        port: Some(port),
        tls: false,
        ..MqttSettings::default()
    };
    MqttConfig::new(&settings, &Password::default(), "desk").expect("MQTT is on")
}

fn temp(id: &str, name: &str) -> SensorInfo {
    sensor(id, name, "temperature", "degC", "hwmon", "nct6798")
}

fn snapshot(sensors: Vec<SensorInfo>) -> DaemonEvent {
    DaemonEvent::Snapshot(Box::new(Snapshot {
        sensors: sensors
            .into_iter()
            .map(|info| (info.id.clone(), info))
            .collect::<BTreeMap<_, _>>(),
        ..Snapshot::default()
    }))
}

fn sample(rows: &[(&str, f64)]) -> DaemonEvent {
    DaemonEvent::Sensors(SensorSample {
        time_us: TIME_US,
        sensors: rows
            .iter()
            .map(|(id, value)| ((*id).to_owned(), *value, 0))
            .collect(),
        channels: Vec::new(),
    })
}

fn topic(id: &str) -> String {
    format!("nicecooling/sensor/{id}")
}

/// Spec (mqtt-publishing, "Hidden sensor"): when the user hides a sensor, the client publishes no
/// message for it. Each connect sends every waiting value again, so the last value of a hidden
/// sensor must leave the client when the user hides the sensor.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sensor_hidden_after_a_publish_gets_no_message_after_a_reconnect() {
    let broker = MqttBroker::start(Behavior::plain());
    let (client, _ends) = MqttHandle::spawn(&tokio::runtime::Handle::current());
    client.configure(Some(config(broker.port)));
    let hidden = Arc::new(Mutex::new(BTreeSet::new()));
    let clock = SimWallClock::new(TIME_US);
    let mut publisher = Publisher::new(client, Arc::clone(&hidden), Arc::new(clock));
    publisher.apply(&snapshot(vec![
        temp("aaa_later_hidden", "Hidden"),
        temp("zzz_last", "Last"),
    ]));
    publisher.apply(&sample(&[("aaa_later_hidden", 1.0), ("zzz_last", 3.0)]));
    publisher.publish_values();
    assert!(broker.wait_until(|b| b.received_on(&topic("zzz_last")).len() == 1));
    assert_eq!(broker.received_on(&topic("aaa_later_hidden")).len(), 1);

    hidden.lock().unwrap().insert("aaa_later_hidden".to_owned());
    publisher.publish_values();
    // The value of this pass goes out on the first connection. Only then the connection drops,
    // so the third message of the other sensor proves that the client sent its values again.
    assert!(broker.wait_until(|b| b.received_on(&topic("zzz_last")).len() == 2));
    broker.kick();

    // The client sends the values in the order of their topics after the connect.
    assert!(broker.wait_until(|b| b.received_on(&topic("zzz_last")).len() >= 3));
    assert_eq!(
        broker.received_on(&topic("aaa_later_hidden")).len(),
        1,
        "the hidden sensor got a message after the reconnect"
    );
}
