//! Review reproducer: two Graphs with the same id share the state of their nodes. The validator
//! takes each one for the stored version of the other, so it refuses neither.

use std::time::Duration;

use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::eval::{Evaluator, Sources};
use nicecooling_graph::model::{Connection, Graph, Kind, Node};

struct Offline(ProcessList);

impl Sources for Offline {
    fn sensor(&self, _: &str) -> Option<f64> {
        None
    }
    fn alarm_active(&self, _: &str, _: &str) -> Option<bool> {
        None
    }
    fn processes(&self) -> &ProcessList {
        &self.0
    }
}

/// A memory node fed with S and A = 5.
fn memory_graph(name: &str, s: f64) -> Graph {
    let mut g = Graph::new("graph_1", name);
    g.nodes = vec![
        Node::new(1, Kind::Constant).with_param("value", s),
        Node::new(2, Kind::Constant).with_param("value", 5.0),
        Node::new(3, Kind::Memory),
        Node::new(4, Kind::SensorEnd),
    ];
    g.connections = vec![
        Connection::new(1, 3, "s"),
        Connection::new(2, 3, "a"),
        Connection::new(3, 4, "a"),
    ];
    g
}

/// Spec "Memory node": the start value is 0. The memory of Graph B never takes a value.
#[test]
fn graphs_with_one_id_do_not_share_node_state() {
    let graphs = [memory_graph("A", 1.0), memory_graph("B", 0.0)];
    let pass =
        Evaluator::with_seed(1).evaluate(&graphs, &Offline(ProcessList::default()), Duration::ZERO);
    assert_eq!(pass.graphs[1].values[&3], Some(0.0));
}
