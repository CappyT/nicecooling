//! What the Graph service shows to the window and the tray: one shared state behind a mutex.
//! The service writes it once per second, and the screens read it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use nicecooling_graph::eval::EndValue;
use nicecooling_graph::pause::PowerKind;

use super::responses::LogRow;

/// Why a Graph does not run, or that it runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RowState {
    Running,
    Disabled,
    /// The Graph breaks a rule of the validator, or does not parse. The text says which.
    Invalid(String),
}

/// One Graph of `graphs.json`.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphRow {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub state: RowState,
    /// The value of each node in the last pass. `None` is no value. Empty for a Graph that did not
    /// run.
    pub values: BTreeMap<u32, Option<f64>>,
    /// The result of the end node in the last pass.
    pub end: EndValue,
}

/// The countdown of a power response that runs now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Countdown {
    pub graph_id: String,
    pub graph_name: String,
    pub kind: PowerKind,
    pub seconds_left: u64,
    /// The length of the countdown in seconds.
    pub total_s: u64,
}

/// The node values of a try.
#[derive(Clone, Debug, PartialEq)]
pub struct TryView {
    pub graph_id: String,
    pub seconds_left: u64,
    pub values: BTreeMap<u32, Option<f64>>,
    pub end: EndValue,
}

/// A Graph sensor of the daemon that none of the Graphs of this app holds. It can belong to the
/// app of another user. The Graphs screen lists it with a remove command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignSensor {
    pub id: String,
    pub name: String,
    pub unit: String,
}

/// How the call that sends the Graph sensor values goes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ValuesState {
    /// Nothing was sent yet, or the last call worked.
    #[default]
    Ok,
    /// polkit refused the call. The app sends nothing for 60 s.
    NoAccess,
    /// The session of the app is not the active one: the app sends nothing.
    InactiveSession,
    /// The call failed for another reason, for example because the daemon is away.
    Failed(String),
}

/// The Response pause as the window shows it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PauseView {
    pub paused: bool,
    /// The poweroff and reboot runs in a row.
    pub count: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphView {
    pub rows: Vec<GraphRow>,
    /// Why a Graph file cannot be used (the app writes nothing over such a file), or why the
    /// Graph service ended.
    pub error: Option<String>,
    /// Why the last Apply or Enable was refused. A command that works clears it.
    pub refusal: Option<String>,
    pub countdown: Option<Countdown>,
    /// The try that runs now.
    pub trying: Option<TryView>,
    /// The state of the call that sends the Graph sensor values.
    pub values: ValuesState,
    pub foreign_sensors: Vec<ForeignSensor>,
    /// The Response pause: no Graph response runs by itself while it is on.
    pub pause: PauseView,
    /// The response log, in the order of the runs: the newest 500 rows of all Graphs.
    pub log: Vec<LogRow>,
}

impl GraphView {
    pub fn row(&self, id: &str) -> Option<&GraphRow> {
        self.rows.iter().find(|row| row.id == id)
    }

    /// The rows of the response log that belong to one Graph, in the order of the runs.
    pub fn log_rows_of(&self, graph_id: &str) -> Vec<&LogRow> {
        self.log
            .iter()
            .filter(|row| row.graph_id == graph_id)
            .collect()
    }
}

/// The view that the service and the screens share.
pub type SharedView = Arc<Mutex<GraphView>>;

pub fn shared_view() -> SharedView {
    Arc::new(Mutex::new(GraphView::default()))
}
