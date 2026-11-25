//! The Graph service: it evaluates every enabled Graph once per second.
//!
//! `GraphService` holds no thread and no timer of its own. The caller feeds it the daemon events,
//! and calls `poll` as often as it likes: a pass runs when a second of its clock has passed. A test
//! replaces the clock, the `/proc` scan and the link to the daemon.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use nicecooling_core::clock::{Clock, SystemWallClock, WallClock};
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::eval::{EndValue, Evaluator, GraphPass, Pass, Sources, value_key};
use nicecooling_graph::model::{Graph, Kind, Node, ResponseKind, StoredGraph};
use nicecooling_graph::pause::PowerKind;
use nicecooling_graph::response::{Action, Timing};
use nicecooling_graph::template::labelled_values;
use tokio::sync::mpsc::UnboundedSender;

use super::channel::GraphCommand;
use super::power::{NOTIFICATION_KEY, Running, countdown_notification, failure_notification};
use super::responses::{NoRunner, Outcome, Responses, Run, Runner, Started};
use super::sensors::{self, CREATED_GRACE, SensorSpec};
use super::session::AlwaysActive;
use super::store::{GraphStore, describe, describe_invalid};
use super::try_run::TryRun;
use super::values::{ActiveSession, SensorError};
use super::view::{GraphRow, PauseView, RowState, SharedView, TryView};
use crate::i18n::Locale;
use crate::model::{AppState, DaemonEvent};
use crate::notify::{Level, NotificationSpec, Plan, open_button};
use crate::screens::format::{date_time_seconds, unit_text};

pub use super::power::{NoPower, PowerCall};

/// The time between two passes.
pub const PASS_INTERVAL: Duration = Duration::from_secs(1);

/// Where the Graph sensor values go after each pass, and the calls that make, rename and remove a
/// Graph sensor. The calls to the daemon are one implementation of it. A test records them.
///
/// The three calls fail with `Unavailable` on a link that has no daemon. The service then stores
/// the Graph without a call.
pub trait SensorLink: Send {
    /// The value of each Graph sensor, in the order of the Graphs. `None` is the no-value entry,
    /// also for the Graph sensor of a disabled Graph.
    fn values(&mut self, values: &[(String, Option<f64>)]);

    /// Make a Graph sensor and return its id.
    fn create_sensor(&mut self, _name: &str, _unit: &str) -> Result<String, SensorError> {
        Err(SensorError::Unavailable)
    }

    fn rename_sensor(&mut self, _id: &str, _name: &str, _unit: &str) -> Result<(), SensorError> {
        Err(SensorError::Unavailable)
    }

    fn delete_sensor(&mut self, _id: &str) -> Result<(), SensorError> {
        Err(SensorError::Unavailable)
    }
}

/// What a store of a Graph does with its Graph sensor.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Store {
    /// Apply: make the sensor, or rename it when the daemon holds another name or unit, and remove
    /// it when the Graph no longer has a sensor end node.
    Apply,
    /// Enable: make the sensor when the Graph has none.
    Enable,
    /// Disable: no call.
    Disable,
}

/// A link that sends nothing.
pub struct NoLink;

impl SensorLink for NoLink {
    fn values(&mut self, _values: &[(String, Option<f64>)]) {}
}

/// The sources of one pass: the newest values of the daemon, and the `/proc` scan of the user.
/// The scan runs at the first process node of the pass, and not in a pass without one.
pub struct LiveSources<'a> {
    state: &'a AppState,
    scan: &'a dyn ProcessSource,
    processes: OnceCell<ProcessList>,
}

impl<'a> LiveSources<'a> {
    pub fn new(state: &'a AppState, scan: &'a dyn ProcessSource) -> Self {
        Self {
            state,
            scan,
            processes: OnceCell::new(),
        }
    }
}

impl Sources for LiveSources<'_> {
    fn sensor(&self, id: &str) -> Option<f64> {
        self.state.sensor_value(id)
    }

    fn alarm_active(&self, event: &str, source: &str) -> Option<bool> {
        self.state.alarm_active(event, source)
    }

    fn processes(&self) -> &ProcessList {
        self.processes.get_or_init(|| self.scan.scan())
    }
}

/// What the service needs from the rest of the app. A test replaces each part.
pub struct Env {
    pub clock: Box<dyn Clock>,
    pub link: Box<dyn SensorLink>,
    pub scan: Box<dyn ProcessSource>,
    pub power: Box<dyn PowerCall>,
    /// The channel that the notification thread reads.
    pub notify: UnboundedSender<Plan>,
    pub locale: Locale,
    pub view: SharedView,
}

pub struct GraphService {
    store: GraphStore,
    evaluator: Evaluator,
    clock: Box<dyn Clock>,
    link: Box<dyn SensorLink>,
    scan: Box<dyn ProcessSource>,
    power: Box<dyn PowerCall>,
    /// The session of the app in logind. An inactive session runs no response.
    session: Box<dyn ActiveSession>,
    notify: UnboundedSender<Plan>,
    locale: Locale,
    /// The countdown of the power response that runs now.
    countdown: Option<Running>,
    /// The try of a draft that runs now. It has no part in the pass of the applied Graphs.
    trying: Option<TryRun>,
    /// The last pass, for the view between two passes.
    last_pass: Option<Pass>,
    state: AppState,
    view: SharedView,
    next_pass: Duration,
    /// The Graphs that a panic stopped. They stay off even when the file cannot be written.
    stopped: BTreeSet<String>,
    /// The last write error of the state file, so one failure logs one line.
    state_error: Option<String>,
    /// The Graph sensors that this service made, with the time of the call. A snapshot that the
    /// client read before the call does not list them, and must not make a second sensor.
    created: BTreeMap<String, Duration>,
    /// The state machines of the response end nodes, the run ids and the open runs.
    responses: Responses,
    /// Starts the responses that are no power response.
    runner: Box<dyn Runner>,
}

impl GraphService {
    pub fn new(store: GraphStore, env: Env) -> Self {
        let Env {
            clock,
            link,
            scan,
            power,
            notify,
            locale,
            view,
        } = env;
        let mut evaluator = Evaluator::new();
        for (key, value) in &store.state().values {
            if let Some((graph, node)) = key.rsplit_once('/')
                && let Ok(node) = node.parse::<u32>()
            {
                evaluator.restore_value(graph, node, *value);
            }
        }
        let mut service = Self {
            store,
            evaluator,
            clock,
            link,
            scan,
            power,
            session: Box::new(AlwaysActive),
            notify,
            locale,
            countdown: None,
            trying: None,
            last_pass: None,
            state: AppState::default(),
            view,
            next_pass: Duration::ZERO,
            stopped: BTreeSet::new(),
            state_error: None,
            created: BTreeMap::new(),
            responses: Responses::new(Box::new(SystemWallClock)),
            runner: Box::new(NoRunner),
        };
        service.publish();
        service.publish_log();
        service.publish_pause();
        service
    }

    /// Use another wall clock for the minimum interval and the run times.
    pub fn with_wall_clock(mut self, wall: Box<dyn WallClock>) -> Self {
        self.responses = Responses::new(wall);
        self
    }

    /// Use `runner` to start the responses that are no power response.
    pub fn with_runner(mut self, runner: Box<dyn Runner>) -> Self {
        self.runner = runner;
        self
    }

    /// Use the session of logind instead of the session that is always active.
    pub fn with_session(mut self, session: Box<dyn ActiveSession>) -> Self {
        self.session = session;
        self
    }

    /// Whether this app may run a Graph response now: only the app of the active session runs
    /// one. The Graphs are evaluated whatever this says.
    pub fn runs_responses(&self) -> bool {
        self.session.active()
    }

    /// Follow the daemon. A new snapshot starts the comparison of the Graphs with the Graph
    /// sensors of the daemon.
    pub fn apply_event(&mut self, event: &DaemonEvent) {
        self.state.apply(event);
        self.enforce_alarm_priority();
        if matches!(event, DaemonEvent::Snapshot(_)) {
            self.compare_sensors();
        }
    }

    /// Make the Graph sensor that a Graph holds and the daemon does not list, and store its id in
    /// the Graph. A Graph sensor that no Graph holds stays, and the view lists it.
    fn compare_sensors(&mut self) {
        let now = self.clock.now();
        let listed = &self.state.snapshot.sensors;
        self.created
            .retain(|id, at| !listed.contains_key(id) && now.saturating_sub(*at) < CREATED_GRACE);
        let graphs = self.store.graphs();
        for id in sensors::missing(&graphs, &self.state.snapshot.sensors, &self.created) {
            let Some(mut graph) = self.store.graph(&id).cloned() else {
                continue;
            };
            let Some(spec) = sensors::spec_of(&graph) else {
                continue;
            };
            match self.link.create_sensor(&spec.name, &spec.unit) {
                Ok(sensor) => {
                    self.created.insert(sensor.clone(), now);
                    graph.sensor_id = Some(sensor);
                    if let Err(error) = self.store.put(graph) {
                        self.refuse(error.to_string());
                    }
                }
                Err(SensorError::Unavailable) => {}
                Err(error) => self.refuse(format!("{}: {error}", spec.name)),
            }
        }
        self.publish();
    }

    /// The time until the service has work again: the next pass, or the end of a countdown.
    pub fn until_next_wake(&self) -> Duration {
        let now = self.clock.now();
        let pass = self.next_pass.saturating_sub(now);
        match &self.countdown {
            Some(countdown) => pass.min(countdown.ends.saturating_sub(now)),
            None => pass,
        }
    }

    /// Do the work that is due: the end of a countdown, and a pass when a second has passed since
    /// the last one. Returns whether a pass ran.
    pub fn poll(&mut self) -> bool {
        let now = self.clock.now();
        self.collect_outcomes();
        self.tick_countdown(now);
        if now < self.next_pass {
            return false;
        }
        // A late pass (a suspend, a stop of the thread) does not run again to catch up.
        self.next_pass = if now - self.next_pass >= PASS_INTERVAL {
            now + PASS_INTERVAL
        } else {
            self.next_pass + PASS_INTERVAL
        };
        self.pass(now);
        true
    }

    /// Start the countdown of a power response of the Graph `graph_id`. One countdown runs at a
    /// time: a second start returns false.
    pub fn start_countdown(&mut self, graph_id: &str, kind: PowerKind, seconds: u32) -> bool {
        let Some(graph) = self.store.graph(graph_id) else {
            return false;
        };
        let time_us = self.responses.wall_us();
        let run = Run {
            id: self.responses.next_run_id(time_us),
            key: value_key(graph_id, 0),
            graph_id: graph_id.to_owned(),
            graph_name: graph.name.clone(),
            node: Node::new(0, Kind::ResponseEnd),
            kind: response_kind(kind),
            time_us,
            time_text: date_time_seconds(time_us),
            values: Vec::new(),
        };
        self.begin_power(run, kind, seconds)
    }

    /// Start the countdown of a run. A node that has a run with no outcome, and a second power
    /// response while a countdown runs, get the outcome `busy`. While the daemon reports the
    /// poweroff countdown of an Alarm, no power response starts. Returns whether it started.
    fn begin_power(&mut self, run: Run, kind: PowerKind, seconds: u32) -> bool {
        if self.alarm_poweroff_pending() {
            return false;
        }
        if self.countdown.is_some() || self.responses.is_open(&run.key) {
            self.log_run(&run, Outcome::Busy);
            return false;
        }
        let now = self.clock.now();
        self.store.state_mut().pause.start_run(kind);
        self.responses.open(run.clone());
        self.countdown = Some(Running::new(run, kind, seconds, now));
        self.publish_countdown();
        true
    }

    /// Whether the daemon reports a poweroff countdown of an Alarm. The Alarm wins over every Graph
    /// power response.
    pub fn alarm_poweroff_pending(&self) -> bool {
        self.state.is_online() && self.state.next_poweroff().is_some()
    }

    /// The countdown of a Graph power response ends when the Alarm countdown starts.
    fn enforce_alarm_priority(&mut self) {
        if self.countdown.is_some() && self.alarm_poweroff_pending() {
            self.cancel_countdown();
        }
    }

    /// Cancel the countdown. No power call follows. The run gives back its count for the Response
    /// pause, and its outcome is `cancelled`. A cancel that comes after the call changes nothing.
    pub fn cancel_countdown(&mut self) {
        let Some(running) = self.countdown.take() else {
            return;
        };
        self.store.state_mut().pause.cancel_run(running.kind);
        self.finish_power(&running, Outcome::Cancelled);
    }

    fn tick_countdown(&mut self, now: Duration) {
        self.enforce_alarm_priority();
        let Some(running) = &self.countdown else {
            return;
        };
        if now >= running.ends {
            self.end_countdown(now);
        } else {
            self.publish_countdown();
        }
    }

    /// The countdown is over: write `graph-state.json`, then call logind. A state that cannot be
    /// written makes no call, because after a restart the response could run a second time.
    fn end_countdown(&mut self, now: Duration) {
        let Some(running) = self.countdown.take() else {
            return;
        };
        let kind = running.kind;
        // A session that is not active runs no response.
        if !self.session.active() {
            self.store.state_mut().pause.cancel_run(kind);
            self.finish_power(&running, Outcome::Cancelled);
            return;
        }
        self.store.state_mut().values = self.evaluator.stored_values();
        let written = match self.store.file_error() {
            Some(error) => Err(error.to_owned()),
            None => self
                .store
                .write_state(now, true)
                .map(|_| ())
                .map_err(|error| error.to_string()),
        };
        let result = written.and_then(|()| self.power.call(kind));
        let outcome = match result {
            Ok(()) => Outcome::Ok,
            Err(error) => {
                eprintln!("nicecooling: {}: {error}", running.run.graph_name);
                let spec = failure_notification(&running.run.graph_name, &error, self.locale);
                let _ = self.notify.send(Plan::Show(spec));
                Outcome::Failed
            }
        };
        self.finish_power(&running, outcome);
    }

    /// The run of the countdown has its outcome: log it and close its notification.
    fn finish_power(&mut self, running: &Running, outcome: Outcome) {
        self.responses.close(&running.run.id);
        self.log_run(&running.run, outcome);
        let _ = self.notify.send(Plan::Close(NOTIFICATION_KEY.to_owned()));
        self.publish_countdown();
    }

    /// Show the countdown in the view, and in its notification once per second.
    fn publish_countdown(&mut self) {
        let now = self.clock.now();
        let shown = self.countdown.as_ref().map(|running| running.view(now));
        if let (Some(countdown), Some(running)) = (&shown, &mut self.countdown)
            && running.shown_s != Some(countdown.seconds_left)
        {
            running.shown_s = Some(countdown.seconds_left);
            let spec = countdown_notification(countdown, self.locale);
            let _ = self.notify.send(Plan::Show(spec));
        }
        if let Ok(mut view) = self.view.lock() {
            view.countdown = shown;
        }
        // Every start and every end of a countdown can change the count of the Response pause.
        self.publish_pause();
    }

    /// One command of the window, the tray or a notification.
    pub fn handle(&mut self, command: GraphCommand) {
        match command {
            GraphCommand::Apply(graph) => self.store_graph(*graph, Store::Apply),
            GraphCommand::Enable { graph, enabled } => {
                if let Some(mut graph) = self.store.graph(&graph).cloned() {
                    graph.enabled = enabled;
                    self.store_graph(
                        graph,
                        if enabled {
                            Store::Enable
                        } else {
                            Store::Disable
                        },
                    );
                }
            }
            GraphCommand::Delete { graph } => self.delete_graph(&graph),
            GraphCommand::CancelCountdown => self.cancel_countdown(),
            GraphCommand::ResponsesOn => {
                self.store.state_mut().pause.turn_on();
                self.write_state(self.clock.now(), true);
                self.publish_pause();
            }
            GraphCommand::Try(request) => {
                let now = self.clock.now();
                match TryRun::start(&request, &self.store.all_graphs(), now) {
                    Ok(run) => {
                        self.trying = Some(run);
                        self.step_try(now);
                    }
                    Err(error) => self.refuse(error.to_string()),
                }
            }
            GraphCommand::StopTry => {
                self.trying = None;
                self.publish_try(None);
            }
            GraphCommand::RunOnce { graph } => self.run_once(*graph),
            // The MQTT and SMTP settings belong to the section of the network clients.
            GraphCommand::NetworkSettingsEdited => {}
        }
    }

    /// Validate a Graph, make or rename its Graph sensor, and store it. Apply starts the state of
    /// its nodes again. A refusal of the daemon stores nothing.
    fn store_graph(&mut self, graph: Graph, mode: Store) {
        let refusals = self.store.validate(&graph);
        if !refusals.is_empty() {
            self.refuse(format!("{}: {}", graph.name, describe(&refusals)));
            return;
        }
        // A Graph file that cannot be written must not leave a sensor behind.
        if let Some(error) = self.store.file_error() {
            self.refuse(error.to_owned());
            return;
        }
        let old = self.store.graph(&graph.id).cloned();
        let (graph, made) = match self.sync_sensor(graph, old.as_ref(), mode) {
            Ok(done) => done,
            Err(reason) => {
                self.refuse(reason);
                return;
            }
        };
        let id = graph.id.clone();
        match self.store.put(graph.clone()) {
            Ok(()) => {
                if mode == Store::Apply {
                    self.evaluator.apply(&graph);
                }
                self.stopped.remove(&id);
                if let Ok(mut view) = self.view.lock() {
                    view.refusal = None;
                }
            }
            Err(error) => {
                if let Some(sensor) = made {
                    let _ = self.link.delete_sensor(&sensor);
                    self.created.remove(&sensor);
                }
                self.refuse(error.to_string());
            }
        }
        self.publish();
    }

    /// The calls for the Graph sensor of a Graph. Returns the Graph with its `sensor_id`, and the
    /// id of a sensor that this call made. The reason of a refusal is the error.
    fn sync_sensor(
        &mut self,
        mut graph: Graph,
        old: Option<&Graph>,
        mode: Store,
    ) -> Result<(Graph, Option<String>), String> {
        let old_sensor = old.and_then(|old| old.sensor_id.clone());
        let name = graph.name.clone();
        let fail = |error: SensorError| format!("{name}: {error}");
        let mut made = None;
        match sensors::spec_of(&graph) {
            Some(spec) => {
                // A draft without the id of the stored Graph must not make a second sensor.
                if graph.sensor_id.is_none() {
                    graph.sensor_id = old_sensor;
                }
                match graph.sensor_id.clone() {
                    None if mode != Store::Disable => {
                        match self.link.create_sensor(&spec.name, &spec.unit) {
                            Ok(sensor) => {
                                self.created.insert(sensor.clone(), self.clock.now());
                                graph.sensor_id = Some(sensor.clone());
                                made = Some(sensor);
                            }
                            Err(SensorError::Unavailable) => {}
                            Err(error) => return Err(fail(error)),
                        }
                    }
                    Some(sensor) if mode == Store::Apply && self.sensor_differs(&sensor, &spec) => {
                        match self.link.rename_sensor(&sensor, &spec.name, &spec.unit) {
                            Ok(()) | Err(SensorError::Unavailable) => {}
                            Err(error) => return Err(fail(error)),
                        }
                    }
                    _ => {}
                }
            }
            None => {
                graph.sensor_id = None;
                if mode == Store::Apply
                    && let Some(sensor) = old_sensor
                {
                    match self.link.delete_sensor(&sensor) {
                        Ok(()) | Err(SensorError::Missing | SensorError::Unavailable) => {
                            self.created.remove(&sensor);
                        }
                        Err(error) => return Err(fail(error)),
                    }
                }
            }
        }
        Ok((graph, made))
    }

    fn sensor_differs(&self, id: &str, spec: &SensorSpec) -> bool {
        sensors::differs(&self.state.snapshot.sensors, id, spec)
    }

    /// Delete a Graph. The Graph sensor goes first, and a sensor that something uses keeps the Graph.
    fn delete_graph(&mut self, id: &str) {
        let held = self
            .store
            .graph(id)
            .map(|graph| (graph.name.clone(), graph.sensor_id.clone()));
        if let Some((name, Some(sensor))) = held {
            match self.link.delete_sensor(&sensor) {
                Ok(()) | Err(SensorError::Missing | SensorError::Unavailable) => {
                    self.created.remove(&sensor);
                }
                Err(error) => {
                    self.refuse(format!("{name}: {error}"));
                    return;
                }
            }
        }
        match self.store.remove(id) {
            Ok(()) => {
                self.evaluator.remove_graph(id);
                if let Ok(mut view) = self.view.lock() {
                    view.refusal = None;
                }
            }
            Err(error) => self.refuse(error.to_string()),
        }
        self.stopped.remove(id);
        self.publish();
    }

    fn refuse(&self, reason: String) {
        if let Ok(mut view) = self.view.lock() {
            view.refusal = Some(reason);
        }
    }

    fn pass(&mut self, now: Duration) {
        let mut graphs = self.store.graphs();
        for graph in &mut graphs {
            graph.enabled &= !self.stopped.contains(&graph.id);
        }
        let sources = LiveSources::new(&self.state, self.scan.as_ref());
        let mut pass = self.evaluator.evaluate(&graphs, &sources, now);
        for id in std::mem::take(&mut pass.panicked) {
            self.stop_graph(&id);
        }
        self.link.values(&pass.sensor_values());
        self.run_responses(now, &graphs, &pass);
        self.store.state_mut().values = self.evaluator.stored_values();
        self.write_state(now, false);
        self.last_pass = Some(pass);
        self.step_try(now);
        self.publish();
    }

    /// Step the response end nodes of one pass and start the runs that are due. An app of a session
    /// that is not active steps nothing: it runs no response and changes no run state.
    fn run_responses(&mut self, now: Duration, graphs: &[Graph], pass: &Pass) {
        let seconds = self.responses.app_seconds(now);
        self.store.state_mut().pause.advance(seconds);
        let keys: BTreeSet<String> = graphs
            .iter()
            .filter(|graph| graph.enabled)
            .filter_map(|graph| {
                graph
                    .response_end()
                    .map(|node| value_key(&graph.id, node.id))
            })
            .collect();
        // The machine of a node that is gone or off starts again from its stored record.
        self.responses.retain(&keys);
        if !self.session.active() {
            return;
        }
        for graph in graphs.iter().filter(|graph| graph.enabled) {
            let Some(node) = graph.response_end() else {
                continue;
            };
            let Some(result) = pass.graph(&graph.id).filter(|result| result.evaluated) else {
                continue;
            };
            let EndValue::Response(input) = result.end else {
                continue;
            };
            let key = value_key(&graph.id, node.id);
            let action = self.responses.step(
                &key,
                &Timing::from_node(node),
                input,
                now,
                &mut self.store.state_mut().responses,
            );
            // The Response pause: no response runs by itself. The state machine has moved on, so a
            // condition that stays true runs nothing when the pause ends.
            if action == Action::Run && !self.store.state().pause.paused {
                let run = self.make_run(graph, node, result, key);
                self.start_run(run);
            }
        }
    }

    /// "Run once": run the response of the draft `draft` one time at once. It ignores the state
    /// machine, the minimum interval and the Response pause. A poweroff or reboot counts for the
    /// pause. No power response starts while the daemon reports the poweroff countdown of an
    /// Alarm, and an app of a session that is not active runs nothing.
    fn run_once(&mut self, draft: Graph) {
        let Some(node) = draft.response_end().cloned() else {
            eprintln!(
                "nicecooling: run once: the Graph {} has no response end node",
                draft.id
            );
            return;
        };
        let kind = node.response_kind();
        if !self.session.active() {
            self.refuse(self.locale.tr("graph_refusal.inactive_session"));
            return;
        }
        if kind.is_power() && self.alarm_poweroff_pending() {
            self.refuse(self.locale.tr("graph_refusal.alarm_countdown"));
            return;
        }
        // The values come from the last pass of the applied Graph with the same id.
        let time_us = self.responses.wall_us();
        let sensors = &self.state.snapshot.sensors;
        let values = match (self.store.graph(&draft.id), self.last_pass.as_ref()) {
            (Some(applied), Some(pass)) => pass
                .graph(&applied.id)
                .map(|result| {
                    labelled_values(applied, result, |id| {
                        sensors
                            .get(id)
                            .map(|sensor| unit_text(&sensor.unit).to_owned())
                            .unwrap_or_default()
                    })
                })
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        let run = Run {
            id: self.responses.next_run_id(time_us),
            key: value_key(&draft.id, node.id),
            graph_id: draft.id.clone(),
            graph_name: draft.name.clone(),
            node,
            kind,
            time_us,
            time_text: date_time_seconds(time_us),
            values,
        };
        if let Ok(mut view) = self.view.lock() {
            view.refusal = None;
        }
        self.start_run(run);
    }

    /// The Response pause for the window.
    fn publish_pause(&self) {
        let pause = self.store.state().pause;
        if let Ok(mut view) = self.view.lock() {
            view.pause = PauseView {
                paused: pause.paused,
                count: pause.count,
            };
        }
    }

    /// The run of a response end node that fires in this pass.
    fn make_run(&mut self, graph: &Graph, node: &Node, result: &GraphPass, key: String) -> Run {
        let time_us = self.responses.wall_us();
        let sensors = &self.state.snapshot.sensors;
        let values = labelled_values(graph, result, |id| {
            sensors
                .get(id)
                .map(|sensor| unit_text(&sensor.unit).to_owned())
                .unwrap_or_default()
        });
        Run {
            id: self.responses.next_run_id(time_us),
            key,
            graph_id: graph.id.clone(),
            graph_name: graph.name.clone(),
            node: node.clone(),
            kind: node.response_kind(),
            time_us,
            time_text: date_time_seconds(time_us),
            values,
        }
    }

    /// Start a run. A node that has a run with no outcome gets the outcome `busy`.
    fn start_run(&mut self, run: Run) {
        if self.responses.is_open(&run.key) {
            self.log_run(&run, Outcome::Busy);
            return;
        }
        if let Some(kind) = PowerKind::from_response(run.kind) {
            // A countdown of less than 10 s leaves no time to cancel.
            let seconds = run.node.number("countdown_s").clamp(10.0, 3600.0) as u32;
            self.begin_power(run, kind, seconds);
            return;
        }
        match self.runner.start(&run) {
            Started::Done(outcome) => self.log_run(&run, outcome),
            Started::Pending => self.responses.open(run),
        }
    }

    /// The runs that had no outcome and have one now.
    fn collect_outcomes(&mut self) {
        for (id, outcome) in self.runner.finished() {
            if let Some(run) = self.responses.close(&id) {
                self.log_run(&run, outcome);
            }
        }
    }

    /// Write the row of a run to the response log.
    fn log_run(&mut self, run: &Run, outcome: Outcome) {
        if let Err(error) = self.store.push_log(run.row(outcome)) {
            eprintln!("nicecooling: response log: {error}");
        }
        self.publish_log();
    }

    fn publish_log(&self) {
        if let Ok(mut view) = self.view.lock() {
            view.log = self.store.log().to_vec();
        }
    }

    /// One evaluation of the try, and the end of it after its time. A try sends no Graph sensor
    /// value and starts no response.
    fn step_try(&mut self, now: Duration) {
        let Some(run) = &mut self.trying else {
            return;
        };
        if run.finished(now) {
            self.trying = None;
            self.publish_try(None);
            return;
        }
        let sources = LiveSources::new(&self.state, self.scan.as_ref());
        let shown = run.step(&sources, now).map(|pass| TryView {
            graph_id: run.graph_id().to_owned(),
            seconds_left: run.seconds_left(now),
            values: pass.values,
            end: pass.end,
        });
        if shown.is_none() {
            // The draft panicked: the try ends.
            self.trying = None;
        }
        self.publish_try(shown);
    }

    fn publish_try(&self, shown: Option<TryView>) {
        if let Ok(mut view) = self.view.lock() {
            view.trying = shown;
        }
    }

    /// A Graph panicked: store it as disabled, and say so.
    fn stop_graph(&mut self, id: &str) {
        let Some(mut graph) = self.store.graph(id).cloned() else {
            return;
        };
        self.stopped.insert(id.to_owned());
        // The pass that panicked can leave a node state half done: the next run starts clean.
        self.evaluator.apply(&graph);
        graph.enabled = false;
        if let Err(error) = self.store.put(graph.clone()) {
            eprintln!("nicecooling: graph {id}: cannot store it as disabled: {error}");
        }
        let locale = self.locale;
        let _ = self.notify.send(Plan::Show(NotificationSpec {
            key: format!("graph-stopped-{id}"),
            title: locale.fmt("notification.graph_stopped.title", &[("name", &graph.name)]),
            body: locale.tr("notification.graph_stopped.body"),
            level: Level::Normal,
            buttons: vec![open_button(locale)],
            sound: None,
        }));
    }

    fn write_state(&mut self, now: Duration, force: bool) {
        let result = self.store.write_state(now, force);
        match (result, &self.state_error) {
            (Err(error), previous) if previous.as_deref() != Some(&error.to_string()) => {
                eprintln!("nicecooling: graph state: {error}");
                self.state_error = Some(error.to_string());
            }
            (Ok(_), _) => self.state_error = None,
            _ => {}
        }
    }

    /// Write the view for the screens.
    fn publish(&mut self) {
        let pass = self.last_pass.as_ref();
        let rows: Vec<GraphRow> = self
            .store
            .entries()
            .iter()
            .map(|entry| match entry {
                StoredGraph::Valid(graph) => row_of(graph, pass),
                StoredGraph::Invalid(invalid) => GraphRow {
                    id: invalid.id.clone().unwrap_or_default(),
                    name: invalid.name.clone().unwrap_or_default(),
                    enabled: false,
                    state: RowState::Invalid(describe_invalid(invalid)),
                    values: Default::default(),
                    end: nicecooling_graph::eval::EndValue::None,
                },
            })
            .collect();
        let error = self.store.error().map(str::to_owned);
        let foreign = sensors::foreign(self.store.entries(), &self.state.snapshot.sensors);
        if let Ok(mut view) = self.view.lock() {
            view.rows = rows;
            view.error = error;
            view.foreign_sensors = foreign;
        }
    }

    /// The service ends after a panic outside the evaluation of a Graph: the view shows an error
    /// and a notification names the Graph service. The caller ends the thread.
    pub fn fail(&self, reason: &str) {
        // A panic can leave the mutex poisoned: the error shows anyway.
        let mut view = self
            .view
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        view.error = Some(format!("the Graph service stopped: {reason}"));
        drop(view);
        let locale = self.locale;
        let _ = self.notify.send(Plan::Show(NotificationSpec {
            key: "graph-service-stopped".to_owned(),
            title: locale.tr("notification.graph_service_stopped.title"),
            body: locale.tr("notification.graph_service_stopped.body"),
            level: Level::Normal,
            buttons: vec![open_button(locale)],
            sound: None,
        }));
    }

    /// The stop of the app: write the state file whatever the time since the last write.
    pub fn stop(&mut self) {
        self.cancel_countdown();
        let now = self.clock.now();
        self.store.state_mut().values = self.evaluator.stored_values();
        self.write_state(now, true);
    }

    pub fn store(&self) -> &GraphStore {
        &self.store
    }
}

fn row_of(graph: &Graph, pass: Option<&Pass>) -> GraphRow {
    let result = pass.and_then(|p| p.graph(&graph.id));
    GraphRow {
        id: graph.id.clone(),
        name: graph.name.clone(),
        enabled: graph.enabled,
        state: if graph.enabled {
            RowState::Running
        } else {
            RowState::Disabled
        },
        values: result
            .filter(|r| r.evaluated)
            .map(|r| r.values.clone())
            .unwrap_or_default(),
        end: result.map_or(nicecooling_graph::eval::EndValue::None, |r| r.end.clone()),
    }
}

/// The response kind of a power kind.
fn response_kind(kind: PowerKind) -> ResponseKind {
    match kind {
        PowerKind::Poweroff => ResponseKind::Poweroff,
        PowerKind::Reboot => ResponseKind::Reboot,
        PowerKind::Suspend => ResponseKind::Suspend,
    }
}
