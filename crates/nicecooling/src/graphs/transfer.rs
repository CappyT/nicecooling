//! Export and import of one Graph as one JSON file (`docs/storage.md`, Export file). The daemon
//! has no part in it.

use std::fmt;
use std::io::Read as _;
use std::path::Path;

use nicecooling_graph::model::{Connection, Endpoint, Export, ExportError, Graph, Node};
use nicecooling_graph::validate::{Refusal, validate};

use super::store::describe;
use crate::app_settings::write_atomic;

/// The largest file that the app imports, in bytes.
pub const MAX_IMPORT_BYTES: u64 = 1024 * 1024;

/// The text of an export file. It holds the name, the nodes and the connections. It holds no
/// sensor id, no stored value, no run state and no password.
pub fn export_text(graph: &Graph) -> String {
    Export::from_graph(graph).to_json()
}

pub fn export_file(path: &Path, graph: &Graph) -> std::io::Result<()> {
    write_atomic(path, export_text(graph).as_bytes(), 0o666)
}

/// Why a file is not imported. The window shows a message for each.
#[derive(Clone, Debug, PartialEq)]
pub enum ImportError {
    /// The file is over 1 MiB.
    TooLarge,
    /// The file cannot be read.
    Read(String),
    /// The file is not a Graph of this app, or it has a higher format version.
    Format(ExportError),
    /// The Graph breaks a rule of the validator.
    Invalid(Vec<Refusal>),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge => write!(f, "the file is over {MAX_IMPORT_BYTES} bytes"),
            Self::Read(why) => write!(f, "the file cannot be read: {why}"),
            Self::Format(error) => error.fmt(f),
            Self::Invalid(refusals) => {
                write!(f, "the Graph is not valid: {}", describe(refusals))
            }
        }
    }
}

impl std::error::Error for ImportError {}

/// The id of a Graph that does not exist yet: `graph_<n>` above every `graph_<n>` of `existing`.
fn new_graph_id(existing: &[Graph]) -> String {
    let highest = existing
        .iter()
        .filter_map(|g| nicecooling_graph::model::parse_sensor_id(&g.id))
        .max()
        .unwrap_or(0);
    nicecooling_graph::model::format_sensor_id(highest.saturating_add(1))
}

/// Make a Graph of an export. The Graph and its nodes get new ids. The Graph is disabled and has
/// no `sensor_id`, so a response from another machine does not run before the user enabled it.
/// A source on a sensor that this machine does not have stays as it is: it gives no value.
/// `existing` holds every stored entry as a Graph (`GraphStore::all_graphs`), for the id and the
/// validator: an entry that does not run keeps its id and counts for the limit.
pub fn import_text(text: &str, existing: &[Graph]) -> Result<Graph, ImportError> {
    let export = Export::parse(text).map_err(ImportError::Format)?;
    // The nodes count from 1, in the order of the file.
    let new_id = |old: u32| {
        export
            .graph
            .nodes
            .iter()
            .position(|n| n.id == old)
            .map(|index| index as u32 + 1)
    };
    let nodes: Vec<Node> = export
        .graph
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| Node {
            id: index as u32 + 1,
            ..node.clone()
        })
        .collect();
    // A connection to a node that does not exist keeps its old id, so the validator names it.
    let connections: Vec<Connection> = export
        .graph
        .connections
        .iter()
        .map(|c| {
            let end = |e: &Endpoint| Endpoint {
                node: new_id(e.node).unwrap_or(u32::MAX),
                pin: e.pin.clone(),
            };
            Connection {
                from: end(&c.from),
                to: end(&c.to),
            }
        })
        .collect();
    let mut graph = Graph::new(&new_graph_id(existing), &export.graph.name);
    graph.enabled = false;
    graph.nodes = nodes;
    graph.connections = connections;
    let refusals = validate(&graph, existing);
    if refusals.is_empty() {
        Ok(graph)
    } else {
        Err(ImportError::Invalid(refusals))
    }
}

pub fn import_file(path: &Path, existing: &[Graph]) -> Result<Graph, ImportError> {
    let file = std::fs::File::open(path).map_err(|e| ImportError::Read(e.to_string()))?;
    let size = file
        .metadata()
        .map_err(|e| ImportError::Read(e.to_string()))?
        .len();
    if size > MAX_IMPORT_BYTES {
        return Err(ImportError::TooLarge);
    }
    // The file can grow after the check: read one byte more than the limit.
    let mut bytes = Vec::new();
    file.take(MAX_IMPORT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ImportError::Read(e.to_string()))?;
    if bytes.len() as u64 > MAX_IMPORT_BYTES {
        return Err(ImportError::TooLarge);
    }
    let text = String::from_utf8(bytes).map_err(|e| ImportError::Read(e.to_string()))?;
    import_text(&text, existing)
}
