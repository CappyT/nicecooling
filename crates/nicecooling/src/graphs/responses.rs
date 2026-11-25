//! The responses of the Graphs in the app. This file connects the state machine of
//! `nicecooling_graph::response` to the clocks, to `graph-state.json`, to the response log and to
//! the run ids. It starts no response: a `Runner` does.
//!
//! One response end node has at most one run without an outcome. A run that starts while the
//! node still has one gets the outcome `busy`.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use nicecooling_core::clock::WallClock;
use nicecooling_graph::model::{Node, ResponseKind};
use nicecooling_graph::response::{Action, Response, RunRecord, RunState, Timing};
use nicecooling_graph::template::LabelledValue;
use serde::{Deserialize, Serialize};

/// The rows that `response-log.json` keeps.
pub const LOG_LIMIT: usize = 500;

/// How a run ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    Ok,
    Failed,
    Timeout,
    NotConfigured,
    Busy,
    Cancelled,
}

impl Outcome {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Failed => "failed",
            Self::Timeout => "timeout",
            Self::NotConfigured => "not-configured",
            Self::Busy => "busy",
            Self::Cancelled => "cancelled",
        }
    }
}

/// One row of `response-log.json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRow {
    /// The wall clock time at which the run started, in microseconds since the Unix epoch.
    pub time_us: u64,
    pub graph_id: String,
    pub graph_name: String,
    /// One of the names of `ResponseKind`.
    pub kind: String,
    pub outcome: Outcome,
}

/// One run of a response: what the `Runner` needs to start it.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// The id of the run. A command sees it as `NICECOOLING_RUN_ID`.
    pub id: String,
    /// The key of the response end node in `graph-state.json`: `<graph id>/<node id>`.
    pub key: String,
    pub graph_id: String,
    pub graph_name: String,
    /// The response end node, with its parameters at the time of the run.
    pub node: Node,
    pub kind: ResponseKind,
    /// The wall clock time at which the run starts, in microseconds since the Unix epoch.
    pub time_us: u64,
    /// The local date and time of the run as `YYYY-MM-DD HH:MM:SS`, for the placeholder `{time}`.
    pub time_text: String,
    /// The labelled nodes of the Graph with their values in the pass of the run.
    pub values: Vec<LabelledValue>,
}

impl Run {
    pub fn row(&self, outcome: Outcome) -> LogRow {
        LogRow {
            time_us: self.time_us,
            graph_id: self.graph_id.clone(),
            graph_name: self.graph_name.clone(),
            kind: self.kind.as_str().to_owned(),
            outcome,
        }
    }
}

/// What a `Runner` says when it starts a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Started {
    /// The run has its outcome now.
    Done(Outcome),
    /// The outcome comes later. `Runner::finished` gives it.
    Pending,
}

/// What starts the response of a run. `kinds.rs` is the one that the app uses. The power kinds do
/// not come here: `power.rs` runs them.
pub trait Runner: Send {
    fn start(&mut self, run: &Run) -> Started;

    /// The outcomes of the runs that were `Pending` and have ended since the last call, with
    /// their run ids.
    fn finished(&mut self) -> Vec<(String, Outcome)>;
}

/// A runner that runs nothing: every kind is not configured.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoRunner;

impl Runner for NoRunner {
    fn start(&mut self, _run: &Run) -> Started {
        Started::Done(Outcome::NotConfigured)
    }

    fn finished(&mut self) -> Vec<(String, Outcome)> {
        Vec::new()
    }
}

/// The record of a response end node that has never run.
const FRESH: RunRecord = RunRecord {
    state: RunState::NotRun,
    last_run_us: None,
};

/// The state machines of the response end nodes and the runs that have no outcome yet.
pub struct Responses {
    machines: BTreeMap<String, Response>,
    wall: Box<dyn WallClock>,
    serial: u64,
    /// The runs with no outcome yet, by the key of their node.
    open: BTreeMap<String, Run>,
    /// The monotonic time up to which the app run time is counted.
    counted_to: Option<Duration>,
}

impl Responses {
    pub fn new(wall: Box<dyn WallClock>) -> Self {
        Self {
            machines: BTreeMap::new(),
            wall,
            serial: 0,
            open: BTreeMap::new(),
            counted_to: None,
        }
    }

    /// The wall clock time now, in microseconds since the Unix epoch.
    pub fn wall_us(&self) -> u64 {
        self.wall.now_us()
    }

    /// A run id. It holds the wall clock time of the run and a serial number, so it is unique in
    /// one app run and it differs after a restart.
    pub fn next_run_id(&mut self, time_us: u64) -> String {
        self.serial += 1;
        format!("run_{time_us}_{}", self.serial)
    }

    /// The whole seconds of app run time since the last call. The first call gives 0.
    pub fn app_seconds(&mut self, now: Duration) -> u64 {
        let counted = self.counted_to.get_or_insert(now);
        let seconds = now.saturating_sub(*counted).as_secs();
        *counted += Duration::from_secs(seconds);
        seconds
    }

    /// Forget the machine of every node that is not in `keys`: its node is gone.
    pub fn retain(&mut self, keys: &BTreeSet<String>) {
        self.machines.retain(|key, _| keys.contains(key));
    }

    /// One evaluation of the response end node `key`. The machine of a node that this app run has
    /// not seen yet starts from the stored record. After the step the record of the node in
    /// `records` holds the state and the time of the last run.
    pub fn step(
        &mut self,
        key: &str,
        timing: &Timing,
        input: Option<bool>,
        now: Duration,
        records: &mut BTreeMap<String, RunRecord>,
    ) -> Action {
        let wall_us = self.wall.now_us();
        let machine =
            self.machines
                .entry(key.to_owned())
                .or_insert_with(|| match records.get(key) {
                    Some(record) => Response::restore(record, now),
                    None => Response::default(),
                });
        let action = machine.step(timing, input, now, wall_us);
        let record = machine.record();
        let current = records.get(key).copied();
        // A node that never ran leaves no record: the state file stays as it is.
        if current != Some(record) && !(current.is_none() && record == FRESH) {
            records.insert(key.to_owned(), record);
        }
        action
    }

    /// Whether the node `key` has a run with no outcome.
    pub fn is_open(&self, key: &str) -> bool {
        self.open.contains_key(key)
    }

    /// Remember a run that has no outcome yet.
    pub fn open(&mut self, run: Run) {
        self.open.insert(run.key.clone(), run);
    }

    /// The run `run_id` has its outcome. Returns the run, or `None` for an id that is not open.
    pub fn close(&mut self, run_id: &str) -> Option<Run> {
        let key = self
            .open
            .iter()
            .find(|(_, run)| run.id == run_id)
            .map(|(key, _)| key.clone())?;
        self.open.remove(&key)
    }
}
