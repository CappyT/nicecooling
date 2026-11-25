//! Review reproducers: a node with finite inputs and a finite result gives no value when an
//! intermediate step of its formula is not finite.

use std::collections::HashMap;
use std::time::Duration;

use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::eval::{EndValue, Evaluator, Sources};
use nicecooling_graph::funcs::binary;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};

#[derive(Default)]
struct Fake {
    sensors: HashMap<String, f64>,
    processes: ProcessList,
}

impl Sources for Fake {
    fn sensor(&self, id: &str) -> Option<f64> {
        self.sensors.get(id).copied()
    }
    fn alarm_active(&self, _: &str, _: &str) -> Option<bool> {
        Some(false)
    }
    fn processes(&self) -> &ProcessList {
        &self.processes
    }
}

/// Spec "Arithmetic of two inputs": the average is (A + B) / 2. Both inputs and the result are
/// finite.
#[test]
fn average_of_two_large_numbers_has_a_value() {
    assert_eq!(binary(Kind::Average, 1e308, 1e308), Some(1e308));
}

/// Spec "Sensor source node": the node gives the value of the sensor, rounded to 8 decimals.
#[test]
fn large_finite_reading_has_a_value() {
    let mut g = Graph::new("graph_1", "g");
    g.nodes = vec![
        Node::new(1, Kind::SensorSource).with_param("sensor", "big"),
        Node::new(2, Kind::SensorEnd),
    ];
    g.connections = vec![Connection::new(1, 2, "a")];
    let fake = Fake {
        sensors: HashMap::from([("big".to_owned(), 1e301)]),
        ..Fake::default()
    };
    let pass = Evaluator::with_seed(1).evaluate(std::slice::from_ref(&g), &fake, Duration::ZERO);
    assert_eq!(pass.graphs[0].end, EndValue::Sensor(Some(1e301)));
}
