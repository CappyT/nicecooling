mod support;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{AppContext as _, Bounds, Pixels, TestAppContext, VisualTestContext};
use nicecooling::app::{self, AppModel, Backend, StartMode};
use nicecooling::graphs::channel::{GraphChannel, GraphCommand, Input, Inputs};
use nicecooling::graphs::power::PowerCall;
use nicecooling::graphs::responses::{Outcome, Run, Runner, Started};
use nicecooling::graphs::service::{Env, GraphService, SensorLink};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::view::{PauseView, SharedView, shared_view};
use nicecooling::i18n::{EN, Language};
use nicecooling::model::{DaemonEvent, Link, Response, SensorSample};
use nicecooling::shell::{BannerAction, BannerKind, Page};
use nicecooling_core::clock::{SimClock, SimWallClock};
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};
use nicecooling_graph::pause::PowerKind;
use support::fixtures::{LEAKSHIELD, alarm, with_leakshield};
use support::graphs::Dir;
use tokio::sync::broadcast;
use tokio::sync::mpsc::{self, UnboundedReceiver, unbounded_channel};

/// The wall clock at the start of a test: 2025-12-17 19:33:20 UTC.
const WALL_START_US: u64 = 1_766_000_000_000_000;

struct NoScan;

impl ProcessSource for NoScan {
    fn scan(&self) -> ProcessList {
        ProcessList::default()
    }
}

#[derive(Clone, Default)]
struct FakePower(Arc<Mutex<Vec<PowerKind>>>);

impl PowerCall for FakePower {
    fn call(&mut self, kind: PowerKind) -> Result<(), String> {
        self.0.lock().unwrap().push(kind);
        Ok(())
    }
}

/// A runner that records the runs and gives the outcome `ok` at once.
#[derive(Clone, Default)]
struct FakeRunner(Arc<Mutex<Vec<Run>>>);

impl Runner for FakeRunner {
    fn start(&mut self, run: &Run) -> Started {
        self.0.lock().unwrap().push(run.clone());
        Started::Done(Outcome::Ok)
    }

    fn finished(&mut self) -> Vec<(String, Outcome)> {
        Vec::new()
    }
}

/// The values of one pass: a sensor id and its value, or no value.
type PassValues = Vec<(String, Option<f64>)>;

/// The values that the Graph service sends to the daemon.
#[derive(Clone, Default)]
struct Sent(Arc<Mutex<Vec<PassValues>>>);

impl SensorLink for Sent {
    fn values(&mut self, values: &[(String, Option<f64>)]) {
        self.0.lock().unwrap().push(values.to_vec());
    }
}

/// A Graph whose response end node fires while the sensor `x` is above 0.
fn response_graph(id: &str, kind: &str, countdown: u32, repeat: Option<u32>) -> Graph {
    let mut end = Node::new(2, Kind::ResponseEnd)
        .with_param("kind", kind)
        .with_param("title", "Hot")
        .with_param("countdown_s", countdown)
        .with_param("min_interval_s", 0);
    if let Some(repeat) = repeat {
        end = end.with_param("repeat_s", repeat);
    }
    let mut graph = Graph::new(id, "Overheat");
    graph.nodes = vec![
        Node::new(1, Kind::SensorSource).with_param("sensor", "x"),
        end,
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    graph
}

/// A Graph with a sensor end node that gives the sensor `x`.
fn sensor_graph(id: &str) -> Graph {
    let mut graph = Graph::new(id, "Water");
    graph.nodes = vec![
        Node::new(1, Kind::SensorSource).with_param("sensor", "x"),
        Node::new(2, Kind::SensorEnd),
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    graph.sensor_id = Some(id.to_owned());
    graph
}

/// The Graph service with a simulated clock, a recording runner and a recording power call.
struct Rig {
    service: GraphService,
    clock: SimClock,
    view: SharedView,
    runner: FakeRunner,
    power: FakePower,
    sent: Sent,
    dir: Dir,
    _notifications: UnboundedReceiver<nicecooling::notify::Plan>,
}

impl Rig {
    fn new(name: &str, graphs: Vec<Graph>) -> Self {
        Self::build(Dir::new(name), graphs, shared_view(), 0)
    }

    /// The service starts with `count` power runs in a row. At 3 the Response pause is on.
    fn with_count(name: &str, graphs: Vec<Graph>, count: u32) -> Self {
        Self::build(Dir::new(name), graphs, shared_view(), count)
    }

    fn build(dir: Dir, graphs: Vec<Graph>, view: SharedView, count: u32) -> Self {
        let mut store = GraphStore::open(dir.graph_paths());
        for graph in graphs {
            store.put(graph).unwrap();
        }
        if count > 0 {
            store.state_mut().pause.count = count;
            store.state_mut().pause.paused = count >= 3;
        }
        let (runner, power, sent) = (FakeRunner::default(), FakePower::default(), Sent::default());
        let clock = SimClock::default();
        let (notify, notifications) = unbounded_channel();
        let service = GraphService::new(
            store,
            Env {
                clock: Box::new(clock.clone()),
                link: Box::new(sent.clone()),
                scan: Box::new(NoScan),
                power: Box::new(power.clone()),
                notify,
                locale: EN,
                view: view.clone(),
            },
        )
        .with_wall_clock(Box::new(SimWallClock::new(WALL_START_US)))
        .with_runner(Box::new(runner.clone()));
        let mut rig = Self {
            service,
            clock,
            view,
            runner,
            power,
            sent,
            dir,
            _notifications: notifications,
        };
        rig.service.apply_event(&DaemonEvent::Link(Link::Online));
        rig
    }

    /// The app stops and starts again on the same files.
    fn restart(self) -> Self {
        let Self {
            mut service,
            dir,
            view,
            ..
        } = self;
        service.stop();
        drop(service);
        Self::build(dir, Vec::new(), view, 0)
    }

    fn feed(&mut self, value: f64) {
        self.service
            .apply_event(&DaemonEvent::Sensors(SensorSample {
                time_us: 1,
                sensors: vec![("x".into(), value, 0)],
                channels: Vec::new(),
            }));
    }

    fn ticks(&mut self, seconds: u32) {
        for _ in 0..seconds {
            self.clock.advance(Duration::from_secs(1));
            self.service.poll();
        }
    }

    fn paused(&self) -> bool {
        self.view.lock().unwrap().pause.paused
    }

    fn count(&self) -> u32 {
        self.view.lock().unwrap().pause.count
    }

    fn runs(&self) -> Vec<Run> {
        self.runner.0.lock().unwrap().clone()
    }

    fn power_calls(&self) -> Vec<PowerKind> {
        self.power.0.lock().unwrap().clone()
    }

    fn countdown_runs(&self) -> bool {
        self.view.lock().unwrap().countdown.is_some()
    }

    fn state_json(&self) -> serde_json::Value {
        let text = std::fs::read_to_string(self.dir.graph_paths().state).expect("the state file");
        serde_json::from_str(&text).expect("valid JSON")
    }
}

/// A reboot response that runs three times in a row. Each countdown is 10 s and the repeat is
/// 12 s, so the runs start at second 1, 13 and 25. The rig is at second 25, in the third countdown.
fn three_reboots(name: &str) -> Rig {
    let mut rig = Rig::new(
        name,
        vec![response_graph("graph_1", "reboot", 10, Some(12))],
    );
    rig.feed(1.0);
    rig.ticks(13);
    assert_eq!((rig.paused(), rig.count()), (false, 2));
    rig.ticks(12);
    rig
}

#[test]
fn third_run_enters_the_pause() {
    let mut rig = three_reboots("pause-third");
    assert_eq!((rig.paused(), rig.count()), (true, 3));
    // The third run goes on: it is a countdown of 10 s from second 25.
    assert!(rig.countdown_runs());
    rig.ticks(10);
    assert_eq!(rig.power_calls(), [PowerKind::Reboot; 3]);
    // The repeat at second 37 starts nothing: the pause is on.
    rig.ticks(30);
    assert!(!rig.countdown_runs());
    assert_eq!(rig.power_calls().len(), 3);
}

#[test]
fn pause_blocks_responses_not_sensor_values() {
    let mut rig = Rig::with_count(
        "pause-blocks",
        vec![
            sensor_graph("graph_1"),
            response_graph("graph_2", "notification", 30, None),
        ],
        3,
    );
    assert!(rig.paused());
    rig.feed(1.0);
    rig.ticks(5);
    // The Graph sensor still sends its value once per second.
    let sent = rig.sent.0.lock().unwrap().clone();
    assert_eq!(sent.len(), 5);
    assert_eq!(sent[4], [("graph_1".to_owned(), Some(1.0))]);
    // The response runs nothing, and the log has no row.
    assert!(rig.runs().is_empty());
    assert!(rig.view.lock().unwrap().log.is_empty());

    // The user turns the responses on. The condition has been true all along: it runs nothing
    // until it rises again.
    rig.service.handle(GraphCommand::ResponsesOn);
    assert!(!rig.paused());
    rig.ticks(3);
    assert!(rig.runs().is_empty());
    rig.feed(0.0);
    rig.ticks(1);
    rig.feed(1.0);
    rig.ticks(1);
    assert_eq!(rig.runs().len(), 1);
}

#[test]
fn pause_survives_a_restart() {
    let mut rig = three_reboots("pause-restart");
    rig.ticks(10);
    assert_eq!(rig.power_calls().len(), 3);
    assert!(rig.paused());

    let mut rig = rig.restart();
    // The banner shows at once, and the file holds the pause.
    assert_eq!((rig.paused(), rig.count()), (true, 3));
    let state = rig.state_json();
    assert_eq!(state["pause"]["paused"], true);
    assert_eq!(state["pause"]["count"], 3);
    // The condition is still true, the repeat is due, and nothing runs.
    rig.feed(1.0);
    rig.ticks(40);
    assert!(!rig.countdown_runs());
    assert_eq!(rig.power_calls(), []);
}

#[test]
fn run_once_runs_in_the_pause() {
    let mut rig = Rig::with_count("run-once-pause", Vec::new(), 3);
    assert!(rig.paused());
    let draft = response_graph("graph_9", "notification", 30, None);
    rig.service.handle(GraphCommand::RunOnce {
        graph: Box::new(draft),
    });

    // The run uses the parameters of the draft, though the Graph is not applied.
    let runs = rig.runs();
    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].graph_id, "graph_9");
    assert_eq!(runs[0].node.text("title"), "Hot");
    assert_eq!(rig.view.lock().unwrap().log.len(), 1);
    assert!(rig.paused(), "a notification does not end the pause");
}

#[test]
fn run_once_poweroff_counts() {
    let mut rig = Rig::with_count("run-once-count", Vec::new(), 2);
    assert_eq!((rig.paused(), rig.count()), (false, 2));
    let draft = response_graph("graph_9", "reboot", 10, None);
    rig.service.handle(GraphCommand::RunOnce {
        graph: Box::new(draft),
    });

    // The count is 3 and the pause starts. The countdown of the run goes on.
    assert_eq!((rig.paused(), rig.count()), (true, 3));
    assert!(rig.countdown_runs());
    rig.ticks(10);
    assert_eq!(rig.power_calls(), [PowerKind::Reboot]);

    // A suspend does not count.
    let mut rig = Rig::with_count("run-once-suspend", Vec::new(), 2);
    let draft = response_graph("graph_9", "suspend", 10, None);
    rig.service.handle(GraphCommand::RunOnce {
        graph: Box::new(draft),
    });
    assert_eq!((rig.paused(), rig.count()), (false, 2));
}

#[test]
fn run_once_blocked_during_an_alarm_countdown() {
    let mut rig = Rig::new("run-once-alarm", Vec::new());
    rig.service.apply_event(&DaemonEvent::AlarmRaised(alarm(
        "a1",
        "pump-stop",
        "loop_1",
        Response::Poweroff,
        20,
    )));
    let draft = response_graph("graph_9", "poweroff", 10, None);
    rig.service.handle(GraphCommand::RunOnce {
        graph: Box::new(draft),
    });

    // Nothing runs, and the app gives the reason.
    assert!(!rig.countdown_runs());
    assert_eq!(rig.count(), 0);
    let view = rig.view.lock().unwrap();
    assert!(view.log.is_empty());
    assert_eq!(
        view.refusal.as_deref(),
        Some("An Alarm poweroff countdown runs. No poweroff, reboot or suspend response starts.")
    );
    drop(view);
    rig.ticks(30);
    assert_eq!(rig.power_calls(), []);
}

/// The window with the Graph channel, and the shared view that the service writes.
struct Window {
    model: gpui::Entity<AppModel>,
    view: SharedView,
    inputs: Inputs,
    _events: broadcast::Sender<DaemonEvent>,
    _runtime: tokio::runtime::Runtime,
}

fn start_window(cx: &mut TestAppContext, view: SharedView) -> Window {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (events, events_rx) = broadcast::channel(64);
    let (commands, inbox) = mpsc::unbounded_channel();
    let (channel, inputs) = GraphChannel::new();
    let backend = Backend {
        events: events_rx,
        commands,
        inbox,
        client: None,
        graphs: Some(channel.with_view(view.clone())),
        runtime: runtime.handle().clone(),
        language: Language::English,
        settings: Default::default(),
    };
    let model = cx.update(|cx| app::init(cx, StartMode::Normal, backend));
    let state = with_leakshield();
    events
        .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
        .unwrap();
    cx.run_until_parked();
    Window {
        model,
        view,
        inputs,
        _events: events,
        _runtime: runtime,
    }
}

/// The bounds of an element after a new frame.
fn named(visual: &mut VisualTestContext, name: &'static str) -> Option<Bounds<Pixels>> {
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    visual.debug_bounds(name)
}

#[gpui::test]
fn banner_shows_on_every_page(cx: &mut TestAppContext) {
    let window = start_window(cx, shared_view());
    let handle = cx.windows()[0];
    let pages = [
        Page::Overview,
        Page::Loops,
        Page::Channels,
        Page::Sensors,
        Page::Profiles,
        Page::Alarms,
        Page::History,
        Page::Settings,
        Page::Leakshield(LEAKSHIELD.to_owned()),
    ];
    let show = |cx: &mut TestAppContext, page: &Page| {
        cx.update(|cx| {
            window.model.update(cx, |app, cx| {
                app.page = page.clone();
                cx.notify();
            });
        });
        cx.run_until_parked();
    };

    // Without the pause no page has the banner.
    for page in &pages {
        show(cx, page);
        let mut visual = VisualTestContext::from_window(handle, cx);
        assert!(
            named(&mut visual, "banner-responses-on").is_none(),
            "{page:?}"
        );
    }
    window.view.lock().unwrap().pause = PauseView {
        paused: true,
        count: 3,
    };
    for page in &pages {
        show(cx, page);
        let mut visual = VisualTestContext::from_window(handle, cx);
        assert!(
            named(&mut visual, "banner-responses-on").is_some(),
            "{page:?}"
        );
    }

    // The banner names the cause and offers the button.
    let shell = cx.read_entity(&window.model, |app, _| app.shell());
    let banner = shell
        .banners
        .iter()
        .find(|banner| banner.kind == BannerKind::ResponsePause)
        .expect("the Response pause banner");
    assert_eq!(banner.title, "Response pause: 3 power responses in a row");
    assert_eq!(
        banner.detail.as_deref(),
        Some("No Graph response runs by itself. The Graph sensors still work.")
    );
    assert_eq!(
        banner.button,
        Some((
            "Turn the Graph responses on".to_owned(),
            BannerAction::TurnResponsesOn
        ))
    );
}

#[gpui::test]
fn turn_on_button_ends_the_pause(cx: &mut TestAppContext) {
    let view = shared_view();
    let mut window = start_window(cx, view.clone());
    // The Graph service starts in the pause, and writes it to the view that the window reads.
    let mut rig = Rig::build(Dir::new("pause-turn-on"), Vec::new(), view.clone(), 3);
    assert!(view.lock().unwrap().pause.paused);
    cx.update(|cx| window.model.update(cx, |_, cx| cx.notify()));
    cx.run_until_parked();

    let handle = cx.windows()[0];
    let mut visual = VisualTestContext::from_window(handle, cx);
    let button = named(&mut visual, "banner-responses-on").expect("the button of the banner");
    visual.simulate_click(button.center(), gpui::Modifiers::none());
    visual.run_until_parked();

    // The press sends one Graph command, and the app routes it to the service.
    match window.inputs.try_recv() {
        Ok(Input::Command(command)) => {
            assert_eq!(command, GraphCommand::ResponsesOn);
            rig.service.handle(command);
        }
        other => panic!("the service got {other:?}"),
    }
    assert!(window.inputs.try_recv().is_err(), "one command");
    // The pause ends at once, and the count is 0.
    assert!(!window.view.lock().unwrap().pause.paused);
    assert_eq!(rig.count(), 0);
    visual.update(|_, cx| window.model.update(cx, |_, cx| cx.notify()));
    assert!(named(&mut visual, "banner-responses-on").is_none());
}
