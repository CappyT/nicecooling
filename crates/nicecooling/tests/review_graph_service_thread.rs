//! Review of the service thread: the stop path against a countdown, a panic outside a Graph, and
//! the bound of the command channel.

mod support;

use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nicecooling::graphs::channel::{GraphChannel, GraphCommand, QUIT_WAIT, TryRequest};
use nicecooling::graphs::service::{Env, GraphService, NoPower, PowerCall, SensorLink};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::view::{SharedView, shared_view};
use nicecooling::i18n::EN;
use nicecooling::model::{DaemonEvent, Link};
use nicecooling::notify::Plan;
use nicecooling::services::spawn_service;
use nicecooling_core::clock::SimClock;
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::model::Graph;
use nicecooling_graph::pause::PowerKind;
use support::graphs::{Dir, constant_graph};
use tokio::sync::{broadcast, mpsc};

struct NoScan;

impl ProcessSource for NoScan {
    fn scan(&self) -> ProcessList {
        ProcessList::default()
    }
}

#[derive(Clone, Default)]
struct Power(Arc<Mutex<Vec<PowerKind>>>);

impl PowerCall for Power {
    fn call(&mut self, kind: PowerKind) {
        self.0.lock().unwrap().push(kind);
    }
}

/// A link that blocks the pass until the test drops the sender. After that it returns at once.
struct SlowLink(std_mpsc::Receiver<()>);

impl SensorLink for SlowLink {
    fn values(&mut self, _values: &[(String, Option<f64>)]) {
        let _ = self.0.recv();
    }
}

/// A link that panics: a bug outside the evaluation of a Graph.
struct PanicLink;

impl SensorLink for PanicLink {
    fn values(&mut self, _values: &[(String, Option<f64>)]) {
        panic!("the daemon link failed");
    }
}

struct Parts {
    service: GraphService,
    clock: SimClock,
    view: SharedView,
    notifications: mpsc::UnboundedReceiver<Plan>,
    _dir: Dir,
}

fn parts(
    name: &str,
    graphs: Vec<Graph>,
    link: Box<dyn SensorLink>,
    power: Box<dyn PowerCall>,
) -> Parts {
    let dir = Dir::new(name);
    let mut store = GraphStore::open(dir.graph_paths());
    for graph in graphs {
        store.put(graph).unwrap();
    }
    let clock = SimClock::default();
    let view = shared_view();
    let (notify, notifications) = mpsc::unbounded_channel();
    let service = GraphService::new(
        store,
        Env {
            clock: Box::new(clock.clone()),
            link,
            scan: Box::new(NoScan),
            power,
            notify,
            locale: EN,
            view: view.clone(),
        },
    );
    Parts {
        service,
        clock,
        view,
        notifications,
        _dir: dir,
    }
}

/// "Quit" arrives 1 s before the end of a power countdown, while the service is busy with a pass
/// (a slow disk, a slow daemon call). When the service is free again, the stop message, a daemon
/// event and the end of the countdown are all due. The stop must win: the spec says that "Quit"
/// in a countdown makes no power call. One round has a chance of 1/3 to pass by luck, so the test
/// runs 12 rounds.
#[test]
fn quit_while_the_service_is_busy_makes_no_power_call() {
    let mut calls = 0;
    for round in 0..12 {
        let power = Power::default();
        let (release, hold) = std_mpsc::channel::<()>();
        let mut parts = parts(
            &format!("rv-quit-race-{round}"),
            vec![constant_graph("graph_1", 1.0)],
            Box::new(SlowLink(hold)),
            Box::new(power.clone()),
        );
        assert!(
            parts
                .service
                .start_countdown("graph_1", PowerKind::Poweroff, 10)
        );
        let (channel, inputs) = GraphChannel::new();
        let (events, _keep) = broadcast::channel(8);
        spawn_service(parts.service, events.subscribe(), inputs);
        // The first pass is now inside the link.
        std::thread::sleep(Duration::from_millis(100));

        parts.clock.advance(Duration::from_secs(9));
        let quit = {
            let channel = channel.clone();
            std::thread::spawn(move || channel.stop(QUIT_WAIT))
        };
        std::thread::sleep(Duration::from_millis(50));
        // The service is still busy when the countdown ends and the next event arrives.
        parts.clock.advance(Duration::from_secs(1));
        events.send(DaemonEvent::Link(Link::Online)).unwrap();
        drop(release);
        assert!(quit.join().unwrap(), "the service answered the stop");
        calls += power.0.lock().unwrap().len();
        drop(parts.notifications);
    }
    assert_eq!(calls, 0, "power calls after Quit in 12 rounds");
}
