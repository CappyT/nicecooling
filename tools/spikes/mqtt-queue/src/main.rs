//! Spike for task 6.1 of `add-node-graph`: the queue rules of the MQTT client on `rumqttc`.
//! The result is in `docs/spikes/mqtt-queue.md`.

// The test broker has controls that only the tests use.
#![cfg_attr(not(test), allow(dead_code))]

mod broker;
mod client;

use rumqttc::MqttOptions;
use rustls::pki_types::CertificateDer;
use std::sync::Arc;
use std::time::Duration;

/// The TLS setup of the app: `rustls` with the `ring` provider, the system certificates and an
/// optional extra CA. There is no way to skip the check.
pub fn tls_config(
    extra_ca: Vec<CertificateDer<'static>>,
) -> Result<rustls::ClientConfig, rustls::Error> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        // A system certificate that `rustls` cannot parse is skipped.
        let _ = roots.add(cert);
    }
    for cert in extra_ca {
        roots.add(cert)?;
    }
    Ok(rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()?
        .with_root_certificates(roots)
        .with_no_client_auth())
}

fn options(port: u16) -> MqttOptions {
    let mut o = MqttOptions::new("nicecooling-spike", "127.0.0.1", port);
    o.set_keep_alive(Duration::from_secs(30));
    o
}

#[tokio::main]
async fn main() {
    let port = broker::free_port();
    let broker = broker::Broker::start(port).await;
    let (reports, mut done) = tokio::sync::mpsc::unbounded_channel();
    let (client, _task) = client::Client::start(options(port), reports);
    client.set_value("nicecooling/sensor/cpu", b"46.2".to_vec());
    client.enqueue("nicecooling/response/1", b"hello".to_vec(), Some(1));
    let report = done.recv().await.expect("one report");
    println!("response {} ended with {:?}", report.id, report.outcome);
    for m in broker.received() {
        println!(
            "{} qos {} retain {} {}",
            m.topic,
            m.qos,
            m.retain,
            String::from_utf8_lossy(&m.payload)
        );
    }
    broker.stop().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::{Broker, free_port};
    use crate::client::{Client, Outcome, QUEUE_CAP, Report};
    use rumqttc::{AsyncClient, QoS};
    use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

    async fn until(mut done: impl FnMut() -> bool) {
        let end = tokio::time::Instant::now() + Duration::from_secs(10);
        while !done() {
            assert!(
                tokio::time::Instant::now() < end,
                "condition not met in 10 s"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }

    async fn settle() {
        tokio::time::sleep(Duration::from_millis(300)).await;
    }

    fn drain(rx: &mut UnboundedReceiver<Report>) -> Vec<Report> {
        let mut v = Vec::new();
        while let Ok(r) = rx.try_recv() {
            v.push(r);
        }
        v
    }

    fn text(payload: &[u8]) -> String {
        String::from_utf8_lossy(payload).into_owned()
    }

    #[tokio::test]
    async fn pending_value_replaced_while_down() {
        let port = free_port();
        let (tx, _rx) = unbounded_channel();
        let (client, _task) = Client::start(options(port), tx);
        for i in 0..100 {
            client.set_value("nc/sensor/a", format!("a{i}").into_bytes());
        }
        for i in 0..50 {
            client.set_value("nc/sensor/b", format!("b{i}").into_bytes());
        }
        assert_eq!(client.slot_count(), 2);
        assert_eq!(client.queue_len(), 0);

        let broker = Broker::start(port).await;
        until(|| broker.received().len() >= 2).await;
        settle().await;
        let got = broker.received();
        assert_eq!(got.len(), 2, "one message for each sensor topic");
        for m in &got {
            assert_eq!((m.qos, m.retain), (0, true));
        }
        let find = |t: &str| text(&got.iter().find(|m| m.topic == t).unwrap().payload);
        assert_eq!(find("nc/sensor/a"), "a99");
        assert_eq!(find("nc/sensor/b"), "b49");
        assert_eq!(broker.subscribes(), 0);
        broker.stop().await;
    }

    #[tokio::test]
    async fn queue_of_256_drops_the_oldest() {
        let port = free_port();
        let (tx, mut rx) = unbounded_channel();
        let (client, _task) = Client::start(options(port), tx);
        for i in 0..300u64 {
            client.enqueue("nc/response/x", i.to_string().into_bytes(), Some(i));
        }
        assert_eq!(client.queue_len(), QUEUE_CAP);
        let failed = drain(&mut rx);
        let want: Vec<Report> = (0..44)
            .map(|id| Report {
                id,
                outcome: Outcome::Failed,
            })
            .collect();
        assert_eq!(failed, want, "the 44 oldest responses fail, in order");

        let broker = Broker::start(port).await;
        until(|| broker.received().len() >= QUEUE_CAP).await;
        settle().await;
        let got: Vec<String> = broker.received().iter().map(|m| text(&m.payload)).collect();
        let want: Vec<String> = (44..300).map(|i: u64| i.to_string()).collect();
        assert_eq!(got, want, "the 256 newest messages, in order");
        let mut ok = Vec::new();
        until(|| {
            ok.extend(drain(&mut rx));
            ok.len() >= QUEUE_CAP
        })
        .await;
        settle().await;
        ok.extend(drain(&mut rx));
        assert_eq!(ok.len(), QUEUE_CAP);
        assert!(ok.iter().all(|r| r.outcome == Outcome::Ok && r.id >= 44));
        broker.stop().await;
    }

    #[tokio::test]
    async fn ack_maps_to_one_run() {
        let port = free_port();
        let broker = Broker::start(port).await;
        broker.hold_acks(true);
        let (tx, mut rx) = unbounded_channel();
        let (client, _task) = Client::start(options(port), tx);
        for id in [10u64, 11, 12] {
            client.enqueue("nc/response/x", id.to_string().into_bytes(), Some(id));
        }
        until(|| broker.received().len() >= 3).await;
        settle().await;
        assert!(drain(&mut rx).is_empty(), "no report before the PUBACK");
        let mut ids: Vec<u16> = broker.received().iter().map(|m| m.pkid).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), 3, "three packet ids");
        assert!(
            broker
                .received()
                .iter()
                .all(|m| !m.dup && m.qos == 1 && !m.retain)
        );
        let c = broker.connects();
        assert_eq!(c.len(), 1);
        assert_eq!(
            (c[0].client_id.as_str(), c[0].clean_session, c[0].keep_alive),
            ("nicecooling-spike", true, 30)
        );

        broker.release_acks(true);
        let mut got = Vec::new();
        until(|| {
            got.extend(drain(&mut rx));
            got.len() >= 3
        })
        .await;
        settle().await;
        got.extend(drain(&mut rx));
        let ok = |id| Report {
            id,
            outcome: Outcome::Ok,
        };
        assert_eq!(
            got,
            vec![ok(12), ok(11), ok(10)],
            "the ack order decides, one report each"
        );
        broker.stop().await;
    }

    #[tokio::test]
    async fn reconnect_keeps_the_queue() {
        let port = free_port();
        let (tx, mut rx) = unbounded_channel();
        let (client, _task) = Client::start(options(port), tx);
        // The broker is down: 5 messages wait.
        for id in 1..=5u64 {
            client.enqueue("nc/event/x", id.to_string().into_bytes(), Some(id));
        }
        assert_eq!(client.queue_len(), 5);
        let broker = Broker::start(port).await;
        until(|| broker.received().len() >= 5).await;
        let got: Vec<String> = broker.received().iter().map(|m| text(&m.payload)).collect();
        assert_eq!(got, ["1", "2", "3", "4", "5"]);
        let mut seen = Vec::new();
        until(|| {
            seen.extend(drain(&mut rx));
            seen.len() >= 5
        })
        .await;
        assert!(seen.iter().all(|r| r.outcome == Outcome::Ok));

        // The broker closes the connection on the first of three messages, with no PUBACK.
        let before = broker.connects().len();
        broker.drop_next_publishes(1);
        for id in 6..=8u64 {
            client.enqueue("nc/event/x", id.to_string().into_bytes(), Some(id));
        }
        let mut seen = Vec::new();
        until(|| {
            seen.extend(drain(&mut rx));
            seen.len() >= 3
        })
        .await;
        settle().await;
        seen.extend(drain(&mut rx));
        assert!(broker.connects().len() > before, "the client reconnected");
        let mut ids: Vec<u64> = seen.iter().map(|r| r.id).collect();
        ids.sort_unstable();
        assert_eq!(ids, [6, 7, 8], "one report for each message");
        assert!(seen.iter().all(|r| r.outcome == Outcome::Ok));
        let sent: Vec<String> = broker.received().iter().map(|m| text(&m.payload)).collect();
        assert!(
            sent.iter().filter(|p| *p == "6").count() >= 2,
            "message 6 went again"
        );
        assert_eq!(client.queue_len(), 0);
        broker.stop().await;
    }

    async fn poll_forever(mut el: rumqttc::EventLoop) {
        loop {
            if el.poll().await.is_err() {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }

    /// What `rumqttc` does alone, measured. The document quotes these two tests.
    #[tokio::test]
    async fn rumqttc_alone_loses_the_unacked_publish_on_a_clean_reconnect() {
        let port = free_port();
        let broker = Broker::start(port).await;
        broker.drop_next_publishes(1);
        let (raw, el) = AsyncClient::new(options(port), 10);
        let poll = tokio::spawn(poll_forever(el));
        raw.publish("raw", QoS::AtLeastOnce, false, "x")
            .await
            .unwrap();
        until(|| broker.connects().len() >= 2).await;
        settle().await;
        let n = broker
            .received()
            .iter()
            .filter(|m| m.topic == "raw")
            .count();
        assert_eq!(
            n, 1,
            "the client does not send it again after the reconnect"
        );
        poll.abort();
        broker.stop().await;
    }

    #[tokio::test]
    async fn rumqttc_alone_cannot_replace_or_drop_a_waiting_request() {
        let port = free_port();
        let (raw, el) = AsyncClient::new(options(port), 4);
        for i in 0..4 {
            raw.try_publish("raw", QoS::AtLeastOnce, false, format!("m{i}"))
                .unwrap();
        }
        assert!(
            raw.try_publish("raw", QoS::AtLeastOnce, false, "m4")
                .is_err(),
            "the channel is full and has no way to replace or drop"
        );
        let poll = tokio::spawn(poll_forever(el));
        let broker = Broker::start(port).await;
        until(|| broker.received().len() >= 4).await;
        let got: Vec<String> = broker.received().iter().map(|m| text(&m.payload)).collect();
        assert_eq!(got, ["m0", "m1", "m2", "m3"]);
        poll.abort();
        broker.stop().await;
    }

    #[test]
    fn tls_config_builds_with_the_ring_provider() {
        let cfg = tls_config(Vec::new()).expect("config");
        let t =
            rumqttc::Transport::tls_with_config(rumqttc::TlsConfiguration::Rustls(Arc::new(cfg)));
        assert!(matches!(t, rumqttc::Transport::Tls(_)));
    }
}
