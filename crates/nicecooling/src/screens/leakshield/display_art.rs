//! What the Display tab paints itself: the one-bit images of the device, the frame of a preview
//! and the schematic of each template.

use gpui::prelude::*;
use gpui::{Bounds, Div, Pixels, canvas, div, px, size};

use crate::bitmap::Bitmap;
use crate::screens::leakshield::display::Screen;
use crate::tokens::Palette;
use crate::widgets::paint::{at, rect};

/// The dark frame around a preview, in px.
const FRAME: f32 = 4.0;

/// A one-bit image at `scale` window pixels for each pixel. A whole `scale` gives sharp pixels:
/// each run of lit pixels is one rectangle.
pub fn bitmap_view(bitmap: &Bitmap, scale: f32, p: &Palette) -> impl IntoElement + use<> {
    let (width, height) = (f32::from(bitmap.width), f32::from(bitmap.height));
    let runs = bitmap.runs();
    let ink = p.display_ink;
    canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            for (x, y, length) in &runs {
                rect(
                    window,
                    Bounds::new(
                        at(bounds.origin, f32::from(*x) * scale, f32::from(*y) * scale),
                        size(px(f32::from(*length) * scale), px(scale)),
                    ),
                    0.0,
                    ink,
                    None,
                );
            }
        },
    )
    .w(px(width * scale))
    .h(px(height * scale))
    .flex_none()
}

/// A bitmap on the display ground, with the frame of a preview.
pub fn framed(bitmap: &Bitmap, scale: f32, p: &Palette) -> Div {
    div()
        .flex_none()
        .p(px(FRAME))
        .rounded(px(6.))
        .bg(p.display)
        .child(bitmap_view(bitmap, scale, p))
}

/// The preview of a page of 128 x 64 pixels at `scale`: the image of the device on the display
/// ground, or a light box with the reason why the page has no image. `dim` is a page that the
/// device does not show.
pub fn screen_view(screen: &Screen, scale: f32, dim: bool, p: &Palette) -> Div {
    let frame = match screen {
        Screen::Image(bitmap) => framed(bitmap, scale, p),
        Screen::Note(text) => div()
            .flex_none()
            .w(px(128. * scale + 2. * FRAME))
            .h(px(64. * scale + 2. * FRAME))
            .p(px(6.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .border_1()
            .border_dashed()
            .border_color(p.border_strong)
            .bg(p.track)
            .text_size(px(12.))
            .line_height(px(16.))
            .text_color(p.fg_dim)
            .text_center()
            .child(div().min_w_0().child(text.clone())),
    };
    frame
        .debug_selector(|| "display-screen".to_owned())
        .when(dim, |frame| frame.opacity(0.4))
}

/// One shape of a schematic, on the grid of 128 x 64 pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// A line of text: a dim bar.
    Text(u8, u8, u8, u8),
    /// A value or a filled part: a lit bar.
    Fill(u8, u8, u8, u8),
    /// The outline of an icon or of a bar.
    Outline(u8, u8, u8, u8),
}

/// The outline of an icon of 23 x 23 pixels.
const fn icon(x: u8, y: u8) -> Shape {
    Shape::Outline(x, y, 23, 23)
}

/// The shapes that sketch the layout of a template. An empty page and an unknown template
/// have none.
pub fn template_shapes(template: &str) -> Vec<Shape> {
    use Shape::{Fill, Outline, Text};
    // A text line with a rule under it: the head of a page.
    let head = |width: u8| [Text(2, 2, width, 5), Fill(0, 11, 128, 1)];
    // A bar with its filled part.
    let bar = |x: u8, y: u8, w: u8, h: u8, fill: u8| [Outline(x, y, w, h), Fill(x, y, fill, h)];
    match template {
        "logo" => vec![Outline(12, 18, 104, 28), Fill(24, 28, 80, 8)],
        "chart" => {
            let mut shapes = vec![Text(46, 2, 50, 5), Fill(14, 4, 1, 50), Fill(14, 54, 110, 1)];
            for (index, height) in [10u8, 14, 8, 18, 24, 32, 28, 36, 30, 20, 14, 8]
                .into_iter()
                .enumerate()
            {
                shapes.push(Fill(20 + 8 * index as u8, 54 - height, 6, height));
            }
            shapes
        }
        "values-1" => vec![
            icon(2, 2),
            Text(32, 8, 80, 10),
            Fill(26, 36, 60, 20),
            Text(92, 46, 30, 6),
        ],
        "values-2" => [
            &head(50)[..],
            &[
                icon(2, 14),
                Fill(40, 16, 50, 14),
                Text(96, 20, 28, 6),
                icon(2, 40),
                Fill(60, 42, 30, 14),
                Text(96, 46, 28, 6),
            ],
        ]
        .concat(),
        "values-4" => (0..4u8)
            .flat_map(|row| [Text(2, 4 + 16 * row, 46, 7), Fill(82, 4 + 16 * row, 40, 7)])
            .collect(),
        "values-3a" => [
            &head(70)[..],
            &[
                icon(2, 14),
                Fill(44, 16, 48, 14),
                icon(2, 40),
                Text(70, 42, 40, 6),
                Text(70, 53, 40, 6),
            ],
        ]
        .concat(),
        "values-3b" => [
            &head(70)[..],
            &[
                icon(2, 14),
                Fill(44, 16, 48, 14),
                Text(18, 48, 36, 7),
                Text(82, 48, 36, 7),
            ],
        ]
        .concat(),
        "time" => vec![Fill(4, 4, 100, 18), Text(4, 32, 90, 10), Text(4, 50, 60, 6)],
        "bars-2" => [
            &[icon(2, 6), Text(30, 2, 40, 5)][..],
            &bar(30, 20, 96, 10, 56),
            &[icon(2, 38), Text(30, 34, 50, 5)],
            &bar(30, 52, 96, 10, 36),
        ]
        .concat(),
        "bars-4a" => [
            &[icon(2, 4)][..],
            &bar(30, 6, 96, 8, 40),
            &bar(30, 22, 96, 8, 70),
            &[icon(2, 36)],
            &bar(30, 38, 96, 8, 56),
            &bar(30, 54, 96, 8, 30),
        ]
        .concat(),
        "bars-4b" => [
            &head(50)[..],
            &bar(2, 15, 124, 9, 50),
            &bar(2, 27, 124, 9, 40),
            &bar(2, 39, 124, 9, 36),
            &bar(2, 51, 124, 9, 20),
        ]
        .concat(),
        "value-and-bar" => [
            &head(50)[..],
            &[Fill(30, 16, 56, 18)],
            &bar(2, 48, 124, 10, 60),
        ]
        .concat(),
        "value-and-time" => [
            &head(50)[..],
            &[icon(2, 14), Fill(50, 16, 50, 14), Fill(14, 44, 90, 14)],
        ]
        .concat(),
        "leakshield-state" => vec![
            Text(2, 3, 80, 5),
            Fill(2, 14, 100, 14),
            Text(2, 40, 70, 5),
            Text(2, 52, 56, 5),
        ],
        _ => Vec::new(),
    }
}

/// The schematic of a template, 96 x 48 px on the display ground.
pub fn schematic(template: &str, p: &Palette) -> Div {
    const SCALE: f32 = 0.75;
    let shapes = template_shapes(template);
    let (ink, dim) = (p.display_ink, p.display_ink.opacity(0.55));
    let clear = gpui::transparent_black();
    div()
        .flex_none()
        .p(px(FRAME))
        .rounded(px(6.))
        .bg(p.display)
        .child(
            canvas(
                |_, _, _| (),
                move |bounds: Bounds<Pixels>, _, window, _| {
                    for shape in &shapes {
                        let (x, y, w, h, fill, outline) = match *shape {
                            Shape::Text(x, y, w, h) => (x, y, w, h, dim, None),
                            Shape::Fill(x, y, w, h) => (x, y, w, h, ink, None),
                            Shape::Outline(x, y, w, h) => {
                                (x, y, w, h, clear, Some((ink, 1.0, false)))
                            }
                        };
                        rect(
                            window,
                            Bounds::new(
                                at(bounds.origin, f32::from(x) * SCALE, f32::from(y) * SCALE),
                                size(px(f32::from(w) * SCALE), px(f32::from(h) * SCALE)),
                            ),
                            0.0,
                            fill,
                            outline,
                        );
                    }
                },
            )
            .w(px(128. * SCALE))
            .h(px(64. * SCALE))
            .flex_none(),
        )
}
