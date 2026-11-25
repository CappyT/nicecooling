//! "Apply" and leaving the editor.
//!
//! "Apply" sends the draft to the Graph service. The service makes or renames the Graph sensor
//! of a Graph with a sensor end node first, and that call can wait for the daemon. The editor
//! never waits in the window: it shows "Applying" with its buttons off until the service
//! answers. A refusal keeps the draft, and the toolbar shows the reason.
//!
//! Leaving the editor with a draft that is not applied asks: "Cancel", "Discard" or "Apply".

use nicecooling_graph::model::Graph;
use nicecooling_graph::validate::validate;

use super::EditorUi;
use super::rules::first_rule_text;
use crate::command::Command;
use crate::graphs::channel::GraphCommand;
use crate::graphs::values::SensorError;
use crate::graphs::view::{ApplyError, GraphRow, GraphView};
use crate::i18n::Locale;
use crate::screens::ScreenInput;
use crate::screens::sensors::{SensorUse, sensor_users};
use crate::shell::Page;

/// What the editor shows of its last "Apply".
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ApplyState {
    /// Nothing to show.
    #[default]
    Idle,
    /// The editor sent the draft and waits for the answer of the Graph service.
    Pending,
    /// The service stored the draft at this wall clock time, in microseconds since the Unix
    /// epoch.
    Applied { at_us: u64 },
    /// The service did not store the draft.
    Refused(ApplyError),
}

/// Where the user goes when the editor closes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Leave {
    /// The back button: the list of the Graphs screen.
    List,
    /// An entry of the sidebar, or a link to another page.
    Page(Page),
    /// "Quit".
    Quit,
}

/// The state of "Apply" and of the question on leaving.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ApplyUi {
    pub state: ApplyState,
    /// The question on leaving is open, with the place that the user wanted to go to.
    pub leave: Option<Leave>,
    /// The user chose "Apply" in the question: the editor closes when the service stored the
    /// draft.
    leaves_after_apply: bool,
}

/// The question on leaving with a draft.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LeaveDialog {
    /// "The Graph 'Render temperature' has changes that are not applied."
    pub title: String,
    pub cancel: String,
    pub discard: String,
    pub apply: String,
    /// "Apply" works: the draft breaks no rule, and the daemon is there when the Graph needs it.
    pub apply_enabled: bool,
    /// The buttons are off while the service has not answered the Apply.
    pub pending: bool,
}

/// Why "Apply" is off for a draft that changed and breaks no rule: the daemon is offline, and the
/// Graph has a sensor end node. `None` when the daemon is not in the way.
pub fn offline_reason(input: &ScreenInput<'_>, ui: &EditorUi) -> Option<String> {
    let needs_daemon = ui.draft.graph().sensor_end().is_some();
    (needs_daemon && !input.state.is_online()).then(|| input.locale.tr("graph_editor.offline"))
}

/// A reason of the daemon or of the app without its last full stop: the text of the toolbar adds
/// one.
fn reason_text(reason: &str, locale: Locale) -> String {
    locale.fmt(
        "graph_editor.not_applied_reason",
        &[("reason", reason.trim().trim_end_matches('.'))],
    )
}

/// The text of a refused Apply for the toolbar: "Not applied: the limit of 64 Graph sensors is
/// reached."
pub fn refusal_text(
    error: &ApplyError,
    input: &ScreenInput<'_>,
    view: &GraphView,
    ui: &EditorUi,
) -> String {
    let locale = input.locale;
    match error {
        ApplyError::Rules(refusals) => {
            let others = view.graphs();
            let rule = first_rule_text(refusals, &ui.draft.candidate(), &others, locale);
            reason_text(&rule.unwrap_or_default(), locale)
        }
        ApplyError::Sensor(SensorError::InUse(reason)) => {
            // The snapshot gives the name of the Channel. The reason of the daemon has its id.
            let curve = ui
                .draft
                .stored()
                .and_then(|stored| stored.sensor_id.as_deref())
                .map(|sensor| sensor_users(input.state, sensor))
                .unwrap_or_default()
                .into_iter()
                .find_map(|user| match user {
                    SensorUse::Curve { name, .. } => Some(name),
                    _ => None,
                });
            match curve {
                Some(name) => locale.fmt("graph_editor.not_applied_curve", &[("name", &name)]),
                None => reason_text(reason, locale),
            }
        }
        ApplyError::Sensor(error) => reason_text(&error.to_string(), locale),
        ApplyError::Other(reason) => reason_text(reason, locale),
    }
}

/// The question on leaving, while it is open.
pub fn leave_dialog(
    input: &ScreenInput<'_>,
    view: &GraphView,
    ui: &EditorUi,
) -> Option<LeaveDialog> {
    ui.apply.leave.as_ref()?;
    let locale = input.locale;
    Some(LeaveDialog {
        title: locale.fmt(
            "graph_editor.leave.title",
            &[("name", &ui.draft.graph().name)],
        ),
        cancel: locale.tr("graphs.cancel"),
        discard: locale.tr("graph_editor.leave.discard"),
        apply: locale.tr("graph_editor.apply"),
        apply_enabled: ui.can_apply(view, input.state.is_online()),
        pending: ui.apply.state == ApplyState::Pending,
    })
}

impl EditorUi {
    /// Whether "Apply" works: the draft changed, it breaks no rule, no Apply waits, and the
    /// daemon is online when the Graph has a sensor end node.
    pub fn can_apply(&self, view: &GraphView, online: bool) -> bool {
        let needs_daemon = self.draft.graph().sensor_end().is_some();
        self.draft.is_dirty()
            && self.apply.state != ApplyState::Pending
            && (online || !needs_daemon)
            && validate(&self.draft.candidate(), &view.graphs()).is_empty()
    }

    /// "Apply": the command that makes the Graph service store the draft. The editor then waits
    /// for the answer: `follow` takes it. `None` while "Apply" is off.
    pub fn apply(&mut self, view: &GraphView, online: bool) -> Option<Command> {
        if !self.can_apply(view, online) {
            return None;
        }
        self.apply.state = ApplyState::Pending;
        let graph: Graph = self.draft.candidate();
        Some(Command::Graph(GraphCommand::Apply(Box::new(graph))))
    }

    /// Follow what the Graph service shows. The stored Graph of the draft stays in step with the
    /// service. An Apply that waits gets its answer: the service stored the draft, or it gives
    /// the reason of its refusal. Returns where the user goes when the Apply of the question
    /// on leaving worked.
    pub fn follow(&mut self, view: &GraphView, now_us: u64) -> Option<Leave> {
        let id = self.draft.graph().id.clone();
        let stored = view.row(&id).and_then(GraphRow::graph);
        if stored != self.draft.stored() {
            self.draft.set_stored(stored.cloned());
        }
        if self.apply.state != ApplyState::Pending {
            return None;
        }
        let answer = view
            .applied
            .as_ref()
            .filter(|applied| applied.graph == id)?;
        let leaves = std::mem::take(&mut self.apply.leaves_after_apply);
        match &answer.result {
            Ok(()) => {
                self.apply.state = ApplyState::Applied { at_us: now_us };
                if leaves {
                    self.apply.leave.take()
                } else {
                    None
                }
            }
            Err(error) => {
                // The editor stays open, with the draft and the reason.
                self.apply.state = ApplyState::Refused(error.clone());
                self.apply.leave = None;
                None
            }
        }
    }

    /// The user wants to leave the editor: the back button, an entry of the sidebar or "Quit".
    /// A draft that is applied leaves at once: the function returns where to go. A draft that
    /// is not applied opens the question.
    pub fn request_leave(&mut self, target: Leave) -> Option<Leave> {
        if self.draft.is_dirty() || self.apply.state == ApplyState::Pending {
            self.apply.leave = Some(target);
            None
        } else {
            Some(target)
        }
    }

    /// "Cancel" in the question: the editor stays, with its draft.
    pub fn cancel_leave(&mut self) {
        if self.apply.state != ApplyState::Pending {
            self.apply.leave = None;
        }
    }

    /// "Discard" in the question: the draft goes. Returns where the user goes.
    pub fn discard_leave(&mut self) -> Option<Leave> {
        if self.apply.state == ApplyState::Pending {
            return None;
        }
        self.apply.leave.take()
    }

    /// "Apply" in the question: the command of `apply`. The editor closes when the service
    /// stored the draft, and stays open when the Apply fails.
    pub fn apply_and_leave(&mut self, view: &GraphView, online: bool) -> Option<Command> {
        self.apply.leave.as_ref()?;
        let command = self.apply(view, online)?;
        self.apply.leaves_after_apply = true;
        Some(command)
    }
}
