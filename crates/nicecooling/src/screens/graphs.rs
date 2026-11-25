//! The Graphs screen (`docs/ui/screens/graphs.md`): the Graphs of the user with their state and
//! live result, and the Graph sensors that no Graph of this user holds.

use nicecooling_graph::eval::EndValue;
use nicecooling_graph::model::{Graph, Kind, Node, ParseProblem, ResponseKind, StoredGraph};
use nicecooling_graph::response::RunState;
use nicecooling_graph::validate::MAX_GRAPHS;

use crate::command::Command;
use crate::graphs::channel::GraphCommand;
use crate::graphs::transfer::new_graph_id;
use crate::graphs::view::{GraphRow, GraphView, RowState, ValuesState};
use crate::i18n::Locale;
use crate::screens::format::{clock, date_time, number, optional_value, unit_text};
use crate::screens::sensors::{SensorUse, sensor_users};
use crate::screens::{ScreenInput, WriteOutcome};

/// What the user asked of a Graph with the last command that the Graph service can refuse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    Enable,
    Delete,
}

/// The state of the screen that the user changes.
#[derive(Clone, Debug, Default)]
pub struct GraphsUi {
    /// The Graph that the Graph editor is open on: a stored Graph, or a draft that is not stored.
    pub editor: Option<Graph>,
    /// The Graph that "Delete" asks about.
    pub delete: Option<String>,
    /// The Graph sensor that "Remove" asks about.
    pub remove: Option<String>,
    /// The last command that the service can refuse, with its Graph. The refusal of the view
    /// shows in the row of this Graph.
    pub asked: Option<(String, Asked)>,
}

impl GraphsUi {
    /// "New sensor" and "New response": open the editor on a draft with one end node of the kind
    /// `end`. The draft has the lowest free number as its name. Nothing is stored.
    pub fn new_draft(&mut self, view: &GraphView, end: Kind, locale: Locale) {
        let name_of = |n: u32| locale.fmt("graphs.new_name", &[("n", &n.to_string())]);
        let free = (1u32..)
            .find(|n| {
                let name = name_of(*n);
                !view.rows.iter().any(|row| row.name == name)
            })
            .unwrap_or(1);
        let mut graph = Graph::new(&new_graph_id(&view.graphs()), &name_of(free));
        graph.nodes = vec![Node::new(1, end)];
        self.editor = Some(graph);
    }

    /// "Edit": open the editor on a stored Graph. A Graph that the app cannot read opens nothing.
    pub fn edit(&mut self, view: &GraphView, graph: &str) {
        if let Some(stored) = view.row(graph).and_then(GraphRow::graph) {
            self.editor = Some(stored.clone());
        }
    }

    /// Open the editor on the Graph that holds the Graph sensor `sensor`. Returns whether a Graph
    /// of this user holds it.
    pub fn edit_sensor(&mut self, view: &GraphView, sensor: &str) -> bool {
        let holder = view
            .rows
            .iter()
            .filter_map(GraphRow::graph)
            .find(|graph| graph.sensor_id.as_deref() == Some(sensor));
        match holder {
            Some(graph) => {
                self.editor = Some(graph.clone());
                true
            }
            None => false,
        }
    }

    /// The switch of a row: the command that enables or disables the Graph at once.
    pub fn toggle(&mut self, graph: &str, enabled: bool) -> Command {
        self.asked = enabled.then(|| (graph.to_owned(), Asked::Enable));
        Command::Graph(GraphCommand::Enable {
            graph: graph.to_owned(),
            enabled,
        })
    }

    /// "Delete Graph" in the dialog.
    pub fn confirm_delete(&mut self) -> Option<Command> {
        let graph = self.delete.take()?;
        self.asked = Some((graph.clone(), Asked::Delete));
        Some(Command::Graph(GraphCommand::Delete { graph }))
    }

    /// "Remove" in the dialog.
    pub fn confirm_remove(&mut self) -> Option<Command> {
        let sensor = self.remove.take()?;
        Some(Command::DeleteGraphSensor { sensor })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphsState {
    /// The Graph service gave nothing yet.
    Loading,
    /// `graphs.json` cannot be used.
    FileError,
    Empty,
    Ready,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineState {
    Running,
    Disabled,
    Invalid,
    /// The Response pause is on, and the Graph has a response end node.
    Paused,
}

/// One row of the card "Graphs".
#[derive(Clone, Debug, PartialEq)]
pub struct GraphLine {
    pub id: String,
    pub name: String,
    /// Under the name: the count of the nodes, the reason of an invalid Graph, or the refusal of
    /// the last command.
    pub caption: String,
    /// The caption is a reason or a refusal: it has the Warning color.
    pub caption_warns: bool,
    /// "Sensor" or "Response".
    pub end: String,
    /// The id of the Graph sensor, or the kind of the response.
    pub end_caption: Option<String>,
    /// The caption of the end node is an id: it shows in mono text.
    pub end_mono: bool,
    pub state: LineState,
    pub state_text: String,
    /// The whole result: "46.2 °C", "--", "Fired" or "Not fired".
    pub result: String,
    /// The number and the unit of a sensor result.
    pub value: Option<(String, String)>,
    pub fired: bool,
    /// "Last run 14:05:09".
    pub last_run: Option<String>,
    /// The result shows in `fg-dim`.
    pub dim: bool,
    pub checked: bool,
    pub switch_enabled: bool,
    pub switch_tooltip: Option<String>,
    /// "Edit" and "Export" work.
    pub readable: bool,
    pub read_tooltip: Option<String>,
    pub delete_enabled: bool,
    pub delete_tooltip: Option<String>,
}

/// One Graph sensor that no Graph of this user holds.
#[derive(Clone, Debug, PartialEq)]
pub struct OrphanLine {
    pub id: String,
    pub name: String,
    pub value: String,
    /// Why "Remove" did not remove the sensor.
    pub note: Option<String>,
    pub remove_enabled: bool,
    pub tooltip: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrphanCard {
    pub title: String,
    pub text: String,
    pub remove: String,
    pub rows: Vec<OrphanLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConfirmDialog {
    pub title: String,
    pub text: String,
    pub cancel: String,
    pub confirm: String,
}

/// The Graph that the Graph editor is open on.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorTarget {
    /// The button that goes back to the list.
    pub back: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GraphsModel {
    pub state: GraphsState,
    pub new_sensor: String,
    pub new_response: String,
    pub import: String,
    /// False at the limit of 64 Graphs and while `graphs.json` cannot be used.
    pub create_enabled: bool,
    /// "The limit is 64 Graphs."
    pub create_note: Option<String>,
    pub title: String,
    /// "Graph", "End node", "State", "Result".
    pub columns: [String; 4],
    pub edit: String,
    pub export: String,
    pub delete: String,
    pub empty_text: String,
    /// "nicecooling cannot read graphs.json." and the error text.
    pub file_error: Option<(String, String)>,
    /// The warning rows above the table.
    pub warnings: Vec<String>,
    pub rows: Vec<GraphLine>,
    pub orphans: Option<OrphanCard>,
    pub dialog: Option<ConfirmDialog>,
    pub editor: Option<EditorTarget>,
}

impl GraphsModel {
    /// `view` is what the Graph service shows now. `None` before the service gave anything.
    pub fn build(input: &ScreenInput<'_>, view: Option<&GraphView>, ui: &GraphsUi) -> Self {
        let locale = input.locale;
        let rows: Vec<GraphLine> = view
            .map(|view| {
                view.rows
                    .iter()
                    .map(|row| line_of(input, view, ui, row))
                    .collect()
            })
            .unwrap_or_default();
        let file_error = view.and_then(|view| view.file_error.clone());
        let state = match view {
            None => GraphsState::Loading,
            Some(_) if file_error.is_some() => GraphsState::FileError,
            Some(view) if view.rows.is_empty() => GraphsState::Empty,
            Some(_) => GraphsState::Ready,
        };
        let at_limit = rows.len() >= MAX_GRAPHS;
        let mut warnings = Vec::new();
        if let Some(view) = view {
            if view.values == ValuesState::NoAccess {
                warnings.push(locale.tr("graphs.no_access"));
            }
            if let Some(error) = view.error.as_ref().filter(|_| file_error.is_none()) {
                warnings.push(error.clone());
            }
        }
        Self {
            state,
            new_sensor: locale.tr("graphs.new_sensor"),
            new_response: locale.tr("graphs.new_response"),
            import: locale.tr("graphs.import"),
            create_enabled: view.is_some() && file_error.is_none() && !at_limit,
            create_note: at_limit.then(|| locale.tr("graphs.limit")),
            title: locale.tr("nav.graphs"),
            columns: [
                locale.tr("graphs.columns.graph"),
                locale.tr("graphs.columns.end"),
                locale.tr("graphs.columns.state"),
                locale.tr("graphs.columns.result"),
            ],
            edit: locale.tr("graphs.edit"),
            export: locale.tr("graphs.export"),
            delete: locale.tr("graphs.delete"),
            empty_text: locale.tr("graphs.empty"),
            file_error: file_error.map(|error| (locale.tr("graphs.file_error"), error)),
            warnings,
            rows,
            orphans: view.and_then(|view| orphans_of(input, view)),
            dialog: view.and_then(|view| dialog_of(input, view, ui)),
            editor: ui.editor.as_ref().map(|graph| EditorTarget {
                back: locale.tr("nav.graphs"),
                name: graph.name.clone(),
            }),
        }
    }
}

/// The end node of a row.
enum End {
    Sensor {
        id: Option<String>,
        unit: String,
    },
    Response(ResponseKind),
    /// The stored text does not say which.
    Unknown,
}

fn end_of(row: &GraphRow) -> End {
    match &row.stored {
        StoredGraph::Valid(graph) => match (graph.sensor_end(), graph.response_end()) {
            (Some(node), _) => End::Sensor {
                id: graph.sensor_id.clone(),
                unit: node.text("unit"),
            },
            (None, Some(node)) => End::Response(node.response_kind()),
            (None, None) => End::Unknown,
        },
        StoredGraph::Invalid(invalid) => stored_end(invalid.text.get()),
    }
}

/// The end node that the text of a Graph names, for a Graph that this version cannot run.
fn stored_end(text: &str) -> End {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return End::Unknown;
    };
    fn kind_of(node: &serde_json::Value) -> Option<&str> {
        node.get("kind").and_then(serde_json::Value::as_str)
    }
    let nodes = value.get("nodes").and_then(serde_json::Value::as_array);
    let end = nodes.into_iter().flatten().find(|node| {
        kind_of(node).is_some_and(|kind| {
            kind == Kind::SensorEnd.as_str() || kind == Kind::ResponseEnd.as_str()
        })
    });
    let Some(end) = end else {
        return End::Unknown;
    };
    let param = |name: &str| {
        end.get("params")
            .and_then(|params| params.get(name))
            .and_then(serde_json::Value::as_str)
    };
    if kind_of(end) == Some(Kind::SensorEnd.as_str()) {
        End::Sensor {
            id: value
                .get("sensor_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned),
            unit: param("unit").unwrap_or_default().to_owned(),
        }
    } else {
        End::Response(
            param("kind")
                .and_then(ResponseKind::parse)
                .unwrap_or(ResponseKind::Notification),
        )
    }
}

/// The time of a run: "14:05:09" for a run of today, "2026-11-24 21:40" for another day.
fn run_time(us: u64, now_us: u64) -> String {
    let (day, today) = (date_time(us), date_time(now_us));
    if day.get(..10) == today.get(..10) {
        clock(us)
    } else {
        day
    }
}

/// The refusal of the view without the name of the Graph before it.
fn reason_of<'a>(refusal: &'a str, name: &str) -> &'a str {
    refusal
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix(": "))
        .unwrap_or(refusal)
}

fn line_of(input: &ScreenInput<'_>, view: &GraphView, ui: &GraphsUi, row: &GraphRow) -> GraphLine {
    let locale = input.locale;
    let online = input.state.is_online();
    let offline = (!online).then(|| locale.tr("tooltip.daemon_offline"));
    let end = end_of(row);
    let state = match &row.state {
        RowState::Invalid(_) => LineState::Invalid,
        RowState::Disabled => LineState::Disabled,
        RowState::Running if view.pause.paused && matches!(end, End::Response(_)) => {
            LineState::Paused
        }
        RowState::Running => LineState::Running,
    };
    let runs = matches!(state, LineState::Running | LineState::Paused);

    let (end_text, end_caption, end_mono) = match &end {
        End::Sensor { id, .. } => (locale.tr("graphs.end.sensor"), id.clone(), true),
        End::Response(kind) => (
            locale.tr("graphs.end.response"),
            Some(locale.tr(&format!("graphs.kind.{}", kind.as_str()))),
            false,
        ),
        End::Unknown => ("--".to_owned(), None, false),
    };

    let (result, value, fired, last_run, dim) = match &end {
        End::Sensor { unit, .. } => {
            let shown = match &row.end {
                EndValue::Sensor(value) if online && runs => *value,
                _ => None,
            };
            (
                optional_value(locale, shown, unit),
                shown.map(|value| (number(locale, value, unit), unit_text(unit).to_owned())),
                false,
                None,
                !(online && runs),
            )
        }
        End::Response(_) => {
            let fired = runs && row.run.is_some_and(|run| run.state == RunState::Run);
            let last_run = row
                .run
                .and_then(|run| run.last_run_us)
                .map(|us| locale.fmt("graphs.last_run", &[("time", &run_time(us, input.now_us))]));
            (
                locale.tr(if fired {
                    "graphs.fired"
                } else {
                    "graphs.not_fired"
                }),
                None,
                fired,
                last_run,
                !runs,
            )
        }
        End::Unknown => ("--".to_owned(), None, false, None, true),
    };

    let refusal = match (&ui.asked, &view.refusal) {
        (Some((graph, asked)), Some(refusal)) if *graph == row.id => {
            Some((*asked, reason_of(refusal, &row.name)))
        }
        _ => None,
    };
    let (caption, caption_warns) = match (refusal, &row.stored) {
        (Some((Asked::Delete, reason)), stored) => {
            // The snapshot gives the name of the Channel. The reason of the daemon has its id.
            let curve = match (stored, &end) {
                (StoredGraph::Valid(_), End::Sensor { id: Some(id), .. }) => {
                    sensor_users(input.state, id)
                        .into_iter()
                        .find_map(|user| match user {
                            SensorUse::Curve { name, .. } => Some(name),
                            _ => None,
                        })
                }
                _ => None,
            };
            let text = match curve {
                Some(name) => locale.fmt("graphs.not_deleted.curve", &[("name", &name)]),
                None => locale.fmt("graphs.not_deleted.other", &[("reason", reason)]),
            };
            (text, true)
        }
        (Some((Asked::Enable, reason)), _) => (
            locale.fmt("graphs.not_turned_on", &[("reason", reason)]),
            true,
        ),
        (None, StoredGraph::Invalid(invalid)) => (
            match &invalid.problem {
                ParseProblem::UnknownKind(kind) => {
                    locale.fmt("graphs.invalid.unknown_kind", &[("kind", kind)])
                }
                ParseProblem::Malformed(why) => why.clone(),
            },
            true,
        ),
        (None, StoredGraph::Valid(graph)) => {
            (locale.plural("graphs.nodes", graph.nodes.len()), false)
        }
    };

    let readable = row.graph().is_some();
    let unreadable = (!readable).then(|| locale.tr("graphs.unreadable"));
    let is_sensor = matches!(end, End::Sensor { .. });
    // The enable of a Graph with a sensor end node and no Graph sensor makes the sensor first.
    let needs_daemon = matches!(end, End::Sensor { id: None, .. }) && !online;
    GraphLine {
        id: row.id.clone(),
        name: row.name.clone(),
        caption,
        caption_warns,
        end: end_text,
        end_caption,
        end_mono,
        state,
        state_text: locale.tr(match state {
            LineState::Running => "graphs.state.running",
            LineState::Disabled => "graphs.state.disabled",
            LineState::Invalid => "graphs.state.invalid",
            LineState::Paused => "graphs.state.paused",
        }),
        result,
        value,
        fired,
        last_run,
        dim,
        checked: row.enabled,
        switch_enabled: readable && !needs_daemon,
        switch_tooltip: unreadable
            .clone()
            .or_else(|| offline.clone().filter(|_| needs_daemon)),
        readable,
        read_tooltip: unreadable,
        delete_enabled: online || !is_sensor,
        delete_tooltip: offline.filter(|_| is_sensor),
    }
}

fn orphans_of(input: &ScreenInput<'_>, view: &GraphView) -> Option<OrphanCard> {
    if view.foreign_sensors.is_empty() {
        return None;
    }
    let locale = input.locale;
    let state = input.state;
    let online = state.is_online();
    let rows = view
        .foreign_sensors
        .iter()
        .map(|sensor| {
            let note = input
                .feedback
                .get(&format!("graph-sensor:{}", sensor.id))
                .and_then(|outcome| match outcome {
                    WriteOutcome::Refused { error } if error == "InUse" => {
                        let curve = sensor_users(state, &sensor.id)
                            .into_iter()
                            .find_map(|user| match user {
                                SensorUse::Curve { name, .. } => Some(name),
                                _ => None,
                            });
                        Some(match curve {
                            Some(name) => {
                                locale.fmt("sensors.delete_refused.curve", &[("name", &name)])
                            }
                            None => locale.tr("graphs.orphans.in_use"),
                        })
                    }
                    other if other.is_failure() => {
                        Some(other.text(locale, &locale.tr("graphs.orphans.refused")))
                    }
                    _ => None,
                });
            OrphanLine {
                id: sensor.id.clone(),
                name: state
                    .snapshot
                    .sensors
                    .get(&sensor.id)
                    .map_or_else(|| sensor.name.clone(), |known| known.name.clone()),
                value: optional_value(locale, state.sensor_value(&sensor.id), &sensor.unit),
                note,
                remove_enabled: online,
                tooltip: (!online).then(|| locale.tr("tooltip.daemon_offline")),
            }
        })
        .collect();
    Some(OrphanCard {
        title: locale.tr("graphs.orphans.title"),
        text: locale.tr("graphs.orphans.text"),
        remove: locale.tr("graphs.orphans.remove"),
        rows,
    })
}

fn dialog_of(input: &ScreenInput<'_>, view: &GraphView, ui: &GraphsUi) -> Option<ConfirmDialog> {
    let locale = input.locale;
    if let Some(row) = ui.delete.as_ref().and_then(|graph| view.row(graph)) {
        let sensor = matches!(end_of(row), End::Sensor { .. });
        return Some(ConfirmDialog {
            title: locale.fmt("graphs.delete_dialog.title", &[("name", &row.name)]),
            text: if sensor {
                locale.tr("graphs.delete_dialog.sensor")
            } else {
                String::new()
            },
            cancel: locale.tr("graphs.cancel"),
            confirm: locale.tr("graphs.delete_dialog.confirm"),
        });
    }
    let sensor = ui.remove.as_ref()?;
    let orphan = view.foreign_sensors.iter().find(|s| s.id == *sensor)?;
    let name = input
        .state
        .snapshot
        .sensors
        .get(sensor)
        .map_or_else(|| orphan.name.clone(), |known| known.name.clone());
    Some(ConfirmDialog {
        title: locale.fmt("graphs.remove_dialog.title", &[("name", &name)]),
        text: locale.tr("graphs.remove_dialog.text"),
        cancel: locale.tr("graphs.cancel"),
        confirm: locale.tr("graphs.orphans.remove"),
    })
}
