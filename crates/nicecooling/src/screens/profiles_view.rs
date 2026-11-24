//! The window of the Profiles screen: the list, the options, the detail of one Profile and
//! the Rule groups with their editor.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Div, Entity, FontWeight, KeyDownEvent, Render, SharedString, Window, div,
    px, relative,
};
use gpui_component::button::ButtonVariants as _;
use gpui_component::checkbox::Checkbox;
use gpui_component::switch::Switch;
use gpui_component::{Disableable as _, h_flex, v_flex};

use crate::app_settings::HINT_IMMEDIATE_SWITCHING;
use crate::icons::{NcIcon, icon};
use crate::model::Rule;
use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::controls::{Pick, act};
use crate::screens::format::plain;
use crate::screens::profiles::{
    COMPARISONS, Dialog, DiffDialog, EditorModel, GroupRow, NoticeKind, PROCESS_MATCHES,
    ProfileDetail, ProfilesModel, ProfilesState, RuleEditorModel, RuleKind, minutes_text,
};
use crate::screens::ui::Tip as _;
use crate::screens::ui::{
    TagKind, caption, card, card_header, card_row, heading, label, modal, notice_row, page,
    popover, tag, with_menu, with_popover, write_line,
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

/// A row of a tight list: 30 px, with a line above it.
fn tight_row(p: &Palette) -> Div {
    h_flex()
        .w_full()
        .min_h(px(30.))
        .px(px(18.))
        .py(px(3.))
        .gap(px(12.))
        .border_t_1()
        .border_color(p.border)
}

/// The icon of a rule, by its condition.
fn rule_icon(rule: &Rule) -> NcIcon {
    match rule {
        Rule::Sensor { .. } => NcIcon::Gauge,
        Rule::Process { .. } => NcIcon::App,
        Rule::Time { .. } | Rule::Interval { .. } => NcIcon::Clock,
    }
}

/// A field of the group editor: the label as a caption above the control.
fn editor_field(text: String, width: f32, control: AnyElement, p: &Palette) -> Div {
    v_flex()
        .w(px(width))
        .max_w_full()
        .gap(px(3.))
        .child(caption(text, p))
        .child(control)
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
        let mut list = card(p).child(
            card_header(locale.tr("profiles.list.title"), p).child(
                Self::small_button("profile-new", locale.tr("profiles.new"), Some(NcIcon::Plus))
                    .disabled(!enabled)
                    .tip(tooltip.clone())
                    .on_click(act(&view, |this, _, cx| {
                        let command = this.with_input(cx, |input| this.profiles.new_profile(input));
                        this.send(command);
                    })),
            ),
        );
        for (index, row) in model.rows.iter().enumerate() {
            let id = row.id.clone();
            let mut line = card_row(p)
                .id(("profile-row", index))
                .cursor_pointer()
                .when(row.selected, |line| line.bg(p.accent_tint))
                .when(!row.selected, |line| line.hover(|line| line.bg(p.hover)))
                .on_click(act(&view, move |this, _, _| {
                    this.profiles.select(&id);
                    this.open_picker = None;
                }))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(heading(row.name.clone()).truncate())
                        .child(
                            caption(row.caption.clone(), p)
                                .when(row.selected, |text| text.text_color(p.fg)),
                        ),
                );
            if row.release {
                line = line.child(div().flex_none().child(icon(NcIcon::Warn, p.warning)));
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
            let button = Self::button(
                SharedString::from(format!("{id}-confirm")),
                dialog.confirm.clone(),
            );
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
        let cancel_button = Self::button(
            SharedString::from(format!("{id}-cancel")),
            dialog.cancel.clone(),
        )
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
        let cancel = Self::button("diff-cancel", dialog.cancel.clone())
            .on_click(act(&view, |this, _, _| this.profiles.diff_dialog = None));
        let confirm = Self::button("diff-confirm", dialog.confirm.clone())
            .primary()
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
        let cancel = Self::button("immediate-cancel", dialog.cancel.clone())
            .on_click(act(&view, |this, _, _| {
                this.profiles.immediate_dialog = None
            }));
        let confirm = Self::button("immediate-confirm", dialog.confirm.clone())
            .primary()
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
        let number = |text: String, state, unit: String| {
            tight_row(p).child(label(text).flex_1()).child(
                div()
                    .w(px(86.))
                    .flex_none()
                    .child(Self::field_input(state, Some(unit.into()), true, p).disabled(!timing)),
            )
        };
        let mut card = card(p)
            .child(card_header(options.title.clone(), p))
            .child(number(
                locale.tr("profiles.options.rule_delay"),
                &delay,
                locale.tr("profiles.options.seconds"),
            ))
            .child(number(
                locale.tr("profiles.options.dwell"),
                &dwell,
                locale.tr("profiles.options.minutes"),
            ));
        if let Some(text) = &options.timing_caption {
            card = card.child(tight_row(p).child(caption(text.clone(), p)));
        }
        card = card.child(
            tight_row(p)
                .min_h(px(52.))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(locale.tr("profiles.options.immediate"))
                        .child(caption(locale.tr("profiles.options.immediate_text"), p)),
                )
                .child(
                    Switch::new("profile-immediate")
                        .checked(options.immediate)
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
            card = card.child(tight_row(p).py(px(6.)).child(caption(text.clone(), p)));
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
        // The header: the name with the rename popover, the tag, the activation button and
        // the menu with its popover.
        let rename = self.profiles.rename.clone().map(|(_, name)| {
            self.rename_popover(
                InputKey::ProfileRename,
                &name,
                locale.tr("sensors.rename_placeholder"),
                p,
                window,
                cx,
            )
        });
        let menu = self.profiles.menu_open.then(|| {
            let delete_tooltip = detail.delete_tooltip.clone().unwrap_or_default();
            let item = |id: &'static str, text: String| {
                Self::small_button(id, text, None)
                    .ghost()
                    .w_full()
                    .justify_start()
            };
            popover(p)
                .min_w(px(160.))
                .p(px(4.))
                .child(
                    item("profile-duplicate", detail.duplicate_label.clone())
                        .disabled(!enabled)
                        .on_click(act(&view, |this, _, cx| {
                            this.profiles.menu_open = false;
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
                    item("profile-rename", detail.rename_label.clone())
                        .disabled(!enabled)
                        .on_click(act(&view, |this, _, cx| {
                            this.profiles.menu_open = false;
                            let state = this.state(cx);
                            this.profiles.open_rename(&state);
                        })),
                )
                .child(
                    item("profile-delete", detail.delete_label.clone())
                        .disabled(!detail.delete_enabled)
                        .tip(delete_tooltip)
                        .on_click(act(&view, |this, _, _| {
                            this.profiles.menu_open = false;
                            this.profiles.delete_dialog = true;
                        })),
                )
        });
        let activate = {
            let revoke = detail.revoke;
            let button =
                Self::small_button("profile-activate", detail.activate_label.clone(), None)
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
                    }));
            if revoke { button } else { button.primary() }
        };
        let header = h_flex()
            .min_h(px(40.))
            .pl(px(18.))
            .pr(px(12.))
            .py(px(6.))
            .gap(px(9.))
            .child(with_popover(
                heading(detail.name.clone()).truncate(),
                rename,
                Self::dismiss(&view),
            ))
            .children(detail.tag.clone().map(|text| tag(text, TagKind::Accent, p)))
            .child(div().flex_1())
            .child(activate)
            .child(
                with_menu(
                    Self::icon_button("profile-menu", NcIcon::Menu).on_click(act(
                        &view,
                        |this, _, _| {
                            this.profiles.menu_open = !this.profiles.menu_open;
                        },
                    )),
                    menu.map(IntoElement::into_any_element),
                    Self::dismiss(&view),
                )
                .flex_none(),
            );
        // A tinted last row keeps the round corners of the card.
        let mut card = card(p).overflow_hidden().child(header);
        if let Some(text) = &detail.rule_group {
            card = card.child(card_row(p).child(caption(text.clone(), p)));
        }
        if let Some(line) = detail
            .activating
            .as_ref()
            .or(detail.name_write_line.as_ref())
        {
            card = card.child(card_row(p).child(write_line(line, p)));
        }

        // The two halves: Curves and Leakshield settings.
        let half_title = |text: String, link: Option<AnyElement>| {
            tight_row(p)
                .child(heading(text).flex_1().truncate())
                .children(link)
        };
        // The name keeps its width. A long value wraps in the rest of the row.
        let value_row = |name: String, value: AnyElement| {
            tight_row(p)
                .child(label(name).flex_none().max_w(relative(0.5)))
                .child(div().flex_1().min_w_0().flex().justify_end().child(value))
        };
        let dim = |text: String| {
            div()
                .min_w_0()
                .text_color(p.fg_dim)
                .text_right()
                .child(text)
                .into_any_element()
        };
        let mut curves = v_flex()
            .flex_1()
            .min_w_0()
            .border_r_1()
            .border_color(p.border)
            .child(half_title(
                locale.tr("profiles.curves.title"),
                Some(
                    Self::link_button("profile-open-channels", locale.tr("profiles.open"), p)
                        .on_click(act(&view, |this, _, cx| this.go(Page::Channels, cx)))
                        .into_any_element(),
                ),
            ));
        if detail.curves.is_empty() {
            curves =
                curves.child(tight_row(p).child(caption(locale.tr("profiles.curves.none"), p)));
        }
        for row in &detail.curves {
            curves = curves.child(value_row(row.channel.clone(), dim(row.summary.clone())));
        }

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
        let slot_select = self.select_field(
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
            true,
            p.track,
            pick,
            p,
            cx,
        );
        let device = self
            .state(cx)
            .snapshot
            .leakshields
            .first()
            .map(|d| d.id.clone());
        let mut leakshield = v_flex()
            .flex_1()
            .min_w_0()
            .child(half_title(
                locale.tr("profiles.leakshield.title"),
                device.map(|device| {
                    Self::link_button("profile-open-leakshield", locale.tr("profiles.open"), p)
                        .on_click(act(&view, move |this, _, cx| {
                            this.go(Page::Leakshield(device.clone()), cx);
                        }))
                        .into_any_element()
                }),
            ))
            .child(
                tight_row(p)
                    .py(px(6.))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(locale.tr("profiles.slot.label"))
                            .children(slot.shared.clone().map(|text| caption(text, p))),
                    )
                    .child(div().w(px(110.)).flex_none().child(slot_select)),
            );
        if let Some(line) = &slot.write_line {
            leakshield = leakshield.child(tight_row(p).child(write_line(line, p)));
        }
        let mode = locale.tr("profiles.leakshield.mode");
        for (name, value) in &detail.leakshield_rows {
            let value = if *name == mode {
                tag(value.clone(), TagKind::Neutral, p)
                    .font_weight(FontWeight::BOLD)
                    .into_any_element()
            } else {
                dim(value.clone())
            };
            leakshield = leakshield.child(value_row(name.clone(), value));
        }
        card = card.child(
            div()
                .flex()
                .flex_row()
                .w_full()
                .child(curves)
                .child(leakshield),
        );
        for (index, notice) in detail.notices.iter().enumerate() {
            let mut row = notice_row(notice.text.clone(), notice.kind == NoticeKind::Warning, p);
            if let Some((label, commands)) = notice.button.clone() {
                row = row.child(
                    Self::small_button(
                        SharedString::from(format!("notice-button-{index}")),
                        label,
                        None,
                    )
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
                .child(div().flex_1().min_w_0().child(offer.text.clone()));
            if let Some(command) = offer.command.clone() {
                row = row.child(
                    Self::small_button(
                        SharedString::from(format!("profile-restore-{index}")),
                        offer.button.clone(),
                        None,
                    )
                    .danger()
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
        let mut card = card(p).overflow_hidden().child(
            card_header(locale.tr("profiles.groups.title"), p).child(
                Self::small_button(
                    "group-add",
                    locale.tr("profiles.groups.add"),
                    Some(NcIcon::Plus),
                )
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
                card_row(p)
                    .child(icon(NcIcon::Info, p.fg_dim))
                    .child(div().flex_1().min_w_0().child(text.clone()))
                    .child(
                        Self::small_button(
                            "groups-revoke",
                            locale.tr("profiles.revoke_short"),
                            None,
                        )
                        .disabled(!enabled)
                        .on_click(act(&view, |this, _, _| {
                            this.send(crate::command::Command::ReturnToAutomatic);
                        })),
                    ),
            );
        }
        if let Some(text) = &groups.empty {
            card = card.child(card_row(p).child(div().flex_1().min_w_0().child(text.clone())));
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
        // One line for each rule: the icon of its condition and its sentence.
        let kinds: Vec<NcIcon> = self
            .state(cx)
            .snapshot
            .rule_groups
            .get(index)
            .map(|group| group.rules.iter().map(rule_icon).collect())
            .unwrap_or_default();
        let mut rules = v_flex().flex_1().min_w_0().gap(px(2.));
        for (line, sentence) in row.rules.iter().enumerate() {
            rules = rules.child(
                h_flex()
                    .gap(px(12.))
                    .items_start()
                    .children(
                        kinds
                            .get(line)
                            .map(|kind| div().flex_none().pt(px(2.)).child(icon(*kind, p.fg))),
                    )
                    .child(div().flex_1().min_w_0().child(sentence.clone())),
            );
        }
        if let Some(text) = &row.warning {
            rules = rules.child(
                h_flex()
                    .gap(px(12.))
                    .items_start()
                    .child(
                        div()
                            .flex_none()
                            .pt(px(2.))
                            .child(icon(NcIcon::Warn, p.warning)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(p.warning)
                            .child(text.clone()),
                    ),
            );
        }
        let mut line = card_row(p)
            .id(("group-row", index))
            .min_h(px(46.))
            .cursor_pointer()
            .when(row.selected, |line| line.bg(p.accent_tint))
            .when(!row.selected, |line| line.hover(|line| line.bg(p.hover)))
            .when(!row.enabled, |line| line.opacity(0.6))
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
            .child(div().flex_none().child(icon(NcIcon::Grip, p.fg_dim)))
            .child(
                div()
                    .w(px(18.))
                    .flex_none()
                    .text_right()
                    .text_color(p.fg_dim)
                    .child(row.number.to_string()),
            )
            .child(rules)
            .child(div().flex_none().child(icon(NcIcon::Right, p.fg_dim)))
            .child(heading(row.target.clone()).max_w(px(180.)).truncate());
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
        let target = self.select_field(
            "group-target",
            &target_shown,
            editor.targets.clone(),
            enabled,
            false,
            p.card,
            pick,
            p,
            cx,
        );
        let rule_open = editor.rule_editor.is_some();
        // The rules of the open group, for their icons.
        let kinds: Vec<NcIcon> = self
            .profiles
            .editor
            .as_ref()
            .map(|open| open.group.rules.iter().map(rule_icon).collect())
            .unwrap_or_default();
        let mut body = v_flex()
            .w_full()
            .px(px(18.))
            .pt(px(9.))
            .pb(px(12.))
            .gap(px(9.))
            .border_t_1()
            .border_color(p.border)
            .bg(p.hover)
            .child(
                h_flex()
                    .gap(px(12.))
                    .items_end()
                    .child(editor_field(
                        locale.tr("profiles.editor.activate"),
                        260.,
                        target,
                        p,
                    ))
                    .child(div().flex_1())
                    .child(
                        h_flex().h(px(34.)).flex_none().child(
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
                    ),
            );
        for (index, sentence) in editor.rules.iter().enumerate() {
            body = body.child(
                h_flex()
                    .gap(px(9.))
                    .items_center()
                    .children(
                        kinds
                            .get(index)
                            .map(|kind| div().flex_none().child(icon(*kind, p.fg))),
                    )
                    .child(div().flex_1().min_w_0().child(sentence.clone()))
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
                    Self::small_button(
                        "rule-add",
                        locale.tr("profiles.editor.add_rule"),
                        Some(NcIcon::Plus),
                    )
                    .disabled(!enabled)
                    .on_click(act(&view, |this, _, cx| {
                        let state = this.state(cx);
                        this.profiles.add_rule(&state);
                    })),
                ),
            );
        }
        if let Some(text) = &editor.warning {
            body = body.child(
                h_flex()
                    .gap(px(9.))
                    .items_start()
                    .child(
                        div()
                            .flex_none()
                            .pt(px(2.))
                            .child(icon(NcIcon::Warn, p.warning)),
                    )
                    .child(div().flex_1().min_w_0().child(text.clone())),
            );
        }
        let delete = Self::small_button(
            "group-delete",
            locale.tr("profiles.editor.delete"),
            Some(NcIcon::Trash),
        )
        .ghost()
        .disabled(!enabled || !editor.can_delete)
        .on_click(act(&view, |this, _, cx| {
            let state = this.state(cx);
            if let Some(command) = this.profiles.delete_group(&state) {
                this.send(command);
            }
        }));
        // One suggested button: "Save rule" while a rule is open, "Save group" else.
        let save = Self::small_button("group-save", locale.tr("profiles.editor.save"), None)
            .disabled(!enabled)
            .on_click(act(&view, |this, _, cx| {
                let state = this.state(cx);
                if let Some(command) = this.profiles.save_group(&state) {
                    this.send(command);
                }
            }));
        body = body.child(
            h_flex()
                .gap(px(6.))
                .flex_wrap()
                .child(delete)
                .child(div().flex_1())
                .child(
                    Self::small_button("group-cancel", locale.tr("profiles.cancel"), None)
                        .on_click(act(&view, |this, _, _| this.profiles.cancel_group())),
                )
                .child(if rule_open { save } else { save.primary() }),
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
        let kind_select = self.select_field(
            "rule-kind",
            &locale.tr(&format!("profiles.kind.{}", rule.kind.id())),
            kinds,
            enabled,
            false,
            p.card,
            kind_pick,
            p,
            cx,
        );
        // The fields of the condition, in one row that wraps.
        let mut fields = h_flex()
            .w_full()
            .gap(px(9.))
            .flex_wrap()
            .items_end()
            .child(editor_field(
                locale.tr("profiles.editor.condition"),
                200.,
                kind_select,
                p,
            ));
        for key in rule.keys.clone() {
            fields = fields.child(self.rule_field(key, &rule.rule, &state, enabled, p, window, cx));
        }
        v_flex()
            .gap(px(9.))
            .pt(px(9.))
            .border_t_1()
            .border_color(p.border)
            .child(fields)
            .child(
                h_flex()
                    .gap(px(6.))
                    .justify_end()
                    .child(
                        Self::small_button("rule-cancel", locale.tr("profiles.cancel"), None)
                            .on_click(act(&view, |this, _, _| this.profiles.cancel_rule())),
                    )
                    .child(
                        Self::small_button(
                            "rule-save",
                            locale.tr("profiles.editor.save_rule"),
                            None,
                        )
                        .primary()
                        .disabled(!rule.complete)
                        .on_click(act(&view, |this, _, _| this.profiles.save_rule())),
                    ),
            )
            .into_any_element()
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
        let title = locale.tr(&format!("profiles.field.{key}"));
        let width = match key {
            "sensor" | "names" => 260.,
            "comparison" | "match" | "match_on" => 150.,
            "days" => 330.,
            _ => 110.,
        };
        let text_input =
            |this: &mut Self, value: String, window: &mut Window, cx: &mut Context<Self>| {
                let input = this.input(InputKey::RuleField(key), &value, "", window, cx);
                Self::field_input(&input, None, false, p)
                    .bg(p.card)
                    .border_color(p.card)
                    .disabled(!enabled)
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
                self.select_field(
                    "rule-sensor",
                    &shown,
                    choices,
                    enabled,
                    false,
                    p.card,
                    pick,
                    p,
                    cx,
                )
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
                self.select_field(
                    "rule-comparison",
                    &locale.tr(&format!("profiles.cmp_select.{comparison}")),
                    choices,
                    enabled,
                    false,
                    p.card,
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
                self.select_field(
                    "rule-match",
                    &locale.tr(&format!("profiles.match.{match_kind}")),
                    choices,
                    enabled,
                    false,
                    p.card,
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
                self.select_field(
                    "rule-match-on",
                    &locale.tr(&format!("profiles.match_on.{match_on}")),
                    choices,
                    enabled,
                    false,
                    p.card,
                    pick,
                    p,
                    cx,
                )
            }
            ("days", Rule::Time { days, .. }) => {
                let mut row = h_flex().h(px(34.)).gap(px(3.));
                for day in 1u8..=7 {
                    let on = days.contains(&day);
                    row = row.child(
                        Self::small_button(
                            SharedString::from(format!("rule-day-{day}")),
                            locale.tr(&format!("profiles.day.d{day}")),
                            None,
                        )
                        .when(on, |b| b.primary())
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
        editor_field(title, width, control, p).into_any_element()
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
