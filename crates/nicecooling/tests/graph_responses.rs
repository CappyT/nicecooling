mod support;

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling::graphs::responses::{Outcome, Run, Runner, Started};
use nicecooling::graphs::service::{Env, GraphService, NoLink, NoPower};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::view::{SharedView, shared_view};
use nicecooling::i18n::EN;
use nicecooling::model::{DaemonEvent, Link, SensorSample};
use nicecooling::notify::Plan;
use nicecooling_core::clock::{SimClock, SimWallClock};
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};
use support::graphs::Dir;
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
        let mut store = GraphStore::open(dir.graph_paths());
        for graph in graphs {
            store.put(graph).unwrap();
        }
        let clock = SimClock::default();
        let runner = FakeRunner::default();
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
        .with_runner(Box::new(runner.clone()));
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
