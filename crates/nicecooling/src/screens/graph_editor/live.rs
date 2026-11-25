//! The live values of the editor: the value of each node of the applied Graph.
//!
//! The Graph service writes the node values of every pass to its shared view. While the editor
//! is open it reads that view once per second. A node that exists only in the draft has no
//! value before "Apply", and a node with no value shows "--".

use std::collections::BTreeMap;

use nicecooling_graph::eval::EndValue;
use nicecooling_graph::response::RunRecord;

use crate::graphs::view::{GraphView, SharedView, TryView, ValuesState};

/// The time between two reads of the node values, in microseconds.
pub const READ_INTERVAL_US: u64 = 1_000_000;

/// Where the editor reads the node values. The shared view of the Graph service is one, and a
/// test counts the reads with another.
pub trait ValueSource {
    /// What the Graph service shows now. `None` when the read fails.
    fn read(&self) -> Option<GraphView>;
}

impl ValueSource for SharedView {
    fn read(&self) -> Option<GraphView> {
        self.lock().ok().map(|view| view.clone())
    }
}

/// The values that the editor read last.
#[derive(Clone, Debug, PartialEq)]
pub struct LiveValues {
    /// The time of the last read, in microseconds since the Unix epoch.
    read_at_us: Option<u64>,
    reads: u64,
    /// The editor is closed: it reads nothing.
    closed: bool,
    /// The last read failed: the nodes show "--" in `fg-dim`.
    pub failed: bool,
    /// A read gave the values once. Before it the nodes show "--".
    pub loaded: bool,
    /// The value of each node of the applied Graph in the last pass. `None` is no value.
    pub values: BTreeMap<u32, Option<f64>>,
    /// The result of the end node in the last pass.
    pub end: EndValue,
    /// The stored run state of the response end node.
    pub run: Option<RunRecord>,
    /// The applied Graph is enabled. A disabled Graph is not evaluated.
    pub enabled: bool,
    /// The try of this Graph that runs now.
    pub trying: Option<TryView>,
    /// The state of the call that sends the Graph sensor values to the daemon.
    pub access: ValuesState,
}

impl Default for LiveValues {
    fn default() -> Self {
        Self {
            read_at_us: None,
            reads: 0,
            closed: false,
            failed: false,
            loaded: false,
            values: BTreeMap::new(),
            end: EndValue::None,
            run: None,
            enabled: true,
            trying: None,
            access: ValuesState::Ok,
        }
    }
}

impl LiveValues {
    /// Read the node values of the Graph `graph_id` when a second has passed since the last
    /// read. The first call reads at once. Returns whether it read.
    pub fn poll(&mut self, graph_id: &str, now_us: u64, source: &dyn ValueSource) -> bool {
        if self.closed {
            return false;
        }
        // A wall clock that went back reads at once.
        let due = self
            .read_at_us
            .is_none_or(|last| now_us < last || now_us - last >= READ_INTERVAL_US);
        if !due {
            return false;
        }
        self.read_at_us = Some(now_us);
        self.reads += 1;
        match source.read() {
            Some(view) => {
                self.failed = false;
                self.take(graph_id, &view);
            }
            None => self.failed = true,
        }
        true
    }

    /// Keep what a view shows of the Graph `graph_id`.
    pub fn take(&mut self, graph_id: &str, view: &GraphView) {
        self.loaded = true;
        self.access = view.values.clone();
        self.trying = view
            .trying
            .clone()
            .filter(|trying| trying.graph_id == graph_id);
        match view.row(graph_id) {
            Some(row) => {
                self.values = row.values.clone();
                self.end = row.end.clone();
                self.run = row.run;
                self.enabled = row.enabled;
            }
            // The app does not store the Graph yet.
            None => {
                self.values.clear();
                self.end = EndValue::None;
                self.run = None;
                self.enabled = true;
            }
        }
    }

    /// "Retry" after a failed read: the next poll reads at once.
    pub fn retry(&mut self) {
        self.read_at_us = None;
    }

    /// The editor closes: it reads no value after this.
    pub fn close(&mut self) {
        self.closed = true;
    }

    /// The reads that the editor made.
    pub fn reads(&self) -> u64 {
        self.reads
    }

    /// The value of a node of the applied Graph. `None` for no value, for a failed read and
    /// before the first read.
    pub fn value(&self, node: u32) -> Option<f64> {
        if self.failed {
            return None;
        }
        self.values.get(&node).copied().flatten()
    }
}
