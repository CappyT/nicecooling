//! The small charts of the Leakshield tabs: a gauge, a line over time and a row of bands.

use gpui::prelude::*;
use gpui::{AnyElement, Bounds, Hsla, Pixels, canvas, point, px, size};

use crate::model::PressureLimits;
use crate::tokens::Palette;
use crate::widgets::paint::{at, polygon, polyline, rect};

/// The scale of the pressure gauge in mbar.
const GAUGE_MAX: f32 = 450.0;

/// A horizontal scale from 0 to 450 mbar: the band of the limits, the target and the marker.
pub fn gauge(
    value: Option<f64>,
    limits: Option<PressureLimits>,
    alarm: bool,
    p: &Palette,
) -> AnyElement {
    let (track, band, outline, accent, fg, red) =
        (p.track, p.accent_tint, p.accent, p.accent, p.fg, p.alarm);
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let x = |v: f64| (v as f32 / GAUGE_MAX).clamp(0.0, 1.0) * w;
            let top = h / 2.0 - 5.0;
            rect(
                window,
                Bounds::new(at(bounds.origin, 0.0, top), size(px(w), px(10.))),
                5.0,
                track,
                None,
            );
            if let Some(limits) = limits {
                rect(
                    window,
                    Bounds::new(
                        at(bounds.origin, x(limits.min), top),
                        size(px(x(limits.max) - x(limits.min)), px(10.)),
                    ),
                    3.0,
                    band,
                    Some((outline, 1.0, false)),
                );
                rect(
                    window,
                    Bounds::new(
                        at(bounds.origin, x(limits.target) - 1.0, top - 4.0),
                        size(px(2.), px(18.)),
                    ),
                    0.0,
                    accent,
                    None,
                );
            }
            if let Some(value) = value {
                rect(
                    window,
                    Bounds::new(
                        at(bounds.origin, x(value) - 3.0, h / 2.0 - 15.0),
                        size(px(6.), px(30.)),
                    ),
                    2.0,
                    if alarm { red } else { fg },
                    None,
                );
            }
        },
    )
    .w_full()
    .h(px(44.))
    .into_any_element()
}

/// A line over time. `lines` are horizontal reference lines: value, color, dashed.
pub fn line_chart(
    points: Vec<(f64, f64)>,
    lines: Vec<(f64, Hsla)>,
    color: Hsla,
    area: Option<Hsla>,
    height: f32,
) -> AnyElement {
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            if points.len() < 2 {
                return;
            }
            let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let (t0, t1) = (
                points[0].0,
                points[points.len() - 1].0.max(points[0].0 + 1.0),
            );
            let values = points.iter().map(|p| p.1).chain(lines.iter().map(|l| l.0));
            let (low, high) = values.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(v), hi.max(v))
            });
            let pad = ((high - low) * 0.1).max(1.0);
            let (low, high) = (low - pad, high + pad);
            let y = |v: f64| h - ((v - low) / (high - low)) as f32 * h;
            let x = |t: f64| ((t - t0) / (t1 - t0)) as f32 * w;
            let trace: Vec<_> = points
                .iter()
                .map(|(t, v)| at(bounds.origin, x(*t), y(*v)))
                .collect();
            if let Some(fill) = area {
                let mut shape = trace.clone();
                shape.push(at(bounds.origin, x(points[points.len() - 1].0), h));
                shape.push(at(bounds.origin, x(points[0].0), h));
                polygon(window, &shape, fill);
            }
            for (value, line) in &lines {
                polyline(
                    window,
                    &[
                        at(bounds.origin, 0.0, y(*value)),
                        at(bounds.origin, w, y(*value)),
                    ],
                    1.0,
                    *line,
                    Some((5.0, 4.0)),
                );
            }
            polyline(window, &trace, 2.0, color, None);
        },
    )
    .w_full()
    .h(px(height))
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
