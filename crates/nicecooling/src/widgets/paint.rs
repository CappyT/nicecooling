//! Painting helpers for the custom widgets: lines, dashes and discs on a canvas.

use gpui::{BorderStyle, Bounds, Hsla, PathBuilder, Pixels, Point, Window, point, px, quad, size};

/// The origin of a canvas plus an offset in px.
pub fn at(origin: Point<Pixels>, x: f32, y: f32) -> Point<Pixels> {
    point(origin.x + px(x), origin.y + px(y))
}

/// Stroke a polyline. `dash` is the dash and the gap in px.
pub fn polyline(
    window: &mut Window,
    points: &[Point<Pixels>],
    width: f32,
    color: Hsla,
    dash: Option<(f32, f32)>,
) {
    if points.len() < 2 {
        return;
    }
    let mut builder = PathBuilder::stroke(px(width));
    if let Some((on, off)) = dash {
        builder = builder.dash_array(&[px(on), px(off)]);
    }
    builder.move_to(points[0]);
    for p in &points[1..] {
        builder.line_to(*p);
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

/// Fill a closed polygon.
pub fn polygon(window: &mut Window, points: &[Point<Pixels>], color: Hsla) {
    if points.len() < 3 {
        return;
    }
    let mut builder = PathBuilder::fill();
    builder.add_polygon(points, true);
    if let Ok(path) = builder.build() {
        window.paint_path(path, color);
    }
}

/// A rectangle with a fill and an optional 1.5 px outline.
pub fn rect(
    window: &mut Window,
    bounds: Bounds<Pixels>,
    radius: f32,
    fill: Hsla,
    outline: Option<(Hsla, f32, bool)>,
) {
    let (border, width, dashed) = outline.unwrap_or((fill, 0.0, false));
    window.paint_quad(quad(
        bounds,
        px(radius),
        fill,
        px(width),
        border,
        if dashed {
            BorderStyle::Dashed
        } else {
            BorderStyle::Solid
        },
    ));
}

/// A disc of radius `r`.
pub fn disc(
    window: &mut Window,
    center: Point<Pixels>,
    r: f32,
    fill: Hsla,
    outline: Option<(Hsla, f32, bool)>,
) {
    let bounds = Bounds::new(
        point(center.x - px(r), center.y - px(r)),
        size(px(2.0 * r), px(2.0 * r)),
    );
    rect(window, bounds, r, fill, outline);
}

/// Hatch a rectangle with 45 degree lines, `gap` px apart.
pub fn hatch(window: &mut Window, bounds: Bounds<Pixels>, gap: f32, color: Hsla) {
    let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
    let mut offset = -h;
    while offset < w {
        // The line y = h - (x - offset), clipped to the rectangle.
        let x0 = offset.max(0.0);
        let x1 = (offset + h).min(w);
        polyline(
            window,
            &[
                at(bounds.origin, x0, h - (x0 - offset)),
                at(bounds.origin, x1, h - (x1 - offset)),
            ],
            1.0,
            color,
            None,
        );
        offset += gap;
    }
}
