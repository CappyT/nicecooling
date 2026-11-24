//! The window of the Overview tab: the gauge, the status, the Mode and the Actions.

use chrono::{Local, TimeZone as _};
use gpui::prelude::*;
use gpui::{
    AnyElement, Context, FontWeight, HighlightStyle, SharedString, StyledText, Window, div, px,
};
use gpui_component::button::ButtonVariants as _;
use gpui_component::progress::Progress;
use gpui_component::tooltip::Tooltip;
use gpui_component::{Disableable as _, h_flex, v_flex};

use crate::app::now_us;
use crate::history::HistoryDb;
use crate::history::query::{downsample, load_series};
use crate::icons::{NcIcon, icon, small_icon};
use crate::screen_view::{InputKey, ScreenView};
use crate::screens::controls::act;
use crate::screens::leakshield::TabState;
use crate::screens::leakshield::charts::{
    Reference, Runs, TimeChart, gauge, runtime_chart, time_chart,
};
use crate::screens::leakshield::overview::{
    ActionView, ChartRange, Confirm, GaugeState, ModeCard, OverviewModel, TestSource,
};
use crate::screens::leakshield::parts::{header, loading_card, state_tag};
use crate::screens::ui::{
    caption, card, card_header, card_row, modal, notice_row, page, seg, seg_item, severity_icon,
    write_line,
};
use crate::tokens::Palette;

/// The most points that the pressure chart draws for one run.
const CHART_POINTS: usize = 600;

impl ScreenView {
    pub(crate) fn render_ls_overview(
        &mut self,
        id: &str,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = self.with_input(cx, |input| {
            OverviewModel::build(input, &self.leakshield.overview, id)
        });
        let view = cx.entity();
        let mut content = page();
        if model.state == TabState::Loading {
            return content
                .child(loading_card(model.loading_text.clone(), p))
                .into_any_element();
        }
        let dim = model.state != TabState::Ready;
        let pressure = &model.pressure;
        let out_of_range = matches!(pressure.gauge, GaugeState::Below | GaugeState::Above);
        let value_color = if pressure.alarm {
            p.alarm
        } else if out_of_range {
            p.warning
        } else if dim {
            p.fg_dim
        } else {
            p.fg
        };
        let tooltip = pressure.tooltip.clone();
        let pressure_card = card(p)
            .flex_1()
            .flex_grow(1.5)
            .child(
                header(pressure.title.clone(), pressure.caption.clone(), p).child(state_tag(
                    pressure.tag.clone(),
                    pressure.tag_kind,
                    p,
                )),
            )
            .child(
                v_flex()
                    .id("ls-gauge")
                    .px(px(18.))
                    .pb(px(12.))
                    .when(!tooltip.is_empty(), |body| {
                        body.tooltip(move |window, cx| {
                            Tooltip::new(tooltip.clone()).build(window, cx)
                        })
                    })
                    .child(
                        h_flex()
                            .items_baseline()
                            .gap(px(6.))
                            .child(
                                div()
                                    .text_size(px(28.))
                                    .line_height(px(32.))
                                    .font_weight(FontWeight::EXTRA_BOLD)
                                    .text_color(value_color)
                                    .child(pressure.number.clone()),
                            )
                            .child(div().text_color(p.fg_dim).child(pressure.unit.clone())),
                    )
                    .child(gauge(
                        pressure
                            .value
                            .filter(|_| pressure.gauge != GaugeState::Offline),
                        pressure.limits,
                        value_color,
                        pressure.labels.clone(),
                        p,
                    )),
            );
        let mut status_card =
            card(p)
                .flex_1()
                .child(card_header(model.status.title.clone(), p).child(state_tag(
                    model.status.tag.clone(),
                    model.status.tag_kind,
                    p,
                )));
        for row in &model.status.rows {
            status_card = status_card.child(
                h_flex()
                    .w_full()
                    .min_h(px(30.))
                    .px(px(18.))
                    .py(px(3.))
                    .gap(px(12.))
                    .border_t_1()
                    .border_color(p.border)
                    .text_color(if dim { p.fg_dim } else { p.fg })
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(6.))
                            .when_some(row.severity, |label, severity| {
                                label.child(severity_icon(severity, p))
                            })
                            .child(div().min_w_0().truncate().child(row.label.clone())),
                    )
                    .child(
                        div()
                            .flex_none()
                            .when(row.bold, |value| value.font_weight(FontWeight::BOLD))
                            .child(row.value.clone()),
                    )
                    .when_some(row.caption.clone(), |line, text| {
                        line.child(caption(text, p).flex_none())
                    }),
            );
        }
        content = content.child(
            h_flex()
                .w_full()
                .gap(px(12.))
                .items_stretch()
                .child(pressure_card)
                .child(status_card),
        );
        if let Some(text) = &model.mode.release_banner {
            content = content.child(card(p).child(notice_row(text.clone(), true, p)));
        }
        content = content.child(self.mode_card(id, &model.mode, p, cx));

        content = content.child(match &model.action_view {
            Some(action) => self.action_view(id, action, p, window, cx),
            None => self.charts_row(id, &model, p, cx),
        });

        let mut root = div().relative().w_full().child(content);
        if let Some(dialog) = &model.dialog {
            let (cancel, confirm) = (view.clone(), view.clone());
            let id = id.to_owned();
            let mut buttons = vec![
                Self::small_button("ls-dialog-cancel", dialog.cancel.clone(), None)
                    .on_click(act(&cancel, |this, _, _| {
                        this.leakshield.overview.cancel_dialog()
                    }))
                    .into_any_element(),
            ];
            if let Some(label) = &dialog.confirm {
                buttons.push(
                    Self::small_button("ls-dialog-confirm", label.clone(), None)
                        .on_click(act(&confirm, move |this, _, _| {
                            if let Some(command) = this.leakshield.overview.confirm(&id) {
                                this.send(command);
                            }
                        }))
                        .into_any_element(),
                );
            }
            let steps = dialog
                .steps
                .iter()
                .map(|step| div().child(step.clone()).into_any_element())
                .collect();
            root = root.child(modal(
                dialog.title.clone(),
                dialog.text.clone(),
                steps,
                buttons,
                p,
            ));
        }
        root.into_any_element()
    }

    fn mode_card(
        &mut self,
        id: &str,
        mode: &ModeCard,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let mut selector = seg(p);
        for choice in &mode.buttons {
            let (device, name) = (id.to_owned(), choice.mode.clone());
            let mut item = seg_item(
                SharedString::from(format!("ls-mode-{}", choice.mode)),
                choice.label.clone(),
                choice.selected,
                choice.enabled,
                p,
            );
            if choice.enabled {
                item = item.on_click(act(&view, move |this, _, cx| {
                    let state = this.state(cx);
                    if let Some(command) = this
                        .leakshield
                        .overview
                        .request_mode(&state, &device, &name)
                    {
                        this.send(command);
                    }
                }));
            }
            if let Some(tooltip) = mode.tooltip.clone() {
                item =
                    item.tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx));
            }
            selector = selector.child(item);
        }
        let mut actions = h_flex()
            .flex_1()
            .min_w_0()
            .flex_wrap()
            .justify_end()
            .gap(px(6.));
        if let Some(cancel) = &mode.cancel {
            let device = id.to_owned();
            actions = actions.child(Self::button("ls-action-cancel", cancel.clone()).on_click(
                act(&view, move |this, _, _| {
                    this.send(crate::command::Command::CancelLeakshieldAction {
                        leakshield: device.clone(),
                    });
                }),
            ));
        } else {
            for action in &mode.actions {
                let (device, name) = (id.to_owned(), action.action.clone());
                let mut item = Self::button(
                    SharedString::from(format!("ls-action-{}", action.action)),
                    action.label.clone(),
                )
                .disabled(!action.enabled)
                .on_click(act(&view, move |this, _, cx| {
                    let state = this.state(cx);
                    if let Some(command) = this
                        .leakshield
                        .overview
                        .request_action(&state, &device, &name)
                    {
                        this.send(command);
                    }
                }));
                if let Some(tooltip) = &mode.tooltip {
                    item = item.tooltip(tooltip.clone());
                }
                actions = actions.child(item);
            }
        }
        // One caption: the meaning of the Mode, then the hint about Actions.
        let text = [mode.caption.as_str(), mode.action_hint.as_str()]
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        let mut body = v_flex()
            .px(px(18.))
            .pb(px(14.))
            .gap(px(6.))
            .child(
                h_flex()
                    .w_full()
                    .gap_x(px(24.))
                    .gap_y(px(6.))
                    .child(selector)
                    .child(actions),
            )
            .child(caption(text, p));
        if let Some((text, link)) = &mode.read_only {
            body = body.child(
                h_flex()
                    .gap(px(6.))
                    .child(small_icon(NcIcon::Lock, p.fg_dim))
                    .child(caption(text.clone(), p))
                    .child(
                        Self::link_button("ls-open-fields-mode", link.clone(), p)
                            .text_size(px(12.))
                            .on_click(act(&view, |this, _, _| {
                                this.leakshield.open_fields(Some("default_mode"))
                            })),
                    ),
            );
        }
        if let Some(text) = &mode.denied {
            body = body.child(div().text_color(p.alarm).child(text.clone()));
        } else if let Some(line) = &mode.write_line {
            body = body.child(write_line(line, p));
        }
        if let Some(line) = &mode.action_line {
            body = body.child(write_line(line, p));
        }
        body = body.child(
            h_flex()
                .w_full()
                .px(px(12.))
                .py(px(8.))
                .gap(px(9.))
                .rounded(px(9.))
                .bg(p.hover)
                .child(icon(NcIcon::Info, p.fg))
                .child(div().flex_1().min_w_0().child(
                    StyledText::new(mode.refill.text.clone()).with_highlights([(
                        mode.refill.action.clone(),
                        HighlightStyle {
                            font_weight: Some(FontWeight::BOLD),
                            ..Default::default()
                        },
                    )]),
                ))
                .child(
                    Self::link_button("ls-refill-steps", mode.refill.link.clone(), p).on_click(
                        act(&view, |this, _, _| {
                            this.leakshield.overview.confirm = Some(Confirm::RefillSteps);
                        }),
                    ),
                ),
        );
        card(p)
            .child(card_header(mode.title.clone(), p))
            .child(body)
            .into_any_element()
    }

    /// The pressure of the selected range, from the history database: the runs of
    /// `(seconds, mbar)` and the first and the last second of the range. The read runs in
    /// the background. The chart keeps its last runs until the new ones are there.
    fn pressure_history(&mut self, id: &str, cx: &mut Context<Self>) -> (Runs, (i64, i64)) {
        let range = self.leakshield.overview.range;
        let now = (now_us() / 1_000_000) as i64;
        let from = now - range.seconds();
        let key = (id.to_owned(), range);
        let path = self.model.read(cx).history_path.clone();
        let history = &mut self.leakshield.overview.history;
        if history.key.as_ref() != Some(&key) {
            history.key = Some(key.clone());
            history.runs.clear();
            history.loaded_at = 0;
        }
        // One hour has a sample per second. The longer ranges have one value per minute.
        let period = if range == ChartRange::Hour { 5 } else { 60 };
        if let Some(path) = path
            && !history.pending
            && now - history.loaded_at >= period
        {
            history.pending = true;
            let sensor = format!("{id}_pressure");
            let task = cx.background_executor().spawn(async move {
                let db = HistoryDb::open(&path).ok()?;
                let series = load_series(&db, &sensor, from, now, now).ok()?;
                Some(
                    series
                        .segments
                        .iter()
                        .map(|run| {
                            downsample(run, CHART_POINTS)
                                .iter()
                                .map(|point| (point.t as f64, point.mean))
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>(),
                )
            });
            cx.spawn(async move |this, cx| {
                let runs = task.await;
                let _ = this.update(cx, |this, cx| {
                    let history = &mut this.leakshield.overview.history;
                    history.pending = false;
                    // The user may have picked another range in the meantime.
                    if history.key.as_ref() == Some(&key) {
                        history.loaded_at = now;
                        history.runs = runs.unwrap_or_default();
                        cx.notify();
                    }
                });
            })
            .detach();
        }
        (self.leakshield.overview.history.runs.clone(), (from, now))
    }

    fn charts_row(
        &mut self,
        id: &str,
        model: &OverviewModel,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let (runs, (from, now)) = self.pressure_history(id, cx);
        let mut references = Vec::new();
        if let Some(limits) = model.pressure.limits {
            for value in [limits.min, limits.max] {
                references.push(Reference {
                    value,
                    color: p.border_strong,
                    dash: (2.0, 3.0),
                    label: None,
                });
            }
            references.push(Reference {
                value: limits.target,
                color: p.accent,
                dash: (5.0, 4.0),
                label: None,
            });
        }
        let range = self.leakshield.overview.range;
        let mut x_labels = vec![(from as f64, time_label(from, range, true))];
        x_labels.extend(
            round_times(from, now, range)
                .into_iter()
                .map(|time| (time as f64, time_label(time, range, false))),
        );
        x_labels.push((now as f64, model.charts.now.clone()));
        let mut ranges = seg(p);
        for (range, label, selected) in &model.charts.ranges {
            let range = *range;
            ranges = ranges.child(
                seg_item(
                    SharedString::from(format!("ls-range-{range:?}")),
                    label.clone(),
                    *selected,
                    true,
                    p,
                )
                .on_click(act(&view, move |this, _, _| {
                    this.leakshield.overview.range = range;
                })),
            );
        }
        let pressure = card(p)
            .flex_1()
            .child(card_header(model.charts.pressure_title.clone(), p).child(ranges))
            .child(div().px(px(18.)).pb(px(12.)).child(time_chart(
                TimeChart {
                    runs,
                    domain: Some((from as f64, now as f64)),
                    references,
                    area: true,
                    x_labels,
                    y_labels: true,
                    note: model.charts.pressure_note.clone(),
                    empty: Some(model.charts.pressure_empty.clone()),
                    height: 170.,
                },
                p.series[0],
                p,
            )));
        let runtime = card(p)
            .flex_1()
            .child(header(
                model.charts.runtime_title.clone(),
                model.charts.runtime_caption.clone(),
                p,
            ))
            .child(div().px(px(18.)).pb(px(12.)).child(runtime_chart(
                model.charts.runtime_limit.clone(),
                Some(model.charts.runtime_empty.clone()),
                p,
            )));
        h_flex()
            .w_full()
            .gap(px(12.))
            .items_stretch()
            .child(pressure)
            .child(runtime)
            .into_any_element()
    }

    fn action_view(
        &mut self,
        id: &str,
        action: &ActionView,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        match action {
            ActionView::TestSetup {
                title,
                pressure,
                text,
                error,
                start,
                cancel,
                ..
            } => {
                let input = self.input(
                    InputKey::Ls("test-pressure".into()),
                    pressure,
                    "",
                    window,
                    cx,
                );
                let device = id.to_owned();
                let mut body = v_flex()
                    .p(px(18.))
                    .gap(px(10.))
                    .child(div().w(px(124.)).child(Self::field_input(
                        &input,
                        Some("mbar".into()),
                        false,
                        p,
                    )))
                    .child(caption(text.clone(), p));
                if let Some(error) = error {
                    body = body.child(div().text_color(p.alarm).child(error.clone()));
                }
                body = body.child(
                    h_flex()
                        .gap(px(8.))
                        .child(
                            Self::button("ls-test-start", start.clone())
                                .primary()
                                .disabled(error.is_some())
                                .on_click(act(&view, move |this, _, cx| {
                                    let mut ui = std::mem::take(&mut this.leakshield.overview);
                                    let command =
                                        this.with_input(cx, |input| ui.start_test(input, &device));
                                    this.leakshield.overview = ui;
                                    if let Some(command) = command {
                                        this.send(command);
                                    }
                                })),
                        )
                        .child(
                            Self::button("ls-test-cancel", cancel.clone())
                                .on_click(act(&view, |this, _, _| {
                                    this.leakshield.overview.cancel_test_setup()
                                })),
                        ),
                );
                card(p)
                    .child(card_header(title.clone(), p))
                    .child(body)
                    .into_any_element()
            }
            ActionView::TestRunning {
                source_label,
                sources,
                reference,
                reset_zoom,
                cancel,
                reference_value,
            } => {
                let traces = self.leakshield.traces.get(id);
                let source = sources
                    .iter()
                    .find(|(_, _, selected)| *selected)
                    .map_or(TestSource::Pressure, |(s, _, _)| *s);
                let points: Vec<(f64, f64)> = traces
                    .map(|t| match source {
                        TestSource::Pressure => t
                            .pressure
                            .points
                            .iter()
                            .map(|(t, v)| (*t as f64, *v))
                            .collect(),
                        TestSource::Difference => t
                            .pressure
                            .points
                            .iter()
                            .map(|(t, v)| (*t as f64, v - reference_value.unwrap_or(*v)))
                            .collect(),
                        TestSource::Change => t
                            .pressure_change
                            .points
                            .iter()
                            .map(|(t, v)| (*t as f64, *v))
                            .collect(),
                    })
                    .unwrap_or_default();
                let mut choices = h_flex()
                    .gap(px(4.))
                    .items_center()
                    .child(source_label.clone());
                for (kind, label, selected) in sources {
                    let kind = *kind;
                    let mut button = Self::small_button(
                        SharedString::from(format!("ls-source-{kind:?}")),
                        label.clone(),
                        None,
                    )
                    .on_click(act(&view, move |this, _, _| {
                        this.leakshield.overview.source = kind;
                    }));
                    if *selected {
                        button = button.primary();
                    }
                    choices = choices.child(button);
                }
                let device = id.to_owned();
                let current = self
                    .leakshield
                    .traces
                    .get(id)
                    .and_then(|t| t.pressure.points.back().map(|(_, v)| *v));
                card(p)
                    .child(
                        card_row(p)
                            .child(choices)
                            .child(div().flex_1())
                            .child(
                                Self::small_button("ls-test-reference", reference.clone(), None)
                                    .on_click(act(&view, move |this, _, _| {
                                        this.leakshield.overview.reference = current;
                                    })),
                            )
                            .child(Self::small_button("ls-test-zoom", reset_zoom.clone(), None))
                            .child({
                                let device = device.clone();
                                Self::small_button("ls-test-cancel-run", cancel.clone(), None)
                                    .on_click(act(&view, move |this, _, _| {
                                        this.send(
                                            crate::command::Command::CancelLeakshieldAction {
                                                leakshield: device.clone(),
                                            },
                                        );
                                    }))
                            }),
                    )
                    .child(div().px(px(18.)).pb(px(12.)).child(time_chart(
                        TimeChart {
                            runs: vec![points],
                            y_labels: true,
                            height: 170.,
                            ..Default::default()
                        },
                        p.series[0],
                        p,
                    )))
                    .into_any_element()
            }
            ActionView::MeasureFillLevel {
                title,
                step,
                banner,
                result,
            } => card(p)
                .child(card_header(title.clone(), p))
                .child(notice_row(banner.clone(), true, p))
                .child(
                    v_flex()
                        .p(px(18.))
                        .gap(px(8.))
                        .child(step.clone())
                        .child(Progress::new("ls-measure-progress").value(50.))
                        .when_some(result.clone(), |body, text| {
                            body.child(div().font_weight(FontWeight::BOLD).child(text))
                        }),
                )
                .into_any_element(),
            ActionView::Deaerate {
                title,
                current,
                overall,
            } => card(p)
                .child(card_header(title.clone(), p))
                .child(
                    v_flex()
                        .p(px(18.))
                        .gap(px(8.))
                        .child(current.clone())
                        .child(Progress::new("ls-deaerate-current").value(50.))
                        .child(overall.clone())
                        .child(Progress::new("ls-deaerate-overall").value(25.)),
                )
                .into_any_element(),
        }
    }
}

/// The label of a time of the pressure chart: the clock for one hour and one day, the date
/// for seven days. The first label of seven days has both.
fn time_label(seconds: i64, range: ChartRange, first: bool) -> String {
    let pattern = match (range, first) {
        (ChartRange::Week, true) => "%m-%d %H:%M",
        (ChartRange::Week, false) => "%m-%d",
        _ => "%H:%M",
    };
    Local
        .timestamp_opt(seconds, 0)
        .single()
        .map_or_else(String::new, |time| time.format(pattern).to_string())
}

/// The round local times between `from` and `to`: every 20 minutes of one hour, every
/// 6 hours of one day, every 2 days of seven days.
fn round_times(from: i64, to: i64, range: ChartRange) -> Vec<i64> {
    let step = match range {
        ChartRange::Hour => 1_200,
        ChartRange::Day => 21_600,
        ChartRange::Week => 172_800,
    };
    let offset = Local
        .timestamp_opt(from, 0)
        .single()
        .map_or(0, |time| i64::from(time.offset().local_minus_utc()));
    let first = (from + offset).div_euclid(step) * step + step - offset;
    (0..)
        .map(|index| first + index * step)
        .take_while(|time| *time < to)
        .collect()
}
