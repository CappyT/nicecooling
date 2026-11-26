//! Review of the active-session rule against the state machine of a response end node.
//!
//! An app of a session that is not active steps no response end node. The machine keeps the
//! trigger time of the input from before the switch. When the session is active again, the hold
//! time counts from that old trigger time, also when the input was false for most of that time.

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use nicecooling::graphs::service::{Env, GraphService, NoLink, NoPower};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::values::ActiveSession;
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

#[derive(Clone)]
struct Switch(Arc<AtomicBool>);

impl ActiveSession for Switch {
    fn active(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// A poweroff response with a hold time of 60 s on the sensor `s1`.
fn hold_graph() -> Graph {
    let mut graph = Graph::new("graph_1", "Overheat");
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

struct Rig {
    service: GraphService,
    clock: SimClock,
    view: SharedView,
    session: Switch,
    _dir: Dir,
}

impl Rig {
    fn new(name: &str) -> Self {
        let dir = Dir::new(name);
        let mut store = GraphStore::open(dir.graph_paths());
        store.put(hold_graph()).unwrap();
        let clock = SimClock::default();
        let view = shared_view();
        let session = Switch(Arc::new(AtomicBool::new(true)));
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
        .with_wall_clock(Box::new(SimWallClock::new(WALL_START_US)))
        .with_session(Box::new(session.clone()));
        service.apply_event(&DaemonEvent::Link(Link::Online));
        Self {
            service,
            clock,
            view,
            session,
            _dir: dir,
        }
    }

    fn feed(&mut self, value: f64) {
        self.service
            .apply_event(&DaemonEvent::Sensors(SensorSample {
                time_us: 1,
                sensors: vec![("s1".into(), value, 0)],
                channels: Vec::new(),
            }));
    }

    fn ticks(&mut self, seconds: u32) {
        for _ in 0..seconds {
            self.clock.advance(Duration::from_secs(1));
            self.service.poll();
        }
    }

    fn countdown(&self) -> bool {
        self.view.lock().unwrap().countdown.is_some()
    }
}

/// The spec (graph-responses, Response state machine): "In the state "not run", when the trigger
/// time plus the hold time is not after now, the node SHALL run its response". The input of this
/// test is true for 10 s, false for 1 h while the session is not active, and true for 2 s after
/// the switch back: it never stays true for the hold time of 60 s.
#[test]
fn hold_time_does_not_count_the_time_of_an_inactive_session() {
    let mut rig = Rig::new("rv2-session-hold");
    rig.feed(1.0);
    rig.ticks(10);
    assert!(!rig.countdown(), "the hold time is not over");

    // The user switches to another session. The input falls.
    rig.session.0.store(false, Ordering::SeqCst);
    rig.feed(0.0);
    for _ in 0..60 {
        rig.ticks(60);
    }

    // The user switches back. The input is true again for 2 s.
    rig.session.0.store(true, Ordering::SeqCst);
    rig.feed(1.0);
    rig.ticks(2);
    assert!(
        !rig.countdown(),
        "a poweroff countdown started after 2 s of a true input with a hold time of 60 s"
    );
}
