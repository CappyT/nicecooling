//! The window of the History screen: the toolbar, the chart and the legend.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, Bounds, Context, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, ScrollEvent, SharedString, Window, canvas, div, px, size,
};
use gpui_component::{Disableable as _, h_flex, v_flex};

use crate::history::query::Resolution;
use crate::icons::NcIcon;
use crate::model::Severity;
use crate::screen_view::{Drag, ScreenView};
use crate::screens::controls::act;
use crate::screens::format::clock;
use crate::screens::history::{
    ChartModel, HistoryModel, HistoryState, SeriesModel, Side, picker_rows,
};
use crate::screens::sensors_view::PickRow;
use crate::screens::ui::{
    caption, card, heading, page, seg, seg_item, select_trigger, severity_icon, with_popover,
};
use crate::tokens::Palette;
use crate::widgets::paint::{at, hatch, polygon, polyline, rect};

const CHART_HEIGHT: f32 = 340.0;
const LEFT: f32 = 44.0;
const RIGHT: f32 = 44.0;
const TOP: f32 = 12.0;
const BOTTOM: f32 = 26.0;

type Pick = Rc<dyn Fn(&mut ScreenView, String, &mut Window, &mut Context<ScreenView>)>;

impl ScreenView {
    pub(crate) fn render_history(
        &mut self,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let data = self.history_data(cx);
        let now = (crate::app::now_us() / 1_000_000) as i64;
        let model = self.with_input(cx, |input| {
            HistoryModel::build(input, &self.history, &data, now, 1200)
        });
        let locale = self.model.read(cx).locale;
        let view = cx.entity();

        // The toolbar: the sensor picker, the range and the navigation.
        let picker = (self.history.picker_open || model.state == HistoryState::Empty).then(|| {
            let rows: Vec<PickRow> = self.with_input(cx, |input| {
                picker_rows(input)
                    .into_iter()
                    .map(|row| PickRow {
                        selected: self.history.selected.contains(&row.id),
                        id: row.id,
                        name: row.name,
                        caption: row.device,
                        value: row.value,
                    })
                    .collect()
            });
            let pick: Pick = Rc::new(|this, sensor, _, cx| {
                this.history.toggle_sensor(&sensor);
                // The popover stays open in multi-select mode.
                this.history.picker_open = true;
                cx.notify();
            });
            self.picker_list("history", rows, pick, p, window, cx)
        });
        let sensors = select_trigger(
            "history-sensors",
            model.sensors_label.clone(),
            None,
            false,
            true,
            p.track,
        )
        .on_click(act(&view, |this, _, _| {
            this.history.picker_open = !this.history.picker_open;
        }));
        let mut ranges = seg(p);
        for button in &model.ranges {
            let range = button.range;
            ranges = ranges.child(
                seg_item(
                    SharedString::from(format!("range-{}", button.label)),
                    button.label.clone(),
                    button.active,
                    true,
                    p,
                )
                .px(px(10.))
                .on_click(act(&view, move |this, _, _| this.history.set_range(range))),
            );
        }
        let nav = |id: &'static str,
                   label: String,
                   icon: Option<NcIcon>,
                   enabled: bool,
                   action: fn(&mut ScreenView, i64)| {
            Self::small_button(id, label, icon)
                .disabled(!enabled)
                .on_click(act(&view, move |this, _, _| {
                    action(this, (crate::app::now_us() / 1_000_000) as i64);
                }))
        };
        let toolbar = h_flex()
            .w_full()
            .gap_x(px(12.))
            .gap_y(px(6.))
            .items_center()
            .flex_wrap()
            .child(with_popover(sensors, picker).w(px(190.)).flex_none())
            .child(ranges)
            .child(
                h_flex()
                    .gap(px(6.))
                    .child(nav(
                        "history-back",
                        locale.tr("history.back"),
                        Some(NcIcon::Left),
                        true,
                        |this, now| this.history.back(now),
                    ))
                    .child(nav(
                        "history-forward",
                        locale.tr("history.forward"),
                        Some(NcIcon::Right),
                        model.can_forward,
                        |this, now| this.history.forward(now),
                    ))
                    .child(nav(
                        "history-now",
                        locale.tr("history.now"),
                        None,
                        !model.now_active,
                        |this, _| this.history.go_now(),
                    ))
                    .child(nav(
                        "history-reset",
                        locale.tr("history.reset_zoom"),
                        None,
                        model.can_reset_zoom,
                        |this, _| this.history.reset_zoom(),
                    )),
            );

        let mut content = page().child(toolbar);
        if self.history_progress() {
            content = content.child(
                gpui_component::progress::Progress::new("history-progress")
                    .loading(true)
                    .color(p.accent),
            );
        }
        // A state without a chart keeps the place of the chart: the text is in its middle.
        let placeholder = |text: String, more: Vec<AnyElement>| {
            card(p).child(
                v_flex()
                    .w_full()
                    .h(px(CHART_HEIGHT + 36.))
                    .p(px(18.))
                    .gap(px(9.))
                    .items_center()
                    .justify_center()
                    .text_color(p.fg_dim)
                    .child(div().text_center().child(text))
                    .children(more),
            )
        };

        match model.state {
            HistoryState::Empty => {
                content = content.child(placeholder(model.empty_text.clone(), Vec::new()));
            }
            HistoryState::DatabaseError => {
                let mut more: Vec<AnyElement> = model
                    .error_path
                    .clone()
                    .map(|text| caption(text, p).into_any_element())
                    .into_iter()
                    .collect();
                more.push(
                    Self::button("history-retry", locale.tr("history.retry"))
                        .on_click(act(&view, |this, _, cx| this.retry_history(cx)))
                        .into_any_element(),
                );
                content = content.child(placeholder(model.error_text.clone(), more));
            }
            HistoryState::NoData => {
                content = content.child(placeholder(model.no_data_text.clone(), Vec::new()));
            }
            HistoryState::Ready => {
                // Each chart is a card: the plot, then the legend rows of its series.
                for (index, chart) in model.charts.iter().enumerate() {
                    let mut chart_card = card(p).child(
                        div()
                            .px(px(18.))
                            .pt(px(12.))
                            .pb(px(12.))
                            .child(self.history_chart(index, chart, &model, p, cx)),
                    );
                    for series_index in &chart.series {
                        if let Some(series) = model.series.get(*series_index) {
                            chart_card =
                                chart_card.child(self.legend_row(*series_index, series, p, cx));
                        }
                    }
                    content = content.child(chart_card);
                }
                if let Some(hint) = &model.aggregate_hint {
                    content = content.child(caption(hint.clone(), p));
                }
            }
        }
        content.into_any_element()
    }

    fn legend_row(
        &mut self,
        index: usize,
        series: &SeriesModel,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let sensor = series.sensor.clone();
        let color = p.series[series.color];
        h_flex()
            .id(("legend", index))
            .w_full()
            .min_h(px(30.))
            .px(px(18.))
            .py(px(3.))
            .gap(px(9.))
            .items_center()
            .border_t_1()
            .border_color(p.border)
            .cursor_pointer()
            .hover(|row| row.bg(p.hover))
            .opacity(if series.hidden { 0.4 } else { 1.0 })
            .on_click(move |_, _, cx| {
                let sensor = sensor.clone();
                view.update(cx, |this, cx| {
                    this.history.toggle_series(&sensor);
                    cx.notify();
                })
            })
            .child(
                div()
                    .w(px(16.))
                    .h(px(3.))
                    .flex_none()
                    .rounded_full()
                    .bg(color),
            )
            .child(heading(series.legend.name.clone()).flex_1().truncate())
            .child(div().flex_none().child(series.legend.value.clone()))
            .child(caption(series.legend.extremes.clone(), p).flex_none())
            .into_any_element()
    }

    /// One chart: the axes, the series, the gaps, the markers and the cursor.
    fn history_chart(
        &mut self,
        index: usize,
        chart: &ChartModel,
        model: &HistoryModel,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let (from, to) = model.window;
        let span = (to - from).max(1) as f32;
        let colors = p.series;
        let (grid, axis_color, dash_color, hatch_color, alarm, warning, cursor_color) = (
            p.border,
            p.border_strong,
            p.fg_dim,
            p.border_strong,
            p.alarm,
            p.warning,
            p.fg,
        );
        let series: Vec<(SeriesModel, usize)> = chart
            .series
            .iter()
            .map(|i| (model.series[*i].clone(), *i))
            .collect();
        let axes = chart.axes.clone();
        let gaps = model.gaps.clone();
        let markers: Vec<_> = model.markers.iter().map(|m| (m.t, m.severity)).collect();
        let hover = self.hover;
        let drag_start = match self.drag {
            Some(Drag::HistoryZoom { start }) => Some(start),
            _ => None,
        };
        let cell = self.plot_bounds.clone();
        let first_chart = index == 0;

        let paint = canvas(
            {
                let cell = cell.clone();
                move |bounds: Bounds<Pixels>, _, _| {
                    if first_chart {
                        cell.set(bounds);
                    }
                }
            },
            move |bounds: Bounds<Pixels>, _, window, _| {
                let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                let x_of = |t: i64| ((t - from) as f32 / span * w).clamp(0.0, w);
                for step in 0..4 {
                    let y = h * step as f32 / 4.0;
                    polyline(
                        window,
                        &[at(bounds.origin, 0.0, y), at(bounds.origin, w, y)],
                        1.0,
                        grid,
                        None,
                    );
                }
                polyline(
                    window,
                    &[at(bounds.origin, 0.0, h), at(bounds.origin, w, h)],
                    1.0,
                    axis_color,
                    None,
                );
                for (start, end) in &gaps {
                    let region = Bounds::new(
                        at(bounds.origin, x_of(*start), 0.0),
                        size(px((x_of(*end) - x_of(*start)).max(1.0)), px(h)),
                    );
                    hatch(window, region, 8.0, hatch_color.opacity(0.5));
                }
                for (t, severity) in &markers {
                    let color = if *severity == Severity::Alarm {
                        alarm
                    } else {
                        warning
                    };
                    polyline(
                        window,
                        &[
                            at(bounds.origin, x_of(*t), 0.0),
                            at(bounds.origin, x_of(*t), h),
                        ],
                        1.5,
                        color,
                        None,
                    );
                }
                for (model_series, _) in &series {
                    if model_series.hidden {
                        continue;
                    }
                    let Some(axis) = axes.iter().find(|a| a.unit == model_series.unit) else {
                        continue;
                    };
                    let y_of = |v: f64| h - (((v - axis.min) / (axis.max - axis.min)) as f32 * h);
                    let color = colors[model_series.color];
                    for segment in &model_series.segments {
                        if model_series.resolution == Resolution::Minutes && segment.len() > 1 {
                            // The band from the minimum to the maximum, at 20 % opacity.
                            let mut outline: Vec<_> = segment
                                .iter()
                                .map(|pt| at(bounds.origin, x_of(pt.t), y_of(pt.max)))
                                .collect();
                            outline.extend(
                                segment
                                    .iter()
                                    .rev()
                                    .map(|pt| at(bounds.origin, x_of(pt.t), y_of(pt.min))),
                            );
                            polygon(window, &outline, color.opacity(0.2));
                        }
                        let trace: Vec<_> = segment
                            .iter()
                            .map(|pt| at(bounds.origin, x_of(pt.t), y_of(pt.mean)))
                            .collect();
                        polyline(
                            window,
                            &trace,
                            2.0,
                            color,
                            model_series.dashed.then_some((6.0, 4.0)),
                        );
                    }
                }
                if let Some(fraction) = hover {
                    let x = fraction * w;
                    polyline(
                        window,
                        &[at(bounds.origin, x, 0.0), at(bounds.origin, x, h)],
                        1.0,
                        cursor_color,
                        Some((3.0, 3.0)),
                    );
                }
                if let (Some(start), Some(now)) = (drag_start, hover) {
                    let (a, b) = (start.min(now) * w, start.max(now) * w);
                    let selection = Bounds::new(at(bounds.origin, a, 0.0), size(px(b - a), px(h)));
                    rect(window, selection, 0.0, dash_color.opacity(0.15), None);
                }
            },
        )
        .absolute()
        .size_full();

        let mut area = div()
            .id(("history-area", index))
            .absolute()
            .left(px(LEFT))
            .right(px(RIGHT))
            .top(px(TOP))
            .bottom(px(BOTTOM))
            .child(paint);

        let mut frame = div()
            .relative()
            .w_full()
            .h(px(CHART_HEIGHT))
            .text_size(px(12.))
            .line_height(px(16.))
            .text_color(p.fg_dim);
        // The y axes: one for each unit, at most two.
        for axis in &chart.axes {
            // A narrow range needs a decimal: two ticks never show the same number.
            let digits = usize::from(axis.max - axis.min < 8.0);
            for step in 0..=4 {
                let value = axis.max - (axis.max - axis.min) * step as f64 / 4.0;
                let top = TOP - 8.0 + (CHART_HEIGHT - TOP - BOTTOM) * step as f32 / 4.0;
                let label = div()
                    .absolute()
                    .top(px(top))
                    .w(px(LEFT - 8.0))
                    .child(format!("{value:.digits$}"));
                frame = frame.child(match axis.side {
                    Side::Left => label.left(px(0.)).text_right(),
                    Side::Right => label.right(px(0.)),
                });
            }
            let title = div().absolute().top(px(0.)).child(axis.unit_text.clone());
            frame = frame.child(match axis.side {
                Side::Left => title.left(px(LEFT + 6.0)),
                Side::Right => title.right(px(RIGHT + 6.0)),
            });
        }
        // The x axis: 5 times.
        for step in 0..=4 {
            let t = from + (to - from) * step / 4;
            let label = if to - from > 2 * 24 * 3600 {
                crate::screens::format::date_time(t as u64 * 1_000_000)
            } else {
                clock(t as u64 * 1_000_000)
            };
            frame = frame.child(
                div()
                    .absolute()
                    .left(px(LEFT))
                    .right(px(RIGHT))
                    .bottom(px(4.))
                    .child(
                        div()
                            .absolute()
                            .left(gpui::relative(step as f32 / 4.0))
                            .ml(px(if step == 4 { -60.0 } else { 0.0 }))
                            .child(label),
                    ),
            );
        }
        // The label of a gap, and the icons of the markers.
        if !model.gaps.is_empty() {
            frame = frame.child(
                div()
                    .absolute()
                    .left(px(LEFT + 8.0))
                    .top(px(TOP + 22.0))
                    .text_color(p.fg_dim)
                    .child(model.gap_label.clone()),
            );
        }
        for (marker_index, marker) in model.markers.iter().enumerate() {
            let fraction = ((marker.t - from) as f32 / span).clamp(0.0, 1.0);
            let instance = marker.instance.clone();
            let view = view.clone();
            frame = frame.child(
                div()
                    .id(("marker", marker_index))
                    .absolute()
                    .left(px(LEFT))
                    .right(px(RIGHT))
                    .top(px(0.))
                    .child(
                        div()
                            .absolute()
                            .left(gpui::relative(fraction))
                            .ml(px(-6.))
                            .cursor_pointer()
                            .child(severity_icon(marker.severity, p))
                            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                let _ = &instance;
                                view.update(cx, |this, cx| this.go(crate::shell::Page::Alarms, cx));
                            }),
                    ),
            );
        }

        let (window_from, window_to) = (from, to);
        area = area
            .on_mouse_move({
                let view = view.clone();
                move |event: &MouseMoveEvent, _, cx| {
                    view.update(cx, |this, cx| {
                        this.history_pointer(event.position, window_from, window_to, cx)
                    });
                }
            })
            .on_mouse_down(MouseButton::Left, {
                let view = view.clone();
                move |event: &MouseDownEvent, window, cx| {
                    let position = event.position;
                    let shift = event.modifiers.shift;
                    let double = event.click_count == 2;
                    view.update(cx, |this, cx| {
                        let bounds = this.plot_bounds.get();
                        let fraction = (f32::from(position.x - bounds.origin.x)
                            / f32::from(bounds.size.width).max(1.0))
                        .clamp(0.0, 1.0);
                        if double {
                            this.history.reset_zoom();
                        } else if shift {
                            this.drag = Some(Drag::HistoryPan {
                                start: fraction,
                                from: window_from,
                                to: window_to,
                            });
                        } else {
                            this.drag = Some(Drag::HistoryZoom { start: fraction });
                        }
                        if let Some(focus) = &this.chart_focus {
                            window.focus(focus, cx);
                        }
                        cx.notify();
                    });
                }
            })
            .on_mouse_up(MouseButton::Left, {
                let view = view.clone();
                move |event: &MouseUpEvent, _, cx| {
                    let position = event.position;
                    view.update(cx, |this, cx| {
                        this.end_history_drag(position, window_from, window_to, cx)
                    });
                }
            })
            .on_scroll({
                let view = view.clone();
                move |event: &ScrollEvent, _, cx| {
                    let delta = event.delta.pixel_delta(px(16.)).y;
                    let position = event.position;
                    view.update(cx, |this, cx| {
                        let bounds = this.plot_bounds.get();
                        let fraction = f32::from(position.x - bounds.origin.x)
                            / f32::from(bounds.size.width).max(1.0);
                        let around = window_from
                            + ((window_to - window_from) as f32 * fraction.clamp(0.0, 1.0)) as i64;
                        let factor = if f32::from(delta) < 0.0 { 0.8 } else { 1.25 };
                        this.history.zoom_around(
                            (crate::app::now_us() / 1_000_000) as i64,
                            around,
                            factor,
                        );
                        cx.notify();
                    });
                }
            });
        if first_chart && let Some(focus) = self.chart_focus.clone() {
            area = area.track_focus(&focus).on_key_down({
                let view = view.clone();
                move |event: &KeyDownEvent, _, cx| {
                    view.update(cx, |this, cx| this.history_key(event, cx))
                }
            });
        }
        frame.child(area).into_any_element()
    }

    fn history_pointer(
        &mut self,
        position: gpui::Point<Pixels>,
        from: i64,
        to: i64,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.plot_bounds.get();
        let fraction = (f32::from(position.x - bounds.origin.x)
            / f32::from(bounds.size.width).max(1.0))
        .clamp(0.0, 1.0);
        self.hover = Some(fraction);
        self.history.cursor = Some(from + ((to - from) as f32 * fraction) as i64);
        if let Some(Drag::HistoryPan {
            start,
            from: pan_from,
            to: pan_to,
        }) = self.drag
        {
            let shift = ((fraction - start) * (pan_to - pan_from) as f32) as i64;
            self.history.zoom = Some((pan_from - shift, pan_to - shift));
        }
        cx.notify();
    }

    fn end_history_drag(
        &mut self,
        position: gpui::Point<Pixels>,
        from: i64,
        to: i64,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.plot_bounds.get();
        let fraction = (f32::from(position.x - bounds.origin.x)
            / f32::from(bounds.size.width).max(1.0))
        .clamp(0.0, 1.0);
        if let Some(Drag::HistoryZoom { start }) = self.drag.take()
            && (fraction - start).abs() > 0.01
        {
            let span = (to - from) as f32;
            let (a, b) = (start.min(fraction), start.max(fraction));
            self.history
                .zoom_to(from + (span * a) as i64, from + (span * b) as i64);
        }
        self.drag = None;
        cx.notify();
    }

    /// Left and Right move the cursor by one sample. Plus and Minus zoom. Home goes to the
    /// start of the range, End to now.
    fn history_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let now = (crate::app::now_us() / 1_000_000) as i64;
        let (from, to) = self.history.window(now);
        let step = 1;
        match event.keystroke.key.as_str() {
            "left" => self.history.cursor = Some(self.history.cursor.unwrap_or(to) - step),
            "right" => self.history.cursor = Some(self.history.cursor.unwrap_or(from) + step),
            "+" | "=" => self.history.zoom_around(now, (from + to) / 2, 0.8),
            "-" => self.history.zoom_around(now, (from + to) / 2, 1.25),
            "home" => self.history.cursor = Some(from),
            "end" => self.history.go_now(),
            _ => return,
        }
        cx.notify();
    }
}
