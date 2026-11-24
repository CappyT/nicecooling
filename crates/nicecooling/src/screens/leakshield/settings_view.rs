//! The window of the Settings tab: the stepper, the step card and the pending changes.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, px};
use gpui_component::button::ButtonVariants as _;
use gpui_component::switch::Switch;
use gpui_component::{Disableable as _, Sizable as _, h_flex, v_flex};

use crate::icons::{NcIcon, icon, small_icon, text_icon};
use crate::model::Severity;
use crate::screen_view::{InputKey, ScreenView};
use crate::screens::controls::{Pick, act};
use crate::screens::leakshield::TabState;
use crate::screens::leakshield::band::band_diagram;
use crate::screens::leakshield::parts::{field_badge, header, loading_card, radio_mark};
use crate::screens::leakshield::settings::{Control, Link, Row, SettingsModel, Summary};
use crate::screens::ui::{
    TagKind, caption, card, card_header, card_row, cell, field_row, heading, locked, modal,
    notice_row, page, severity_icon, table_header, table_row, tag, wide_cell, write_line,
};
use crate::shell::Page;
use crate::tokens::Palette;

impl ScreenView {
    pub(crate) fn render_ls_settings(
        &mut self,
        id: &str,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut ui = std::mem::take(&mut self.leakshield.settings);
        self.with_input(cx, |input| ui.reconcile(input, id));
        self.leakshield.settings = ui;
        let model = self.with_input(cx, |input| {
            SettingsModel::build(input, &self.leakshield.settings, id)
        });
        let view = cx.entity();
        let mut content = page();
        if model.state == TabState::Loading {
            return content
                .child(loading_card(model.loading_text.clone(), p))
                .into_any_element();
        }
        let ready = model.state == TabState::Ready;

        // The stepper: a numbered mark and the name of each step, with a line between two
        // steps. A step that does not fit goes to the next line.
        let mut stepper = h_flex()
            .w_full()
            .flex_wrap()
            .gap(px(6.))
            .px(px(18.))
            .py(px(12.));
        for (index, item) in model.stepper.iter().enumerate() {
            let step = item.step;
            let view = view.clone();
            if index > 0 {
                stepper = stepper.child(div().flex_1().min_w(px(6.)).h(px(1.)).bg(p.border));
            }
            let mark = div()
                .size(px(22.))
                .flex_none()
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.))
                .font_weight(FontWeight::BOLD)
                .map(|mark| {
                    if item.current {
                        mark.bg(p.accent_bg)
                            .text_color(p.accent_fg)
                            .child((index + 1).to_string())
                    } else if item.done {
                        mark.bg(p.accent_tint)
                            .child(small_icon(NcIcon::Check, p.accent))
                    } else {
                        mark.bg(p.track).child((index + 1).to_string())
                    }
                });
            stepper = stepper.child(
                h_flex()
                    .id(SharedString::from(format!("ls-step-{step:?}")))
                    .flex_none()
                    .gap(px(6.))
                    .cursor_pointer()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(if item.current { p.fg } else { p.fg_dim })
                    .hover(|step| step.text_color(p.fg))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.leakshield.settings.go(step);
                            cx.notify();
                        });
                    })
                    .child(mark)
                    .child(item.label.clone()),
            );
        }
        content = content.child(card(p).child(stepper));

        // The step card.
        let mut step_card = card(p).child(header(model.title.clone(), model.counter.clone(), p));
        if model.unknown_firmware {
            step_card = step_card.child(notice_row(
                self.model.read(cx).locale.tr("leakshield.fields.unknown"),
                true,
                p,
            ));
        }
        for row in &model.rows {
            step_card = step_card.child(self.setting_row(id, row, ready, p, window, cx));
        }
        if let Some(summary) = &model.summary {
            step_card = step_card.child(self.summary_card(id, summary, ready, p, cx));
        }
        let footer = {
            let mut footer = h_flex()
                .w_full()
                .px(px(18.))
                .py(px(12.))
                .gap(px(6.))
                .border_t_1()
                .border_color(p.border);
            footer = footer.child(
                Self::button("ls-wizard-cancel", model.cancel.clone())
                    .ghost()
                    .on_click(act(&view, |this, _, _| {
                        this.leakshield.settings.request_discard()
                    })),
            );
            footer = footer.child(div().flex_1());
            if let Some((step, label)) = &model.back {
                let step = *step;
                footer = footer.child(
                    Self::button("ls-wizard-back", label.clone())
                        .icon(text_icon(NcIcon::Left))
                        .on_click(act(&view, move |this, _, _| {
                            this.leakshield.settings.go(step)
                        })),
                );
            }
            if let Some((step, label)) = &model.next {
                let step = *step;
                footer = footer.child(
                    Self::button("ls-wizard-next", label.clone())
                        .primary()
                        .child(text_icon(NcIcon::Right))
                        .on_click(act(&view, move |this, _, _| {
                            this.leakshield.settings.go(step)
                        })),
                );
            }
            footer
        };
        step_card = step_card.child(footer);
        content = content.child(step_card);

        // The pending changes: the name, then the old value struck through and the new one.
        let mut pending_header = card_header(model.pending_title.clone(), p);
        if !model.pending.is_empty() {
            pending_header = h_flex()
                .min_h(px(40.))
                .px(px(18.))
                .py(px(6.))
                .gap(px(9.))
                .child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .child(model.pending_title.clone()),
                )
                .child(tag(model.pending.len().to_string(), TagKind::Accent, p));
        }
        let mut pending = card(p).child(pending_header);
        let line = || {
            v_flex()
                .w_full()
                .px(px(18.))
                .py(px(9.))
                .gap(px(2.))
                .border_t_1()
                .border_color(p.border)
        };
        if let Some(text) = &model.no_changes {
            pending = pending.child(line().text_color(p.fg_dim).child(text.clone()));
        }
        for change in &model.pending {
            pending = pending.child(
                line()
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .child(change.name.clone()),
                    )
                    .child(
                        h_flex()
                            .gap(px(4.))
                            .child(
                                div()
                                    .line_through()
                                    .text_color(p.fg_dim)
                                    .child(change.old.clone()),
                            )
                            .child("->")
                            .child(change.new.clone()),
                    ),
            );
        }
        let mut steps = v_flex().gap(px(3.)).pl(px(6.));
        for (index, step) in model.pending_steps.iter().enumerate() {
            steps = steps.child(caption(format!("{}. {step}", index + 1), p));
        }
        pending = pending.child(
            line()
                .gap(px(9.))
                .pb(px(12.))
                .child(caption(model.pending_note.clone(), p))
                .child(steps),
        );
        content = content.child(pending);

        let mut root = div().id("ls-settings").relative().w_full().child(content);
        {
            let view = view.clone();
            root = root.on_key_down(move |event, _, cx| {
                if event.keystroke.modifiers.alt {
                    let delta = match event.keystroke.key.as_str() {
                        "right" => 1,
                        "left" => -1,
                        _ => return,
                    };
                    view.update(cx, |this, cx| {
                        this.leakshield.settings.step_by(delta);
                        cx.notify();
                    });
                }
            });
        }
        if let Some((title, keep, discard)) = &model.discard {
            let (keep_view, discard_view) = (view.clone(), view.clone());
            root = root.child(modal(
                title.clone(),
                String::new(),
                Vec::new(),
                vec![
                    Self::small_button("ls-discard-keep", keep.clone(), None)
                        .on_click(act(&keep_view, |this, _, _| {
                            this.leakshield.settings.keep_editing()
                        }))
                        .into_any_element(),
                    Self::small_button("ls-discard-confirm", discard.clone(), None)
                        .danger()
                        .on_click(act(&discard_view, |this, _, _| {
                            this.leakshield.settings.discard()
                        }))
                        .into_any_element(),
                ],
                p,
            ));
        }
        root.into_any_element()
    }

    fn setting_row(
        &mut self,
        id: &str,
        row: &Row,
        ready: bool,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        if let Control::Band(diagram) = &row.control {
            return v_flex()
                .debug_selector(|| "ls-row-band".to_owned())
                .w_full()
                .px(px(18.))
                .py(px(12.))
                .gap(px(9.))
                .border_t_1()
                .border_color(p.border)
                .child(heading(row.label.clone()))
                .child(band_diagram(diagram, p))
                .children(row.caption.clone().map(|text| caption(text, p)))
                .into_any_element();
        }
        let enabled = row.writable && ready && !row.fixed;
        let key = row.key.clone();
        let device = id.to_owned();
        let control: AnyElement = match &row.control {
            Control::Select { shown, options } => {
                let pick: Pick = {
                    let (device, key) = (device.clone(), key.clone());
                    Rc::new(move |this, choice, _, cx| {
                        this.settings_edit(&device, &key, &choice, cx)
                    })
                };
                let select = self.select_field(
                    &format!("ls-setting-{key}"),
                    shown,
                    options.clone(),
                    enabled,
                    false,
                    p.track,
                    pick,
                    p,
                    cx,
                );
                div()
                    .w(px(240.))
                    .flex_none()
                    .child(select)
                    .into_any_element()
            }
            Control::Number { text, unit } => {
                let input = self.input(
                    InputKey::Ls(format!("settings:{key}")),
                    text,
                    "",
                    window,
                    cx,
                );
                div()
                    .w(px(124.))
                    .flex_none()
                    .child(Self::mode_input(
                        &input,
                        unit,
                        enabled,
                        row.error.is_some(),
                        false,
                        p,
                    ))
                    .into_any_element()
            }
            Control::Time { text } => {
                let input = self.input(
                    InputKey::Ls(format!("settings:{key}")),
                    text,
                    "HH:MM",
                    window,
                    cx,
                );
                div()
                    .w(px(78.))
                    .flex_none()
                    .child(Self::mode_input(
                        &input,
                        "",
                        enabled,
                        row.error.is_some(),
                        false,
                        p,
                    ))
                    .into_any_element()
            }
            Control::Switch(on) => {
                let (view, device, key) = (view.clone(), device.clone(), key.clone());
                Switch::new(SharedString::from(format!("ls-setting-{key}")))
                    .large()
                    .checked(*on)
                    .disabled(!enabled)
                    .on_click(move |on, _, cx| {
                        let value = if *on { "1" } else { "0" };
                        let (device, key) = (device.clone(), key.clone());
                        view.update(cx, |this, cx| this.settings_edit(&device, &key, value, cx));
                    })
                    .into_any_element()
            }
            Control::Info(text) => div()
                .max_w(px(420.))
                .min_w_0()
                .text_color(p.fg_dim)
                .child(text.clone())
                .into_any_element(),
            // The choices are radio rows under the name of the setting.
            Control::Radio(_) | Control::Band(_) => div().into_any_element(),
        };
        // A text shrinks and wraps. Every other control keeps its width.
        let control = {
            let name = format!("ls-control-{key}");
            div()
                .map(|control| match &row.control {
                    Control::Info(_) => control.min_w_0(),
                    _ => control.flex_none(),
                })
                .debug_selector(move || name)
                .child(control)
        };
        let mut title = v_flex().flex_1().min_w_0().gap(px(3.)).child(
            h_flex()
                .gap(px(6.))
                .child(
                    div()
                        .min_w_0()
                        .font_weight(FontWeight::BOLD)
                        .child(row.label.clone()),
                )
                .when(!row.badge.is_empty(), |t| {
                    t.child(field_badge(
                        row.badge.clone(),
                        row.writable && !row.fixed,
                        p,
                    ))
                }),
        );
        if let Some(text) = &row.caption {
            title = title.child(caption(text.clone(), p));
        }
        if let Some(reason) = &row.reason {
            let mut line = h_flex()
                .flex_wrap()
                .gap(px(4.))
                .child(caption(reason.clone(), p));
            if let Some((label, Link::Fields(field))) = &row.link {
                let field = field.clone();
                line = line.child(
                    Self::link_button(
                        SharedString::from(format!("ls-link-{}", row.key)),
                        label.clone(),
                        p,
                    )
                    .text_size(px(12.))
                    .on_click(act(&view, move |this, _, _| {
                        this.leakshield.open_fields(Some(&field))
                    })),
                );
            }
            title = title.child(line);
        }
        if let Some(error) = &row.error {
            title = title.child(caption(error.clone(), p).text_color(p.alarm));
        }
        if let Some(note) = &row.note {
            let (name, color) = if note.warning {
                (NcIcon::Warn, p.warning)
            } else {
                (NcIcon::Info, p.accent)
            };
            title = title.child(
                h_flex()
                    .items_start()
                    .gap(px(4.))
                    .child(small_icon(name, color).mt(px(2.)))
                    .child(caption(note.text.clone(), p).text_color(p.fg)),
            );
        }
        let row_name = format!("ls-row-{key}");
        let mut line = field_row(p)
            .debug_selector(move || row_name)
            .when(row.dim, |l| l.opacity(0.5))
            .when(row.changed, |l| l.bg(p.accent_tint))
            .child(title)
            .child(control);
        if let Some(label) = &row.use_current {
            let (device, key) = (device.clone(), row.key.clone());
            line = line.child(
                Self::small_button(
                    SharedString::from(format!("ls-use-{key}")),
                    label.clone(),
                    None,
                )
                .disabled(!enabled)
                .debug_selector({
                    let name = format!("ls-use-{}", row.key);
                    move || name
                })
                .on_click(act(&view, move |this, _, cx| {
                    let mut ui = std::mem::take(&mut this.leakshield.settings);
                    this.with_input(cx, |input| ui.use_current(input, &device, &key));
                    this.leakshield.settings = ui;
                })),
            );
        }
        // The link to the Fields tab is after the reason. Without a reason it is in the row.
        if let Some((label, target)) = row
            .link
            .as_ref()
            .filter(|(_, target)| row.reason.is_none() || !matches!(target, Link::Fields(_)))
        {
            let target = target.clone();
            line = line.child(
                Self::link_button(
                    SharedString::from(format!("ls-link-{}", row.key)),
                    label.clone(),
                    p,
                )
                .debug_selector({
                    let name = format!("ls-link-{}", row.key);
                    move || name
                })
                .on_click(act(&view, move |this, _, cx| match &target {
                    Link::Loops => this.go(Page::Loops, cx),
                    Link::Fields(field) => this.leakshield.open_fields(Some(field)),
                })),
            );
        }
        let Control::Radio(options) = &row.control else {
            return line.into_any_element();
        };
        let mut column = v_flex().w_full().child(line);
        for (mode, text, selected) in options {
            let (view, device, key, mode) =
                (view.clone(), device.clone(), row.key.clone(), mode.clone());
            column = column.child(
                h_flex()
                    .id(SharedString::from(format!("ls-radio-{mode}")))
                    .w_full()
                    .min_h(px(40.))
                    .px(px(18.))
                    .py(px(6.))
                    .gap(px(12.))
                    .border_t_1()
                    .border_color(p.border)
                    .when(*selected, |option| option.bg(p.accent_tint))
                    .when(enabled, |option| {
                        option
                            .cursor_pointer()
                            .when(!*selected, |option| option.hover(|o| o.bg(p.hover)))
                            .on_click(move |_, _, cx| {
                                let (device, key, mode) =
                                    (device.clone(), key.clone(), mode.clone());
                                view.update(cx, |this, cx| {
                                    this.settings_edit(&device, &key, &mode, cx);
                                    cx.notify();
                                });
                            })
                    })
                    .when(!enabled, |option| option.opacity(0.5))
                    .child(radio_mark(*selected, p))
                    .child(div().flex_1().min_w_0().child(text.clone())),
            );
        }
        column.into_any_element()
    }

    fn summary_card(
        &mut self,
        id: &str,
        summary: &Summary,
        ready: bool,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let mut column = v_flex().w_full();
        for text in &summary.warnings {
            column = column.child(notice_row(text.clone(), true, p));
        }
        for text in &summary.errors {
            column = column.child(
                card_row(p)
                    .child(severity_icon(Severity::Alarm, p))
                    .child(text.clone()),
            );
        }
        if let Some(text) = &summary.nothing {
            column = column.child(card_row(p).child(text.clone()));
        } else {
            let locale = self.model.read(cx).locale;
            column = column.child(table_header(
                &["setting", "now", "new", "note"]
                    .iter()
                    .map(|name| locale.tr(&format!("leakshield.settings.column.{name}")))
                    .collect::<Vec<_>>(),
                &[None, Some(140.), Some(140.), Some(200.)],
                p,
            ));
        }
        for row in &summary.rows {
            let name = if row.padlock {
                locked(row.name.clone(), p)
            } else {
                div().child(row.name.clone())
            };
            column = column.child(
                table_row(p)
                    .child(wide_cell().child(name))
                    .child(cell(140.).text_color(p.fg_dim).child(row.now.clone()))
                    .child(
                        cell(140.)
                            .font_weight(FontWeight::BOLD)
                            .child(row.new.clone()),
                    )
                    .child(
                        div().w(px(200.)).flex_none().min_w_0().child(
                            row.note
                                .clone()
                                .or_else(|| row.status.clone())
                                .unwrap_or_default(),
                        ),
                    ),
            );
        }
        if let Some(text) = &summary.success {
            column = column.child(
                card_row(p)
                    .child(icon(NcIcon::Check, p.success))
                    .child(text.clone()),
            );
        }
        if let Some(failed) = &summary.failed {
            let mut row = card_row(p)
                .bg(p.alarm_tint)
                .child(div().flex_1().child(failed.text.clone()));
            if let Some(command) = failed.restore_command.clone() {
                row = row.child(
                    Self::small_button("ls-wizard-restore", failed.restore.clone(), None)
                        .danger()
                        .disabled(!ready)
                        .on_click(act(&view, move |this, _, _| this.send(command.clone()))),
                );
            }
            row = row.child(
                Self::small_button("ls-wizard-keep", failed.keep.clone(), None)
                    .on_click(act(&view, |this, _, _| {
                        this.leakshield.settings.kept = true
                    })),
            );
            column = column.child(row);
        }
        let device = id.to_owned();
        column = column.child(
            card_row(p).child(div().flex_1()).child(
                Self::button("ls-wizard-apply", summary.apply.clone())
                    .primary()
                    .disabled(!summary.apply_enabled)
                    .on_click(act(&view, move |this, _, cx| {
                        let mut ui = std::mem::take(&mut this.leakshield.settings);
                        let command = this.with_input(cx, |input| ui.apply(input, &device));
                        this.leakshield.settings = ui;
                        if let Some(command) = command {
                            this.send(command);
                        }
                    })),
            ),
        );
        if let Some(line) = self.with_input(cx, |input| {
            crate::screens::leakshield::write_line(input, id, "wizard")
        }) {
            column = column.child(card_row(p).child(write_line(&line, p)));
        }
        column.into_any_element()
    }

    /// An edit of a setting: the change goes into the pending changes.
    pub(crate) fn settings_edit(
        &mut self,
        id: &str,
        key: &str,
        text: &str,
        cx: &mut Context<Self>,
    ) {
        let state = self.state(cx);
        if let Some(data) = state.snapshot.leakshield_data.get(id) {
            self.leakshield.settings.edit(data, key, text);
        }
    }
}
