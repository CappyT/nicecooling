//! One test for each scenario of `specs/node-graph/spec.md` that the unit tests of the crate do
//! not check with the values of the scenario. The test names follow the scenario names.

use std::collections::{BTreeSet, HashMap};
use std::time::Duration;

use nicecooling_core::rules::process::{ProcessEntry, ProcessList};
use nicecooling_graph::eval::{EndValue, Evaluator, GraphPass, Sources};
use nicecooling_graph::model::{
    Connection, Export, Graph, GraphFile, Kind, Node, ResponseKind, StoredGraph,
    default_params_for, param_specs,
};
use nicecooling_graph::validate::validate;
use serde_json::json;

#[derive(Default)]
struct Fake {
    sensors: HashMap<String, f64>,
    alarms: Option<BTreeSet<(String, String)>>,
    processes: ProcessList,
}

impl Fake {
    fn with(mut self, id: &str, value: f64) -> Self {
        self.sensors.insert(id.to_owned(), value);
        self
    }

    fn running(mut self, comm: &str, exe: &str) -> Self {
        self.processes
            .entries
            .push(ProcessEntry::new(comm, exe, ""));
        self
    }
}

impl Sources for Fake {
    fn sensor(&self, id: &str) -> Option<f64> {
        self.sensors.get(id).copied()
    }

    fn alarm_active(&self, event: &str, source: &str) -> Option<bool> {
        self.alarms
            .as_ref()
            .map(|a| a.contains(&(event.to_owned(), source.to_owned())))
    }

    fn processes(&self) -> &ProcessList {
        &self.processes
    }
}

fn constant_node(id: u32, value: f64) -> Node {
    Node::new(id, Kind::Constant).with_param("value", value)
}

fn source(id: u32, sensor: &str) -> Node {
    Node::new(id, Kind::SensorSource).with_param("sensor", sensor)
}

fn graph(nodes: Vec<Node>, connections: Vec<Connection>) -> Graph {
    let mut g = Graph::new("graph_1", "Test");
    g.nodes = nodes;
    g.connections = connections;
    g
}

fn pass_at(evaluator: &mut Evaluator, graph: &Graph, fake: &Fake, second: u64) -> GraphPass {
    evaluator
        .evaluate(
            std::slice::from_ref(graph),
            fake,
            Duration::from_secs(second),
        )
        .graphs
        .remove(0)
}

fn pass(graph: &Graph, fake: &Fake) -> GraphPass {
    pass_at(&mut Evaluator::with_seed(1), graph, fake, 0)
}

/// The value of the node `id` when the Graph `nodes` + `connections` runs once.
fn value_of(id: u32, graph: &Graph, fake: &Fake) -> Option<f64> {
    pass(graph, fake).values[&id]
}

#[test]
fn one_output_two_nodes() {
    // One output pin to the input pins of two nodes.
    let g = graph(
        vec![
            constant_node(1, 4.0),
            Node::new(2, Kind::Add),
            Node::new(3, Kind::Subtract),
            Node::new(4, Kind::SensorEnd),
        ],
        vec![
            Connection::new(1, 2, "a"),
            Connection::new(1, 3, "a"),
            Connection::new(2, 4, "a"),
        ],
    );
    assert_eq!(validate(&g, &[]), vec![]);
    let pass = pass(&g, &Fake::default());
    assert_eq!(pass.values[&2], Some(4.0));
    assert_eq!(pass.values[&3], Some(4.0));
}

#[test]
fn sensor_link() {
    let mut g = graph(
        vec![constant_node(1, 12.5), Node::new(2, Kind::SensorEnd)],
        vec![Connection::new(1, 2, "a")],
    );
    g.sensor_id = Some("graph_3".into());
    assert_eq!(validate(&g, &[]), vec![]);
    let mut evaluator = Evaluator::with_seed(1);
    let pass = evaluator.evaluate(&[g], &Fake::default(), Duration::ZERO);
    assert_eq!(pass.graphs[0].end, EndValue::Sensor(Some(12.5)));
    assert_eq!(
        pass.sensor_values(),
        vec![("graph_3".to_owned(), Some(12.5))]
    );
}

#[test]
fn counter_period() {
    let g = graph(
        vec![
            constant_node(1, 0.5),
            Node::new(2, Kind::Counter),
            Node::new(3, Kind::SensorEnd),
        ],
        vec![Connection::new(1, 2, "period"), Connection::new(2, 3, "a")],
    );
    assert_eq!(validate(&g, &[]), vec![]);
}

#[test]
fn constant() {
    let g = graph(
        vec![constant_node(1, 2.5), Node::new(2, Kind::SensorEnd)],
        vec![Connection::new(1, 2, "a")],
    );
    let mut evaluator = Evaluator::with_seed(1);
    for second in 0..20 {
        let pass = pass_at(&mut evaluator, &g, &Fake::default(), second);
        assert_eq!(pass.values[&1], Some(2.5), "tick {second}");
        assert_eq!(pass.end, EndValue::Sensor(Some(2.5)));
    }
}

#[test]
fn process_runs() {
    let g = graph(
        vec![Node::new(1, Kind::Process).with_param("name", "blender")],
        vec![],
    );
    assert_eq!(
        value_of(1, &g, &Fake::default().running("Blender", "")),
        Some(1.0)
    );
}

#[test]
fn process_of_another_user_by_path() {
    // The app cannot read the program path of a process of another user: its entry has the
    // `comm` of that process and no path.
    let g = graph(
        vec![Node::new(1, Kind::Process).with_param("name", "render-tool")],
        vec![],
    );
    let other_user = Fake::default().running("python3", "");
    assert_eq!(value_of(1, &g, &other_user), Some(0.0));
    // The same process with a readable path matches.
    let own = Fake::default().running("python3", "render-tool");
    assert_eq!(value_of(1, &g, &own), Some(1.0));
}

#[test]
fn process_ends() {
    let g = graph(
        vec![Node::new(1, Kind::Process).with_param("name", "blender")],
        vec![],
    );
    let mut evaluator = Evaluator::with_seed(1);
    let running = Fake::default().running("blender", "");
    assert_eq!(
        pass_at(&mut evaluator, &g, &running, 0).values[&1],
        Some(1.0)
    );
    // The last process of that name ends.
    let ended = Fake::default().running("bash", "");
    assert_eq!(pass_at(&mut evaluator, &g, &ended, 1).values[&1], Some(0.0));
}

/// A round node with `decimals` and `mode`, fed with `a`.
fn round(decimals: u32, mode: &str, a: f64) -> Option<f64> {
    let g = graph(
        vec![
            constant_node(1, a),
            Node::new(2, Kind::Round)
                .with_param("decimals", decimals)
                .with_param("mode", mode),
        ],
        vec![Connection::new(1, 2, "a")],
    );
    value_of(2, &g, &Fake::default())
}

#[test]
#[allow(
    clippy::approx_constant,
    reason = "the scenario of the spec uses 3.141"
)]
fn round_nearest_down_up() {
    assert_eq!(round(1, "nearest", 2.25), Some(2.3));
    assert_eq!(round(0, "down", -1.5), Some(-2.0));
    assert_eq!(round(2, "up", 3.141), Some(3.15));
    assert_eq!(round(2, "down", 0.29), Some(0.29));
    assert_eq!(round(8, "down", 1.123_456_789), Some(1.123_456_78));
    assert_eq!(round(0, "nearest", 3_000_000_000.3), Some(3_000_000_000.0));
}

fn source_graph() -> Graph {
    graph(
        vec![source(1, "nct_temp1"), Node::new(2, Kind::SensorEnd)],
        vec![Connection::new(1, 2, "a")],
    )
}

#[test]
fn live_sensor() {
    let fake = Fake::default().with("nct_temp1", 46.123_456_789);
    assert_eq!(
        pass(&source_graph(), &fake).end,
        EndValue::Sensor(Some(46.123_456_79))
    );
}

#[test]
fn rounding_down() {
    let fake = Fake::default().with("nct_temp1", 46.123_456_782_6);
    assert_eq!(
        pass(&source_graph(), &fake).end,
        EndValue::Sensor(Some(46.123_456_78))
    );
}

#[test]
fn exact_value() {
    let fake = Fake::default().with("nct_temp1", 46.5);
    assert_eq!(
        pass(&source_graph(), &fake).end,
        EndValue::Sensor(Some(46.5))
    );
}

#[test]
fn sensor_that_does_not_exist() {
    // The Graph is valid: the validator does not know the sensors of the machine. The node is
    // unbound and gives no value, and so does the Graph sensor.
    let g = graph(
        vec![source(1, "nct9999_temp7"), Node::new(2, Kind::SensorEnd)],
        vec![Connection::new(1, 2, "a")],
    );
    assert_eq!(validate(&g, &[]), vec![]);
    let fake = Fake::default().with("nct_temp1", 40.0);
    let pass = pass(&g, &fake);
    assert_eq!(pass.values[&1], None);
    assert_eq!(pass.end, EndValue::Sensor(None));
}

#[test]
fn equal_with_tolerance() {
    // A = 0.1 + 0.2, computed by an addition node.
    let g = graph(
        vec![
            constant_node(1, 0.1),
            constant_node(2, 0.2),
            Node::new(3, Kind::Add),
            constant_node(4, 0.3),
            Node::new(5, Kind::Equal),
        ],
        vec![
            Connection::new(1, 3, "a"),
            Connection::new(2, 3, "b"),
            Connection::new(3, 5, "a"),
            Connection::new(4, 5, "b"),
        ],
    );
    let pass = pass(&g, &Fake::default());
    assert_eq!(pass.values[&3], Some(0.1 + 0.2));
    assert_ne!(pass.values[&3], Some(0.3));
    assert_eq!(pass.values[&5], Some(1.0));
}

#[test]
fn switch_with_an_input_without_value() {
    // S = 0, A = 10, and B on a source with no value.
    let g = graph(
        vec![
            constant_node(1, 0.0),
            constant_node(2, 10.0),
            source(3, "dead"),
            Node::new(4, Kind::Switch),
        ],
        vec![
            Connection::new(1, 4, "s"),
            Connection::new(2, 4, "a"),
            Connection::new(3, 4, "b"),
        ],
    );
    assert_eq!(value_of(4, &g, &Fake::default()), None);
    // With a value on B the same Graph gives A.
    assert_eq!(
        value_of(4, &g, &Fake::default().with("dead", 20.0)),
        Some(10.0)
    );
}

#[test]
fn value_lost() {
    // The input of "has value" is a sensor source node whose sensor has no value.
    let g = graph(
        vec![
            source(1, "dead"),
            Node::new(2, Kind::HasValue),
            constant_node(3, 5.0),
            Node::new(4, Kind::Add),
        ],
        vec![
            Connection::new(1, 2, "a"),
            Connection::new(2, 4, "a"),
            Connection::new(3, 4, "b"),
        ],
    );
    let lost = pass(&g, &Fake::default());
    assert_eq!(lost.values[&2], Some(0.0));
    assert_eq!(lost.values[&4], Some(5.0));
    let there = pass(&g, &Fake::default().with("dead", 12.0));
    assert_eq!(there.values[&2], Some(1.0));
}

/// A memory node with A = 5. The sensor `s` is its S.
fn memory_graph() -> Graph {
    graph(
        vec![
            constant_node(1, 5.0),
            source(2, "s"),
            Node::new(3, Kind::Memory),
            Node::new(4, Kind::SensorEnd),
        ],
        vec![
            Connection::new(1, 3, "a"),
            Connection::new(2, 3, "s"),
            Connection::new(3, 4, "a"),
        ],
    )
}

#[test]
fn memory_restart() {
    let g = memory_graph();
    let mut evaluator = Evaluator::with_seed(1);
    let taking = Fake::default().with("s", 1.0);
    let holding = Fake::default().with("s", 0.0);
    assert_eq!(
        pass_at(&mut evaluator, &g, &taking, 0).values[&3],
        Some(5.0)
    );
    assert_eq!(
        pass_at(&mut evaluator, &g, &holding, 1).values[&3],
        Some(5.0)
    );
    // The app restarts: the evaluator is new. The node gives 0 until S is true.
    let mut restarted = Evaluator::with_seed(1);
    assert_eq!(
        pass_at(&mut restarted, &g, &holding, 0).values[&3],
        Some(0.0)
    );
    assert_eq!(
        pass_at(&mut restarted, &g, &holding, 1).values[&3],
        Some(0.0)
    );
    assert_eq!(
        pass_at(&mut restarted, &g, &taking, 2).values[&3],
        Some(5.0)
    );
}

#[test]
fn input_above_zero() {
    let response_graph = |value: f64| {
        graph(
            vec![
                constant_node(1, value),
                Node::new(2, Kind::ResponseEnd).with_param("title", "Hot"),
            ],
            vec![Connection::new(1, 2, "a")],
        )
    };
    let end = |value: f64| pass(&response_graph(value), &Fake::default()).end;
    assert_eq!(end(1.0), EndValue::Response(Some(true)));
    assert_eq!(end(0.001), EndValue::Response(Some(true)));
    assert_eq!(end(0.0), EndValue::Response(Some(false)));
    assert_eq!(end(-1.0), EndValue::Response(Some(false)));
}

#[test]
fn parameter_names() {
    let names = |kind: Kind| -> Vec<&'static str> {
        param_specs(kind, ResponseKind::Notification)
            .into_iter()
            .map(|s| s.name)
            .collect()
    };
    let response = |kind: ResponseKind| -> Vec<&'static str> {
        param_specs(Kind::ResponseEnd, kind)
            .into_iter()
            .map(|s| s.name)
            .collect()
    };
    let has = |found: &[&str], expected: &[&str]| {
        for name in expected {
            assert!(found.contains(name), "{name} is missing from {found:?}");
        }
    };
    assert_eq!(names(Kind::SensorSource), ["sensor"]);
    has(&names(Kind::AlarmEventSource), &["event", "source"]);
    assert_eq!(names(Kind::AlarmEventSource).len(), 2);
    assert_eq!(names(Kind::Constant), ["value"]);
    has(&names(Kind::Random), &["min", "max"]);
    assert_eq!(names(Kind::Random).len(), 2);
    assert_eq!(names(Kind::Process), ["name"]);
    has(&names(Kind::Round), &["decimals", "mode"]);
    assert_eq!(names(Kind::Round).len(), 2);
    has(&names(Kind::Limiter), &["min", "max"]);
    has(&names(Kind::Table), &["mode", "pairs"]);
    assert_eq!(names(Kind::Table).len(), 2);
    assert_eq!(names(Kind::RiseDelay), ["delay_s"]);
    assert_eq!(names(Kind::FallDelay), ["delay_s"]);
    for kind in [
        Kind::Peak,
        Kind::WindowMinimum,
        Kind::WindowAverage,
        Kind::Trend,
    ] {
        assert_eq!(names(kind), ["span_s"], "{kind}");
    }
    has(&names(Kind::LowPass), &["span_s", "damping"]);
    assert_eq!(names(Kind::LowPass).len(), 2);
    assert_eq!(names(Kind::SensorEnd), ["unit"]);
    has(&names(Kind::Comment), &["text", "width", "height"]);
    assert_eq!(names(Kind::Comment).len(), 3);
    for kind in [
        Kind::Add,
        Kind::Not,
        Kind::Switch,
        Kind::Memory,
        Kind::Counter,
        Kind::LinearGen,
    ] {
        assert!(names(kind).is_empty(), "{kind}");
    }
    let timing = ["kind", "hold_s", "repeat_s", "min_interval_s"];
    has(&response(ResponseKind::Notification), &timing);
    has(
        &response(ResponseKind::Notification),
        &["title", "text", "urgency"],
    );
    has(&response(ResponseKind::Sound), &timing);
    has(&response(ResponseKind::Sound), &["sound"]);
    has(
        &response(ResponseKind::Command),
        &["path", "args", "time_limit_s"],
    );
    has(&response(ResponseKind::Email), &["to", "subject", "text"]);
    has(&response(ResponseKind::Mqtt), &["topic", "payload"]);
    for kind in [
        ResponseKind::Poweroff,
        ResponseKind::Reboot,
        ResponseKind::Suspend,
    ] {
        has(&response(kind), &timing);
        has(&response(kind), &["countdown_s"]);
    }
}

#[test]
fn keys() {
    // Node 4 of `graph_2` is a persistent storage node that holds 7.
    let mut g = Graph::new("graph_2", "Keys");
    g.nodes = vec![
        constant_node(1, 7.0),
        constant_node(2, 1.0),
        constant_node(3, 0.0),
        Node::new(4, Kind::PersistentStorage),
        Node::new(5, Kind::SensorEnd),
    ];
    g.connections = vec![
        Connection::new(1, 4, "a"),
        Connection::new(2, 4, "s"),
        Connection::new(4, 5, "a"),
    ];
    let mut evaluator = Evaluator::with_seed(1);
    evaluator.evaluate(&[g], &Fake::default(), Duration::ZERO);
    let stored = evaluator.stored_values();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored["graph_2/4"], 7.0);
}

#[test]
fn no_secret_in_the_file() {
    let mut end = Node::new(2, Kind::ResponseEnd);
    end.params = default_params_for(Kind::ResponseEnd, ResponseKind::Email);
    end.params.insert("kind".into(), "email".into());
    end.params.insert("to".into(), "owner@example.org".into());
    end.params
        .insert("subject".into(), "Water {value:water}".into());
    end.params.insert("text".into(), "The water is hot".into());
    let mut g = graph(
        vec![constant_node(1, 1.0), end],
        vec![Connection::new(1, 2, "a")],
    );
    g.sensor_id = None;
    let text = Export::from_graph(&g).to_json();
    assert!(text.contains("owner@example.org"));
    assert!(text.contains("Water {value:water}"));
    assert!(text.contains("The water is hot"));
    assert!(!text.to_lowercase().contains("password"), "{text}");
    let back = Export::parse(&text).unwrap();
    assert_eq!(back.graph.nodes[1].text("to"), "owner@example.org");
    assert_eq!(back.graph.nodes[1].text("subject"), "Water {value:water}");
    assert_eq!(back.graph.nodes[1].text("text"), "The water is hot");
}

#[test]
fn export_file() {
    // A Graph of 5 nodes with a Graph sensor.
    let mut g = graph(
        vec![
            constant_node(1, 1.0),
            constant_node(2, 2.0),
            Node::new(3, Kind::Add),
            Node::new(4, Kind::Round),
            Node::new(5, Kind::SensorEnd),
        ],
        vec![
            Connection::new(1, 3, "a"),
            Connection::new(2, 3, "b"),
            Connection::new(3, 4, "a"),
            Connection::new(4, 5, "a"),
        ],
    );
    g.sensor_id = Some("graph_4".into());
    let text = Export::from_graph(&g).to_json();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["format"], "nicecooling-graph");
    assert_eq!(value["format_version"], 1);
    assert_eq!(value["graph"]["nodes"].as_array().unwrap().len(), 5);
    assert_eq!(value["graph"]["connections"].as_array().unwrap().len(), 4);
    assert_eq!(value["graph"]["name"], "Test");
    // No `sensor_id` and no state in the file.
    assert!(value["graph"].get("sensor_id").is_none());
    assert!(value.get("sensor_id").is_none());
    assert!(!text.contains("graph_4"));
}

#[test]
fn graph_storage_restart() {
    // The user applies a Graph and the app restarts: the Graph has the same nodes, connections and
    // Graph sensor.
    let mut g = graph(
        vec![
            constant_node(1, 5.0),
            Node::new(2, Kind::Table).with_param("pairs", json!([[20, 30], [40, 80]])),
            Node::new(3, Kind::SensorEnd).with_param("unit", "%"),
        ],
        vec![Connection::new(1, 2, "a"), Connection::new(2, 3, "a")],
    );
    g.sensor_id = Some("graph_7".into());
    let file = GraphFile {
        graphs: vec![StoredGraph::Valid(g.clone())],
    };
    let back = GraphFile::parse(&file.to_json()).unwrap();
    let StoredGraph::Valid(back) = &back.graphs[0] else {
        panic!("the Graph is valid after the restart");
    };
    assert_eq!(back.nodes.len(), 3);
    assert_eq!(back.connections, g.connections);
    assert_eq!(back.sensor_id.as_deref(), Some("graph_7"));
    assert_eq!(back, &g);
}
