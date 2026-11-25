//! "Try" in the editor: the test value and the test time of a Sensor node, the start and the stop
//! of a try, and the banner "Try running, nothing is published".
//!
//! The Graph service evaluates the draft (`crate::graphs::try_run`). The editor sends the draft
//! with the test values and shows the node values of the try. A try sends no Graph sensor value
//! and runs no response.

use std::collections::BTreeMap;

use nicecooling_graph::model::{Graph, Kind, ParamType};

use super::EditorUi;
use super::params::typed_value;
use super::rules::range_text;
use crate::command::Command;
use crate::graphs::channel::{GraphCommand, TryRequest};
use crate::graphs::try_run::{MAX_SECONDS, MIN_SECONDS};
use crate::i18n::Locale;

/// The time of a test value at first, and the time of a try without a test value, in seconds.
pub const DEFAULT_SECONDS: u32 = 60;

/// The reads of the node values after which a try that the service did not start counts as over.
const START_READS: u64 = 2;

/// The type of the field "For": 1 to 600 s.
const FOR_TYPE: ParamType = ParamType::Seconds {
    min: MIN_SECONDS,
    max: MAX_SECONDS,
};

/// The key of the field "Test value" of a Sensor node.
pub fn test_key(node: u32) -> String {
    format!("test:{node}")
}

/// The key of the field "For" of a Sensor node.
pub fn for_key(node: u32) -> String {
    format!("for:{node}")
}

/// The test value and the test time of one Sensor node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TestValue {
    /// `None`: the source reads its live value in a try.
    pub value: Option<f64>,
    /// 1 to 600.
    pub seconds: u32,
}

impl Default for TestValue {
    fn default() -> Self {
        Self {
            value: None,
            seconds: DEFAULT_SECONDS,
        }
    }
}

/// A try that the editor started.
#[derive(Clone, Debug, PartialEq)]
pub struct TryState {
    /// The draft at the start. A change of the draft ends the try.
    started: Graph,
    /// The time of the try in seconds.
    pub seconds: u32,
    /// A read showed the try of the service.
    seen: bool,
    /// The reads of the node values at the start.
    reads_at_start: u64,
}

/// The test values and the try of the editor.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TryUi {
    /// The test value of each Sensor node, by node id. It is not part of the draft.
    pub tests: BTreeMap<u32, TestValue>,
    pub running: Option<TryState>,
}

/// The fields "Test value" and "For" of a Sensor node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestRows {
    /// "Try".
    pub title: String,
    pub value_label: String,
    /// Empty: the source reads its live value.
    pub value: String,
    pub seconds_label: String,
    pub seconds: String,
    /// "s".
    pub unit: String,
    /// The range of "For", while its field holds a value that did not commit.
    pub error: Option<String>,
}

/// The banner of a try that runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TryBanner {
    /// "Try running, nothing is published".
    pub text: String,
    /// "24 s left".
    pub left: String,
    pub stop: String,
}

/// The fields of the test value of a node. `None` for a node that is no Sensor node.
pub fn test_rows(ui: &EditorUi, node: u32, locale: Locale) -> Option<TestRows> {
    let kind = ui.draft.graph().node(node)?.kind;
    if kind != Kind::SensorSource {
        return None;
    }
    let test = ui.tries.tests.get(&node).copied().unwrap_or_default();
    Some(TestRows {
        title: locale.tr("graph_editor.try"),
        value_label: locale.tr("graph_editor.try_run.test_value"),
        value: test
            .value
            .map(|value| super::number_text(locale, value))
            .unwrap_or_default(),
        seconds_label: locale.tr("graph_editor.try_run.for"),
        seconds: test.seconds.to_string(),
        unit: locale.tr("graph_editor.panel.seconds"),
        error: ui
            .field_error
            .as_ref()
            .filter(|(key, _)| *key == for_key(node))
            .map(|(_, range)| range.clone()),
    })
}

/// The banner of the try, while one runs.
pub fn try_banner(ui: &EditorUi, locale: Locale) -> Option<TryBanner> {
    let running = ui.tries.running.as_ref()?;
    // The service counts the time. Before its first answer the banner shows the whole time.
    let left = ui
        .live
        .trying
        .as_ref()
        .map_or(u64::from(running.seconds), |trying| trying.seconds_left);
    Some(TryBanner {
        text: locale.tr("graph_editor.try_run.banner"),
        left: locale.fmt(
            "graph_editor.try_run.left",
            &[("seconds", &left.to_string())],
        ),
        stop: locale.tr("graph_editor.try_run.stop"),
    })
}

impl EditorUi {
    /// Commit the field "Test value" of a Sensor node. An empty text removes the test value: the
    /// source then reads its live value. A text that is no number does not commit.
    pub fn commit_test_value(&mut self, node: u32, text: &str) -> bool {
        let text = text.trim();
        let value = if text.is_empty() {
            None
        } else {
            // A value that is not finite is a test value too: the source then gives no value.
            match text.replace(',', ".").parse::<f64>() {
                Ok(value) => Some(value),
                Err(_) => return false,
            }
        };
        self.field_error = None;
        self.tries.tests.entry(node).or_default().value = value;
        true
    }

    /// Commit the field "For" of a Sensor node: 1 to 600 s.
    pub fn commit_test_seconds(&mut self, node: u32, text: &str, locale: Locale) -> bool {
        let seconds = typed_value(FOR_TYPE, text).and_then(|value| value.as_u64());
        let Some(seconds) = seconds else {
            self.field_error = Some((for_key(node), range_text(FOR_TYPE, locale)));
            return false;
        };
        self.field_error = None;
        self.tries.tests.entry(node).or_default().seconds = seconds as u32;
        true
    }

    /// The test values of the Sensor nodes that the draft has.
    fn test_values(&self) -> Vec<(u32, TestValue)> {
        let graph = self.draft.graph();
        self.tries
            .tests
            .iter()
            .filter(|(id, test)| {
                test.value.is_some()
                    && graph
                        .node(**id)
                        .is_some_and(|node| node.kind == Kind::SensorSource)
            })
            .map(|(id, test)| (*id, *test))
            .collect()
    }

    /// Whether a Sensor node has a test value: its title row has the tag "Test".
    pub fn has_test_value(&self, node: u32) -> bool {
        self.tries
            .tests
            .get(&node)
            .is_some_and(|test| test.value.is_some())
    }

    /// "Try": the command that makes the Graph service evaluate the draft. The try lasts for the
    /// longest time of a test value, or 60 s when no source has a test value. `None` while a
    /// try runs.
    pub fn start_try(&mut self) -> Option<Command> {
        if self.tries.running.is_some() {
            return None;
        }
        let tests = self.test_values();
        let seconds = tests
            .iter()
            .map(|(_, test)| test.seconds)
            .max()
            .unwrap_or(DEFAULT_SECONDS);
        let graph = self.draft.candidate();
        self.tries.running = Some(TryState {
            started: self.draft.graph().clone(),
            seconds,
            seen: false,
            reads_at_start: self.live.reads(),
        });
        // The next frame reads the values of the try.
        self.live.retry();
        Some(Command::Graph(GraphCommand::Try(Box::new(TryRequest {
            graph,
            tests: tests
                .into_iter()
                .filter_map(|(id, test)| test.value.map(|value| (id, value)))
                .collect(),
            seconds,
        }))))
    }

    /// "Stop": the command that ends the try. `None` when no try runs.
    pub fn stop_try(&mut self) -> Option<Command> {
        self.tries.running.take()?;
        self.live.trying = None;
        self.live.retry();
        Some(Command::Graph(GraphCommand::StopTry))
    }

    /// Follow the try after a read of the node values. The try is over when the service shows
    /// none after it showed one, and when the service did not start it. A change of the draft
    /// ends the try: the function then returns the command that stops it.
    pub fn follow_try(&mut self) -> Option<Command> {
        let reads = self.live.reads();
        let shown = self.live.trying.is_some();
        let running = self.tries.running.as_mut()?;
        if running.started != *self.draft.graph() {
            return self.stop_try();
        }
        if shown {
            running.seen = true;
        } else if running.seen || reads >= running.reads_at_start + START_READS {
            self.tries.running = None;
        }
        None
    }

    /// Whether the nodes show the values of a try now.
    pub fn shows_try(&self) -> bool {
        self.tries.running.is_some() && self.live.trying.is_some()
    }
}
