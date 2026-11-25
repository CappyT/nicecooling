//! Graphs and directories for the tests of the Graph service.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use nicecooling::graphs::store::GraphPaths;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};

/// A new empty directory for one test.
pub struct Dir(pub PathBuf);

impl Dir {
    pub fn new(name: &str) -> Self {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nicecooling-graphs-{name}-{}-{id}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("test directory");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    /// The files of the Graph service inside this directory.
    pub fn graph_paths(&self) -> GraphPaths {
        GraphPaths::in_dirs(&self.0.join("config"), &self.0.join("data"))
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A Graph with a sensor end node: a constant that feeds the end node.
pub fn constant_graph(id: &str, value: f64) -> Graph {
    let mut graph = Graph::new(id, "Five");
    graph.nodes = vec![
        Node::new(1, Kind::Constant).with_param("value", value),
        Node::new(2, Kind::SensorEnd),
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    graph
}

/// A Graph with a sensor end node whose input is the sensor `sensor`.
pub fn source_graph(id: &str, sensor: &str) -> Graph {
    let mut graph = Graph::new(id, "Source");
    graph.nodes = vec![
        Node::new(1, Kind::SensorSource).with_param("sensor", sensor),
        Node::new(2, Kind::SensorEnd),
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    graph
}

/// A Graph with a persistent storage node 4 that has no input.
pub fn persistent_graph(id: &str) -> Graph {
    let mut graph = constant_graph(id, 1.0);
    graph.nodes.push(Node::new(4, Kind::PersistentStorage));
    graph
}
