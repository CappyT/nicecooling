//! Review of the stop path of the Graph service against a Graph sensor call that the daemon
//! answers while "Quit" waits.
//!
//! The answers of the deferred daemon calls travel on the channel of the stop message. The service
//! loop takes one input and then polls, and a poll ends a countdown that is due. So an answer that
//! came before the stop message lets the countdown end, and the power call happens, before the
//! service reads the stop message.

mod support;

use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling::graphs::channel::{GraphChannel, GraphCommand, QUIT_WAIT, TryRequest};
use nicecooling::graphs::service::{
    Env, GraphService, NoLink, PowerCall, SensorCall, SensorDone, SensorLink,
};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::view::shared_view;
use nicecooling::i18n::EN;
use nicecooling::model::{DaemonEvent, Link};
use nicecooling::services::spawn_service;
use nicecooling_core::clock::SimClock;
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};
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
    fn call(&mut self, kind: PowerKind) -> Result<(), String> {
        self.0.lock().unwrap().push(kind);
        Ok(())
    }
}

/// A link that makes its Graph sensor calls on a task, as the link to the daemon does. The test
/// holds the answer of each call and gives it when it wants. The values call blocks until the
/// test drops the sender of `hold`: a slow pass.
struct DeferredLink {
    hold: std_mpsc::Receiver<()>,
    calls: Arc<Mutex<Vec<(SensorCall, SensorDone)>>>,
}

impl SensorLink for DeferredLink {
    fn values(&mut self, _values: &[(String, Option<f64>)]) {
        let _ = self.hold.recv();
    }

    fn deferred(&self) -> bool {
        true
    }

    fn start(&mut self, call: SensorCall, done: SensorDone) {
        self.calls.lock().unwrap().push((call, done));
    }
}

/// The spec (graph-responses, Stop of the app): "The stop message SHALL come before every event,
/// command and timer, so a countdown that ended while "Quit" waited makes no power call."
///
/// A Graph sensor call of an Apply waits for the daemon. A pass is slow. While the pass runs, the
/// countdown ends, the daemon answers the call, and the user chooses "Quit". The service must read
/// the stop message first and make no power call.
#[test]
fn answer_of_the_daemon_before_quit_makes_no_power_call() {
    assert_eq!(quit_after_a_slow_pass(true), Vec::<PowerKind>::new());
}

/// The control, and the scenario "Countdown ends while Quit waits": the same steps with no answer
/// of the daemon, and a daemon event that waits with the stop message, make no power call.
#[test]
fn quit_during_a_slow_pass_makes_no_power_call() {
    assert_eq!(quit_after_a_slow_pass(false), Vec::<PowerKind>::new());
}

/// The power calls after a Quit that comes while a pass runs and the countdown ends. With
/// `answer`, the daemon answers the call of an Apply before the Quit.
fn quit_after_a_slow_pass(answer: bool) -> Vec<PowerKind> {
    let dir = Dir::new("rv2-stop-answer");
    let mut store = GraphStore::open(dir.graph_paths());
    store.put(constant_graph("graph_1", 1.0)).unwrap();
    let clock = SimClock::default();
    let (notify, _notifications) = mpsc::unbounded_channel();
    let power = Power::default();
    let (release, hold) = std_mpsc::channel::<()>();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let (channel, inputs) = GraphChannel::new();
    let mut service = GraphService::new(
        store,
        Env {
            clock: Box::new(clock.clone()),
            link: Box::new(DeferredLink {
                hold,
                calls: calls.clone(),
            }),
            scan: Box::new(NoScan),
            power: Box::new(power.clone()),
            notify,
            locale: EN,
            view: shared_view(),
        },
    )
    .with_answers(channel.answers());
    service.apply_event(&DaemonEvent::Link(Link::Online));

    // An Apply of a Graph with a sensor end node: the call that makes its sensor waits.
    service.handle(GraphCommand::Apply(Box::new(constant_graph(
        "graph_2", 5.0,
    ))));
    assert_eq!(calls.lock().unwrap().len(), 1, "the Apply started one call");
    assert!(service.start_countdown("graph_1", PowerKind::Poweroff, 10));

    let (events, _keep) = broadcast::channel(8);
    spawn_service(service, events.subscribe(), inputs);
    // The first pass is now inside the values call.
    std::thread::sleep(Duration::from_millis(200));

    // The countdown ends while the pass runs, the daemon answers, and then the user quits.
    clock.advance(Duration::from_secs(10));
    let (_, done) = calls.lock().unwrap().pop().expect("the call of the Apply");
    if answer {
        done(Ok(Some("graph_9".to_owned())));
    }
    let quit = {
        let channel = channel.clone();
        std::thread::spawn(move || channel.stop(QUIT_WAIT))
    };
    std::thread::sleep(Duration::from_millis(100));
    let _ = events.send(DaemonEvent::Link(Link::Online));
    drop(release);

    assert!(quit.join().unwrap(), "the service answered the stop");
    power.0.lock().unwrap().clone()
}

/// A scan of `/proc` that blocks until the test drops the sender of `hold`.
struct SlowScan(Mutex<std_mpsc::Receiver<()>>);

impl ProcessSource for SlowScan {
    fn scan(&self) -> ProcessList {
        let _ = self.0.lock().unwrap().recv();
        ProcessList::default()
    }
}

/// The same rule for a command. The window sends a Try of a draft with a process node, and the
/// scan of `/proc` for it is slow. While the service handles the Try, the countdown ends and the
/// user chooses "Quit".
#[test]
fn command_handled_before_quit_makes_no_power_call() {
    let dir = Dir::new("rv2-stop-command");
    let mut store = GraphStore::open(dir.graph_paths());
    store.put(constant_graph("graph_1", 1.0)).unwrap();
    let clock = SimClock::default();
    let (notify, _notifications) = mpsc::unbounded_channel();
    let power = Power::default();
    let (release, hold) = std_mpsc::channel::<()>();
    let (channel, inputs) = GraphChannel::new();
    let mut service = GraphService::new(
        store,
        Env {
            clock: Box::new(clock.clone()),
            link: Box::new(NoLink),
            scan: Box::new(SlowScan(Mutex::new(hold))),
            power: Box::new(power.clone()),
            notify,
            locale: EN,
            view: shared_view(),
        },
    );
    service.apply_event(&DaemonEvent::Link(Link::Online));
    assert!(service.start_countdown("graph_1", PowerKind::Poweroff, 10));

    let (events, _keep) = broadcast::channel(8);
    spawn_service(service, events.subscribe(), inputs);

    let mut draft = Graph::new("graph_3", "Draft");
    draft.nodes = vec![
        Node::new(1, Kind::Process).with_param("name", "nicecooling-none"),
        Node::new(2, Kind::SensorEnd),
    ];
    draft.connections = vec![Connection::new(1, 2, "a")];
    assert!(channel.send(GraphCommand::Try(Box::new(TryRequest {
        graph: draft,
        tests: Vec::new(),
        seconds: 5,
    }))));
    // The service now handles the Try and waits in the scan.
    std::thread::sleep(Duration::from_millis(200));

    clock.advance(Duration::from_secs(10));
    let quit = {
        let channel = channel.clone();
        std::thread::spawn(move || channel.stop(QUIT_WAIT))
    };
    std::thread::sleep(Duration::from_millis(100));
    drop(release);

    assert!(quit.join().unwrap(), "the service answered the stop");
    assert_eq!(
        *power.0.lock().unwrap(),
        Vec::<PowerKind>::new(),
        "no power call after Quit"
    );
}
