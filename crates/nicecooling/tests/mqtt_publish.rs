mod support;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nicecooling::graphs::secrets::Password;
use nicecooling::model::{
    AlarmEntry, DaemonEvent, FLAG_NO_VALUE, Link, Response, SensorInfo, SensorSample, Snapshot,
};
use nicecooling::mqtt::client::{MqttConfig, MqttHandle, ResponseEnd};
use nicecooling::mqtt::publish::{
    Publisher, ResponseDone, ResponseOutcome, ResponseRequest, ResponseSender,
    default_response_payload,
};
use nicecooling::mqtt::settings::MqttSettings;
use nicecooling_core::clock::SimWallClock;
use serde_json::Value;
use support::fixtures::{LEAKSHIELD, alarm, sensor};
use support::mqtt_broker::{Behavior, MqttBroker, Received, free_port};
use tokio::sync::broadcast;
use tokio::sync::mpsc::UnboundedReceiver;

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

/// A broker, a client that is connected to it, and a publisher that has not run as a task: the
/// test calls `apply` and `publish_values` by hand.
struct Rig {
    broker: MqttBroker,
    publisher: Publisher,
    hidden: Arc<Mutex<BTreeSet<String>>>,
    clock: SimWallClock,
}

fn rig() -> Rig {
    let broker = MqttBroker::start(Behavior::plain());
    let (client, _ends) = MqttHandle::spawn(&tokio::runtime::Handle::current());
    client.configure(Some(config(broker.port)));
    let hidden = Arc::new(Mutex::new(BTreeSet::new()));
    let clock = SimWallClock::new(TIME_US);
    let publisher = Publisher::new(client, Arc::clone(&hidden), Arc::new(clock.clone()));
    Rig {
        broker,
        publisher,
        hidden,
        clock,
    }
}

fn temp(id: &str, name: &str) -> SensorInfo {
    sensor(id, name, "temperature", "degC", "hwmon", "nct6798")
}

fn snapshot(sensors: Vec<SensorInfo>, alarms: Vec<AlarmEntry>) -> DaemonEvent {
    DaemonEvent::Snapshot(Box::new(Snapshot {
        sensors: sensors
            .into_iter()
            .map(|info| (info.id.clone(), info))
            .collect::<BTreeMap<_, _>>(),
        alarms,
        ..Snapshot::default()
    }))
}

fn sample(rows: &[(&str, f64, u8)]) -> DaemonEvent {
    DaemonEvent::Sensors(SensorSample {
        time_us: TIME_US,
        sensors: rows
            .iter()
            .map(|(id, value, flags)| ((*id).to_owned(), *value, *flags))
            .collect(),
        channels: Vec::new(),
    })
}

fn json(message: &Received) -> Value {
    serde_json::from_slice(&message.payload).expect("the payload is JSON")
}

/// Wait until the broker holds `count` messages for `topic`.
fn wait_for(broker: &MqttBroker, topic: &str, count: usize) {
    assert!(
        broker.wait_until(|broker| broker.received_on(topic).len() >= count),
        "no {count} messages on {topic}"
    );
}

fn sensor_topic(id: &str) -> String {
    format!("nicecooling/sensor/{id}")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sensor_message_has_the_four_fields() {
    let mut rig = rig();
    rig.publisher
        .apply(&snapshot(vec![temp("nct6798_temp1", "CPU")], vec![]));
    rig.publisher.apply(&sample(&[("nct6798_temp1", 46.2, 0)]));

    rig.publisher.publish_values();

    let topic = sensor_topic("nct6798_temp1");
    wait_for(&rig.broker, &topic, 1);
    let messages = rig.broker.received_on(&topic);
    assert_eq!((messages[0].qos, messages[0].retain), (0, true));
    let message = json(&messages[0]);
    assert_eq!(message["value"], 46.2);
    assert_eq!(message["unit"], "°C");
    assert_eq!(message["name"], "CPU");
    assert_eq!(message["time"], TIME_US);
    assert_eq!(message.as_object().expect("an object").len(), 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sensor_unit_is_the_display_text() {
    let mut rig = rig();
    let rows = [
        ("t", "degC", "°C"),
        ("f", "l/h", "L/h"),
        ("r", "rpm", "rpm"),
        ("p", "mbar", "mbar"),
        ("d", "%", "%"),
    ];
    let sensors = rows
        .iter()
        .map(|(id, unit, _)| sensor(id, id, "other", unit, "hwmon", ""))
        .collect();
    rig.publisher.apply(&snapshot(sensors, vec![]));
    let values: Vec<(&str, f64, u8)> = rows.iter().map(|(id, _, _)| (*id, 1.0, 0)).collect();
    rig.publisher.apply(&sample(&values));

    rig.publisher.publish_values();

    for (id, _, text) in rows {
        let topic = sensor_topic(id);
        wait_for(&rig.broker, &topic, 1);
        assert_eq!(json(&rig.broker.received_on(&topic)[0])["unit"], text);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn hidden_sensor_is_skipped() {
    let mut rig = rig();
    rig.hidden
        .lock()
        .expect("hidden lock")
        .insert("aaa_hidden".to_owned());
    rig.publisher.apply(&snapshot(
        vec![
            temp("aaa_hidden", "Hidden"),
            temp("mmm_shown", "Shown"),
            temp("zzz_last", "Last"),
        ],
        vec![],
    ));
    rig.publisher.apply(&sample(&[
        ("aaa_hidden", 1.0, 0),
        ("mmm_shown", 2.0, 0),
        ("zzz_last", 3.0, 0),
    ]));

    rig.publisher.publish_values();

    // The client sends the values in the order of their topics: when the last one is here, a
    // hidden one that was sent would be here too.
    wait_for(&rig.broker, &sensor_topic("zzz_last"), 1);
    assert_eq!(rig.broker.received_on(&sensor_topic("mmm_shown")).len(), 1);
    assert!(
        rig.broker
            .received_on(&sensor_topic("aaa_hidden"))
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn lost_sensor_value_is_null() {
    let mut rig = rig();
    let topic = sensor_topic("cpu");
    rig.publisher
        .apply(&snapshot(vec![temp("cpu", "CPU")], vec![]));
    rig.publisher.apply(&sample(&[("cpu", 46.2, 0)]));
    rig.publisher.publish_values();
    wait_for(&rig.broker, &topic, 1);
    assert_eq!(json(&rig.broker.received_on(&topic)[0])["value"], 46.2);

    // The sensor has the "no value" flag: the message says so, and keeps its name and unit.
    rig.publisher.apply(&sample(&[("cpu", 0.0, FLAG_NO_VALUE)]));
    rig.publisher.publish_values();
    wait_for(&rig.broker, &topic, 2);
    let lost = json(&rig.broker.received_on(&topic)[1]);
    assert!(lost["value"].is_null(), "{lost}");
    assert_eq!(
        (lost["unit"].as_str(), lost["name"].as_str()),
        (Some("°C"), Some("CPU"))
    );

    // The daemon is away: every sensor reads null.
    rig.publisher.apply(&sample(&[("cpu", 47.0, 0)]));
    rig.publisher.publish_values();
    wait_for(&rig.broker, &topic, 3);
    assert_eq!(json(&rig.broker.received_on(&topic)[2])["value"], 47.0);
    rig.publisher.apply(&DaemonEvent::Link(Link::Offline));
    rig.publisher.publish_values();
    wait_for(&rig.broker, &topic, 4);
    assert!(json(&rig.broker.received_on(&topic)[3])["value"].is_null());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disabled_graph_sensor_is_null() {
    let mut rig = rig();
    let topic = sensor_topic("graph_3");
    rig.publisher.apply(&snapshot(
        vec![sensor(
            "graph_3",
            "Water out",
            "temperature",
            "degC",
            "graph",
            "",
        )],
        vec![],
    ));
    rig.publisher.apply(&sample(&[("graph_3", 21.5, 0)]));
    rig.publisher.publish_values();
    wait_for(&rig.broker, &topic, 1);
    assert_eq!(json(&rig.broker.received_on(&topic)[0])["value"], 21.5);

    // A disabled Graph: the Graph service sends the "no value" entry, and the daemon flags it.
    rig.publisher
        .apply(&sample(&[("graph_3", 0.0, FLAG_NO_VALUE)]));
    rig.publisher.publish_values();

    wait_for(&rig.broker, &topic, 2);
    let message = json(&rig.broker.received_on(&topic)[1]);
    assert!(message["value"].is_null(), "{message}");
    assert_eq!(message["name"], "Water out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn values_published_at_once_after_connect() {
    let port = free_port();
    let (client, ends) = MqttHandle::spawn(&tokio::runtime::Handle::current());
    // The interval is 1 h: only the connect can send the value in this test.
    let mut down = config(port);
    down.interval = Duration::from_secs(3600);
    client.configure(Some(down));
    let (events, events_rx) = broadcast::channel(16);
    let publisher = Publisher::new(
        client.clone(),
        Arc::default(),
        Arc::new(SimWallClock::new(TIME_US)),
    );
    let (_responses, _done) = publisher.spawn(&tokio::runtime::Handle::current(), ends, events_rx);
    events
        .send(snapshot(vec![temp("cpu", "CPU")], vec![]))
        .expect("the publisher listens");
    events
        .send(sample(&[("cpu", 46.2, 0)]))
        .expect("the publisher listens");
    // The broker is down while the publisher reads the events.
    tokio::time::sleep(Duration::from_millis(300)).await;

    let broker = MqttBroker::start_on(port, Behavior::plain());

    let topic = sensor_topic("cpu");
    wait_for(&broker, &topic, 1);
    let messages = broker.received_on(&topic);
    assert_eq!((messages[0].qos, messages[0].retain), (0, true));
    assert_eq!(json(&messages[0])["value"], 46.2);
}

fn pump_stop() -> AlarmEntry {
    let mut raised = alarm("alarm_1_1", "pump-stop", "loop_1", Response::Notify, 0);
    // The `AlarmRaised` signal has no raise time.
    raised.raised_us = 0;
    raised.details.value = Some(0.0);
    raised
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn event_raise_and_clear_messages() {
    let mut rig = rig();
    let topic = "nicecooling/event/pump-stop";

    rig.publisher.apply(&DaemonEvent::AlarmRaised(pump_stop()));
    wait_for(&rig.broker, topic, 1);
    let raise = rig.broker.received_on(topic)[0].clone();
    assert_eq!((raise.qos, raise.retain), (1, false));
    let message = json(&raise);
    assert_eq!(message["event"], "pump-stop");
    assert_eq!(message["source"], "loop_1");
    assert_eq!(message["severity"], "alarm");
    assert_eq!(message["state"], "raised");
    assert_eq!(message["time"], TIME_US);
    assert_eq!(message["details"]["value"], 0.0);

    rig.clock.advance(Duration::from_secs(5));
    rig.publisher.apply(&DaemonEvent::AlarmCleared {
        instance: "alarm_1_1".to_owned(),
    });
    wait_for(&rig.broker, topic, 2);
    let clear = rig.broker.received_on(topic)[1].clone();
    assert_eq!((clear.qos, clear.retain), (1, false));
    let message = json(&clear);
    assert_eq!(message["state"], "cleared");
    assert_eq!(message["time"], TIME_US + 5_000_000);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn clear_message_uses_the_stored_raise_fields() {
    let mut rig = rig();
    let topic = "nicecooling/event/pump-stop";
    let mut raised = pump_stop();
    raised.details.limit = Some(1.5);
    raised.details.unit = "l/h".to_owned();
    rig.publisher.apply(&DaemonEvent::AlarmRaised(raised));
    wait_for(&rig.broker, topic, 1);

    // The signal of the clear carries the instance and nothing else.
    rig.publisher.apply(&DaemonEvent::AlarmCleared {
        instance: "alarm_1_1".to_owned(),
    });

    wait_for(&rig.broker, topic, 2);
    let messages = rig.broker.received_on(topic);
    let (raise, clear) = (json(&messages[0]), json(&messages[1]));
    assert_eq!(clear["source"], "loop_1");
    assert_eq!(clear["severity"], "alarm");
    assert_eq!(clear["details"], raise["details"]);
    assert_eq!(clear["details"]["value"], 0.0);
    assert_eq!(clear["details"]["limit"], 1.5);
    assert_eq!(clear["details"]["unit"], "L/h");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn snapshot_without_the_alarm_publishes_the_clear() {
    let mut rig = rig();
    let topic = "nicecooling/event/pump-stop";
    rig.publisher.apply(&DaemonEvent::AlarmRaised(pump_stop()));

    // The Alarm closed while the app had no link. The next full read does not list it.
    rig.publisher.apply(&snapshot(vec![], vec![]));
    // The signal that comes late gives no second message.
    rig.publisher.apply(&DaemonEvent::AlarmCleared {
        instance: "alarm_1_1".to_owned(),
    });
    rig.publisher.apply(&DaemonEvent::AlarmRaised(alarm(
        "alarm_1_2",
        "lost-input",
        "cpu",
        Response::Notify,
        0,
    )));

    wait_for(&rig.broker, "nicecooling/event/lost-input", 1);
    let messages = rig.broker.received_on(topic);
    assert_eq!(messages.len(), 2);
    assert_eq!(json(&messages[0])["state"], "raised");
    assert_eq!(json(&messages[1])["state"], "cleared");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restarted_daemon_keeps_the_alarm_raised() {
    let mut rig = rig();
    let topic = "nicecooling/event/pump-stop";
    rig.publisher.apply(&DaemonEvent::AlarmRaised(pump_stop()));

    // The daemon restarted. The condition still holds, under a new instance.
    let mut again = pump_stop();
    again.instance = "alarm_2_1".to_owned();
    rig.publisher.apply(&snapshot(vec![], vec![again]));
    rig.publisher.apply(&DaemonEvent::AlarmCleared {
        instance: "alarm_2_1".to_owned(),
    });

    wait_for(&rig.broker, topic, 2);
    let messages = rig.broker.received_on(topic);
    assert_eq!(messages.len(), 2);
    assert_eq!(json(&messages[0])["state"], "raised");
    assert_eq!(json(&messages[1])["state"], "cleared");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_event_message_for_an_alarm_active_at_start() {
    let mut rig = rig();
    let active = alarm("alarm_1_1", "pump-stop", "loop_1", Response::Notify, 0);

    // The first full read lists the Alarm. Its clear has no raise message to answer.
    rig.publisher.apply(&snapshot(vec![], vec![active.clone()]));
    rig.publisher.apply(&DaemonEvent::AlarmCleared {
        instance: "alarm_1_1".to_owned(),
    });
    // The app reads the daemon again and the Alarm is active: no message either.
    rig.publisher.apply(&snapshot(vec![], vec![active]));
    // A new Alarm rises and gives a message.
    rig.publisher.apply(&DaemonEvent::AlarmRaised(alarm(
        "alarm_1_2",
        "lost-input",
        "cpu",
        Response::Notify,
        0,
    )));

    // The messages of the queue go out in order: after this one, an earlier one would be here.
    wait_for(&rig.broker, "nicecooling/event/lost-input", 1);
    let events: Vec<Received> = rig
        .broker
        .received()
        .into_iter()
        .filter(|message| message.topic.starts_with("nicecooling/event/"))
        .collect();
    assert_eq!(events.len(), 1);
    assert_eq!(json(&events[0])["state"], "raised");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn event_id_with_a_dot_in_the_topic() {
    let mut rig = rig();
    rig.publisher.apply(&DaemonEvent::AlarmRaised(alarm(
        "alarm_1_1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        Response::Notify,
        0,
    )));
    rig.publisher.apply(&DaemonEvent::AlarmRaised(alarm(
        "alarm_1_2",
        "leakshield.leak-warning",
        LEAKSHIELD,
        Response::Notify,
        0,
    )));

    let alarm_topic = "nicecooling/event/leakshield.leak-alarm";
    let warning_topic = "nicecooling/event/leakshield.leak-warning";
    wait_for(&rig.broker, alarm_topic, 1);
    wait_for(&rig.broker, warning_topic, 1);
    let message = json(&rig.broker.received_on(alarm_topic)[0]);
    assert_eq!(message["event"], "leakshield.leak-alarm");
    assert_eq!(message["severity"], "alarm");
    assert_eq!(message["source"], LEAKSHIELD);
    assert_eq!(
        json(&rig.broker.received_on(warning_topic)[0])["severity"],
        "warning"
    );
}

/// The publisher as a task, with the channels of the Graph service.
fn run_publisher(
    client: &MqttHandle,
    ends: UnboundedReceiver<ResponseEnd>,
    limit: Duration,
) -> (
    ResponseSender,
    UnboundedReceiver<ResponseDone>,
    broadcast::Sender<DaemonEvent>,
) {
    let (events, events_rx) = broadcast::channel(16);
    let publisher = Publisher::new(
        client.clone(),
        Arc::default(),
        Arc::new(SimWallClock::new(TIME_US)),
    )
    .with_response_limit(limit);
    let (responses, done) = publisher.spawn(&tokio::runtime::Handle::current(), ends, events_rx);
    (responses, done, events)
}

async fn next_done(done: &mut UnboundedReceiver<ResponseDone>) -> ResponseDone {
    tokio::time::timeout(Duration::from_secs(10), done.recv())
        .await
        .expect("an outcome in time")
        .expect("the channel of the outcomes is open")
}

fn water_alert(id: u64) -> ResponseRequest {
    ResponseRequest {
        id,
        topic: "alerts/water".to_owned(),
        payload: br#"{"text":"leak"}"#.to_vec(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn response_message_ok_on_ack() {
    let broker = MqttBroker::start(Behavior::plain());
    let (client, ends) = MqttHandle::spawn(&tokio::runtime::Handle::current());
    client.configure(Some(config(broker.port)));
    let (responses, mut done, _events) = run_publisher(&client, ends, Duration::from_secs(30));

    assert!(responses.send(water_alert(7)));

    assert_eq!(
        next_done(&mut done).await,
        ResponseDone {
            id: 7,
            outcome: ResponseOutcome::Ok
        }
    );
    let messages = broker.received_on("nicecooling/alerts/water");
    assert_eq!(messages.len(), 1);
    assert_eq!((messages[0].qos, messages[0].retain), (1, false));
    assert_eq!(messages[0].text(), r#"{"text":"leak"}"#);
}

/// A payload that expands to 37 440 bytes is over the limit of 8192 bytes: the outcome is
/// `failed` at once, and the next response and the sensor values still reach the broker.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn response_message_over_the_size_limit_is_failed_and_the_values_go_on() {
    let broker = MqttBroker::start(Behavior::plain());
    let (client, ends) = MqttHandle::spawn(&tokio::runtime::Handle::current());
    client.configure(Some(config(broker.port)));
    let (responses, mut done, events) = run_publisher(&client, ends, Duration::from_secs(30));
    events
        .send(snapshot(vec![temp("cpu", "CPU")], vec![]))
        .expect("the publisher listens");
    events
        .send(sample(&[("cpu", 46.2, 0)]))
        .expect("the publisher listens");

    assert!(responses.send(ResponseRequest {
        id: 5,
        topic: "alerts/water".to_owned(),
        payload: vec![b'x'; 37_440],
    }));
    assert!(responses.send(water_alert(6)));

    assert_eq!(
        next_done(&mut done).await,
        ResponseDone {
            id: 5,
            outcome: ResponseOutcome::Failed
        }
    );
    assert_eq!(
        next_done(&mut done).await,
        ResponseDone {
            id: 6,
            outcome: ResponseOutcome::Ok
        }
    );
    wait_for(&broker, &sensor_topic("cpu"), 1);
    assert_eq!(broker.received_on("nicecooling/alerts/water").len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn response_message_timeout_after_30s_and_still_delivered() {
    let broker = MqttBroker::start(Behavior::plain());
    broker.hold_acks(true);
    let (client, ends) = MqttHandle::spawn(&tokio::runtime::Handle::current());
    client.configure(Some(config(broker.port)));
    // The test shortens the limit of 30 s to 300 ms.
    let (responses, mut done, _events) = run_publisher(&client, ends, Duration::from_millis(300));
    let started = Instant::now();

    assert!(responses.send(water_alert(8)));

    assert_eq!(
        next_done(&mut done).await,
        ResponseDone {
            id: 8,
            outcome: ResponseOutcome::Timeout
        }
    );
    assert!(started.elapsed() >= Duration::from_millis(300));
    assert!(started.elapsed() < Duration::from_secs(5));
    // The broker has the message: it only did not acknowledge it in time.
    assert_eq!(broker.received_on("nicecooling/alerts/water").len(), 1);

    // The late acknowledgment gives no second outcome.
    broker.release_acks();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(done.try_recv().is_err());
    assert_eq!(client.queue_len(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn response_without_a_host_is_not_configured() {
    // The client has no config: the host is empty.
    let (client, ends) = MqttHandle::spawn(&tokio::runtime::Handle::current());
    let (responses, mut done, _events) = run_publisher(&client, ends, Duration::from_secs(30));

    assert!(responses.send(water_alert(9)));

    assert_eq!(
        next_done(&mut done).await,
        ResponseDone {
            id: 9,
            outcome: ResponseOutcome::NotConfigured
        }
    );
    assert_eq!(ResponseOutcome::NotConfigured.id(), "not-configured");
    assert_eq!(client.queue_len(), 0);
}

#[test]
fn default_payload_has_the_three_fields() {
    let payload = default_response_payload("graph_2", "Hot water", 1_766_000_000_000_000);

    assert_eq!(
        String::from_utf8(payload).expect("UTF-8"),
        r#"{"graph_id":"graph_2","graph_name":"Hot water","time":1766000000000000}"#
    );
}
