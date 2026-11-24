//! The window of the Profiles screen: the list, the options, the detail of one Profile and
//! the Rule groups with their editor.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Entity, FontWeight, KeyDownEvent, Render, SharedString, Window, div, px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::checkbox::Checkbox;
use gpui_component::input::Input;
use gpui_component::switch::Switch;
use gpui_component::{Disableable as _, Sizable as _, h_flex, v_flex};

use crate::app_settings::HINT_IMMEDIATE_SWITCHING;
use crate::icons::{NcIcon, icon, text_icon};
use crate::model::Rule;
use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::controls::{Pick, act};
use crate::screens::format::plain;
use crate::screens::profiles::{
    COMPARISONS, Dialog, DiffDialog, EditorModel, GroupRow, NoticeKind, PROCESS_MATCHES,
    ProfileDetail, ProfilesModel, ProfilesState, RuleEditorModel, RuleKind, minutes_text,
};
use crate::screens::ui::{
    TagKind, caption, card, card_header, card_row, modal, notice_row, page, tag, write_line,
};
use crate::shell::Page;
use crate::tokens::Palette;

/// The value of a group drag: the position in the list and the label of the ghost.
#[derive(Clone)]
struct DraggedGroup {
    index: usize,
    label: SharedString,
}

struct GroupGhost(SharedString);

impl Render for GroupGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(12.))
            .py(px(6.))
            .rounded(px(9.))
            .bg(gpui::black().opacity(0.7))
            .text_color(gpui::white())
            .child(self.0.clone())
    }
}

impl ScreenView {
    pub(crate) fn render_profiles(
        &mut self,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state(cx);
        self.profiles.sync(&state);
        let model = self.with_input(cx, |input| ProfilesModel::build(input, &self.profiles));
        let locale = self.model.read(cx).locale;
        let view = cx.entity();
        let enabled = model.writes_enabled;
        let tooltip = model.writes_tooltip.clone().unwrap_or_default();

        if model.state == ProfilesState::Loading {
            return page()
                .child(
                    card(p)
                        .p(px(18.))
                        .gap(px(12.))
                        .child(skeleton(p, 200.))
                        .child(skeleton(p, 400.))
                        .child(skeleton(p, 300.)),
                )
                .into_any_element();
        }

        // The list column.
        let mut list = card(p).child(card_header(locale.tr("profiles.list.title"), p).child({
            Button::new("profile-new")
                .small()
                .label(locale.tr("profiles.new"))
                .disabled(!enabled)
                .tooltip(tooltip.clone())
                .on_click(act(&view, |this, _, cx| {
                    let command = this.with_input(cx, |input| this.profiles.new_profile(input));
                    this.send(command);
                }))
        }));
        for (index, row) in model.rows.iter().enumerate() {
            let id = row.id.clone();
            let mut line = h_flex()
                .id(("profile-row", index))
                .w_full()
                .px(px(18.))
                .py(px(8.))
                .gap(px(8.))
                .items_center()
                .border_t_1()
                .border_color(p.border)
                .cursor_pointer()
                .when(row.selected, |line| line.bg(p.selected))
                .hover(|line| line.bg(p.hover))
                .on_click(act(&view, move |this, _, _| {
                    this.profiles.select(&id);
                    this.open_picker = None;
                }))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().font_weight(FontWeight::BOLD).child(row.name.clone()))
                        .child(caption(row.caption.clone(), p)),
                );
            if row.release {
                line = line.child(icon(NcIcon::Warn, p.warning));
            }
            if row.active {
                line = line.child(tag(locale.tr("profiles.tag.active"), TagKind::Accent, p));
            }
            list = list.child(line);
        }
        if let Some(line) = &model.new_write_line {
            list = list.child(card_row(p).child(write_line(line, p)));
        }
        let options = self.options_card(&model, p, window, cx);
        let list_column = v_flex()
            .w(px(292.))
            .flex_none()
            .gap(px(12.))
            .child(list)
            .child(options);

        let mut detail_column = v_flex().flex_1().min_w_0().gap(px(12.));
        if let Some(detail) = &model.detail {
            detail_column = detail_column.child(self.detail_card(detail, enabled, p, window, cx));
        }
        detail_column = detail_column.child(self.groups_card(&model, enabled, p, window, cx));

        let mut root = div().id("profiles-page").relative().w_full().child(
            page().child(
                h_flex()
                    .w_full()
                    .gap(px(12.))
                    .items_start()
                    .child(list_column)
                    .child(detail_column),
            ),
        );
        if let Some(detail) = &model.detail {
            if let Some(dialog) = &detail.slot_dialog {
                root = root.child(self.simple_dialog(
                    "slot",
                    dialog,
                    |this, cx| {
                        let state = this.state(cx);
                        if let Some(command) = this.profiles.confirm_slot(&state) {
                            this.send(command);
                        }
                    },
                    |this| this.profiles.slot_dialog = None,
                    false,
                    p,
                    cx,
                ));
            }
            if let Some(dialog) = &detail.diff_dialog {
                root = root.child(self.diff_dialog(dialog, p, cx));
            }
            if let Some(dialog) = &detail.delete_dialog {
                root = root.child(self.simple_dialog(
                    "delete",
                    dialog,
                    |this, cx| {
                        let state = this.state(cx);
                        if let Some(command) = this.profiles.confirm_delete(&state) {
                            this.send(command);
                        }
                    },
                    |this| this.profiles.delete_dialog = false,
                    true,
                    p,
                    cx,
                ));
            }
        }
        if let Some(dialog) = &model.immediate_dialog {
            root = root.child(self.immediate_dialog(dialog, p, cx));
        }
        root.into_any_element()
    }

    /// A dialog with "Cancel" and a confirm button.
    #[allow(clippy::too_many_arguments)]
    fn simple_dialog(
        &self,
        id: &'static str,
        dialog: &Dialog,
        confirm: impl Fn(&mut ScreenView, &mut Context<ScreenView>) + 'static,
        cancel: impl Fn(&mut ScreenView) + 'static,
        danger: bool,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let confirm_button = {
            let button = Button::new(SharedString::from(format!("{id}-confirm")))
                .label(dialog.confirm.clone());
            let button = if danger {
                button.danger()
            } else {
                button.primary()
            };
            button.on_click({
                let view = view.clone();
                move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        confirm(this, cx);
                        this.profiles.slot_dialog = None;
                        this.profiles.delete_dialog = false;
                        cx.notify();
                    })
                }
            })
        };
        let cancel_button = Button::new(SharedString::from(format!("{id}-cancel")))
            .label(dialog.cancel.clone())
            .on_click(act(&view, move |this, _, _| cancel(this)));
        modal(
            dialog.title.clone(),
            dialog.text.clone(),
            Vec::new(),
            vec![
                cancel_button.into_any_element(),
                confirm_button.into_any_element(),
            ],
            p,
        )
        .into_any_element()
    }

    /// The fields that change on the Leakshield when the Profile is activated.
    fn diff_dialog(&self, dialog: &DiffDialog, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let row = |cells: &[String; 3], head: bool| {
            let line = card_row(p)
                .child(div().flex_1().child(cells[0].clone()))
                .child(div().w(px(100.)).child(cells[1].clone()))
                .child(div().w(px(100.)).child(cells[2].clone()));
            if head {
                line.text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.fg_dim)
            } else {
                line
            }
        };
        let mut table = v_flex().w_full().child(row(&dialog.columns, true));
        for cells in &dialog.rows {
            table = table.child(row(cells, false));
        }
        let cancel = Button::new("diff-cancel")
            .label(dialog.cancel.clone())
            .on_click(act(&view, |this, _, _| this.profiles.diff_dialog = None));
        let confirm = Button::new("diff-confirm")
            .primary()
            .label(dialog.confirm.clone())
            .on_click(act(&view, |this, _, _| {
                if let Some(command) = this.profiles.confirm_activation() {
                    this.send(command);
                }
            }));
        modal(
            dialog.title.clone(),
            dialog.text.clone(),
            vec![table.into_any_element()],
            vec![cancel.into_any_element(), confirm.into_any_element()],
            p,
        )
        .into_any_element()
    }

    fn immediate_dialog(&self, dialog: &Dialog, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let hide = self.profiles.immediate_dialog.unwrap_or(false);
        let checkbox = Checkbox::new("immediate-hide")
            .checked(hide)
            .label(dialog.checkbox.clone().unwrap_or_default())
            .on_click({
                let view = view.clone();
                move |checked, _, cx| {
                    let checked = *checked;
                    view.update(cx, |this, cx| {
                        this.profiles.immediate_dialog = Some(checked);
                        cx.notify();
                    })
                }
            });
        let cancel = Button::new("immediate-cancel")
            .label(dialog.cancel.clone())
            .on_click(act(&view, |this, _, _| {
                this.profiles.immediate_dialog = None
            }));
        let confirm = Button::new("immediate-confirm")
            .primary()
            .label(dialog.confirm.clone())
            .on_click(act(&view, |this, _, cx| {
                if let Some((command, hide)) = this.profiles.confirm_immediate() {
                    if hide {
                        this.model.update(cx, |app, _| {
                            app.settings
                                .update(|values| values.dismiss_hint(HINT_IMMEDIATE_SWITCHING));
                        });
                    }
                    this.send(command);
                }
            }));
        modal(
            dialog.title.clone(),
            dialog.text.clone(),
            vec![checkbox.into_any_element()],
            vec![cancel.into_any_element(), confirm.into_any_element()],
            p,
        )
        .into_any_element()
    }

    fn options_card(
        &mut self,
        model: &ProfilesModel,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.model.read(cx).locale;
        let view = cx.entity();
        let options = &model.options;
        let enabled = model.writes_enabled;
        let timing = enabled && options.timing_enabled;
        let delay = self.input(
            InputKey::RuleDelay,
            &options.rule_delay_s.to_string(),
            "",
            window,
            cx,
        );
        let dwell = self.input(
            InputKey::ProfileDwell,
            &options.dwell_min.to_string(),
            "",
            window,
            cx,
        );
        let dismissed = self
            .model
            .read(cx)
            .settings
            .values
            .hint_dismissed(HINT_IMMEDIATE_SWITCHING);
        let mut card = card(p)
            .child(card_header(options.title.clone(), p))
            .child(
                card_row(p)
                    .child(
                        div()
                            .flex_1()
                            .child(locale.tr("profiles.options.rule_delay")),
                    )
                    .child(div().w(px(72.)).child(Input::new(&delay).disabled(!timing)))
                    .child(caption(locale.tr("profiles.options.seconds"), p)),
            )
            .child(
                card_row(p)
                    .child(div().flex_1().child(locale.tr("profiles.options.dwell")))
                    .child(div().w(px(72.)).child(Input::new(&dwell).disabled(!timing)))
                    .child(caption(locale.tr("profiles.options.minutes"), p)),
            );
        if let Some(text) = &options.timing_caption {
            card = card.child(card_row(p).child(caption(text.clone(), p)));
        }
        card = card.child(
            card_row(p).child(
                Switch::new("profile-immediate")
                    .checked(options.immediate)
                    .label(locale.tr("profiles.options.immediate"))
                    .disabled(!enabled)
                    .on_click({
                        let view = view.clone();
                        move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                let state = this.state(cx);
                                if let Some(command) =
                                    this.profiles.toggle_immediate(&state, dismissed)
                                {
                                    this.send(command);
                                }
                                cx.notify();
                            })
                        }
                    }),
            ),
        );
        for text in &options.help {
            card = card.child(card_row(p).child(caption(text.clone(), p)));
        }
        if let Some(line) = &options.write_line {
            card = card.child(card_row(p).child(write_line(line, p)));
        }
        card.into_any_element()
    }

    fn detail_card(
        &mut self,
        detail: &ProfileDetail,
        enabled: bool,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.model.read(cx).locale;
        let view = cx.entity();
        let mut header = card_header(detail.name.clone(), p);
        if let Some(text) = &detail.tag {
            header = header.child(tag(text.clone(), TagKind::Accent, p));
        }
        header = header
            .child({
                let revoke = detail.revoke;
                Button::new("profile-activate")
                    .small()
                    .label(detail.activate_label.clone())
                    .disabled(!enabled || !detail.activate_enabled)
                    .on_click(act(&view, move |this, _, cx| {
                        let state = this.state(cx);
                        let command = if revoke {
                            Some(crate::command::Command::ReturnToAutomatic)
                        } else {
                            this.profiles.request_activation(&state)
                        };
                        if let Some(command) = command {
                            this.send(command);
                        }
                    }))
            })
            .child(
                Button::new("profile-menu")
                    .small()
                    .ghost()
                    .icon(text_icon(NcIcon::Menu))
                    .on_click(act(&view, |this, _, _| {
                        this.profiles.menu_open = !this.profiles.menu_open;
                    })),
            );
        let mut card = card(p).child(header);
        if self.profiles.menu_open {
            let delete_tooltip = detail.delete_tooltip.clone().unwrap_or_default();
            card = card.child(
                h_flex()
                    .px(px(18.))
                    .py(px(6.))
                    .gap(px(8.))
                    .border_t_1()
                    .border_color(p.border)
                    .child(
                        Button::new("profile-duplicate")
                            .small()
                            .label(detail.duplicate_label.clone())
                            .disabled(!enabled)
                            .on_click(act(&view, |this, _, cx| {
                                let model = this.model.clone();
                                let command =
                                    crate::screen_view::with_screen_input(&model, cx, |input| {
                                        this.profiles.duplicate(input)
                                    });
                                if let Some(command) = command {
                                    this.send(command);
                                }
                            })),
                    )
                    .child(
                        Button::new("profile-rename")
                            .small()
                            .label(detail.rename_label.clone())
                            .disabled(!enabled)
                            .on_click(act(&view, |this, _, cx| {
                                let state = this.state(cx);
                                this.profiles.open_rename(&state);
                            })),
                    )
                    .child(
                        Button::new("profile-delete")
                            .small()
                            .danger()
                            .label(detail.delete_label.clone())
                            .disabled(!detail.delete_enabled)
                            .tooltip(delete_tooltip)
                            .on_click(act(&view, |this, _, _| {
                                this.profiles.menu_open = false;
                                this.profiles.delete_dialog = true;
                            })),
                    ),
            );
        }
        if let Some((_, name)) = self.profiles.rename.clone() {
            card = card.child(self.rename_popover(
                InputKey::ProfileRename,
                &name,
                locale.tr("sensors.rename_placeholder"),
                p,
                window,
                cx,
            ));
        }
        card = card.child(card_row(p).child(caption(detail.caption.clone(), p)));
        if let Some(line) = detail
            .activating
            .as_ref()
            .or(detail.name_write_line.as_ref())
        {
            card = card.child(card_row(p).child(write_line(line, p)));
        }

        // The two halves: Curves and Leakshield settings.
        let mut curves = v_flex().flex_1().gap(px(6.)).child(
            div()
                .font_weight(FontWeight::BOLD)
                .child(locale.tr("profiles.curves.title")),
        );
        if detail.curves.is_empty() {
            curves = curves.child(caption(locale.tr("profiles.curves.none"), p));
        }
        for row in &detail.curves {
            curves = curves.child(
                h_flex()
                    .gap(px(8.))
                    .child(div().flex_1().child(row.channel.clone()))
                    .child(caption(row.summary.clone(), p)),
            );
        }
        curves = curves.child(
            Self::link_button("profile-open-channels", locale.tr("profiles.open"), p)
                .on_click(act(&view, |this, _, cx| this.go(Page::Channels, cx))),
        );

        let slot = &detail.slot;
        let pick: Pick = Rc::new(|this, id, _, cx| {
            let state = this.state(cx);
            if let Some(command) = id
                .parse::<u8>()
                .ok()
                .and_then(|slot| this.profiles.request_slot(&state, slot))
            {
                this.send(command);
            }
        });
        let slot_select = self.select(
            "profile-slot",
            &locale.fmt("profiles.slot.value", &[("slot", &slot.value.to_string())]),
            slot.options
                .iter()
                .map(|s| {
                    (
                        s.to_string(),
                        locale.fmt("profiles.slot.value", &[("slot", &s.to_string())]),
                        *s == slot.value,
                    )
                })
                .collect(),
            slot.enabled,
            "",
            pick,
            p,
            cx,
        );
        let mut leakshield = v_flex()
            .flex_1()
            .gap(px(6.))
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .child(locale.tr("profiles.leakshield.title")),
            )
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(div().w(px(120.)).child(locale.tr("profiles.slot.label")))
                    .child(slot_select),
            );
        if let Some(text) = &slot.shared {
            leakshield = leakshield.child(caption(text.clone(), p));
        }
        if let Some(line) = &slot.write_line {
            leakshield = leakshield.child(write_line(line, p));
        }
        for (label, value) in &detail.leakshield_rows {
            leakshield = leakshield.child(
                h_flex()
                    .gap(px(8.))
                    .child(div().w(px(120.)).child(label.clone()))
                    .child(caption(value.clone(), p)),
            );
        }
        let device = self
            .state(cx)
            .snapshot
            .leakshields
            .first()
            .map(|d| d.id.clone());
        if let Some(device) = device {
            leakshield = leakshield.child(
                Self::link_button("profile-open-leakshield", locale.tr("profiles.open"), p)
                    .on_click(act(&view, move |this, _, cx| {
                        this.go(Page::Leakshield(device.clone()), cx);
                    })),
            );
        }
        card = card.child(
            h_flex()
                .px(px(18.))
                .py(px(12.))
                .gap(px(24.))
                .items_start()
                .border_t_1()
                .border_color(p.border)
                .child(curves)
                .child(leakshield),
        );
        if let Some(text) = &detail.rule_group {
            card = card.child(card_row(p).child(caption(text.clone(), p)));
        }
        for (index, notice) in detail.notices.iter().enumerate() {
            let mut row = notice_row(notice.text.clone(), notice.kind == NoticeKind::Warning, p);
            if let Some((label, commands)) = notice.button.clone() {
                row = row.child(
                    Button::new(("notice-button", index))
                        .small()
                        .label(label)
                        .disabled(!enabled)
                        .on_click(act(&view, move |this, _, _| {
                            for command in commands.clone() {
                                this.send(command);
                            }
                        })),
                );
            }
            card = card.child(row);
        }
        for (index, offer) in detail.restore.iter().enumerate() {
            let mut row = card_row(p)
                .bg(p.alarm_tint)
                .child(div().flex_1().child(offer.text.clone()));
            if let Some(command) = offer.command.clone() {
                row = row.child(
                    Button::new(("profile-restore", index))
                        .small()
                        .danger()
                        .label(offer.button.clone())
                        .disabled(!enabled)
                        .on_click(act(&view, move |this, _, _| this.send(command.clone()))),
                );
            }
            if let Some(line) = &offer.write_line {
                row = row.child(write_line(line, p));
            }
            card = card.child(row);
        }
        card.into_any_element()
    }

    fn groups_card(
        &mut self,
        model: &ProfilesModel,
        enabled: bool,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.model.read(cx).locale;
        let view = cx.entity();
        let groups = &model.groups;
        let mut card = card(p).child(
            card_header(locale.tr("profiles.groups.title"), p).child(
                Button::new("group-add")
                    .small()
                    .label(locale.tr("profiles.groups.add"))
                    .disabled(!enabled)
                    .on_click(act(&view, |this, _, cx| {
                        let state = this.state(cx);
                        this.profiles.add_group(&state);
                    })),
            ),
        );
        card = card.child(card_row(p).child(caption(groups.caption.clone(), p)));
        if let Some(text) = &groups.manual {
            card = card.child(
                card_row(p).child(div().flex_1().child(text.clone())).child(
                    Button::new("groups-revoke")
                        .small()
                        .label(locale.tr("profiles.revoke_short"))
                        .disabled(!enabled)
                        .on_click(act(&view, |this, _, _| {
                            this.send(crate::command::Command::ReturnToAutomatic);
                        })),
                ),
            );
        }
        if let Some(text) = &groups.empty {
            card = card.child(card_row(p).child(text.clone()));
        }
        let count = groups.groups.len();
        for row in &groups.groups {
            card = card.child(self.group_row(row, count, p, cx));
            if row.selected
                && let Some(editor) = &groups.editor
            {
                card = card.child(self.group_editor(editor, enabled, p, window, cx));
            }
        }
        if let Some(editor) = &groups.editor
            && editor.index.is_none()
        {
            card = card.child(self.group_editor(editor, enabled, p, window, cx));
        }
        if let Some(line) = &groups.write_line {
            card = card.child(card_row(p).child(write_line(line, p)));
        }
        let mut root = div().id("groups-card").w_full().child(card);
        if let Some(focus) = self.groups_focus.clone() {
            root = root.track_focus(&focus).on_key_down({
                let view = view.clone();
                move |event: &KeyDownEvent, _, cx| {
                    let keys = &event.keystroke;
                    if !keys.modifiers.alt || !matches!(keys.key.as_str(), "up" | "down") {
                        return;
                    }
                    let up = keys.key == "up";
                    view.update(cx, |this, cx| {
                        let state = this.state(cx);
                        let Some(from) = this.profiles.editor.as_ref().and_then(|e| e.index) else {
                            return;
                        };
                        let to = if up {
                            from.checked_sub(1)
                        } else {
                            Some(from + 1)
                        };
                        if let Some(to) = to
                            && let Some(command) = this.profiles.move_group(&state, from, to)
                        {
                            this.send(command);
                            this.profiles.open_group(&state, to);
                        }
                        cx.notify();
                    });
                }
            });
        }
        root.into_any_element()
    }

    fn group_row(
        &mut self,
        row: &GroupRow,
        count: usize,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let index = row.index;
        let drag = DraggedGroup {
            index,
            label: SharedString::from(format!("{} {}", row.number, row.target)),
        };
        let focus = self.groups_focus.clone();
        let mut rules = v_flex().flex_1().min_w_0().gap(px(2.));
        for sentence in &row.rules {
            rules = rules.child(div().child(sentence.clone()));
        }
        if let Some(text) = &row.warning {
            rules = rules.child(div().text_color(p.warning).child(text.clone()));
        }
        let mut line = h_flex()
            .id(("group-row", index))
            .w_full()
            .px(px(18.))
            .py(px(8.))
            .gap(px(10.))
            .items_start()
            .border_t_1()
            .border_color(p.border)
            .cursor_pointer()
            .when(row.selected, |line| line.bg(p.selected))
            .when(!row.enabled, |line| line.opacity(0.6))
            .hover(|line| line.bg(p.hover))
            .on_click(act(&view, move |this, window, cx| {
                let state = this.state(cx);
                if this.profiles.editor.as_ref().and_then(|e| e.index) == Some(index) {
                    this.profiles.cancel_group();
                } else {
                    this.profiles.open_group(&state, index);
                }
                if let Some(focus) = &focus {
                    window.focus(focus, cx);
                }
            }))
            .on_drag(drag, |dragged, _, _, cx| {
                cx.new(|_| GroupGhost(dragged.label.clone()))
            })
            .on_drop::<DraggedGroup>({
                let view = view.clone();
                move |dragged, _, cx| {
                    let from = dragged.index;
                    view.update(cx, |this, cx| {
                        let state = this.state(cx);
                        if let Some(command) = this.profiles.move_group(&state, from, index) {
                            this.send(command);
                        }
                        cx.notify();
                    });
                }
            })
            .child(icon(NcIcon::Grip, p.fg_dim))
            .child(
                div()
                    .w(px(20.))
                    .font_weight(FontWeight::BOLD)
                    .child(row.number.to_string()),
            )
            .child(rules)
            .child(icon(NcIcon::Right, p.fg_dim))
            .child(
                div()
                    .w(px(130.))
                    .font_weight(FontWeight::BOLD)
                    .child(row.target.clone()),
            );
        if let Some(text) = &row.tag {
            line = line.child(tag(text.clone(), TagKind::Accent, p));
        }
        let _ = count;
        line.into_any_element()
    }

    fn group_editor(
        &mut self,
        editor: &EditorModel,
        enabled: bool,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.model.read(cx).locale;
        let view = cx.entity();
        let pick: Pick = Rc::new(|this, id, _, _| this.profiles.set_group_target(&id));
        let target_shown = editor
            .targets
            .iter()
            .find(|(_, _, selected)| *selected)
            .map_or_else(String::new, |(_, name, _)| name.clone());
        let target = self.select(
            "group-target",
            &target_shown,
            editor.targets.clone(),
            enabled,
            "",
            pick,
            p,
            cx,
        );
        let mut body = v_flex()
            .w_full()
            .px(px(18.))
            .py(px(12.))
            .gap(px(10.))
            .border_t_1()
            .border_color(p.border)
            .bg(p.view)
            .child(
                h_flex()
                    .gap(px(12.))
                    .items_center()
                    .child(div().child(locale.tr("profiles.editor.activate")))
                    .child(target)
                    .child(div().flex_1())
                    .child(
                        Switch::new("group-enabled")
                            .checked(editor.enabled)
                            .label(locale.tr("profiles.editor.enabled"))
                            .on_click({
                                let view = view.clone();
                                move |checked, _, cx| {
                                    let checked = *checked;
                                    view.update(cx, |this, cx| {
                                        this.profiles.set_group_enabled(checked);
                                        cx.notify();
                                    })
                                }
                            }),
                    ),
            );
        for (index, sentence) in editor.rules.iter().enumerate() {
            body = body.child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(div().flex_1().child(sentence.clone()))
                    .child(
                        Self::flat_button(
                            SharedString::from(format!("rule-edit-{index}")),
                            locale.tr("profiles.editor.edit"),
                        )
                        .on_click(act(&view, move |this, _, _| this.profiles.edit_rule(index))),
                    )
                    .child(
                        Self::flat_button(
                            SharedString::from(format!("rule-remove-{index}")),
                            locale.tr("profiles.editor.remove"),
                        )
                        .on_click(act(&view, move |this, _, _| {
                            this.profiles.remove_rule(index)
                        })),
                    ),
            );
        }
        if let Some(rule) = &editor.rule_editor {
            body = body.child(self.rule_editor(rule, enabled, p, window, cx));
        } else {
            body = body.child(
                h_flex().child(
                    Button::new("rule-add")
                        .small()
                        .label(locale.tr("profiles.editor.add_rule"))
                        .disabled(!enabled)
                        .on_click(act(&view, |this, _, cx| {
                            let state = this.state(cx);
                            this.profiles.add_rule(&state);
                        })),
                ),
            );
        }
        if let Some(text) = &editor.warning {
            body = body.child(notice_row(text.clone(), true, p));
        }
        let delete = Button::new("group-delete")
            .small()
            .danger()
            .label(locale.tr("profiles.editor.delete"))
            .disabled(!enabled || !editor.can_delete)
            .on_click(act(&view, |this, _, cx| {
                let state = this.state(cx);
                if let Some(command) = this.profiles.delete_group(&state) {
                    this.send(command);
                }
            }));
        body = body.child(
            h_flex()
                .gap(px(8.))
                .child(delete)
                .child(div().flex_1())
                .child(
                    Button::new("group-cancel")
                        .small()
                        .label(locale.tr("profiles.cancel"))
                        .on_click(act(&view, |this, _, _| this.profiles.cancel_group())),
                )
                .child(
                    Button::new("group-save")
                        .small()
                        .primary()
                        .label(locale.tr("profiles.editor.save"))
                        .disabled(!enabled)
                        .on_click(act(&view, |this, _, cx| {
                            let state = this.state(cx);
                            if let Some(command) = this.profiles.save_group(&state) {
                                this.send(command);
                            }
                        })),
                ),
        );
        body.into_any_element()
    }

    fn rule_editor(
        &mut self,
        rule: &RuleEditorModel,
        enabled: bool,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.model.read(cx).locale;
        let view = cx.entity();
        let state = self.state(cx);
        let kind_pick: Pick = Rc::new(|this, id, _, cx| {
            let state = this.state(cx);
            let kind = match id.as_str() {
                "process" => RuleKind::Process,
                "time" => RuleKind::Time,
                "interval" => RuleKind::Interval,
                _ => RuleKind::Sensor,
            };
            this.profiles.set_rule_kind(&state, kind);
        });
        let kinds = [
            RuleKind::Sensor,
            RuleKind::Process,
            RuleKind::Time,
            RuleKind::Interval,
        ]
        .into_iter()
        .map(|kind| {
            (
                kind.id().to_owned(),
                locale.tr(&format!("profiles.kind.{}", kind.id())),
                kind == rule.kind,
            )
        })
        .collect();
        let kind_select = self.select(
            "rule-kind",
            &locale.tr(&format!("profiles.kind.{}", rule.kind.id())),
            kinds,
            enabled,
            "",
            kind_pick,
            p,
            cx,
        );
        let mut column = v_flex()
            .gap(px(8.))
            .p(px(12.))
            .rounded(px(9.))
            .border_1()
            .border_color(p.border)
            .child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .child(
                        div()
                            .w(px(120.))
                            .child(locale.tr("profiles.editor.condition")),
                    )
                    .child(kind_select),
            );
        for key in rule.keys.clone() {
            column = column.child(self.rule_field(key, &rule.rule, &state, enabled, p, window, cx));
        }
        column = column.child(
            h_flex()
                .gap(px(8.))
                .justify_end()
                .child(
                    Button::new("rule-cancel")
                        .small()
                        .label(locale.tr("profiles.cancel"))
                        .on_click(act(&view, |this, _, _| this.profiles.cancel_rule())),
                )
                .child(
                    Button::new("rule-save")
                        .small()
                        .primary()
                        .label(locale.tr("profiles.editor.save_rule"))
                        .disabled(!rule.complete)
                        .on_click(act(&view, |this, _, _| this.profiles.save_rule())),
                ),
        );
        column.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn rule_field(
        &mut self,
        key: &'static str,
        rule: &Rule,
        state: &crate::model::AppState,
        enabled: bool,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.model.read(cx).locale;
        let view = cx.entity();
        let label = div()
            .w(px(120.))
            .child(locale.tr(&format!("profiles.field.{key}")));
        let text_input =
            |this: &mut Self, value: String, window: &mut Window, cx: &mut Context<Self>| {
                let input = this.input(InputKey::RuleField(key), &value, "", window, cx);
                div()
                    .w(px(220.))
                    .child(Input::new(&input).disabled(!enabled))
                    .into_any_element()
            };
        let control: AnyElement = match (key, rule) {
            ("sensor", Rule::Sensor { sensor, .. }) => {
                let pick: Pick = Rc::new(|this, id, _, _| {
                    if let Some(Rule::Sensor { sensor, .. }) = this.profiles.rule_draft() {
                        *sensor = id;
                    }
                });
                let choices = state
                    .snapshot
                    .sensors
                    .values()
                    .map(|s| (s.id.clone(), s.name.clone(), &s.id == sensor))
                    .collect();
                let shown = state
                    .snapshot
                    .sensors
                    .get(sensor)
                    .map_or_else(|| locale.tr("loops.none"), |s| s.name.clone());
                self.select("rule-sensor", &shown, choices, enabled, "", pick, p, cx)
            }
            ("comparison", Rule::Sensor { comparison, .. }) => {
                let pick: Pick = Rc::new(|this, id, _, _| {
                    if let Some(Rule::Sensor { comparison, .. }) = this.profiles.rule_draft() {
                        *comparison = id;
                    }
                });
                let choices = COMPARISONS
                    .iter()
                    .map(|c| {
                        (
                            (*c).to_owned(),
                            locale.tr(&format!("profiles.cmp_select.{c}")),
                            c == comparison,
                        )
                    })
                    .collect();
                self.select(
                    "rule-comparison",
                    &locale.tr(&format!("profiles.cmp_select.{comparison}")),
                    choices,
                    enabled,
                    "",
                    pick,
                    p,
                    cx,
                )
            }
            ("value", Rule::Sensor { value, .. }) => {
                text_input(self, plain(locale, *value), window, cx)
            }
            ("value_max", Rule::Sensor { value_max, .. }) => {
                text_input(self, plain(locale, *value_max), window, cx)
            }
            ("hold_s", Rule::Sensor { hold_s, .. }) => {
                text_input(self, hold_text(*hold_s), window, cx)
            }
            ("names", Rule::Process { names, .. }) => {
                text_input(self, names.join(", "), window, cx)
            }
            ("match", Rule::Process { match_kind, .. }) => {
                let pick: Pick = Rc::new(|this, id, _, _| {
                    if let Some(Rule::Process { match_kind, .. }) = this.profiles.rule_draft() {
                        *match_kind = id;
                    }
                });
                let choices = PROCESS_MATCHES
                    .iter()
                    .map(|m| {
                        (
                            (*m).to_owned(),
                            locale.tr(&format!("profiles.match.{m}")),
                            m == match_kind,
                        )
                    })
                    .collect();
                self.select(
                    "rule-match",
                    &locale.tr(&format!("profiles.match.{match_kind}")),
                    choices,
                    enabled,
                    "",
                    pick,
                    p,
                    cx,
                )
            }
            ("match_on", Rule::Process { match_on, .. }) => {
                let pick: Pick = Rc::new(|this, id, _, _| {
                    if let Some(Rule::Process { match_on, .. }) = this.profiles.rule_draft() {
                        *match_on = id;
                    }
                });
                let choices = ["name", "cmdline"]
                    .iter()
                    .map(|m| {
                        (
                            (*m).to_owned(),
                            locale.tr(&format!("profiles.match_on.{m}")),
                            *m == match_on,
                        )
                    })
                    .collect();
                self.select(
                    "rule-match-on",
                    &locale.tr(&format!("profiles.match_on.{match_on}")),
                    choices,
                    enabled,
                    "",
                    pick,
                    p,
                    cx,
                )
            }
            ("days", Rule::Time { days, .. }) => {
                let mut row = h_flex().gap(px(4.));
                for day in 1u8..=7 {
                    let on = days.contains(&day);
                    row = row.child(
                        Button::new(SharedString::from(format!("rule-day-{day}")))
                            .small()
                            .when(on, |b| b.primary())
                            .label(locale.tr(&format!("profiles.day.d{day}")))
                            .on_click(act(&view, move |this, _, _| {
                                if let Some(Rule::Time { days, .. }) = this.profiles.rule_draft() {
                                    if let Some(at) = days.iter().position(|d| *d == day) {
                                        days.remove(at);
                                    } else {
                                        days.push(day);
                                        days.sort_unstable();
                                    }
                                }
                            })),
                    );
                }
                row.into_any_element()
            }
            ("start", Rule::Time { start, .. }) => {
                text_input(self, minutes_text(*start), window, cx)
            }
            ("end", Rule::Time { end, .. }) => text_input(self, minutes_text(*end), window, cx),
            ("interval_s", Rule::Interval { interval_s, .. }) => {
                text_input(self, interval_seconds(*interval_s), window, cx)
            }
            ("delay_s", Rule::Interval { delay_s, .. }) => {
                text_input(self, interval_seconds(*delay_s), window, cx)
            }
            ("duration_s", Rule::Interval { duration_s, .. }) => {
                text_input(self, interval_seconds(*duration_s), window, cx)
            }
            _ => div().into_any_element(),
        };
        h_flex()
            .gap(px(8.))
            .items_start()
            .child(label)
            .child(control)
            .into_any_element()
    }
}

fn hold_text(seconds: u32) -> String {
    seconds.to_string()
}

fn interval_seconds(seconds: u32) -> String {
    seconds.to_string()
}

/// Keeps `Entity` in the import list for the drag ghost.
#[allow(dead_code)]
type GhostEntity = Entity<GroupGhost>;
