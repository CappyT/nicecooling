//! What the Graph service shows to the window and the tray: one shared state behind a mutex.
//! The service writes it once per second, and the screens read it.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use nicecooling_graph::eval::EndValue;

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

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphView {
    pub rows: Vec<GraphRow>,
    /// Why a Graph file cannot be used. The app writes nothing over such a file.
    pub error: Option<String>,
}

impl GraphView {
    pub fn row(&self, id: &str) -> Option<&GraphRow> {
        self.rows.iter().find(|row| row.id == id)
    }
}

/// The view that the service and the screens share.
pub type SharedView = Arc<Mutex<GraphView>>;

pub fn shared_view() -> SharedView {
    Arc::new(Mutex::new(GraphView::default()))
}
