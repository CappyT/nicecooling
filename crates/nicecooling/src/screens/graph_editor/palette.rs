//! The node list of the editor: the 51 kinds by family, with a search field.
//!
//! At a window width of 1280 px or more the list is a column of the editor. Below 1280 px it is
//! a popover that the button "Add node" of the toolbar opens.

use nicecooling_graph::model::{Family, Kind};

use super::EditorUi;
use super::rules::kind_name;
use crate::i18n::Locale;

/// The window width from which the node list is a column.
pub const COLUMN_MIN_WIDTH: f32 = 1280.0;

/// The families, in the order of the list.
pub const FAMILIES: [Family; 7] = [
    Family::Sources,
    Family::Math,
    Family::Logic,
    Family::StateAndTime,
    Family::Filters,
    Family::Generators,
    Family::Other,
];

/// The part of the locale key of a family name.
pub const fn family_key(family: Family) -> &'static str {
    match family {
        Family::Sources => "sources",
        Family::Math => "math",
        Family::Logic => "logic",
        Family::StateAndTime => "state_and_time",
        Family::Filters => "filters",
        Family::Generators => "generators",
        Family::Other => "other",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListForm {
    /// The left column of the editor. A drag of a row onto the canvas adds a node.
    Column,
    /// A popover under the button "Add node". A drag of a row adds no node.
    Popover,
}

pub fn list_form(window_width: f32) -> ListForm {
    if window_width >= COLUMN_MIN_WIDTH {
        ListForm::Column
    } else {
        ListForm::Popover
    }
}

/// The state of the node list that the user changes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ListUi {
    pub search: String,
    /// The families that the user folded.
    pub folded: Vec<Family>,
    /// The popover is open.
    pub open: bool,
    /// The row that the keyboard marks.
    pub cursor: Option<Kind>,
    /// The search field takes the focus at the next frame: the popover opened.
    pub focus_search: bool,
    /// The button "Add node" takes the focus at the next frame: Escape closed the popover.
    pub focus_button: bool,
}

/// A key of the node list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKey {
    Up,
    Down,
    /// Fold the family of the marked row.
    Left,
    /// Unfold the family of the marked row.
    Right,
    /// Add the marked kind at the centre of the view.
    Enter,
    /// Close the popover.
    Escape,
}

impl ListKey {
    /// The key of the node list for a key name of the window.
    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "up" => Some(Self::Up),
            "down" => Some(Self::Down),
            "left" => Some(Self::Left),
            "right" => Some(Self::Right),
            "enter" => Some(Self::Enter),
            "escape" => Some(Self::Escape),
            _ => None,
        }
    }
}

/// One kind of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListRow {
    pub kind: Kind,
    pub name: String,
    /// The full name and the help text of the kind, or why the row is off.
    pub tooltip: String,
    /// False for the two end kinds: the Graph has its end node.
    pub enabled: bool,
    /// The keyboard marks this row.
    pub marked: bool,
    /// A drag of the row onto the canvas adds a node: in the column form only.
    pub draggable: bool,
}

/// One family of the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ListFamily {
    pub family: Family,
    pub name: String,
    /// The kinds of the family that the list holds now.
    pub count: usize,
    /// The rows of the family show.
    pub open: bool,
    pub rows: Vec<ListRow>,
}

/// The node list as the editor shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeList {
    pub form: ListForm,
    /// The list shows: always in the column form, and while the popover is open.
    pub open: bool,
    /// "Search nodes".
    pub search: String,
    pub families: Vec<ListFamily>,
    /// "No node kind has this name."
    pub empty: Option<String>,
}

/// The kinds of a family whose name holds the search text, without case.
fn matches(family: Family, query: &str, locale: Locale) -> Vec<(Kind, String)> {
    Kind::ALL
        .iter()
        .copied()
        .filter(|kind| kind.family() == family)
        .map(|kind| (kind, kind_name(kind, locale)))
        .filter(|(_, name)| query.is_empty() || name.to_lowercase().contains(query))
        .collect()
}

/// Whether the rows of a family show: with a search text every family with a match is open.
fn family_open(list: &ListUi, family: Family, query: &str) -> bool {
    !query.is_empty() || !list.folded.contains(&family)
}

pub fn node_list(ui: &EditorUi, locale: Locale) -> NodeList {
    let query = ui.list.search.trim().to_lowercase();
    let form = list_form(ui.width);
    let end_taken = locale.tr("graph_editor.list.end_taken");
    let families: Vec<ListFamily> = FAMILIES
        .into_iter()
        .filter_map(|family| {
            let kinds = matches(family, &query, locale);
            if !query.is_empty() && kinds.is_empty() {
                return None;
            }
            let rows: Vec<ListRow> = kinds
                .into_iter()
                .map(|(kind, name)| {
                    let enabled = !kind.is_end();
                    let tooltip = if enabled {
                        let help = locale.tr(&format!("graph_editor.kind.{}.help", kind.as_str()));
                        format!("{name}: {help}")
                    } else {
                        end_taken.clone()
                    };
                    ListRow {
                        kind,
                        name,
                        tooltip,
                        enabled,
                        marked: ui.list.cursor == Some(kind),
                        draggable: enabled && form == ListForm::Column,
                    }
                })
                .collect();
            Some(ListFamily {
                family,
                name: locale.tr(&format!("graph_editor.family.{}", family_key(family))),
                count: rows.len(),
                open: family_open(&ui.list, family, &query),
                rows,
            })
        })
        .collect();
    NodeList {
        form,
        open: form == ListForm::Column || ui.list.open,
        search: locale.tr("graph_editor.list.search"),
        empty: families
            .is_empty()
            .then(|| locale.tr("graph_editor.list.no_match")),
        families,
    }
}

impl EditorUi {
    /// The window has another width. A width of 1280 px or more closes the popover.
    pub fn set_width(&mut self, width: f32) {
        self.width = width;
        if list_form(width) == ListForm::Column {
            self.list.open = false;
        }
    }

    /// The button "Add node": open or close the popover. It opens with the focus in the search
    /// field.
    pub fn toggle_list(&mut self) {
        if list_form(self.width) == ListForm::Column {
            return;
        }
        self.list.open = !self.list.open;
        self.list.focus_search = self.list.open;
        self.list.cursor = None;
    }

    /// A press outside the popover closes it.
    pub fn press_outside_list(&mut self) {
        self.list.open = false;
    }

    /// The kinds of the rows that show, in the order of the list.
    fn shown_kinds(&self, locale: Locale) -> Vec<Kind> {
        let query = self.list.search.trim().to_lowercase();
        FAMILIES
            .into_iter()
            .filter(|family| family_open(&self.list, *family, &query))
            .flat_map(|family| matches(family, &query, locale))
            .map(|(kind, _)| kind)
            .collect()
    }

    /// A double click on a row of the list: add a node of the kind at the centre of the view.
    /// The popover stays open.
    pub fn list_double_click(&mut self, kind: Kind) -> Option<u32> {
        self.add_node_at(kind, self.view_centre)
    }

    /// A row of the list that the user lets go at `point` of the canvas. The column form adds
    /// the node there. A drag from the popover adds no node.
    pub fn drop_from_list(&mut self, kind: Kind, point: (f64, f64)) -> Option<u32> {
        match list_form(self.width) {
            ListForm::Column => self.add_node_at(kind, point),
            ListForm::Popover => None,
        }
    }

    /// A key in the node list. Returns the id of the node that Enter added.
    pub fn list_key(&mut self, key: ListKey, locale: Locale) -> Option<u32> {
        let shown = self.shown_kinds(locale);
        let at = self
            .list
            .cursor
            .and_then(|cursor| shown.iter().position(|kind| *kind == cursor));
        match key {
            ListKey::Down => {
                let next = at.map_or(0, |at| (at + 1).min(shown.len().saturating_sub(1)));
                self.list.cursor = shown.get(next).copied();
            }
            ListKey::Up => {
                let previous = at.map_or(0, |at| at.saturating_sub(1));
                self.list.cursor = shown.get(previous).copied();
            }
            ListKey::Left | ListKey::Right => {
                if let Some(family) = self.list.cursor.map(Kind::family) {
                    self.list.folded.retain(|folded| *folded != family);
                    if key == ListKey::Left {
                        self.list.folded.push(family);
                    }
                }
            }
            ListKey::Enter => {
                let kind = self.list.cursor.filter(|_| at.is_some())?;
                return self.add_node_at(kind, self.view_centre);
            }
            ListKey::Escape => {
                if self.list.open {
                    self.list.open = false;
                    self.list.focus_button = true;
                }
            }
        }
        None
    }

    /// A click on a family row folds or unfolds it.
    pub fn toggle_family(&mut self, family: Family) {
        if self.list.folded.contains(&family) {
            self.list.folded.retain(|folded| *folded != family);
        } else {
            self.list.folded.push(family);
        }
    }
}
