//! Review 2, the MQTT client of the app. Test broker in this process only.

mod support;

use std::sync::Arc;
use std::time::Duration;

use nicecooling::graphs::secrets::Password;
use nicecooling::mqtt::client::{FailureClass, MqttConfig, MqttHandle, MqttStatus, ResponseEnd};
use nicecooling::mqtt::settings::MqttSettings;
use rcgen::{
    BasicConstraints, CertificateParams, DnType, ExtendedKeyUsagePurpose, IsCa, Issuer, KeyPair,
    KeyUsagePurpose,
};
use rustls::ServerConfig;
use rustls::pki_types::{PrivateKeyDer, PrivatePkcs8KeyDer};
use support::mqtt_broker::{Behavior, MqttBroker};
use tokio::sync::mpsc::UnboundedReceiver;

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

fn start() -> (MqttHandle, UnboundedReceiver<ResponseEnd>) {
    MqttHandle::spawn(&tokio::runtime::Handle::current())
}

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

/// A CA and a server certificate for `wrong.example` only: not for `127.0.0.1`.
pub fn wrong_name_pki() -> (Vec<u8>, Arc<ServerConfig>) {
    let ca_key = KeyPair::generate().unwrap();
    let mut ca_params = CertificateParams::new(Vec::<String>::new()).unwrap();
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "Review CA");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign];
    let ca_cert = ca_params.self_signed(&ca_key).unwrap();
    let issuer = Issuer::new(ca_params, ca_key);
    let leaf_key = KeyPair::generate().unwrap();
    let mut leaf_params = CertificateParams::new(vec!["wrong.example".to_owned()]).unwrap();
    leaf_params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    let leaf = leaf_params.signed_by(&leaf_key, &issuer).unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let config = ServerConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(
            vec![leaf.der().clone()],
            PrivateKeyDer::from(PrivatePkcs8KeyDer::from(leaf_key.serialize_der())),
        )
        .unwrap();
    (ca_cert.der().to_vec(), Arc::new(config))
}

/// A payload template has up to 4096 bytes. 585 times `{graph}` with a Graph name of 64
/// characters expands to 37 440 bytes. `rumqttc` refuses a packet over 10 KiB (its default
/// `max_outgoing_packet_size`) with an error of the event loop, so the connection ends, the
/// message goes back to the front of the queue, and the next connection fails on it again.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_oversize_response_message_does_not_stop_the_values() {
    let broker = MqttBroker::start(Behavior::plain());
    let (client, mut ends) = start();
    client.configure(Some(config(broker.port)));
    status_is(&client, |status| *status == MqttStatus::Connected).await;

    client.enqueue("nicecooling/alerts/big", vec![b'x'; 37_440], Some(7));
    client.set_value(
        "nicecooling/sensor/k10temp_temp1",
        b"{\"value\":40}".to_vec(),
    );

    let value_seen = broker.wait_until(|broker| {
        !broker
            .received_on("nicecooling/sensor/k10temp_temp1")
            .is_empty()
    });
    let end = tokio::time::timeout(Duration::from_secs(5), ends.recv()).await;
    let connects = broker.connects().len();
    assert!(
        value_seen,
        "no sensor value reached the broker in 10 s, {connects} connections, status {:?}",
        client.status()
    );
    assert!(
        matches!(end, Ok(Some(ResponseEnd { response_id: 7, .. }))),
        "the oversize response got no end: {end:?}"
    );
}

/// With TLS on, a certificate from a trusted CA for another host name must fail.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_certificate_for_another_host_name_fails() {
    let (ca_der, server_config) = wrong_name_pki();
    let broker = MqttBroker::start(Behavior::tls(server_config));
    let (client, _ends) = start();
    let mut tls = config(broker.port);
    tls.tls = true;
    client.configure(Some(tls.with_extra_root(ca_der)));
    let status = status_is(&client, |status| matches!(status, MqttStatus::Failed(_))).await;
    assert!(broker.connects().is_empty(), "{status:?}");
    assert!(
        matches!(&status, MqttStatus::Failed(failure) if failure.class == FailureClass::NameMismatch),
        "{status:?}"
    );
}

/// The password is in no `Debug` text and in no status text.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_password_is_in_no_debug_or_status_text() {
    let mut with_user = settings(1);
    with_user.user = "bob".to_owned();
    let config = MqttConfig::new(&with_user, &Password::new("hunter2-secret"), "desk").unwrap();
    assert!(!format!("{config:?}").contains("hunter2-secret"));
    let (client, _ends) = start();
    client.configure(Some(config));
    let status = status_is(&client, |status| matches!(status, MqttStatus::Failed(_))).await;
    assert!(
        !format!("{status:?}").contains("hunter2-secret"),
        "{status:?}"
    );
}
