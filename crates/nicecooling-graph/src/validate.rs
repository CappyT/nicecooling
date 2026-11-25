//! The validator. The editor and the load of `graphs.json` use the same one. It returns every
//! refusal of a Graph, each with one code.

use std::collections::{BTreeMap, BTreeSet};

use nicecooling_core::graph_sensor::MAX_GRAPH_SENSORS;

use crate::model::{Connection, Graph, Kind, Node, ParamType, parse_pairs, parse_sensor_id};

/// The most Graphs that the app stores.
pub const MAX_GRAPHS: usize = 64;
/// The most nodes in one Graph.
pub const MAX_NODES: usize = 256;
/// The longest Graph name, in characters.
pub const MAX_NAME_CHARS: usize = 64;
/// The most window samples that all Graphs hold together.
pub const MAX_WINDOW_SAMPLES: usize = 200_000;
/// The longest node label, in characters.
pub const MAX_LABEL_CHARS: usize = 32;

/// The code of one refusal. The derived order is the order of the spec list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RefusalCode {
    NoEndNode,
    TwoEndNodes,
    EndNodeOutput,
    UnknownKind,
    UnknownPin,
    PinOccupied,
    SameDirection,
    SameNode,
    Loop,
    LoopAcrossGraphs,
    LimitGraphs,
    LimitNodes,
    LimitName,
    LimitSamples,
    LimitGraphSensors,
    DuplicateLabel,
    DuplicateTableInput,
    BadParameter,
    BadSensorId,
}

impl RefusalCode {
    /// The name of the code in the spec.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoEndNode => "no-end-node",
            Self::TwoEndNodes => "two-end-nodes",
            Self::EndNodeOutput => "end-node-output",
            Self::UnknownKind => "unknown-kind",
            Self::UnknownPin => "unknown-pin",
            Self::PinOccupied => "pin-occupied",
            Self::SameDirection => "same-direction",
            Self::SameNode => "same-node",
            Self::Loop => "loop",
            Self::LoopAcrossGraphs => "loop-across-graphs",
            Self::LimitGraphs => "limit-graphs",
            Self::LimitNodes => "limit-nodes",
            Self::LimitName => "limit-name",
            Self::LimitSamples => "limit-samples",
            Self::LimitGraphSensors => "limit-graph-sensors",
            Self::DuplicateLabel => "duplicate-label",
            Self::DuplicateTableInput => "duplicate-table-input",
            Self::BadParameter => "bad-parameter",
            Self::BadSensorId => "bad-sensor-id",
        }
    }
}

/// One refusal of a Graph.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub code: RefusalCode,
    /// The name of the parameter, for `bad-parameter`.
    pub parameter: Option<String>,
    /// The node that the refusal is about, when there is one.
    pub node: Option<u32>,
}

impl Refusal {
    fn new(code: RefusalCode) -> Self {
        Self {
            code,
            parameter: None,
            node: None,
        }
    }

    fn at(code: RefusalCode, node: u32) -> Self {
        Self {
            node: Some(node),
            ..Self::new(code)
        }
    }

    fn parameter(node: u32, name: &str) -> Self {
        Self {
            code: RefusalCode::BadParameter,
            parameter: Some(name.to_owned()),
            node: Some(node),
        }
    }
}

/// The window samples that the window nodes of a Graph hold at most: the span plus one for each.
pub fn window_samples(graph: &Graph) -> usize {
    graph
        .nodes
        .iter()
        .filter(|n| n.kind.is_window())
        .map(|n| {
            let span = n.number("span_s");
            // A span out of range is refused on its own. Here it only has to stay a number.
            (span.clamp(0.0, 1800.0) as usize) + 1
        })
        .sum()
}

/// Checks `graph` against the rules of the spec. `others` are the other Graphs of the app: the
/// limits and the loop check across Graphs need them. A Graph in `others` with the id of `graph`
/// is the stored version of it and does not count.
pub fn validate(graph: &Graph, others: &[Graph]) -> Vec<Refusal> {
    let others: Vec<&Graph> = others.iter().filter(|g| g.id != graph.id).collect();
    let mut out = Vec::new();

    check_end_nodes(graph, &mut out);
    check_connections(graph, &mut out);
    check_loop_across_graphs(graph, &others, &mut out);
    check_limits(graph, &others, &mut out);
    check_labels(graph, &mut out);
    check_parameters(graph, &mut out);
    check_sensor_id(graph, &others, &mut out);

    // The first refusal in the order of the spec list comes first. The sort keeps the order of the
    // nodes inside one code.
    out.sort_by_key(|r| r.code);
    out.dedup();
    out
}

fn check_end_nodes(graph: &Graph, out: &mut Vec<Refusal>) {
    match graph.end_nodes().count() {
        0 => out.push(Refusal::new(RefusalCode::NoEndNode)),
        1 => {}
        _ => out.push(Refusal::new(RefusalCode::TwoEndNodes)),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Output,
    Input,
    Unknown,
}

fn side(graph: &Graph, node: u32, pin: &str) -> Side {
    let Some(node) = graph.node(node) else {
        return Side::Unknown;
    };
    if node.kind.input(pin).is_some() {
        Side::Input
    } else if pin == crate::model::OUTPUT_PIN && node.kind.has_output() {
        Side::Output
    } else {
        Side::Unknown
    }
}

/// The connections that join an output pin to an input pin of another node.
fn well_formed(graph: &Graph) -> impl Iterator<Item = &Connection> {
    graph.connections.iter().filter(|c| {
        c.from.node != c.to.node
            && side(graph, c.from.node, &c.from.pin) == Side::Output
            && side(graph, c.to.node, &c.to.pin) == Side::Input
    })
}

fn check_connections(graph: &Graph, out: &mut Vec<Refusal>) {
    let mut taken: BTreeSet<(u32, &str)> = BTreeSet::new();
    for c in &graph.connections {
        if graph.node(c.from.node).is_some_and(|n| n.kind.is_end()) {
            out.push(Refusal::at(RefusalCode::EndNodeOutput, c.from.node));
            continue;
        }
        let from = side(graph, c.from.node, &c.from.pin);
        let to = side(graph, c.to.node, &c.to.pin);
        if from == Side::Unknown {
            out.push(Refusal::at(RefusalCode::UnknownPin, c.from.node));
        }
        if to == Side::Unknown {
            out.push(Refusal::at(RefusalCode::UnknownPin, c.to.node));
        }
        if from == Side::Unknown || to == Side::Unknown {
            continue;
        }
        // A connection runs from an output pin to an input pin. Any other pair, the reverse
        // included, has the wrong direction.
        if (from, to) != (Side::Output, Side::Input) {
            out.push(Refusal::at(RefusalCode::SameDirection, c.to.node));
        }
        if c.from.node == c.to.node {
            out.push(Refusal::at(RefusalCode::SameNode, c.to.node));
        }
        if to == Side::Input && !taken.insert((c.to.node, c.to.pin.as_str())) {
            out.push(Refusal::at(RefusalCode::PinOccupied, c.to.node));
        }
    }
    if has_cycle(graph) {
        out.push(Refusal::new(RefusalCode::Loop));
    }
}

/// Whether the well-formed connections close a loop.
fn has_cycle(graph: &Graph) -> bool {
    let mut indegree: BTreeMap<u32, usize> = graph.nodes.iter().map(|n| (n.id, 0)).collect();
    let mut next: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for c in well_formed(graph) {
        next.entry(c.from.node).or_default().push(c.to.node);
        *indegree.entry(c.to.node).or_default() += 1;
    }
    let mut ready: Vec<u32> = indegree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(n, _)| *n)
        .collect();
    let mut done = 0;
    while let Some(node) = ready.pop() {
        done += 1;
        for to in next.get(&node).into_iter().flatten() {
            if let Some(d) = indegree.get_mut(to) {
                *d -= 1;
                if *d == 0 {
                    ready.push(*to);
                }
            }
        }
    }
    done < indegree.len()
}

fn check_loop_across_graphs(graph: &Graph, others: &[&Graph], out: &mut Vec<Refusal>) {
    // The Graph that holds each Graph sensor, with this Graph in place of its stored version.
    let mut all: Vec<&Graph> = others.to_vec();
    all.push(graph);
    let holder: BTreeMap<&str, &Graph> = all
        .iter()
        .filter_map(|g| g.sensor_id.as_deref().map(|id| (id, *g)))
        .collect();
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut stack = vec![graph];
    while let Some(g) = stack.pop() {
        for sensor in g.source_sensors() {
            let Some(&reads) = holder.get(sensor.as_str()) else {
                continue;
            };
            if reads.id == graph.id {
                out.push(Refusal::new(RefusalCode::LoopAcrossGraphs));
                return;
            }
            if seen.insert(reads.id.as_str()) {
                stack.push(reads);
            }
        }
    }
}

fn check_limits(graph: &Graph, others: &[&Graph], out: &mut Vec<Refusal>) {
    if others.len() + 1 > MAX_GRAPHS {
        out.push(Refusal::new(RefusalCode::LimitGraphs));
    }
    if graph.nodes.len() > MAX_NODES {
        out.push(Refusal::new(RefusalCode::LimitNodes));
    }
    if graph.name.chars().count() > MAX_NAME_CHARS {
        out.push(Refusal::new(RefusalCode::LimitName));
    }
    let samples = window_samples(graph) + others.iter().map(|g| window_samples(g)).sum::<usize>();
    if samples > MAX_WINDOW_SAMPLES {
        out.push(Refusal::new(RefusalCode::LimitSamples));
    }
    if graph.sensor_end().is_some() {
        let sensors = 1 + others.iter().filter(|g| g.sensor_end().is_some()).count();
        if sensors > MAX_GRAPH_SENSORS {
            out.push(Refusal::new(RefusalCode::LimitGraphSensors));
        }
    }
}

fn label_is_valid(label: &str) -> bool {
    (1..=MAX_LABEL_CHARS).contains(&label.chars().count())
        && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn check_labels(graph: &Graph, out: &mut Vec<Refusal>) {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    for node in &graph.nodes {
        let Some(label) = node.label.as_deref() else {
            continue;
        };
        if !label_is_valid(label) {
            out.push(Refusal::parameter(node.id, "label"));
        } else if !seen.insert(label) {
            out.push(Refusal::at(RefusalCode::DuplicateLabel, node.id));
        }
    }
}

fn check_parameters(graph: &Graph, out: &mut Vec<Refusal>) {
    let mut ids: BTreeSet<u32> = BTreeSet::new();
    for node in &graph.nodes {
        if node.id == 0 || !ids.insert(node.id) {
            out.push(Refusal::parameter(node.id, "id"));
        }
        let mut bad = BTreeSet::new();
        for spec in node.param_specs() {
            match node.params.get(spec.name) {
                Some(value) if !spec.ty.accepts(value) => {
                    bad.insert(spec.name);
                }
                Some(_) if spec.ty == ParamType::Pairs => check_pairs(node, out),
                Some(_) => {}
                // A missing parameter reads as its default, so a default that fails is refused.
                None if !spec.ty.accepts(&spec.default.to_value()) => {
                    bad.insert(spec.name);
                }
                None => {}
            }
        }
        if matches!(node.kind, Kind::Random | Kind::Limiter)
            && !bad.contains("min")
            && !bad.contains("max")
            && node.number("min") > node.number("max")
        {
            bad.insert("max");
        }
        out.extend(
            bad.into_iter()
                .map(|name| Refusal::parameter(node.id, name)),
        );
    }
}

fn check_pairs(node: &Node, out: &mut Vec<Refusal>) {
    let Some(pairs) = node.params.get("pairs").and_then(parse_pairs) else {
        return;
    };
    let mut inputs: Vec<f64> = pairs.iter().map(|p| p.0).collect();
    inputs.sort_by(f64::total_cmp);
    if inputs.windows(2).any(|w| w[0] == w[1]) {
        out.push(Refusal::at(RefusalCode::DuplicateTableInput, node.id));
    }
}

fn check_sensor_id(graph: &Graph, others: &[&Graph], out: &mut Vec<Refusal>) {
    let Some(id) = graph.sensor_id.as_deref() else {
        return;
    };
    let held_twice = others.iter().any(|g| g.sensor_id.as_deref() == Some(id));
    if parse_sensor_id(id).is_none() || graph.sensor_end().is_none() || held_twice {
        out.push(Refusal::new(RefusalCode::BadSensorId));
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::model::{Graph, ResponseKind, default_params_for};

    fn node(id: u32, kind: Kind) -> Node {
        Node::new(id, kind)
    }

    /// A valid Graph: a constant that feeds a sensor end node.
    fn sensor_graph(id: &str) -> Graph {
        let mut g = Graph::new(id, "Five");
        g.nodes = vec![node(1, Kind::Constant), node(2, Kind::SensorEnd)];
        g.connections = vec![Connection::new(1, 2, "a")];
        g
    }

    /// A valid Graph with a response end node that shows a notification.
    fn response_graph(id: &str) -> Graph {
        let mut g = Graph::new(id, "Alarm");
        let end = node(2, Kind::ResponseEnd).with_param("title", "Hot");
        g.nodes = vec![node(1, Kind::Constant), end];
        g.connections = vec![Connection::new(1, 2, "a")];
        g
    }

    fn codes(refusals: &[Refusal]) -> Vec<RefusalCode> {
        refusals.iter().map(|r| r.code).collect()
    }

    fn only(graph: &Graph, code: RefusalCode) {
        assert_eq!(codes(&validate(graph, &[])), vec![code], "{graph:?}");
    }

    fn bad_parameter(graph: &Graph, name: &str) {
        let refusals = validate(graph, &[]);
        assert_eq!(codes(&refusals), vec![RefusalCode::BadParameter], "{name}");
        assert_eq!(refusals[0].parameter.as_deref(), Some(name));
    }

    #[test]
    fn valid_graphs_pass() {
        assert_eq!(validate(&sensor_graph("graph_1"), &[]), vec![]);
        assert_eq!(validate(&response_graph("graph_2"), &[]), vec![]);
        // A comment node is no end node and has no pins.
        let mut g = sensor_graph("graph_1");
        g.nodes.push(node(3, Kind::Comment));
        assert_eq!(validate(&g, &[]), vec![]);
        // Two nodes read one output.
        let mut g = sensor_graph("graph_1");
        g.nodes.push(node(3, Kind::Add));
        g.connections.push(Connection::new(1, 3, "a"));
        g.connections.push(Connection::new(1, 3, "b"));
        assert_eq!(validate(&g, &[]), vec![]);
    }

    #[test]
    fn refusal_code_no_end_node() {
        let mut g = Graph::new("graph_1", "No end");
        g.nodes = vec![node(1, Kind::Constant), node(2, Kind::Add)];
        g.connections = vec![Connection::new(1, 2, "a")];
        only(&g, RefusalCode::NoEndNode);
    }

    #[test]
    fn refusal_code_two_end_nodes() {
        let mut g = sensor_graph("graph_1");
        g.nodes
            .push(node(3, Kind::ResponseEnd).with_param("title", "x"));
        only(&g, RefusalCode::TwoEndNodes);
    }

    #[test]
    fn refusal_code_end_node_output() {
        let mut g = sensor_graph("graph_1");
        g.nodes.push(node(3, Kind::Not));
        g.connections.push(Connection::new(2, 3, "a"));
        only(&g, RefusalCode::EndNodeOutput);
    }

    #[test]
    fn refusal_code_pin_occupied() {
        let mut g = sensor_graph("graph_1");
        g.nodes.push(node(3, Kind::Constant));
        g.connections.push(Connection::new(3, 2, "a"));
        only(&g, RefusalCode::PinOccupied);
    }

    #[test]
    fn refusal_code_same_direction() {
        // Two output pins.
        let mut g = sensor_graph("graph_1");
        g.nodes.push(node(3, Kind::Constant));
        g.connections.push(Connection {
            from: crate::model::Endpoint {
                node: 3,
                pin: "out".into(),
            },
            to: crate::model::Endpoint {
                node: 1,
                pin: "out".into(),
            },
        });
        only(&g, RefusalCode::SameDirection);
        // Two input pins.
        let mut g = sensor_graph("graph_1");
        g.nodes.push(node(3, Kind::Add));
        g.nodes.push(node(4, Kind::Add));
        g.connections.push(Connection {
            from: crate::model::Endpoint {
                node: 3,
                pin: "a".into(),
            },
            to: crate::model::Endpoint {
                node: 4,
                pin: "a".into(),
            },
        });
        only(&g, RefusalCode::SameDirection);
    }

    #[test]
    fn refusal_code_same_node() {
        let mut g = sensor_graph("graph_1");
        g.nodes.push(node(3, Kind::Add));
        g.connections.push(Connection::new(3, 3, "a"));
        only(&g, RefusalCode::SameNode);
    }

    #[test]
    fn refusal_code_unknown_pin() {
        let mut g = sensor_graph("graph_1");
        g.connections[0].to.pin = "z".into();
        only(&g, RefusalCode::UnknownPin);
        // A node that does not exist.
        let mut g = sensor_graph("graph_1");
        g.connections.push(Connection::new(9, 2, "a"));
        assert!(codes(&validate(&g, &[])).contains(&RefusalCode::UnknownPin));
        // The comment node has no pins.
        let mut g = sensor_graph("graph_1");
        g.nodes.push(node(3, Kind::Comment));
        g.connections.push(Connection::new(3, 2, "a"));
        assert!(codes(&validate(&g, &[])).contains(&RefusalCode::UnknownPin));
    }

    #[test]
    fn refusal_code_loop() {
        let mut g = sensor_graph("graph_1");
        g.nodes.push(node(3, Kind::Add));
        g.nodes.push(node(4, Kind::Add));
        g.connections.push(Connection::new(3, 4, "a"));
        g.connections.push(Connection::new(4, 3, "a"));
        only(&g, RefusalCode::Loop);
        // The same chain without the way back is no loop.
        g.connections.pop();
        assert_eq!(validate(&g, &[]), vec![]);
    }

    fn source(id: u32, sensor: &str) -> Node {
        node(id, Kind::SensorSource).with_param("sensor", sensor)
    }

    #[test]
    fn refusal_code_loop_across_graphs() {
        // A reads the sensor of B.
        let mut a = sensor_graph("graph_1");
        a.sensor_id = Some("graph_1".into());
        a.nodes[0] = source(1, "graph_2");
        let mut b = sensor_graph("graph_2");
        b.sensor_id = Some("graph_2".into());
        assert_eq!(validate(&b, &[a.clone()]), vec![]);
        // The user applies B with a source on the sensor of A.
        b.nodes[0] = source(1, "graph_1");
        let refusals = validate(&b, &[a.clone()]);
        assert_eq!(codes(&refusals), vec![RefusalCode::LoopAcrossGraphs]);
        // A chain of three closes too.
        let mut c = sensor_graph("graph_3");
        c.sensor_id = Some("graph_3".into());
        c.nodes[0] = source(1, "graph_2");
        b.nodes[0] = source(1, "graph_3");
        assert_eq!(
            codes(&validate(&b, &[a, c])),
            vec![RefusalCode::LoopAcrossGraphs]
        );
    }

    #[test]
    fn graph_reads_its_own_sensor() {
        let mut g = sensor_graph("graph_1");
        g.sensor_id = Some("graph_1".into());
        g.nodes[0] = source(1, "graph_1");
        only(&g, RefusalCode::LoopAcrossGraphs);
    }

    #[test]
    fn refusal_code_limit_graphs() {
        let others: Vec<Graph> = (1..=64).map(|i| response_graph(&format!("g{i}"))).collect();
        let new = response_graph("new");
        assert_eq!(
            codes(&validate(&new, &others)),
            vec![RefusalCode::LimitGraphs]
        );
        // The 64th Graph is fine, and so is an edit of a stored one.
        assert_eq!(validate(&new, &others[..63]), vec![]);
        assert_eq!(validate(&others[0], &others), vec![]);
    }

    #[test]
    fn refusal_code_limit_nodes() {
        let mut g = sensor_graph("graph_1");
        while g.nodes.len() < 256 {
            let id = u32::try_from(g.nodes.len()).unwrap() + 1;
            g.nodes.push(node(id, Kind::Constant));
        }
        assert_eq!(validate(&g, &[]), vec![]);
        g.nodes.push(node(257, Kind::Constant));
        only(&g, RefusalCode::LimitNodes);
    }

    #[test]
    fn refusal_code_limit_name() {
        let mut g = sensor_graph("graph_1");
        g.name = "é".repeat(64);
        assert_eq!(validate(&g, &[]), vec![]);
        g.name = "é".repeat(65);
        only(&g, RefusalCode::LimitName);
    }

    fn window_graph(id: &str, windows: usize, span: u32) -> Graph {
        let mut g = sensor_graph(id);
        for i in 0..windows {
            let id = u32::try_from(i).unwrap() + 3;
            g.nodes
                .push(node(id, Kind::WindowAverage).with_param("span_s", span));
        }
        g
    }

    #[test]
    fn refusal_code_limit_samples_111_ok_112_refused() {
        // 111 nodes of 1800 s: 111 x 1801 = 199 911 samples.
        let ok = window_graph("graph_1", 111, 1800);
        assert_eq!(window_samples(&ok), 199_911);
        assert_eq!(validate(&ok, &[]), vec![]);
        // 112 nodes: 201 712 samples. Here the 112th is in a second Graph.
        let more = window_graph("graph_2", 1, 1800);
        assert_eq!(
            codes(&validate(&more, std::slice::from_ref(&ok))),
            vec![RefusalCode::LimitSamples]
        );
        let all_in_one = window_graph("graph_3", 112, 1800);
        assert_eq!(window_samples(&all_in_one), 201_712);
        only(&all_in_one, RefusalCode::LimitSamples);
        // 100 nodes of the default span of 10 s are 1 100 samples.
        let default_span = window_graph("graph_4", 100, 10);
        assert_eq!(window_samples(&default_span), 1_100);
        assert_eq!(validate(&default_span, &[]), vec![]);
    }

    #[test]
    fn refusal_code_limit_graph_sensors() {
        let others: Vec<Graph> = (1..=64)
            .map(|i| {
                let mut g = sensor_graph(&format!("g{i}"));
                g.sensor_id = Some(format!("graph_{i}"));
                g
            })
            .collect();
        let new = sensor_graph("new");
        let refusals = codes(&validate(&new, &others));
        assert!(
            refusals.contains(&RefusalCode::LimitGraphSensors),
            "{refusals:?}"
        );
        assert_eq!(validate(&new, &others[..63]), vec![]);
        // A Graph with a response end node makes no Graph sensor.
        let response = response_graph("r");
        assert!(!codes(&validate(&response, &others)).contains(&RefusalCode::LimitGraphSensors));
    }

    #[test]
    fn refusal_code_duplicate_label() {
        let mut g = sensor_graph("graph_1");
        g.nodes[0].label = Some("water".into());
        g.nodes.push(node(3, Kind::Constant).with_label("water"));
        only(&g, RefusalCode::DuplicateLabel);
        g.nodes[2].label = Some("pump".into());
        assert_eq!(validate(&g, &[]), vec![]);
    }

    #[test]
    fn label_with_a_space_refused() {
        let mut g = sensor_graph("graph_1");
        g.nodes[0].label = Some("hot water".into());
        bad_parameter(&g, "label");
        for bad in ["", "é", "a-b", &"x".repeat(33)] {
            g.nodes[0].label = Some(bad.to_owned());
            bad_parameter(&g, "label");
        }
        for good in ["water", "Water_2", &"x".repeat(32), "_"] {
            g.nodes[0].label = Some(good.to_owned());
            assert_eq!(validate(&g, &[]), vec![], "{good}");
        }
    }

    #[test]
    fn refusal_code_duplicate_table_input() {
        let mut g = sensor_graph("graph_1");
        g.nodes
            .push(node(3, Kind::Table).with_param("pairs", json!([[20, 30], [40, 80], [20, 50]])));
        only(&g, RefusalCode::DuplicateTableInput);
        g.nodes[2] = node(3, Kind::Table).with_param("pairs", json!([[40, 80], [20, 30]]));
        // The order of the pairs does not matter: they are sorted when they run.
        assert_eq!(validate(&g, &[]), vec![]);
    }

    #[test]
    fn bad_parameter_names_the_parameter() {
        let mut g = sensor_graph("graph_1");
        g.nodes
            .push(node(3, Kind::Round).with_param("decimals", 12));
        bad_parameter(&g, "decimals");
        g.nodes[2] = node(3, Kind::Random)
            .with_param("min", 5)
            .with_param("max", 1);
        bad_parameter(&g, "max");
        g.nodes[2] = node(3, Kind::Limiter)
            .with_param("min", 5)
            .with_param("max", 1);
        bad_parameter(&g, "max");
        g.nodes[2] = node(3, Kind::Limiter)
            .with_param("min", 5)
            .with_param("max", 5);
        assert_eq!(validate(&g, &[]), vec![]);
        g.nodes[2] = node(3, Kind::LowPass).with_param("damping", 100);
        bad_parameter(&g, "damping");
        g.nodes[2] = node(3, Kind::SensorSource).with_param("sensor", 7);
        bad_parameter(&g, "sensor");
        g.nodes[2] = node(3, Kind::Process);
        bad_parameter(&g, "name");
        g.nodes[1].params.insert("unit".into(), json!("bar"));
        let refusals = validate(&g, &[]);
        let names: Vec<_> = refusals
            .iter()
            .filter_map(|r| r.parameter.as_deref())
            .collect();
        assert_eq!(names, ["unit", "name"], "in the order of the nodes");
        // A duplicate or zero node id is a bad `id`.
        let mut g = sensor_graph("graph_1");
        g.nodes[0].id = 2;
        assert!(
            validate(&g, &[])
                .iter()
                .any(|r| r.parameter.as_deref() == Some("id"))
        );
    }

    fn command_graph(path: &str, args: Vec<String>) -> Graph {
        let mut g = sensor_graph("graph_1");
        let mut end = node(2, Kind::ResponseEnd);
        end.params = default_params_for(Kind::ResponseEnd, ResponseKind::Command);
        end.params.insert("kind".into(), "command".into());
        end.params.insert("path".into(), path.into());
        end.params.insert("args".into(), json!(args));
        g.nodes[1] = end;
        g
    }

    #[test]
    fn relative_command_path_refused() {
        bad_parameter(&command_graph("notify-send", vec![]), "path");
        bad_parameter(&command_graph("", vec![]), "path");
        bad_parameter(&command_graph("./run.sh", vec![]), "path");
        assert_eq!(
            validate(
                &command_graph("/usr/bin/notify-send", vec!["Hot".into()]),
                &[]
            ),
            vec![]
        );
    }

    #[test]
    fn command_33_arguments_refused() {
        bad_parameter(&command_graph("/bin/true", vec!["a".into(); 33]), "args");
        assert_eq!(
            validate(&command_graph("/bin/true", vec!["a".into(); 32]), &[]),
            vec![]
        );
        bad_parameter(&command_graph("/bin/true", vec!["a".repeat(1025)]), "args");
    }

    fn response_with(kind: ResponseKind, name: &str, value: serde_json::Value) -> Graph {
        let mut g = sensor_graph("graph_1");
        let mut end = node(2, Kind::ResponseEnd);
        end.params = default_params_for(Kind::ResponseEnd, kind);
        end.params.insert("kind".into(), kind.as_str().into());
        // Fill the fields that the validator wants non-empty, so that only `name` is wrong.
        for (key, fill) in [
            ("title", "t"),
            ("sound", "s"),
            ("path", "/bin/true"),
            ("to", "a@b.org"),
            ("topic", "alerts"),
        ] {
            if end.params.contains_key(key) {
                end.params.insert(key.into(), fill.into());
            }
        }
        end.params.insert(name.into(), value);
        g.nodes[1] = end;
        g
    }

    #[test]
    fn topic_wildcard_refused() {
        for topic in ["alerts/#", "alerts/+", "/alerts", "alerts/", "a//b", ""] {
            bad_parameter(
                &response_with(ResponseKind::Mqtt, "topic", json!(topic)),
                "topic",
            );
        }
        assert_eq!(
            validate(
                &response_with(ResponseKind::Mqtt, "topic", json!("alerts/water")),
                &[]
            ),
            vec![]
        );
    }

    #[test]
    fn countdown_below_10_refused() {
        for kind in [
            ResponseKind::Poweroff,
            ResponseKind::Reboot,
            ResponseKind::Suspend,
        ] {
            bad_parameter(&response_with(kind, "countdown_s", json!(5)), "countdown_s");
            bad_parameter(
                &response_with(kind, "countdown_s", json!(3601)),
                "countdown_s",
            );
            assert_eq!(
                validate(&response_with(kind, "countdown_s", json!(10)), &[]),
                vec![]
            );
        }
    }

    #[test]
    fn fraction_hold_time_refused() {
        bad_parameter(
            &response_with(ResponseKind::Notification, "hold_s", json!(2.5)),
            "hold_s",
        );
        bad_parameter(
            &response_with(ResponseKind::Notification, "hold_s", json!(3601)),
            "hold_s",
        );
        bad_parameter(
            &response_with(ResponseKind::Notification, "repeat_s", json!(0)),
            "repeat_s",
        );
        let ok = response_with(ResponseKind::Notification, "hold_s", json!(3600));
        assert_eq!(validate(&ok, &[]), vec![]);
        // The delay of a node is a whole number of seconds too.
        let mut g = sensor_graph("graph_1");
        g.nodes
            .push(node(3, Kind::RiseDelay).with_param("delay_s", 0.5));
        bad_parameter(&g, "delay_s");
    }

    #[test]
    fn address_without_at_refused() {
        for to in ["owner.example.org", "a@b@c", "@b.org", "a@", ""] {
            bad_parameter(&response_with(ResponseKind::Email, "to", json!(to)), "to");
        }
        assert_eq!(
            validate(
                &response_with(ResponseKind::Email, "to", json!("owner@example.org")),
                &[]
            ),
            vec![]
        );
    }

    #[test]
    fn comment_too_wide_refused() {
        let mut g = sensor_graph("graph_1");
        g.nodes
            .push(node(3, Kind::Comment).with_param("width", 900));
        bad_parameter(&g, "width");
        g.nodes[2] = node(3, Kind::Comment).with_param("height", 19);
        bad_parameter(&g, "height");
        g.nodes[2] = node(3, Kind::Comment).with_param("text", "x".repeat(1025));
        bad_parameter(&g, "text");
        g.nodes[2] = node(3, Kind::Comment)
            .with_param("width", 800)
            .with_param("height", 20)
            .with_param("text", "x".repeat(1024));
        assert_eq!(validate(&g, &[]), vec![]);
    }

    #[test]
    fn bad_sensor_id_on_response_graph() {
        let mut g = response_graph("graph_1");
        g.sensor_id = Some("graph_1".into());
        only(&g, RefusalCode::BadSensorId);
        // The form of the id and a second holder are refused too.
        for bad in ["graph_0", "sensor_1", "graph_01", ""] {
            let mut g = sensor_graph("graph_1");
            g.sensor_id = Some(bad.into());
            only(&g, RefusalCode::BadSensorId);
        }
        let mut a = sensor_graph("graph_1");
        a.sensor_id = Some("graph_3".into());
        let mut b = sensor_graph("graph_2");
        b.sensor_id = Some("graph_3".into());
        assert_eq!(
            codes(&validate(&b, &[a.clone()])),
            vec![RefusalCode::BadSensorId]
        );
        b.sensor_id = Some("graph_4".into());
        assert_eq!(validate(&b, &[a]), vec![]);
        let mut ok = sensor_graph("graph_1");
        ok.sensor_id = Some("graph_12".into());
        assert_eq!(validate(&ok, &[]), vec![]);
    }

    #[test]
    fn all_refusals_returned_in_list_order() {
        // Two end nodes and a random node with min 5 and max 1.
        let mut g = sensor_graph("graph_1");
        g.nodes
            .push(node(3, Kind::ResponseEnd).with_param("title", "x"));
        g.nodes.push(
            node(4, Kind::Random)
                .with_param("min", 5)
                .with_param("max", 1),
        );
        let refusals = validate(&g, &[]);
        assert_eq!(
            codes(&refusals),
            vec![RefusalCode::TwoEndNodes, RefusalCode::BadParameter]
        );
        assert_eq!(refusals[1].parameter.as_deref(), Some("max"));
        // Many codes at once come out in the order of the list.
        let mut g = Graph::new("graph_1", &"n".repeat(70));
        g.sensor_id = Some("bad".into());
        g.nodes = vec![
            node(1, Kind::Constant).with_label("a b"),
            node(2, Kind::Add),
            node(3, Kind::Add),
        ];
        g.connections = vec![Connection::new(2, 3, "a"), Connection::new(3, 2, "a")];
        let found = codes(&validate(&g, &[]));
        let mut sorted = found.clone();
        sorted.sort();
        assert_eq!(found, sorted);
        assert_eq!(
            found,
            vec![
                RefusalCode::NoEndNode,
                RefusalCode::Loop,
                RefusalCode::LimitName,
                RefusalCode::BadParameter,
                RefusalCode::BadSensorId,
            ]
        );
        // The code names are the names of the spec.
        let names: Vec<_> = found.iter().map(|c| c.as_str()).collect();
        assert_eq!(
            names,
            [
                "no-end-node",
                "loop",
                "limit-name",
                "bad-parameter",
                "bad-sensor-id"
            ]
        );
    }
}
