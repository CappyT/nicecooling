//! The band diagram of the Pressure step: the three lines of the SHIELD control band, the
//! zones around them and the live pressure (`docs/ui/widgets.md`, "Pressure band diagram").

use gpui::prelude::*;
use gpui::{AnyElement, Bounds, Div, FontWeight, Pixels, SharedString, canvas, div, px, size};
use gpui_component::{h_flex, v_flex};

use crate::screens::leakshield::settings::{BAND_LINES, BandDiagram};
use crate::screens::ui::caption;
use crate::tokens::Palette;
use crate::widgets::paint::{at, disc, hatch, polyline, rect};

/// The height of the diagram in px.
const HEIGHT: f32 = 184.0;
/// The center of the marker of the live pressure, from the right edge of the plot.
const MARKER_INSET: f32 = 20.0;

/// The text of a zone on a plate, so that the hatch does not cross it. `place` is the center
/// of the zone from the top of the diagram (0 to 1).
fn zone_label(text: &str, place: f32, name: &'static str, p: &Palette) -> Div {
    div()
        .absolute()
        .top(px(place * HEIGHT - 11.))
        .left(px(12.))
        // The marker and its text have the right part of the plot.
        .right(px(150.))
        .flex()
        .child(
            div()
                .min_w_0()
                .h(px(22.))
                .px(px(6.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .bg(p.card)
                .text_size(px(12.))
                .line_height(px(16.))
                .debug_selector(move || name.to_owned())
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .child(SharedString::from(text.to_owned())),
                ),
        )
}

/// The diagram: the plot at the left and the values of the three lines at the right.
pub fn band_diagram(model: &BandDiagram, p: &Palette) -> AnyElement {
    let (band, ink, accent, card, hatching) = (
        p.accent_tint,
        p.fg,
        p.accent,
        p.card,
        p.fg_dim.opacity(0.45),
    );
    let marker = model.marker.as_ref().map(|(place, _)| *place);
    let paint = canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let o = bounds.origin;
            let w = f32::from(bounds.size.width);
            let [top, middle, bottom] = BAND_LINES.map(|line| (line * HEIGHT).round());
            // The band between the limits, and the hatched zones outside it.
            rect(
                window,
                Bounds::new(at(o, 0.0, top), size(px(w), px(bottom - top))),
                0.0,
                band,
                None,
            );
            for (from, to) in [(0.0, top), (bottom, HEIGHT)] {
                hatch(
                    window,
                    Bounds::new(at(o, 0.0, from), size(px(w), px(to - from))),
                    8.0,
                    hatching,
                );
            }
            for (y, width, color) in [(top, 1.5, ink), (middle, 2.0, accent), (bottom, 1.5, ink)] {
                polyline(window, &[at(o, 0.0, y), at(o, w, y)], width, color, None);
            }
            if let Some(place) = marker {
                disc(
                    window,
                    at(o, w - MARKER_INSET, place * HEIGHT),
                    6.0,
                    ink,
                    Some((card, 1.5, false)),
                );
            }
        },
    )
    .absolute()
    .top_0()
    .left_0()
    .size_full();

    let mut plot = div()
        .relative()
        .flex_1()
        .min_w_0()
        .h(px(HEIGHT))
        .debug_selector(|| "ls-band-plot".to_owned())
        .child(paint);
    let [top, middle, bottom] = BAND_LINES;
    for (text, place, name) in [
        (&model.zones[0], top / 2.0, "ls-band-zone-above"),
        (
            &model.zones[1],
            (middle + bottom) / 2.0,
            "ls-band-zone-inside",
        ),
        (&model.zones[2], (bottom + 1.0) / 2.0, "ls-band-zone-below"),
    ] {
        plot = plot.child(zone_label(text, place, name, p));
    }
    if let Some((place, text)) = &model.marker {
        plot = plot.child(
            div()
                .absolute()
                .top(px(place * HEIGHT - 11.))
                .right(px(MARKER_INSET + 12.))
                .h(px(22.))
                .px(px(6.))
                .flex()
                .items_center()
                .rounded(px(6.))
                .bg(p.card)
                .font_weight(FontWeight::BOLD)
                .whitespace_nowrap()
                .debug_selector(|| "ls-band-marker".to_owned())
                .child(text.clone()),
        );
    }

    // The values of the lines: each one starts 10 px above its line, so that its first text
    // line is on the line of the plot.
    let cell = HEIGHT * (middle - top);
    let mut values = v_flex()
        .flex_none()
        .pt(px(top * HEIGHT - 10.))
        .whitespace_nowrap()
        .debug_selector(|| "ls-band-values".to_owned());
    for (index, line) in model.lines.iter().enumerate() {
        values = values.child(
            v_flex()
                .h(px(cell))
                .child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .when(index == 1, |label| label.text_color(p.accent))
                        .child(line.label.clone()),
                )
                .children(line.now.clone().map(|now| caption(now, p))),
        );
    }
    h_flex()
        .w_full()
        .items_start()
        .gap(px(12.))
        .child(plot)
        .child(values)
        .into_any_element()
}
