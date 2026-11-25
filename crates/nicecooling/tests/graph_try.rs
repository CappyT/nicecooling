mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling::graphs::channel::{GraphCommand, TryRequest};
use nicecooling::graphs::service::{Env, GraphService, PowerCall, SensorLink};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::view::{SharedView, shared_view};
use nicecooling::i18n::EN;
use nicecooling::model::{DaemonEvent, Link, SensorSample};
use nicecooling::notify::Plan;
use nicecooling_core::clock::SimClock;
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::eval::EndValue;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};
use nicecooling_graph::pause::PowerKind;
use support::graphs::Dir;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

type Calls = Vec<Vec<(String, Option<f64>)>>;

#[derive(Clone, Default)]
struct Sent(Arc<Mutex<Calls>>);

impl SensorLink for Sent {
    fn values(&mut self, values: &[(String, Option<f64>)]) {
        self.0.lock().unwrap().push(values.to_vec());
    }
}

#[derive(Clone, Default)]
struct Powered(Arc<Mutex<Vec<PowerKind>>>);

impl PowerCall for Powered {
    fn call(&mut self, kind: PowerKind) {
        self.0.lock().unwrap().push(kind);
    }
}

struct NoScan;

impl ProcessSource for NoScan {
    fn scan(&self) -> ProcessList {
        ProcessList::default()
    }
}

struct Rig {
    service: GraphService,
    clock: SimClock,
    view: SharedView,
    sent: Sent,
    powered: Powered,
    notifications: UnboundedReceiver<Plan>,
    _dir: Dir,
}

fn rig(name: &str, stored: Vec<Graph>) -> Rig {
    let dir = Dir::new(name);
    let mut store = GraphStore::open(dir.graph_paths());
    for graph in stored {
        store.put(graph).unwrap();
    }
    let clock = SimClock::default();
    let view = shared_view();
    let sent = Sent::default();
    let powered = Powered::default();
    let (notify, notifications) = unbounded_channel();
    let mut service = GraphService::new(
        store,
        Env {
            clock: Box::new(clock.clone()),
            link: Box::new(sent.clone()),
            scan: Box::new(NoScan),
            power: Box::new(powered.clone()),
            notify,
            locale: EN,
            view: view.clone(),
        },
    );
    service.apply_event(&DaemonEvent::Link(Link::Online));
    Rig {
        service,
        clock,
        view,
        sent,
        powered,
        notifications,
        _dir: dir,
    }
}

fn live(rig: &mut Rig, rows: &[(&str, f64)]) {
    rig.service.apply_event(&DaemonEvent::Sensors(SensorSample {
        time_us: 1,
        sensors: rows.iter().map(|(id, v)| ((*id).into(), *v, 0)).collect(),
        channels: Vec::new(),
    }));
}

fn source(id: u32, sensor: &str) -> Node {
    Node::new(id, Kind::SensorSource).with_param("sensor", sensor)
}

fn request(graph: Graph, tests: &[(u32, f64)], seconds: u32) -> GraphCommand {
    GraphCommand::Try(Box::new(TryRequest {
        graph,
        tests: tests.to_vec(),
        seconds,
    }))
}

/// A draft: the source 1 plus a constant 5, into a sensor end node.
fn add_five() -> Graph {
    let mut graph = Graph::new("graph_9", "Draft");
    graph.nodes = vec![
        source(1, "nct6798_temp1"),
        Node::new(2, Kind::Constant).with_param("value", 5.0),
        Node::new(3, Kind::Add),
        Node::new(4, Kind::SensorEnd),
    ];
    graph.connections = vec![
        Connection::new(1, 3, "a"),
        Connection::new(2, 3, "b"),
        Connection::new(3, 4, "a"),
    ];
    graph
}

fn values(rig: &Rig) -> std::collections::BTreeMap<u32, Option<f64>> {
    rig.view
        .lock()
        .unwrap()
        .trying
        .as_ref()
        .expect("a try")
        .values
        .clone()
}

#[test]
fn try_value_80_gives_85() {
    let mut rig = rig("try-85", vec![]);
    live(&mut rig, &[("nct6798_temp1", 20.0)]);
    rig.service.handle(request(add_five(), &[(1, 80.0)], 30));
    let shown = values(&rig);
    assert_eq!(shown[&1], Some(80.0));
    assert_eq!(shown[&3], Some(85.0));
    assert_eq!(shown[&4], Some(85.0));
    let view = rig.view.lock().unwrap();
    assert_eq!(view.trying.as_ref().unwrap().seconds_left, 30);
    assert_eq!(view.refusal, None);
}

#[test]
fn try_source_without_a_test_value_reads_the_live_value() {
    let mut draft = add_five();
    draft.nodes[1] = source(2, "nct6798_temp2");
    let mut rig = rig("try-live", vec![]);
    live(
        &mut rig,
        &[("nct6798_temp1", 20.0), ("nct6798_temp2", 20.0)],
    );
    // The first source has the test value 80. The second one has none and reads 20.
    rig.service.handle(request(draft, &[(1, 80.0)], 30));
    let shown = values(&rig);
    assert_eq!(shown[&1], Some(80.0));
    assert_eq!(shown[&2], Some(20.0));
    assert_eq!(shown[&3], Some(100.0));
}

#[test]
fn try_sends_nothing_to_the_daemon() {
    // The applied Graph has a Graph sensor that a Curve could use.
    let mut applied = add_five();
    applied.id = "graph_1".into();
    applied.sensor_id = Some("graph_1".into());
    let mut rig = rig("try-sends", vec![applied.clone()]);
    live(&mut rig, &[("nct6798_temp1", 20.0)]);
    rig.service.poll();
    assert_eq!(
        rig.sent.0.lock().unwrap().last().unwrap(),
        &[("graph_1".to_owned(), Some(25.0))]
    );

    // A try of a draft of the same Graph with the test value 80 changes the node values of the
    // try. The applied Graph still sends its own value in each pass.
    rig.service.handle(request(applied, &[(1, 80.0)], 30));
    assert_eq!(values(&rig)[&4], Some(85.0));
    for _ in 0..3 {
        rig.clock.advance(Duration::from_secs(1));
        rig.service.poll();
        assert_eq!(
            rig.sent.0.lock().unwrap().last().unwrap(),
            &[("graph_1".to_owned(), Some(25.0))]
        );
    }
    assert_eq!(values(&rig)[&4], Some(85.0));
    let calls = rig.sent.0.lock().unwrap().len();
    assert_eq!(calls, 4, "one call for each pass of the applied Graph");
}

#[test]
fn try_runs_no_response() {
    let mut draft = Graph::new("graph_9", "Draft");
    draft.nodes = vec![
        Node::new(1, Kind::Constant).with_param("value", 1.0),
        Node::new(2, Kind::ResponseEnd)
            .with_param("kind", "poweroff")
            .with_param("countdown_s", 10),
    ];
    draft.connections = vec![Connection::new(1, 2, "a")];
    let mut rig = rig("try-response", vec![]);
    rig.service.handle(request(draft, &[], 60));
    for _ in 0..30 {
        rig.clock.advance(Duration::from_secs(1));
        rig.service.poll();
    }
    // The condition is true all the time, and nothing runs.
    let view = rig.view.lock().unwrap();
    assert_eq!(
        view.trying.as_ref().unwrap().end,
        EndValue::Response(Some(true))
    );
    assert_eq!(view.countdown, None);
    assert_eq!(*rig.powered.0.lock().unwrap(), []);
    drop(view);
    assert!(rig.notifications.try_recv().is_err());
}

#[test]
fn try_ends_after_the_time() {
    let mut rig = rig("try-ends", vec![]);
    rig.service.handle(request(add_five(), &[(1, 80.0)], 3));
    for second in 1..=2 {
        rig.clock.advance(Duration::from_secs(1));
        rig.service.poll();
        let left = rig
            .view
            .lock()
            .unwrap()
            .trying
            .as_ref()
            .unwrap()
            .seconds_left;
        assert_eq!(left, 3 - second);
    }
    rig.clock.advance(Duration::from_secs(1));
    rig.service.poll();
    assert_eq!(rig.view.lock().unwrap().trying, None, "the time is over");

    // "Stop" ends a try at once. A bad time and a bad draft are refused with a reason.
    rig.service.handle(request(add_five(), &[(1, 80.0)], 600));
    assert!(rig.view.lock().unwrap().trying.is_some());
    rig.service.handle(GraphCommand::StopTry);
    assert_eq!(rig.view.lock().unwrap().trying, None);
    for seconds in [0, 601] {
        rig.service.handle(request(add_five(), &[], seconds));
        assert!(rig.view.lock().unwrap().trying.is_none());
        let refusal = rig.view.lock().unwrap().refusal.clone().unwrap();
        assert!(refusal.contains("1 to 600"), "{refusal}");
    }
    let mut broken = add_five();
    broken.nodes.push(Node::new(5, Kind::SensorEnd));
    rig.service.handle(request(broken, &[], 10));
    let refusal = rig.view.lock().unwrap().refusal.clone().unwrap();
    assert!(refusal.contains("two-end-nodes"), "{refusal}");
}

#[test]
fn try_with_a_fake_clock_in_10_steps() {
    // The input of a 3 s on delay node rises at the start of the try. With a clock that moves by
    // hand, the node has given 1 from the 4th step.
    let mut draft = Graph::new("graph_9", "Delay");
    draft.nodes = vec![
        source(1, "x"),
        Node::new(2, Kind::RiseDelay).with_param("delay_s", 3),
        Node::new(3, Kind::SensorEnd),
    ];
    draft.connections = vec![Connection::new(1, 2, "a"), Connection::new(2, 3, "a")];
    let mut rig = rig("try-delay", vec![]);
    rig.service.handle(request(draft, &[(1, 1.0)], 60));
    let mut given = vec![values(&rig)[&2]];
    for _ in 0..9 {
        rig.clock.advance(Duration::from_secs(1));
        rig.service.poll();
        given.push(values(&rig)[&2]);
    }
    let first_one = given.iter().position(|v| *v == Some(1.0)).expect("a 1");
    assert_eq!(first_one, 3, "the 4th step: {given:?}");
    assert!(
        given[first_one..].iter().all(|v| *v == Some(1.0)),
        "{given:?}"
    );
}

#[test]
fn try_value_that_is_not_finite_gives_the_source_no_value() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut rig = rig("try-not-finite", vec![]);
        live(&mut rig, &[("nct6798_temp1", 20.0)]);
        rig.service.handle(request(add_five(), &[(1, value)], 30));
        let shown = values(&rig);
        assert_eq!(shown.get(&1).copied().flatten(), None, "{value}");
        assert_eq!(shown.get(&3).copied().flatten(), None, "{value}");
    }
}
