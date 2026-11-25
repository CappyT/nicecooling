//! The evaluator. One pass evaluates every enabled Graph once: the Graphs in the order of their
//! Graph sensors, and the nodes of a Graph from the sources to the end node. A pin carries a number
//! or no value, and the no-value rule of the spec holds for every kind.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::time::Duration;

use nicecooling_core::rules::process::{MatchMode, MatchOn, ProcessList, ProcessRule};

use crate::funcs::{
    RoundMode, all_values, binary, finite, has_value, is_true, limiter, logic, random_value,
    round_to, source_value, switch, unary,
};
use crate::generator::{Generator, Shape};
use crate::model::{Connection, Graph, Kind, Node};
use crate::state::{Counter, FallDelay, Memory, Persistent, RiseDelay, Trigger};
use crate::table::Table;
use crate::window::{Filter, Stat};

/// What a Graph reads from the outside once per pass.
pub trait Sources {
    /// The value of a sensor, or `None` while it has none. An id that is no sensor of this machine
    /// has no value too.
    fn sensor(&self, id: &str) -> Option<f64>;

    /// Whether the Alarm or Warning `event` of `source` is active. `None` while the daemon is
    /// offline.
    fn alarm_active(&self, event: &str, source: &str) -> Option<bool>;

    /// The scan of `/proc`.
    fn processes(&self) -> &ProcessList;
}

/// The result of the end node of a Graph.
#[derive(Clone, Debug, PartialEq)]
pub enum EndValue {
    /// The Graph has no end node.
    None,
    /// The value of the Graph sensor, or no value.
    Sensor(Option<f64>),
    /// The input of the response end node: true or false, or no value. A response end node with no
    /// connection is false.
    Response(Option<bool>),
}

/// The result of one Graph in one pass.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphPass {
    pub graph_id: String,
    pub sensor_id: Option<String>,
    /// False for a disabled Graph.
    pub evaluated: bool,
    /// The value of each node. The comment node has none.
    pub values: BTreeMap<u32, Option<f64>>,
    pub end: EndValue,
}

/// The result of one pass, in the order of the Graphs that were given.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pass {
    pub graphs: Vec<GraphPass>,
}

impl Pass {
    pub fn graph(&self, id: &str) -> Option<&GraphPass> {
        self.graphs.iter().find(|g| g.graph_id == id)
    }

    /// The Graph sensors with their values for the call to the daemon. A Graph that is disabled or
    /// that gives no value has the entry `None`.
    pub fn sensor_values(&self) -> Vec<(String, Option<f64>)> {
        self.graphs
            .iter()
            .filter_map(|g| {
                let id = g.sensor_id.clone()?;
                let value = match g.end {
                    EndValue::Sensor(v) if g.evaluated => v,
                    _ => None,
                };
                Some((id, value))
            })
            .collect()
    }
}

/// The state of one node.
#[derive(Clone, Debug, PartialEq)]
enum NodeState {
    Memory(Memory),
    Persistent(Persistent),
    Trigger(Trigger),
    RiseDelay(RiseDelay),
    FallDelay(FallDelay),
    Counter(Counter),
    Filter(Filter),
    Generator(Generator),
}

impl NodeState {
    fn new(kind: Kind) -> Option<Self> {
        Some(match kind {
            Kind::Memory => Self::Memory(Memory::default()),
            Kind::PersistentStorage => Self::Persistent(Persistent::default()),
            Kind::Trigger => Self::Trigger(Trigger::default()),
            Kind::RiseDelay => Self::RiseDelay(RiseDelay::default()),
            Kind::FallDelay => Self::FallDelay(FallDelay::default()),
            Kind::Counter => Self::Counter(Counter::default()),
            Kind::Peak
            | Kind::WindowMinimum
            | Kind::WindowAverage
            | Kind::LowPass
            | Kind::Trend => Self::Filter(Filter::default()),
            Kind::LinearGen
            | Kind::SawtoothGen
            | Kind::TriangleGen
            | Kind::SineGen
            | Kind::RectangleGen => Self::Generator(Generator::default()),
            _ => return None,
        })
    }

    /// Whether this state belongs to a node of `kind`.
    fn fits(&self, kind: Kind) -> bool {
        Self::new(kind).is_some_and(|s| std::mem::discriminant(&s) == std::mem::discriminant(self))
    }
}

/// The key of a stored value in `graph-state.json`.
pub fn value_key(graph_id: &str, node_id: u32) -> String {
    format!("{graph_id}/{node_id}")
}

/// The evaluator with the state of all nodes of all Graphs.
pub struct Evaluator {
    rng: fastrand::Rng,
    states: HashMap<(String, u32), NodeState>,
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl Evaluator {
    pub fn new() -> Self {
        Self {
            rng: fastrand::Rng::new(),
            states: HashMap::new(),
        }
    }

    /// An evaluator whose random nodes give the same numbers for the same seed.
    pub fn with_seed(seed: u64) -> Self {
        Self {
            rng: fastrand::Rng::with_seed(seed),
            states: HashMap::new(),
        }
    }

    /// Apply of a Graph: the state of its nodes starts again, except the stored values of the
    /// persistent storage nodes that keep their id and kind. The state of a node that is gone is
    /// dropped.
    pub fn apply(&mut self, graph: &Graph) {
        self.states.retain(|(graph_id, node_id), state| {
            graph_id != &graph.id
                || (matches!(state, NodeState::Persistent(_))
                    && graph
                        .node(*node_id)
                        .is_some_and(|n| n.kind == Kind::PersistentStorage))
        });
    }

    /// Drops the state of a Graph that was deleted.
    pub fn remove_graph(&mut self, graph_id: &str) {
        self.states.retain(|(id, _), _| id != graph_id);
    }

    /// The values of the persistent storage nodes, keyed `<graph id>/<node id>`.
    pub fn stored_values(&self) -> BTreeMap<String, f64> {
        self.states
            .iter()
            .filter_map(|((graph, node), state)| match state {
                NodeState::Persistent(p) => Some((value_key(graph, *node), p.value())),
                _ => None,
            })
            .collect()
    }

    /// Sets the value of a persistent storage node, as it was stored.
    pub fn restore_value(&mut self, graph_id: &str, node_id: u32, value: f64) {
        self.states.insert(
            (graph_id.to_owned(), node_id),
            NodeState::Persistent(Persistent::restore(value)),
        );
    }

    /// One pass over the Graphs. A disabled Graph is not evaluated and keeps its state.
    pub fn evaluate(&mut self, graphs: &[Graph], sources: &dyn Sources, now: Duration) -> Pass {
        // The value of each Graph sensor in this pass. The Graph sensor of a disabled Graph has no
        // value.
        let mut sensors: HashMap<String, Option<f64>> = graphs
            .iter()
            .filter(|g| !g.enabled)
            .filter_map(|g| g.sensor_id.clone())
            .map(|id| (id, None))
            .collect();
        let mut results: Vec<Option<GraphPass>> = vec![None; graphs.len()];
        for index in graph_order(graphs) {
            let graph = &graphs[index];
            // The validator refuses two Graphs with one id. If the set has them anyway, the
            // later one gets a state of its own and does not write into the state of the first.
            let earlier = graphs[..index].iter().filter(|g| g.id == graph.id).count();
            let pass = if earlier == 0 {
                self.evaluate_graph(graph, sources, &sensors, now)
            } else {
                let renamed = Graph {
                    id: format!("{}#{earlier}", graph.id),
                    ..graph.clone()
                };
                GraphPass {
                    graph_id: graph.id.clone(),
                    ..self.evaluate_graph(&renamed, sources, &sensors, now)
                }
            };
            if let (Some(id), EndValue::Sensor(value)) = (&graph.sensor_id, &pass.end) {
                sensors.insert(id.clone(), *value);
            }
            results[index] = Some(pass);
        }
        Pass {
            graphs: graphs
                .iter()
                .zip(results)
                .map(|(graph, result)| result.unwrap_or_else(|| skipped(graph)))
                .collect(),
        }
    }

    fn evaluate_graph(
        &mut self,
        graph: &Graph,
        sources: &dyn Sources,
        sensors: &HashMap<String, Option<f64>>,
        now: Duration,
    ) -> GraphPass {
        let mut values: BTreeMap<u32, Option<f64>> = BTreeMap::new();
        let mut end = EndValue::None;
        for id in node_order(graph) {
            let Some(node) = graph.node(id) else {
                continue;
            };
            let inputs = resolve_inputs(graph, node, &values);
            let connected = graph.connection_to(node.id, "a").is_some();
            let value = match node.kind {
                Kind::SensorEnd => {
                    end = EndValue::Sensor(inputs[0]);
                    inputs[0]
                }
                Kind::ResponseEnd => {
                    end = EndValue::Response(if connected {
                        inputs[0].map(is_true)
                    } else {
                        Some(false)
                    });
                    inputs[0]
                }
                _ => self.node_value(graph, node, &inputs, sources, sensors, now),
            };
            values.insert(node.id, value.and_then(finite));
        }
        GraphPass {
            graph_id: graph.id.clone(),
            sensor_id: graph.sensor_id.clone(),
            evaluated: true,
            values,
            end,
        }
    }

    fn state(&mut self, graph: &Graph, node: &Node) -> Option<&mut NodeState> {
        let key = (graph.id.clone(), node.id);
        if self.states.get(&key).is_some_and(|s| !s.fits(node.kind)) {
            self.states.remove(&key);
        }
        if !self.states.contains_key(&key) {
            self.states.insert(key.clone(), NodeState::new(node.kind)?);
        }
        self.states.get_mut(&key)
    }

    fn node_value(
        &mut self,
        graph: &Graph,
        node: &Node,
        inputs: &[Option<f64>],
        sources: &dyn Sources,
        sensors: &HashMap<String, Option<f64>>,
        now: Duration,
    ) -> Option<f64> {
        let input = |i: usize| inputs.get(i).copied().flatten();
        match node.kind {
            Kind::SensorSource => {
                let id = node.text("sensor");
                let reading = match sensors.get(&id) {
                    Some(v) => *v,
                    None => sources.sensor(&id),
                };
                reading.and_then(finite).map(source_value)
            }
            Kind::AlarmEventSource => sources
                .alarm_active(&node.text("event"), &node.text("source"))
                .map(logic),
            Kind::Constant => Some(node.number("value")),
            Kind::Random => {
                let (min, max) = (node.number("min"), node.number("max"));
                (min <= max).then(|| random_value(&mut self.rng, min, max))
            }
            Kind::Process => {
                let name = node.text("name");
                let rule = ProcessRule {
                    names: vec![name.clone()],
                    match_mode: MatchMode::Any,
                    match_on: MatchOn::Name,
                };
                Some(logic(
                    !name.is_empty() && rule.name_matches(&name, sources.processes()),
                ))
            }
            Kind::Add
            | Kind::Subtract
            | Kind::Multiply
            | Kind::Divide
            | Kind::Minimum
            | Kind::Maximum
            | Kind::Average
            | Kind::Logarithm
            | Kind::Power
            | Kind::Less
            | Kind::LessThanOrEqual
            | Kind::Equal
            | Kind::GreaterThanOrEqual
            | Kind::Greater
            | Kind::Different
            | Kind::And
            | Kind::Or => {
                let [a, b] = all_values([input(0), input(1)])?;
                binary(node.kind, a, b)
            }
            Kind::Sine | Kind::Cosine | Kind::Tangent | Kind::SquareRoot | Kind::Not => {
                unary(node.kind, input(0)?)
            }
            Kind::Round => {
                let a = input(0)?;
                let mode = RoundMode::parse(&node.text("mode")).unwrap_or(RoundMode::Nearest);
                let decimals = node.number("decimals").clamp(0.0, 8.0) as u32;
                Some(round_to(a, decimals, mode))
            }
            Kind::Limiter => {
                let a = input(0)?;
                let (min, max) = (node.number("min"), node.number("max"));
                (min <= max).then(|| limiter(a, min, max))
            }
            Kind::Table => {
                let a = input(0)?;
                Table::from_node(node)?.output(a)
            }
            Kind::Switch => {
                let [s, a, b] = all_values([input(0), input(1), input(2)])?;
                Some(switch(s, a, b))
            }
            Kind::HasValue => Some(has_value(input(0))),
            Kind::Comment | Kind::SensorEnd | Kind::ResponseEnd => None,
            Kind::Memory
            | Kind::PersistentStorage
            | Kind::Trigger
            | Kind::RiseDelay
            | Kind::FallDelay
            | Kind::Counter
            | Kind::Peak
            | Kind::WindowMinimum
            | Kind::WindowAverage
            | Kind::LowPass
            | Kind::Trend
            | Kind::LinearGen
            | Kind::SawtoothGen
            | Kind::TriangleGen
            | Kind::SineGen
            | Kind::RectangleGen => {
                let params = StateParams::of(node);
                match self.state(graph, node)? {
                    NodeState::Memory(m) => m.step(input(0), input(1)),
                    NodeState::Persistent(p) => p.step(input(0), input(1)),
                    NodeState::Trigger(t) => t.step(input(0)),
                    NodeState::RiseDelay(d) => d.step(input(0), params.delay, now),
                    NodeState::FallDelay(d) => d.step(input(0), params.delay, now),
                    NodeState::Counter(c) => c.step(input(0), input(1), input(2), input(3), now),
                    NodeState::Filter(f) => {
                        f.step(params.stat(node.kind), input(0), params.span_s, now)
                    }
                    NodeState::Generator(g) => {
                        g.step(shape_of(node.kind)?, input(0), input(1), input(2), now)
                    }
                }
            }
        }
    }
}

/// The parameters that the nodes with a state read.
struct StateParams {
    delay: Duration,
    span_s: u32,
    damping: f64,
}

impl StateParams {
    fn of(node: &Node) -> Self {
        Self {
            delay: Duration::from_secs(node.number("delay_s").clamp(0.0, 3600.0) as u64),
            span_s: node.number("span_s").clamp(1.0, 1800.0) as u32,
            damping: node.number("damping").clamp(0.0, 99.99),
        }
    }

    fn stat(&self, kind: Kind) -> Stat {
        match kind {
            Kind::Peak => Stat::Peak,
            Kind::WindowMinimum => Stat::Minimum,
            Kind::WindowAverage => Stat::Average,
            Kind::Trend => Stat::Trend,
            _ => Stat::LowPass {
                damping: self.damping,
            },
        }
    }
}

fn shape_of(kind: Kind) -> Option<Shape> {
    Some(match kind {
        Kind::LinearGen => Shape::Linear,
        Kind::SawtoothGen => Shape::Sawtooth,
        Kind::TriangleGen => Shape::Triangle,
        Kind::SineGen => Shape::Sine,
        Kind::RectangleGen => Shape::Rectangle,
        _ => return None,
    })
}

fn skipped(graph: &Graph) -> GraphPass {
    GraphPass {
        graph_id: graph.id.clone(),
        sensor_id: graph.sensor_id.clone(),
        evaluated: false,
        values: BTreeMap::new(),
        end: if graph.sensor_end().is_some() {
            EndValue::Sensor(None)
        } else {
            EndValue::None
        },
    }
}

/// The value of each input pin of a node: the value of the connected output pin, or the default
/// of the pin while it has no connection.
fn resolve_inputs(
    graph: &Graph,
    node: &Node,
    values: &BTreeMap<u32, Option<f64>>,
) -> Vec<Option<f64>> {
    node.kind
        .inputs()
        .iter()
        .map(|pin| match graph.connection_to(node.id, pin.name) {
            Some(Connection { from, .. }) => values.get(&from.node).copied().flatten(),
            None => pin.default,
        })
        .collect()
}

/// The ids of the nodes in the order of evaluation: a node comes after every node that feeds one
/// of its input pins, and the nodes that need no input come first. The comment node is left out.
/// A node on a loop is left out too.
pub fn node_order(graph: &Graph) -> Vec<u32> {
    let nodes: Vec<&Node> = graph
        .nodes
        .iter()
        .filter(|n| n.kind != Kind::Comment)
        .collect();
    let ids: BTreeSet<u32> = nodes.iter().map(|n| n.id).collect();
    let mut indegree: BTreeMap<u32, usize> = ids.iter().map(|id| (*id, 0)).collect();
    let mut next: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for c in &graph.connections {
        let to_is_input = graph
            .node(c.to.node)
            .is_some_and(|n| n.kind.input(&c.to.pin).is_some());
        if c.from.node != c.to.node
            && to_is_input
            && ids.contains(&c.from.node)
            && ids.contains(&c.to.node)
        {
            next.entry(c.from.node).or_default().push(c.to.node);
            *indegree.entry(c.to.node).or_default() += 1;
        }
    }
    // The ready nodes come out in the order of their ids, the sources first.
    let mut ready: BTreeSet<u32> = indegree
        .iter()
        .filter(|(_, d)| **d == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut order = Vec::with_capacity(ids.len());
    while let Some(id) = ready.pop_first() {
        order.push(id);
        for to in next.get(&id).into_iter().flatten() {
            if let Some(d) = indegree.get_mut(to) {
                *d -= 1;
                if *d == 0 {
                    ready.insert(*to);
                }
            }
        }
    }
    order
}

/// The indexes of the enabled Graphs in the order of evaluation: a Graph comes after every Graph
/// whose Graph sensor it reads. Graphs on a loop come last, in the order that they were given.
fn graph_order(graphs: &[Graph]) -> Vec<usize> {
    let holder: HashMap<&str, usize> = graphs
        .iter()
        .enumerate()
        .filter(|(_, g)| g.enabled)
        .filter_map(|(i, g)| g.sensor_id.as_deref().map(|id| (id, i)))
        .collect();
    let enabled: Vec<usize> = (0..graphs.len()).filter(|i| graphs[*i].enabled).collect();
    let mut waiting: BTreeMap<usize, BTreeSet<usize>> = enabled
        .iter()
        .map(|i| {
            let deps = graphs[*i]
                .source_sensors()
                .filter_map(|s| holder.get(s.as_str()).copied())
                .filter(|j| j != i)
                .collect();
            (*i, deps)
        })
        .collect();
    let mut order = Vec::with_capacity(enabled.len());
    while let Some(&index) = waiting
        .iter()
        .find(|(_, deps)| deps.is_empty())
        .map(|(i, _)| i)
    {
        waiting.remove(&index);
        order.push(index);
        for deps in waiting.values_mut() {
            deps.remove(&index);
        }
    }
    order.extend(waiting.into_keys());
    order
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use nicecooling_core::rules::process::ProcessEntry;

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

    fn node(id: u32, kind: Kind) -> Node {
        Node::new(id, kind)
    }

    fn source(id: u32, sensor: &str) -> Node {
        node(id, Kind::SensorSource).with_param("sensor", sensor)
    }

    fn constant(id: u32, value: f64) -> Node {
        node(id, Kind::Constant).with_param("value", value)
    }

    fn graph(id: &str, nodes: Vec<Node>, connections: Vec<Connection>) -> Graph {
        let mut g = Graph::new(id, id);
        g.nodes = nodes;
        g.connections = connections;
        g
    }

    fn pass_one(graph: &Graph, fake: &Fake) -> GraphPass {
        let mut evaluator = Evaluator::with_seed(1);
        evaluator
            .evaluate(std::slice::from_ref(graph), fake, Duration::ZERO)
            .graphs
            .remove(0)
    }

    #[test]
    fn missing_input_gives_no_value_down_the_chain() {
        // A source with no value into A of an addition, the constant 5 into B, then a chain.
        let g = graph(
            "g",
            vec![
                source(1, "dead"),
                constant(2, 5.0),
                node(3, Kind::Add),
                node(4, Kind::Multiply),
                node(5, Kind::SensorEnd),
            ],
            vec![
                Connection::new(1, 3, "a"),
                Connection::new(2, 3, "b"),
                Connection::new(3, 4, "a"),
                Connection::new(2, 4, "b"),
                Connection::new(4, 5, "a"),
            ],
        );
        let pass = pass_one(&g, &Fake::default());
        assert_eq!(pass.values[&1], None);
        assert_eq!(pass.values[&2], Some(5.0));
        assert_eq!(pass.values[&3], None);
        assert_eq!(pass.values[&4], None);
        assert_eq!(pass.end, EndValue::Sensor(None));
        // With a value, the chain gives (7 + 5) x 5.
        let pass = pass_one(&g, &Fake::default().with("dead", 7.0));
        assert_eq!(pass.values[&3], Some(12.0));
        assert_eq!(pass.end, EndValue::Sensor(Some(60.0)));
    }

    #[test]
    fn state_kept_over_a_gap() {
        // A memory node holds 7. Its input S loses its value for 3 ticks and returns with 0.
        let g = graph(
            "g",
            vec![
                constant(1, 7.0),
                source(2, "s"),
                node(3, Kind::Memory),
                node(4, Kind::SensorEnd),
            ],
            vec![
                Connection::new(1, 3, "a"),
                Connection::new(2, 3, "s"),
                Connection::new(3, 4, "a"),
            ],
        );
        let mut evaluator = Evaluator::with_seed(1);
        let mut out = Vec::new();
        let readings = [Some(1.0), None, None, None, Some(0.0)];
        for (t, reading) in readings.into_iter().enumerate() {
            let mut fake = Fake::default();
            if let Some(r) = reading {
                fake = fake.with("s", r);
            }
            let pass = evaluator.evaluate(
                std::slice::from_ref(&g),
                &fake,
                Duration::from_secs(t as u64),
            );
            out.push(pass.graphs[0].values[&3]);
        }
        assert_eq!(out, [Some(7.0), None, None, None, Some(7.0)]);
    }

    #[test]
    fn unconnected_defaults_log_power_add() {
        let g = graph(
            "g",
            vec![
                constant(1, 1.0),
                node(2, Kind::Logarithm),
                constant(3, 3.0),
                node(4, Kind::Power),
                constant(5, 5.0),
                node(6, Kind::Add),
                node(7, Kind::SensorEnd),
                node(8, Kind::Multiply),
                node(9, Kind::HasValue),
            ],
            vec![
                Connection::new(1, 2, "a"),
                Connection::new(3, 4, "a"),
                Connection::new(5, 6, "a"),
                Connection::new(6, 7, "a"),
            ],
        );
        let pass = pass_one(&g, &Fake::default());
        // The natural logarithm of 1, 3 squared, and 5 + 0.
        assert_eq!(pass.values[&2], Some(0.0));
        assert_eq!(pass.values[&4], Some(9.0));
        assert_eq!(pass.values[&6], Some(5.0));
        // Two unconnected pins of a multiplication are 0 x 0.
        assert_eq!(pass.values[&8], Some(0.0));
        // The node "has value" with no connection gives 0.
        assert_eq!(pass.values[&9], Some(0.0));
        assert_eq!(pass.end, EndValue::Sensor(Some(5.0)));
        // An end node with no connection has no default.
        let lone = graph("g", vec![node(1, Kind::SensorEnd)], vec![]);
        assert_eq!(
            pass_one(&lone, &Fake::default()).end,
            EndValue::Sensor(None)
        );
        let lone = graph("g", vec![node(1, Kind::ResponseEnd)], vec![]);
        assert_eq!(
            pass_one(&lone, &Fake::default()).end,
            EndValue::Response(Some(false))
        );
    }

    #[test]
    fn sources_come_first() {
        // The nodes are listed from the end to the sources.
        let g = graph(
            "g",
            vec![
                node(9, Kind::SensorEnd),
                node(8, Kind::Add),
                node(7, Kind::Multiply),
                constant(6, 1.0),
                source(5, "s"),
                node(10, Kind::Comment),
            ],
            vec![
                Connection::new(7, 8, "a"),
                Connection::new(8, 9, "a"),
                Connection::new(6, 7, "a"),
                Connection::new(5, 7, "b"),
            ],
        );
        let order = node_order(&g);
        assert_eq!(order, [5, 6, 7, 8, 9]);
        let pos = |id| order.iter().position(|n| *n == id).unwrap();
        assert!(pos(5) < pos(7) && pos(6) < pos(7) && pos(7) < pos(8) && pos(8) < pos(9));
        // A node on a loop is left out and does not stop the others.
        let mut looped = g.clone();
        looped.connections.push(Connection::new(8, 7, "b"));
        assert!(!node_order(&looped).contains(&8));
        assert!(node_order(&looped).contains(&5));
    }

    fn sensor_graph(id: &str, sensor_id: &str, reads: &str, add: f64) -> Graph {
        let mut g = graph(
            id,
            vec![
                source(1, reads),
                constant(2, add),
                node(3, Kind::Add),
                node(4, Kind::SensorEnd),
            ],
            vec![
                Connection::new(1, 3, "a"),
                Connection::new(2, 3, "b"),
                Connection::new(3, 4, "a"),
            ],
        );
        g.sensor_id = Some(sensor_id.to_owned());
        g
    }

    #[test]
    fn graph_b_runs_after_graph_a_in_the_same_pass() {
        // B reads the Graph sensor of A, and B comes first in the list.
        let b = sensor_graph("b", "graph_2", "graph_1", 100.0);
        let a = sensor_graph("a", "graph_1", "base", 1.0);
        let fake = Fake::default().with("base", 10.0);
        let mut evaluator = Evaluator::with_seed(1);
        let pass = evaluator.evaluate(&[b, a], &fake, Duration::ZERO);
        // The result stays in the order of the list.
        assert_eq!(pass.graphs[0].graph_id, "b");
        assert_eq!(pass.graphs[1].graph_id, "a");
        assert_eq!(pass.graphs[1].end, EndValue::Sensor(Some(11.0)));
        assert_eq!(pass.graphs[0].end, EndValue::Sensor(Some(111.0)));
        assert_eq!(
            pass.sensor_values(),
            vec![
                ("graph_2".to_owned(), Some(111.0)),
                ("graph_1".to_owned(), Some(11.0))
            ]
        );
    }

    #[test]
    fn disabled_graph_is_not_evaluated() {
        let mut a = sensor_graph("a", "graph_1", "base", 1.0);
        a.nodes.push(node(5, Kind::Counter));
        a.enabled = false;
        // B reads the sensor of the disabled Graph: no value.
        let b = sensor_graph("b", "graph_2", "graph_1", 100.0);
        let fake = Fake::default().with("base", 10.0).with("graph_1", 999.0);
        let mut evaluator = Evaluator::with_seed(1);
        let pass = evaluator.evaluate(&[a.clone(), b], &fake, Duration::ZERO);
        let first = &pass.graphs[0];
        assert!(!first.evaluated);
        assert!(first.values.is_empty());
        assert_eq!(first.end, EndValue::Sensor(None));
        assert_eq!(pass.graphs[1].end, EndValue::Sensor(None));
        assert_eq!(
            pass.sensor_values(),
            vec![("graph_1".to_owned(), None), ("graph_2".to_owned(), None)]
        );
        // Its nodes keep their state while it is disabled.
        assert!(evaluator.states.is_empty());
        // A response Graph that is disabled has no end value.
        let mut r = graph("r", vec![node(1, Kind::ResponseEnd)], vec![]);
        r.enabled = false;
        let pass = evaluator.evaluate(&[r], &fake, Duration::ZERO);
        assert_eq!(pass.graphs[0].end, EndValue::None);
        assert!(pass.sensor_values().is_empty());
    }

    #[test]
    fn comment_node_is_ignored() {
        let mut g = sensor_graph("g", "graph_1", "base", 1.0);
        g.nodes.push(node(9, Kind::Comment));
        let pass = pass_one(&g, &Fake::default().with("base", 1.0));
        assert!(!pass.values.contains_key(&9));
        assert_eq!(pass.end, EndValue::Sensor(Some(2.0)));
        assert_eq!(pass.values.len(), 4);
    }

    /// One Graph for each kind, with every input pin on a source that has no value.
    fn dead_input_graph(kind: Kind) -> Graph {
        let mut nodes = vec![source(1, "dead"), node(2, kind)];
        let mut connections = Vec::new();
        for pin in kind.inputs() {
            connections.push(Connection::new(1, 2, pin.name));
        }
        if kind.has_output() {
            nodes.push(node(3, Kind::SensorEnd));
            connections.push(Connection::new(2, 3, "a"));
        }
        graph("g", nodes, connections)
    }

    #[test]
    fn every_kind_honors_the_no_value_rule() {
        assert_eq!(Kind::ALL.len(), 51);
        for kind in Kind::ALL {
            let g = dead_input_graph(*kind);
            let fake = Fake {
                processes: ProcessList {
                    entries: vec![ProcessEntry::new("x", "", "")],
                },
                ..Fake::default()
            };
            let pass = pass_one(&g, &fake);
            let value = pass.values.get(&2).copied();
            match kind {
                // The node "has value" is the one exception: it sees the missing value.
                Kind::HasValue => assert_eq!(value, Some(Some(0.0)), "{kind}"),
                // The comment node has no pins and no value.
                Kind::Comment => assert_eq!(value, None, "{kind}"),
                // Sources that have no input pin: the sensor has none and the daemon is offline.
                Kind::SensorSource | Kind::AlarmEventSource => {
                    assert_eq!(value, Some(None), "{kind}");
                }
                Kind::Constant | Kind::Random | Kind::Process => {
                    assert!(matches!(value, Some(Some(_))), "{kind}");
                }
                Kind::SensorEnd => assert_eq!(pass.end, EndValue::Sensor(None), "{kind}"),
                Kind::ResponseEnd => assert_eq!(pass.end, EndValue::Response(None), "{kind}"),
                _ => {
                    assert!(!kind.inputs().is_empty());
                    assert_eq!(value, Some(None), "{kind}");
                    // The next node sees no value either.
                    assert_eq!(pass.end, EndValue::Sensor(None), "{kind}");
                }
            }
        }
    }

    #[test]
    fn state_nodes_keep_their_state_when_a_pin_has_no_value() {
        // A counter that has counted once keeps its output over a gap in `count`.
        let g = graph(
            "g",
            vec![
                source(1, "count"),
                constant(2, 1.0),
                node(3, Kind::Counter),
                node(4, Kind::SensorEnd),
            ],
            vec![
                Connection::new(1, 3, "count"),
                Connection::new(2, 3, "period"),
                Connection::new(3, 4, "a"),
            ],
        );
        let mut evaluator = Evaluator::with_seed(1);
        let mut out = Vec::new();
        for t in 0..6u64 {
            let fake = if t == 3 {
                Fake::default()
            } else {
                Fake::default().with("count", 2.0)
            };
            let pass = evaluator.evaluate(std::slice::from_ref(&g), &fake, Duration::from_secs(t));
            out.push(pass.graphs[0].values[&3]);
        }
        // The output 4 is still there after the gap: the next count makes 6.
        assert_eq!(
            out,
            [Some(0.0), Some(2.0), Some(4.0), None, Some(6.0), Some(8.0)]
        );
    }

    #[test]
    fn sources_alarm_process_random_and_response_input() {
        let g = graph(
            "g",
            vec![
                node(1, Kind::AlarmEventSource)
                    .with_param("event", "pump-stop")
                    .with_param("source", "loop_1"),
                node(2, Kind::Process).with_param("name", "Blender"),
                node(3, Kind::Random)
                    .with_param("min", 10)
                    .with_param("max", 20),
                node(4, Kind::And),
                node(5, Kind::ResponseEnd),
            ],
            vec![
                Connection::new(1, 4, "a"),
                Connection::new(2, 4, "b"),
                Connection::new(4, 5, "a"),
            ],
        );
        let mut fake = Fake::default();
        // The daemon is offline: the Alarm has no value.
        let pass = pass_one(&g, &fake);
        assert_eq!(pass.values[&1], None);
        assert_eq!(pass.end, EndValue::Response(None));
        // The Alarm of another Loop does not count.
        fake.alarms = Some(BTreeSet::from([(
            "pump-stop".to_owned(),
            "loop_2".to_owned(),
        )]));
        assert_eq!(pass_one(&g, &fake).values[&1], Some(0.0));
        fake.alarms = Some(BTreeSet::from([(
            "pump-stop".to_owned(),
            "loop_1".to_owned(),
        )]));
        let pass = pass_one(&g, &fake);
        assert_eq!(pass.values[&1], Some(1.0));
        // The process does not run yet.
        assert_eq!(pass.values[&2], Some(0.0));
        assert_eq!(pass.end, EndValue::Response(Some(false)));
        fake.processes = ProcessList {
            entries: vec![ProcessEntry::new("blender", "", "")],
        };
        let pass = pass_one(&g, &fake);
        assert_eq!(pass.values[&2], Some(1.0));
        assert_eq!(pass.end, EndValue::Response(Some(true)));
        let random = pass.values[&3].unwrap();
        assert!((10.0..=20.0).contains(&random));
        // The same seed gives the same random numbers.
        assert_eq!(pass_one(&g, &fake).values[&3], pass.values[&3]);
    }

    #[test]
    fn apply_resets_node_state_keeps_stored_values() {
        // A persistent storage node and an average over a window.
        let g = graph(
            "g",
            vec![
                constant(1, 5.0),
                constant(2, 1.0),
                node(3, Kind::PersistentStorage),
                node(4, Kind::WindowAverage).with_param("span_s", 60),
                node(5, Kind::Add),
                node(6, Kind::SensorEnd),
            ],
            vec![
                Connection::new(1, 3, "a"),
                Connection::new(2, 3, "s"),
                Connection::new(1, 4, "a"),
                Connection::new(3, 5, "a"),
                Connection::new(4, 5, "b"),
                Connection::new(5, 6, "a"),
            ],
        );
        let mut evaluator = Evaluator::with_seed(1);
        let fake = Fake::default();
        for t in 0..3u64 {
            evaluator.evaluate(std::slice::from_ref(&g), &fake, Duration::from_secs(t));
        }
        assert_eq!(
            evaluator.stored_values(),
            BTreeMap::from([("g/3".to_owned(), 5.0)])
        );
        let window = |e: &Evaluator| match e.states.get(&("g".to_owned(), 4)) {
            Some(NodeState::Filter(f)) => f.window().len(),
            _ => 0,
        };
        assert_eq!(window(&evaluator), 3);
        // The user changes a constant and applies: the window is empty again.
        let mut changed = g.clone();
        changed.nodes[1] = constant(2, 0.0);
        evaluator.apply(&changed);
        assert_eq!(window(&evaluator), 0);
        // The stored value is kept: the node gives it while S is 0.
        let pass = evaluator.evaluate(
            std::slice::from_ref(&changed),
            &fake,
            Duration::from_secs(10),
        );
        assert_eq!(pass.graphs[0].values[&3], Some(5.0));
        // Apply of a Graph without the node deletes its stored value, and the others stay.
        let mut other = graph("other", vec![node(1, Kind::PersistentStorage)], vec![]);
        other.nodes[0].id = 1;
        evaluator.evaluate(std::slice::from_ref(&other), &fake, Duration::from_secs(11));
        assert_eq!(evaluator.stored_values().len(), 2);
        let mut without = changed.clone();
        without.nodes.remove(2);
        without
            .connections
            .retain(|c| c.from.node != 3 && c.to.node != 3);
        evaluator.apply(&without);
        let stored = evaluator.stored_values();
        assert_eq!(stored.keys().collect::<Vec<_>>(), ["other/1"]);
        // A node that keeps its id and changes its kind loses the value.
        let mut kind_change = other.clone();
        kind_change.nodes[0].kind = Kind::Memory;
        evaluator.apply(&kind_change);
        assert!(evaluator.stored_values().is_empty());
        // A stored value comes back with `restore_value`.
        evaluator.restore_value("g", 3, 7.0);
        assert_eq!(evaluator.stored_values()["g/3"], 7.0);
        evaluator.remove_graph("g");
        assert!(evaluator.stored_values().is_empty());
    }
}
