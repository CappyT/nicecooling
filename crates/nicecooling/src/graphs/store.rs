//! The files of the Graph service: `graphs.json` and `graph-state.json` (`docs/storage.md`).
//!
//! The store parses `graphs.json` one Graph at a time and runs the validator on each. A Graph that
//! fails stays in the file with its stored text and does not run. A file that does not parse, or
//! that has a higher schema version, stays unchanged, and the store writes nothing over it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use nicecooling_graph::model::{
    Graph, GraphFile, InvalidGraph, Kind, ParseProblem, SCHEMA_VERSION, StoredGraph,
};
use nicecooling_graph::pause::Pause;
use nicecooling_graph::response::RunRecord;
use nicecooling_graph::validate::{Refusal, RefusalCode, validate};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

use super::responses::{LOG_LIMIT, LogRow};
use super::transfer::next_graph_id;
use crate::app_settings::{config_home, data_home, write_atomic};

/// The state file changes at most this often.
pub const STATE_WRITE_INTERVAL: Duration = Duration::from_secs(30);

/// The paths of the files.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphPaths {
    /// `$XDG_CONFIG_HOME/nicecooling/graphs.json`
    pub graphs: PathBuf,
    /// `$XDG_DATA_HOME/nicecooling/graph-state.json`
    pub state: PathBuf,
    /// `$XDG_CONFIG_HOME/nicecooling/secrets.toml`
    pub secrets: PathBuf,
    /// `$XDG_DATA_HOME/nicecooling/response-log.json`
    pub log: PathBuf,
}

impl GraphPaths {
    pub fn in_dirs(config: &Path, data: &Path) -> Self {
        Self {
            graphs: config.join("graphs.json"),
            state: data.join("graph-state.json"),
            secrets: config.join("secrets.toml"),
            log: data.join("response-log.json"),
        }
    }

    /// The files of the user.
    pub fn user() -> Self {
        Self::in_dirs(
            &config_home().join("nicecooling"),
            &data_home().join("nicecooling"),
        )
    }
}

/// The content of `graph-state.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StateFile {
    pub schema_version: u32,
    /// The persistent storage nodes, keyed `<graph id>/<node id>`.
    pub values: BTreeMap<String, f64>,
    /// The response end nodes, with the same key.
    pub responses: BTreeMap<String, RunRecord>,
    pub pause: Pause,
}

impl Default for StateFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            values: BTreeMap::new(),
            responses: BTreeMap::new(),
            pause: Pause::default(),
        }
    }
}

impl StateFile {
    /// Whether a write is worth it: a change of the app run time alone is not.
    fn differs_from(&self, other: &Self) -> bool {
        let mut other = other.clone();
        other.pause.quiet_s = self.pause.quiet_s;
        *self != other
    }
}

/// The content of `response-log.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LogFile {
    pub schema_version: u32,
    /// The rows in the order of their runs.
    pub rows: Vec<LogRow>,
}

impl Default for LogFile {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            rows: Vec::new(),
        }
    }
}

/// The text of the refusals of one Graph, for the list of invalid Graphs.
pub fn describe(refusals: &[Refusal]) -> String {
    refusals
        .iter()
        .map(|r| {
            let mut text = r.code.as_str().to_owned();
            if let Some(parameter) = &r.parameter {
                text.push(' ');
                text.push_str(parameter);
            }
            if let Some(node) = r.node {
                text.push_str(&format!(" (node {node})"));
            }
            text
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Why an invalid Graph does not run.
pub fn describe_invalid(invalid: &InvalidGraph) -> String {
    invalid.problem.to_string()
}

fn io_error(text: impl Into<String>) -> std::io::Error {
    std::io::Error::other(text.into())
}

/// `graphs.json` and `graph-state.json` in memory.
#[derive(Debug)]
pub struct GraphStore {
    paths: GraphPaths,
    entries: Vec<StoredGraph>,
    /// Why `graphs.json` cannot be used.
    file_error: Option<String>,
    state: StateFile,
    state_error: Option<String>,
    /// The state that the file holds, `None` while there is no file.
    written: Option<StateFile>,
    last_write: Option<Duration>,
    /// The newest rows of `response-log.json`, at most `LOG_LIMIT`.
    log: Vec<LogRow>,
    /// Why `response-log.json` cannot be used. The app writes nothing over such a file.
    log_error: Option<String>,
}

impl GraphStore {
    pub fn open(paths: GraphPaths) -> Self {
        let (entries, file_error) = load_graphs(&paths.graphs);
        let (state, state_error, written) = load_state(&paths.state);
        let (log, log_error) = load_log(&paths.log);
        let mut store = Self {
            paths,
            entries,
            file_error,
            state,
            state_error,
            written,
            last_write: None,
            log,
            log_error,
        };
        store.prune_state();
        store
    }

    pub fn paths(&self) -> &GraphPaths {
        &self.paths
    }

    /// Every Graph of the file, the invalid ones included.
    pub fn entries(&self) -> &[StoredGraph] {
        &self.entries
    }

    /// The Graphs that passed the validator.
    pub fn graphs(&self) -> Vec<Graph> {
        self.entries
            .iter()
            .filter_map(|entry| match entry {
                StoredGraph::Valid(graph) => Some(graph.clone()),
                StoredGraph::Invalid(_) => None,
            })
            .collect()
    }

    /// Every entry of the file as a Graph, for the validator and for new ids. An entry that does
    /// not run has its id and no nodes: it counts for the limit of 64 Graphs and its id is taken.
    pub fn all_graphs(&self) -> Vec<Graph> {
        self.entries
            .iter()
            .enumerate()
            .map(|(index, entry)| match entry {
                StoredGraph::Valid(graph) => graph.clone(),
                StoredGraph::Invalid(invalid) => {
                    let id = invalid
                        .id
                        .clone()
                        .unwrap_or_else(|| format!("<unreadable {index}>"));
                    let mut graph = Graph::new(&id, invalid.name.as_deref().unwrap_or_default());
                    graph.enabled = false;
                    graph
                }
            })
            .collect()
    }

    /// The refusals of the validator for `graph` among the stored entries. An id that an entry
    /// without a valid Graph holds is a refusal of `id`: that entry is not a version of `graph`.
    pub fn validate(&self, graph: &Graph) -> Vec<Refusal> {
        let mut refusals = validate(graph, &self.all_graphs());
        if self.holds_unreadable(&graph.id) {
            refusals.push(Refusal {
                code: RefusalCode::BadParameter,
                parameter: Some("id".to_owned()),
                node: None,
            });
            refusals.sort_by_key(|r| r.code);
            refusals.dedup();
        }
        refusals
    }

    /// Whether an entry that this version cannot run has the id `id`.
    fn holds_unreadable(&self, id: &str) -> bool {
        self.entries
            .iter()
            .any(|e| matches!(e, StoredGraph::Invalid(_)) && e.id() == Some(id))
    }

    pub fn graph(&self, id: &str) -> Option<&Graph> {
        self.entries.iter().find_map(|entry| match entry {
            StoredGraph::Valid(graph) if graph.id == id => Some(graph),
            _ => None,
        })
    }

    /// The error of `graphs.json`, of `graph-state.json`, or of `response-log.json`.
    pub fn error(&self) -> Option<&str> {
        self.file_error
            .as_deref()
            .or(self.state_error.as_deref())
            .or(self.log_error.as_deref())
    }

    pub fn file_error(&self) -> Option<&str> {
        self.file_error.as_deref()
    }

    /// Read `graphs.json` again: before each edit, and at "Retry" after a file that the app could
    /// not use. A file that loads replaces the Graphs of the store and clears the error. A file
    /// that does not load sets the error, and the store keeps the Graphs that it loaded last, so
    /// they keep running. The state and the response log stay as the store holds them.
    pub fn reload(&mut self) {
        if self.read().is_ok() {
            self.prune_state();
        }
    }

    fn read(&mut self) -> std::io::Result<()> {
        let (entries, error) = load_graphs(&self.paths.graphs);
        match error {
            Some(error) => {
                self.file_error = Some(error.clone());
                Err(io_error(error))
            }
            None => {
                self.entries = entries;
                self.file_error = None;
                Ok(())
            }
        }
    }

    /// Add a Graph, or replace the Graph with the same id, and write the file. The caller runs the
    /// validator first. An entry that is not a valid Graph is never replaced: its id is taken.
    pub fn put(&mut self, graph: Graph) -> std::io::Result<()> {
        self.change(|entries| {
            match entries
                .iter_mut()
                .find(|e| e.id() == Some(graph.id.as_str()))
            {
                Some(StoredGraph::Valid(slot)) => *slot = graph,
                Some(StoredGraph::Invalid(_)) => {
                    return Err(io_error(format!(
                        "the id {} belongs to a Graph that this version does not run",
                        graph.id
                    )));
                }
                None => entries.push(StoredGraph::Valid(graph)),
            }
            Ok(())
        })
    }

    /// Whether an entry, valid or not, has the id `id`.
    pub fn holds(&self, id: &str) -> bool {
        self.entries.iter().any(|e| e.id() == Some(id))
    }

    /// Add a new Graph, write the file and return the id of the Graph. The store gives the id
    /// after it read the file again. The id of `graph` is a proposal: the caller took it from an
    /// older view, so it can be the id of a Graph that another writer added. The Graph keeps it
    /// when no entry holds it, and gets a free id when one does. A new Graph never replaces an
    /// entry.
    pub fn add(&mut self, mut graph: Graph) -> std::io::Result<String> {
        self.change(|entries| {
            let taken = |id: &str| entries.iter().any(|e| e.id() == Some(id));
            if taken(&graph.id) {
                graph.id = next_graph_id(entries.iter().filter_map(StoredGraph::id));
                if taken(&graph.id) {
                    return Err(io_error("no Graph id is free"));
                }
            }
            let id = graph.id.clone();
            entries.push(StoredGraph::Valid(graph));
            Ok(id)
        })
    }

    /// Change the Graph `id` as the file holds it after the store read it again, and write the
    /// file. `edit` changes the fields that its command owns. Every other field keeps the value
    /// of the file, so a hand edit of the Graph is not lost. A Graph that the file no longer
    /// holds as a valid Graph is an error: it does not come back.
    pub fn update(&mut self, id: &str, edit: impl FnOnce(&mut Graph)) -> std::io::Result<()> {
        self.change(
            |entries| match entries.iter_mut().find(|e| e.id() == Some(id)) {
                Some(StoredGraph::Valid(graph)) => {
                    edit(graph);
                    Ok(())
                }
                _ => Err(io_error(format!(
                    "the Graph file holds no valid Graph with the id {id}"
                ))),
            },
        )
    }

    pub fn remove(&mut self, id: &str) -> std::io::Result<()> {
        self.change(|entries| {
            entries.retain(|e| e.id() != Some(id));
            Ok(())
        })
    }

    fn change<T>(
        &mut self,
        edit: impl FnOnce(&mut Vec<StoredGraph>) -> std::io::Result<T>,
    ) -> std::io::Result<T> {
        // Another writer can have changed the file since the last read (a second instance of the
        // app, a hand edit): edit what the file holds now, so no change of it is lost. A file
        // that does not load stays as it is, and a file that the user repaired works again.
        self.read()?;
        let before = self.entries.clone();
        let done = match edit(&mut self.entries) {
            Ok(done) => done,
            Err(error) => {
                self.entries = before;
                return Err(error);
            }
        };
        let text = GraphFile {
            graphs: self.entries.clone(),
        }
        .to_json();
        if let Err(error) = write_atomic(&self.paths.graphs, text.as_bytes(), 0o666) {
            self.entries = before;
            return Err(error);
        }
        self.prune_state();
        Ok(done)
    }

    /// The rows of the response log, in the order of their runs.
    pub fn log(&self) -> &[LogRow] {
        &self.log
    }

    /// Add a row to the response log and write the file. The log keeps the newest `LOG_LIMIT`
    /// rows. A log file that does not load stays as it is, and the row is lost.
    pub fn push_log(&mut self, row: LogRow) -> std::io::Result<()> {
        if let Some(error) = &self.log_error {
            return Err(io_error(error.clone()));
        }
        self.log.push(row);
        if self.log.len() > LOG_LIMIT {
            let excess = self.log.len() - LOG_LIMIT;
            self.log.drain(..excess);
        }
        let file = LogFile {
            schema_version: SCHEMA_VERSION,
            rows: self.log.clone(),
        };
        let text = serde_json::to_string_pretty(&file).map_err(std::io::Error::other)?;
        write_atomic(&self.paths.log, text.as_bytes(), 0o666)
    }

    pub fn state(&self) -> &StateFile {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut StateFile {
        &mut self.state
    }

    /// Delete the stored values and run states of nodes and Graphs that are gone. The state of an
    /// invalid Graph stays: the Graph can become valid again. A Graph file that does not load
    /// prunes nothing: the Graphs are unknown, not gone.
    fn prune_state(&mut self) {
        if self.file_error.is_some() {
            return;
        }
        let keep = |key: &str, kind: Kind| -> bool {
            let Some((graph_id, node)) = key.rsplit_once('/') else {
                return false;
            };
            let Ok(node) = node.parse::<u32>() else {
                return false;
            };
            self.entries.iter().any(|entry| match entry {
                StoredGraph::Valid(graph) => {
                    graph.id == graph_id && graph.node(node).is_some_and(|n| n.kind == kind)
                }
                StoredGraph::Invalid(invalid) => invalid.id.as_deref() == Some(graph_id),
            })
        };
        let values: BTreeMap<_, _> = self
            .state
            .values
            .iter()
            .filter(|(key, _)| keep(key, Kind::PersistentStorage))
            .map(|(key, value)| (key.clone(), *value))
            .collect();
        let responses: BTreeMap<_, _> = self
            .state
            .responses
            .iter()
            .filter(|(key, _)| keep(key, Kind::ResponseEnd))
            .map(|(key, record)| (key.clone(), *record))
            .collect();
        self.state.values = values;
        self.state.responses = responses;
    }

    /// Write `graph-state.json`. Without `force`, the file is written when the state changed and
    /// 30 s passed since the last write. With `force` (the stop of the app, a power call), it is
    /// written when the state differs from the file at all. Returns whether it wrote. The file
    /// stays as it is while `graphs.json` does not load: its values and run states belong to
    /// Graphs that the store cannot see.
    pub fn write_state(&mut self, now: Duration, force: bool) -> std::io::Result<bool> {
        if let Some(error) = &self.state_error {
            return Err(io_error(error.clone()));
        }
        if self.file_error.is_some() {
            return Ok(false);
        }
        self.prune_state();
        let default = StateFile::default();
        let baseline = self.written.as_ref().unwrap_or(&default);
        let due = if force {
            self.state != *baseline
        } else {
            self.state.differs_from(baseline)
                && self
                    .last_write
                    .is_none_or(|last| now.saturating_sub(last) >= STATE_WRITE_INTERVAL)
        };
        if !due {
            return Ok(false);
        }
        let text = serde_json::to_string_pretty(&self.state).map_err(std::io::Error::other)?;
        write_atomic(&self.paths.state, text.as_bytes(), 0o666)?;
        self.written = Some(self.state.clone());
        self.last_write = Some(now);
        Ok(true)
    }
}

/// The Graphs of `graphs.json` as the file holds them.
#[derive(Deserialize)]
struct RawList {
    graphs: Vec<Box<RawValue>>,
}

fn load_graphs(path: &Path) -> (Vec<StoredGraph>, Option<String>) {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return (Vec::new(), None),
        Err(error) => return (Vec::new(), Some(error.to_string())),
    };
    let file = match GraphFile::parse(&text) {
        Ok(file) => file,
        Err(error) => return (Vec::new(), Some(error.to_string())),
    };
    let valid: Vec<Graph> = file
        .graphs
        .iter()
        .filter_map(|entry| match entry {
            StoredGraph::Valid(graph) => Some(graph.clone()),
            StoredGraph::Invalid(_) => None,
        })
        .collect();
    // The text of each Graph in the file, in the order of `file.graphs`.
    let texts: Vec<Box<RawValue>> = serde_json::from_str::<RawList>(&text)
        .map(|list| list.graphs)
        .unwrap_or_default();
    let entries = file
        .graphs
        .into_iter()
        .enumerate()
        .map(|(index, entry)| match entry {
            StoredGraph::Valid(graph) => {
                let refusals = validate(&graph, &valid);
                match texts.get(index) {
                    Some(text) if !refusals.is_empty() => StoredGraph::Invalid(InvalidGraph {
                        id: Some(graph.id.clone()),
                        name: Some(graph.name.clone()),
                        problem: ParseProblem::Malformed(describe(&refusals)),
                        text: text.clone(),
                    }),
                    _ => StoredGraph::Valid(graph),
                }
            }
            invalid @ StoredGraph::Invalid(_) => invalid,
        })
        .collect();
    (entries, None)
}

fn load_log(path: &Path) -> (Vec<LogRow>, Option<String>) {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return (Vec::new(), None),
        Err(error) => return (Vec::new(), Some(error.to_string())),
    };
    match serde_json::from_str::<LogFile>(&text) {
        Ok(file) if file.schema_version > SCHEMA_VERSION => (
            Vec::new(),
            Some(format!(
                "the response log has the schema version {}, and this version knows {SCHEMA_VERSION}",
                file.schema_version
            )),
        ),
        Ok(mut file) => {
            if file.rows.len() > LOG_LIMIT {
                let excess = file.rows.len() - LOG_LIMIT;
                file.rows.drain(..excess);
            }
            (file.rows, None)
        }
        Err(error) => (
            Vec::new(),
            Some(format!("the response log is not valid: {error}")),
        ),
    }
}

fn load_state(path: &Path) -> (StateFile, Option<String>, Option<StateFile>) {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return (StateFile::default(), None, None);
        }
        Err(error) => return (StateFile::default(), Some(error.to_string()), None),
    };
    match serde_json::from_str::<StateFile>(&text) {
        Ok(file) if file.schema_version > SCHEMA_VERSION => (
            StateFile::default(),
            Some(format!(
                "the state file has the schema version {}, and this version knows {SCHEMA_VERSION}",
                file.schema_version
            )),
            None,
        ),
        Ok(file) => (file.clone(), None, Some(file)),
        Err(error) => (
            StateFile::default(),
            Some(format!("the state file is not valid: {error}")),
            None,
        ),
    }
}
