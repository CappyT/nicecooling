//! What the app publishes through the MQTT client: the sensor values, the Alarm events and the
//! MQTT message response (`openspec/changes/add-node-graph/specs/mqtt-publishing`).
//!
//! The `Publisher` follows the daemon events like the history writer does. It keeps its own
//! `AppState`, so the Graph service takes no part: the Graph sensors reach MQTT as the sensors of
//! the daemon, and a Graph sensor of a disabled Graph has the "no value" flag there.
//!
//! The Graph service sends an MQTT message response as a `ResponseRequest` on the channel of the
//! `ResponseSender` and reads its `ResponseDone` from the channel that `Publisher::spawn` returns.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use nicecooling_core::clock::WallClock;
use serde_json::{Map, Value, json};
use tokio::sync::{broadcast, mpsc, watch};
use tokio::time::Instant;

use super::client::{Fate, MqttConfig, MqttHandle, MqttStatus, ResponseEnd};
use super::settings::DEFAULT_INTERVAL_S;
use crate::model::{AlarmDetails, AlarmEntry, AppState, DaemonEvent, Severity, Snapshot};
use crate::screens::format::unit_text;

/// The time that a response message has to get its acknowledgment.
pub const RESPONSE_LIMIT: Duration = Duration::from_secs(30);

/// A deadline for a wait that has nothing to wait for.
const IDLE: Duration = Duration::from_secs(3600);

/// The message of a sensor: `value` is `null` for a sensor with no value.
pub fn sensor_payload(name: &str, unit: &str, value: Option<f64>, time_us: u64) -> Vec<u8> {
    json!({
        "value": value,
        "unit": unit_text(unit),
        "name": name,
        "time": time_us,
    })
    .to_string()
    .into_bytes()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventState {
    Raised,
    Cleared,
}

fn details_json(details: &AlarmDetails) -> Value {
    let mut map = Map::new();
    if let Some(value) = details.value {
        map.insert("value".to_owned(), json!(value));
    }
    if let Some(limit) = details.limit {
        map.insert("limit".to_owned(), json!(limit));
    }
    if !details.unit.is_empty() {
        map.insert("unit".to_owned(), json!(unit_text(&details.unit)));
    }
    if let Some(sensor) = &details.sensor {
        map.insert("sensor".to_owned(), json!(sensor));
    }
    if details.latched_bits != 0 {
        map.insert("latched_bits".to_owned(), json!(details.latched_bits));
    }
    Value::Object(map)
}

/// The message of an Alarm or Warning event.
pub fn event_payload(alarm: &AlarmEntry, state: EventState, time_us: u64) -> Vec<u8> {
    let severity = match alarm.severity {
        Severity::Alarm => "alarm",
        Severity::Warning => "warning",
    };
    let state = match state {
        EventState::Raised => "raised",
        EventState::Cleared => "cleared",
    };
    json!({
        "event": alarm.event,
        "source": alarm.source,
        "severity": severity,
        "state": state,
        "time": time_us,
        "details": details_json(&alarm.details),
    })
    .to_string()
    .into_bytes()
}

/// The payload of an MQTT message response with an empty template.
pub fn default_response_payload(graph_id: &str, graph_name: &str, time_us: u64) -> Vec<u8> {
    json!({
        "graph_id": graph_id,
        "graph_name": graph_name,
        "time": time_us,
    })
    .to_string()
    .into_bytes()
}

/// How an MQTT message response ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResponseOutcome {
    /// The broker acknowledged the message.
    Ok,
    /// No acknowledgment in 30 s. The message stays in the queue and goes out later.
    Timeout,
    /// The queue dropped the message.
    Failed,
    /// MQTT is off: no host is set.
    NotConfigured,
}

impl ResponseOutcome {
    /// The outcome name of `graph-responses`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Timeout => "timeout",
            Self::Failed => "failed",
            Self::NotConfigured => "not-configured",
        }
    }
}

/// One MQTT message response. `id` is the run id of the response. The payload is the expanded
/// text, or `default_response_payload` for an empty template.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponseRequest {
    pub id: u64,
    /// The topic below the base topic: `alerts/water` goes to `<base>/alerts/water`.
    pub topic: String,
    pub payload: Vec<u8>,
}

/// The outcome of one `ResponseRequest`. It comes once for each request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResponseDone {
    pub id: u64,
    pub outcome: ResponseOutcome,
}

/// The sending end for the Graph service. It never blocks.
#[derive(Clone, Debug)]
pub struct ResponseSender(mpsc::UnboundedSender<ResponseRequest>);

impl ResponseSender {
    /// False when the publisher task has ended.
    pub fn send(&self, request: ResponseRequest) -> bool {
        self.0.send(request).is_ok()
    }
}

/// The response messages that wait for an acknowledgment.
struct Responses {
    limit: Duration,
    /// The end of the time limit of each message.
    pending: HashMap<u64, Instant>,
    done: mpsc::UnboundedSender<ResponseDone>,
}

impl Responses {
    fn new(limit: Duration, done: mpsc::UnboundedSender<ResponseDone>) -> Self {
        Self {
            limit,
            pending: HashMap::new(),
            done,
        }
    }

    fn finish(&self, id: u64, outcome: ResponseOutcome) {
        let _ = self.done.send(ResponseDone { id, outcome });
    }

    fn started(&mut self, id: u64) {
        self.pending.insert(id, Instant::now() + self.limit);
    }

    /// The client reports the end of a message. A message that timed out has no entry any more:
    /// its outcome stays `timeout`.
    fn ended(&mut self, end: ResponseEnd) {
        if self.pending.remove(&end.response_id).is_none() {
            return;
        }
        let outcome = match end.fate {
            Fate::Acknowledged => ResponseOutcome::Ok,
            Fate::Dropped => ResponseOutcome::Failed,
        };
        self.finish(end.response_id, outcome);
    }

    /// Every message that has no end at `now` and is past its limit gets `timeout`.
    fn expire(&mut self, now: Instant) {
        let mut due: Vec<u64> = self
            .pending
            .iter()
            .filter(|(_, deadline)| **deadline <= now)
            .map(|(id, _)| *id)
            .collect();
        due.sort_unstable();
        for id in due {
            self.pending.remove(&id);
            self.finish(id, ResponseOutcome::Timeout);
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        self.pending.values().min().copied()
    }
}

/// The time between two sets of sensor values. MQTT off has no interval: the default does no harm,
/// because nothing is published.
fn period(config: &watch::Receiver<Option<MqttConfig>>) -> Duration {
    config
        .borrow()
        .as_ref()
        .map_or(
            Duration::from_secs(u64::from(DEFAULT_INTERVAL_S)),
            |config| config.interval,
        )
        .max(Duration::from_millis(1))
}

pub struct Publisher {
    client: MqttHandle,
    /// The daemon state: the sensors and their values.
    state: AppState,
    /// The sensors that the user hid on the Sensors screen.
    hidden: Arc<Mutex<BTreeSet<String>>>,
    clock: Arc<dyn WallClock>,
    /// The Alarms that this publisher sent as raised, by instance. A clear message takes its
    /// fields from here.
    raised: HashMap<String, AlarmEntry>,
    response_limit: Duration,
}

impl Publisher {
    pub fn new(
        client: MqttHandle,
        hidden: Arc<Mutex<BTreeSet<String>>>,
        clock: Arc<dyn WallClock>,
    ) -> Self {
        Self {
            client,
            state: AppState::default(),
            hidden,
            clock,
            raised: HashMap::new(),
            response_limit: RESPONSE_LIMIT,
        }
    }

    /// A shorter limit than `RESPONSE_LIMIT`, so a test does not wait 30 s.
    pub fn with_response_limit(mut self, limit: Duration) -> Self {
        self.response_limit = limit;
        self
    }

    /// Follow one daemon event: keep the state, and publish the Alarm events.
    pub fn apply(&mut self, event: &DaemonEvent) {
        self.state.apply(event);
        let Some(base) = self.client.base_topic() else {
            // MQTT is off. When it turns on, the Alarms that are active then are "active at start".
            self.raised.clear();
            return;
        };
        match event {
            DaemonEvent::AlarmRaised(alarm) => self.raise(alarm, &base),
            DaemonEvent::AlarmCleared { instance } => self.clear(instance, &base),
            DaemonEvent::Snapshot(snapshot) => self.reconcile(snapshot, &base),
            _ => {}
        }
    }

    /// A raise that this publisher has not sent yet. An Alarm that is active when the app starts
    /// or reads the daemon again comes in a snapshot, never as a raise, so it gets no message.
    fn raise(&mut self, alarm: &AlarmEntry, base: &str) {
        if self.raised.contains_key(&alarm.instance) {
            return;
        }
        let mut stored = alarm.clone();
        // The signal has no raise time.
        if stored.raised_us == 0 {
            stored.raised_us = self.clock.now_us();
        }
        self.send_event(&stored, EventState::Raised, stored.raised_us, base);
        self.raised.insert(stored.instance.clone(), stored);
    }

    /// The clear of an Alarm that this publisher sent as raised. The clear of any other Alarm gets
    /// no message: it has no fields to send.
    fn clear(&mut self, instance: &str, base: &str) {
        if let Some(stored) = self.raised.remove(instance) {
            self.send_event(&stored, EventState::Cleared, self.clock.now_us(), base);
        }
    }

    /// A full read of the daemon. An Alarm that this publisher sent as raised and that the read
    /// no longer lists has closed while the app did not see it: it gets its clear message. After
    /// a restart of the daemon the same Alarm can be active under a new instance. It stays raised.
    fn reconcile(&mut self, snapshot: &Snapshot, base: &str) {
        let gone: Vec<String> = self
            .raised
            .keys()
            .filter(|instance| {
                !snapshot
                    .alarms
                    .iter()
                    .any(|active| &active.instance == *instance)
            })
            .cloned()
            .collect();
        for instance in gone {
            let Some(stored) = self.raised.remove(&instance) else {
                continue;
            };
            let same = snapshot
                .alarms
                .iter()
                .find(|active| active.event == stored.event && active.source == stored.source);
            match same {
                Some(active) => {
                    self.raised.insert(active.instance.clone(), stored);
                }
                None => self.send_event(&stored, EventState::Cleared, self.clock.now_us(), base),
            }
        }
    }

    fn send_event(&self, alarm: &AlarmEntry, state: EventState, time_us: u64, base: &str) {
        let topic = format!("{base}/event/{}", alarm.event);
        self.client
            .enqueue(&topic, event_payload(alarm, state, time_us), None);
    }

    /// Set the message of each sensor that the user did not hide. The client sends it at once when
    /// the connection is up, and keeps the newest one while it is down.
    pub fn publish_values(&self) {
        let Some(base) = self.client.base_topic() else {
            return;
        };
        let hidden = self
            .hidden
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        let now = self.clock.now_us();
        let sample_time = self.state.sensors.as_ref().map(|sample| sample.time_us);
        for (id, info) in &self.state.snapshot.sensors {
            if hidden.contains(id) {
                continue;
            }
            // While the daemon is away, or when the sensor has the "no value" flag, the value is
            // `null`: a reader sees the loss.
            let value = self.state.sensor_value(id);
            let time_us = match (value, sample_time) {
                (Some(_), Some(time_us)) => time_us,
                _ => now,
            };
            self.client.set_value(
                &format!("{base}/sensor/{id}"),
                sensor_payload(&info.name, &info.unit, value, time_us),
            );
        }
    }

    /// The config changed. MQTT off forgets the Alarms that were sent as raised.
    fn config_changed(&mut self) {
        if self.client.base_topic().is_none() {
            self.raised.clear();
        }
        self.publish_values();
    }

    fn start_response(&mut self, request: ResponseRequest, responses: &mut Responses) {
        let Some(base) = self.client.base_topic() else {
            responses.finish(request.id, ResponseOutcome::NotConfigured);
            return;
        };
        let topic = format!("{base}/{}", request.topic);
        if self
            .client
            .enqueue(&topic, request.payload, Some(request.id))
        {
            responses.started(request.id);
        } else {
            responses.finish(request.id, ResponseOutcome::NotConfigured);
        }
    }

    /// Run the publisher as a task on `runtime`. `ends` is the receiver that
    /// `MqttHandle::spawn` returned. The first result is the sender of the MQTT message responses,
    /// the second one gets their outcomes.
    pub fn spawn(
        self,
        runtime: &tokio::runtime::Handle,
        ends: mpsc::UnboundedReceiver<ResponseEnd>,
        events: broadcast::Receiver<DaemonEvent>,
    ) -> (ResponseSender, mpsc::UnboundedReceiver<ResponseDone>) {
        let (request_tx, requests) = mpsc::unbounded_channel();
        let (done_tx, done_rx) = mpsc::unbounded_channel();
        runtime.spawn(self.run(events, ends, requests, done_tx));
        (ResponseSender(request_tx), done_rx)
    }

    async fn run(
        mut self,
        mut events: broadcast::Receiver<DaemonEvent>,
        mut ends: mpsc::UnboundedReceiver<ResponseEnd>,
        mut requests: mpsc::UnboundedReceiver<ResponseRequest>,
        done: mpsc::UnboundedSender<ResponseDone>,
    ) {
        let mut config = self.client.watch_config();
        let mut status = self.client.watch_status();
        let mut responses = Responses::new(self.response_limit, done);
        // The first tick comes at once.
        let mut timer = tokio::time::interval(period(&config));
        let (mut events_open, mut requests_open) = (true, true);
        loop {
            let deadline = responses.next_deadline();
            tokio::select! {
                event = events.recv(), if events_open => match event {
                    Ok(event) => self.apply(&event),
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => events_open = false,
                },
                _ = timer.tick() => self.publish_values(),
                changed = config.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    timer = tokio::time::interval(period(&config));
                    self.config_changed();
                }
                changed = status.changed() => {
                    if changed.is_err() {
                        return;
                    }
                    // The values go out at once after a connect, not after the next interval.
                    let connected = *status.borrow_and_update() == MqttStatus::Connected;
                    if connected {
                        self.publish_values();
                    }
                }
                request = requests.recv(), if requests_open => match request {
                    Some(request) => self.start_response(request, &mut responses),
                    None => requests_open = false,
                },
                end = ends.recv() => match end {
                    Some(end) => responses.ended(end),
                    None => return,
                },
                () = tokio::time::sleep_until(deadline.unwrap_or_else(|| Instant::now() + IDLE)),
                    if deadline.is_some() => responses.expire(Instant::now()),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn done() -> (Responses, mpsc::UnboundedReceiver<ResponseDone>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Responses::new(Duration::from_secs(30), sender), receiver)
    }

    #[test]
    fn response_times_out_once_and_a_late_ack_gives_no_second_outcome() {
        let (mut responses, mut outcomes) = done();
        responses.started(1);
        let start = Instant::now();
        responses.expire(start + Duration::from_secs(20));
        assert!(outcomes.try_recv().is_err());
        responses.expire(start + Duration::from_secs(31));
        assert_eq!(
            outcomes.try_recv(),
            Ok(ResponseDone {
                id: 1,
                outcome: ResponseOutcome::Timeout
            })
        );
        responses.ended(ResponseEnd {
            response_id: 1,
            fate: Fate::Acknowledged,
        });
        assert!(outcomes.try_recv().is_err());
        assert_eq!(responses.next_deadline(), None);
    }

    #[test]
    fn response_end_gives_ok_or_failed() {
        let (mut responses, mut outcomes) = done();
        responses.started(1);
        responses.started(2);
        responses.ended(ResponseEnd {
            response_id: 2,
            fate: Fate::Dropped,
        });
        responses.ended(ResponseEnd {
            response_id: 1,
            fate: Fate::Acknowledged,
        });
        let first = outcomes.try_recv().expect("an outcome");
        let second = outcomes.try_recv().expect("an outcome");
        assert_eq!((first.id, first.outcome), (2, ResponseOutcome::Failed));
        assert_eq!((second.id, second.outcome), (1, ResponseOutcome::Ok));
        assert_eq!(ResponseOutcome::NotConfigured.id(), "not-configured");
    }
}
