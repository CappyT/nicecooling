//! What the MQTT client has to send: one pending value for each sensor topic, and a queue of
//! at most 256 messages for events and responses. The type has no network and no clock. The
//! client task asks it for the next message while the connection is up, and gives back what
//! the broker did not acknowledge when the connection ends. The queue stays across reconnects.

use std::collections::{BTreeMap, VecDeque};

pub const QUEUE_CAP: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Failed,
}

/// The end of a response message that never left the queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    pub response_id: u64,
    pub outcome: Outcome,
}

/// A message of the queue: an event (no `response_id`) or a response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Queued {
    pub topic: String,
    pub payload: Vec<u8>,
    pub response_id: Option<u64>,
    /// The order of arrival. A message that comes back after a lost connection keeps its place.
    pub seq: u64,
}

/// The next message to send.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Next {
    /// QoS 1, no retain. The caller gives it back with `requeue` if the broker does not acknowledge it.
    Queued(Queued),
    /// QoS 0, retained. The caller does not give it back.
    Value { topic: String, payload: Vec<u8> },
}

struct Slot {
    payload: Vec<u8>,
    unsent: bool,
}

pub struct Outbox {
    slots: BTreeMap<String, Slot>,
    queue: VecDeque<Queued>,
    next_seq: u64,
    journal: Box<dyn FnMut(&str) + Send>,
}

impl Default for Outbox {
    fn default() -> Self {
        Self::new()
    }
}

impl Outbox {
    /// A drop writes a line to the journal through the standard error stream of the app.
    pub fn new() -> Self {
        Self::with_journal(|line| eprintln!("{line}"))
    }

    pub fn with_journal(journal: impl FnMut(&str) + Send + 'static) -> Self {
        Self {
            slots: BTreeMap::new(),
            queue: VecDeque::new(),
            next_seq: 0,
            journal: Box::new(journal),
        }
    }

    /// A new value replaces the one that waits for the same topic.
    pub fn set_value(&mut self, topic: &str, payload: Vec<u8>) {
        self.slots.insert(
            topic.to_owned(),
            Slot {
                payload,
                unsent: true,
            },
        );
    }

    /// Add an event or a response message. At 256 messages the oldest one goes. The report is
    /// `Some` when the dropped message was a response.
    pub fn enqueue(
        &mut self,
        topic: &str,
        payload: Vec<u8>,
        response_id: Option<u64>,
    ) -> Option<Report> {
        self.next_seq += 1;
        self.queue.push_back(Queued {
            topic: topic.to_owned(),
            payload,
            response_id,
            seq: self.next_seq,
        });
        self.trim().pop()
    }

    /// The next message: the queue first, then the values that changed since their last send.
    pub fn pop_next(&mut self) -> Option<Next> {
        if let Some(message) = self.queue.pop_front() {
            return Some(Next::Queued(message));
        }
        let (topic, slot) = self.slots.iter_mut().find(|(_, slot)| slot.unsent)?;
        slot.unsent = false;
        Some(Next::Value {
            topic: topic.clone(),
            payload: slot.payload.clone(),
        })
    }

    /// The connection ended. Messages that the broker did not acknowledge go back to the front,
    /// in their order of arrival. The queue is cut to 256 again, and the oldest messages go.
    pub fn requeue(&mut self, mut back: Vec<Queued>) -> Vec<Report> {
        back.sort_by_key(|message| message.seq);
        for message in back.into_iter().rev() {
            self.queue.push_front(message);
        }
        self.trim()
    }

    /// A connection opened. Each value goes out again, so the broker has the newest ones.
    pub fn mark_all_unsent(&mut self) {
        for slot in self.slots.values_mut() {
            slot.unsent = true;
        }
    }

    /// The base topic changed. The values wait under the old topics, so they go. The queue stays.
    pub fn clear_values(&mut self) {
        self.slots.clear();
    }

    /// The host is cleared. The values and the queue go. A response message gets `Failed`.
    pub fn clear(&mut self) -> Vec<Report> {
        self.slots.clear();
        self.queue
            .drain(..)
            .filter_map(|message| failed(&message))
            .collect()
    }

    pub fn queue_len(&self) -> usize {
        self.queue.len()
    }

    pub fn value_count(&self) -> usize {
        self.slots.len()
    }

    fn trim(&mut self) -> Vec<Report> {
        let mut reports = Vec::new();
        while self.queue.len() > QUEUE_CAP {
            let Some(gone) = self.queue.pop_front() else {
                break;
            };
            // The line names the topic only. A payload can hold a text that the user wrote.
            (self.journal)(&format!(
                "nicecooling: mqtt queue is full: dropped the oldest message for {}",
                gone.topic
            ));
            reports.extend(failed(&gone));
        }
        reports
    }
}

fn failed(message: &Queued) -> Option<Report> {
    message.response_id.map(|response_id| Report {
        response_id,
        outcome: Outcome::Failed,
    })
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    fn text(next: Option<Next>) -> String {
        match next {
            Some(Next::Value { payload, .. }) => String::from_utf8(payload).expect("text"),
            Some(Next::Queued(message)) => String::from_utf8(message.payload).expect("text"),
            None => "none".to_owned(),
        }
    }

    #[test]
    fn new_value_replaces_the_old_one() {
        let mut outbox = Outbox::new();
        for n in 0..100 {
            outbox.set_value("t/a", format!("a{n}").into_bytes());
        }
        outbox.set_value("t/b", b"b0".to_vec());
        assert_eq!(outbox.value_count(), 2);
        assert_eq!(text(outbox.pop_next()), "a99");
        assert_eq!(text(outbox.pop_next()), "b0");
        assert_eq!(outbox.pop_next(), None);
        outbox.mark_all_unsent();
        assert_eq!(text(outbox.pop_next()), "a99");
    }

    #[test]
    fn clear_values_keeps_the_queue() {
        let mut outbox = Outbox::new();
        outbox.set_value("t/a", b"a".to_vec());
        outbox.enqueue("t/event/x", b"raise".to_vec(), None);
        outbox.clear_values();
        assert_eq!(outbox.value_count(), 0);
        assert_eq!(outbox.queue_len(), 1);
        assert_eq!(text(outbox.pop_next()), "raise");
        assert_eq!(outbox.pop_next(), None);
    }

    #[test]
    fn event_waits_across_a_reconnect() {
        let mut outbox = Outbox::new();
        outbox.enqueue("t/event/x", b"raise".to_vec(), None);
        outbox.enqueue("t/event/y", b"other".to_vec(), None);
        // The first connection takes the first message and ends before the acknowledgment.
        let Some(Next::Queued(first)) = outbox.pop_next() else {
            panic!("a queued message");
        };
        assert!(outbox.requeue(vec![first]).is_empty());
        assert_eq!(outbox.queue_len(), 2);
        assert_eq!(text(outbox.pop_next()), "raise");
        assert_eq!(text(outbox.pop_next()), "other");
        assert_eq!(outbox.pop_next(), None);
    }

    #[test]
    fn full_queue_drops_the_oldest() {
        let mut outbox = Outbox::new();
        for n in 0..300u64 {
            outbox.enqueue("t/event/x", n.to_string().into_bytes(), None);
        }
        assert_eq!(outbox.queue_len(), QUEUE_CAP);
        assert_eq!(text(outbox.pop_next()), "44");
        // A requeue over the limit also drops the oldest.
        let back: Vec<Queued> = (0..2)
            .map(|n| Queued {
                topic: "t/event/x".to_owned(),
                payload: b"old".to_vec(),
                response_id: None,
                seq: n,
            })
            .collect();
        outbox.requeue(back);
        assert_eq!(outbox.queue_len(), QUEUE_CAP);
    }

    #[test]
    fn dropped_response_message_is_failed() {
        let mut outbox = Outbox::new();
        assert_eq!(outbox.enqueue("t/r", b"first".to_vec(), Some(7)), None);
        for _ in 0..QUEUE_CAP - 1 {
            assert_eq!(outbox.enqueue("t/event/x", b"e".to_vec(), None), None);
        }
        assert_eq!(
            outbox.enqueue("t/event/x", b"e".to_vec(), None),
            Some(Report {
                response_id: 7,
                outcome: Outcome::Failed
            })
        );
        // An event that goes gives no report.
        assert_eq!(outbox.enqueue("t/event/x", b"e".to_vec(), None), None);
        assert_eq!(
            outbox.clear(),
            Vec::new(),
            "no response message is left in the queue"
        );
        outbox.enqueue("t/r", b"x".to_vec(), Some(9));
        assert_eq!(
            outbox.clear(),
            vec![Report {
                response_id: 9,
                outcome: Outcome::Failed
            }]
        );
        assert_eq!(outbox.queue_len(), 0);
    }

    #[test]
    fn drop_writes_a_journal_line() {
        let lines = Arc::new(Mutex::new(Vec::<String>::new()));
        let sink = Arc::clone(&lines);
        let mut outbox =
            Outbox::with_journal(move |line| sink.lock().expect("lock").push(line.to_owned()));
        for _ in 0..QUEUE_CAP {
            outbox.enqueue("t/event/x", b"secret text".to_vec(), None);
        }
        assert!(lines.lock().expect("lock").is_empty());
        outbox.enqueue("t/event/y", b"e".to_vec(), None);
        outbox.enqueue("t/event/y", b"e".to_vec(), None);
        let lines = lines.lock().expect("lock");
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("t/event/x"));
        assert!(!lines[0].contains("secret text"));
    }
}
