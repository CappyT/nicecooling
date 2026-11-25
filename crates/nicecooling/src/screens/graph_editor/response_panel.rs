//! The response panel: the part of the parameter panel that only a response end node has. It
//! holds the warning of the kind, "Run once" and the newest rows of the response log of the
//! Graph.
//!
//! The app warns and never refuses a choice of the user: a warning turns no button off.

use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;

use nicecooling_graph::model::{Kind, Node, ResponseKind};

use super::EditorUi;
use crate::command::Command;
use crate::graphs::channel::GraphCommand;
use crate::graphs::responses::Outcome;
use crate::graphs::view::GraphView;
use crate::i18n::Locale;
use crate::model::AppState;
use crate::screens::ScreenInput;
use crate::screens::format::date_time_seconds;

/// The rows of the response log that the panel shows.
pub const SHOWN_RUNS: usize = 20;

/// The parameters of every response kind: the panel shows them after the parameters of the
/// kind.
pub const TIMING: [&str; 3] = ["hold_s", "repeat_s", "min_interval_s"];

/// What the panel needs of the network settings of the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Network {
    /// Settings has an SMTP host.
    pub smtp: bool,
    /// Settings has an MQTT broker host.
    pub mqtt: bool,
    /// The base topic of the MQTT messages.
    pub base_topic: String,
}

impl Default for Network {
    fn default() -> Self {
        Self {
            smtp: false,
            mqtt: false,
            base_topic: "nicecooling".to_owned(),
        }
    }
}

/// The warning of a response kind.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Warning {
    pub text: String,
    /// "Open Settings", for an account that Settings does not have.
    pub link: Option<String>,
}

/// One row of "Last runs".
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunRow {
    /// The local date and time of the run.
    pub time: String,
    /// The outcome, after the kind for a row of another kind than the kind of the draft:
    /// "OK", "E-mail: OK".
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResponsePanel {
    pub warning: Option<Warning>,
    /// The base topic with "/", before the field "Topic" of an MQTT message.
    pub topic_prefix: Option<String>,
    /// "Placeholders: {graph}, {time}, {value:label}, {unit:label}", for a kind with a text.
    pub placeholders: Option<String>,
    pub run_once: String,
    /// "Runs now with the parameters of the draft." A poweroff and a reboot add that the run
    /// counts for the Response pause.
    pub run_once_caption: String,
    /// Why the last "Run once" ran nothing.
    pub run_once_blocked: Option<String>,
    /// "Last runs".
    pub runs_title: String,
    /// The newest rows of the response log of this Graph, the newest first: 20 at most.
    pub runs: Vec<RunRow>,
    /// "No run yet."
    pub no_run: Option<String>,
}

/// Whether a path is a file that the user can execute.
fn is_executable(path: &str) -> bool {
    std::fs::metadata(Path::new(path))
        .is_ok_and(|file| file.is_file() && file.permissions().mode() & 0o111 != 0)
}

/// The warning of the response kind of a node, when one applies.
fn warning_of(node: &Node, network: &Network, locale: Locale) -> Option<Warning> {
    let plain = |key: &str| Warning {
        text: locale.tr(key),
        link: None,
    };
    let account = |key: &str| Warning {
        text: locale.tr(key),
        link: Some(locale.tr("graph_editor.response.open_settings")),
    };
    match node.response_kind() {
        ResponseKind::Poweroff | ResponseKind::Reboot | ResponseKind::Suspend => {
            Some(plain("graph_editor.response.warning.power"))
        }
        ResponseKind::Command if !is_executable(&node.text("path")) => {
            Some(plain("graph_editor.response.warning.command"))
        }
        ResponseKind::Email if !network.smtp => Some(account("graph_editor.response.warning.smtp")),
        ResponseKind::Mqtt if !network.mqtt => Some(account("graph_editor.response.warning.mqtt")),
        _ => None,
    }
}

/// The text of an outcome of the response log.
fn outcome_text(outcome: Outcome, locale: Locale) -> String {
    locale.tr(match outcome {
        Outcome::Ok => "graph_editor.response.outcome.ok",
        Outcome::Failed => "graph_editor.response.outcome.failed",
        Outcome::Timeout => "graph_editor.response.outcome.timeout",
        Outcome::NotConfigured => "graph_editor.response.outcome.not_configured",
        Outcome::Busy => "graph_editor.response.outcome.busy",
        Outcome::Cancelled => "graph_editor.response.outcome.cancelled",
    })
}

/// The response panel of a node. `None` for a node that is no response end node.
pub fn response_panel(
    input: &ScreenInput<'_>,
    view: &GraphView,
    ui: &EditorUi,
    node: &Node,
) -> Option<ResponsePanel> {
    if node.kind != Kind::ResponseEnd {
        return None;
    }
    let locale = input.locale;
    let kind = node.response_kind();
    let runs: Vec<RunRow> = view
        .log_rows_of(&ui.draft.graph().id)
        .into_iter()
        .rev()
        .take(SHOWN_RUNS)
        .map(|row| {
            let outcome = outcome_text(row.outcome, locale);
            RunRow {
                time: date_time_seconds(row.time_us),
                text: if row.kind == kind.as_str() {
                    outcome
                } else {
                    locale.fmt(
                        "graph_editor.response.other_kind",
                        &[
                            ("kind", &locale.tr(&format!("graphs.kind.{}", row.kind))),
                            ("outcome", &outcome),
                        ],
                    )
                },
            }
        })
        .collect();
    let mut caption = locale.tr("graph_editor.response.run_once_caption");
    if matches!(kind, ResponseKind::Poweroff | ResponseKind::Reboot) {
        caption.push(' ');
        caption.push_str(&locale.tr("graph_editor.response.run_once_counts"));
    }
    let has_text = matches!(
        kind,
        ResponseKind::Notification | ResponseKind::Email | ResponseKind::Mqtt
    );
    Some(ResponsePanel {
        warning: warning_of(node, &ui.network, locale),
        topic_prefix: (kind == ResponseKind::Mqtt).then(|| format!("{}/", ui.network.base_topic)),
        placeholders: has_text.then(|| locale.tr("graph_editor.response.placeholders")),
        run_once: locale.tr("graph_editor.response.run_once"),
        run_once_caption: caption,
        run_once_blocked: ui
            .run_blocked
            .then(|| locale.tr("graph_editor.response.run_once_blocked")),
        runs_title: locale.tr("graph_editor.response.last_runs"),
        no_run: runs
            .is_empty()
            .then(|| locale.tr("graph_editor.response.no_run")),
        runs,
    })
}

impl EditorUi {
    /// "Run once": the command that runs the response one time at once, with the parameters of
    /// the draft. It ignores the state of the response, its minimum interval and the Response
    /// pause. While the daemon reports the poweroff countdown of an Alarm, a power response
    /// runs nothing: the function returns `None`, and the panel shows the reason.
    pub fn run_once(&mut self, state: &AppState) -> Option<Command> {
        let kind = self.draft.graph().response_end()?.response_kind();
        self.run_blocked = kind.is_power() && state.is_online() && state.next_poweroff().is_some();
        if self.run_blocked {
            return None;
        }
        Some(Command::Graph(GraphCommand::RunOnce {
            graph: Box::new(self.draft.candidate()),
        }))
    }
}
