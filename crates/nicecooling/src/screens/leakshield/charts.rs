//! The charts of the Leakshield tabs: the pressure gauge, a line over time, the frame of the
//! pump runtime chart and a row of bands. Their geometry comes from the comps in
//! `docs/ui/comps/`.

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, FontWeight, Hsla, Pixels, Point, ShapedLine, SharedString, TextAlign,
    TextRun, Window, canvas, point, px, size,
};

use crate::model::PressureLimits;
use crate::tokens::Palette;
use crate::widgets::paint::{at, polygon, polyline, rect};

/// The scale of the pressure gauge in mbar.
const GAUGE_MAX: f64 = 450.0;
/// The ticks of the gauge scale in mbar.
const GAUGE_TICKS: [f64; 6] = [0.0, 100.0, 200.0, 300.0, 400.0, 450.0];

/// One line of 12 px text for a canvas, in the font of the element around it.
pub(crate) fn shape(window: &Window, text: &str, color: Hsla, bold: bool) -> ShapedLine {
    let mut font = window.text_style().font();
    if bold {
        font.weight = FontWeight::BOLD;
    }
    shape_with(window, text, color, font, 12.0)
}

/// One line of text for a canvas in `font` at `size` px.
pub(crate) fn shape_with(
    window: &Window,
    text: &str,
    color: Hsla,
    font: gpui::Font,
    size: f32,
) -> ShapedLine {
    let text = SharedString::from(text.to_owned());
    let run = TextRun {
        len: text.len(),
        font,
        color,
        ..Default::default()
    };
    window
        .text_system()
        .shape_line(text, px(size), &[run], None)
}

/// Paint a line of text. `anchor` says which part of the line is at `x`: 0 its left, 0.5 its
/// center, 1 its right. `y` is the top of the 16 px line. Returns the left and the right.
pub(crate) fn paint_text(
    window: &mut Window,
    cx: &mut App,
    line: &ShapedLine,
    origin: Point<Pixels>,
    x: f32,
    y: f32,
    anchor: f32,
) -> (f32, f32) {
    let width = f32::from(line.width);
    let left = (x - width * anchor).round();
    let _ = line.paint(
        at(origin, left, y),
        px(16.),
        TextAlign::Left,
        None,
        window,
        cx,
    );
    (left, left + width)
}

/// The texts of the gauge: "Minimum 250", "Maximum 350", "Target 300".
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GaugeLabels {
    pub minimum: String,
    pub maximum: String,
    pub target: String,
}

/// A horizontal scale from 0 to 450 mbar: the band of the limits, the target and the marker.
/// `marker` is the color of the marker of the current value.
pub fn gauge(
    value: Option<f64>,
    limits: Option<PressureLimits>,
    marker: Hsla,
    labels: GaugeLabels,
    p: &Palette,
) -> AnyElement {
    let (track, band, accent, card, dim, axis) = (
        p.track,
        p.accent_tint,
        p.accent,
        p.card,
        p.fg_dim,
        p.border_strong,
    );
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, cx| {
            let o = bounds.origin;
            let w = f32::from(bounds.size.width);
            // The track is 540 units of the comp and the scale 520: the track runs on after
            // the last tick.
            let track_w = (w - 40.0).max(1.0);
            let scale_w = track_w * 520.0 / 540.0;
            let x = |v: f64| 20.0 + (v / GAUGE_MAX).clamp(0.0, 1.0) as f32 * scale_w;
            rect(
                window,
                Bounds::new(at(o, 20.0, 26.0), size(px(track_w), px(10.))),
                5.0,
                track,
                None,
            );
            let mut target_span = None;
            if let Some(limits) = limits {
                rect(
                    window,
                    Bounds::new(
                        at(o, x(limits.min), 26.0),
                        size(px(x(limits.max) - x(limits.min)), px(10.)),
                    ),
                    0.0,
                    band,
                    Some((accent, 1.0, false)),
                );
                rect(
                    window,
                    Bounds::new(at(o, x(limits.target) - 1.0, 20.0), size(px(2.), px(22.))),
                    0.0,
                    accent,
                    None,
                );
                // The two limit labels sit over their limit. On a narrow band they move
                // apart, so that they do not run into each other.
                let middle = (x(limits.min) + x(limits.max)) / 2.0;
                let minimum = shape(window, &labels.minimum, dim, false);
                let maximum = shape(window, &labels.maximum, dim, false);
                let min_right = (x(limits.min) + f32::from(minimum.width) / 2.0).min(middle - 6.0);
                let max_left = (x(limits.max) - f32::from(maximum.width) / 2.0).max(middle + 6.0);
                paint_text(window, cx, &minimum, o, min_right, 1.0, 1.0);
                paint_text(window, cx, &maximum, o, max_left, 1.0, 0.0);
                let target = shape(window, &labels.target, accent, true);
                target_span = Some(paint_text(
                    window,
                    cx,
                    &target,
                    o,
                    x(limits.target),
                    51.0,
                    0.5,
                ));
            }
            for tick in GAUGE_TICKS {
                polyline(
                    window,
                    &[at(o, x(tick), 48.0), at(o, x(tick), 53.0)],
                    1.0,
                    axis,
                    None,
                );
                let text = shape(window, &format!("{tick:.0}"), dim, false);
                let half = f32::from(text.width) / 2.0;
                // The target label takes the place of a scale label that it covers.
                let covered = target_span.is_some_and(|(left, right)| {
                    x(tick) + half + 6.0 > left && x(tick) - half - 6.0 < right
                });
                if !covered {
                    paint_text(window, cx, &text, o, x(tick), 51.0, 0.5);
                }
            }
            if let Some(value) = value {
                rect(
                    window,
                    Bounds::new(at(o, x(value) - 5.0, 14.0), size(px(10.), px(34.))),
                    4.0,
                    card,
                    None,
                );
                rect(
                    window,
                    Bounds::new(at(o, x(value) - 3.0, 16.0), size(px(6.), px(30.))),
                    2.0,
                    marker,
                    None,
                );
            }
        },
    )
    .w_full()
    .h(px(68.))
    .into_any_element()
}

/// A horizontal line of a chart at `value`.
#[derive(Clone, Debug, PartialEq)]
pub struct Reference {
    pub value: f64,
    pub color: Hsla,
    /// The dash and the gap in px.
    pub dash: (f32, f32),
    /// A text over the left end of the line.
    pub label: Option<String>,
}

/// The runs of `(time, value)` of a chart. A gap in the data ends a run.
pub type Runs = Vec<Vec<(f64, f64)>>;

/// What a chart over time shows.
#[derive(Clone, Debug, Default)]
pub struct TimeChart {
    pub runs: Runs,
    /// The first and the last time of the x axis. `None` takes them from the points.
    pub domain: Option<(f64, f64)>,
    pub references: Vec<Reference>,
    /// The fill under the line.
    pub area: bool,
    /// The labels under the x axis: the time and the text.
    pub x_labels: Vec<(f64, String)>,
    /// The value labels at the left of the grid lines.
    pub y_labels: bool,
    /// A caption at the top right of the plot.
    pub note: Option<String>,
    /// The text in the plot while it has no point.
    pub empty: Option<String>,
    pub height: f32,
}

/// A step of 1, 2 or 5 times a power of ten that gives about three grid lines.
fn nice_step(span: f64) -> f64 {
    let raw = (span / 3.0).max(1e-9);
    let power = 10f64.powf(raw.log10().floor());
    let unit = raw / power;
    power
        * if unit <= 1.0 {
            1.0
        } else if unit <= 2.0 {
            2.0
        } else if unit <= 5.0 {
            5.0
        } else {
            10.0
        }
}

/// The lowest grid value, the step and the number of steps of a value axis for `low..high`.
pub fn value_axis(low: f64, high: f64) -> (f64, f64, usize) {
    let (low, high) = if high - low < 1e-9 {
        (low - 1.0, high + 1.0)
    } else {
        (low, high)
    };
    let step = nice_step(high - low);
    let first = (low / step).floor() * step;
    let count = (((high - first) / step).ceil() as usize).max(1);
    (first, step, count)
}

fn tick_text(value: f64, step: f64) -> String {
    if step >= 1.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

/// A line over time on a grid, with its axis and its labels.
pub fn time_chart(chart: TimeChart, color: Hsla, p: &Palette) -> AnyElement {
    let (grid, axis, dim) = (p.border, p.border_strong, p.fg_dim);
    let height = chart.height;
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, cx| {
            let o = bounds.origin;
            let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            // The frame of `leakshield-overview.html` with value labels, and of
            // `leakshield-status.html` without them.
            let (left, right, top, bottom) = if chart.y_labels {
                (36.0, w - 4.0, 20.0, h - 30.0)
            } else {
                (0.0, w, 12.0, h - 16.0)
            };
            let points = chart.runs.iter().flatten();
            let (t0, t1) = chart.domain.unwrap_or_else(|| {
                points
                    .clone()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), (t, _)| {
                        (lo.min(*t), hi.max(*t))
                    })
            });
            let values = points
                .map(|(_, v)| *v)
                .chain(chart.references.iter().map(|r| r.value));
            let (low, high) = values.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(v), hi.max(v))
            });
            let (first, step, count) = if low.is_finite() {
                value_axis(low, high)
            } else {
                (0.0, 1.0, 3)
            };
            let last = first + step * count as f64;
            let y = |v: f64| bottom - ((v - first) / (last - first)) as f32 * (bottom - top);
            let x = |t: f64| {
                if t1 > t0 {
                    left + ((t - t0) / (t1 - t0)).clamp(0.0, 1.0) as f32 * (right - left)
                } else {
                    left
                }
            };
            let line = |window: &mut Window, value: f64, color: Hsla, dash| {
                polyline(
                    window,
                    &[at(o, left, y(value)), at(o, right, y(value))],
                    1.0,
                    color,
                    dash,
                );
            };

            for index in 1..=count {
                let value = first + step * index as f64;
                line(window, value, grid, None);
                if chart.y_labels && low.is_finite() {
                    let text = shape(window, &tick_text(value, step), dim, false);
                    paint_text(window, cx, &text, o, 30.0, y(value) - 9.0, 1.0);
                }
            }
            line(window, first, axis, None);
            for reference in &chart.references {
                line(
                    window,
                    reference.value,
                    reference.color,
                    Some(reference.dash),
                );
                if let Some(label) = &reference.label {
                    let text = shape(window, label, dim, false);
                    paint_text(
                        window,
                        cx,
                        &text,
                        o,
                        left + 4.0,
                        y(reference.value) - 17.0,
                        0.0,
                    );
                }
            }
            for run in &chart.runs {
                let trace: Vec<_> = run.iter().map(|(t, v)| at(o, x(*t), y(*v))).collect();
                if chart.area
                    && let (Some(start), Some(end)) = (run.first(), run.last())
                    && run.len() > 1
                {
                    let mut shape = trace.clone();
                    shape.push(at(o, x(end.0), bottom));
                    shape.push(at(o, x(start.0), bottom));
                    polygon(window, &shape, color.opacity(0.12));
                }
                polyline(window, &trace, 2.0, color, None);
            }
            if chart.runs.iter().all(Vec::is_empty)
                && let Some(empty) = &chart.empty
            {
                let text = shape(window, empty, dim, false);
                paint_text(
                    window,
                    cx,
                    &text,
                    o,
                    (left + right) / 2.0,
                    (top + bottom) / 2.0 - 8.0,
                    0.5,
                );
            }
            if let Some(note) = &chart.note {
                let text = shape(window, note, dim, false);
                paint_text(window, cx, &text, o, right - 4.0, top + 3.0, 1.0);
            }
            // The labels of the x axis: the first at the left edge, the last at the right
            // edge, the others centered on their time when they have room.
            let mut taken: Vec<(f32, f32)> = Vec::new();
            let count = chart.x_labels.len();
            let ends = chart
                .x_labels
                .first()
                .into_iter()
                .chain(chart.x_labels.last().filter(|_| count > 1));
            let inner = chart.x_labels.iter().skip(1).take(count.saturating_sub(2));
            for (index, (time, label)) in ends.chain(inner).enumerate() {
                let text = shape(window, label, dim, false);
                let width = f32::from(text.width);
                let (anchor, at_x) = match index {
                    0 => (0.0, left),
                    1 if count > 1 => (1.0, right),
                    _ => (0.5, x(*time)),
                };
                let span = (at_x - width * anchor, at_x + width * (1.0 - anchor));
                if taken
                    .iter()
                    .any(|(l, r)| span.0 < r + 12.0 && span.1 > l - 12.0)
                {
                    continue;
                }
                paint_text(window, cx, &text, o, at_x, bottom + 5.0, anchor);
                taken.push(span);
            }
        },
    )
    .w_full()
    .h(px(height))
    .into_any_element()
}

/// One bar of the pump runtime chart.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeBar {
    /// The text under the bar.
    pub label: String,
    pub seconds: f64,
    pub color: Hsla,
    /// The value shows over the bar: the color alone does not say that a day is high.
    pub flagged: bool,
}

/// The pump runtime chart: one bar for each day, in seconds, with the dashed limit line of the
/// device. `empty` shows while no day has a runtime.
pub fn runtime_chart(
    bars: Vec<RuntimeBar>,
    limit: Option<(f64, String)>,
    empty: Option<String>,
    p: &Palette,
) -> AnyElement {
    let (grid, axis, dim, warning) = (p.border, p.border_strong, p.fg_dim, p.warning);
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, cx| {
            let o = bounds.origin;
            let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let (left, right, top, bottom) = (36.0, w - 4.0, 20.0, h - 30.0);
            let highest = bars
                .iter()
                .map(|bar| bar.seconds)
                .chain(limit.iter().map(|(seconds, _)| *seconds))
                .fold(5.0, f64::max);
            let (_, step, count) = value_axis(0.0, highest);
            let last = step * count as f64;
            let y = |seconds: f64| bottom - (seconds / last) as f32 * (bottom - top);
            for index in 0..=count {
                let seconds = step * index as f64;
                let color = if index == 0 { axis } else { grid };
                polyline(
                    window,
                    &[at(o, left, y(seconds)), at(o, right, y(seconds))],
                    1.0,
                    color,
                    None,
                );
                let label = if index == count {
                    format!("{} s", tick_text(seconds, step))
                } else {
                    tick_text(seconds, step)
                };
                let text = shape(window, &label, dim, false);
                paint_text(window, cx, &text, o, 30.0, y(seconds) - 9.0, 1.0);
            }
            let slot = (right - left) / bars.len().max(1) as f32;
            for (index, bar) in bars.iter().enumerate() {
                let centre = left + slot * (index as f32 + 0.5);
                if bar.seconds > 0.0 {
                    let top_y = y(bar.seconds);
                    rect(
                        window,
                        Bounds::new(
                            at(o, centre - slot * 0.3, top_y),
                            size(px(slot * 0.6), px(bottom - top_y)),
                        ),
                        2.0,
                        bar.color,
                        None,
                    );
                    if bar.flagged {
                        let text = shape(window, &format!("{:.0}", bar.seconds), dim, false);
                        paint_text(window, cx, &text, o, centre, top_y - 17.0, 0.5);
                    }
                }
                let text = shape(window, &bar.label, dim, false);
                paint_text(window, cx, &text, o, centre, bottom + 5.0, 0.5);
            }
            if let Some((seconds, label)) = &limit {
                polyline(
                    window,
                    &[at(o, left, y(*seconds)), at(o, right, y(*seconds))],
                    1.0,
                    warning,
                    Some((5.0, 4.0)),
                );
                let text = shape(window, label, dim, false);
                paint_text(window, cx, &text, o, right - 4.0, y(*seconds) - 19.0, 1.0);
            }
            if let Some(empty) = &empty {
                let text = shape(window, empty, dim, false);
                paint_text(
                    window,
                    cx,
                    &text,
                    o,
                    (left + right) / 2.0,
                    (top + bottom) / 2.0 - 8.0,
                    0.5,
                );
            }
        },
    )
    .w_full()
    .h(px(170.))
    .into_any_element()
}

/// Colored bands over time: `(start, color)` in order. The last band ends at `end`.
pub fn bands(items: Vec<(f64, Hsla)>, end: f64, height: f32) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let Some(first) = items.first().map(|i| i.0) else {
                return;
            };
            let span = (end - first).max(1.0);
            for (index, (start, color)) in items.iter().enumerate() {
                let stop = items.get(index + 1).map_or(end, |next| next.0);
                let left = ((start - first) / span) as f32 * w;
                let width = (((stop - start) / span) as f32 * w).max(1.0);
                window.paint_quad(gpui::fill(
                    Bounds::new(
                        point(bounds.origin.x + px(left), bounds.origin.y),
                        size(px(width), px(h)),
                    ),
                    *color,
                ));
            }
        },
    )
    .w_full()
    .h(px(height))
    .into_any_element()
}
