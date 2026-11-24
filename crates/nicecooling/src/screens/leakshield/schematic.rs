//! The device schematic of the Status tab (`docs/ui/widgets.md`, "Device schematic"). It has
//! the notation of the Loop diagram: dashed air lines, part symbols with a 1.5 px outline and
//! bubbles. The geometry comes from `docs/ui/comps/leakshield-status.html`: x in units of
//! 1/1000 of the width, y in px.

use gpui::prelude::*;
use gpui::{AnyElement, App, Bounds, FontWeight, Hsla, Pixels, Point, Window, canvas, px, size};
use gpui_component::ActiveTheme as _;

use crate::model::Severity;
use crate::screens::leakshield::charts::{paint_text, shape, shape_with};
use crate::screens::leakshield::status::{Bubble, SchematicModel};
use crate::tokens::Palette;
use crate::widgets::paint::{at, disc, polygon, polyline, rect};

pub const HEIGHT: f32 = 176.0;

/// The colors of a bubble: fill, outline, dashed, tag, value and caption.
fn bubble_colors(
    severity: Option<Severity>,
    dim: bool,
    p: &Palette,
) -> (Hsla, Hsla, bool, Hsla, Hsla, Hsla) {
    if dim {
        return (p.card, p.offline, true, p.offline, p.offline, p.fg_dim);
    }
    match severity {
        Some(Severity::Alarm) => (p.alarm_bg, p.alarm_bg, false, p.alarm_fg, p.alarm, p.alarm),
        Some(Severity::Warning) => (
            p.warning_bg,
            p.warning_bg,
            false,
            gpui::rgb(0x2A2A2E).into(),
            p.fg,
            p.warning,
        ),
        None => (p.card, p.fg, false, p.fg, p.fg, p.fg_dim),
    }
}

/// A valve: two triangles that meet at `center`.
fn valve(window: &mut Window, center: Point<Pixels>, fill: Hsla, outline: Hsla) {
    let corner = |dx: f32, dy: f32| at(center, dx, dy);
    for triangle in [
        [corner(-20.0, -10.0), corner(0.0, 0.0), corner(-20.0, 10.0)],
        [corner(20.0, -10.0), corner(0.0, 0.0), corner(20.0, 10.0)],
    ] {
        polygon(window, &triangle, fill);
        polyline(
            window,
            &[triangle[0], triangle[1], triangle[2], triangle[0]],
            1.5,
            outline,
            None,
        );
    }
}

fn bubble(
    window: &mut Window,
    cx: &mut App,
    center: Point<Pixels>,
    bubble: &Bubble,
    dim: bool,
    p: &Palette,
) -> (Hsla, Hsla) {
    let (fill, outline, dashed, tag, value, caption) = bubble_colors(bubble.severity, dim, p);
    disc(window, center, 18.0, fill, Some((outline, 1.5, dashed)));
    let mut mono = window.text_style().font();
    mono.family = cx.theme().mono_font_family.clone();
    mono.weight = FontWeight::BOLD;
    let text = shape_with(window, bubble.tag, tag, mono, 12.0);
    paint_text(window, cx, &text, center, 0.0, -8.0, 0.5);
    (value, caption)
}

/// The schematic of one Leakshield.
pub fn element(model: &SchematicModel, p: &Palette) -> AnyElement {
    let model = model.clone();
    let p = Palette::new(p.dark);
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, cx| {
            let o = bounds.origin;
            let w = f32::from(bounds.size.width);
            let x = |units: f32| (units * w / 1000.0).round();
            let dim = model.dim;
            let ink = if dim { p.fg_dim } else { p.fg };
            let active = |key: &str| {
                !dim && model
                    .parts
                    .iter()
                    .any(|part| part.key == key && part.active)
            };
            let label = |key: &str| {
                model
                    .parts
                    .iter()
                    .find(|part| part.key == key)
                    .map_or_else(String::new, |part| part.label.clone())
            };
            let part_colors = |on: bool| {
                if on {
                    (p.accent_tint, p.accent)
                } else {
                    (p.part, ink)
                }
            };
            let (bar, vent, pump_valve, pump, joint, pt, membrane, tank, lt) = (
                x(150.0),
                x(350.0),
                x(270.0),
                x(392.0),
                x(500.0),
                x(600.0),
                x(710.0),
                x(820.0),
                x(926.0).max(x(820.0) + 106.0),
            );

            // The air path.
            let air = |window: &mut Window, points: &[(f32, f32)]| {
                let points: Vec<_> = points.iter().map(|(px, py)| at(o, *px, *py)).collect();
                polyline(window, &points, 2.0, p.air, Some((5.0, 4.0)));
            };
            air(window, &[(bar, 52.0), (vent - 20.0, 52.0)]);
            air(
                window,
                &[
                    (vent + 20.0, 52.0),
                    (joint, 52.0),
                    (joint, 120.0),
                    (pump + 18.0, 120.0),
                ],
            );
            air(window, &[(pump - 18.0, 120.0), (pump_valve + 20.0, 120.0)]);
            air(window, &[(pump_valve - 20.0, 120.0), (bar, 120.0)]);
            air(window, &[(joint, 86.0), (membrane - 20.0, 86.0)]);
            air(window, &[(membrane + 20.0, 86.0), (tank, 86.0)]);

            // The atmosphere bar.
            polyline(
                window,
                &[at(o, bar, 40.0), at(o, bar, 132.0)],
                3.0,
                ink,
                None,
            );
            let text = shape(window, &label("atmosphere"), ink, true);
            paint_text(window, cx, &text, o, bar - 10.0, 77.0, 1.0);

            // The valves.
            for (key, center, y, label_y) in [
                ("vent_valve", vent, 52.0, 17.0),
                ("pump_valve", pump_valve, 120.0, 87.0),
            ] {
                let (fill, outline) = part_colors(active(key));
                valve(window, at(o, center, y), fill, outline);
                let text = shape(window, &label(key), p.fg_dim, false);
                paint_text(window, cx, &text, o, center, label_y, 0.5);
            }

            // The vacuum pump: a circle with a triangle that points to the atmosphere.
            let runs = active("vacuum_pump");
            let (fill, outline) = part_colors(runs);
            disc(
                window,
                at(o, pump, 120.0),
                18.0,
                fill,
                Some((outline, 1.5, false)),
            );
            polygon(
                window,
                &[
                    at(o, pump + 8.0, 110.0),
                    at(o, pump - 8.0, 120.0),
                    at(o, pump + 8.0, 130.0),
                ],
                outline,
            );
            let text = shape(
                window,
                &label("vacuum_pump"),
                if runs { p.accent } else { p.fg_dim },
                runs,
            );
            paint_text(window, cx, &text, o, pump, 145.0, 0.5);

            // The pressure bubble on the line to the membrane.
            if let Some(model_bubble) = model.bubbles.first() {
                polyline(window, &[at(o, pt, 86.0), at(o, pt, 62.0)], 1.0, ink, None);
                let (value, caption) = bubble(window, cx, at(o, pt, 44.0), model_bubble, dim, &p);
                let mut bold = window.text_style().font();
                bold.weight = FontWeight::BOLD;
                let text = if dim {
                    "--"
                } else {
                    model_bubble.value.as_str()
                };
                let line = shape_with(window, text, value, bold, 16.0);
                let _ = line.paint(
                    at(o, pt + 26.0, 24.0),
                    px(22.),
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
                let text = shape(window, &model_bubble.caption, caption, false);
                paint_text(window, cx, &text, o, pt + 26.0, 46.0, 0.0);
            }

            // The membrane.
            rect(
                window,
                Bounds::new(at(o, membrane - 20.0, 70.0), size(px(40.), px(32.))),
                3.0,
                p.part,
                Some((ink, 1.5, false)),
            );
            polyline(
                window,
                &[
                    at(o, membrane - 12.0, 78.0),
                    at(o, membrane - 4.0, 94.0),
                    at(o, membrane + 4.0, 78.0),
                    at(o, membrane + 12.0, 94.0),
                ],
                1.0,
                ink,
                None,
            );
            let text = shape(window, &label("membrane"), p.fg_dim, false);
            paint_text(window, cx, &text, o, membrane, 109.0, 0.5);

            // The reservoir with its fill level, and the level bubble.
            rect(
                window,
                Bounds::new(at(o, tank, 46.0), size(px(76.), px(110.))),
                4.0,
                p.part,
                Some((ink, 1.5, false)),
            );
            if let Some(level) = model.level {
                let height = (108.0 * (level / 100.0).clamp(0.0, 1.0)) as f32;
                rect(
                    window,
                    Bounds::new(at(o, tank + 1.0, 155.0 - height), size(px(74.), px(height))),
                    0.0,
                    p.water.opacity(0.3),
                    None,
                );
            }
            let text = shape(window, &label("reservoir"), p.fg_dim, false);
            paint_text(window, cx, &text, o, tank + 38.0, 159.0, 0.5);
            if let Some(model_bubble) = model.bubbles.get(1) {
                let text = shape(
                    window,
                    if dim {
                        "--"
                    } else {
                        model_bubble.value.as_str()
                    },
                    ink,
                    true,
                );
                paint_text(window, cx, &text, o, tank + 38.0, 111.0, 0.5);
                polyline(
                    window,
                    &[at(o, tank + 76.0, 66.0), at(o, lt - 18.0, 66.0)],
                    1.0,
                    ink,
                    None,
                );
                let (_, caption) = bubble(window, cx, at(o, lt, 66.0), model_bubble, dim, &p);
                let text = shape(window, &model.level_label, p.fg_dim, false);
                paint_text(window, cx, &text, o, lt, 91.0, 0.5);
                let text = shape(window, &model_bubble.caption, caption, false);
                paint_text(window, cx, &text, o, lt, 107.0, 0.5);
            }
        },
    )
    .w_full()
    .h(px(HEIGHT))
    .into_any_element()
}
