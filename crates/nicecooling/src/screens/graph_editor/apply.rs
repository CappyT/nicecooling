//! "Apply" and leaving the editor.
//!
//! "Apply" sends the draft to the Graph service. The service makes or renames the Graph sensor
//! of a Graph with a sensor end node first, and that call can wait for the daemon. The editor
//! never waits in the window: it shows "Applying" with its buttons off until the service
//! answers. A refusal keeps the draft, and the toolbar shows the reason.
//!
//! Leaving the editor with a draft that is not applied asks: "Cancel", "Discard" or "Apply".

use nicecooling_core::graph_sensor::MAX_GRAPH_SENSORS;
use nicecooling_graph::model::Graph;
use nicecooling_graph::validate::validate;

use super::EditorUi;
use super::params::user_text;
use super::rules::first_rule_text;
use crate::command::Command;
use crate::graphs::channel::GraphCommand;
use crate::graphs::transfer::new_graph_id;
use crate::graphs::values::SensorError;
use crate::graphs::view::{ApplyError, GraphRow, GraphView};
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
    /// The question on leaving is open, with the place that the user wanted to go to. An Apply
    /// that the service stores while the question is open closes the editor.
    pub leave: Option<Leave>,
    /// The app stores no Graph with the id of the draft: a new draft, or a Graph that was
    /// deleted. A row that then shows this id is another Graph.
    unstored: bool,
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
    /// Why the service did not store the draft, after an Apply that failed.
    pub error: Option<String>,
}

/// Why "Apply" is off for a draft that changed and breaks no rule: the daemon is offline, and the
/// Graph has a sensor end node. `None` when the daemon is not in the way.
pub fn offline_reason(input: &ScreenInput<'_>, ui: &EditorUi) -> Option<String> {
    let needs_daemon = ui.draft.graph().sensor_end().is_some();
    (needs_daemon && !input.state.is_online()).then(|| input.locale.tr("graph_editor.offline"))
}

/// The text of a Graph sensor that something uses. The snapshot gives the names of the users:
/// Curves, Loops, Derived sensors and Software sensor slots. The reason of the daemon has ids:
/// it shows only for a user that the snapshot does not have.
fn in_use_text(reason: &str, input: &ScreenInput<'_>, ui: &EditorUi) -> String {
    let locale = input.locale;
    let stored = ui.draft.stored();
    let sensor = stored.and_then(|stored| stored.sensor_id.as_deref());
    let users = sensor
        .map(|sensor| sensor_users(input.state, sensor))
        .unwrap_or_default();
    let slots: Vec<String> = input
        .state
        .snapshot
        .leakshield_data
        .values()
        .flat_map(|device| &device.software_sensors)
        .filter(|(_, held)| sensor.is_some() && held.as_deref() == sensor)
        .map(|(slot, _)| slot.to_string())
        .collect();
    let one = |key: &str, name: &str| locale.fmt(key, &[("name", name)]);
    match (users.as_slice(), slots.as_slice()) {
        ([], []) => locale.fmt(
            "graph_editor.refused.in_use_detail",
            &[("detail", reason.trim())],
        ),
        ([SensorUse::Curve { name, .. }], []) => one("graph_editor.not_applied_curve", name),
        ([SensorUse::Loop { name, .. }], []) => one("graph_editor.refused.in_use_loop", name),
        ([SensorUse::Derived { name, .. }], []) => one("graph_editor.refused.in_use_derived", name),
        ([], [slot]) => locale.fmt("graph_editor.refused.in_use_slot", &[("slot", slot)]),
        (users, slots) => {
            let names: Vec<String> = users
                .iter()
                .map(|user| user_text(user, locale))
                .chain(
                    slots
                        .iter()
                        .map(|slot| locale.fmt("leakshield.sensors.software_name", &[("n", slot)])),
                )
                .collect();
            locale.fmt(
                "graph_editor.refused.in_use_many",
                &[("users", &names.join(", "))],
            )
        }
    }
}

/// The text of a refused Apply for the toolbar: "Not applied: the limit of 64 Graph sensors is
/// reached." Each kind of refusal has its own sentence in the locale files. A text of the daemon
/// or of the service that the app does not know shows after such a sentence, as its detail.
pub fn refusal_text(
    error: &ApplyError,
    input: &ScreenInput<'_>,
    view: &GraphView,
    ui: &EditorUi,
) -> String {
    let locale = input.locale;
    let detail = |key: &str, text: &str| locale.fmt(key, &[("detail", text.trim())]);
    match error {
        ApplyError::Rules(refusals) => {
            let others = view.graphs();
            let rule = first_rule_text(refusals, &ui.draft.candidate(), &others, locale);
            locale.fmt(
                "graph_editor.not_applied_reason",
                &[("reason", rule.unwrap_or_default().trim_end_matches('.'))],
            )
        }
        ApplyError::Sensor(error) => match error {
            SensorError::Unavailable => locale.tr("graph_editor.refused.unavailable"),
            SensorError::Denied => locale.tr("graph_editor.refused.denied"),
            SensorError::Missing => locale.tr("graph_editor.refused.missing"),
            SensorError::InUse(reason) => in_use_text(reason, input, ui),
            // The text of the daemon at its limit of Graph sensors.
            SensorError::Refused(reason)
                if *reason == format!("at most {MAX_GRAPH_SENSORS} Graph sensors") =>
            {
                locale.tr("graph_editor.refused.limit")
            }
            SensorError::Refused(reason) => detail("graph_editor.refused.refused_detail", reason),
            // The service gives up a call after 120 s, with this text.
            SensorError::Failed(reason) if *reason == locale.tr("graph_refusal.no_answer") => {
                locale.tr("graph_editor.refused.no_answer")
            }
            SensorError::Failed(reason) => detail("graph_editor.refused.failed_detail", reason),
        },
        ApplyError::Other(reason) => detail("graph_editor.refused.other_detail", reason),
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
        error: match &ui.apply.state {
            ApplyState::Refused(error) => Some(refusal_text(error, input, view, ui)),
            _ => None,
        },
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
    /// the reason of its refusal. Returns where the user goes when the service stored the draft
    /// while the question on leaving was open: the draft is applied, so the question closes and
    /// the leave goes on. After a refusal the question stays open, with the reason.
    pub fn follow(&mut self, view: &GraphView, now_us: u64) -> Option<Leave> {
        let mut id = self.draft.graph().id.clone();
        if self.apply.state != ApplyState::Pending {
            if view.row(&id).is_none() {
                self.apply.unstored = true;
            } else if self.apply.unstored {
                // Another Graph took the id of the draft. With that id, "Apply" would replace
                // the other Graph: the draft takes a free id.
                id = new_graph_id(&view.graphs());
                self.draft.set_id(&id);
            }
        }
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
        match &answer.result {
            Ok(stored_id) => {
                // The store gives a new Graph its id: the draft follows it.
                if *stored_id != id {
                    self.draft.set_id(stored_id);
                    let stored = view.row(stored_id).and_then(GraphRow::graph);
                    self.draft.set_stored(stored.cloned());
                }
                self.apply.unstored = false;
                self.apply.state = ApplyState::Applied { at_us: now_us };
                self.apply.leave.take()
            }
            Err(error) => {
                // The editor stays open, with the draft and the reason.
                self.apply.state = ApplyState::Refused(error.clone());
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
        self.apply(view, online)
    }
}
