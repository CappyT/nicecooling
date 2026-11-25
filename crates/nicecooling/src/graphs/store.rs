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
use nicecooling_graph::validate::{Refusal, validate};
use serde::{Deserialize, Serialize};
use serde_json::value::to_raw_value;

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
}

impl GraphPaths {
    pub fn in_dirs(config: &Path, data: &Path) -> Self {
        Self {
            graphs: config.join("graphs.json"),
            state: data.join("graph-state.json"),
            secrets: config.join("secrets.toml"),
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
}

impl GraphStore {
    pub fn open(paths: GraphPaths) -> Self {
        let (entries, file_error) = load_graphs(&paths.graphs);
        let (state, state_error, written) = load_state(&paths.state);
        let mut store = Self {
            paths,
            entries,
            file_error,
            state,
            state_error,
            written,
            last_write: None,
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

    pub fn graph(&self, id: &str) -> Option<&Graph> {
        self.entries.iter().find_map(|entry| match entry {
            StoredGraph::Valid(graph) if graph.id == id => Some(graph),
            _ => None,
        })
    }

    /// The error of `graphs.json`, or of `graph-state.json`.
    pub fn error(&self) -> Option<&str> {
        self.file_error.as_deref().or(self.state_error.as_deref())
    }

    pub fn file_error(&self) -> Option<&str> {
        self.file_error.as_deref()
    }

    /// Add a Graph, or replace the Graph with the same id, and write the file. The caller runs the
    /// validator first.
    pub fn put(&mut self, graph: Graph) -> std::io::Result<()> {
        self.change(|entries| {
            let entry = StoredGraph::Valid(graph);
            match entries.iter_mut().find(|e| e.id() == entry.id()) {
                Some(slot) => *slot = entry,
                None => entries.push(entry),
            }
        })
    }

    pub fn remove(&mut self, id: &str) -> std::io::Result<()> {
        self.change(|entries| entries.retain(|e| e.id() != Some(id)))
    }

    fn change(&mut self, edit: impl FnOnce(&mut Vec<StoredGraph>)) -> std::io::Result<()> {
        if let Some(error) = &self.file_error {
            return Err(io_error(error.clone()));
        }
        let before = self.entries.clone();
        edit(&mut self.entries);
        let text = GraphFile {
            graphs: self.entries.clone(),
        }
        .to_json();
        if let Err(error) = write_atomic(&self.paths.graphs, text.as_bytes(), 0o666) {
            self.entries = before;
            return Err(error);
        }
        self.prune_state();
        Ok(())
    }

    pub fn state(&self) -> &StateFile {
        &self.state
    }

    pub fn state_mut(&mut self) -> &mut StateFile {
        &mut self.state
    }

    /// Delete the stored values and run states of nodes and Graphs that are gone. The state of an
    /// invalid Graph stays: the Graph can become valid again.
    fn prune_state(&mut self) {
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
    /// written when the state differs from the file at all. Returns whether it wrote.
    pub fn write_state(&mut self, now: Duration, force: bool) -> std::io::Result<bool> {
        if let Some(error) = &self.state_error {
            return Err(io_error(error.clone()));
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
    let entries = file
        .graphs
        .into_iter()
        .map(|entry| match entry {
            StoredGraph::Valid(graph) => {
                let refusals = validate(&graph, &valid);
                if refusals.is_empty() {
                    return StoredGraph::Valid(graph);
                }
                match to_raw_value(&graph) {
                    Ok(text) => StoredGraph::Invalid(InvalidGraph {
                        id: Some(graph.id.clone()),
                        name: Some(graph.name.clone()),
                        problem: ParseProblem::Malformed(describe(&refusals)),
                        text,
                    }),
                    Err(_) => StoredGraph::Valid(graph),
                }
            }
            invalid @ StoredGraph::Invalid(_) => invalid,
        })
        .collect();
    (entries, None)
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
