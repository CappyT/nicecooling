mod support;

use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui::TestAppContext;
use nicecooling::app::{self, Backend, StartMode};
use nicecooling::command::Command;
use nicecooling::daemon_client::DaemonClient;
use nicecooling::graphs::channel::{
    COMMAND_BOUND, GraphChannel, GraphCommand, Input, QUIT_WAIT, SendError,
};
use nicecooling::graphs::service::{Env, GraphService, NoLink, NoPower, PowerCall, SensorLink};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::view::{Countdown, SharedView, shared_view};
use nicecooling::i18n::EN;
use nicecooling::model::{DaemonEvent, Link, SensorSample};
use nicecooling::notify::{Level, NotificationSpec, Plan};
use nicecooling::services::{spawn_notifications, spawn_service, spawn_sigterm};
use nicecooling::tray::{Tray, TrayView};
use nicecooling_core::clock::SimClock;
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};
use nicecooling_graph::pause::PowerKind;
use support::bus::PrivateBus;
use support::fake_daemon::{FakeDaemon, Options};
use support::fake_notification_server::FakeNotificationServer;
use support::fake_tray_host::FakeTrayHost;
use support::fixtures::{NOW_US, online};
use support::graphs::{Dir, constant_graph};
use support::{SESSION_LOCK, use_as_session_bus};
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

/// A link that blocks the pass until the test drops the sender.
struct StuckLink(std_mpsc::Receiver<()>);

impl SensorLink for StuckLink {
    fn values(&mut self, _values: &[(String, Option<f64>)]) {
        let _ = self.0.recv();
    }
}

struct Parts {
    service: GraphService,
    clock: SimClock,
    view: SharedView,
    notifications: mpsc::UnboundedReceiver<Plan>,
    dir: Dir,
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
        dir,
    }
}

fn backend(
    runtime: &tokio::runtime::Runtime,
    client: Option<DaemonClient>,
    graphs: Option<GraphChannel>,
) -> (
    Backend,
    broadcast::Sender<DaemonEvent>,
    mpsc::UnboundedSender<Command>,
) {
    let (events, events_rx) = broadcast::channel(64);
    let (commands, inbox) = mpsc::unbounded_channel();
    let backend = Backend {
        events: events_rx,
        commands: commands.clone(),
        inbox,
        client,
        graphs,
        runtime: runtime.handle().clone(),
        language: nicecooling::i18n::Language::English,
        settings: Default::default(),
    };
    (backend, events, commands)
}

#[gpui::test]
fn graph_commands_reach_the_service_not_the_daemon_client(cx: &mut TestAppContext) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (channel, mut inputs) = GraphChannel::new();
    let (backend, _events, commands) = backend(&runtime, None, Some(channel));
    cx.update(|cx| app::init(cx, StartMode::Background, backend));

    let graph = constant_graph("graph_1", 5.0);
    let sent = [
        GraphCommand::Apply(Box::new(graph.clone())),
        GraphCommand::ResponsesOn,
    ];
    for command in &sent {
        commands.send(Command::Graph(command.clone())).unwrap();
    }
    cx.run_until_parked();
    let received: Vec<GraphCommand> = std::iter::from_fn(|| inputs.try_recv().ok())
        .map(|input| match input {
            Input::Command(command) => command,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(received, sent);

    // The daemon client makes no call for a Graph command, even when one reaches it.
    let bus = PrivateBus::start("graph-links-routing");
    let (daemon, client) = runtime.block_on(async {
        let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
        (daemon, DaemonClient::new(bus.connect().await))
    });
    for command in sent {
        let done = runtime.block_on(Command::Graph(command).run_on_daemon(&client));
        assert!(!done.unwrap());
    }
    assert_eq!(daemon.calls(), Vec::<String>::new());
}

#[tokio::test(flavor = "multi_thread")]
async fn tray_cancel_reaches_the_service() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("graph-links-tray");
    use_as_session_bus(&bus);
    let host = FakeTrayHost::start(bus.connect().await).await;

    let countdown = Countdown {
        graph_id: "graph_1".into(),
        graph_name: "Hot water".into(),
        kind: PowerKind::Poweroff,
        seconds_left: 30,
        total_s: 30,
    };
    let view = TrayView::build(&online(), NOW_US, EN).with_graph_countdown(Some(&countdown), EN);
    let (tx, mut commands) = mpsc::unbounded_channel();
    let tray = tokio::task::spawn_blocking(move || Tray::spawn(tx, view).expect("tray"))
        .await
        .unwrap();
    let name = host.item_name().await;
    let menu = host.layout(&name).await;
    let entry = menu
        .find("Cancel the countdown of Graph Hot water (0:30)")
        .expect("the cancel entry");
    host.click(&name, entry.id).await;

    // The tray sends a Graph command, and the app routes it to the service.
    let command = commands.recv().await.expect("a command");
    assert_eq!(command, Command::Graph(GraphCommand::CancelCountdown));
    let (channel, mut inputs) = GraphChannel::new();
    assert_eq!(channel.route(command), None);
    assert!(matches!(
        inputs.try_recv(),
        Ok(Input::Command(GraphCommand::CancelCountdown))
    ));
    // A command of the daemon does not go to the service.
    assert_eq!(
        channel.route(Command::ReturnToAutomatic),
        Some(Command::ReturnToAutomatic)
    );
    assert!(inputs.try_recv().is_err());
    tokio::task::spawn_blocking(move || tray.shutdown())
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn notification_request_reaches_the_notification_thread() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("graph-links-notify");
    use_as_session_bus(&bus);
    let server = FakeNotificationServer::start(bus.connect().await, &["body"]).await;

    let (events, events_rx) = broadcast::channel(16);
    let (requests, requests_rx) = mpsc::unbounded_channel();
    let (commands, _inbox) = mpsc::unbounded_channel();
    spawn_notifications(
        events_rx,
        requests_rx,
        commands,
        EN,
        Arc::new(Mutex::new(nicecooling::notify::NotifyPrefs::default())),
    );
    requests
        .send(Plan::Show(NotificationSpec {
            key: "graph-stopped-graph_2".into(),
            title: "Graph Hot water stopped".into(),
            body: "An error stopped the Graph.".into(),
            level: Level::Normal,
            buttons: Vec::new(),
            sound: None,
        }))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while server.received().is_empty() && Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let received = server.received();
    assert_eq!(received.len(), 1, "{received:?}");
    assert_eq!(received[0].summary, "Graph Hot water stopped");

    // The thread goes on with the daemon events after every request sender is gone.
    drop(requests);
    events.send(DaemonEvent::Link(Link::Online)).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(events);
}

/// A Graph that stores the sensor `x` in a persistent storage node.
fn storing_graph() -> Graph {
    let mut graph = Graph::new("graph_1", "Store");
    graph.nodes = vec![
        Node::new(1, Kind::SensorSource).with_param("sensor", "x"),
        Node::new(3, Kind::Constant).with_param("value", 1.0),
        Node::new(4, Kind::PersistentStorage),
        Node::new(2, Kind::SensorEnd),
    ];
    graph.connections = vec![
        Connection::new(3, 4, "s"),
        Connection::new(1, 4, "a"),
        Connection::new(4, 2, "a"),
    ];
    graph
}

fn sensor_x(value: f64) -> DaemonEvent {
    DaemonEvent::Sensors(SensorSample {
        time_us: 1,
        sensors: vec![("x".into(), value, 0)],
        channels: Vec::new(),
    })
}

/// Advance the clock and send the sensor until the view shows `value` in node 4.
fn settle(
    clock: &SimClock,
    events: &broadcast::Sender<DaemonEvent>,
    view: &SharedView,
    value: f64,
) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        clock.advance(Duration::from_secs(1));
        events.send(DaemonEvent::Link(Link::Online)).unwrap();
        events.send(sensor_x(value)).unwrap();
        std::thread::sleep(Duration::from_millis(30));
        let shown = view
            .lock()
            .unwrap()
            .row("graph_1")
            .and_then(|row| row.values.get(&4).copied().flatten());
        if shown == Some(value) {
            return;
        }
        assert!(Instant::now() < deadline, "the view never showed {value}");
    }
}

fn stored_value(dir: &Dir) -> Option<f64> {
    let text = std::fs::read_to_string(dir.graph_paths().state).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    json["values"]["graph_1/4"].as_f64()
}

#[gpui::test]
fn quit_writes_the_state_and_quits_within_2s(cx: &mut TestAppContext) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let parts = parts(
        "quit",
        vec![storing_graph()],
        Box::new(NoLink),
        Box::new(NoPower),
    );
    let (channel, inputs) = GraphChannel::new();
    let (backend, events, commands) = backend(&runtime, None, Some(channel.clone()));
    spawn_service(parts.service, events.subscribe(), inputs);
    cx.update(|cx| app::init(cx, StartMode::Background, backend));

    settle(&parts.clock, &events, &parts.view, 1.0);
    // The node had a stored value from its start: the first write is at once, and later ones wait
    // for the 30 s of the write rate.
    settle(&parts.clock, &events, &parts.view, 2.0);
    assert_ne!(stored_value(&parts.dir), Some(2.0), "the write is not due");

    let start = Instant::now();
    commands.send(Command::Quit).unwrap();
    while !channel.is_closed() {
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(10));
        assert!(start.elapsed() < QUIT_WAIT, "the service did not stop");
    }
    assert_eq!(stored_value(&parts.dir), Some(2.0));
}

#[test]
fn stuck_service_does_not_hold_the_quit_beyond_2s() {
    let (release, hold) = std_mpsc::channel::<()>();
    let parts = parts(
        "stuck",
        vec![constant_graph("graph_1", 1.0)],
        Box::new(StuckLink(hold)),
        Box::new(NoPower),
    );
    let (channel, inputs) = GraphChannel::new();
    let (events, _) = broadcast::channel(8);
    spawn_service(parts.service, events.subscribe(), inputs);
    // The first pass blocks in the link.
    std::thread::sleep(Duration::from_millis(200));

    let start = Instant::now();
    assert!(!channel.stop(QUIT_WAIT), "a stuck service does not answer");
    let waited = start.elapsed();
    assert!(
        waited >= QUIT_WAIT && waited < QUIT_WAIT + Duration::from_millis(500),
        "{waited:?}"
    );
    drop(release);
}

#[test]
fn quit_in_a_countdown_makes_no_power_call() {
    let power = Power::default();
    let mut parts = parts(
        "countdown",
        vec![constant_graph("graph_1", 1.0)],
        Box::new(NoLink),
        Box::new(power.clone()),
    );
    assert!(
        parts
            .service
            .start_countdown("graph_1", PowerKind::Poweroff, 30)
    );
    assert!(
        !parts
            .service
            .start_countdown("graph_1", PowerKind::Reboot, 30),
        "one countdown at a time"
    );
    parts.clock.advance(Duration::from_secs(10));
    parts.service.poll();
    assert_eq!(
        parts
            .view
            .lock()
            .unwrap()
            .countdown
            .as_ref()
            .unwrap()
            .seconds_left,
        20
    );

    let (channel, inputs) = GraphChannel::new();
    let (events, _) = broadcast::channel(8);
    spawn_service(parts.service, events.subscribe(), inputs);
    assert!(channel.stop(QUIT_WAIT));
    assert_eq!(*power.0.lock().unwrap(), []);
    assert_eq!(parts.view.lock().unwrap().countdown, None);
    drop(parts.notifications);
}

#[test]
fn countdown_end_makes_the_power_call() {
    let power = Power::default();
    let mut parts = parts(
        "countdown-end",
        vec![constant_graph("graph_1", 1.0)],
        Box::new(NoLink),
        Box::new(power.clone()),
    );
    assert!(
        parts
            .service
            .start_countdown("graph_1", PowerKind::Reboot, 30)
    );
    parts.clock.advance(Duration::from_secs(29));
    parts.service.poll();
    assert_eq!(*power.0.lock().unwrap(), []);
    parts.clock.advance(Duration::from_secs(1));
    parts.service.poll();
    assert_eq!(*power.0.lock().unwrap(), [PowerKind::Reboot]);
    assert_eq!(parts.view.lock().unwrap().countdown, None);

    // A cancel gives no call.
    assert!(
        parts
            .service
            .start_countdown("graph_1", PowerKind::Poweroff, 5)
    );
    parts.service.handle(GraphCommand::CancelCountdown);
    parts.clock.advance(Duration::from_secs(10));
    parts.service.poll();
    assert_eq!(power.0.lock().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn sigterm_takes_the_quit_path() {
    let (commands, mut inbox) = mpsc::unbounded_channel();
    spawn_sigterm(&tokio::runtime::Handle::current(), commands).expect("the handler");
    // The handler is in place: the signal gives a command and does not end the test process.
    let status = std::process::Command::new("kill")
        .args(["-TERM", &std::process::id().to_string()])
        .status()
        .expect("kill");
    assert!(status.success());
    let command = tokio::time::timeout(Duration::from_secs(10), inbox.recv())
        .await
        .expect("a command in 10 s");
    assert_eq!(command, Some(Command::Quit));
}

#[test]
fn full_channel_refuses_a_command_and_still_takes_the_stop() {
    let view = nicecooling::graphs::view::shared_view();
    let (channel, mut inputs) = GraphChannel::new();
    let channel = channel.with_view(view.clone());
    for _ in 0..COMMAND_BOUND {
        assert_eq!(channel.try_send(GraphCommand::ResponsesOn), Ok(()));
    }
    assert_eq!(
        channel.try_send(GraphCommand::ResponsesOn),
        Err(SendError::Full)
    );
    // A Graph command that the app routes into the full channel shows a refusal.
    assert_eq!(
        channel.route(Command::Graph(GraphCommand::ResponsesOn)),
        None
    );
    assert!(view.lock().unwrap().refusal.is_some());

    // The stop has a path of its own and comes first.
    let stopper = {
        let channel = channel.clone();
        std::thread::spawn(move || channel.stop(Duration::from_secs(1)))
    };
    std::thread::sleep(Duration::from_millis(200));
    let done = match inputs.try_recv() {
        Ok(Input::Stop(done)) => done,
        other => panic!("the first message is {other:?}"),
    };
    done.send(()).unwrap();
    assert!(stopper.join().unwrap());

    // A service that is gone gives no answer to a stop, and takes no command.
    drop(inputs);
    assert!(channel.is_closed());
    assert_eq!(
        channel.try_send(GraphCommand::ResponsesOn),
        Err(SendError::Closed)
    );
    assert!(!channel.stop(Duration::from_secs(1)));
}

/// The cancel of a countdown does not wait behind the 256 commands of a full channel, and it is
/// not refused: a full channel never lets a power call go ahead.
#[test]
fn cancel_gets_through_a_full_channel() {
    let power = Power::default();
    let mut parts = parts(
        "cancel-full-channel",
        vec![constant_graph("graph_1", 1.0)],
        Box::new(NoLink),
        Box::new(power.clone()),
    );
    assert!(
        parts
            .service
            .start_countdown("graph_1", PowerKind::Poweroff, 30)
    );
    let (channel, mut inputs) = GraphChannel::new();
    for _ in 0..COMMAND_BOUND {
        assert_eq!(channel.try_send(GraphCommand::ResponsesOn), Ok(()));
    }
    assert_eq!(
        channel.try_send(GraphCommand::ResponsesOn),
        Err(SendError::Full)
    );

    // The button of the notification, of the tray and of the window sends the cancel this way.
    assert_eq!(
        channel.route(Command::Graph(GraphCommand::CancelCountdown)),
        None
    );
    match inputs.try_recv() {
        Ok(Input::Command(command)) => {
            assert_eq!(command, GraphCommand::CancelCountdown);
            parts.service.handle(command);
        }
        other => panic!("the first message is {other:?}"),
    }
    parts.clock.advance(Duration::from_secs(30));
    parts.service.poll();
    assert_eq!(*power.0.lock().unwrap(), []);
    assert_eq!(parts.view.lock().unwrap().countdown, None);
}

/// A link that panics: a bug outside the evaluation of a Graph.
struct PanicLink;

impl SensorLink for PanicLink {
    fn values(&mut self, _values: &[(String, Option<f64>)]) {
        panic!("the daemon link failed");
    }
}

#[test]
fn panic_outside_a_graph_ends_the_service_and_the_stop_reports_it() {
    let mut parts = parts(
        "panic-stop",
        vec![constant_graph("graph_1", 1.0)],
        Box::new(PanicLink),
        Box::new(NoPower),
    );
    let (channel, inputs) = GraphChannel::new();
    let (events, _) = broadcast::channel(8);
    spawn_service(parts.service, events.subscribe(), inputs);
    let deadline = Instant::now() + Duration::from_secs(5);
    while !channel.is_closed() {
        assert!(Instant::now() < deadline, "the service did not end");
        std::thread::sleep(Duration::from_millis(20));
    }
    // The state was not written, and the stop says so.
    assert!(!channel.stop(QUIT_WAIT));
    let error = parts.view.lock().unwrap().error.clone().expect("an error");
    assert!(error.contains("the daemon link failed"), "{error}");
    let Plan::Show(spec) = parts.notifications.try_recv().expect("a notification") else {
        panic!("a notification to show");
    };
    assert_eq!(spec.title, "Graph service stopped");
    assert_eq!(spec.key, "graph-service-stopped");
}
