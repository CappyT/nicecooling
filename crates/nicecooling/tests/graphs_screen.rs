mod support;

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling::command::Command;
use nicecooling::graphs::channel::GraphCommand;
use nicecooling::graphs::service::{Env, GraphService, NoPower, SensorLink};
use nicecooling::graphs::store::GraphStore;
use nicecooling::graphs::transfer::{MAX_IMPORT_BYTES, export_file, export_text};
use nicecooling::graphs::values::SensorError;
use nicecooling::graphs::view::{SharedView, shared_view};
use nicecooling::i18n::EN;
use nicecooling::model::{AppState, DaemonEvent, Link, SensorSample};
use nicecooling::notify::Plan;
use nicecooling::screens::format::{clock, date_time};
use nicecooling::screens::graphs::{
    Asked, GraphsModel, GraphsState, GraphsUi, LineState, Notice, shown_path,
};
use nicecooling::screens::{Feedback, ScreenInput, WriteOutcome};
use nicecooling_core::clock::{SimClock, SimWallClock};
use nicecooling_core::rules::proc_scan::ProcessSource;
use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::model::{Connection, Graph, Kind, Node};
use support::fixtures::{full, online, sensor};
use support::graphs::{Dir, constant_graph};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

/// The wall clock at the start of a test: 2025-12-17 19:33:20 UTC.
const WALL_START_US: u64 = 1_766_000_000_000_000;

struct NoScan;

impl ProcessSource for NoScan {
    fn scan(&self) -> ProcessList {
        ProcessList::default()
    }
}

/// A daemon that makes and removes Graph sensors, and that can refuse both.
#[derive(Clone, Default)]
struct FakeLink {
    /// The text of the `InUse` answer to a delete. `None`: the delete works.
    in_use: Arc<Mutex<Option<String>>>,
    /// The text of the refusal of a create. `None`: the create works.
    no_create: Arc<Mutex<Option<String>>>,
    /// The names of the sensors that the service asked for.
    created: Arc<Mutex<Vec<String>>>,
    deleted: Arc<Mutex<Vec<String>>>,
}

impl SensorLink for FakeLink {
    fn values(&mut self, _values: &[(String, Option<f64>)]) {}

    fn create_sensor(&mut self, name: &str, _unit: &str) -> Result<String, SensorError> {
        match self.no_create.lock().unwrap().clone() {
            Some(text) => Err(SensorError::Refused(text)),
            None => {
                self.created.lock().unwrap().push(name.to_owned());
                Ok("graph_9".to_owned())
            }
        }
    }

    fn delete_sensor(&mut self, id: &str) -> Result<(), SensorError> {
        match self.in_use.lock().unwrap().clone() {
            Some(text) => Err(SensorError::InUse(text)),
            None => {
                self.deleted.lock().unwrap().push(id.to_owned());
                Ok(())
            }
        }
    }
}

/// A Graph with a sensor end node that gives the sensor `x`. It holds the Graph sensor `graph_3`.
fn sensor_graph(id: &str) -> Graph {
    let mut graph = Graph::new(id, "Render temperature");
    graph.nodes = vec![
        Node::new(1, Kind::SensorSource).with_param("sensor", "x"),
        Node::new(2, Kind::SensorEnd).with_param("unit", "degC"),
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    graph.sensor_id = Some("graph_3".to_owned());
    graph
}

/// A Graph whose notification response fires while the sensor `x` is above 0.
fn response_graph(id: &str) -> Graph {
    let mut graph = Graph::new(id, "Water too warm");
    graph.nodes = vec![
        Node::new(1, Kind::SensorSource).with_param("sensor", "x"),
        Node::new(2, Kind::ResponseEnd)
            .with_param("kind", "notification")
            .with_param("title", "Hot")
            .with_param("hold_s", 0)
            .with_param("min_interval_s", 0),
    ];
    graph.connections = vec![Connection::new(1, 2, "a")];
    graph
}

/// The Graph service with a simulated clock and the fake daemon link.
struct Rig {
    service: GraphService,
    clock: SimClock,
    view: SharedView,
    link: FakeLink,
    dir: Dir,
    _notifications: UnboundedReceiver<Plan>,
}

impl Rig {
    fn new(name: &str, graphs: Vec<Graph>) -> Self {
        let dir = Dir::new(name);
        let mut store = GraphStore::open(dir.graph_paths());
        for graph in graphs {
            store.put(graph).unwrap();
        }
        Self::with_store(dir, store)
    }

    fn with_store(dir: Dir, store: GraphStore) -> Self {
        let clock = SimClock::default();
        let view = shared_view();
        let link = FakeLink::default();
        let (notify, notifications) = unbounded_channel();
        let service = GraphService::new(
            store,
            Env {
                clock: Box::new(clock.clone()),
                link: Box::new(link.clone()),
                scan: Box::new(NoScan),
                power: Box::new(NoPower),
                notify,
                locale: EN,
                view: view.clone(),
            },
        )
        .with_wall_clock(Box::new(SimWallClock::new(WALL_START_US)));
        let mut rig = Self {
            service,
            clock,
            view,
            link,
            dir,
            _notifications: notifications,
        };
        rig.service.apply_event(&DaemonEvent::Link(Link::Online));
        rig
    }

    /// The sensor `x` of the daemon has the value `value`.
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

    /// Give a command of the screen to the service, as the app does.
    fn run(&mut self, command: Command) {
        match command {
            Command::Graph(command) => self.service.handle(command),
            other => panic!("not a Graph command: {other:?}"),
        }
    }

    /// The model of the screen at the wall clock time `now_us`.
    fn model_at(&self, state: &AppState, ui: &GraphsUi, now_us: u64) -> GraphsModel {
        self.model_with(state, ui, now_us, &Feedback::default())
    }

    fn model_with(
        &self,
        state: &AppState,
        ui: &GraphsUi,
        now_us: u64,
        feedback: &Feedback,
    ) -> GraphsModel {
        let shown = self.view.lock().unwrap().clone();
        GraphsModel::build(
            &ScreenInput {
                state,
                now_us,
                locale: EN,
                down_since_us: None,
                feedback,
            },
            Some(&shown),
            ui,
        )
    }

    fn model(&self, state: &AppState, ui: &GraphsUi) -> GraphsModel {
        self.model_at(state, ui, WALL_START_US)
    }
}

#[test]
fn row_of_a_sensor_graph_shows_46_2_running() {
    let mut rig = Rig::new("screen-sensor-row", vec![sensor_graph("graph_1")]);
    let ui = GraphsUi::default();
    let state = online();

    // The source has no value yet: "--", and the state stays "Running".
    rig.ticks(1);
    let model = rig.model(&state, &ui);
    assert_eq!(model.state, GraphsState::Ready);
    let row = &model.rows[0];
    assert_eq!((row.result.as_str(), row.state), ("--", LineState::Running));
    assert!(!row.dim && row.value.is_none());

    rig.feed(46.2);
    rig.ticks(1);
    let model = rig.model(&state, &ui);
    let row = &model.rows[0];
    assert_eq!(row.name, "Render temperature");
    assert_eq!(row.caption, "2 nodes");
    assert!(!row.caption_warns);
    assert_eq!(row.result, "46.2 °C");
    assert_eq!(row.value, Some(("46.2".to_owned(), "°C".to_owned())));
    assert_eq!(row.state, LineState::Running);
    assert_eq!(row.state_text, "Running");
    assert_eq!(row.end, "Sensor");
    assert_eq!(row.end_caption.as_deref(), Some("graph_3"));
    assert!(row.end_mono && row.checked && row.switch_enabled);
    assert!(row.readable && row.delete_enabled && !row.fired);
    assert_eq!(
        model.columns,
        ["Graph", "End node", "State", "Result"].map(str::to_owned)
    );
    assert_eq!(
        (
            model.edit.as_str(),
            model.export.as_str(),
            model.delete.as_str()
        ),
        ("Edit", "Export", "Delete")
    );

    // The daemon is offline: the result is "--" in `fg-dim`, and "Delete" is off.
    let mut offline = online();
    offline.link = Link::Offline;
    let model = rig.model(&offline, &ui);
    let row = &model.rows[0];
    assert_eq!(row.result, "--");
    assert!(row.dim && !row.delete_enabled);
    assert_eq!(
        row.delete_tooltip.as_deref(),
        Some("The daemon is offline.")
    );
    assert!(row.switch_enabled, "the Graph has its Graph sensor");
}

#[test]
fn row_of_a_response_graph_shows_fired_with_the_time() {
    let mut rig = Rig::new("screen-response-row", vec![response_graph("graph_2")]);
    let ui = GraphsUi::default();
    let state = online();

    // The response never ran: "Not fired" and no caption.
    rig.feed(0.0);
    rig.ticks(1);
    let model = rig.model(&state, &ui);
    let row = &model.rows[0];
    assert_eq!(row.end, "Response");
    assert_eq!(row.end_caption.as_deref(), Some("Notification"));
    assert!(!row.end_mono);
    assert_eq!(row.result, "Not fired");
    assert!(!row.fired && row.last_run.is_none());

    // The input is true: the response end node is in the state "run".
    rig.feed(1.0);
    rig.ticks(2);
    let model = rig.model_at(&state, &ui, WALL_START_US + 2_000_000);
    let row = &model.rows[0];
    assert!(row.fired);
    assert_eq!(row.result, "Fired");
    assert_eq!(row.state_text, "Running");
    assert_eq!(
        row.last_run,
        Some(format!("Last run {}", clock(WALL_START_US)))
    );

    // The input is false again: "Not fired", with the last run. On another day the caption has
    // the date and no seconds.
    rig.feed(0.0);
    rig.ticks(1);
    let two_days = 2 * 24 * 3600 * 1_000_000;
    let model = rig.model_at(&state, &ui, WALL_START_US + two_days);
    let row = &model.rows[0];
    assert!(!row.fired);
    assert_eq!(row.result, "Not fired");
    assert_eq!(
        row.last_run,
        Some(format!("Last run {}", date_time(WALL_START_US)))
    );

    // The Response pause: an enabled Graph with a response end node is "Paused", and a Graph
    // with a sensor end node stays "Running".
    let dir = Dir::new("screen-response-pause");
    let mut store = GraphStore::open(dir.graph_paths());
    store.put(sensor_graph("graph_1")).unwrap();
    store.put(response_graph("graph_2")).unwrap();
    store.state_mut().pause.count = 3;
    store.state_mut().pause.paused = true;
    let mut paused = Rig::with_store(dir, store);
    paused.ticks(1);
    let model = paused.model(&state, &ui);
    assert_eq!(model.rows[0].state, LineState::Running);
    assert_eq!(model.rows[1].state, LineState::Paused);
    assert_eq!(model.rows[1].state_text, "Paused");
}

#[test]
fn delete_in_use_keeps_the_graph_and_names_the_curve_in_the_row() {
    let mut rig = Rig::new("screen-delete-in-use", vec![sensor_graph("graph_1")]);
    rig.ticks(1);
    *rig.link.in_use.lock().unwrap() =
        Some("graph_3 is read by the Curve of nct6798_pwm6 in profile_default".to_owned());
    // "Delete" asks first.
    let mut ui = GraphsUi {
        delete: Some("graph_1".to_owned()),
        ..Default::default()
    };
    let dialog = rig.model(&online(), &ui).dialog.expect("the question");
    assert_eq!(dialog.title, "Delete the Graph 'Render temperature'?");
    assert_eq!(dialog.text, "Its Graph sensor is removed too.");
    assert_eq!(
        (dialog.cancel.as_str(), dialog.confirm.as_str()),
        ("Cancel", "Delete Graph")
    );

    let command = ui.confirm_delete().expect("the delete command");
    assert_eq!(
        command,
        Command::Graph(GraphCommand::Delete {
            graph: "graph_1".to_owned()
        })
    );
    assert_eq!(ui.asked, Some(("graph_1".to_owned(), Asked::Delete)));
    rig.run(command);

    // The Graph stays, and the row names the Curve of the snapshot.
    assert!(rig.service.store().graph("graph_1").is_some());
    let mut state = full();
    for channel in state.snapshot.channels.values_mut() {
        if channel.name == "Waterpump" {
            channel.curve.input = Some("graph_3".to_owned());
        }
    }
    let model = rig.model(&state, &ui);
    assert!(model.dialog.is_none());
    let row = &model.rows[0];
    assert_eq!(
        row.caption,
        "Not deleted: the Curve of Waterpump uses its Graph sensor."
    );
    assert!(row.caption_warns);
    // Without the Curve in the snapshot the row has the reason of the daemon.
    let model = rig.model(&online(), &ui);
    assert_eq!(
        model.rows[0].caption,
        "Not deleted: graph_3 is read by the Curve of nct6798_pwm6 in profile_default"
    );

    // Nothing uses the sensor: the sensor goes first, and the Graph after it.
    *rig.link.in_use.lock().unwrap() = None;
    ui.delete = Some("graph_1".to_owned());
    let command = ui.confirm_delete().expect("the delete command");
    rig.run(command);
    assert!(rig.service.store().graph("graph_1").is_none());
    assert_eq!(*rig.link.deleted.lock().unwrap(), ["graph_3"]);
    let model = rig.model(&online(), &ui);
    assert!(model.rows.is_empty());
    assert_eq!(model.state, GraphsState::Empty);
}

#[test]
fn enable_switch_stores_the_graph() {
    let mut imported = sensor_graph("graph_1");
    imported.sensor_id = None;
    imported.enabled = false;
    let mut rig = Rig::new("screen-enable", vec![response_graph("graph_2"), imported]);
    rig.ticks(1);
    let mut ui = GraphsUi::default();
    let state = online();
    let stored = |rig: &Rig, id: &str| {
        GraphStore::open(rig.dir.graph_paths())
            .graph(id)
            .expect("the Graph is in graphs.json")
            .clone()
    };

    // Off: the file holds the Graph as disabled.
    let command = ui.toggle("graph_2", false);
    assert_eq!(ui.asked, None);
    rig.run(command);
    assert!(!stored(&rig, "graph_2").enabled);
    let model = rig.model(&state, &ui);
    let row = &model.rows[0];
    assert!(!row.checked && row.switch_enabled);
    assert_eq!(row.state, LineState::Disabled);
    assert_eq!(row.state_text, "Disabled");
    assert_eq!(row.result, "Not fired");
    assert!(row.dim);

    // On again.
    let command = ui.toggle("graph_2", true);
    assert_eq!(ui.asked, Some(("graph_2".to_owned(), Asked::Enable)));
    rig.run(command);
    assert!(stored(&rig, "graph_2").enabled);
    assert!(rig.model(&state, &ui).rows[0].checked);

    // A Graph with a sensor end node and no Graph sensor: the daemon refuses the sensor, the
    // switch stays off and the row has the reason.
    *rig.link.no_create.lock().unwrap() =
        Some("the limit of 64 Graph sensors is reached".to_owned());
    let command = ui.toggle("graph_1", true);
    rig.run(command);
    assert!(!stored(&rig, "graph_1").enabled);
    let model = rig.model(&state, &ui);
    let row = &model.rows[1];
    assert!(!row.checked);
    assert_eq!(
        row.caption,
        "Not turned on: the limit of 64 Graph sensors is reached"
    );
    assert!(row.caption_warns);

    // The daemon makes the sensor: the Graph is on and holds the id.
    *rig.link.no_create.lock().unwrap() = None;
    let command = ui.toggle("graph_1", true);
    rig.run(command);
    let graph = stored(&rig, "graph_1");
    assert!(graph.enabled);
    assert_eq!(graph.sensor_id.as_deref(), Some("graph_9"));
    assert_eq!(rig.model(&state, &ui).rows[1].caption, "2 nodes");

    // Offline, the switch of a Graph without a Graph sensor is off. A Graph with a response end
    // node keeps every control.
    let mut bare = sensor_graph("graph_1");
    bare.sensor_id = None;
    bare.enabled = false;
    let rig = Rig::new(
        "screen-enable-offline",
        vec![response_graph("graph_2"), bare],
    );
    let mut offline = online();
    offline.link = Link::Offline;
    let model = rig.model(&offline, &ui);
    assert!(model.rows[0].switch_enabled && model.rows[0].delete_enabled);
    assert!(!model.rows[1].switch_enabled && !model.rows[1].delete_enabled);
    assert_eq!(
        model.rows[1].switch_tooltip.as_deref(),
        Some("The daemon is offline.")
    );
}

#[test]
fn empty_state_text() {
    let rig = Rig::new("screen-empty", Vec::new());
    let mut ui = GraphsUi::default();
    let state = online();
    let model = rig.model(&state, &ui);
    assert_eq!(model.state, GraphsState::Empty);
    assert_eq!(
        model.empty_text,
        "No Graph. A Graph computes a sensor or runs a response from nodes."
    );
    assert!(model.rows.is_empty() && model.orphans.is_none());
    assert_eq!(
        (
            model.new_sensor.as_str(),
            model.new_response.as_str(),
            model.import.as_str()
        ),
        ("New sensor", "New response", "Import")
    );
    assert!(model.create_enabled && model.create_note.is_none());

    // Before the Graph service gave anything the screen is loading.
    let feedback = Feedback::default();
    let loading = GraphsModel::build(
        &ScreenInput {
            state: &state,
            now_us: WALL_START_US,
            locale: EN,
            down_since_us: None,
            feedback: &feedback,
        },
        None,
        &ui,
    );
    assert_eq!(loading.state, GraphsState::Loading);
    assert!(!loading.create_enabled);

    // "New sensor" opens the editor on a draft with one sensor end node. Nothing is stored.
    let shown = rig.view.lock().unwrap().clone();
    ui.new_draft(&shown, Kind::SensorEnd, EN);
    let draft = ui.editor.clone().expect("the draft");
    assert_eq!(draft.name, "Graph 1");
    assert_eq!(draft.nodes.len(), 1);
    assert_eq!(draft.nodes[0].kind, Kind::SensorEnd);
    assert!(rig.service.store().entries().is_empty());
    let model = rig.model(&state, &ui);
    assert_eq!(
        model.editor.map(|editor| editor.name),
        Some("Graph 1".to_owned())
    );
    ui.new_draft(&shown, Kind::ResponseEnd, EN);
    assert_eq!(ui.editor.as_ref().unwrap().nodes[0].kind, Kind::ResponseEnd);

    // At 64 Graphs the three buttons are off.
    let graphs = (1..=64)
        .map(|n| constant_graph(&format!("graph_{n}"), 1.0))
        .collect();
    let full_rig = Rig::new("screen-limit", graphs);
    let model = full_rig.model(&state, &GraphsUi::default());
    assert_eq!(model.rows.len(), 64);
    assert!(!model.create_enabled);
    assert_eq!(
        model.create_note.as_deref(),
        Some("The limit is 64 Graphs.")
    );
}

#[test]
fn invalid_graph_row() {
    let dir = Dir::new("screen-invalid");
    let paths = dir.graph_paths();
    std::fs::create_dir_all(paths.graphs.parent().unwrap()).unwrap();
    let odd = r#"{"id":"graph_2","name":"Radiator delta T","enabled":true,"sensor_id":"graph_2","nodes":[{"id":1,"kind":"quantum","label":null,"params":{},"x":0.0,"y":0.0},{"id":2,"kind":"sensor_end","label":null,"params":{"unit":"degC"},"x":0.0,"y":0.0}],"connections":[]}"#;
    let valid = serde_json::to_string(&response_graph("graph_1")).unwrap();
    std::fs::write(
        &paths.graphs,
        format!(r#"{{"schema_version":1,"graphs":[{valid},{odd}]}}"#),
    )
    .unwrap();
    let store = GraphStore::open(paths);
    let mut rig = Rig::with_store(dir, store);
    rig.ticks(1);
    let mut ui = GraphsUi::default();
    let model = rig.model(&online(), &ui);
    assert_eq!(model.rows.len(), 2);
    let row = &model.rows[1];
    assert_eq!(row.name, "Radiator delta T");
    assert_eq!(row.state, LineState::Invalid);
    assert_eq!(row.state_text, "Invalid");
    assert_eq!(row.caption, "Unknown node kind: quantum");
    assert!(row.caption_warns);
    assert_eq!(row.end, "Sensor");
    assert_eq!(row.end_caption.as_deref(), Some("graph_2"));
    assert_eq!(row.result, "--");
    assert!(row.dim);
    // The switch, "Edit" and "Export" are off. "Delete" works.
    assert!(!row.checked && !row.switch_enabled && !row.readable);
    assert_eq!(
        row.read_tooltip.as_deref(),
        Some("The app cannot read this Graph.")
    );
    assert!(row.delete_enabled);
    // The valid Graph next to it is as before.
    assert!(model.rows[0].readable && model.rows[0].read_tooltip.is_none());

    // "Edit" opens nothing for it.
    let shown = rig.view.lock().unwrap().clone();
    ui.edit(&shown, "graph_2");
    assert!(ui.editor.is_none());
    ui.edit(&shown, "graph_1");
    assert_eq!(ui.editor.as_ref().map(|g| g.id.as_str()), Some("graph_1"));
}

#[test]
fn orphan_sensor_row_has_a_remove_button() {
    let mut rig = Rig::new("screen-orphan", vec![response_graph("graph_2")]);
    let mut state = full();
    state.snapshot.sensors.insert(
        "graph_7".to_owned(),
        sensor(
            "graph_7",
            "Case air delta",
            "temperature",
            "degC",
            "graph",
            "",
        ),
    );
    rig.service
        .apply_event(&DaemonEvent::Snapshot(Box::new(state.snapshot.clone())));
    let mut ui = GraphsUi::default();

    let model = rig.model(&state, &ui);
    let card = model.orphans.expect("the second card");
    assert_eq!(card.title, "Graph sensors without a Graph");
    assert_eq!(
        card.text,
        "No Graph of this user holds these Graph sensors. A Graph sensor can belong to another user."
    );
    assert_eq!(card.remove, "Remove");
    assert_eq!(card.rows.len(), 1);
    let row = &card.rows[0];
    assert_eq!(
        (row.id.as_str(), row.name.as_str()),
        ("graph_7", "Case air delta")
    );
    assert_eq!(row.value, "--");
    assert!(row.remove_enabled && row.note.is_none());

    // "Remove" asks, then sends the delete of the Graph sensor to the daemon.
    ui.remove = Some("graph_7".to_owned());
    let dialog = rig.model(&state, &ui).dialog.expect("the question");
    assert_eq!(dialog.title, "Remove the Graph sensor 'Case air delta'?");
    assert_eq!(
        dialog.text,
        "No Graph of this user holds it. It can belong to another user."
    );
    assert_eq!(
        (dialog.cancel.as_str(), dialog.confirm.as_str()),
        ("Cancel", "Remove")
    );
    assert_eq!(
        ui.confirm_remove(),
        Some(Command::DeleteGraphSensor {
            sensor: "graph_7".to_owned()
        })
    );
    assert!(rig.model(&state, &ui).dialog.is_none());

    // A sensor in use is not removed, and the row names its user.
    for channel in state.snapshot.channels.values_mut() {
        if channel.name == "Waterpump" {
            channel.curve.input = Some("graph_7".to_owned());
        }
    }
    let mut feedback = Feedback::default();
    feedback.set(
        "graph-sensor:graph_7".to_owned(),
        WriteOutcome::Refused {
            error: "InUse".to_owned(),
        },
    );
    let model = rig.model_with(&state, &ui, WALL_START_US, &feedback);
    assert_eq!(
        model.orphans.unwrap().rows[0].note.as_deref(),
        Some("The Curve of Waterpump uses this sensor.")
    );

    // Offline, "Remove" is off.
    let mut offline = state.clone();
    offline.link = Link::Offline;
    let card = rig.model(&offline, &ui).orphans.expect("the second card");
    let row = &card.rows[0];
    assert!(!row.remove_enabled);
    assert_eq!(row.tooltip.as_deref(), Some("The daemon is offline."));
}

#[test]
fn export_writes_a_json_file() {
    let mut rig = Rig::new("screen-export", vec![sensor_graph("graph_1")]);
    rig.ticks(1);
    let mut ui = GraphsUi::default();
    let shown = rig.view.lock().unwrap().clone();

    // The file dialog proposes the name of the Graph.
    let (name, graph) = GraphsUi::export_target(&shown, "graph_1").expect("a readable Graph");
    assert_eq!(name, "Render temperature.json");
    let path = rig.dir.path().join(&name);
    ui.export_to(&path, &graph, EN);

    let text = std::fs::read_to_string(&path).expect("the export file");
    let file: serde_json::Value = serde_json::from_str(&text).expect("JSON");
    assert_eq!(file["format"].as_str(), Some("nicecooling-graph"));
    assert_eq!(file["format_version"].as_u64(), Some(1));
    assert_eq!(file["graph"]["name"].as_str(), Some("Render temperature"));
    assert_eq!(file["graph"]["nodes"].as_array().unwrap().len(), 2);
    assert_eq!(file["graph"]["connections"].as_array().unwrap().len(), 1);
    // No Graph sensor id, no Graph id and no enable flag.
    assert!(!text.contains("sensor_id") && !text.contains("graph_3"));
    assert!(file["graph"].get("id").is_none() && file["graph"].get("enabled").is_none());

    // The row above the table says where the file is.
    let notice = ui.notice.clone().expect("the row above the table");
    assert!(!notice.warning);
    assert!(
        notice.text.starts_with("Exported to ") && notice.text.ends_with("Render temperature.json"),
        "{}",
        notice.text
    );
    assert_eq!(rig.model(&online(), &ui).notice, Some(notice));
    assert_eq!(
        shown_path(
            Path::new("/home/u/Documents/Render temperature.json"),
            Some(Path::new("/home/u"))
        ),
        "~/Documents/Render temperature.json"
    );
    assert_eq!(
        shown_path(Path::new("/srv/x.json"), Some(Path::new("/home/u"))),
        "/srv/x.json"
    );

    // A file that cannot be written gives the reason.
    ui.export_to(&path.join("below-a-file.json"), &graph, EN);
    let notice = ui.notice.clone().expect("the reason");
    assert!(notice.warning);
    assert!(notice.text.starts_with("Not exported: "), "{}", notice.text);

    // A name with a path separator makes no directory.
    let mut odd = sensor_graph("graph_4");
    odd.name = "In/Out".to_owned();
    odd.sensor_id = None;
    odd.enabled = false;
    rig.service.handle(GraphCommand::Import(Box::new(odd)));
    let shown = rig.view.lock().unwrap().clone();
    let (name, _) = GraphsUi::export_target(&shown, "graph_4").expect("a readable Graph");
    assert_eq!(name, "In_Out.json");
    assert_eq!(GraphsUi::export_target(&shown, "graph_none"), None);
}

#[test]
fn import_adds_a_disabled_graph() {
    let mut rig = Rig::new("screen-import", vec![sensor_graph("graph_1")]);
    rig.ticks(1);
    let mut ui = GraphsUi::default();
    let state = online();
    let path = rig.dir.path().join("render.json");
    export_file(&path, &sensor_graph("graph_1")).unwrap();

    let shown = rig.view.lock().unwrap().clone();
    let command = ui
        .import_from(&path, &shown, EN)
        .expect("the Graph is accepted");
    assert!(ui.notice.is_none());
    rig.run(command);

    // The new Graph is the last row: a new id, disabled, with no Graph sensor.
    let model = rig.model(&state, &ui);
    assert_eq!(model.rows.len(), 2);
    let row = &model.rows[1];
    assert_ne!(row.id, "graph_1");
    assert_eq!(row.name, "Render temperature");
    assert_eq!(row.state, LineState::Disabled);
    assert!(!row.checked);
    assert_eq!(row.caption, "Imported. Check it, then turn it on.");
    assert_eq!(row.end_caption, None);
    let id = row.id.clone();
    let stored = rig.service.store().graph(&id).expect("stored").clone();
    assert!(!stored.enabled);
    assert_eq!(stored.sensor_id, None);
    assert!(
        rig.link.created.lock().unwrap().is_empty(),
        "the import makes no call to the daemon"
    );

    // The same file again gives a second Graph with another id.
    let shown = rig.view.lock().unwrap().clone();
    let command = ui
        .import_from(&path, &shown, EN)
        .expect("the Graph is accepted again");
    rig.run(command);
    let model = rig.model(&state, &ui);
    assert_eq!(model.rows.len(), 3);
    assert_ne!(model.rows[2].id, id);
    assert_ne!(model.rows[2].id, "graph_1");

    // The caption stays until the user turns the Graph on. The Graph sensor is made then.
    let command = ui.toggle(&id, true);
    rig.run(command);
    let model = rig.model(&state, &ui);
    assert!(model.rows[1].checked);
    assert_eq!(model.rows[1].caption, "2 nodes");
    assert_eq!(model.rows[1].end_caption.as_deref(), Some("graph_9"));
    assert_eq!(*rig.link.created.lock().unwrap(), ["Render temperature"]);
    assert_eq!(
        model.rows[2].caption,
        "Imported. Check it, then turn it on."
    );
}

#[test]
fn import_bad_file_shows_a_message() {
    let rig = Rig::new("screen-import-bad", vec![response_graph("graph_2")]);
    let mut ui = GraphsUi::default();
    let state = online();
    let shown = rig.view.lock().unwrap().clone();
    let refused = |ui: &mut GraphsUi, path: &Path| {
        assert_eq!(ui.import_from(path, &shown, EN), None);
        ui.notice.clone().expect("the reason above the table")
    };

    // A file that is no export file.
    let path = rig.dir.path().join("hello.json");
    std::fs::write(&path, "{\"hello\": 1}").unwrap();
    let notice = refused(&mut ui, &path);
    assert_eq!(
        notice,
        Notice {
            text: "Not imported: the file is not a nicecooling Graph.".to_owned(),
            warning: true,
        }
    );
    let model = rig.model(&state, &ui);
    assert_eq!(model.notice, Some(notice));
    assert_eq!(model.close, "Close");
    assert_eq!(model.rows.len(), 1);
    assert!(ui.imported.is_empty());

    // Text that is not JSON.
    std::fs::write(&path, "render temperature").unwrap();
    assert_eq!(
        refused(&mut ui, &path).text,
        "Not imported: the file is not a nicecooling Graph."
    );

    // A file over 1 MiB.
    std::fs::write(&path, vec![b' '; MAX_IMPORT_BYTES as usize + 1]).unwrap();
    assert_eq!(
        refused(&mut ui, &path).text,
        "Not imported: the file is larger than 1 MiB."
    );

    // A Graph that breaks a rule of the validator: the message names the first broken rule.
    let mut two_ends = sensor_graph("graph_1");
    two_ends.nodes.push(Node::new(3, Kind::ResponseEnd));
    std::fs::write(&path, export_text(&two_ends)).unwrap();
    let notice = refused(&mut ui, &path);
    assert!(
        notice.text.starts_with("Not imported: two-end-nodes"),
        "{}",
        notice.text
    );

    // Two nodes with the id 1: the message names `bad-parameter` for `id`.
    let twice = r#"{"format":"nicecooling-graph","format_version":1,"graph":{"name":"Dup","nodes":[
        {"id":1,"kind":"constant","label":null,"params":{"value":5},"x":0.0,"y":0.0},
        {"id":1,"kind":"constant","label":null,"params":{"value":9},"x":0.0,"y":0.0},
        {"id":2,"kind":"sensor_end","label":null,"params":{"unit":"degC"},"x":0.0,"y":0.0}],
        "connections":[{"from":{"node":1,"pin":"out"},"to":{"node":2,"pin":"a"}}]}}"#;
    std::fs::write(&path, twice).unwrap();
    let notice = refused(&mut ui, &path);
    assert!(
        notice.text.starts_with("Not imported: ") && notice.text.contains("bad-parameter id"),
        "{}",
        notice.text
    );

    // A file that is not there.
    let notice = refused(&mut ui, &rig.dir.path().join("none.json"));
    assert!(notice.warning && notice.text.starts_with("Not imported: "));

    // Nothing was stored. The close button takes the row away.
    assert_eq!(rig.service.store().entries().len(), 1);
    ui.notice = None;
    assert!(rig.model(&state, &ui).notice.is_none());
}

#[test]
fn import_higher_version_shows_a_message() {
    let mut rig = Rig::new("screen-import-version", Vec::new());
    let mut ui = GraphsUi::default();
    let shown = rig.view.lock().unwrap().clone();
    let path = rig.dir.path().join("newer.json");
    let text = export_text(&sensor_graph("graph_1"))
        .replace("\"format_version\": 1", "\"format_version\": 2");
    assert!(text.contains("\"format_version\": 2"));
    std::fs::write(&path, &text).unwrap();

    assert_eq!(ui.import_from(&path, &shown, EN), None);
    assert_eq!(
        ui.notice,
        Some(Notice {
            text: "Not imported: the file has format version 2. This app reads version 1."
                .to_owned(),
            warning: true,
        })
    );
    assert!(rig.service.store().entries().is_empty());
    assert_eq!(rig.model(&online(), &ui).state, GraphsState::Empty);

    // The same Graph with the format version 1 is accepted, and the message goes.
    std::fs::write(&path, export_text(&sensor_graph("graph_1"))).unwrap();
    let command = ui
        .import_from(&path, &shown, EN)
        .expect("version 1 is accepted");
    assert!(ui.notice.is_none());
    rig.run(command);
    assert_eq!(rig.service.store().entries().len(), 1);
}
