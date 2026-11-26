//! What the Graph service shows to the window and the tray: one shared state behind a mutex.
//! The service writes it once per second, and the screens read it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use nicecooling_graph::eval::EndValue;
use nicecooling_graph::model::{Graph, StoredGraph};
use nicecooling_graph::pause::PowerKind;
use nicecooling_graph::response::RunRecord;
use nicecooling_graph::validate::Refusal;

use super::responses::LogRow;
use super::values::SensorError;

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
    /// The entry of `graphs.json`: the Graph, or the text that this version cannot run.
    pub stored: StoredGraph,
    /// The stored run state of the response end node: whether the response ran for the input
    /// that is true now, and the time of the last run. `None` for a node that never ran.
    pub run: Option<RunRecord>,
}

impl GraphRow {
    /// The Graph of the row. `None` for an entry that this version cannot run.
    pub fn graph(&self) -> Option<&Graph> {
        match &self.stored {
            StoredGraph::Valid(graph) => Some(graph),
            StoredGraph::Invalid(_) => None,
        }
    }
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

/// Why the service did not store the Graph of an "Apply".
#[derive(Clone, Debug, PartialEq)]
pub enum ApplyError {
    /// The Graph breaks rules of the validator.
    Rules(Vec<Refusal>),
    /// The daemon refused the call for the Graph sensor.
    Sensor(SensorError),
    /// Another reason: a Graph file that cannot be written, a service that has too much to do.
    Other(String),
}

/// The answer of the service to an "Apply".
#[derive(Clone, Debug, PartialEq)]
pub struct Applied {
    /// The id of the Graph in the command.
    pub graph: String,
    /// The id under which the store holds the Graph. The store gives a new Graph its id, so it
    /// can differ from the id in the command.
    pub result: Result<String, ApplyError>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphView {
    pub rows: Vec<GraphRow>,
    /// Why a Graph file cannot be used (the app writes nothing over such a file), or why the
    /// Graph service ended.
    pub error: Option<String>,
    /// Why `graphs.json` cannot be used. The service stores no Graph while it is set.
    pub file_error: Option<String>,
    /// Why the last Apply or Enable was refused. A command that works clears it.
    pub refusal: Option<String>,
    /// The answer to the last "Apply". The editor clears it when it sends an Apply, and waits
    /// for it: the service can wait for the daemon before it answers.
    pub applied: Option<Applied>,
    /// The answer to the last "Import" that the service stored: the id of the Graph in the
    /// command, then the id that the store gave it.
    pub imported: Option<(String, String)>,
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

    /// Every row as a Graph, for the validator and for new ids. A row that this version cannot
    /// run has its id and no nodes: it counts for the limit of 64 Graphs and its id is taken.
    pub fn graphs(&self) -> Vec<Graph> {
        self.rows
            .iter()
            .map(|row| match &row.stored {
                StoredGraph::Valid(graph) => graph.clone(),
                StoredGraph::Invalid(_) => {
                    let mut graph = Graph::new(&row.id, &row.name);
                    graph.enabled = false;
                    graph
                }
            })
            .collect()
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

/// Lock the shared view. A thread that panicked with the lock leaves it poisoned. The data is
/// still the view: the service goes on writing it, and the window and the tray go on reading it,
/// so a countdown still shows.
pub fn lock_view(view: &SharedView) -> MutexGuard<'_, GraphView> {
    view.lock().unwrap_or_else(PoisonError::into_inner)
}
