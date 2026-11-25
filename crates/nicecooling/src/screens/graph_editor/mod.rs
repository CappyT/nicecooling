//! The Graph editor (`docs/ui/screens/graph-editor.md`). It builds one Graph on a draft: nodes on
//! a canvas, connections between their pins, and one end node. The app stores the Graph at
//! "Apply".
//!
//! `EditorUi` is the state that the user changes, and `EditorModel` is what the window shows. The
//! tests read both.

pub mod draft;
pub mod rules;

use nicecooling_graph::model::Graph;
use nicecooling_graph::validate::validate;

use self::draft::Draft;
use self::rules::first_rule_text;
use crate::graphs::view::{GraphRow, GraphView};
use crate::i18n::Locale;
use crate::screens::ScreenInput;

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
}

/// The meaning of a key press for the editor: Ctrl+Z is undo and Ctrl+Shift+Z is redo. `editing`
/// is true while a text field has an open edit: the keys then belong to the field.
pub fn editor_key(key: &str, control: bool, shift: bool, editing: bool) -> Option<EditorKey> {
    if editing || !control {
        return None;
    }
    match (key.eq_ignore_ascii_case("z"), shift) {
        (true, false) => Some(EditorKey::Undo),
        (true, true) => Some(EditorKey::Redo),
        _ => None,
    }
}

/// The state of the editor that the user changes.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorUi {
    pub draft: Draft,
    pub selected: Option<Selected>,
}

impl EditorUi {
    /// Open the editor on `graph`: a stored Graph, or a Graph that the app does not store yet.
    pub fn open(graph: Graph, view: &GraphView) -> Self {
        let stored = view.row(&graph.id).and_then(GraphRow::graph).cloned();
        Self {
            draft: Draft::new(graph, stored),
            selected: None,
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

    /// Do what a key asks. Returns whether the draft changed.
    pub fn press(&mut self, key: EditorKey) -> bool {
        match key {
            EditorKey::Undo => self.undo(),
            EditorKey::Redo => self.redo(),
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
    /// "Not applied" while the draft differs from the stored Graph.
    pub draft_state: Option<String>,
    pub undo: Action,
    pub redo: Action,
    /// The first rule of the validator that the draft breaks.
    pub rule: Option<String>,
    /// The node that the broken rule is about.
    pub rule_node: Option<u32>,
    pub try_run: Action,
    pub apply: Action,
}

/// What the editor shows.
#[derive(Clone, Debug, PartialEq)]
pub struct EditorModel {
    pub toolbar: Toolbar,
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
        Self {
            toolbar: Toolbar {
                back: locale.tr("nav.graphs"),
                name: ui.draft.graph().name.clone(),
                draft_state: dirty.then(|| locale.tr("graph_editor.not_applied")),
                undo: Action::new(locale.tr("graph_editor.undo"), ui.draft.can_undo()),
                redo: Action::new(locale.tr("graph_editor.redo"), ui.draft.can_redo()),
                rule,
                rule_node: refusals.first().and_then(|refusal| refusal.node),
                try_run: Action::new(locale.tr("graph_editor.try"), valid),
                apply: Action::new(locale.tr("graph_editor.apply"), dirty && valid),
            },
        }
    }
}
