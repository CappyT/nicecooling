//! Export and import of one Graph as one JSON file (`docs/storage.md`, Export file). The daemon
//! has no part in it.

use std::fmt;
use std::io::Read as _;
use std::path::Path;

use nicecooling_graph::model::{Export, ExportError, Graph, Node};
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
/// It is the id of a draft in the editor. The store gives the stored Graph its id
/// (`GraphStore::add`), so this id can change at the first Apply.
pub fn new_graph_id(existing: &[Graph]) -> String {
    next_graph_id(existing.iter().map(|g| g.id.as_str()))
}

/// `graph_<n>` above every `graph_<n>` of `ids`.
pub fn next_graph_id<'a>(ids: impl Iterator<Item = &'a str>) -> String {
    let highest = ids
        .filter_map(nicecooling_graph::model::parse_sensor_id)
        .max()
        .unwrap_or(0);
    nicecooling_graph::model::format_sensor_id(highest.saturating_add(1))
}

/// Make a Graph of an export. The validator reads the Graph of the file first, with the ids of
/// the file: a Graph that fails is refused. Then the Graph and its nodes get new ids. The Graph is
/// disabled and has no `sensor_id`, so a response from another machine does not run before the user
/// enabled it. A source on a sensor that this machine does not have stays as it is: it gives no
/// value. A parameter that the kind of a node does not define is dropped.
/// `existing` holds every stored entry as a Graph (`GraphStore::all_graphs`), for the id and the
/// validator: an entry that does not run keeps its id and counts for the limit.
pub fn import_text(text: &str, existing: &[Graph]) -> Result<Graph, ImportError> {
    let export = Export::parse(text).map_err(ImportError::Format)?;
    let mut graph = Graph::new(&new_graph_id(existing), &export.graph.name);
    graph.enabled = false;
    graph.nodes = export.graph.nodes.into_iter().map(known_params).collect();
    graph.connections = export.graph.connections;
    let refusals = validate(&graph, existing);
    if !refusals.is_empty() {
        return Err(ImportError::Invalid(refusals));
    }
    renumber(&mut graph);
    Ok(graph)
}

/// Keep the parameters that the kind of the node defines. A field that the kind does not know (a
/// `password` of a hand-made file) never reaches `graphs.json` and the next export.
fn known_params(mut node: Node) -> Node {
    let specs = node.param_specs();
    node.params
        .retain(|name, _| specs.iter().any(|spec| spec.name == name.as_str()));
    node
}

/// The nodes count from 1, in the order of the file, and the connections follow. The validator
/// accepted the Graph, so each node id is unique and each connection ends on a node.
fn renumber(graph: &mut Graph) {
    let old: Vec<u32> = graph.nodes.iter().map(|n| n.id).collect();
    let new_id = |id: u32| {
        old.iter()
            .position(|&o| o == id)
            .map_or(id, |index| index as u32 + 1)
    };
    for (index, node) in graph.nodes.iter_mut().enumerate() {
        node.id = index as u32 + 1;
    }
    for connection in &mut graph.connections {
        connection.from.node = new_id(connection.from.node);
        connection.to.node = new_id(connection.to.node);
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
