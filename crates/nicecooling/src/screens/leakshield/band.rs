//! The band diagram of the Pressure step: the control band of SHIELD on one linear mbar axis,
//! the zones around it and the live pressure (`docs/ui/widgets.md`, "Pressure band diagram").

use gpui::prelude::*;
use gpui::{AnyElement, Bounds, Div, FontWeight, Pixels, SharedString, canvas, div, px, size};
use gpui_component::{h_flex, v_flex};

use crate::screens::leakshield::settings::BandDiagram;
use crate::screens::ui::caption;
use crate::tokens::Palette;
use crate::widgets::paint::{at, disc, hatch, polyline, rect};

/// The height of the diagram in px.
pub const HEIGHT: f32 = 220.0;
/// The center of the marker of the live pressure, from the right edge of the plot.
const MARKER_INSET: f32 = 20.0;
/// The least height of a zone that shows its text, in px.
const ZONE_TEXT: f32 = 26.0;

/// The tops of blocks that must not overlap in a column of `total` px. `blocks` has the wanted
/// top and the height of each block, from the top block to the bottom one. A block moves down
/// when the one above it is in its way. Then the blocks that leave the column move up.
pub fn stack(blocks: &[(f32, f32)], total: f32) -> Vec<f32> {
    let mut tops = Vec::with_capacity(blocks.len());
    let mut next = 0.0_f32;
    for (wanted, height) in blocks {
        let top = wanted.max(next);
        tops.push(top);
        next = top + height;
    }
    let mut limit = total;
    for (top, (_, height)) in tops.iter_mut().zip(blocks).rev() {
        *top = top.min(limit - height);
        limit = *top;
    }
    tops
}

/// The text of a zone on a plate, so that the hatch does not cross it. `center` is the middle
/// of the zone in px from the top of the diagram.
fn zone_label(text: &str, center: f32, name: &'static str, p: &Palette) -> Div {
    div()
        .absolute()
        .top(px(center - 11.))
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

/// The diagram: the marks of the axis at the left, the plot, and the values of its lines at
/// the right.
pub fn band_diagram(model: &BandDiagram, p: &Palette) -> AnyElement {
    let (band, ink, accent, card, hatching, mark) = (
        p.accent_tint,
        p.fg,
        p.accent,
        p.card,
        p.fg_dim.opacity(0.45),
        p.border_strong,
    );
    let y = |place: f32| (place * HEIGHT).round();
    let [top, middle, bottom] = [0, 1, 2].map(|index| y(model.lines[index].place));
    let draft = model
        .draft
        .as_ref()
        .map(|lines| [0, 1, 2].map(|index| y(lines[index].place)));
    let marker = model.marker.as_ref().map(|(place, _)| y(*place));
    let ticks: Vec<f32> = model.ticks.iter().map(|(place, _)| y(*place)).collect();
    let paint = canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let o = bounds.origin;
            let w = f32::from(bounds.size.width);
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
            for tick in &ticks {
                polyline(
                    window,
                    &[at(o, 0.0, *tick), at(o, 5.0, *tick)],
                    1.0,
                    mark,
                    None,
                );
            }
            let widths = [(1.5, ink), (2.0, accent), (1.5, ink)];
            for (line, (width, color)) in [top, middle, bottom].into_iter().zip(widths) {
                polyline(
                    window,
                    &[at(o, 0.0, line), at(o, w, line)],
                    width,
                    color,
                    None,
                );
            }
            if let Some(draft) = draft {
                for (line, (width, color)) in draft.into_iter().zip(widths) {
                    polyline(
                        window,
                        &[at(o, 0.0, line), at(o, w, line)],
                        width,
                        color,
                        Some((7.0, 5.0)),
                    );
                }
            }
            if let Some(marker) = marker {
                disc(
                    window,
                    at(o, w - MARKER_INSET, marker),
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
    // The text of the band goes into the wider one of its two parts.
    let inside = if middle - top >= bottom - middle {
        (top, middle)
    } else {
        (middle, bottom)
    };
    for (text, (from, to), name) in [
        (&model.zones[0], (0.0, top), "ls-band-zone-above"),
        (&model.zones[1], inside, "ls-band-zone-inside"),
        (&model.zones[2], (bottom, HEIGHT), "ls-band-zone-below"),
    ] {
        if to - from >= ZONE_TEXT {
            plot = plot.child(zone_label(text, (from + to) / 2.0, name, p));
        }
    }
    if let (Some(marker), Some((_, text))) = (marker, &model.marker) {
        plot = plot.child(
            div()
                .absolute()
                .top(px((marker - 11.).clamp(0.0, HEIGHT - 22.)))
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

    // The marks of the axis, at the left of the plot.
    let mut axis = div().relative().flex_none().w(px(30.)).h(px(HEIGHT));
    for (place, text) in &model.ticks {
        axis = axis.child(
            caption(text.clone(), p)
                .absolute()
                .top(px(y(*place) - 8.))
                .right(px(4.))
                .whitespace_nowrap(),
        );
    }

    // The values of the lines, each one on the height of its line. Two values that are close
    // together move apart.
    let mut blocks: Vec<(f32, f32, Div)> = Vec::new();
    for (index, line) in model.lines.iter().enumerate() {
        let note = line.note.clone();
        let height = if note.is_some() { 34. } else { 18. };
        blocks.push((
            y(line.place) - 9.,
            height,
            v_flex()
                .child(
                    div()
                        .h(px(18.))
                        .line_height(px(18.))
                        .font_weight(FontWeight::BOLD)
                        .when(index == 1, |label| label.text_color(p.accent))
                        .child(line.label.clone()),
                )
                .children(note.map(|note| caption(note, p))),
        ));
    }
    for (index, line) in model.draft.iter().flatten().enumerate() {
        blocks.push((
            y(line.place) - 8.,
            16.,
            caption(line.label.clone(), p).text_color(if index == 1 { p.accent } else { p.fg }),
        ));
    }
    blocks.sort_by(|a, b| a.0.total_cmp(&b.0));
    let wanted: Vec<(f32, f32)> = blocks
        .iter()
        .map(|(top, height, _)| (*top, *height))
        .collect();
    let tops = stack(&wanted, HEIGHT);
    let mut values = v_flex()
        .flex_none()
        .h(px(HEIGHT))
        .whitespace_nowrap()
        .debug_selector(|| "ls-band-values".to_owned());
    let mut end = 0.0;
    for (index, ((_, height, block), top)) in blocks.into_iter().zip(tops).enumerate() {
        values = values.child(
            block
                .h(px(height))
                .flex_none()
                .mt(px(top - end))
                .debug_selector(move || format!("ls-band-label-{index}")),
        );
        end = top + height;
    }
    h_flex()
        .w_full()
        .items_start()
        .child(axis)
        .child(plot)
        .child(div().w(px(12.)).flex_none())
        .child(values)
        .into_any_element()
}
