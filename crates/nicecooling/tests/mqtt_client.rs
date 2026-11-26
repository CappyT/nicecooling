mod support;

use std::time::{Duration, Instant};

use nicecooling::graphs::secrets::Password;
use nicecooling::mqtt::client::{
    FailureClass, Fate, MqttConfig, MqttFailure, MqttHandle, MqttStatus, ResponseEnd,
};
use nicecooling::mqtt::settings::MqttSettings;
use support::graphs::Dir;
use support::mqtt_broker::{Behavior, MqttBroker, Will, free_port};
use support::smtp_server::TestPki;
use tokio::sync::mpsc::UnboundedReceiver;

const STATUS: &str = "nicecooling/status";

fn settings(port: u16) -> MqttSettings {
    MqttSettings {
        host: "127.0.0.1".to_owned(),
        port: Some(port),
        tls: false,
        ..MqttSettings::default()
    }
}

fn config(port: u16) -> MqttConfig {
    MqttConfig::new(&settings(port), &Password::default(), "desk").expect("MQTT is on")
}

fn tls_config(port: u16) -> MqttConfig {
    let mut config = config(port);
    config.tls = true;
    config
}

fn start() -> (MqttHandle, UnboundedReceiver<ResponseEnd>) {
    MqttHandle::spawn(&tokio::runtime::Handle::current())
}

/// Wait up to 10 s until the status satisfies `wanted`, and return it.
async fn status_is(client: &MqttHandle, wanted: impl Fn(&MqttStatus) -> bool) -> MqttStatus {
    let mut status = client.watch_status();
    let reached = tokio::time::timeout(
        Duration::from_secs(10),
        status.wait_for(|status| wanted(status)),
    )
    .await
    .expect("the status changes in time")
    .expect("the status channel is open");
    reached.clone()
}

async fn next_end(ends: &mut UnboundedReceiver<ResponseEnd>) -> ResponseEnd {
    tokio::time::timeout(Duration::from_secs(10), ends.recv())
        .await
        .expect("a response ends in time")
        .expect("the channel of the ends is open")
}

/// What the caller of the client sees while the connection is down or slow.
struct Worst {
    /// The longest time between two ticks of a loop that sleeps 20 ms.
    gap: Duration,
    /// The longest `set_value` and `enqueue` of one tick.
    call: Duration,
    ticks: usize,
}

/// Call the client every 20 ms for `span`, as the Graph service does once per second.
async fn tick_for(client: &MqttHandle, span: Duration) -> Worst {
    let end = Instant::now() + span;
    let mut worst = Worst {
        gap: Duration::ZERO,
        call: Duration::ZERO,
        ticks: 0,
    };
    let mut last = Instant::now();
    while Instant::now() < end {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let now = Instant::now();
        worst.gap = worst.gap.max(now - last);
        last = now;
        let started = Instant::now();
        client.set_value("nicecooling/sensor/cpu", b"1".to_vec());
        assert!(client.enqueue("nicecooling/event/x", b"e".to_vec(), None));
        worst.call = worst.call.max(started.elapsed());
        worst.ticks += 1;
    }
    worst
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connect_publishes_online_retained() {
    let broker = MqttBroker::start(Behavior::plain());
    let (client, _ends) = start();
    client.configure(Some(config(broker.port)));

    assert!(broker.wait_until(|broker| !broker.received_on(STATUS).is_empty()));

    let messages = broker.received_on(STATUS);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].text(), "online");
    assert_eq!((messages[0].qos, messages[0].retain), (1, true));
    let connects = broker.connects();
    assert_eq!(connects.len(), 1);
    assert_eq!(connects[0].client_id, "nicecooling-desk");
    assert!(connects[0].clean_session);
    assert_eq!(connects[0].keep_alive, 30);
    assert_eq!(client.status(), MqttStatus::Connected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn last_will_is_offline() {
    let broker = MqttBroker::start(Behavior::plain());
    let (client, _ends) = start();
    client.configure(Some(config(broker.port)));
    assert!(broker.wait_until(|broker| !broker.connects().is_empty()));

    let will = Will {
        topic: STATUS.to_owned(),
        payload: b"offline".to_vec(),
        qos: 1,
        retain: true,
    };
    assert_eq!(broker.connects()[0].will, Some(will.clone()));

    // A connection that ends with no DISCONNECT makes the broker publish the will.
    broker.kick();
    assert!(broker.wait_until(|broker| !broker.fired_wills().is_empty()));
    assert_eq!(broker.fired_wills()[0], will);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_credentials_without_a_user() {
    let broker = MqttBroker::start(Behavior::plain());
    let (client, _ends) = start();
    // A password with no user name goes nowhere.
    let mut no_user = config(broker.port);
    no_user.password = Password::new("secret");
    client.configure(Some(no_user.clone()));
    assert!(broker.wait_until(|broker| broker.connects().len() == 1));
    let first = broker.connects()[0].clone();
    assert_eq!((first.user, first.password), (None, None));

    let mut with_user = no_user;
    with_user.user = "bob".to_owned();
    client.configure(Some(with_user));
    assert!(broker.wait_until(|broker| broker.connects().len() == 2));
    let second = broker.connects()[1].clone();
    assert_eq!(second.user.as_deref(), Some("bob"));
    assert_eq!(second.password.as_deref(), Some("secret"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn invalid_certificate_fails_the_connection() {
    let pki = TestPki::new();
    let broker = MqttBroker::start(Behavior::tls(pki.server_config.clone()));
    let (client, _ends) = start();
    // The test CA is not in the system certificates, and this config adds no root.
    client.configure(Some(tls_config(broker.port)));

    let status = status_is(&client, |status| matches!(status, MqttStatus::Failed(_))).await;

    assert!(
        matches!(
            &status,
            MqttStatus::Failed(failure)
                if failure.class == FailureClass::Untrusted && !failure.text.is_empty()
        ),
        "{status:?}"
    );
    // The handshake failed, so the broker never read a CONNECT.
    assert!(broker.connects().is_empty());
    assert!(broker.received().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn valid_certificate_connects_over_tls() {
    let pki = TestPki::new();
    let broker = MqttBroker::start(Behavior::tls(pki.server_config.clone()));
    let (client, _ends) = start();
    client.configure(Some(
        tls_config(broker.port).with_extra_root(pki.ca_der.clone()),
    ));

    assert!(broker.wait_until(|broker| !broker.received_on(STATUS).is_empty()));
    assert_eq!(client.status(), MqttStatus::Connected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ca_file_is_trusted_and_a_missing_file_fails() {
    let pki = TestPki::new();
    let broker = MqttBroker::start(Behavior::tls(pki.server_config.clone()));
    let dir = Dir::new("mqtt-ca");
    let ca_file = dir.path().join("ca.pem");
    std::fs::write(&ca_file, &pki.ca_pem).expect("write the CA file");
    let (client, _ends) = start();

    let mut missing = tls_config(broker.port);
    missing.ca_file = dir.path().join("none.pem").display().to_string();
    client.configure(Some(missing));
    let status = status_is(&client, |status| matches!(status, MqttStatus::Failed(_))).await;
    assert!(
        matches!(
            &status,
            MqttStatus::Failed(failure)
                if failure.class == FailureClass::CaFile && failure.text.contains("CA file")
        ),
        "{status:?}"
    );

    let mut trusted = tls_config(broker.port);
    trusted.ca_file = ca_file.display().to_string();
    client.configure(Some(trusted));
    assert!(broker.wait_until(|broker| !broker.received_on(STATUS).is_empty()));
    assert_eq!(client.status(), MqttStatus::Connected);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_never_subscribes() {
    let broker = MqttBroker::start(Behavior::plain());
    let (client, _ends) = start();
    client.configure(Some(config(broker.port)));
    client.set_value("nicecooling/sensor/a", b"1".to_vec());
    client.enqueue("nicecooling/event/x", b"raise".to_vec(), None);

    // The status, the event and the value: the client did all it does.
    assert!(broker.wait_until(|broker| broker.received().len() >= 3));

    assert_eq!(broker.subscribes(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn incoming_publish_is_ignored() {
    let broker = MqttBroker::start(Behavior::plain());
    let (client, _ends) = start();
    client.configure(Some(config(broker.port)));
    assert!(broker.wait_until(|broker| !broker.connects().is_empty()));

    broker.push_publish("nicecooling/sensor/a", b"from the broker");
    // The messages that follow still go out in order: the client did not stop or reconnect.
    client.enqueue("nicecooling/event/x", b"after".to_vec(), None);
    assert!(broker.wait_until(|broker| !broker.received_on("nicecooling/event/x").is_empty()));

    assert_eq!(client.status(), MqttStatus::Connected);
    assert_eq!(broker.connects().len(), 1);
    assert_eq!(broker.subscribes(), 0);
    assert!(broker.received_on("nicecooling/sensor/a").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn broker_down_does_not_delay_the_evaluation() {
    let (client, _ends) = start();
    // Nothing listens on this port.
    client.configure(Some(config(free_port())));

    let worst = tick_for(&client, Duration::from_millis(1500)).await;

    assert!(worst.gap < Duration::from_millis(250), "{:?}", worst.gap);
    assert!(worst.call < Duration::from_millis(50), "{:?}", worst.call);
    assert!(matches!(
        client.status(),
        MqttStatus::Connecting | MqttStatus::Failed(_)
    ));
    // The messages wait in the queue, and the value waits in its slot.
    assert_eq!(client.queue_len(), worst.ticks);
    assert_eq!(client.value_count(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slow_broker_does_not_delay_the_evaluation() {
    // The broker takes 2 s to answer the CONNECT.
    let broker = MqttBroker::start(Behavior::plain().slow(Duration::from_secs(2)));
    let (client, mut ends) = start();
    client.configure(Some(config(broker.port)));

    let worst = tick_for(&client, Duration::from_millis(1200)).await;
    assert!(worst.gap < Duration::from_millis(250), "{:?}", worst.gap);
    assert!(worst.call < Duration::from_millis(50), "{:?}", worst.call);
    assert_eq!(client.status(), MqttStatus::Connecting);

    // After the connect the queue goes out. Then the broker holds the PUBACK messages, and the
    // calls stay fast.
    status_is(&client, |status| *status == MqttStatus::Connected).await;
    let sent = worst.ticks;
    assert!(broker.wait_until(|broker| broker.received_on("nicecooling/event/x").len() >= sent));
    broker.hold_acks(true);
    client.enqueue("nicecooling/alerts/x", b"p".to_vec(), Some(1));
    assert!(broker.wait_until(|broker| !broker.received_on("nicecooling/alerts/x").is_empty()));
    let worst = tick_for(&client, Duration::from_millis(600)).await;
    assert!(worst.gap < Duration::from_millis(250), "{:?}", worst.gap);
    assert!(worst.call < Duration::from_millis(50), "{:?}", worst.call);

    broker.release_acks();
    assert_eq!(
        next_end(&mut ends).await,
        ResponseEnd {
            response_id: 1,
            fate: Fate::Acknowledged
        }
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mqtt_setting_change_reconnects() {
    let broker = MqttBroker::start(Behavior::plain());
    let (client, _ends) = start();
    client.configure(Some(config(broker.port)));
    assert!(broker.wait_until(|broker| broker.connects().len() == 1));
    assert_eq!(broker.connects()[0].client_id, "nicecooling-desk");

    let mut changed = config(broker.port);
    changed.client_id = "other".to_owned();
    client.configure(Some(changed.clone()));

    assert!(broker.wait_until(|broker| broker.connects().len() == 2));
    assert_eq!(broker.connects()[1].client_id, "other");
    // The first connection ended with no DISCONNECT, so the broker publishes its will.
    assert!(broker.wait_until(|broker| broker.fired_wills().len() == 1));
    status_is(&client, |status| *status == MqttStatus::Connected).await;

    // The same config again changes nothing.
    client.configure(Some(changed));
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert_eq!(broker.connects().len(), 2);

    // The host is cleared: the client closes the connection and queues nothing.
    client.configure(None);
    status_is(&client, |status| *status == MqttStatus::Off).await;
    assert!(!client.enqueue("nicecooling/event/x", b"e".to_vec(), None));
    client.set_value("nicecooling/sensor/a", b"1".to_vec());
    assert_eq!((client.queue_len(), client.value_count()), (0, 0));
    assert!(broker.wait_until(|broker| broker.fired_wills().len() == 2));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_cleared_ends_the_response_in_the_queue() {
    let broker = MqttBroker::start(Behavior::plain());
    broker.hold_acks(true);
    let (client, mut ends) = start();
    client.configure(Some(config(broker.port)));
    assert!(broker.wait_until(|broker| !broker.connects().is_empty()));
    // The message is on the wire and has no PUBACK.
    client.enqueue("nicecooling/alerts/x", b"p".to_vec(), Some(5));
    assert!(broker.wait_until(|broker| !broker.received_on("nicecooling/alerts/x").is_empty()));

    client.configure(None);

    assert_eq!(
        next_end(&mut ends).await,
        ResponseEnd {
            response_id: 5,
            fate: Fate::Dropped
        }
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unacknowledged_message_goes_again_after_a_reconnect() {
    let broker = MqttBroker::start(Behavior::plain());
    broker.hold_acks(true);
    let (client, mut ends) = start();
    client.configure(Some(config(broker.port)));
    client.enqueue("nicecooling/alerts/x", b"p".to_vec(), Some(3));
    assert!(broker.wait_until(|broker| !broker.received_on("nicecooling/alerts/x").is_empty()));

    // The connection breaks before the PUBACK. The message stays in the queue.
    broker.kick();
    broker.hold_acks(false);

    assert!(broker.wait_until(|broker| broker.received_on("nicecooling/alerts/x").len() >= 2));
    assert_eq!(
        next_end(&mut ends).await,
        ResponseEnd {
            response_id: 3,
            fate: Fate::Acknowledged
        }
    );
    // One end for one response.
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(ends.try_recv().is_err());
}

/// The ends that arrive until none comes in 700 ms.
async fn ends_until_quiet(ends: &mut UnboundedReceiver<ResponseEnd>) -> Vec<ResponseEnd> {
    let mut got = Vec::new();
    while let Ok(Some(end)) = tokio::time::timeout(Duration::from_millis(700), ends.recv()).await {
        got.push(end);
    }
    got
}

/// One message has no PUBACK while more than 100 later ones get theirs. The packet ids of
/// `rumqttc` wrap, and a later message meets the id of the one that waits. The acknowledgment of
/// a message ends that message only.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unacknowledged_message_never_takes_the_ack_of_another() {
    const STUCK: u64 = 1000;
    let broker = MqttBroker::start(Behavior::plain());
    broker.hold_acks_on("nicecooling/alerts/stuck");
    let (client, mut ends) = start();
    client.configure(Some(config(broker.port)));
    status_is(&client, |status| *status == MqttStatus::Connected).await;

    assert!(client.enqueue("nicecooling/alerts/stuck", b"s".to_vec(), Some(STUCK)));
    for id in 1..=150u64 {
        assert!(client.enqueue("nicecooling/alerts/other", b"o".to_vec(), Some(id)));
    }

    let before = ends_until_quiet(&mut ends).await;
    assert!(
        before.iter().all(|end| end.response_id != STUCK),
        "the message with no PUBACK ended: {before:?}"
    );
    assert!(
        before.iter().all(|end| end.fate == Fate::Acknowledged),
        "{before:?}"
    );

    broker.release_acks();
    let mut all = before;
    all.extend(ends_until_quiet(&mut ends).await);
    let mut ids: Vec<u64> = all.iter().map(|end| end.response_id).collect();
    ids.sort_unstable();
    let wanted: Vec<u64> = (1..=150).chain([STUCK]).collect();
    assert_eq!(ids, wanted, "one end for each message");
    assert!(all.iter().all(|end| end.fate == Fate::Acknowledged));
}

/// A message of 8192 bytes (topic and payload) goes to the broker and gets its PUBACK. One byte
/// more is dropped at once and never reaches the broker. The values go on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn message_limit_is_8192_bytes_for_the_topic_and_the_payload() {
    const TOPIC: &str = "nicecooling/alerts/big";
    let broker = MqttBroker::start(Behavior::plain());
    let (client, mut ends) = start();
    client.configure(Some(config(broker.port)));
    status_is(&client, |status| *status == MqttStatus::Connected).await;

    assert!(client.enqueue(TOPIC, vec![b'a'; 8192 - TOPIC.len()], Some(1)));
    assert_eq!(
        next_end(&mut ends).await,
        ResponseEnd {
            response_id: 1,
            fate: Fate::Acknowledged
        }
    );
    let sent = broker.received_on(TOPIC);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].topic.len() + sent[0].payload.len(), 8192);

    assert!(client.enqueue(TOPIC, vec![b'b'; 8193 - TOPIC.len()], Some(2)));
    assert_eq!(
        next_end(&mut ends).await,
        ResponseEnd {
            response_id: 2,
            fate: Fate::Dropped
        }
    );
    assert_eq!(client.queue_len(), 0);

    client.set_value("nicecooling/sensor/cpu", b"{\"value\":40}".to_vec());
    assert!(broker.wait_until(|broker| !broker.received_on("nicecooling/sensor/cpu").is_empty()));
    assert_eq!(
        broker.received_on(TOPIC).len(),
        1,
        "the large one never left"
    );
    assert_eq!(broker.connects().len(), 1, "no reconnect");
}

/// The status of a failed connection has the class of its error: the settings screen has one
/// text for each class.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_connection_has_the_class_of_its_error() {
    use rumqttc::{ConnectReturnCode, ConnectionError};

    // Nothing listens on this port.
    let (client, _ends) = start();
    client.configure(Some(config(free_port())));
    let status = status_is(&client, |status| matches!(status, MqttStatus::Failed(_))).await;
    assert!(
        matches!(&status, MqttStatus::Failed(failure) if failure.class == FailureClass::Unreachable),
        "{status:?}"
    );

    let class = |error: ConnectionError| MqttFailure::from(&error).class;
    for code in [
        ConnectReturnCode::BadUserNamePassword,
        ConnectReturnCode::NotAuthorized,
    ] {
        assert_eq!(
            class(ConnectionError::ConnectionRefused(code)),
            FailureClass::Login
        );
    }
    assert_eq!(
        class(ConnectionError::ConnectionRefused(
            ConnectReturnCode::ServiceUnavailable
        )),
        FailureClass::Other
    );
    assert_eq!(
        class(ConnectionError::NetworkTimeout),
        FailureClass::Timeout
    );
    let timed_out = std::io::Error::from(std::io::ErrorKind::TimedOut);
    assert_eq!(class(ConnectionError::Io(timed_out)), FailureClass::Timeout);
    let no_address = std::io::Error::other("failed to lookup address information: no such host");
    assert_eq!(
        class(ConnectionError::Io(no_address)),
        FailureClass::Unreachable
    );
    assert_eq!(class(ConnectionError::RequestsDone), FailureClass::Other);
}
