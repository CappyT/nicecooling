//! The window of the Graphs screen: the button row, the table of the Graphs and the Graph
//! sensors that no Graph of this user holds.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Div, ElementId, FontWeight, KeyDownEvent, SharedString, Window, div,
    px,
};
use gpui_component::button::ButtonVariants as _;
use gpui_component::switch::Switch;
use gpui_component::{Disableable as _, h_flex, v_flex};
use nicecooling_graph::model::Kind;

use crate::command::Command;
use crate::graphs::channel::GraphCommand;
use crate::graphs::transfer::{export_file, import_file};
use crate::graphs::view::lock_view;
use crate::icons::{NcIcon, icon, small_icon, text_icon};
use crate::screen_view::{ScreenView, skeleton};
use crate::screens::controls::act;
use crate::screens::graphs::{
    ConfirmDialog, GraphLine, GraphsModel, GraphsState, GraphsUi, LineState, OrphanCard, home_dir,
};
use crate::screens::ui::Tip as _;
use crate::screens::ui::{
    TagKind, caption, card, card_header, card_row, cell, heading, label, modal, notice_row, page,
    table_header, table_row, tag,
};
use crate::shell::Page;
use crate::tokens::Palette;

/// The widths of the cells after the name. The name takes the rest of the row. The cell of the
/// buttons has the width of its buttons: their texts change with the language.
const END: f32 = 92.;
const STATE: f32 = 100.;
const RESULT: f32 = 156.;
/// The gap between the switch of a row and its buttons, and the gap between two buttons.
const SWITCH_GAP: f32 = 6.;
const BUTTON_GAP: f32 = 3.;

impl ScreenView {
    pub(crate) fn render_graphs(
        &mut self,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shown = self.model.read(cx).graphs();
        if let Some(shown) = &shown {
            self.graphs.follow(shown);
        }
        let model = self.with_input(cx, |input| {
            GraphsModel::build(input, shown.as_ref(), &self.graphs)
        });
        if model.editor.is_some()
            && let Some(shown) = &shown
        {
            return self.render_graph_editor(shown, p, window, cx);
        }
        self.close_graph_editor();
        self.drop_graph_draft(cx);
        let view = cx.entity();
        let create = model.create_enabled;

        let mut tools = h_flex()
            .w_full()
            .gap(px(6.))
            .items_center()
            .child(
                Self::button("graph-new-sensor", model.new_sensor.clone())
                    .icon(text_icon(NcIcon::Plus))
                    .disabled(!create)
                    .on_click(act(&view, |this, _, cx| {
                        this.new_graph(Kind::SensorEnd, cx);
                    })),
            )
            .child(
                Self::button("graph-new-response", model.new_response.clone())
                    .icon(text_icon(NcIcon::Plus))
                    .disabled(!create)
                    .on_click(act(&view, |this, _, cx| {
                        this.new_graph(Kind::ResponseEnd, cx);
                    })),
            )
            .child(div().flex_1());
        if let Some(note) = &model.create_note {
            tools = tools.child(caption(note.clone(), p).flex_none());
        }
        tools = tools.child(
            Self::button("graph-import", model.import.clone())
                .disabled(!create)
                .on_click(act(&view, |this, _, cx| this.import_graph(cx))),
        );
        // The result of the last export or import, above the table.
        let notice = model.notice.as_ref().map(|notice| {
            notice_row(notice.text.clone(), notice.warning, p)
                .child(
                    Self::icon_button("graph-notice-close", NcIcon::X)
                        .tip(model.close.clone())
                        .on_click(act(&view, |this, _, _| {
                            this.graphs.notice = None;
                        })),
                )
                .into_any_element()
        });

        let mut content = page().child(tools);
        let titles = model.columns.clone();
        let widths = [None, Some(END), Some(STATE), Some(RESULT)];
        // The last cell of the header has the width of the buttons of a row, so the titles
        // stand above their columns.
        let header = || table_header(&titles, &widths, p).child(Self::graph_buttons_width(&model));
        match model.state {
            GraphsState::Loading => {
                let mut table = card(p)
                    .child(card_header(model.title.clone(), p))
                    .child(header());
                for width in [220., 300., 180.] {
                    table = table.child(table_row(p).min_h(px(48.)).child(skeleton(p, width)));
                }
                content = content.child(table);
            }
            GraphsState::FileError => {
                let (text, error) = model.file_error.clone().unwrap_or_default();
                let retry = model.retry.clone().map(|text| {
                    Self::small_button("graph-file-retry", text, None).on_click(act(
                        &view,
                        |this, _, cx| {
                            let command = this.graphs.retry();
                            this.send_graph(command, cx);
                        },
                    ))
                });
                content = content.child(
                    card(p).child(
                        h_flex()
                            .p(px(18.))
                            .gap(px(12.))
                            .items_start()
                            .child(icon(NcIcon::Warn, p.warning))
                            .child(
                                v_flex()
                                    .flex_1()
                                    .min_w_0()
                                    .gap(px(3.))
                                    .child(heading(text))
                                    .child(caption(error, p)),
                            )
                            .children(retry),
                    ),
                );
            }
            GraphsState::Empty => {
                let mut list = card(p).child(card_header(model.title.clone(), p));
                for text in &model.warnings {
                    list = list.child(notice_row(text.clone(), true, p));
                }
                list = list.children(notice);
                list = list.child(
                    card_row(p).child(div().flex_1().min_w_0().child(model.empty_text.clone())),
                );
                list = list.child(
                    card_row(p)
                        .child(
                            Self::small_button(
                                "graph-empty-new-sensor",
                                model.new_sensor.clone(),
                                Some(NcIcon::Plus),
                            )
                            .disabled(!create)
                            .on_click(act(&view, |this, _, cx| {
                                this.new_graph(Kind::SensorEnd, cx);
                            })),
                        )
                        .child(
                            Self::small_button(
                                "graph-empty-new-response",
                                model.new_response.clone(),
                                Some(NcIcon::Plus),
                            )
                            .disabled(!create)
                            .on_click(act(&view, |this, _, cx| {
                                this.new_graph(Kind::ResponseEnd, cx);
                            })),
                        ),
                );
                content = content.child(list);
            }
            GraphsState::Ready => {
                let mut table = card(p).child(card_header(model.title.clone(), p));
                for text in &model.warnings {
                    table = table.child(notice_row(text.clone(), true, p));
                }
                table = table.children(notice).child(header());
                for (index, line) in model.rows.iter().enumerate() {
                    table = table.child(self.graph_line(index, line, &model, p, window, cx));
                }
                content = content.child(table);
            }
        }
        if model.state != GraphsState::FileError
            && let Some(orphans) = &model.orphans
        {
            content = content.child(self.orphan_card(orphans, p, cx));
        }

        let mut root = div().id("graphs-page").relative().w_full().child(content);
        if let Some(dialog) = &model.dialog {
            root = root.child(self.graphs_dialog(dialog, p, cx));
        }
        root.into_any_element()
    }

    /// The switch and the three buttons of a row: the look that the row and the header share.
    fn graph_row_buttons(
        index: Option<usize>,
        model: &GraphsModel,
    ) -> (
        gpui_component::button::Button,
        gpui_component::button::Button,
        gpui_component::button::Button,
    ) {
        let id = |name: &str| match index {
            Some(index) => SharedString::from(format!("graph-{name}-{index}")),
            None => SharedString::from(format!("graph-{name}-width")),
        };
        (
            Self::small_button(id("edit"), model.edit.clone(), None),
            Self::flat_button(id("export"), model.export.clone()).px(px(6.)),
            Self::flat_button(id("delete"), model.delete.clone()).px(px(6.)),
        )
    }

    /// An element with the width of the buttons of a row and no height. It shows nothing and
    /// takes no focus.
    fn graph_buttons_width(model: &GraphsModel) -> Div {
        let (edit, export, delete) = Self::graph_row_buttons(None, model);
        h_flex()
            .flex_none()
            .h(px(0.))
            .overflow_hidden()
            .invisible()
            .gap(px(BUTTON_GAP))
            // The switch of a row is 36 px wide.
            .child(div().w(px(36.)).mr(px(SWITCH_GAP)).flex_none())
            .child(edit.tab_stop(false).disabled(true))
            .child(export.tab_stop(false).disabled(true))
            .child(delete.tab_stop(false).disabled(true))
    }

    /// One row of the table: the name, the end node, the state, the result and the buttons.
    fn graph_line(
        &self,
        index: usize,
        line: &GraphLine,
        model: &GraphsModel,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let id = line.id.clone();
        // The row is a Tab stop: Enter on it opens the editor, as "Edit" does.
        let focus = window
            .use_keyed_state(ElementId::from(("graph-row-focus", index)), cx, |_, cx| {
                cx.focus_handle().tab_stop(true)
            })
            .read(cx)
            .clone();
        let focused = focus.is_focused(window);
        let ink = if line.dim { p.fg_dim } else { p.fg };

        // The count of the nodes stays on one line. A reason or a refusal wraps: it is never cut.
        let note = caption(line.caption.clone(), p);
        let note = if line.caption_warns {
            note
        } else {
            note.truncate()
        };
        let name = v_flex()
            .id(("graph-name", index))
            .flex_1()
            .min_w_0()
            .child(heading(line.name.clone()).truncate())
            .child(if !line.caption_warns {
                h_flex().child(note)
            } else if line.state == LineState::Invalid {
                h_flex().child(note.text_color(p.warning))
            } else {
                h_flex()
                    .gap(px(4.))
                    .items_start()
                    .child(small_icon(NcIcon::Warn, p.warning).mt(px(2.)))
                    .child(note.text_color(p.warning))
            })
            // A double click opens the editor, as "Edit" does.
            .when(line.readable, |name| {
                let (view, id) = (view.clone(), id.clone());
                name.on_click(move |event, _, cx| {
                    if event.click_count() == 2 {
                        view.update(cx, |this, cx| {
                            this.edit_graph(&id, cx);
                            cx.notify();
                        });
                    }
                })
            });

        let mut end = v_flex()
            .w(px(END))
            .flex_none()
            .min_w_0()
            .child(label(line.end.clone()));
        if let Some(text) = &line.end_caption {
            let text = caption(text.clone(), p).truncate();
            end = end.child(if line.end_mono {
                text.font_family("Adwaita Mono")
            } else {
                text
            });
        }

        let state = match line.state {
            LineState::Invalid => h_flex()
                .w(px(STATE))
                .flex_none()
                .gap(px(6.))
                .items_center()
                .text_color(p.warning)
                .font_weight(FontWeight::SEMIBOLD)
                .child(icon(NcIcon::Warn, p.warning))
                .child(label(line.state_text.clone())),
            LineState::Disabled => cell(STATE)
                .text_color(p.fg_dim)
                .child(line.state_text.clone()),
            LineState::Running | LineState::Paused => cell(STATE).child(line.state_text.clone()),
        };

        let mut result = v_flex()
            .w(px(RESULT))
            .flex_none()
            .min_w_0()
            .text_color(ink)
            .child(match (&line.value, line.fired) {
                (Some((number, unit)), _) => h_flex()
                    .gap(px(4.))
                    .child(div().font_weight(FontWeight::BOLD).child(number.clone()))
                    .child(div().text_color(p.fg_dim).child(unit.clone())),
                (None, true) => h_flex().child(tag(line.result.clone(), TagKind::Accent, p)),
                (None, false) => h_flex().child(label(line.result.clone())),
            });
        // The time of the last run wraps in a language with a long text: it is never cut.
        if let Some(text) = &line.last_run {
            result = result.child(caption(text.clone(), p));
        }

        let switch = Switch::new(SharedString::from(format!("graph-switch-{index}")))
            .checked(line.checked)
            .disabled(!line.switch_enabled)
            .on_click({
                let (view, id) = (view.clone(), id.clone());
                move |on, _, cx| {
                    let on = *on;
                    view.update(cx, |this, cx| {
                        let command = this.graphs.toggle(&id, on);
                        this.send_graph(command, cx);
                        cx.notify();
                    })
                }
            });
        let (edit, export, delete) = Self::graph_row_buttons(Some(index), model);
        let edit = edit
            .disabled(!line.readable)
            .tip(line.read_tooltip.clone().unwrap_or_default())
            .on_click(act(&view, {
                let id = id.clone();
                move |this, _, cx| this.edit_graph(&id, cx)
            }));
        let export = export
            .disabled(!line.readable)
            .tip(line.read_tooltip.clone().unwrap_or_default())
            .on_click(act(&view, {
                let id = id.clone();
                move |this, _, cx| this.export_graph(&id, cx)
            }));
        let delete = delete
            .disabled(!line.delete_enabled)
            .tip(line.delete_tooltip.clone().unwrap_or_default())
            .on_click(act(&view, {
                let (row, readable) = (focus.clone(), line.readable);
                move |this, window, cx| {
                    // The question gives the focus back to what had it: the row.
                    if readable {
                        window.focus(&row, cx);
                    }
                    this.graphs.delete = Some(id.clone());
                }
            }));
        let buttons = h_flex()
            .flex_none()
            .gap(px(BUTTON_GAP))
            .items_center()
            .child(
                div()
                    .id(("graph-switch-cell", index))
                    .flex_none()
                    .mr(px(SWITCH_GAP))
                    .child(switch)
                    .tip(line.switch_tooltip.clone().unwrap_or_default()),
            )
            .child(edit)
            .child(export)
            .child(delete);

        table_row(p)
            .id(("graph-row", index))
            .min_h(px(48.))
            .items_center()
            .debug_selector(move || format!("graph-row-{index}"))
            .when(line.readable, |row| {
                let (view, id, own) = (view.clone(), line.id.clone(), focus.clone());
                row.track_focus(&focus)
                    .when(focused, |row| row.bg(p.hover))
                    .on_key_down(move |event, window, cx| {
                        let keys = &event.keystroke;
                        // Enter on a button of the row presses that button only.
                        if keys.key == "enter"
                            && !keys.modifiers.modified()
                            && own.is_focused(window)
                        {
                            cx.stop_propagation();
                            view.update(cx, |this, cx| {
                                this.edit_graph(&id, cx);
                                cx.notify();
                            });
                        }
                    })
            })
            .child(name)
            .child(end)
            .child(state)
            .child(result)
            .child(buttons)
            .into_any_element()
    }

    /// The card "Graph sensors without a Graph".
    fn orphan_card(&self, orphans: &OrphanCard, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let mut list = card(p).child(card_header(orphans.title.clone(), p)).child(
            div()
                .px(px(18.))
                .pb(px(9.))
                .child(caption(orphans.text.clone(), p)),
        );
        for (index, row) in orphans.rows.iter().enumerate() {
            let sensor = row.id.clone();
            let mut name = v_flex()
                .flex_1()
                .min_w_0()
                .child(heading(row.name.clone()).truncate())
                .child(caption(row.id.clone(), p).font_family("Adwaita Mono"));
            if let Some(note) = &row.note {
                name = name.child(caption(note.clone(), p).text_color(p.warning));
            }
            list = list.child(
                card_row(p)
                    .min_h(px(48.))
                    .child(name)
                    .child(
                        div()
                            .flex_none()
                            .text_color(p.fg_dim)
                            .child(row.value.clone()),
                    )
                    .child(
                        Self::flat_button(
                            SharedString::from(format!("graph-remove-{index}")),
                            orphans.remove.clone(),
                        )
                        .disabled(!row.remove_enabled)
                        .tip(row.tooltip.clone().unwrap_or_default())
                        .on_click(act(&view, move |this, _, _| {
                            this.graphs.remove = Some(sensor.clone());
                        })),
                    ),
            );
        }
        list.into_any_element()
    }

    /// The question of "Delete" and of "Remove". "Cancel" is the first button: it takes the
    /// focus first.
    fn graphs_dialog(
        &self,
        dialog: &ConfirmDialog,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let cancel = Self::button("graph-dialog-cancel", dialog.cancel.clone()).on_click(act(
            &view,
            |this, _, _| {
                this.graphs.delete = None;
                this.graphs.remove = None;
            },
        ));
        let confirm = Self::button("graph-dialog-confirm", dialog.confirm.clone())
            .danger()
            .on_click(act(&view, |this, _, cx| {
                let command = this.graphs.confirm_delete();
                let command = command.or_else(|| this.graphs.confirm_remove());
                if let Some(command) = command {
                    this.send_graph(command, cx);
                }
            }));
        modal(
            dialog.title.clone(),
            dialog.text.clone(),
            Vec::new(),
            vec![cancel.into_any_element(), confirm.into_any_element()],
            p,
        )
        .into_any_element()
    }

    /// What the user changed on the Graphs screen: a test opens a dialog or the editor with it.
    pub fn graphs_ui(&mut self) -> &mut GraphsUi {
        &mut self.graphs
    }

    /// Open the Graph editor on a new Graph with an end node of the kind `end`, on the page
    /// "Graphs": the link "New sensor" of the Sensors screen.
    pub fn open_new_graph(&mut self, end: Kind, cx: &mut Context<Self>) {
        self.new_graph(end, cx);
        if self.graphs.editor.is_some() {
            self.go(Page::Graphs, cx);
        }
    }

    /// Open the Graph editor on the Graph that holds the Graph sensor `sensor`: "Open Graph" of
    /// the Sensors screen.
    pub fn open_graph_of(&mut self, sensor: &str, cx: &mut Context<Self>) {
        let held = self
            .model
            .read(cx)
            .graphs()
            .is_some_and(|shown| self.graphs.edit_sensor(&shown, sensor));
        if held {
            self.go(Page::Graphs, cx);
        }
    }

    /// "New sensor" and "New response": open the Graph editor on a draft.
    pub(crate) fn new_graph(&mut self, end: Kind, cx: &mut Context<Self>) {
        let (shown, locale) = {
            let app = self.model.read(cx);
            (app.graphs(), app.locale)
        };
        if let Some(shown) = shown {
            self.graphs.new_draft(&shown, end, locale);
        }
    }

    /// "Edit": open the Graph editor on a stored Graph.
    pub(crate) fn edit_graph(&mut self, graph: &str, cx: &mut Context<Self>) {
        if let Some(shown) = self.model.read(cx).graphs() {
            self.graphs.edit(&shown, graph);
        }
    }

    /// "Export": the file dialog of the system with the name of the Graph, then the file.
    fn export_graph(&mut self, graph: &str, cx: &mut Context<Self>) {
        let (shown, locale) = {
            let app = self.model.read(cx);
            (app.graphs(), app.locale)
        };
        let Some((name, graph)) = shown.and_then(|shown| GraphsUi::export_target(&shown, graph))
        else {
            return;
        };
        let path = cx.prompt_for_new_path(&home_dir(), Some(&name));
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(path))) = path.await {
                // The write runs off the UI thread: a slow disk does not stop the window.
                let target = path.clone();
                let written = cx
                    .background_executor()
                    .spawn(async move { export_file(&target, &graph) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.graphs.exported(&path, written, locale);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// "Import": the file dialog of the system, then the Graph of the file or the reason of the
    /// refusal.
    fn import_graph(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = paths.await
                && let Some(path) = paths.pop()
            {
                // The Graphs of the user, for the validator and the id of the new Graph.
                let read = this.update(cx, |this, cx| {
                    let app = this.model.read(cx);
                    app.graphs().map(|shown| (shown.graphs(), app.locale))
                });
                let Ok(Some((existing, locale))) = read else {
                    return;
                };
                // The read runs off the UI thread: a slow disk does not stop the window.
                let result = cx
                    .background_executor()
                    .spawn(async move { import_file(&path, &existing) })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    if let Some(command) = this.graphs.imported(result, locale) {
                        this.send_graph(command, cx);
                    }
                    cx.notify();
                });
            }
        })
        .detach();
    }

    /// Send a command of this screen. A refusal that an earlier command left does not show as
    /// the refusal of this one.
    pub(crate) fn send_graph(&mut self, command: Command, cx: &mut Context<Self>) {
        if let Some(shown) = &self.model.read(cx).graph_view {
            let mut shown = lock_view(shown);
            shown.refusal = None;
            // The editor waits for the answer to this Apply, not for an older one.
            if matches!(command, Command::Graph(GraphCommand::Apply(_))) {
                shown.applied = None;
            }
            // The mark of an imported Graph follows the answer to this Import only.
            if matches!(command, Command::Graph(GraphCommand::Import(_))) {
                shown.imported = None;
            }
        }
        self.send(command);
    }
}
