//! The draft of the Graph editor: the Graph that the user changes, with undo and redo.
//!
//! The app stores nothing before "Apply". One change of the draft is one undo step: one drag,
//! one committed parameter, one connection. The draft keeps the newest 100 steps.

use std::collections::VecDeque;

use nicecooling_graph::model::{
    Connection, Graph, Kind, Node, ResponseKind, default_params, default_params_for,
};
use nicecooling_graph::validate::{Refusal, validate};
use serde_json::Value;

/// The undo steps that the draft keeps.
pub const UNDO_LIMIT: usize = 100;

/// Whether two Graphs have the same name, nodes and connections. The switch of the Graphs screen
/// and the id of the Graph sensor are not part of the draft.
pub fn same_content(a: &Graph, b: &Graph) -> bool {
    a.name == b.name && a.nodes == b.nodes && a.connections == b.connections
}

#[derive(Clone, Debug, PartialEq)]
pub struct Draft {
    graph: Graph,
    /// The Graph as the app stores it. `None` for a Graph that the app does not store yet.
    stored: Option<Graph>,
    undo: VecDeque<Graph>,
    redo: Vec<Graph>,
    /// The id of the next new node. It never goes back: a new node never takes the id of a node
    /// of the stored Graph, so it never shows the live value of that node.
    next_node: u32,
}

fn highest_node(graph: &Graph) -> u32 {
    graph.nodes.iter().map(|node| node.id).max().unwrap_or(0)
}

impl Draft {
    /// A draft of `graph`. `stored` is the Graph with the same id that the app stores.
    pub fn new(graph: Graph, stored: Option<Graph>) -> Self {
        let highest = highest_node(&graph).max(stored.as_ref().map_or(0, highest_node));
        Self {
            graph,
            stored,
            undo: VecDeque::new(),
            redo: Vec::new(),
            next_node: highest.saturating_add(1),
        }
    }

    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    pub fn stored(&self) -> Option<&Graph> {
        self.stored.as_ref()
    }

    /// The app stores another version of the Graph now: after "Apply", or after a command of the
    /// Graphs screen.
    pub fn set_stored(&mut self, stored: Option<Graph>) {
        if let Some(stored) = &stored {
            self.next_node = self.next_node.max(highest_node(stored).saturating_add(1));
        }
        self.stored = stored;
    }

    /// Whether the draft differs from the stored Graph. A Graph that the app does not store yet
    /// differs.
    pub fn is_dirty(&self) -> bool {
        self.stored
            .as_ref()
            .is_none_or(|stored| !same_content(stored, &self.graph))
    }

    /// The Graph that "Apply" stores: the draft with the switch and the Graph sensor of the stored
    /// Graph. A draft with a response end node holds no Graph sensor.
    pub fn candidate(&self) -> Graph {
        let mut graph = self.graph.clone();
        let has_sensor_end = graph.sensor_end().is_some();
        graph.sensor_id = None;
        if let Some(stored) = &self.stored {
            graph.enabled = stored.enabled;
            if has_sensor_end {
                graph.sensor_id = stored.sensor_id.clone();
            }
        }
        graph
    }

    /// The rules of the validator that the draft breaks. `others` are the stored Graphs.
    pub fn refusals(&self, others: &[Graph]) -> Vec<Refusal> {
        validate(&self.candidate(), others)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// The undo steps that the draft holds.
    pub fn undo_steps(&self) -> usize {
        self.undo.len()
    }

    /// Change the draft. A change that leaves the Graph as it is makes no undo step. Returns
    /// whether the Graph changed.
    pub fn change(&mut self, edit: impl FnOnce(&mut Graph)) -> bool {
        let before = self.graph.clone();
        edit(&mut self.graph);
        if self.graph == before {
            return false;
        }
        self.undo.push_back(before);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.pop_front();
        }
        self.redo.clear();
        true
    }

    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop_back() else {
            return false;
        };
        self.redo.push(std::mem::replace(&mut self.graph, previous));
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo
            .push_back(std::mem::replace(&mut self.graph, next));
        true
    }

    pub fn set_name(&mut self, name: &str) -> bool {
        self.change(|graph| graph.name = name.to_owned())
    }

    /// Add a node of `kind` with its default parameters at a point of the canvas. Returns its id.
    pub fn add_node(&mut self, kind: Kind, x: f64, y: f64) -> u32 {
        let id = self.next_node;
        self.next_node = self.next_node.saturating_add(1);
        self.change(|graph| {
            let mut node = Node::new(id, kind);
            node.x = x;
            node.y = y;
            graph.nodes.push(node);
        });
        id
    }

    /// One drag of a node is one call, so one undo step.
    pub fn move_node(&mut self, id: u32, x: f64, y: f64) -> bool {
        self.change(|graph| {
            if let Some(node) = graph.nodes.iter_mut().find(|node| node.id == id) {
                node.x = x;
                node.y = y;
            }
        })
    }

    /// Remove a node with its connections. An end node stays: a Graph keeps its end node.
    pub fn remove_node(&mut self, id: u32) -> bool {
        if self.graph.node(id).is_none_or(|node| node.kind.is_end()) {
            return false;
        }
        self.change(|graph| {
            graph.nodes.retain(|node| node.id != id);
            graph
                .connections
                .retain(|c| c.from.node != id && c.to.node != id);
        })
    }

    /// Connect the output of `from` to the input pin `pin` of `to`, in the place of the
    /// connection that the pin has.
    pub fn connect(&mut self, from: u32, to: u32, pin: &str) -> bool {
        let same = self
            .graph
            .connection_to(to, pin)
            .is_some_and(|c| c.from.node == from);
        if same {
            return false;
        }
        self.change(|graph| {
            graph
                .connections
                .retain(|c| !(c.to.node == to && c.to.pin == pin));
            graph.connections.push(Connection::new(from, to, pin));
        })
    }

    /// Remove the connection of an input pin.
    pub fn disconnect(&mut self, to: u32, pin: &str) -> bool {
        self.change(|graph| {
            graph
                .connections
                .retain(|c| !(c.to.node == to && c.to.pin == pin));
        })
    }

    /// One committed edit of a parameter is one call, so one undo step.
    pub fn set_param(&mut self, node: u32, name: &str, value: Value) -> bool {
        self.change(|graph| {
            if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == node) {
                node.params.insert(name.to_owned(), value);
            }
        })
    }

    /// An empty label removes the label.
    pub fn set_label(&mut self, node: u32, label: &str) -> bool {
        let label = (!label.is_empty()).then(|| label.to_owned());
        self.change(|graph| {
            if let Some(node) = graph.nodes.iter_mut().find(|n| n.id == node) {
                node.label = label;
            }
        })
    }

    /// Change the end node to a sensor end node or to a response end node. The node keeps its id,
    /// its place and its connection, and gets the default parameters of its new kind.
    pub fn set_end_kind(&mut self, kind: Kind) -> bool {
        if !kind.is_end() {
            return false;
        }
        self.change(|graph| {
            if let Some(node) = graph.nodes.iter_mut().find(|node| node.kind.is_end())
                && node.kind != kind
            {
                node.kind = kind;
                node.params = default_params(kind);
                node.label = None;
            }
        })
    }

    /// Change the kind of the response. The minimum interval gets the default of the new kind:
    /// 600 s for poweroff, reboot and suspend, and 0 s for the others. A parameter that the old
    /// and the new kind share keeps its value.
    pub fn set_response_kind(&mut self, kind: ResponseKind) -> bool {
        self.change(|graph| {
            let Some(node) = graph
                .nodes
                .iter_mut()
                .find(|node| node.kind == Kind::ResponseEnd)
            else {
                return;
            };
            if node.response_kind() == kind {
                return;
            }
            let mut params = default_params_for(Kind::ResponseEnd, kind);
            for (name, value) in &node.params {
                if name != "kind" && name != "min_interval_s" && params.contains_key(name) {
                    params.insert(name.clone(), value.clone());
                }
            }
            node.params = params;
        })
    }
}
