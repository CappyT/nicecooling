//! "Try": the evaluation of a draft with test values, for 1 to 600 s.
//!
//! The draft has its own evaluator and its own node state. A sensor source node with a test value
//! gives that value. A source with no test value reads its live value. A test value that is not
//! finite gives the source no value. A try gives no Graph sensor value to the daemon and runs no
//! response: the service only shows the node values.

use std::fmt;
use std::time::Duration;

use nicecooling_graph::eval::{Evaluator, GraphPass, Sources};
use nicecooling_graph::funcs::source_value;
use nicecooling_graph::model::{Graph, Kind, Node};
use nicecooling_graph::validate::{Refusal, validate};

use super::channel::TryRequest;
use super::store::describe;

pub const MIN_SECONDS: u32 = 1;
pub const MAX_SECONDS: u32 = 600;

#[derive(Clone, Debug, PartialEq)]
pub enum TryError {
    /// The test time is outside 1 to 600 s.
    BadTime(u32),
    /// The draft breaks a rule of the validator.
    Invalid(Vec<Refusal>),
}

impl fmt::Display for TryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadTime(seconds) => write!(
                f,
                "the test time is {seconds} s, and it is {MIN_SECONDS} to {MAX_SECONDS} s"
            ),
            Self::Invalid(refusals) => write!(f, "the draft is not valid: {}", describe(refusals)),
        }
    }
}

impl std::error::Error for TryError {}

/// One try that runs.
pub struct TryRun {
    draft: Graph,
    evaluator: Evaluator,
    ends: Duration,
}

impl TryRun {
    /// Start a try at `now`. `others` are the stored Graphs, for the validator.
    pub fn start(request: &TryRequest, others: &[Graph], now: Duration) -> Result<Self, TryError> {
        if !(MIN_SECONDS..=MAX_SECONDS).contains(&request.seconds) {
            return Err(TryError::BadTime(request.seconds));
        }
        let refusals = validate(&request.graph, others);
        if !refusals.is_empty() {
            return Err(TryError::Invalid(refusals));
        }
        let mut draft = request.graph.clone();
        draft.enabled = true;
        // A source with a test value becomes a constant with that value. It keeps its id and its
        // label, so the node values of the draft read as they do in the editor.
        for (id, value) in &request.tests {
            let Some(node) = draft
                .nodes
                .iter_mut()
                .find(|n| n.id == *id && n.kind == Kind::SensorSource)
            else {
                continue;
            };
            if !value.is_finite() {
                // A test value that is not a number leaves the source on no sensor: it gives no
                // value, and not a live reading that the user did not ask for.
                node.params
                    .insert("sensor".to_owned(), String::new().into());
                continue;
            }
            let constant =
                Node::new(node.id, Kind::Constant).with_param("value", source_value(*value));
            *node = Node {
                label: node.label.take(),
                x: node.x,
                y: node.y,
                ..constant
            };
        }
        Ok(Self {
            draft,
            evaluator: Evaluator::new(),
            ends: now + Duration::from_secs(u64::from(request.seconds)),
        })
    }

    pub fn graph_id(&self) -> &str {
        &self.draft.id
    }

    /// Whether the test time is over.
    pub fn finished(&self, now: Duration) -> bool {
        now >= self.ends
    }

    pub fn seconds_left(&self, now: Duration) -> u64 {
        self.ends.saturating_sub(now).as_secs_f64().ceil() as u64
    }

    /// One evaluation of the draft. `None` when it panicked.
    pub fn step(&mut self, sources: &dyn Sources, now: Duration) -> Option<GraphPass> {
        let pass = self
            .evaluator
            .evaluate(std::slice::from_ref(&self.draft), sources, now);
        if pass.panicked.is_empty() {
            pass.graphs.into_iter().next()
        } else {
            None
        }
    }
}
