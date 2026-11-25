//! The engine never panics on any input, and no pass gives a number that is not finite. The test
//! builds Graphs from random kinds, random parameters of every JSON type, random and broken
//! connections, and sensor readings from the whole range of the floating point numbers.

use std::collections::HashMap;
use std::time::Duration;

use nicecooling_core::rules::process::{ProcessEntry, ProcessList};
use nicecooling_graph::eval::{EndValue, Evaluator, Sources};
use nicecooling_graph::model::{
    Connection, Endpoint, Export, Graph, GraphFile, Kind, Node, ResponseKind, StoredGraph,
    param_specs,
};
use nicecooling_graph::pause::{Pause, PowerKind};
use nicecooling_graph::response::{Response, Timing};
use nicecooling_graph::template::{Context, command_environment, expand, labelled_values};
use nicecooling_graph::validate::validate;
use serde_json::{Value, json};

const EXTREMES: [f64; 14] = [
    0.0,
    -0.0,
    1.0,
    -1.0,
    0.1,
    1e-320,
    f64::MIN_POSITIVE,
    1e154,
    1e308,
    -1e308,
    f64::MAX,
    f64::MIN,
    f64::INFINITY,
    f64::NAN,
];

struct Wild {
    readings: HashMap<String, Option<f64>>,
    processes: ProcessList,
    alarm: Option<bool>,
}

impl Sources for Wild {
    fn sensor(&self, id: &str) -> Option<f64> {
        self.readings.get(id).copied().flatten()
    }
    fn alarm_active(&self, _: &str, _: &str) -> Option<bool> {
        self.alarm
    }
    fn processes(&self) -> &ProcessList {
        &self.processes
    }
}

fn number(rng: &mut fastrand::Rng) -> f64 {
    match rng.u8(0..4) {
        0 => EXTREMES[rng.usize(0..EXTREMES.len())],
        1 => (rng.f64() * 2.0 - 1.0) * 1e308,
        2 => rng.f64() * 200.0 - 100.0,
        _ => f64::from(rng.i32(-5000..5000)) / 7.0,
    }
}

fn text(rng: &mut fastrand::Rng) -> String {
    const PARTS: [&str; 12] = [
        "",
        "a",
        "{",
        "}",
        "{{",
        "}}",
        "{value:x}",
        "{unit:x}",
        "{time}",
        "é",
        "/bin/true",
        "x y",
    ];
    (0..rng.usize(0..4))
        .map(|_| PARTS[rng.usize(0..PARTS.len())])
        .collect()
}

fn json_value(rng: &mut fastrand::Rng) -> Value {
    match rng.u8(0..8) {
        0 => Value::Null,
        1 => Value::Bool(rng.bool()),
        2 => json!(number(rng).clamp(-1e308, 1e308)),
        3 => json!(rng.i64(-3..4000)),
        4 => Value::String(text(rng)),
        5 => json!([[number(rng).clamp(-1e308, 1e308), 1], [2, 3]]),
        6 => json!(vec![text(rng); rng.usize(0..40)]),
        _ => json!({"a": 1}),
    }
}

fn random_graph(rng: &mut fastrand::Rng, id: &str) -> Graph {
    let mut graph = Graph::new(id, &text(rng));
    graph.enabled = rng.u8(0..8) != 0;
    if rng.bool() {
        graph.sensor_id = Some(format!("graph_{}", rng.u32(0..4)));
    }
    let count = rng.u32(1..14);
    for i in 0..count {
        let kind = Kind::ALL[rng.usize(0..Kind::ALL.len())];
        let node_id = if rng.u8(0..12) == 0 { 0 } else { i + 1 };
        let mut node = Node::new(node_id, kind);
        if rng.bool() {
            node.label = Some(text(rng));
        }
        for response in [ResponseKind::Notification, ResponseKind::Poweroff] {
            for spec in param_specs(kind, response) {
                match rng.u8(0..4) {
                    0 => {
                        node.params.insert(spec.name.to_owned(), json_value(rng));
                    }
                    1 => {
                        node.params.insert(
                            spec.name.to_owned(),
                            json!(number(rng).clamp(-1e308, 1e308)),
                        );
                    }
                    _ => {}
                }
            }
        }
        if rng.u8(0..6) == 0 {
            node.params
                .insert("sensor".into(), json!(format!("s{}", rng.u8(0..4))));
        }
        graph.nodes.push(node);
    }
    for _ in 0..rng.u32(0..20) {
        let from = rng.u32(0..count + 2);
        let to = rng.u32(0..count + 2);
        let pin = [
            "a", "b", "s", "count", "enable", "reset", "period", "out", "zz",
        ][rng.usize(0..9)];
        let mut connection = Connection::new(from, to, pin);
        if rng.u8(0..10) == 0 {
            connection.from = Endpoint {
                node: from,
                pin: "a".into(),
            };
        }
        graph.connections.push(connection);
    }
    graph
}

fn wild_sources(rng: &mut fastrand::Rng) -> Wild {
    let mut readings = HashMap::new();
    for i in 0..4 {
        let value = (rng.u8(0..5) != 0).then(|| number(rng));
        readings.insert(format!("s{i}"), value);
        readings.insert(format!("graph_{i}"), value);
    }
    let processes = ProcessList {
        entries: vec![ProcessEntry::new("a", "x y", "")],
    };
    Wild {
        readings,
        processes,
        alarm: [None, Some(true), Some(false)][rng.usize(0..3)],
    }
}

#[test]
fn the_engine_never_panics_and_never_gives_a_number_that_is_not_finite() {
    let mut rng = fastrand::Rng::with_seed(2026);
    for round in 0..400 {
        let graphs: Vec<Graph> = (0..rng.usize(1..4))
            .map(|i| random_graph(&mut rng, &format!("graph_{}", i % 2 + 1)))
            .collect();
        for graph in &graphs {
            let others: Vec<Graph> = graphs.iter().filter(|g| *g != graph).cloned().collect();
            let _ = validate(graph, &others);
            let _ = Export::parse(&Export::from_graph(graph).to_json());
        }
        let stored = GraphFile {
            graphs: graphs.iter().cloned().map(StoredGraph::Valid).collect(),
        };
        let _ = GraphFile::parse(&stored.to_json());
        let mut evaluator = Evaluator::with_seed(round);
        let mut now = Duration::ZERO;
        for tick in 0..25 {
            // A clock that jumps and sometimes goes back.
            now = match rng.u8(0..8) {
                0 => now.saturating_sub(Duration::from_secs(rng.u64(0..100))),
                1 => now + Duration::from_secs(rng.u64(0..100_000)),
                2 => now + Duration::from_millis(rng.u64(0..2500)),
                _ => now + Duration::from_secs(1),
            };
            let sources = wild_sources(&mut rng);
            if tick == 12 {
                for graph in &graphs {
                    evaluator.apply(graph);
                }
            }
            let pass = evaluator.evaluate(&graphs, &sources, now);
            for graph_pass in &pass.graphs {
                for (node, value) in &graph_pass.values {
                    assert!(
                        value.is_none_or(f64::is_finite),
                        "round {round} node {node}: {value:?}"
                    );
                }
                if let EndValue::Sensor(Some(v)) = graph_pass.end {
                    assert!(v.is_finite(), "round {round}: end {v}");
                }
            }
            for (_, value) in pass.sensor_values() {
                assert!(value.is_none_or(f64::is_finite), "round {round}");
            }
            for graph in &graphs {
                if let Some(graph_pass) = pass.graph(&graph.id) {
                    let nodes = labelled_values(graph, graph_pass, |_| "°C".into());
                    let context = Context {
                        graph_name: &graph.name,
                        time: "2026-11-25 14:05:09",
                        nodes: &nodes,
                    };
                    let _ = expand(&text(&mut rng), &context);
                    let _ = expand("{value:a}{unit:a}{graph}{time}{{}}{", &context);
                    let _ = command_environment(&graph.id, &graph.name, "run_1", &nodes);
                }
            }
        }
        let _ = evaluator.stored_values();
    }
}

#[test]
fn the_response_and_the_pause_never_panic() {
    let mut rng = fastrand::Rng::with_seed(7);
    for _ in 0..300 {
        let mut node = Node::new(1, Kind::ResponseEnd);
        for name in ["hold_s", "repeat_s", "min_interval_s"] {
            if rng.bool() {
                node.params.insert(name.into(), json_value(&mut rng));
            }
        }
        let timing = Timing::from_node(&node);
        let mut response = Response::default();
        let mut pause = Pause::default();
        let mut now = Duration::ZERO;
        for _ in 0..60 {
            now += Duration::from_secs(rng.u64(0..4000));
            let wall = if rng.u8(0..6) == 0 {
                rng.u64(0..u64::MAX)
            } else {
                rng.u64(0..2_000_000_000_000_000)
            };
            let input = [None, Some(true), Some(false)][rng.usize(0..3)];
            let _ = response.step(&timing, input, now, wall);
            let _ = Response::restore(&response.record(), now);
            let kind =
                [PowerKind::Poweroff, PowerKind::Reboot, PowerKind::Suspend][rng.usize(0..3)];
            match rng.u8(0..4) {
                0 => {
                    pause.start_run(kind);
                }
                1 => pause.cancel_run(kind),
                2 => pause.advance(rng.u64(0..u64::MAX)),
                _ => pause.turn_on(),
            }
        }
    }
}
