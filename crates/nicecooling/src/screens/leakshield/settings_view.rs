//! The window of the Settings tab: the stepper, the step card and the pending changes.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, px};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::switch::Switch;
use gpui_component::{Disableable as _, Sizable as _, h_flex, v_flex};

use crate::model::Severity;
use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::controls::{Pick, act};
use crate::screens::leakshield::TabState;
use crate::screens::leakshield::sensors_view::header_row;
use crate::screens::leakshield::settings::{Control, Link, Row, SettingsModel, Summary};
use crate::screens::ui::{
    caption, card, card_header, card_row, modal, notice_row, page, severity_icon, write_line,
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
                .child(
                    card(p)
                        .p(px(18.))
                        .gap(px(12.))
                        .child(div().child(model.loading_text.clone()))
                        .child(skeleton(p, 320.)),
                )
                .into_any_element();
        }
        let ready = model.state == TabState::Ready;

        // The stepper.
        let mut stepper = h_flex().gap(px(6.)).p(px(12.));
        for item in &model.stepper {
            let step = item.step;
            let view = view.clone();
            stepper = stepper.child(
                div()
                    .id(SharedString::from(format!("ls-step-{step:?}")))
                    .px(px(10.))
                    .h(px(30.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .when(item.current, |s| {
                        s.bg(p.accent_tint)
                            .text_color(p.accent)
                            .font_weight(FontWeight::BOLD)
                    })
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.leakshield.settings.go(step);
                            cx.notify();
                        });
                    })
                    .when(item.done, |s| {
                        s.child(div().text_color(p.success).child("\u{2713}"))
                    })
                    .child(item.label.clone()),
            );
        }
        content = content.child(card(p).child(stepper));

        // The step card.
        let mut step_card = card(p)
            .child(card_header(model.title.clone(), p).child(caption(model.counter.clone(), p)));
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
            let mut footer = h_flex().w_full().px(px(18.)).py(px(10.)).gap(px(8.));
            footer = footer.child(
                Button::new("ls-wizard-cancel")
                    .small()
                    .label(model.cancel.clone())
                    .on_click(act(&view, |this, _, _| {
                        this.leakshield.settings.request_discard()
                    })),
            );
            footer = footer.child(div().flex_1());
            if let Some((step, label)) = &model.back {
                let step = *step;
                footer = footer.child(
                    Button::new("ls-wizard-back")
                        .small()
                        .label(format!("\u{2190} {label}"))
                        .on_click(act(&view, move |this, _, _| {
                            this.leakshield.settings.go(step)
                        })),
                );
            }
            if let Some((step, label)) = &model.next {
                let step = *step;
                footer = footer.child(
                    Button::new("ls-wizard-next")
                        .small()
                        .primary()
                        .label(format!("{label} \u{2192}"))
                        .on_click(act(&view, move |this, _, _| {
                            this.leakshield.settings.go(step)
                        })),
                );
            }
            footer
        };
        step_card = step_card.child(footer);
        content = content.child(step_card);

        // The pending changes.
        let mut pending = card(p).child(card_header(model.pending_title.clone(), p));
        if let Some(text) = &model.no_changes {
            pending = pending.child(card_row(p).child(text.clone()));
        }
        for line in &model.pending {
            pending = pending.child(card_row(p).child(line.clone()));
        }
        pending = pending.child(card_row(p).child(caption(model.pending_note.clone(), p)));
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
                    Button::new("ls-discard-keep")
                        .small()
                        .label(keep.clone())
                        .on_click(act(&keep_view, |this, _, _| {
                            this.leakshield.settings.keep_editing()
                        }))
                        .into_any_element(),
                    Button::new("ls-discard-confirm")
                        .small()
                        .danger()
                        .label(discard.clone())
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
        let enabled = row.writable && ready;
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
                self.select(
                    &format!("ls-setting-{key}"),
                    shown,
                    options.clone(),
                    enabled,
                    "",
                    pick,
                    p,
                    cx,
                )
            }
            Control::Number { text, unit } => {
                let input = self.input(
                    InputKey::Ls(format!("settings:{key}")),
                    text,
                    "",
                    window,
                    cx,
                );
                let mut field = h_flex().gap(px(6.)).items_center().child(
                    div()
                        .w(px(110.))
                        .child(Input::new(&input).disabled(!enabled)),
                );
                if !unit.is_empty() {
                    field = field.child(div().text_color(p.fg_dim).child(unit.clone()));
                }
                field.into_any_element()
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
                    .w(px(110.))
                    .child(Input::new(&input).disabled(!enabled))
                    .into_any_element()
            }
            Control::Switch(on) => {
                let (view, device, key) = (view.clone(), device.clone(), key.clone());
                Switch::new(SharedString::from(format!("ls-setting-{key}")))
                    .checked(*on)
                    .disabled(!enabled)
                    .on_click(move |on, _, cx| {
                        let value = if *on { "1" } else { "0" };
                        let (device, key) = (device.clone(), key.clone());
                        view.update(cx, |this, cx| this.settings_edit(&device, &key, value, cx));
                    })
                    .into_any_element()
            }
            Control::Info(text) => div().max_w(px(420.)).child(text.clone()).into_any_element(),
            Control::Radio(options) => {
                let mut column = v_flex().gap(px(4.));
                for (mode, text, selected, padlock) in options {
                    let (view, device, key, mode) =
                        (view.clone(), device.clone(), key.clone(), mode.clone());
                    let label = if *padlock {
                        format!("\u{1F512} {text}")
                    } else {
                        text.clone()
                    };
                    let mut button = Button::new(SharedString::from(format!("ls-radio-{mode}")))
                        .small()
                        .label(label)
                        .disabled(!enabled)
                        .on_click(move |_, _, cx| {
                            let (device, key, mode) = (device.clone(), key.clone(), mode.clone());
                            view.update(cx, |this, cx| {
                                this.settings_edit(&device, &key, &mode, cx)
                            });
                        });
                    if *selected {
                        button = button.primary();
                    }
                    column = column.child(button);
                }
                column.into_any_element()
            }
        };
        let mut title = v_flex().flex_1().child(
            h_flex()
                .gap(px(8.))
                .child(div().font_weight(FontWeight::BOLD).child(row.label.clone()))
                .when(!row.badge.is_empty(), |t| {
                    t.child(
                        div()
                            .text_size(px(12.))
                            .text_color(if row.writable { p.success } else { p.fg_dim })
                            .child(if row.writable {
                                row.badge.clone()
                            } else {
                                format!("\u{1F512} {}", row.badge)
                            }),
                    )
                }),
        );
        if let Some(text) = &row.caption {
            title = title.child(caption(text.clone(), p));
        }
        if let Some(reason) = &row.reason {
            title = title.child(caption(reason.clone(), p));
        }
        if let Some(error) = &row.error {
            title = title.child(div().text_color(p.alarm).child(error.clone()));
        }
        let mut line = card_row(p)
            .items_start()
            .when(row.dim, |l| l.opacity(0.5))
            .when(row.changed, |l| l.bg(p.accent_tint))
            .child(title)
            .child(control);
        if let Some(label) = &row.use_current {
            let (device, key) = (device.clone(), row.key.clone());
            line = line.child(
                Button::new(SharedString::from(format!("ls-use-{key}")))
                    .small()
                    .label(label.clone())
                    .disabled(!enabled)
                    .on_click(act(&view, move |this, _, cx| {
                        let mut ui = std::mem::take(&mut this.leakshield.settings);
                        this.with_input(cx, |input| ui.use_current(input, &device, &key));
                        this.leakshield.settings = ui;
                    })),
            );
        }
        if let Some((label, target)) = &row.link {
            let target = target.clone();
            line = line.child(
                Self::link_button(
                    SharedString::from(format!("ls-link-{}", row.key)),
                    label.clone(),
                )
                .on_click(act(&view, move |this, _, cx| match &target {
                    Link::Loops => this.go(Page::Loops, cx),
                    Link::Fields(field) => this.leakshield.open_fields(Some(field)),
                })),
            );
        }
        line.into_any_element()
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
            column = column.child(header_row(
                &["setting", "now", "new", "note"]
                    .iter()
                    .map(|name| locale.tr(&format!("leakshield.settings.column.{name}")))
                    .collect::<Vec<_>>(),
                &[0., 160., 160., 220.],
                p,
            ));
        }
        for row in &summary.rows {
            let name = if row.padlock {
                format!("\u{1F512} {}", row.name)
            } else {
                row.name.clone()
            };
            column = column.child(
                card_row(p)
                    .child(div().flex_1().child(name))
                    .child(div().w(px(160.)).child(row.now.clone()))
                    .child(
                        div()
                            .w(px(160.))
                            .font_weight(FontWeight::BOLD)
                            .child(row.new.clone()),
                    )
                    .child(
                        div().w(px(220.)).child(
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
                    .child(div().text_color(p.success).child("\u{2713}"))
                    .child(text.clone()),
            );
        }
        if let Some(failed) = &summary.failed {
            let mut row = card_row(p)
                .bg(p.alarm_tint)
                .child(div().flex_1().child(failed.text.clone()));
            if let Some(command) = failed.restore_command.clone() {
                row = row.child(
                    Button::new("ls-wizard-restore")
                        .small()
                        .danger()
                        .label(format!("\u{1F512} {}", failed.restore))
                        .disabled(!ready)
                        .on_click(act(&view, move |this, _, _| this.send(command.clone()))),
                );
            }
            row = row.child(
                Button::new("ls-wizard-keep")
                    .small()
                    .label(failed.keep.clone())
                    .on_click(act(&view, |this, _, _| {
                        this.leakshield.settings.kept = true
                    })),
            );
            column = column.child(row);
        }
        let device = id.to_owned();
        column = column.child(
            card_row(p).child(div().flex_1()).child(
                Button::new("ls-wizard-apply")
                    .primary()
                    .label(summary.apply.clone())
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
