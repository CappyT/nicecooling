//! The window of the Alarms screen, and the countdown dialog that shows over every page.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, Context, FocusHandle, FontWeight, SharedString, Window, div, px};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::tooltip::Tooltip;
use gpui_component::{Sizable as _, h_flex, v_flex};

use crate::command::Command;
use crate::model::{Response, Severity};
use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::alarms::{
    ActiveRow, AlarmsModel, AlarmsState, CountdownDialog, HistoryRow, ResponseGroup, ResponseRow,
};
use crate::screens::controls::{Pick, act};
use crate::screens::ui::{
    caption, card, card_header, card_row, modal, page, severity_icon, write_line,
};
use crate::tokens::Palette;

impl ScreenView {
    pub(crate) fn render_alarms(
        &mut self,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = self.with_input(cx, |input| AlarmsModel::build(input, &self.alarms));
        let locale = self.model.read(cx).locale;
        if model.state == AlarmsState::Loading {
            return page()
                .child(
                    card(p)
                        .p(px(18.))
                        .gap(px(12.))
                        .child(skeleton(p, 200.))
                        .child(skeleton(p, 520.))
                        .child(skeleton(p, 380.)),
                )
                .into_any_element();
        }

        // Active.
        let active = &model.active;
        let mut active_card = card(p).child(card_header(active.title.clone(), p));
        if let Some(text) = &active.empty {
            active_card = active_card.child(
                card_row(p)
                    .child(div().text_color(p.success).child("\u{2713}"))
                    .child(text.clone()),
            );
        } else {
            active_card =
                active_card.child(columns(&active.columns, &[160., 190., 90., 200., 180.], p));
        }
        for (index, row) in active.rows.iter().enumerate() {
            active_card = active_card.child(self.active_row(index, row, p, cx));
        }

        // Responses.
        let responses = &model.responses;
        let mut responses_card = card(p).child(card_header(responses.title.clone(), p));
        if let Some(text) = &responses.disabled_text {
            responses_card = responses_card.child(
                card_row(p)
                    .bg(p.offline_tint)
                    .child(div().text_color(p.offline).child(text.clone())),
            );
        }
        responses_card =
            responses_card.child(columns(&responses.columns, &[230., 140., 220., 100.], p));
        for group in &responses.groups {
            responses_card =
                responses_card.child(self.response_group(group, responses.enabled, p, window, cx));
        }

        // History.
        let history = &model.history;
        let view = cx.entity();
        let filter_pick: Pick = Rc::new(|this, id, _, _| {
            use crate::screens::alarms::HistoryFilter;
            this.alarms.set_filter(match id.as_str() {
                "alarms" => HistoryFilter::Alarms,
                "warnings" => HistoryFilter::Warnings,
                _ => HistoryFilter::All,
            });
        });
        let filter_choices = history
            .filters
            .iter()
            .map(|(filter, text)| {
                use crate::screens::alarms::HistoryFilter;
                (
                    match filter {
                        HistoryFilter::All => "all",
                        HistoryFilter::Alarms => "alarms",
                        HistoryFilter::Warnings => "warnings",
                    }
                    .to_owned(),
                    text.clone(),
                    *filter == history.filter,
                )
            })
            .collect();
        let filter = self.select(
            "history-filter",
            &history.filter_label,
            filter_choices,
            true,
            "",
            filter_pick,
            p,
            cx,
        );
        let mut history_card = card(p).child(card_header(history.title.clone(), p).child(filter));
        if let Some(text) = &history.empty {
            history_card = history_card.child(card_row(p).child(text.clone()));
        } else {
            history_card = history_card.child(columns(
                &history.columns,
                &[150., 190., 190., 110., 220.],
                p,
            ));
        }
        for (index, row) in history.rows.iter().enumerate() {
            history_card = history_card.child(self.history_row(index, row, p, &view));
        }

        let _ = (locale, window);
        page()
            .child(active_card)
            .child(responses_card)
            .child(history_card)
            .into_any_element()
    }

    fn active_row(
        &mut self,
        index: usize,
        row: &ActiveRow,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let page_target = row.open.clone();
        let mut line = h_flex()
            .id(("alarm-active", index))
            .w_full()
            .px(px(18.))
            .py(px(8.))
            .gap(px(12.))
            .items_center()
            .border_t_1()
            .border_color(p.border)
            .when(page_target.is_some(), |line| {
                line.cursor_pointer().hover(|line| line.bg(p.hover))
            })
            .on_click(act(&view, move |this, _, cx| {
                if let Some(page) = page_target.clone() {
                    this.go(page, cx);
                }
            }))
            .child(
                h_flex()
                    .w(px(160.))
                    .gap(px(6.))
                    .child(severity_icon(row.severity, p))
                    .child(div().font_weight(FontWeight::BOLD).child(row.event.clone())),
            )
            .child(div().w(px(190.)).child(row.source.clone()))
            .child(div().w(px(90.)).child(row.since.clone()))
            .child(div().w(px(200.)).child(row.value.clone()))
            .child(
                v_flex()
                    .flex_1()
                    .child(div().child(row.response.clone()))
                    .children(row.countdown.clone().map(|text| {
                        div()
                            .text_color(p.alarm)
                            .font_weight(FontWeight::BOLD)
                            .child(text)
                    })),
            );
        if let Some(command) = row.cancel.clone() {
            line = line.child(
                Button::new(SharedString::from(format!("alarm-cancel-{index}")))
                    .small()
                    .danger()
                    .label(row.cancel_label.clone())
                    .on_click(act(&view, move |this, _, _| this.send(command.clone()))),
            );
        }
        let mut column = v_flex().w_full().child(line);
        if let Some(text) = &row.cancelled {
            column = column.child(card_row(p).child(caption(text.clone(), p)));
        }
        if let Some(text) = &row.caption {
            column = column.child(card_row(p).child(caption(text.clone(), p)));
        }
        column.into_any_element()
    }

    fn response_group(
        &mut self,
        group: &ResponseGroup,
        enabled: bool,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut column = v_flex().w_full().child(
            card_row(p).bg(p.selected).child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .child(group.title.clone()),
            ),
        );
        for row in &group.rows {
            column = column.child(self.response_row(row, &group.title, enabled, p, window, cx));
        }
        column.into_any_element()
    }

    fn response_row(
        &mut self,
        row: &ResponseRow,
        group: &str,
        enabled: bool,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.model.read(cx).locale;
        let view = cx.entity();
        let event = row.event.clone();
        let source = row.source.clone();
        let delay_s = row.delay_s;
        let pick: Pick = Rc::new(move |this, id, _, _| {
            let response = Response::parse(&id);
            this.send(Command::SetAlarmResponse {
                event: event.clone(),
                source: source.clone(),
                response,
                delay_s,
            });
        });
        let shown = row
            .options
            .iter()
            .find(|o| o.selected)
            .map(|o| o.label.clone())
            .unwrap_or_default();
        let select = self.select(
            &format!("response-{}", row.key),
            &shown,
            row.options
                .iter()
                .map(|o| (o.response.id().to_owned(), o.label.clone(), o.selected))
                .collect(),
            enabled,
            "",
            pick,
            p,
            cx,
        );
        let delay = self.input(
            InputKey::ResponseDelay(row.key.clone()),
            &row.delay_s.to_string(),
            "",
            window,
            cx,
        );
        let mut line = card_row(p)
            .child(
                h_flex()
                    .w(px(230.))
                    .gap(px(6.))
                    .child(severity_icon(row.severity, p))
                    .child(row.title.clone()),
            )
            .child(div().w(px(140.)).child(group.to_owned()))
            .child(div().w(px(220.)).child(select))
            .child(
                div()
                    .w(px(100.))
                    .child(Input::new(&delay).disabled(!row.delay_enabled)),
            );
        if let Some(text) = row.overlap.clone() {
            line = line.child(
                div()
                    .id(SharedString::from(format!("response-overlap-{}", row.key)))
                    .tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx))
                    .child(severity_icon(Severity::Warning, p)),
            );
        }
        if let Some(command) = row.reset.clone() {
            line = line.child(
                Self::link_button(
                    SharedString::from(format!("response-reset-{}", row.key)),
                    locale.tr("alarms.reset"),
                )
                .on_click(act(&view, move |this, _, _| this.send(command.clone()))),
            );
        }
        if let Some(write) = &row.write_line {
            line = line.child(write_line(write, p));
        }
        let mut column = v_flex().w_full().child(line);
        if let Some(text) = &row.delay_warning {
            column = column.child(
                card_row(p)
                    .bg(p.warning_tint)
                    .child(severity_icon(Severity::Warning, p))
                    .child(text.clone()),
            );
        }
        column.into_any_element()
    }

    fn history_row(
        &self,
        index: usize,
        row: &HistoryRow,
        p: &Palette,
        view: &gpui::Entity<ScreenView>,
    ) -> AnyElement {
        let sensor = row.sensor.clone();
        h_flex()
            .id(("alarm-history", index))
            .w_full()
            .px(px(18.))
            .py(px(8.))
            .gap(px(12.))
            .items_center()
            .border_t_1()
            .border_color(p.border)
            .cursor_pointer()
            .hover(|line| line.bg(p.hover))
            .on_click(act(view, move |this, _, cx| match sensor.clone() {
                Some(sensor) => this.open_history_for(&sensor, cx),
                None => this.go(crate::shell::Page::History, cx),
            }))
            .child(div().w(px(150.)).child(row.time.clone()))
            .child(
                h_flex()
                    .w(px(190.))
                    .gap(px(6.))
                    .child(severity_icon(row.severity, p))
                    .child(row.event.clone()),
            )
            .child(div().w(px(190.)).child(row.source.clone()))
            .child(div().w(px(110.)).child(row.duration.clone()))
            .child(div().flex_1().child(row.response.clone()))
            .into_any_element()
    }
}

/// The header line of a table.
fn columns(titles: &[String], widths: &[f32], p: &Palette) -> AnyElement {
    let mut line = h_flex()
        .w_full()
        .px(px(18.))
        .py(px(6.))
        .gap(px(12.))
        .border_t_1()
        .border_color(p.border)
        .text_size(px(12.))
        .font_weight(FontWeight::BOLD)
        .text_color(p.fg_dim);
    for (index, title) in titles.iter().enumerate() {
        line = match widths.get(index) {
            Some(width) if index + 1 < titles.len() => {
                line.child(div().w(px(*width)).child(title.clone()))
            }
            _ => line.child(div().flex_1().child(title.clone())),
        };
    }
    line.into_any_element()
}

/// The countdown dialog: 440 px over the scrim, with the focus on "Cancel poweroff".
pub fn countdown_overlay(
    dialog: &CountdownDialog,
    focus: &FocusHandle,
    on_cancel: impl Fn(&mut gpui::Window, &mut gpui::App) + 'static,
    on_hide: impl Fn(&mut gpui::Window, &mut gpui::App) + 'static,
    p: &Palette,
) -> AnyElement {
    let mut body: Vec<AnyElement> = vec![
        div()
            .text_size(px(40.))
            .font_weight(FontWeight::BOLD)
            .text_color(p.alarm)
            .child(dialog.seconds.to_string())
            .into_any_element(),
        caption(dialog.seconds_label.clone(), p).into_any_element(),
        div()
            .h(px(6.))
            .w_full()
            .rounded_full()
            .bg(p.track)
            .child(
                div()
                    .h(px(6.))
                    .w(gpui::relative(dialog.fraction))
                    .rounded_full()
                    .bg(p.alarm_bg),
            )
            .into_any_element(),
    ];
    for (label, value) in &dialog.rows {
        body.push(
            h_flex()
                .gap(px(8.))
                .child(div().w(px(110.)).text_color(p.fg_dim).child(label.clone()))
                .child(div().flex_1().child(value.clone()))
                .into_any_element(),
        );
    }
    if let Some(text) = &dialog.more {
        body.push(caption(text.clone(), p).into_any_element());
    }
    let mut buttons: Vec<AnyElement> = Vec::new();
    if !dialog.ended {
        buttons.push(
            Button::new("countdown-hide")
                .label(dialog.hide.clone())
                .on_click(move |_, window, cx| on_hide(window, cx))
                .into_any_element(),
        );
        buttons.push(
            Button::new("countdown-cancel")
                .danger()
                .track_focus(focus)
                .label(dialog.cancel.clone())
                .on_click(move |_, window, cx| on_cancel(window, cx))
                .into_any_element(),
        );
        body.push(caption(dialog.hint.clone(), p).into_any_element());
    }
    modal(dialog.title.clone(), dialog.text.clone(), body, buttons, p).into_any_element()
}
