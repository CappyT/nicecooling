//! The window of the Alarms screen.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, Context, Div, FontWeight, SharedString, Window, div, px};
use gpui_component::button::ButtonVariants as _;
use gpui_component::{h_flex, v_flex};

use crate::command::Command;
use crate::icons::{NcIcon, icon};
use crate::model::{Response, Severity};
use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::alarms::{
    ActiveRow, AlarmsModel, AlarmsState, HistoryRow, ResponseGroup, ResponseRow,
};
use crate::screens::controls::{Pick, act};
use crate::screens::ui::Tip as _;
use crate::screens::ui::{
    caption, card, card_header, card_row, cell, heading, label, page, severity_icon, table_header,
    table_row, wrap_cell, write_line,
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
                    .child(icon(NcIcon::Check, p.success))
                    .child(text.clone()),
            );
        } else {
            active_card = active_card.child(table_header(&active.columns, &columns(ACTIVE, 0), p));
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
            responses_card.child(table_header(&responses.columns, &RESPONSE_COLUMNS, p));
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
        let filter = self.select_field(
            "history-filter",
            &history.filter_label,
            filter_choices,
            true,
            true,
            p.track,
            filter_pick,
            p,
            cx,
        );
        let mut history_card = card(p).child(
            card_header(history.title.clone(), p)
                .child(div().w(px(140.)).flex_none().child(filter)),
        );
        if let Some(text) = &history.empty {
            history_card = history_card.child(card_row(p).child(text.clone()));
        } else {
            history_card =
                history_card.child(table_header(&history.columns, &columns(HISTORY, 1), p));
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
        let line = table_row(p)
            .id(("alarm-active", index))
            .py(px(6.))
            .when(page_target.is_some(), |line| {
                line.cursor_pointer().hover(|line| line.bg(p.hover))
            })
            .on_click(act(&view, move |this, _, cx| {
                if let Some(page) = page_target.clone() {
                    this.go(page, cx);
                }
            }))
            .child(
                event_cell()
                    .debug_selector(|| "active-event-cell".to_owned())
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .child(severity_icon(row.severity, p))
                            .child(event_name(row.event.clone())),
                    ),
            )
            .child(cell(ACTIVE[1]).child(row.source.clone()))
            .child(cell(ACTIVE[2]).child(row.since.clone()))
            .child(cell(ACTIVE[3]).child(row.value.clone()))
            .child(cell(ACTIVE[4]).child(row.response.clone()));
        let mut column = v_flex().w_full().child(line);
        // A pending poweroff has a second line in its row: the countdown and the button.
        // The cells of the first line keep their widths in English and in Italian.
        if row.countdown.is_some() || row.cancel.is_some() {
            let mut pending = h_flex()
                .w_full()
                .px(px(18.))
                .pb(px(8.))
                .gap(px(12.))
                .justify_end()
                .children(row.countdown.clone().map(|text| {
                    label(text)
                        .text_color(p.alarm)
                        .font_weight(FontWeight::BOLD)
                }));
            if let Some(command) = row.cancel.clone() {
                pending = pending.child(
                    div()
                        .flex_none()
                        .debug_selector(|| "active-cancel".to_owned())
                        .child(
                            Self::small_button(
                                SharedString::from(format!("alarm-cancel-{index}")),
                                row.cancel_label.clone(),
                                None,
                            )
                            .primary()
                            .on_click(act(&view, move |this, _, _| this.send(command.clone()))),
                        ),
                );
            }
            column = column.child(pending);
        }
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
            card_row(p)
                .min_h(px(30.))
                .py(px(3.))
                .bg(p.hover)
                .child(heading(group.title.clone()).truncate()),
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
        let select = self.select_field(
            &format!("response-{}", row.key),
            &shown,
            row.options
                .iter()
                .map(|o| (o.response.id().to_owned(), o.label.clone(), o.selected))
                .collect(),
            enabled,
            true,
            p.track,
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
        let line = table_row(p)
            .child(
                event_cell()
                    .debug_selector(|| "response-event-cell".to_owned())
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .child(severity_icon(row.severity, p))
                            .child(event_name(row.title.clone()).debug_selector({
                                let key = row.key.clone();
                                move || format!("response-title-{key}")
                            })),
                    ),
            )
            .child(wrap_cell(SOURCE).child(group.to_owned()).debug_selector({
                let key = row.key.clone();
                move || format!("response-source-{key}")
            }))
            .child(div().w(px(RESPONSE)).flex_none().child(select))
            .child(div().w(px(DELAY)).flex_none().child(
                Self::field_input(&delay, Some("s".into()), true, p).disabled(!row.delay_enabled),
            ));
        // The notes of the row: the overlap mark and "Reset", then the write feedback under
        // them. The feedback wraps in the cell.
        let mut marks = h_flex().gap(px(6.));
        let has_marks = row.overlap.is_some() || row.reset.is_some();
        if let Some(text) = row.overlap.clone() {
            marks = marks.child(
                div()
                    .id(SharedString::from(format!("response-overlap-{}", row.key)))
                    .tip(text)
                    .child(severity_icon(Severity::Warning, p)),
            );
        }
        if let Some(command) = row.reset.clone() {
            marks = marks.child(
                Self::link_button(
                    SharedString::from(format!("response-reset-{}", row.key)),
                    locale.tr("alarms.reset"),
                    p,
                )
                .on_click(act(&view, move |this, _, _| this.send(command.clone()))),
            );
        }
        let notes = v_flex()
            .w(px(NOTES))
            .flex_none()
            .min_w_0()
            .gap(px(3.))
            .debug_selector(|| "response-notes".to_owned())
            .children(has_marks.then_some(marks))
            .children(row.write_line.as_ref().map(|write| write_line(write, p)));
        let mut column = v_flex().w_full().child(line.child(notes));
        if let Some(text) = &row.delay_warning {
            column = column.child(
                card_row(p)
                    .bg(p.warning_tint)
                    .child(severity_icon(Severity::Warning, p))
                    .child(div().flex_1().min_w_0().child(text.clone())),
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
        table_row(p)
            .id(("alarm-history", index))
            .py(px(6.))
            .cursor_pointer()
            .hover(|line| line.bg(p.hover))
            .on_click(act(view, move |this, _, cx| match sensor.clone() {
                Some(sensor) => this.open_history_for(&sensor, cx),
                None => this.go(crate::shell::Page::History, cx),
            }))
            .child(cell(HISTORY[0]).child(row.time.clone()))
            .child(
                event_cell()
                    .debug_selector(|| "history-event-cell".to_owned())
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .child(severity_icon(row.severity, p))
                            .child(event_name(row.event.clone())),
                    ),
            )
            .child(wrap_cell(HISTORY[2]).child(row.source.clone()))
            .child(wrap_cell(HISTORY[3]).child(row.duration.clone()))
            .child(wrap_cell(HISTORY[4]).child(row.response.clone()))
            .into_any_element()
    }
}

/// The Event cell takes the rest of its row and never gets less than this: the names of
/// `docs/ui/screens/alarms.md` show in full at the minimum window size.
const EVENT_MIN: f32 = 160.;
/// The widths of the other cells: they hold a value, so they keep their width. The entry of
/// the Event cell is not used. At the minimum window a row is 796 px wide.
const ACTIVE: [f32; 5] = [0., 150., 70., 150., 170.];
const HISTORY: [f32; 5] = [124., 0., 140., 100., 200.];
/// "Leakshield 20417-31072" and "Poweroff after a delay" show in full. At the minimum window
/// the Event cell of this table is 200 px wide.
const SOURCE: f32 = 176.;
const RESPONSE: f32 = 196.;
const DELAY: f32 = 72.;
const NOTES: f32 = 104.;
const RESPONSE_COLUMNS: [Option<f32>; 4] = [
    None,
    Some(SOURCE),
    Some(RESPONSE),
    Some(DELAY + 12. + NOTES),
];

/// The Event cell: a long name wraps to a second line, so it shows in full in Italian too.
fn event_cell() -> Div {
    div().flex_1().min_w(px(EVENT_MIN))
}

/// The name of an event next to its severity icon. It wraps in its cell.
fn event_name(text: impl Into<SharedString>) -> Div {
    div().flex_1().min_w_0().child(text.into())
}

/// The widths of a table header: the cell at `event` takes the rest of the row.
fn columns(widths: [f32; 5], event: usize) -> [Option<f32>; 5] {
    std::array::from_fn(|index| (index != event).then_some(widths[index]))
}
