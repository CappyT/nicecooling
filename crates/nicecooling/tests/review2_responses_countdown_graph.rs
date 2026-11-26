//! Review of a power countdown against a change of its Graph. The countdown keeps no link to its
//! Graph: a Graph that the user disables, deletes, or changes to another kind while the countdown
//! runs still powers off the PC at the end.

mod support;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling::graphs::channel::GraphCommand;
use nicecooling::graphs::power::NOTIFICATION_KEY;
use nicecooling::graphs::responses::Outcome;
use nicecooling::graphs::service::{Env, GraphService, NoLink, PowerCall};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::view::{SharedView, shared_view};
use nicecooling::i18n::EN;
use nicecooling::model::{DaemonEvent, Link};
use nicecooling::notify::Plan;
use nicecooling_core::clock::{SimClock, SimWallClock};
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};
use nicecooling_graph::pause::PowerKind;
use support::graphs::Dir;
use tokio::sync::mpsc;

const WALL_START_US: u64 = 1_766_000_000_000_000;

struct NoScan;

impl ProcessSource for NoScan {
    fn scan(&self) -> ProcessList {
        ProcessList::default()
    }
}

#[derive(Clone, Default)]
struct Power(Arc<Mutex<Vec<PowerKind>>>);

impl PowerCall for Power {
    fn call(&mut self, kind: PowerKind) -> Result<(), String> {
        self.0.lock().unwrap().push(kind);
        Ok(())
    }
}

/// A Graph whose response end node has the kind `kind` and an input that is always true.
fn response_graph(kind: &str) -> Graph {
    let mut graph = Graph::new("graph_1", "Overheat");
    graph.nodes = vec![
        Node::new(1, Kind::Constant).with_param("value", 1.0),
        Node::new(2, Kind::ResponseEnd)
            .with_param("kind", kind)
            .with_param("countdown_s", 10)
            .with_param("min_interval_s", 0)
            .with_param("title", "Hot"),
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    graph
}

struct Rig {
    service: GraphService,
    clock: SimClock,
    view: SharedView,
    power: Power,
    _dir: Dir,
}

impl Rig {
    fn new(name: &str) -> Self {
        let dir = Dir::new(name);
        let mut store = GraphStore::open(dir.graph_paths());
        store.put(response_graph("poweroff")).unwrap();
        let clock = SimClock::default();
        let view = shared_view();
        let power = Power::default();
        let (notify, notifications) = mpsc::unbounded_channel();
        // The notifications are not read: the channel stays open.
        std::mem::forget(notifications);
        let mut service = GraphService::new(
            store,
            Env {
                clock: Box::new(clock.clone()),
                link: Box::new(NoLink),
                scan: Box::new(NoScan),
                power: Box::new(power.clone()),
                notify,
                locale: EN,
                view: view.clone(),
            },
        )
        .with_wall_clock(Box::new(SimWallClock::new(WALL_START_US)));
        service.apply_event(&DaemonEvent::Link(Link::Online));
        Self {
            service,
            clock,
            view,
            power,
            _dir: dir,
        }
    }

    fn ticks(&mut self, seconds: u32) {
        for _ in 0..seconds {
            self.clock.advance(Duration::from_secs(1));
            self.service.poll();
        }
    }

    /// Run the passes until the countdown of the poweroff runs.
    fn start_countdown(&mut self) {
        self.ticks(2);
        assert!(
            self.view.lock().unwrap().countdown.is_some(),
            "the poweroff countdown runs"
        );
    }

    fn calls(&self) -> Vec<PowerKind> {
        self.power.0.lock().unwrap().clone()
    }
}

/// The spec (node-graph, Disabled Graph): "A disabled Graph SHALL not be evaluated. Its Graph
/// sensor SHALL get the no-value entry and its response SHALL not run."
#[test]
fn disable_in_the_countdown_makes_no_power_call() {
    let mut rig = Rig::new("rv2-countdown-disable");
    rig.start_countdown();
    rig.service.handle(GraphCommand::Enable {
        graph: "graph_1".into(),
        enabled: false,
    });
    assert!(
        rig.service
            .store()
            .graph("graph_1")
            .is_some_and(|graph| !graph.enabled),
        "the Graph is stored as disabled"
    );
    rig.ticks(15);
    assert_eq!(
        rig.calls(),
        Vec::new(),
        "the disabled Graph powered off the PC"
    );
}

/// A deleted Graph has no response. Its countdown must not end in a power call.
#[test]
fn delete_in_the_countdown_makes_no_power_call() {
    let mut rig = Rig::new("rv2-countdown-delete");
    rig.start_countdown();
    rig.service.handle(GraphCommand::Delete {
        graph: "graph_1".into(),
    });
    assert!(
        rig.service.store().graph("graph_1").is_none(),
        "the Graph is deleted"
    );
    rig.ticks(15);
    assert_eq!(
        rig.calls(),
        Vec::new(),
        "the deleted Graph powered off the PC"
    );
}

/// The user applies the Graph with the kind notification while the poweroff countdown runs. The
/// applied Graph has no power response any more.
#[test]
fn apply_of_another_kind_in_the_countdown_makes_no_power_call() {
    let mut rig = Rig::new("rv2-countdown-kind");
    rig.start_countdown();
    rig.service
        .handle(GraphCommand::Apply(Box::new(response_graph(
            "notification",
        ))));
    assert!(
        rig.view
            .lock()
            .unwrap()
            .applied
            .as_ref()
            .is_some_and(|applied| applied.result.is_ok()),
        "the Apply is stored"
    );
    rig.ticks(15);
    assert_eq!(
        rig.calls(),
        Vec::new(),
        "a Graph with no power response powered off the PC"
    );
}

/// A scan of `/proc` that panics when the test says so.
#[derive(Clone, Default)]
struct Scan(Arc<AtomicBool>);

impl ProcessSource for Scan {
    fn scan(&self) -> ProcessList {
        assert!(!self.0.load(Ordering::SeqCst), "a test panic");
        ProcessList::default()
    }
}

/// The service with one Graph, and the notifications that it sends.
fn service_with(
    dir: &Dir,
    graph: Graph,
    scan: Box<dyn ProcessSource>,
    clock: &SimClock,
    view: &SharedView,
    power: &Power,
) -> (GraphService, mpsc::UnboundedReceiver<Plan>) {
    let mut store = GraphStore::open(dir.graph_paths());
    store.put(graph).unwrap();
    let (notify, notifications) = mpsc::unbounded_channel();
    let mut service = GraphService::new(
        store,
        Env {
            clock: Box::new(clock.clone()),
            link: Box::new(NoLink),
            scan,
            power: Box::new(power.clone()),
            notify,
            locale: EN,
            view: view.clone(),
        },
    )
    .with_wall_clock(Box::new(SimWallClock::new(WALL_START_US)));
    service.apply_event(&DaemonEvent::Link(Link::Online));
    (service, notifications)
}

fn outcomes(view: &SharedView) -> Vec<Outcome> {
    view.lock()
        .unwrap()
        .log
        .iter()
        .map(|row| row.outcome)
        .collect()
}

/// The countdown that ends with its Graph has the outcome `cancelled`, one log row, and its
/// notification closes. It gives its count for the Response pause back.
#[test]
fn disable_in_the_countdown_logs_cancelled_and_closes_the_notification() {
    let dir = Dir::new("rv2-countdown-disable-log");
    let (clock, view, power) = (SimClock::default(), shared_view(), Power::default());
    let (mut service, mut notifications) = service_with(
        &dir,
        response_graph("poweroff"),
        Box::new(NoScan),
        &clock,
        &view,
        &power,
    );
    for _ in 0..2 {
        clock.advance(Duration::from_secs(1));
        service.poll();
    }
    assert!(view.lock().unwrap().countdown.is_some());
    assert_eq!(service.store().state().pause.count, 1);

    service.handle(GraphCommand::Enable {
        graph: "graph_1".into(),
        enabled: false,
    });
    assert_eq!(view.lock().unwrap().countdown, None);
    assert_eq!(outcomes(&view), [Outcome::Cancelled]);
    assert_eq!(service.store().state().pause.count, 0);
    let plans: Vec<Plan> = std::iter::from_fn(|| notifications.try_recv().ok()).collect();
    assert_eq!(
        plans.last(),
        Some(&Plan::Close(NOTIFICATION_KEY.to_owned())),
        "the countdown notification closes"
    );
}

/// A Graph that panics in a pass is stopped. Its countdown must not end in a power call.
#[test]
fn panic_in_the_countdown_makes_no_power_call() {
    let dir = Dir::new("rv2-countdown-panic");
    let (clock, view, power) = (SimClock::default(), shared_view(), Power::default());
    let scan = Scan::default();
    // No process `nicecooling-none` runs, so the not node gives 1.
    let mut graph = Graph::new("graph_1", "Overheat");
    graph.nodes = vec![
        Node::new(1, Kind::Process).with_param("name", "nicecooling-none"),
        Node::new(2, Kind::Not),
        Node::new(3, Kind::ResponseEnd)
            .with_param("kind", "poweroff")
            .with_param("countdown_s", 10)
            .with_param("min_interval_s", 0),
    ];
    graph.connections = vec![Connection::new(1, 2, "a"), Connection::new(2, 3, "a")];
    let (mut service, _notifications) =
        service_with(&dir, graph, Box::new(scan.clone()), &clock, &view, &power);
    for _ in 0..2 {
        clock.advance(Duration::from_secs(1));
        service.poll();
    }
    assert!(view.lock().unwrap().countdown.is_some());

    scan.0.store(true, Ordering::SeqCst);
    clock.advance(Duration::from_secs(1));
    service.poll();
    assert!(
        service
            .store()
            .graph("graph_1")
            .is_some_and(|graph| !graph.enabled),
        "the Graph is stopped"
    );
    assert_eq!(view.lock().unwrap().countdown, None);
    assert_eq!(outcomes(&view), [Outcome::Cancelled]);
    for _ in 0..15 {
        clock.advance(Duration::from_secs(1));
        service.poll();
    }
    assert_eq!(*power.0.lock().unwrap(), Vec::new());
}

/// The countdown belongs to its Graph only: a change of another Graph leaves it.
#[test]
fn apply_of_another_graph_leaves_the_countdown() {
    let mut rig = Rig::new("rv2-countdown-other");
    rig.start_countdown();
    let mut other = response_graph("notification");
    other.id = "graph_2".into();
    other.name = "Other".into();
    rig.service.handle(GraphCommand::Apply(Box::new(other)));
    rig.service.handle(GraphCommand::Enable {
        graph: "graph_2".into(),
        enabled: false,
    });
    rig.service.handle(GraphCommand::Delete {
        graph: "graph_2".into(),
    });
    assert!(rig.view.lock().unwrap().countdown.is_some());
    rig.ticks(15);
    assert_eq!(rig.calls(), [PowerKind::Poweroff]);
}
