//! The Graph service: it evaluates every enabled Graph once per second.
//!
//! `GraphService` holds no thread and no timer of its own. The caller feeds it the daemon events,
//! and calls `poll` as often as it likes: a pass runs when a second of its clock has passed. A test
//! replaces the clock, the `/proc` scan and the link to the daemon.

use std::cell::OnceCell;
use std::collections::BTreeSet;
use std::time::Duration;

use nicecooling_core::clock::Clock;
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::eval::{Evaluator, Pass, Sources};
use nicecooling_graph::model::{Graph, StoredGraph};
use tokio::sync::mpsc::UnboundedSender;

use super::store::{GraphStore, describe_invalid};
use super::view::{GraphRow, RowState, SharedView};
use crate::i18n::Locale;
use crate::model::{AppState, DaemonEvent};
use crate::notify::{Level, NotificationSpec, Plan, open_button};

/// The time between two passes.
pub const PASS_INTERVAL: Duration = Duration::from_secs(1);

/// Where the Graph sensor values go after each pass. The call to the daemon is one
/// implementation of it. A test records the values.
pub trait SensorLink: Send {
    /// The value of each Graph sensor, in the order of the Graphs. `None` is the no-value entry,
    /// also for the Graph sensor of a disabled Graph.
    fn values(&mut self, values: &[(String, Option<f64>)]);
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
    notify: UnboundedSender<Plan>,
    locale: Locale,
    state: AppState,
    view: SharedView,
    next_pass: Duration,
    /// The Graphs that a panic stopped. They stay off even when the file cannot be written.
    stopped: BTreeSet<String>,
    /// The last write error of the state file, so one failure logs one line.
    state_error: Option<String>,
}

impl GraphService {
    pub fn new(store: GraphStore, env: Env) -> Self {
        let Env {
            clock,
            link,
            scan,
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
            notify,
            locale,
            state: AppState::default(),
            view,
            next_pass: Duration::ZERO,
            stopped: BTreeSet::new(),
            state_error: None,
        };
        service.publish(None);
        service
    }

    /// Follow the daemon.
    pub fn apply_event(&mut self, event: &DaemonEvent) {
        self.state.apply(event);
    }

    /// The time until the next pass is due.
    pub fn until_next_pass(&self) -> Duration {
        self.next_pass.saturating_sub(self.clock.now())
    }

    /// Run a pass when a second has passed since the last one. Returns whether it ran.
    pub fn poll(&mut self) -> bool {
        let now = self.clock.now();
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
        self.store.state_mut().values = self.evaluator.stored_values();
        self.write_state(now, false);
        self.publish(Some(&pass));
    }

    /// A Graph panicked: store it as disabled, and say so.
    fn stop_graph(&mut self, id: &str) {
        let Some(mut graph) = self.store.graph(id).cloned() else {
            return;
        };
        self.stopped.insert(id.to_owned());
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
    fn publish(&mut self, pass: Option<&Pass>) {
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
        if let Ok(mut view) = self.view.lock() {
            view.rows = rows;
            view.error = error;
        }
    }

    /// The stop of the app: write the state file whatever the time since the last write.
    pub fn stop(&mut self) {
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
