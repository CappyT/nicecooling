mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling::graphs::service::{GraphService, SensorLink};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::view::{RowState, SharedView, shared_view};
use nicecooling::model::{DaemonEvent, FLAG_NO_VALUE, Link, Response, SensorSample};
use nicecooling_core::clock::SimClock;
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::{ProcessEntry, ProcessList};
use nicecooling_graph::eval::EndValue;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};
use support::fixtures::alarm;
use support::graphs::{Dir, constant_graph, source_graph};

type Calls = Arc<Mutex<Vec<Vec<(String, Option<f64>)>>>>;

struct Recorder(Calls);

impl SensorLink for Recorder {
    fn values(&mut self, values: &[(String, Option<f64>)]) {
        self.0.lock().unwrap().push(values.to_vec());
    }
}

#[derive(Clone, Default)]
struct FakeScan {
    list: Arc<Mutex<ProcessList>>,
    scans: Arc<Mutex<u32>>,
}

impl ProcessSource for FakeScan {
    fn scan(&self) -> ProcessList {
        *self.scans.lock().unwrap() += 1;
        self.list.lock().unwrap().clone()
    }
}

struct Rig {
    service: GraphService,
    clock: SimClock,
    calls: Calls,
    view: SharedView,
    scan: FakeScan,
    _dir: Dir,
}

fn rig(name: &str, graphs: Vec<Graph>) -> Rig {
    let dir = Dir::new(name);
    let mut store = GraphStore::open(dir.graph_paths());
    for graph in graphs {
        store.put(graph).unwrap();
    }
    let clock = SimClock::default();
    let calls = Calls::default();
    let view = shared_view();
    let scan = FakeScan::default();
    let service = GraphService::new(
        store,
        Box::new(clock.clone()),
        Box::new(Recorder(calls.clone())),
        Box::new(scan.clone()),
        view.clone(),
    );
    Rig {
        service,
        clock,
        calls,
        view,
        scan,
        _dir: dir,
    }
}

fn sample(rows: &[(&str, f64, u8)]) -> DaemonEvent {
    DaemonEvent::Sensors(SensorSample {
        time_us: 1,
        sensors: rows
            .iter()
            .map(|(id, v, f)| ((*id).into(), *v, *f))
            .collect(),
        channels: Vec::new(),
    })
}

fn sensor_graph(id: &str, sensor: &str, sensor_id: &str) -> Graph {
    let mut graph = source_graph(id, sensor);
    graph.sensor_id = Some(sensor_id.into());
    graph
}

#[test]
fn sources_from_sensor_events() {
    let mut rig = rig(
        "sources",
        vec![sensor_graph("graph_1", "nct6798_temp1", "graph_1")],
    );
    rig.service.apply_event(&DaemonEvent::Link(Link::Online));
    rig.service
        .apply_event(&sample(&[("nct6798_temp1", 46.2, 0)]));
    assert!(rig.service.poll());
    assert_eq!(
        rig.calls.lock().unwrap().last().unwrap(),
        &[("graph_1".to_owned(), Some(46.2))]
    );
    // A sensor with the no-value flag, and a sensor that does not exist, give no value.
    rig.clock.advance(Duration::from_secs(1));
    rig.service
        .apply_event(&sample(&[("nct6798_temp1", 0.0, FLAG_NO_VALUE)]));
    rig.service.poll();
    assert_eq!(rig.calls.lock().unwrap().last().unwrap()[0].1, None);
}

fn alarm_graph() -> Graph {
    let mut graph = Graph::new("graph_1", "Pump");
    graph.nodes = vec![
        Node::new(1, Kind::AlarmEventSource)
            .with_param("event", "pump-stop")
            .with_param("source", "loop_1"),
        Node::new(2, Kind::SensorEnd).with_param("unit", "%"),
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    graph.sensor_id = Some("graph_1".into());
    graph
}

#[test]
fn alarm_event_source_value() {
    let mut rig = rig("alarm", vec![alarm_graph()]);
    rig.service.apply_event(&DaemonEvent::Link(Link::Online));
    let value = |rig: &Rig| rig.calls.lock().unwrap().last().unwrap()[0].1;
    rig.service.poll();
    assert_eq!(value(&rig), Some(0.0));

    // The Alarm of another Loop does not count.
    rig.clock.advance(Duration::from_secs(1));
    rig.service.apply_event(&DaemonEvent::AlarmRaised(alarm(
        "a2",
        "pump-stop",
        "loop_2",
        Response::Notify,
        0,
    )));
    rig.service.poll();
    assert_eq!(value(&rig), Some(0.0));

    rig.clock.advance(Duration::from_secs(1));
    rig.service.apply_event(&DaemonEvent::AlarmRaised(alarm(
        "a1",
        "pump-stop",
        "loop_1",
        Response::Notify,
        0,
    )));
    rig.service.poll();
    assert_eq!(value(&rig), Some(1.0));
}

fn process_graph(id: &str, name: &str) -> Graph {
    let mut graph = Graph::new(id, "Process");
    graph.nodes = vec![
        Node::new(1, Kind::Process).with_param("name", name),
        Node::new(2, Kind::SensorEnd).with_param("unit", "%"),
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    graph.sensor_id = Some(id.into());
    graph
}

#[test]
fn daemon_offline_gives_no_value_to_every_source() {
    let mut rig = rig(
        "offline",
        vec![
            sensor_graph("graph_1", "nct6798_temp1", "graph_1"),
            alarm_graph_with_id("graph_2"),
            process_graph("graph_3", "blender"),
        ],
    );
    rig.scan.list.lock().unwrap().entries = vec![ProcessEntry::new("blender", "blender", "")];
    rig.service.apply_event(&DaemonEvent::Link(Link::Online));
    rig.service
        .apply_event(&sample(&[("nct6798_temp1", 46.2, 0)]));
    rig.service.poll();
    let last = |rig: &Rig| rig.calls.lock().unwrap().last().unwrap().clone();
    assert_eq!(
        last(&rig),
        [
            ("graph_1".to_owned(), Some(46.2)),
            ("graph_2".to_owned(), Some(0.0)),
            ("graph_3".to_owned(), Some(1.0)),
        ]
    );

    rig.clock.advance(Duration::from_secs(1));
    rig.service.apply_event(&DaemonEvent::Link(Link::Offline));
    rig.service.poll();
    // The sensor and the alarm sources have no value. The process scan needs no daemon.
    assert_eq!(
        last(&rig),
        [
            ("graph_1".to_owned(), None),
            ("graph_2".to_owned(), None),
            ("graph_3".to_owned(), Some(1.0)),
        ]
    );
}

fn alarm_graph_with_id(id: &str) -> Graph {
    let mut graph = alarm_graph();
    graph.id = id.into();
    graph.sensor_id = Some(id.into());
    graph
}

#[test]
fn process_node_uses_the_scan() {
    let mut rig = rig("process", vec![process_graph("graph_1", "Blender")]);
    rig.service.poll();
    let value = |rig: &Rig| rig.calls.lock().unwrap().last().unwrap()[0].1;
    assert_eq!(value(&rig), Some(0.0));

    // The name match ignores the case, as for a Profile rule.
    rig.scan.list.lock().unwrap().entries = vec![ProcessEntry::new("blender", "", "")];
    rig.clock.advance(Duration::from_secs(1));
    rig.service.poll();
    assert_eq!(value(&rig), Some(1.0));
    assert_eq!(*rig.scan.scans.lock().unwrap(), 2, "one scan for each pass");

    // A pass of Graphs with no process node scans nothing.
    let mut rig = self::rig("no-process", vec![constant_graph("graph_1", 1.0)]);
    rig.service.poll();
    assert_eq!(*rig.scan.scans.lock().unwrap(), 0);
}

#[test]
fn graph_b_reads_a_in_the_same_pass() {
    // B is stored first and reads the Graph sensor of A, which is stored later.
    let b = sensor_graph("graph_2", "graph_1", "graph_2");
    let mut a = constant_graph("graph_1", 5.0);
    a.sensor_id = Some("graph_1".into());
    let mut rig = rig("chain", vec![b, a]);
    rig.service.poll();
    assert_eq!(
        rig.calls.lock().unwrap().last().unwrap(),
        &[
            ("graph_2".to_owned(), Some(5.0)),
            ("graph_1".to_owned(), Some(5.0)),
        ]
    );
}

#[test]
fn disabled_graph_gets_the_no_value_entry() {
    let mut graph = constant_graph("graph_1", 5.0);
    graph.sensor_id = Some("graph_1".into());
    graph.enabled = false;
    let mut rig = rig("disabled", vec![graph]);
    rig.service.poll();
    assert_eq!(
        rig.calls.lock().unwrap().last().unwrap(),
        &[("graph_1".to_owned(), None)]
    );
    let view = rig.view.lock().unwrap();
    let row = view.row("graph_1").unwrap();
    assert_eq!(row.state, RowState::Disabled);
    assert!(row.values.is_empty());
}

#[test]
fn evaluates_once_per_second_with_a_fake_clock() {
    let mut graph = constant_graph("graph_1", 5.0);
    graph.sensor_id = Some("graph_1".into());
    let mut rig = rig("cadence", vec![graph]);
    let mut ran = 0;
    // 10 s in steps of 250 ms. The first poll runs at once, then one pass per second.
    for _ in 0..=40 {
        if rig.service.poll() {
            ran += 1;
        }
        rig.clock.advance(Duration::from_millis(250));
    }
    assert_eq!(ran, 11);
    assert_eq!(rig.calls.lock().unwrap().len(), 11);
    // The time to the next pass shrinks with the clock.
    assert!(rig.service.until_next_pass() <= Duration::from_secs(1));
}

#[test]
fn view_holds_the_node_values() {
    let mut graph = constant_graph("graph_1", 5.0);
    graph.sensor_id = Some("graph_1".into());
    let mut rig = rig("view", vec![graph]);
    rig.service.poll();
    let view = rig.view.lock().unwrap();
    let row = view.row("graph_1").unwrap();
    assert_eq!(row.state, RowState::Running);
    assert_eq!(row.values[&1], Some(5.0));
    assert_eq!(row.values[&2], Some(5.0));
    assert_eq!(row.end, EndValue::Sensor(Some(5.0)));
    assert_eq!(view.error, None);
}
