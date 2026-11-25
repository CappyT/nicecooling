mod support;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling::command::Command;
use nicecooling::graphs::channel::{GraphChannel, GraphCommand, Input};
use nicecooling::graphs::power::{LogindPower, PowerCall, countdown_notification, countdown_text};
use nicecooling::graphs::responses::Outcome;
use nicecooling::graphs::service::{Env, GraphService, NoLink};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::view::{Countdown, SharedView, shared_view};
use nicecooling::i18n::{EN, IT};
use nicecooling::model::{DaemonEvent, Link, Response, SensorSample};
use nicecooling::notify::{Level, NotificationSpec, Notifier, Plan};
use nicecooling::screens::alarms::CountdownDialog;
use nicecooling::tray::{Tray, TrayView};
use nicecooling_core::clock::{SimClock, SimWallClock};
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};
use nicecooling_graph::pause::PowerKind;
use nicecooling_graph::response::RunState;
use support::bus::PrivateBus;
use support::fake_logind::{FakeLogind, PowerCall as LogindCall};
use support::fake_notification_server::FakeNotificationServer;
use support::fake_tray_host::FakeTrayHost;
use support::fixtures::{NOW_US, alarm, online};
use support::graphs::Dir;
use support::{SESSION_LOCK, use_as_session_bus};
use tokio::runtime::Handle;
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

/// The wall clock at the start of a test: 2025-12-17 19:33:20 UTC.
const WALL_START_US: u64 = 1_766_000_000_000_000;

struct NoScan;

impl ProcessSource for NoScan {
    fn scan(&self) -> ProcessList {
        ProcessList::default()
    }
}

/// A power call that records the kind, and the content of the state file at the time of the call.
#[derive(Clone)]
struct FakePower {
    calls: Arc<Mutex<Vec<PowerKind>>>,
    files: Arc<Mutex<Vec<Option<serde_json::Value>>>>,
    state_path: PathBuf,
}

impl FakePower {
    fn new(state_path: PathBuf) -> Self {
        Self {
            calls: Arc::default(),
            files: Arc::default(),
            state_path,
        }
    }

    fn calls(&self) -> Vec<PowerKind> {
        self.calls.lock().unwrap().clone()
    }

    fn files(&self) -> Vec<Option<serde_json::Value>> {
        self.files.lock().unwrap().clone()
    }
}

impl PowerCall for FakePower {
    fn call(&mut self, kind: PowerKind) -> Result<(), String> {
        self.calls.lock().unwrap().push(kind);
        let file = std::fs::read_to_string(&self.state_path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok());
        self.files.lock().unwrap().push(file);
        Ok(())
    }
}

/// A Graph whose response end node is a power response. It fires while the sensor `sensor` is
/// above 0.
fn power_graph(
    id: &str,
    name: &str,
    sensor: &str,
    kind: &str,
    countdown: u32,
    repeat: Option<u32>,
) -> Graph {
    let mut end = Node::new(2, Kind::ResponseEnd)
        .with_param("kind", kind)
        .with_param("countdown_s", countdown)
        .with_param("min_interval_s", 0);
    if let Some(repeat) = repeat {
        end = end.with_param("repeat_s", repeat);
    }
    let mut graph = Graph::new(id, name);
    graph.nodes = vec![
        Node::new(1, Kind::SensorSource).with_param("sensor", sensor),
        end,
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    graph
}

/// The Graph service with a simulated clock and a simulated wall clock.
struct Rig {
    service: GraphService,
    clock: SimClock,
    view: SharedView,
    power: FakePower,
    dir: Dir,
    notifications: UnboundedReceiver<Plan>,
}

impl Rig {
    fn new(name: &str, graphs: Vec<Graph>) -> Self {
        let dir = Dir::new(name);
        let power = FakePower::new(dir.graph_paths().state);
        Self::with_power(dir, graphs, power.clone(), Box::new(power))
    }

    fn with_power(
        dir: Dir,
        graphs: Vec<Graph>,
        power: FakePower,
        call: Box<dyn PowerCall>,
    ) -> Self {
        let mut store = GraphStore::open(dir.graph_paths());
        for graph in graphs {
            store.put(graph).unwrap();
        }
        let clock = SimClock::default();
        let wall = SimWallClock::new(WALL_START_US);
        let view = shared_view();
        let (notify, notifications) = unbounded_channel();
        let service = GraphService::new(
            store,
            Env {
                clock: Box::new(clock.clone()),
                link: Box::new(NoLink),
                scan: Box::new(NoScan),
                power: call,
                notify,
                locale: EN,
                view: view.clone(),
            },
        )
        .with_wall_clock(Box::new(wall));
        let mut rig = Self {
            service,
            clock,
            view,
            power,
            dir,
            notifications,
        };
        rig.service.apply_event(&DaemonEvent::Link(Link::Online));
        rig
    }

    /// The values of the sensors.
    fn feed(&mut self, sensors: &[(&str, f64)]) {
        self.service
            .apply_event(&DaemonEvent::Sensors(SensorSample {
                time_us: 1,
                sensors: sensors
                    .iter()
                    .map(|(id, value)| ((*id).into(), *value, 0))
                    .collect(),
                channels: Vec::new(),
            }));
    }

    /// One second on the clock, and the pass or the end of the countdown that is due.
    fn tick(&mut self) {
        self.clock.advance(Duration::from_secs(1));
        self.service.poll();
    }

    fn ticks(&mut self, seconds: u32) {
        for _ in 0..seconds {
            self.tick();
        }
    }

    fn countdown(&self) -> Option<Countdown> {
        self.view.lock().unwrap().countdown.clone()
    }

    fn outcomes(&self) -> Vec<(String, String, Outcome)> {
        self.view
            .lock()
            .unwrap()
            .log
            .iter()
            .map(|row| (row.graph_id.clone(), row.kind.clone(), row.outcome))
            .collect()
    }

    fn plans(&mut self) -> Vec<Plan> {
        std::iter::from_fn(|| self.notifications.try_recv().ok()).collect()
    }

    fn pause_count(&self) -> u32 {
        self.service.store().state().pause.count
    }

    fn state_json(&self) -> serde_json::Value {
        let text = std::fs::read_to_string(self.dir.graph_paths().state).expect("the state file");
        serde_json::from_str(&text).expect("valid JSON")
    }
}

fn outcome(graph: &str, kind: &str, outcome: Outcome) -> (String, String, Outcome) {
    (graph.to_owned(), kind.to_owned(), outcome)
}

#[test]
fn poweroff_after_the_countdown() {
    let mut rig = Rig::new(
        "power-poweroff",
        vec![power_graph(
            "graph_1", "Overheat", "x", "poweroff", 30, None,
        )],
    );
    rig.feed(&[("x", 1.0)]);
    // The pass at second 1 starts the countdown. It ends at second 31.
    rig.ticks(1);
    assert_eq!(rig.countdown().unwrap().seconds_left, 30);
    rig.ticks(29);
    assert_eq!(rig.power.calls(), []);
    assert_eq!(rig.countdown().unwrap().seconds_left, 1);

    rig.ticks(1);
    assert_eq!(rig.power.calls(), [PowerKind::Poweroff]);
    assert_eq!(rig.countdown(), None);
    assert_eq!(
        rig.outcomes(),
        [outcome("graph_1", "poweroff", Outcome::Ok)]
    );
    // The response ran: the condition that stays true starts nothing more.
    rig.ticks(60);
    assert_eq!(rig.power.calls().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reboot_and_suspend_call_the_right_methods() {
    let bus = PrivateBus::start("power-methods");
    let fake = FakeLogind::start(bus.connect().await, "3", true).await;
    let mut power = LogindPower::new(bus.connect().await, Handle::current());
    tokio::task::block_in_place(|| {
        power.call(PowerKind::Poweroff).unwrap();
        power.call(PowerKind::Reboot).unwrap();
        power.call(PowerKind::Suspend).unwrap();
    });
    let call = |method: &str| LogindCall {
        method: method.to_owned(),
        interactive: false,
    };
    // Every call has `interactive` false.
    assert_eq!(
        fake.calls(),
        [call("PowerOff"), call("Reboot"), call("Suspend")]
    );
}

#[test]
fn state_file_is_written_before_the_logind_call() {
    // Graph 1 starts a countdown of 10 s at second 1. Graph 2 runs a response at second 5: its run
    // state is not due for the periodic write, which waits 30 s after the write of second 1.
    let mut rig = Rig::new(
        "power-state-first",
        vec![
            power_graph("graph_1", "Overheat", "x", "poweroff", 10, None),
            power_graph("graph_2", "Other", "y", "notification", 10, None),
        ],
    );
    rig.feed(&[("x", 1.0), ("y", 0.0)]);
    rig.ticks(4);
    rig.feed(&[("x", 1.0), ("y", 1.0)]);
    rig.ticks(6);
    assert_eq!(rig.power.calls(), []);
    rig.ticks(1);
    assert_eq!(rig.power.calls(), [PowerKind::Poweroff]);

    // At the time of the call the file already held the state of both Graphs, and the count of the
    // Response pause.
    let file = rig.power.files().remove(0).expect("the state file exists");
    assert_eq!(file["responses"]["graph_1/2"]["state"], "run");
    assert_eq!(file["responses"]["graph_2/2"]["state"], "run");
    assert_eq!(file["pause"]["count"], 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn logind_refusal_is_failed_without_a_prompt() {
    let bus = PrivateBus::start("power-refused");
    let fake = FakeLogind::start(bus.connect().await, "3", true).await;
    fake.refuse_power(true);
    let dir = Dir::new("power-refused");
    let recorder = FakePower::new(dir.graph_paths().state);
    let logind = LogindPower::new(bus.connect().await, Handle::current());
    let mut rig = Rig::with_power(
        dir,
        vec![power_graph("graph_1", "Overheat", "x", "reboot", 10, None)],
        recorder,
        Box::new(logind),
    );
    rig.feed(&[("x", 1.0)]);
    // The service blocks on the call of logind: the runtime goes on in the meantime.
    tokio::task::block_in_place(|| rig.ticks(11));

    assert_eq!(
        rig.outcomes(),
        [outcome("graph_1", "reboot", Outcome::Failed)]
    );
    // One call, with `interactive` false: logind could not ask for a password.
    assert_eq!(
        fake.calls(),
        [LogindCall {
            method: "Reboot".into(),
            interactive: false
        }]
    );
    // The user sees the failure.
    let failed: Vec<NotificationSpec> = rig
        .plans()
        .into_iter()
        .filter_map(|plan| match plan {
            Plan::Show(spec) if spec.key == "graph-power-failed" => Some(spec),
            _ => None,
        })
        .collect();
    assert_eq!(failed.len(), 1);
    assert_eq!(failed[0].level, Level::Critical);
    assert!(failed[0].title.starts_with("Overheat"));
}

#[test]
fn restart_does_not_rerun_a_power_response() {
    let mut rig = Rig::new(
        "power-restart",
        vec![power_graph(
            "graph_1", "Overheat", "x", "poweroff", 10, None,
        )],
    );
    rig.feed(&[("x", 1.0)]);
    rig.ticks(11);
    assert_eq!(rig.power.calls(), [PowerKind::Poweroff]);

    // The PC did not power off (or the app starts again after a resume), and the condition is
    // still true: the stored state is "run", so no countdown starts.
    let Rig {
        mut service, dir, ..
    } = rig;
    service.stop();
    drop(service);
    let power = FakePower::new(dir.graph_paths().state);
    let mut rig = Rig::with_power(dir, Vec::new(), power.clone(), Box::new(power));
    rig.feed(&[("x", 1.0)]);
    rig.ticks(60);
    assert_eq!(rig.countdown(), None);
    assert_eq!(rig.power.calls(), []);
    assert_eq!(
        rig.outcomes(),
        [outcome("graph_1", "poweroff", Outcome::Ok)]
    );
}

#[test]
fn cancel_in_time_gives_cancelled() {
    let mut rig = Rig::new(
        "power-cancel",
        vec![power_graph(
            "graph_1", "Overheat", "x", "poweroff", 30, None,
        )],
    );
    rig.feed(&[("x", 1.0)]);
    // The countdown starts at second 1. The user cancels at second 12.
    rig.ticks(12);
    assert_eq!(rig.pause_count(), 1);
    rig.service.handle(GraphCommand::CancelCountdown);

    assert_eq!(rig.countdown(), None);
    assert_eq!(
        rig.outcomes(),
        [outcome("graph_1", "poweroff", Outcome::Cancelled)]
    );
    // The cancelled run gives back its count, and the node stays in the state "run".
    assert_eq!(rig.pause_count(), 0);
    assert_eq!(
        rig.service.store().state().responses["graph_1/2"].state,
        RunState::Run
    );
    rig.ticks(60);
    assert_eq!(rig.power.calls(), []);
    // A cancel after the end of a countdown changes nothing.
    rig.service.handle(GraphCommand::CancelCountdown);
    assert_eq!(rig.outcomes().len(), 1);
}

#[test]
fn repeat_runs_again_after_a_cancel() {
    let mut rig = Rig::new(
        "power-repeat",
        vec![power_graph(
            "graph_1",
            "Overheat",
            "x",
            "poweroff",
            10,
            Some(20),
        )],
    );
    rig.feed(&[("x", 1.0)]);
    rig.ticks(3);
    assert!(rig.countdown().is_some());
    rig.service.handle(GraphCommand::CancelCountdown);
    // The repeat interval of 20 s counts from the run at second 1.
    rig.ticks(17);
    assert_eq!(rig.countdown(), None, "second 20");
    rig.ticks(1);
    assert!(rig.countdown().is_some(), "second 21 runs again");
    rig.ticks(10);
    assert_eq!(rig.power.calls(), [PowerKind::Poweroff]);
    assert_eq!(
        rig.outcomes(),
        [
            outcome("graph_1", "poweroff", Outcome::Cancelled),
            outcome("graph_1", "poweroff", Outcome::Ok),
        ]
    );
}

#[test]
fn second_power_response_is_busy() {
    let mut rig = Rig::new(
        "power-busy",
        vec![
            power_graph("graph_1", "Overheat", "x", "poweroff", 10, None),
            power_graph("graph_2", "Leak", "x", "reboot", 10, None),
        ],
    );
    rig.feed(&[("x", 1.0)]);
    rig.ticks(1);
    assert_eq!(rig.countdown().unwrap().graph_id, "graph_1");
    // The reboot response fires in the same pass while the poweroff countdown runs.
    assert_eq!(
        rig.outcomes(),
        [outcome("graph_2", "reboot", Outcome::Busy)]
    );
    // The busy run does not count for the Response pause.
    assert_eq!(rig.pause_count(), 1);
    rig.ticks(10);
    assert_eq!(rig.power.calls(), [PowerKind::Poweroff]);
}

#[test]
fn app_stop_in_the_countdown_makes_no_call() {
    let mut rig = Rig::new(
        "power-stop",
        vec![power_graph(
            "graph_1", "Overheat", "x", "poweroff", 30, None,
        )],
    );
    rig.feed(&[("x", 1.0)]);
    rig.ticks(11);
    assert_eq!(rig.countdown().unwrap().seconds_left, 20);
    rig.service.stop();

    assert_eq!(rig.power.calls(), []);
    assert_eq!(rig.countdown(), None);
    assert_eq!(
        rig.outcomes(),
        [outcome("graph_1", "poweroff", Outcome::Cancelled)]
    );
    // The file holds the count from before the run, and the node in the state "run".
    let state = rig.state_json();
    assert_eq!(state["pause"]["count"], 0);
    assert_eq!(state["responses"]["graph_1/2"]["state"], "run");
    // The countdown does not end after the stop.
    rig.ticks(60);
    assert_eq!(rig.power.calls(), []);
}

#[test]
fn alarm_countdown_cancels_the_graph_countdown() {
    let mut rig = Rig::new(
        "power-alarm-cancels",
        vec![power_graph(
            "graph_1", "Overheat", "x", "poweroff", 30, None,
        )],
    );
    rig.feed(&[("x", 1.0)]);
    rig.ticks(10);
    assert_eq!(rig.countdown().unwrap().seconds_left, 21);

    // The pump-stop Alarm starts its poweroff countdown.
    rig.service.apply_event(&DaemonEvent::AlarmRaised(alarm(
        "a1",
        "pump-stop",
        "loop_1",
        Response::Poweroff,
        20,
    )));
    assert!(rig.service.alarm_poweroff_pending());
    assert_eq!(rig.countdown(), None);
    assert_eq!(
        rig.outcomes(),
        [outcome("graph_1", "poweroff", Outcome::Cancelled)]
    );
    assert_eq!(rig.pause_count(), 0);
    rig.ticks(60);
    assert_eq!(rig.power.calls(), []);
}

#[test]
fn power_response_not_started_during_an_alarm_countdown() {
    let mut rig = Rig::new(
        "power-alarm-blocks",
        vec![power_graph(
            "graph_1", "Overheat", "x", "poweroff", 10, None,
        )],
    );
    rig.service.apply_event(&DaemonEvent::AlarmRaised(alarm(
        "a1",
        "pump-stop",
        "loop_1",
        Response::Poweroff,
        20,
    )));
    rig.feed(&[("x", 1.0)]);
    rig.ticks(5);
    // The response fires, and does not start: no countdown, no row, no count.
    assert_eq!(rig.countdown(), None);
    assert_eq!(rig.outcomes(), []);
    assert_eq!(rig.pause_count(), 0);
    assert!(
        !rig.service
            .start_countdown("graph_1", PowerKind::Reboot, 10)
    );
    rig.ticks(60);
    assert_eq!(rig.power.calls(), []);
}

#[test]
fn countdown_text_shows_the_seconds_left() {
    let mut rig = Rig::new(
        "power-text",
        vec![power_graph(
            "graph_1", "Overheat", "x", "poweroff", 30, None,
        )],
    );
    rig.feed(&[("x", 1.0)]);
    rig.ticks(1);
    let first = rig.countdown().unwrap();
    assert_eq!(countdown_text(&first, EN), "Overheat: power off in 30 s");
    assert_eq!(countdown_text(&first, IT), "Overheat: spegnimento tra 30 s");

    // The notification is critical, has the button "Cancel", and each second replaces its text.
    rig.ticks(1);
    let shown: Vec<NotificationSpec> = rig
        .plans()
        .into_iter()
        .filter_map(|plan| match plan {
            Plan::Show(spec) if spec.key == "graph-countdown" => Some(spec),
            _ => None,
        })
        .collect();
    let titles: Vec<&str> = shown.iter().map(|spec| spec.title.as_str()).collect();
    assert_eq!(
        titles,
        ["Overheat: power off in 30 s", "Overheat: power off in 29 s"]
    );
    assert_eq!(shown[1].level, Level::Critical);
    assert_eq!(shown[1].buttons.len(), 1);
    assert_eq!(shown[1].buttons[0].label, "Cancel");

    // The dialog of the window shows the same words and the same second.
    let second = rig.countdown().unwrap();
    let dialog = CountdownDialog::for_graph(&second, EN);
    assert_eq!(dialog.title, "Overheat: power off in 29 s");
    assert_eq!(dialog.seconds, 29);
    assert_eq!(dialog.cancel, "Cancel");
    assert!(dialog.graph);
}

/// The user clicks a button of a notification: `notify-rust` subscribes to the action signal in its
/// own thread, so the test sends the click again until the app has it.
async fn wait_for_command(
    commands: &mut UnboundedReceiver<Command>,
    server: &FakeNotificationServer,
    id: u32,
    action: &str,
) -> Command {
    let attempts = async {
        loop {
            server.invoke(id, action).await;
            if let Ok(Some(command)) =
                tokio::time::timeout(Duration::from_millis(200), commands.recv()).await
            {
                return command;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(20), attempts)
        .await
        .expect("the action reached the app")
}

/// Show a notification from a plain thread: `notify-rust` runs its own executor.
async fn show(notifier: Notifier, spec: NotificationSpec) -> Notifier {
    tokio::task::spawn_blocking(move || {
        let mut notifier = notifier;
        notifier.show(&spec).expect("show");
        notifier
    })
    .await
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_works_from_notification_tray_and_window() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("power-cancel-sources");
    use_as_session_bus(&bus);
    let server = FakeNotificationServer::start(bus.connect().await, &["actions", "body"]).await;
    let host = FakeTrayHost::start(bus.connect().await).await;
    let countdown = Countdown {
        graph_id: "graph_1".into(),
        graph_name: "Overheat".into(),
        kind: PowerKind::Poweroff,
        seconds_left: 30,
        total_s: 30,
    };

    // The button of the notification.
    let (notify_tx, mut notify_commands) = unbounded_channel();
    let notifier = show(
        Notifier::new(notify_tx),
        countdown_notification(&countdown, EN),
    )
    .await;
    let id = server.received()[0].id;
    let from_notification =
        wait_for_command(&mut notify_commands, &server, id, "cancel_countdown").await;
    drop(notifier);

    // The entry of the tray menu.
    let view = TrayView::build(&online(), NOW_US, EN).with_graph_countdown(Some(&countdown), EN);
    let (tray_tx, mut tray_commands) = unbounded_channel();
    let tray = tokio::task::spawn_blocking(move || Tray::spawn(tray_tx, view).expect("tray"))
        .await
        .unwrap();
    let name = host.item_name().await;
    let menu = host.layout(&name).await;
    let entry = menu
        .find("Cancel the countdown of Graph Overheat (0:30)")
        .expect("the cancel entry");
    host.click(&name, entry.id).await;
    let from_tray = tray_commands.recv().await.expect("a command");
    tokio::task::spawn_blocking(move || tray.shutdown())
        .await
        .unwrap();

    // The button of the dialog of the window.
    let from_window = CountdownDialog::for_graph(&countdown, EN).cancel_command();

    // Each of the three commands reaches the service through its channel and cancels the countdown.
    for (index, command) in [from_notification, from_tray, from_window]
        .into_iter()
        .enumerate()
    {
        assert_eq!(command, Command::Graph(GraphCommand::CancelCountdown));
        let mut rig = Rig::new(
            &format!("power-cancel-source-{index}"),
            vec![power_graph(
                "graph_1", "Overheat", "x", "poweroff", 30, None,
            )],
        );
        assert!(
            rig.service
                .start_countdown("graph_1", PowerKind::Poweroff, 30)
        );
        let (channel, mut inputs) = GraphChannel::new();
        assert_eq!(channel.route(command), None);
        match inputs.try_recv() {
            Ok(Input::Command(command)) => rig.service.handle(command),
            other => panic!("the service got {other:?}"),
        }
        assert_eq!(rig.countdown(), None, "source {index}");
        assert_eq!(
            rig.outcomes(),
            [outcome("graph_1", "poweroff", Outcome::Cancelled)],
            "source {index}"
        );
        rig.ticks(60);
        assert_eq!(rig.power.calls(), [], "source {index}");
    }
}
