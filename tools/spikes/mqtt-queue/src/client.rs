//! The pattern for the app: the queue lives outside `rumqttc`. One task drives the event loop
//! and hands messages to the client only while the connection is up.

use rumqttc::{AsyncClient, Event, EventLoop, Incoming, MqttOptions, Outgoing, QoS};
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{Notify, mpsc};

pub const QUEUE_CAP: usize = 256;
/// Messages that are with `rumqttc` and not yet acknowledged. It stays below the request channel.
const WINDOW: usize = 16;
const REQUEST_CHANNEL: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Ok,
    Failed,
}

/// The end of one response message. Each response id gets exactly one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub id: u64,
    pub outcome: Outcome,
}

#[derive(Clone, Debug)]
struct Queued {
    topic: String,
    payload: Vec<u8>,
    /// Set for a response message. An event message has none.
    response_id: Option<u64>,
    /// The order of the enqueue calls, to put a message back in its place.
    seq: u64,
}

#[derive(Default)]
struct Shared {
    /// One value for each sensor topic, with a flag for "not sent yet".
    slots: HashMap<String, (Vec<u8>, bool)>,
    queue: VecDeque<Queued>,
    next_seq: u64,
}

pub struct Client {
    shared: Arc<Mutex<Shared>>,
    wake: Arc<Notify>,
    reports: mpsc::UnboundedSender<Report>,
}

impl Client {
    pub fn start(
        options: MqttOptions,
        reports: mpsc::UnboundedSender<Report>,
    ) -> (Client, tokio::task::JoinHandle<()>) {
        let (raw, eventloop) = AsyncClient::new(options, REQUEST_CHANNEL);
        let shared = Arc::new(Mutex::new(Shared::default()));
        let wake = Arc::new(Notify::new());
        let task = tokio::spawn(run(
            shared.clone(),
            wake.clone(),
            raw,
            eventloop,
            reports.clone(),
        ));
        (
            Client {
                shared,
                wake,
                reports,
            },
            task,
        )
    }

    /// QoS 0, retained. A new value for a topic replaces the one that waits.
    pub fn set_value(&self, topic: &str, payload: Vec<u8>) {
        self.shared
            .lock()
            .unwrap()
            .slots
            .insert(topic.to_owned(), (payload, true));
        self.wake.notify_one();
    }

    /// QoS 1, no retain. At 256 messages the oldest one goes, and a response gets `Failed`.
    pub fn enqueue(&self, topic: &str, payload: Vec<u8>, response_id: Option<u64>) {
        let mut s = self.shared.lock().unwrap();
        s.next_seq += 1;
        let seq = s.next_seq;
        s.queue.push_back(Queued {
            topic: topic.to_owned(),
            payload,
            response_id,
            seq,
        });
        trim(&mut s.queue, &self.reports);
        drop(s);
        self.wake.notify_one();
    }

    pub fn queue_len(&self) -> usize {
        self.shared.lock().unwrap().queue.len()
    }

    pub fn slot_count(&self) -> usize {
        self.shared.lock().unwrap().slots.len()
    }
}

fn trim(queue: &mut VecDeque<Queued>, reports: &mpsc::UnboundedSender<Report>) {
    while queue.len() > QUEUE_CAP {
        let gone = queue.pop_front().expect("queue is not empty");
        eprintln!(
            "mqtt queue full: dropped the oldest message for {}",
            gone.topic
        );
        if let Some(id) = gone.response_id {
            let _ = reports.send(Report {
                id,
                outcome: Outcome::Failed,
            });
        }
    }
}

/// What `rumqttc` was given, in the order it was given.
enum Handed {
    Slot,
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

    fn pump(&mut self, shared: &Mutex<Shared>, raw: &AsyncClient) {
        if !self.up {
            return;
        }
        let mut guard = shared.lock().unwrap();
        let s = &mut *guard;
        while self.outstanding() < WINDOW {
            if let Some(m) = s.queue.front() {
                if raw
                    .try_publish(&m.topic, QoS::AtLeastOnce, false, m.payload.clone())
                    .is_err()
                {
                    return;
                }
                let m = s.queue.pop_front().expect("front exists");
                self.sent.push_back(Handed::Queued(m));
                continue;
            }
            let Some((topic, (payload, dirty))) = s.slots.iter_mut().find(|(_, v)| v.1) else {
                return;
            };
            if raw
                .try_publish(topic.as_str(), QoS::AtMostOnce, true, payload.clone())
                .is_err()
            {
                return;
            }
            *dirty = false;
            self.sent.push_back(Handed::Slot);
        }
    }

    /// The connection is gone. Everything that has no PUBACK goes back to the front of the queue.
    /// The request channel of `rumqttc` is empty here: its event loop drained it before it
    /// returned the error, and nothing is handed over while the link is down.
    fn down(&mut self, shared: &Mutex<Shared>, reports: &mpsc::UnboundedSender<Report>) {
        self.up = false;
        let mut back: Vec<Queued> = self.inflight.drain().map(|(_, m)| m).collect();
        back.extend(self.sent.drain(..).filter_map(|h| match h {
            Handed::Queued(m) => Some(m),
            Handed::Slot => None,
        }));
        back.sort_by_key(|m| m.seq);
        let mut s = shared.lock().unwrap();
        for m in back.into_iter().rev() {
            s.queue.push_front(m);
        }
        trim(&mut s.queue, reports);
    }
}

async fn run(
    shared: Arc<Mutex<Shared>>,
    wake: Arc<Notify>,
    raw: AsyncClient,
    mut eventloop: EventLoop,
    reports: mpsc::UnboundedSender<Report>,
) {
    let mut link = Link::default();
    loop {
        // The poll future stays pinned: a wake-up pauses it and never drops it.
        let event = {
            let mut poll = std::pin::pin!(eventloop.poll());
            loop {
                tokio::select! {
                    e = &mut poll => break e,
                    () = wake.notified() => link.pump(&shared, &raw),
                }
            }
        };
        match event {
            Ok(Event::Incoming(Incoming::ConnAck(ack))) => {
                if ack.code == rumqttc::ConnectReturnCode::Success {
                    link.up = true;
                    for slot in shared.lock().unwrap().slots.values_mut() {
                        slot.1 = true;
                    }
                }
            }
            Ok(Event::Outgoing(Outgoing::Publish(pkid))) => match link.sent.pop_front() {
                Some(Handed::Queued(m)) if pkid != 0 => {
                    link.inflight.insert(pkid, m);
                }
                _ => {}
            },
            Ok(Event::Incoming(Incoming::PubAck(ack))) => {
                if let Some(m) = link.inflight.remove(&ack.pkid)
                    && let Some(id) = m.response_id
                {
                    let _ = reports.send(Report {
                        id,
                        outcome: Outcome::Ok,
                    });
                }
            }
            Ok(_) => {}
            Err(_) => {
                link.down(&shared, &reports);
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
        link.pump(&shared, &raw);
    }
}
