//! Review of Apply against the state machine of a response end node. Apply resets the evaluation
//! state of the Graph, but the machine of its response end node keeps the trigger time from before
//! the Apply. So the hold time counts the time before the Apply.

mod support;

use std::time::Duration;

use nicecooling::graphs::channel::GraphCommand;
use nicecooling::graphs::service::{Env, GraphService, NoLink, NoPower};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::view::{SharedView, shared_view};
use nicecooling::i18n::EN;
use nicecooling::model::{DaemonEvent, Link, SensorSample};
use nicecooling_core::clock::{SimClock, SimWallClock};
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};
use support::graphs::Dir;
use tokio::sync::mpsc;

const WALL_START_US: u64 = 1_766_000_000_000_000;

struct NoScan;

impl ProcessSource for NoScan {
    fn scan(&self) -> ProcessList {
        ProcessList::default()
    }
}

/// A poweroff response with a hold time of 60 s on the sensor `s1`.
fn hold_graph(name: &str) -> Graph {
    let mut graph = Graph::new("graph_1", name);
    graph.nodes = vec![
        Node::new(1, Kind::SensorSource).with_param("sensor", "s1"),
        Node::new(2, Kind::ResponseEnd)
            .with_param("kind", "poweroff")
            .with_param("hold_s", 60)
            .with_param("countdown_s", 30)
            .with_param("min_interval_s", 0),
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    graph
}

/// The spec (node-graph, Apply resets state): "Apply of a Graph SHALL reset the state of its nodes
/// except the stored values and the run state of nodes that keep their id and kind." The trigger
/// time of the response end node is not its run state: after an Apply the hold time starts again.
#[test]
fn apply_starts_the_hold_time_again() {
    let dir = Dir::new("rv2-apply-hold");
    let mut store = GraphStore::open(dir.graph_paths());
    store.put(hold_graph("Overheat")).unwrap();
    let clock = SimClock::default();
    let view: SharedView = shared_view();
    let (notify, notifications) = mpsc::unbounded_channel();
    std::mem::forget(notifications);
    let mut service = GraphService::new(
        store,
        Env {
            clock: Box::new(clock.clone()),
            link: Box::new(NoLink),
            scan: Box::new(NoScan),
            power: Box::new(NoPower),
            notify,
            locale: EN,
            view: view.clone(),
        },
    )
    .with_wall_clock(Box::new(SimWallClock::new(WALL_START_US)));
    service.apply_event(&DaemonEvent::Link(Link::Online));
    service.apply_event(&DaemonEvent::Sensors(SensorSample {
        time_us: 1,
        sensors: vec![("s1".into(), 1.0, 0)],
        channels: Vec::new(),
    }));
    let mut ticks = |seconds: u32| {
        for _ in 0..seconds {
            clock.advance(Duration::from_secs(1));
            service.poll();
        }
    };
    ticks(40);
    assert!(view.lock().unwrap().countdown.is_none(), "40 s of 60 s");

    // The user renames the Graph and applies it. The input stays true.
    service.handle(GraphCommand::Apply(Box::new(hold_graph("Overheat 2"))));
    assert!(
        view.lock()
            .unwrap()
            .applied
            .as_ref()
            .is_some_and(|applied| applied.result.is_ok()),
        "the Apply is stored"
    );
    let mut ticks = |seconds: u32| {
        for _ in 0..seconds {
            clock.advance(Duration::from_secs(1));
            service.poll();
        }
    };
    ticks(30);
    assert!(
        view.lock().unwrap().countdown.is_none(),
        "a poweroff countdown started 30 s after the Apply with a hold time of 60 s"
    );
}
