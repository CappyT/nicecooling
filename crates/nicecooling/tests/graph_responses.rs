mod support;

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use nicecooling::app_settings::AppSettings;
use nicecooling::graphs::channel::GraphCommand;
use nicecooling::graphs::kinds::{KindRunner, SmtpSource};
use nicecooling::graphs::responses::{Outcome, Run, Runner, Started};
use nicecooling::graphs::secrets::Password;
use nicecooling::graphs::service::{Env, GraphService, NoLink, NoPower};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::view::{SharedView, shared_view};
use nicecooling::i18n::EN;
use nicecooling::model::{DaemonEvent, Link, SensorSample};
use nicecooling::notify::{NotificationSpec, Notifier, Plan};
use nicecooling::services::smtp_source;
use nicecooling::smtp::send::SmtpAccount;
use nicecooling::smtp::settings::{SmtpSecurity, SmtpSettings};
use nicecooling_core::clock::{SimClock, SimWallClock};
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};
use nicecooling_graph::template::LabelledValue;
use support::bus::PrivateBus;
use support::fake_notification_server::FakeNotificationServer;
use support::graphs::Dir;
use support::smtp_server::{Behavior as SmtpBehavior, SmtpServer};
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

/// A runner that records the runs. It gives the outcome `ok` at once, or leaves the run pending
/// until the test finishes it.
#[derive(Clone, Default)]
struct FakeRunner {
    runs: Arc<Mutex<Vec<Run>>>,
    pending: Arc<Mutex<bool>>,
    finished: Arc<Mutex<Vec<(String, Outcome)>>>,
}

impl FakeRunner {
    fn runs(&self) -> Vec<Run> {
        self.runs.lock().unwrap().clone()
    }

    fn leave_pending(&self, pending: bool) {
        *self.pending.lock().unwrap() = pending;
    }

    fn finish(&self, run_id: &str, outcome: Outcome) {
        self.finished
            .lock()
            .unwrap()
            .push((run_id.to_owned(), outcome));
    }
}

impl Runner for FakeRunner {
    fn start(&mut self, run: &Run) -> Started {
        self.runs.lock().unwrap().push(run.clone());
        if *self.pending.lock().unwrap() {
            Started::Pending
        } else {
            Started::Done(Outcome::Ok)
        }
    }

    fn finished(&mut self) -> Vec<(String, Outcome)> {
        std::mem::take(&mut *self.finished.lock().unwrap())
    }
}

/// A Graph whose response end node is a notification. It fires while the sensor `x` is above 0.
fn response_graph(id: &str, name: &str, hold: u32, repeat: Option<u32>, interval: u32) -> Graph {
    let mut end = Node::new(2, Kind::ResponseEnd)
        .with_param("kind", "notification")
        .with_param("title", "Hot")
        .with_param("hold_s", hold)
        .with_param("min_interval_s", interval);
    if let Some(repeat) = repeat {
        end = end.with_param("repeat_s", repeat);
    }
    let mut graph = Graph::new(id, name);
    graph.nodes = vec![
        Node::new(1, Kind::SensorSource).with_param("sensor", "x"),
        end,
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    graph
}

/// The Graph service with a fake runner, a simulated clock and a simulated wall clock.
struct Rig {
    service: GraphService,
    clock: SimClock,
    wall: SimWallClock,
    runner: FakeRunner,
    view: SharedView,
    dir: Dir,
    _notifications: UnboundedReceiver<Plan>,
}

impl Rig {
    fn new(name: &str, graphs: Vec<Graph>) -> Self {
        Self::in_dir(Dir::new(name), SimWallClock::new(WALL_START_US), graphs)
    }

    fn in_dir(dir: Dir, wall: SimWallClock, graphs: Vec<Graph>) -> Self {
        let runner = FakeRunner::default();
        Self::build(dir, wall, graphs, runner.clone(), Box::new(runner))
    }

    /// The service starts its responses with `starts`. `runner` is the fake that the test reads:
    /// it records nothing when `starts` is another runner.
    fn build(
        dir: Dir,
        wall: SimWallClock,
        graphs: Vec<Graph>,
        runner: FakeRunner,
        starts: Box<dyn Runner>,
    ) -> Self {
        let mut store = GraphStore::open(dir.graph_paths());
        for graph in graphs {
            store.put(graph).unwrap();
        }
        let clock = SimClock::default();
        let view = shared_view();
        let (notify, notifications) = unbounded_channel();
        let service = GraphService::new(
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
        .with_wall_clock(Box::new(wall.clone()))
        .with_runner(starts);
        let mut rig = Self {
            service,
            clock,
            wall,
            runner,
            view,
            dir,
            _notifications: notifications,
        };
        rig.service.apply_event(&DaemonEvent::Link(Link::Online));
        rig
    }

    /// The app stops and starts again. The wall clock goes on, and the monotonic clock starts at 0.
    fn restart(self) -> Self {
        let Self {
            mut service,
            dir,
            wall,
            ..
        } = self;
        service.stop();
        drop(service);
        Self::in_dir(dir, wall, Vec::new())
    }

    /// The value of the sensor `x`.
    fn feed(&mut self, value: f64) {
        self.service
            .apply_event(&DaemonEvent::Sensors(SensorSample {
                time_us: 1,
                sensors: vec![("x".into(), value, 0)],
                channels: Vec::new(),
            }));
    }

    /// One second on both clocks, and the pass that is due.
    fn tick(&mut self) {
        self.clock.advance(Duration::from_secs(1));
        self.wall.advance(Duration::from_secs(1));
        self.service.poll();
    }

    fn log_json(&self) -> serde_json::Value {
        let text = std::fs::read_to_string(self.dir.graph_paths().log).expect("the log file");
        serde_json::from_str(&text).expect("valid JSON")
    }

    fn state_json(&self) -> serde_json::Value {
        let text = std::fs::read_to_string(self.dir.graph_paths().state).expect("the state file");
        serde_json::from_str(&text).expect("valid JSON")
    }
}

#[test]
fn run_ids_are_unique() {
    let mut rig = Rig::new(
        "run-ids",
        vec![
            response_graph("graph_1", "One", 0, Some(1), 0),
            response_graph("graph_2", "Two", 0, Some(1), 0),
        ],
    );
    rig.feed(1.0);
    for _ in 0..10 {
        rig.tick();
    }
    let runs = rig.runner.runs();
    // Two Graphs fire in each of the 10 passes, at the same wall clock time.
    assert_eq!(runs.len(), 20);
    let ids: BTreeSet<&str> = runs.iter().map(|run| run.id.as_str()).collect();
    assert_eq!(ids.len(), 20, "{ids:?}");
    // The id names the wall clock time of the run.
    assert!(
        runs[0]
            .id
            .starts_with(&format!("run_{}_", WALL_START_US + 1_000_000))
    );
}

#[test]
fn log_row_has_the_five_fields() {
    let mut rig = Rig::new(
        "log-row",
        vec![response_graph("graph_3", "Hot water", 0, None, 0)],
    );
    rig.feed(1.0);
    rig.tick();
    let json = rig.log_json();
    assert_eq!(json["schema_version"], 1);
    let rows = json["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    let row = rows[0].as_object().unwrap();
    let mut names: Vec<&str> = row.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        ["graph_id", "graph_name", "kind", "outcome", "time_us"]
    );
    assert_eq!(row["graph_id"], "graph_3");
    assert_eq!(row["graph_name"], "Hot water");
    assert_eq!(row["kind"], "notification");
    assert_eq!(row["outcome"], "ok");
    assert_eq!(row["time_us"], WALL_START_US + 1_000_000);
}

#[test]
fn log_keeps_500_rows() {
    let mut rig = Rig::new(
        "log-500",
        vec![response_graph("graph_1", "One", 0, Some(1), 0)],
    );
    rig.feed(1.0);
    for _ in 0..505 {
        rig.tick();
    }
    let json = rig.log_json();
    let rows = json["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 500);
    // The five oldest rows are gone.
    assert_eq!(rows[0]["time_us"], WALL_START_US + 6 * 1_000_000);
    assert_eq!(rows[499]["time_us"], WALL_START_US + 505 * 1_000_000);
    assert_eq!(rig.view.lock().unwrap().log.len(), 500);
    // The rows survive a restart.
    let reopened = GraphStore::open(rig.dir.graph_paths());
    assert_eq!(reopened.log().len(), 500);
}

#[test]
fn log_rows_of_one_graph() {
    let mut rig = Rig::new(
        "log-graph",
        vec![
            response_graph("graph_1", "One", 0, Some(1), 0),
            response_graph("graph_2", "Two", 0, None, 0),
        ],
    );
    rig.feed(1.0);
    for _ in 0..3 {
        rig.tick();
    }
    let view = rig.view.lock().unwrap();
    assert_eq!(view.log.len(), 4);
    let first = view.log_rows_of("graph_1");
    assert_eq!(first.len(), 3);
    assert!(first.iter().all(|row| row.graph_name == "One"));
    let second = view.log_rows_of("graph_2");
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].graph_name, "Two");
    assert!(view.log_rows_of("graph_9").is_empty());
}

#[test]
fn second_run_without_outcome_is_busy() {
    let mut rig = Rig::new(
        "busy",
        vec![response_graph("graph_1", "One", 0, Some(1), 0)],
    );
    rig.runner.leave_pending(true);
    rig.feed(1.0);
    rig.tick();
    assert_eq!(rig.runner.runs().len(), 1);
    assert!(rig.view.lock().unwrap().log.is_empty(), "no outcome yet");

    // The repeat interval passes again while the first run has no outcome.
    rig.tick();
    assert_eq!(rig.runner.runs().len(), 1, "the runner starts nothing");
    {
        let view = rig.view.lock().unwrap();
        assert_eq!(view.log.len(), 1);
        assert_eq!(view.log[0].outcome, Outcome::Busy);
    }

    // The first run ends. The node takes a new run.
    let first = rig.runner.runs()[0].id.clone();
    rig.runner.finish(&first, Outcome::Timeout);
    rig.tick();
    assert_eq!(rig.runner.runs().len(), 2);
    let outcomes: Vec<Outcome> = rig
        .view
        .lock()
        .unwrap()
        .log
        .iter()
        .map(|row| row.outcome)
        .collect();
    assert_eq!(outcomes, [Outcome::Busy, Outcome::Timeout]);
}

#[test]
fn restart_does_not_rerun_a_true_condition() {
    let mut rig = Rig::new(
        "restart-true",
        vec![response_graph("graph_1", "One", 0, None, 0)],
    );
    rig.feed(1.0);
    rig.tick();
    assert_eq!(rig.runner.runs().len(), 1);

    let mut rig = rig.restart();
    rig.feed(1.0);
    for _ in 0..5 {
        rig.tick();
    }
    assert!(
        rig.runner.runs().is_empty(),
        "the stored state is run, so the condition runs nothing"
    );
}

#[test]
fn restart_with_not_run_state_runs_at_the_first_evaluation() {
    let mut rig = Rig::new(
        "restart-not-run",
        vec![response_graph("graph_1", "One", 0, None, 0)],
    );
    rig.feed(1.0);
    rig.tick();
    assert_eq!(rig.runner.runs().len(), 1);
    // The condition goes away: the state is "not run".
    rig.feed(0.0);
    rig.tick();
    rig.service.stop();
    assert_eq!(
        rig.state_json()["responses"]["graph_1/2"]["state"],
        "not_run"
    );

    let mut rig = rig.restart();
    rig.feed(1.0);
    rig.tick();
    assert_eq!(
        rig.runner.runs().len(),
        1,
        "it runs at the first evaluation"
    );
}

#[test]
fn time_in_the_state_file_is_wall_clock() {
    let mut rig = Rig::new(
        "wall-time",
        vec![response_graph("graph_1", "One", 0, None, 0)],
    );
    rig.feed(1.0);
    for _ in 0..3 {
        rig.tick();
    }
    rig.service.stop();
    let state = rig.state_json();
    let record = &state["responses"]["graph_1/2"];
    assert_eq!(record["state"], "run");
    // The run was at the first pass, one second after the start. The monotonic clock has no part.
    assert_eq!(record["last_run_us"], WALL_START_US + 1_000_000);
}

/// A run of a response end node of the Graph `graph_2` named "Hot water".
fn run_of(node: Node, values: Vec<LabelledValue>) -> Run {
    Run {
        id: "run_1_1".into(),
        key: "graph_2/2".into(),
        graph_id: "graph_2".into(),
        graph_name: "Hot water".into(),
        kind: node.response_kind(),
        node,
        time_us: WALL_START_US,
        time_text: "2026-11-25 14:05:09".into(),
        values,
    }
}

fn labelled(label: &str, value: Option<f64>, unit: &str) -> LabelledValue {
    LabelledValue {
        label: label.into(),
        value,
        unit: unit.into(),
    }
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

fn shown(requests: &mut UnboundedReceiver<Plan>) -> NotificationSpec {
    match requests.try_recv() {
        Ok(Plan::Show(spec)) => spec,
        other => panic!("expected one notification, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn notification_response_is_shown() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("response-notification");
    use_as_session_bus(&bus);
    let server = FakeNotificationServer::start(bus.connect().await, &["actions", "body"]).await;
    let (notify, mut requests) = unbounded_channel();
    let mut runner = KindRunner::new(notify);

    let node = Node::new(2, Kind::ResponseEnd)
        .with_param("kind", "notification")
        .with_param("title", "Water hot")
        .with_param("text", "Check the pump.")
        .with_param("urgency", "critical");
    assert_eq!(
        runner.start(&run_of(node, Vec::new())),
        Started::Done(Outcome::Ok)
    );

    // The notification thread shows the request.
    let spec = shown(&mut requests);
    let (commands, _inbox) = unbounded_channel();
    let notifier = show(Notifier::new(commands), spec).await;
    drop(notifier);
    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].summary, "Water hot");
    assert_eq!(received[0].body, "Check the pump.");
    assert_eq!(received[0].urgency, Some(2));
    assert_eq!(received[0].sound_name, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn sound_response_sends_the_sound_name() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("response-sound");
    use_as_session_bus(&bus);
    let server = FakeNotificationServer::start(bus.connect().await, &["body", "sound"]).await;
    let (notify, mut requests) = unbounded_channel();
    let mut runner = KindRunner::new(notify);

    let node = Node::new(2, Kind::ResponseEnd)
        .with_param("kind", "sound")
        .with_param("sound", "dialog-warning");
    assert_eq!(
        runner.start(&run_of(node, Vec::new())),
        Started::Done(Outcome::Ok)
    );

    let spec = shown(&mut requests);
    let (commands, _inbox) = unbounded_channel();
    let notifier = show(Notifier::new(commands), spec).await;
    drop(notifier);
    // The Graph name is the title, and the sound name is the hint.
    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].summary, "Hot water");
    assert_eq!(received[0].sound_name.as_deref(), Some("dialog-warning"));
}

#[test]
fn placeholders_are_expanded_in_texts() {
    let (notify, mut requests) = unbounded_channel();
    let mut runner = KindRunner::new(notify);
    let node = Node::new(2, Kind::ResponseEnd)
        .with_param("kind", "notification")
        .with_param("title", "{graph}")
        .with_param(
            "text",
            "{graph}: water {value:water} {unit:water}, pump {value:pump}, at {time}. {{done}}",
        );
    let values = vec![
        labelled("water", Some(46.2), "°C"),
        labelled("pump", None, ""),
    ];
    runner.start(&run_of(node, values));

    let spec = shown(&mut requests);
    assert_eq!(
        spec.body,
        "Hot water: water 46.2 °C, pump --, at 2026-11-25 14:05:09. {done}"
    );
    // The spec expands the text of a notification, and not its title.
    assert_eq!(spec.title, "{graph}");
}

/// The path of a program of the system.
fn program(name: &str) -> String {
    ["/usr/bin", "/bin"]
        .iter()
        .map(|dir| format!("{dir}/{name}"))
        .find(|path| Path::new(path).exists())
        .unwrap_or_else(|| panic!("the system has no {name}"))
}

fn command_run(path: &str, args: &[String], limit_s: f64, values: Vec<LabelledValue>) -> Run {
    let node = Node::new(2, Kind::ResponseEnd)
        .with_param("kind", "command")
        .with_param("path", path)
        .with_param("args", serde_json::json!(args))
        .with_param("time_limit_s", limit_s);
    run_of(node, values)
}

/// The outcome of the command of `run_id`, from the runner.
fn wait_outcome(runner: &mut KindRunner, run_id: &str) -> Outcome {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some((_, outcome)) = runner.finished().into_iter().find(|(id, _)| id == run_id) {
            return outcome;
        }
        assert!(Instant::now() < deadline, "the command has no outcome");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn command_starts_without_a_shell() {
    let dir = Dir::new("command-no-shell");
    let (notify, _requests) = unbounded_channel();
    let mut runner = KindRunner::new(notify);

    // One argument with a space, a semicolon, a variable and a placeholder. No shell reads it, so
    // `touch` makes one file with exactly this name.
    let weird = format!("{}/a b; echo $HOME {{graph}}", dir.path().display());
    let run = command_run(
        &program("touch"),
        std::slice::from_ref(&weird),
        30.0,
        vec![labelled("graph", Some(1.0), "")],
    );
    assert_eq!(runner.start(&run), Started::Pending);
    assert_eq!(wait_outcome(&mut runner, &run.id), Outcome::Ok);
    assert!(Path::new(&weird).exists(), "the argument stays as written");
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);

    // A placeholder in the path is not expanded either: the path does not exist.
    let path = format!("{}/{{graph}}", dir.path().display());
    let run = command_run(&path, &[], 30.0, Vec::new());
    assert_eq!(runner.start(&run), Started::Done(Outcome::Failed));
}

#[test]
fn command_environment_has_the_graph_and_labelled_node_values() {
    let dir = Dir::new("command-env");
    let out = dir.path().join("env.txt");
    let (notify, _requests) = unbounded_channel();
    let mut runner = KindRunner::new(notify);

    // `printf` prints each variable. `${NAME-unset}` tells a variable that is empty from one that
    // is not there.
    let script = format!(
        "printf '%s\\n' \"$NICECOOLING_GRAPH_ID\" \"$NICECOOLING_GRAPH_NAME\" \"$NICECOOLING_RUN_ID\" \
         \"$NICECOOLING_NODE_WATER\" \"${{NICECOOLING_NODE_PUMP-unset}}\" \"${{HOME-unset}}\" \
         \"${{NICECOOLING_NODE_THIRD-unset}}\" > '{}'",
        out.display()
    );
    let values = vec![
        labelled("water", Some(46.2), "°C"),
        labelled("pump", None, ""),
    ];
    let run = command_run(&program("sh"), &["-c".to_owned(), script], 30.0, values);
    assert_eq!(runner.start(&run), Started::Pending);
    assert_eq!(wait_outcome(&mut runner, &run.id), Outcome::Ok);

    let text = std::fs::read_to_string(&out).expect("the command wrote its output");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        [
            "graph_2",
            "Hot water",
            "run_1_1",
            "46.2",
            // A node with no value is empty, and a node with no label has no variable.
            "",
            // The environment is cleared.
            "unset",
            "unset",
        ]
    );
}

/// Whether the process is there and not a zombie.
fn alive(pid: i32) -> bool {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => !stat
            .rsplit(") ")
            .next()
            .is_some_and(|rest| rest.starts_with('Z')),
        Err(_) => false,
    }
}

#[test]
fn command_time_limit_kills_the_group() {
    let dir = Dir::new("command-limit");
    let pid_file = dir.path().join("pid.txt");
    let (notify, _requests) = unbounded_channel();
    let mut runner = KindRunner::new(notify);

    // The shell starts a `sleep` of 30 s in its group and waits for it.
    let script = format!("sleep 30 & echo $! > '{}'; wait", pid_file.display());
    let run = command_run(&program("sh"), &["-c".to_owned(), script], 1.0, Vec::new());
    assert_eq!(runner.start(&run), Started::Pending);
    assert_eq!(wait_outcome(&mut runner, &run.id), Outcome::Timeout);

    let pid: i32 = std::fs::read_to_string(&pid_file)
        .expect("the shell wrote the process id")
        .trim()
        .parse()
        .expect("a process id");
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!alive(pid), "the group of the command is dead");
}

#[test]
fn command_missing_program_is_failed() {
    let (notify, _requests) = unbounded_channel();
    let mut runner = KindRunner::new(notify);
    let run = command_run("/nonexistent/notify-send", &[], 30.0, Vec::new());
    assert_eq!(runner.start(&run), Started::Done(Outcome::Failed));
    // A relative name is not looked up in `PATH`.
    let run = command_run("touch", &[], 30.0, Vec::new());
    assert_eq!(runner.start(&run), Started::Done(Outcome::Failed));
    // A program that exits with an error gives the outcome `failed` too.
    let run = command_run(&program("false"), &[], 30.0, Vec::new());
    assert_eq!(runner.start(&run), Started::Pending);
    assert_eq!(wait_outcome(&mut runner, &run.id), Outcome::Failed);
}

#[test]
fn command_output_is_discarded_and_it_leads_its_own_group() {
    let dir = Dir::new("command-stdio");
    let out = dir.path().join("fds.txt");
    let (notify, _requests) = unbounded_channel();
    let mut runner = KindRunner::new(notify);

    // The shell tells where its descriptors 0, 1 and 2 point, and then its process id and the
    // fields of its `stat`. `-ef` compares the files that two paths name.
    let script = format!(
        "for fd in 0 1 2; do if [ /proc/$$/fd/$fd -ef /dev/null ]; then echo null >> '{out}'; \\
         else echo other >> '{out}'; fi; done; read line < /proc/$$/stat; echo \"$$ $line\" >> '{out}'",
        out = out.display()
    );
    let run = command_run(&program("sh"), &["-c".to_owned(), script], 30.0, Vec::new());
    assert_eq!(runner.start(&run), Started::Pending);
    assert_eq!(wait_outcome(&mut runner, &run.id), Outcome::Ok);

    let text = std::fs::read_to_string(&out).expect("the command wrote its output");
    let lines: Vec<&str> = text.lines().collect();
    // Input and output go to `/dev/null`: nothing of the program reaches the app.
    assert_eq!(lines[..3], ["null", "null", "null"]);
    // Field 5 of `stat` is the process group. The program leads its own group: it is its pid.
    let fields: Vec<&str> = lines[3].split(' ').collect();
    assert_eq!(fields[5], fields[0], "{}", lines[3]);
}

/// The Graph service with the real `KindRunner`: "Run once" runs the response of the draft, and
/// the service logs its outcome when the runner reports it. Polls until the log has `rows` rows.
fn wait_for_log(rig: &mut Rig, rows: usize) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        rig.service.poll();
        let logged = rig.view.lock().unwrap().log.len();
        if logged >= rows {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "the log has {logged} of {rows} rows"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

/// A Graph with one response end node, as the editor sends it to "Run once".
fn draft(node: Node) -> Graph {
    let mut graph = Graph::new("graph_2", "Hot water");
    graph.nodes = vec![node];
    graph
}

fn run_once(rig: &mut Rig, node: Node) {
    rig.service.handle(GraphCommand::RunOnce {
        graph: Box::new(draft(node)),
    });
}

fn email_node() -> Node {
    Node::new(2, Kind::ResponseEnd)
        .with_param("kind", "email")
        .with_param("to", "owner@example.org")
        .with_param("subject", "Pump stopped")
        .with_param("text", "The pump stopped.")
}

/// The account of a test server that has no encryption and no login.
fn smtp_settings(port: u16) -> SmtpSettings {
    SmtpSettings {
        host: "127.0.0.1".to_owned(),
        port: Some(port),
        security: SmtpSecurity::None,
        user: String::new(),
        sender: "NiceCooling <nicecooling@example.org>".to_owned(),
    }
}

/// A source that gives the same account at each send.
fn fixed_account(account: SmtpAccount) -> SmtpSource {
    Arc::new(move || account.clone())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn email_response_logs_ok() {
    let dir = Dir::new("email-ok");
    let server = SmtpServer::start(SmtpBehavior::plain());
    // The source of the app reads the account from `settings.toml`.
    let settings_path = dir.path().join("settings.toml");
    let settings = AppSettings {
        smtp: smtp_settings(server.port),
        ..AppSettings::default()
    };
    settings.save(&settings_path).expect("the settings file");
    let source = smtp_source(settings_path, dir.graph_paths().secrets);
    let (notify, _requests) = unbounded_channel();
    let runner = KindRunner::new(notify).with_email(Handle::current(), source);
    let mut rig = Rig::build(
        dir,
        SimWallClock::new(WALL_START_US),
        Vec::new(),
        FakeRunner::default(),
        Box::new(runner),
    );

    run_once(&mut rig, email_node());
    // The send runs in a task: the service has no outcome yet.
    assert!(rig.view.lock().unwrap().log.is_empty());
    wait_for_log(&mut rig, 1);

    let log = rig.view.lock().unwrap().log.clone();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].kind, "email");
    assert_eq!(log[0].outcome, Outcome::Ok);
    assert_eq!(log[0].graph_id, "graph_2");
    assert_eq!(log[0].graph_name, "Hot water");
    assert_eq!(log[0].time_us, WALL_START_US);
    assert!(server.wait_for(|sessions| sessions.iter().any(|session| session.data.is_some())));
    let sessions = server.sessions();
    assert_eq!(sessions[0].rcpt_to, ["owner@example.org"]);
    assert!(
        sessions[0]
            .data_lines()
            .contains(&"Subject: Pump stopped".to_owned())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn email_response_without_an_account_is_not_configured() {
    let (notify, _requests) = unbounded_channel();
    let run = run_of(email_node(), Vec::new());

    // A runner that has no SMTP source.
    let mut bare = KindRunner::new(notify.clone());
    assert_eq!(bare.start(&run), Started::Done(Outcome::NotConfigured));

    // An account with no host: the send gives the outcome in its task.
    let empty = SmtpAccount::new(SmtpSettings::default(), Password::default());
    let mut runner = KindRunner::new(notify).with_email(Handle::current(), fixed_account(empty));
    assert_eq!(runner.start(&run), Started::Pending);
    assert_eq!(wait_outcome(&mut runner, &run.id), Outcome::NotConfigured);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn email_response_expands_placeholders() {
    let server = SmtpServer::start(SmtpBehavior::plain());
    let account = SmtpAccount::new(smtp_settings(server.port), Password::default());
    let (notify, _requests) = unbounded_channel();
    let mut runner = KindRunner::new(notify).with_email(Handle::current(), fixed_account(account));
    let node = email_node()
        .with_param("subject", "{graph}: {value:water}")
        .with_param(
            "text",
            "{graph}: water {value:water} {unit:water}, pump {value:pump}, at {time}. {{done}}",
        );
    let values = vec![
        labelled("water", Some(46.2), "rpm"),
        labelled("pump", None, ""),
    ];
    let run = run_of(node, values);

    assert_eq!(runner.start(&run), Started::Pending);
    assert_eq!(wait_outcome(&mut runner, &run.id), Outcome::Ok);

    let sessions = server.sessions();
    let lines = sessions[0].data_lines();
    assert!(
        lines.contains(&"Subject: Hot water: 46.2".to_owned()),
        "{lines:?}"
    );
    assert!(
        lines.contains(
            &"Hot water: water 46.2 rpm, pump --, at 2026-11-25 14:05:09. {done}".to_owned()
        ),
        "{lines:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn email_response_failed_and_timeout_give_their_outcomes() {
    let (notify, _requests) = unbounded_channel();

    // A recipient that is no address: the send fails before it connects.
    let account = SmtpAccount::new(smtp_settings(1), Password::default());
    let mut runner =
        KindRunner::new(notify.clone()).with_email(Handle::current(), fixed_account(account));
    let run = run_of(email_node().with_param("to", "owner"), Vec::new());
    assert_eq!(runner.start(&run), Started::Pending);
    assert_eq!(wait_outcome(&mut runner, &run.id), Outcome::Failed);

    // A server that waits 5 s before its greeting, and a limit of 300 ms in place of 30 s.
    let server = SmtpServer::start(SmtpBehavior::plain().slow(Duration::from_secs(5)));
    let account = SmtpAccount::new(smtp_settings(server.port), Password::default())
        .with_limit(Duration::from_millis(300));
    let mut runner = KindRunner::new(notify).with_email(Handle::current(), fixed_account(account));
    let run = run_of(email_node(), Vec::new());
    let started = Instant::now();
    assert_eq!(runner.start(&run), Started::Pending);
    assert_eq!(wait_outcome(&mut runner, &run.id), Outcome::Timeout);
    assert!(started.elapsed() < Duration::from_secs(3));
}
