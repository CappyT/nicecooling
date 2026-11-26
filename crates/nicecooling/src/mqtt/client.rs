//! The MQTT client of the app. It only publishes: it never subscribes, and it ignores every
//! packet that the broker sends on its own.
//!
//! The queue is outside `rumqttc` (see `docs/spikes/mqtt-queue.md`). The callers `set_value` and
//! `enqueue` change the `Outbox` and wake the task. They never wait for the network, so a broker
//! that is down or slow costs the caller nothing. One task owns the connection. It hands messages
//! to `rumqttc` only while the connection is up. A message that the broker did not acknowledge
//! when the connection ends goes back to the front of the queue.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use rumqttc::{
    AsyncClient, ConnectReturnCode, Event, Incoming, LastWill, MqttOptions, Outgoing, QoS,
    TlsConfiguration, Transport,
};
use rustls::pki_types::CertificateDer;
use rustls::pki_types::pem::PemObject;
use tokio::sync::{Notify, mpsc, watch};

use super::backoff::Backoff;
use super::queue::{MAX_MESSAGE, Next, Outbox, Outcome, Queued, Report};
use super::settings::MqttSettings;
use crate::graphs::secrets::Password;

pub const KEEP_ALIVE: Duration = Duration::from_secs(30);
const ONLINE: &str = "online";
const OFFLINE: &str = "offline";
/// The capacity of the request channel of `rumqttc`.
const REQUEST_CHANNEL: usize = 32;
/// The messages that are with `rumqttc` and not yet acknowledged. It stays below the request
/// channel, so `try_publish` never finds the channel full.
const WINDOW: usize = 16;
/// The limit of an outgoing packet. A message of `MAX_MESSAGE` bytes fits with the room for the
/// fixed header, the length of the topic and the packet id. The CONNECT packet fits too.
const OUTGOING_PACKET: usize = MAX_MESSAGE + 1024;

/// What one connection needs. A change of any field closes the connection and opens a new one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    pub tls: bool,
    /// The path of a CA file in PEM form. Empty: the system certificates only.
    pub ca_file: String,
    pub user: String,
    pub password: Password,
    pub client_id: String,
    pub base_topic: String,
    /// The time between two sets of sensor values.
    pub interval: Duration,
    extra_roots: Vec<Vec<u8>>,
}

impl MqttConfig {
    /// The config of the settings, or `None` while MQTT is off (the host is empty).
    pub fn new(settings: &MqttSettings, password: &Password, hostname: &str) -> Option<Self> {
        settings.is_enabled().then(|| Self {
            host: settings.host.trim().to_owned(),
            port: settings.effective_port(),
            tls: settings.tls,
            ca_file: settings.ca_file.trim().to_owned(),
            user: settings.user.clone(),
            password: password.clone(),
            client_id: settings.effective_client_id(hostname),
            base_topic: settings.base_topic.clone(),
            interval: settings.interval(),
            extra_roots: Vec::new(),
        })
    }

    /// Trust one more root certificate (DER) next to the system certificates and the CA file. This
    /// only adds a trust anchor. The client has no way to skip the check of the certificate.
    pub fn with_extra_root(mut self, der: Vec<u8>) -> Self {
        self.extra_roots.push(der);
        self
    }

    /// `<base>/<tail>`.
    pub fn topic(&self, tail: &str) -> String {
        format!("{}/{tail}", self.base_topic)
    }

    pub fn status_topic(&self) -> String {
        self.topic("status")
    }
}

/// The state of the connection, for the settings screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MqttStatus {
    Off,
    Connecting,
    Connected,
    /// The last error text. It holds no password.
    Failed(String),
}

/// What happened to a response message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fate {
    /// The broker acknowledged the message.
    Acknowledged,
    /// The queue, a clear of the settings or the size limit dropped the message, or `rumqttc`
    /// refused it.
    Dropped,
}

/// The end of a response message. Each response id gets one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResponseEnd {
    pub response_id: u64,
    pub fate: Fate,
}

struct State {
    /// False while the host is empty: nothing is queued.
    enabled: bool,
    outbox: Outbox,
}

struct Shared {
    state: Mutex<State>,
    wake: Notify,
    config: watch::Sender<Option<MqttConfig>>,
    status: watch::Sender<MqttStatus>,
    ends: mpsc::UnboundedSender<ResponseEnd>,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn set_status(&self, status: MqttStatus) {
        self.status.send_replace(status);
    }

    fn end(&self, response_id: u64, fate: Fate) {
        let _ = self.ends.send(ResponseEnd { response_id, fate });
    }

    fn dropped(&self, reports: impl IntoIterator<Item = Report>) {
        for report in reports {
            self.end(report.response_id, Fate::Dropped);
        }
    }
}

/// The handle of the client. A clone goes to each thread that sends a message.
#[derive(Clone)]
pub struct MqttHandle {
    shared: Arc<Shared>,
}

impl MqttHandle {
    /// Start the task on `runtime`. The client is off until `configure` gives it a config. The
    /// receiver gets the end of each response message.
    pub fn spawn(runtime: &tokio::runtime::Handle) -> (Self, mpsc::UnboundedReceiver<ResponseEnd>) {
        let (ends, ends_rx) = mpsc::unbounded_channel();
        let (config, _) = watch::channel(None);
        let (status, _) = watch::channel(MqttStatus::Off);
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                enabled: false,
                outbox: Outbox::new(),
            }),
            wake: Notify::new(),
            config,
            status,
            ends,
        });
        runtime.spawn(run(Arc::clone(&shared)));
        (Self { shared }, ends_rx)
    }

    /// Set the config, or `None` for MQTT off. A config that differs from the old one closes the
    /// connection and opens a new one. `None` drops the values and the queue, and a response
    /// message in the queue ends as `Dropped`.
    pub fn configure(&self, config: Option<MqttConfig>) {
        let old_base = self
            .shared
            .config
            .borrow()
            .as_ref()
            .map(|old| old.base_topic.clone());
        let reports = {
            let mut state = self.shared.lock();
            state.enabled = config.is_some();
            match &config {
                None => state.outbox.clear(),
                Some(new) => {
                    // The values wait under topics of the old base.
                    if old_base.as_deref() != Some(new.base_topic.as_str()) {
                        state.outbox.clear_values();
                    }
                    Vec::new()
                }
            }
        };
        self.shared.dropped(reports);
        self.shared.config.send_if_modified(|current| {
            if *current == config {
                false
            } else {
                *current = config;
                true
            }
        });
    }

    /// QoS 0, retained. A new value for a topic replaces the one that waits. Nothing happens while
    /// MQTT is off.
    pub fn set_value(&self, topic: &str, payload: Vec<u8>) {
        {
            let mut state = self.shared.lock();
            if !state.enabled {
                return;
            }
            state.outbox.set_value(topic, payload);
        }
        self.shared.wake.notify_one();
    }

    /// QoS 1, no retain. False while MQTT is off: the message is not queued. At 256 messages the
    /// oldest one goes, and a response message that goes ends as `Dropped`. A message of more than
    /// `MAX_MESSAGE` bytes (topic and payload) is not queued and ends as `Dropped` too.
    pub fn enqueue(&self, topic: &str, payload: Vec<u8>, response_id: Option<u64>) -> bool {
        let report = {
            let mut state = self.shared.lock();
            if !state.enabled {
                return false;
            }
            state.outbox.enqueue(topic, payload, response_id)
        };
        self.shared.dropped(report);
        self.shared.wake.notify_one();
        true
    }

    pub fn is_enabled(&self) -> bool {
        self.shared.lock().enabled
    }

    pub fn base_topic(&self) -> Option<String> {
        self.shared
            .config
            .borrow()
            .as_ref()
            .map(|config| config.base_topic.clone())
    }

    pub fn status(&self) -> MqttStatus {
        self.shared.status.borrow().clone()
    }

    pub fn watch_status(&self) -> watch::Receiver<MqttStatus> {
        self.shared.status.subscribe()
    }

    pub fn watch_config(&self) -> watch::Receiver<Option<MqttConfig>> {
        self.shared.config.subscribe()
    }

    pub fn queue_len(&self) -> usize {
        self.shared.lock().outbox.queue_len()
    }

    pub fn value_count(&self) -> usize {
        self.shared.lock().outbox.value_count()
    }
}

/// How one connection ended.
enum Over {
    /// The config changed.
    Changed,
    /// The sender of the config is gone.
    Closed,
    Lost {
        error: String,
        /// How long the connection was up. `None` when it never came up.
        up_for: Option<Duration>,
    },
}

/// The task: one connection at a time for the current config, with the reconnect waits of
/// `Backoff`.
async fn run(shared: Arc<Shared>) {
    let mut config_rx = shared.config.subscribe();
    loop {
        let config = config_rx.borrow_and_update().clone();
        match config {
            Some(config) => {
                if !session(&shared, &mut config_rx, config).await {
                    return;
                }
            }
            None => {
                shared.set_status(MqttStatus::Off);
                if config_rx.changed().await.is_err() {
                    return;
                }
            }
        }
    }
}

/// Connect again and again for one config. It returns when the config changes. False when the
/// channel of the config is closed and the task must end.
async fn session(
    shared: &Shared,
    config_rx: &mut watch::Receiver<Option<MqttConfig>>,
    config: MqttConfig,
) -> bool {
    let mut backoff = Backoff::new();
    shared.set_status(MqttStatus::Connecting);
    loop {
        let over = match options(&config) {
            Ok(options) => connection(shared, config_rx, options, &config.status_topic()).await,
            Err(error) => Over::Lost {
                error,
                up_for: None,
            },
        };
        match over {
            Over::Changed => return true,
            Over::Closed => return false,
            Over::Lost { error, up_for } => {
                backoff.connection_ended(up_for.unwrap_or(Duration::ZERO));
                shared.set_status(MqttStatus::Failed(error));
                tokio::select! {
                    () = tokio::time::sleep(backoff.next_wait()) => {}
                    changed = config_rx.changed() => return changed.is_ok(),
                }
            }
        }
    }
}

fn options(config: &MqttConfig) -> Result<MqttOptions, String> {
    let mut options = MqttOptions::new(config.client_id.clone(), config.host.clone(), config.port);
    options.set_keep_alive(KEEP_ALIVE);
    options.set_clean_session(true);
    options.set_max_packet_size(options.max_packet_size(), OUTGOING_PACKET);
    // The broker publishes this when the connection ends with no DISCONNECT. The client sends
    // no DISCONNECT, so a reader sees `offline` after a stop, a new config or a lost line.
    options.set_last_will(LastWill::new(
        config.status_topic(),
        OFFLINE,
        QoS::AtLeastOnce,
        true,
    ));
    if !config.user.is_empty() {
        options.set_credentials(config.user.clone(), config.password.expose());
    }
    if config.tls {
        options.set_transport(Transport::tls_with_config(TlsConfiguration::Rustls(
            Arc::new(tls_config(config)?),
        )));
    }
    Ok(options)
}

/// `rustls` with the `ring` provider, the system certificates, the extra roots and the CA file.
/// The check of the certificate and of the host name always runs.
fn tls_config(config: &MqttConfig) -> Result<rustls::ClientConfig, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = rustls::RootCertStore::empty();
    for cert in rustls_native_certs::load_native_certs().certs {
        // A system certificate that `rustls` cannot parse is skipped.
        let _ = roots.add(cert);
    }
    for der in &config.extra_roots {
        roots
            .add(CertificateDer::from(der.clone()))
            .map_err(|error| format!("root certificate: {error}"))?;
    }
    if !config.ca_file.is_empty() {
        let file_error =
            |error: &dyn std::fmt::Display| format!("CA file {}: {error}", config.ca_file);
        let mut found = false;
        for cert in
            CertificateDer::pem_file_iter(&config.ca_file).map_err(|error| file_error(&error))?
        {
            let cert = cert.map_err(|error| file_error(&error))?;
            roots.add(cert).map_err(|error| file_error(&error))?;
            found = true;
        }
        if !found {
            return Err(file_error(&"no certificate found"));
        }
    }
    Ok(rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|error| error.to_string())?
        .with_root_certificates(roots)
        .with_no_client_auth())
}

/// What `rumqttc` was given, in the order it was given. Each `Outgoing::Publish` event belongs
/// to the first entry.
enum Handed {
    Value,
    Status,
    Queued(Queued),
}

#[derive(Default)]
struct Link {
    up: bool,
    /// Given to `rumqttc`, no `Outgoing::Publish` event yet.
    sent: VecDeque<Handed>,
    /// Written with a packet id, no PUBACK yet.
    inflight: HashMap<u16, Queued>,
}

impl Link {
    fn outstanding(&self) -> usize {
        self.sent.len() + self.inflight.len()
    }

    /// Hand messages to `rumqttc` while the connection is up: the queue first, then the values.
    fn pump(&mut self, shared: &Shared, raw: &AsyncClient) {
        if !self.up {
            return;
        }
        let mut state = shared.lock();
        while self.outstanding() < WINDOW {
            let Some(next) = state.outbox.pop_next() else {
                return;
            };
            match next {
                Next::Queued(message) => {
                    let payload = message.payload.clone();
                    if raw
                        .try_publish(message.topic.as_str(), QoS::AtLeastOnce, false, payload)
                        .is_ok()
                    {
                        self.sent.push_back(Handed::Queued(message));
                    } else {
                        // A message that `rumqttc` refuses (a bad topic) must not block the queue.
                        eprintln!("nicecooling: mqtt: dropped a message for {}", message.topic);
                        if let Some(id) = message.response_id {
                            shared.end(id, Fate::Dropped);
                        }
                    }
                }
                Next::Value { topic, payload } => {
                    if raw
                        .try_publish(topic.as_str(), QoS::AtMostOnce, true, payload)
                        .is_ok()
                    {
                        self.sent.push_back(Handed::Value);
                    } else {
                        eprintln!("nicecooling: mqtt: dropped a value for {topic}");
                    }
                }
            }
        }
    }

    /// The connection is gone. Each message that has no PUBACK goes back to the front of the
    /// queue. The request channel of `rumqttc` is empty here: its event loop drained it before it
    /// returned the error, and the client drops the event loop. While MQTT is off the messages go.
    fn down(&mut self, shared: &Shared) {
        self.up = false;
        let mut back: Vec<Queued> = self.inflight.drain().map(|(_, message)| message).collect();
        back.extend(self.sent.drain(..).filter_map(|handed| match handed {
            Handed::Queued(message) => Some(message),
            Handed::Value | Handed::Status => None,
        }));
        let reports = {
            let mut state = shared.lock();
            if state.enabled {
                state.outbox.requeue(back)
            } else {
                back.iter()
                    .filter_map(|message| message.response_id)
                    .map(|response_id| Report {
                        response_id,
                        outcome: Outcome::Failed,
                    })
                    .collect()
            }
        };
        shared.dropped(reports);
    }
}

/// One connection of one config: connect, publish `online`, hand over the outbox, map each
/// acknowledgment to its message. It ends with the first error of the event loop or with a change
/// of the config. It sends no DISCONNECT.
async fn connection(
    shared: &Shared,
    config_rx: &mut watch::Receiver<Option<MqttConfig>>,
    options: MqttOptions,
    status_topic: &str,
) -> Over {
    let (raw, mut eventloop) = AsyncClient::new(options, REQUEST_CHANNEL);
    let mut link = Link::default();
    let mut up_since: Option<Instant> = None;
    loop {
        // The poll future stays pinned: a wake-up pauses it and never drops it.
        let event = {
            let mut poll = std::pin::pin!(eventloop.poll());
            loop {
                tokio::select! {
                    event = &mut poll => break event,
                    () = shared.wake.notified() => link.pump(shared, &raw),
                    changed = config_rx.changed() => {
                        link.down(shared);
                        return if changed.is_ok() { Over::Changed } else { Over::Closed };
                    }
                }
            }
        };
        match event {
            Ok(Event::Incoming(Incoming::ConnAck(ack))) => {
                if ack.code == ConnectReturnCode::Success {
                    up_since = Some(Instant::now());
                    link.up = true;
                    shared.lock().outbox.mark_all_unsent();
                    shared.set_status(MqttStatus::Connected);
                    // It goes first, so a reader sees `online` before the values.
                    if raw
                        .try_publish(status_topic, QoS::AtLeastOnce, true, ONLINE)
                        .is_ok()
                    {
                        link.sent.push_back(Handed::Status);
                    }
                }
            }
            Ok(Event::Outgoing(Outgoing::Publish(pkid))) => {
                if let Some(Handed::Queued(message)) = link.sent.pop_front()
                    && pkid != 0
                {
                    link.inflight.insert(pkid, message);
                }
            }
            Ok(Event::Incoming(Incoming::PubAck(ack))) => {
                if let Some(message) = link.inflight.remove(&ack.pkid)
                    && let Some(id) = message.response_id
                {
                    shared.end(id, Fate::Acknowledged);
                }
            }
            // A publish of the broker and every other packet: the client never subscribes and
            // ignores them.
            Ok(_) => {}
            Err(error) => {
                link.down(shared);
                return Over::Lost {
                    error: error.to_string(),
                    up_for: up_since.map(|since| since.elapsed()),
                };
            }
        }
        link.pump(shared, &raw);
    }
}
