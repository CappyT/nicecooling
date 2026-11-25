//! The Graph editor (`docs/ui/screens/graph-editor.md`). It builds one Graph on a draft: nodes on
//! a canvas, connections between their pins, and one end node. The app stores the Graph at
//! "Apply".
//!
//! `EditorUi` is the state that the user changes, and `EditorModel` is what the window shows. The
//! tests read both.

pub mod apply;
pub mod canvas;
pub mod draft;
pub mod live;
pub mod palette;
pub(crate) mod panel_view;
pub mod params;
pub mod response_panel;
pub mod rules;
pub mod try_run;
pub(crate) mod view;

use nicecooling_graph::model::Graph;
use nicecooling_graph::validate::validate;

use self::apply::{ApplyState, ApplyUi, LeaveDialog, leave_dialog, offline_reason, refusal_text};
use self::canvas::{EdgeFace, NodeFace, edge_faces, node_faces};
use self::draft::Draft;
use self::live::LiveValues;
use self::palette::{ListForm, ListUi, NodeList, node_list};
use self::params::{Panel, panel};
use self::response_panel::Network;
use self::rules::first_rule_text;
use self::try_run::{TryBanner, TryUi, try_banner};
use crate::graphs::view::{GraphRow, GraphView, ValuesState};
use crate::i18n::Locale;
use crate::screens::ScreenInput;
use crate::screens::format::clock;

/// A number of a parameter or of a node title: at most 8 decimals, no trailing zero, and the
/// decimal mark of the language.
pub fn number_text(locale: Locale, value: f64) -> String {
    let mut text = format!("{value:.8}");
    if text.contains('.') {
        let kept = text.trim_end_matches('0').trim_end_matches('.').len();
        text.truncate(kept);
    }
    if text == "-0" {
        text = "0".to_owned();
    }
    if matches!(locale.code(), "it" | "de" | "fr") {
        text = text.replace('.', ",");
    }
    text
}

/// What the user selected on the canvas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selected {
    Node(u32),
    /// The connection that ends on the input pin `pin` of the node `to`.
    Connection {
        to: u32,
        pin: String,
    },
}

/// What a key does in the editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditorKey {
    Undo,
    Redo,
    ZoomIn,
    ZoomOut,
    /// Put the whole Graph in the view.
    ShowAll,
}

/// The meaning of a key press for the editor: Ctrl+Z is undo and Ctrl+Shift+Z is redo. Ctrl with
/// "+" and Ctrl with "-" zoom, and Ctrl+0 puts the whole Graph in the view. `editing` is true
/// while a text field has an open edit: the keys then belong to the field.
pub fn editor_key(key: &str, control: bool, shift: bool, editing: bool) -> Option<EditorKey> {
    if editing || !control {
        return None;
    }
    match key {
        "z" | "Z" if shift => Some(EditorKey::Redo),
        "z" | "Z" => Some(EditorKey::Undo),
        "+" | "=" => Some(EditorKey::ZoomIn),
        "-" | "_" => Some(EditorKey::ZoomOut),
        "0" => Some(EditorKey::ShowAll),
        _ => None,
    }
}

/// The state of the editor that the user changes.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorUi {
    pub draft: Draft,
    pub selected: Option<Selected>,
    pub list: ListUi,
    /// The width of the window in px: it chooses the form of the node list.
    pub width: f32,
    /// The middle of the view of the canvas, in units of the canvas. A new node goes there.
    pub view_centre: (f64, f64),
    /// The zoom of the canvas: 1 is 100%.
    pub zoom: f32,
    /// The reason that the canvas gave for the last refused connection, and the time until
    /// which the status line shows it, in microseconds since the Unix epoch.
    pub refused: Option<(String, u64)>,
    /// The key of the field that holds a value that did not commit, and the range that the
    /// panel shows under it.
    pub field_error: Option<(String, String)>,
    /// The node values that the editor read last.
    pub live: LiveValues,
    /// The test values of the Sensor nodes, and the try that runs.
    pub tries: TryUi,
    /// The state of "Apply", and the question on leaving.
    pub apply: ApplyUi,
    /// What the response panel needs of the network settings of the app.
    pub network: Network,
    /// The last "Run once" ran nothing: the poweroff countdown of an Alarm runs.
    pub run_blocked: bool,
}

impl EditorUi {
    /// Open the editor on `graph`: a stored Graph, or a Graph that the app does not store yet.
    pub fn open(graph: Graph, view: &GraphView) -> Self {
        let stored = view.row(&graph.id).and_then(GraphRow::graph).cloned();
        Self {
            draft: Draft::new(graph, stored),
            selected: None,
            list: ListUi::default(),
            width: palette::COLUMN_MIN_WIDTH,
            view_centre: (0.0, 0.0),
            zoom: 1.0,
            refused: None,
            field_error: None,
            live: LiveValues::default(),
            tries: TryUi::default(),
            apply: ApplyUi::default(),
            network: Network::default(),
            run_blocked: false,
        }
    }

    pub fn undo(&mut self) -> bool {
        let done = self.draft.undo();
        self.drop_lost_selection();
        done
    }

    pub fn redo(&mut self) -> bool {
        let done = self.draft.redo();
        self.drop_lost_selection();
        done
    }

    /// Do what a key asks of the draft. Returns whether the draft changed. The keys of the view
    /// change nothing here: the canvas takes them.
    pub fn press(&mut self, key: EditorKey) -> bool {
        match key {
            EditorKey::Undo => self.undo(),
            EditorKey::Redo => self.redo(),
            EditorKey::ZoomIn | EditorKey::ZoomOut | EditorKey::ShowAll => false,
        }
    }

    /// Remove a node with its connections, as one undo step. An end node stays.
    pub fn remove_node(&mut self, id: u32) -> bool {
        let done = self.draft.remove_node(id);
        self.drop_lost_selection();
        done
    }

    /// Clear a selection that names a node or a connection that the draft no longer has.
    fn drop_lost_selection(&mut self) {
        let graph = self.draft.graph();
        let kept = match &self.selected {
            Some(Selected::Node(id)) => graph.node(*id).is_some(),
            Some(Selected::Connection { to, pin }) => graph.connection_to(*to, pin).is_some(),
            None => true,
        };
        if !kept {
            self.selected = None;
        }
    }
}

/// A button of the editor: its text, and whether it works.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub label: String,
    pub enabled: bool,
    /// Why the button is off.
    pub tooltip: Option<String>,
}

impl Action {
    fn new(label: String, enabled: bool) -> Self {
        Self {
            label,
            enabled,
            tooltip: None,
        }
    }
}

/// The bar above the canvas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toolbar {
    /// The button that goes back to the Graphs screen.
    pub back: String,
    pub name: String,
    /// "Applying" while the service has not answered, "Not applied" while the draft differs
    /// from the stored Graph, or "Applied 21:44:02" after an Apply.
    pub draft_state: Option<String>,
    /// Why the service did not store the draft: "Not applied: the limit of 64 Graph sensors is
    /// reached."
    pub refused: Option<String>,
    /// The button "Add node", below a window width of 1280 px only.
    pub add_node: Option<Action>,
    pub undo: Action,
    pub redo: Action,
    /// The first rule of the validator that the draft breaks.
    pub rule: Option<String>,
    /// The node that the broken rule is about.
    pub rule_node: Option<u32>,
    pub try_run: Action,
    pub apply: Action,
}

/// The line under the canvas.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusLine {
    /// The reason of a refused connection for 5 s, or a read error, or why the nodes have no
    /// value.
    pub text: Option<String>,
    /// "Retry", next to a read error of the node values.
    pub retry: Option<String>,
    pub zoom_out: String,
    pub zoom_in: String,
    /// "100 %".
    pub zoom: String,
}

/// What the editor shows.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorModel {
    pub toolbar: Toolbar,
    pub list: NodeList,
    pub nodes: Vec<NodeFace>,
    pub edges: Vec<EdgeFace>,
    pub status: StatusLine,
    /// The text in the middle of the canvas of a Graph that has its end node only.
    pub hint: Option<String>,
    /// The parameter panel: the selected node, or the end node.
    pub panel: Option<Panel>,
    /// The banner under the toolbar, while a try runs.
    pub try_banner: Option<TryBanner>,
    /// The question on leaving with a draft that is not applied.
    pub leave: Option<LeaveDialog>,
}

impl EditorModel {
    /// `view` is what the Graph service shows now.
    pub fn build(input: &ScreenInput<'_>, view: &GraphView, ui: &EditorUi) -> Self {
        let locale = input.locale;
        let others = view.graphs();
        let candidate = ui.draft.candidate();
        let refusals = validate(&candidate, &others);
        let rule = first_rule_text(&refusals, &candidate, &others, locale);
        let valid = rule.is_none();
        let dirty = ui.draft.is_dirty();
        // The buttons are off while the service has not answered an Apply.
        let pending = ui.apply.state == ApplyState::Pending;
        let offline = offline_reason(input, ui);
        let draft_state = match &ui.apply.state {
            ApplyState::Pending => Some(locale.tr("graph_editor.applying")),
            _ if dirty => Some(locale.tr("graph_editor.not_applied")),
            ApplyState::Applied { at_us } => {
                Some(locale.fmt("feedback.applied", &[("time", &clock(*at_us))]))
            }
            _ => None,
        };
        let refused = match &ui.apply.state {
            ApplyState::Refused(error) => Some(refusal_text(error, input, view, ui)),
            _ => None,
        };
        let rule_node = refusals.first().and_then(|refusal| refusal.node);
        let list = node_list(ui, locale);
        let hint = (ui.draft.graph().nodes.len() <= 1).then(|| {
            locale.tr(match list.form {
                ListForm::Column => "graph_editor.list.hint_column",
                ListForm::Popover => "graph_editor.list.hint_popover",
            })
        });
        let percent = (f64::from(ui.zoom) * 100.0).round().to_string();
        // The line has one text: the reason of a refused connection first, then a read error,
        // then why the nodes have no value.
        let reason = ui.refusal_reason(input.now_us, locale);
        let read_error = reason.is_none() && ui.live.failed;
        let status_text = reason.or_else(|| {
            if ui.live.failed {
                Some(locale.tr("graph_editor.live.values_error"))
            } else if !ui.live.enabled {
                Some(locale.tr("graph_editor.canvas.disabled"))
            } else if ui.live.access == ValuesState::NoAccess {
                Some(locale.tr("graph_editor.live.no_access"))
            } else {
                None
            }
        });
        Self {
            nodes: node_faces(input, ui, rule_node),
            edges: edge_faces(ui),
            status: StatusLine {
                text: status_text,
                retry: read_error.then(|| locale.tr("graph_editor.live.retry")),
                zoom_out: locale.tr("graph_editor.canvas.zoom_out"),
                zoom_in: locale.tr("graph_editor.canvas.zoom_in"),
                zoom: locale.fmt("graph_editor.canvas.zoom", &[("percent", &percent)]),
            },
            hint,
            panel: panel(input, view, ui),
            try_banner: try_banner(ui, locale),
            leave: leave_dialog(input, view, ui),
            toolbar: Toolbar {
                back: locale.tr("nav.graphs"),
                name: ui.draft.graph().name.clone(),
                draft_state,
                refused,
                add_node: (list.form == ListForm::Popover)
                    .then(|| Action::new(locale.tr("graph_editor.add_node"), true)),
                undo: Action::new(
                    locale.tr("graph_editor.undo"),
                    ui.draft.can_undo() && !pending,
                ),
                redo: Action::new(
                    locale.tr("graph_editor.redo"),
                    ui.draft.can_redo() && !pending,
                ),
                rule,
                rule_node,
                // "Try" is off while the draft breaks a rule and while a try runs.
                try_run: Action::new(
                    locale.tr("graph_editor.try"),
                    valid && !pending && ui.tries.running.is_none(),
                ),
                apply: Action {
                    label: locale.tr("graph_editor.apply"),
                    enabled: dirty && valid && !pending && offline.is_none(),
                    tooltip: offline.filter(|_| dirty && valid),
                },
            },
            list,
        }
    }
}
