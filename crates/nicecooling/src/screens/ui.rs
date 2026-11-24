//! Small building blocks that the screens share. They paint with the tokens of `Palette`.

use gpui::prelude::*;
use gpui::{
    AnyElement, BoxShadow, Div, ElementId, FontWeight, Hsla, SharedString, Stateful, deferred, div,
    point, px, relative, rgba,
};
use gpui_component::{Icon, Sizable as _, h_flex, v_flex};

use crate::icons::{NcIcon, icon, small_icon};
use crate::model::Severity;
use crate::screens::FeedbackLine;
use crate::tokens::Palette;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TagKind {
    Neutral,
    Accent,
    Warning,
    Alarm,
    Offline,
    Success,
}

/// A 22 px pill with 12 px text.
pub fn tag(text: impl Into<SharedString>, kind: TagKind, p: &Palette) -> Div {
    let clear = gpui::transparent_black();
    let (fill, ink) = match kind {
        TagKind::Neutral => (p.selected, p.fg),
        TagKind::Accent => (p.accent_tint, p.accent),
        TagKind::Warning => (p.warning_tint, p.warning),
        TagKind::Alarm => (p.alarm_bg, p.alarm_fg),
        TagKind::Offline => (clear, p.offline),
        TagKind::Success => (clear, p.success),
    };
    div()
        .h(px(22.))
        .flex_none()
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(4.))
        .rounded_full()
        .bg(fill)
        .text_color(ink)
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .when(kind == TagKind::Offline, |tag| {
            tag.border_1().border_dashed().border_color(p.offline)
        })
        .when(kind == TagKind::Success, |tag| {
            tag.border_1().border_color(p.success)
        })
        .child(text.into())
}

/// 12 px text in `fg-dim`. In a row it takes the width that is left and wraps there.
pub fn caption(text: impl Into<SharedString>, p: &Palette) -> Div {
    div()
        .min_w_0()
        .text_size(px(12.))
        .line_height(px(16.))
        .text_color(p.fg_dim)
        .child(text.into())
}

pub fn heading(text: impl Into<SharedString>) -> Div {
    div()
        .min_w_0()
        .font_weight(FontWeight::BOLD)
        .child(text.into())
}

// A text that is a direct child of a row keeps its full width: GPUI gives it no smaller
// minimum, so it runs into the next cell. The helpers below put each text in a box that can
// shrink (`min_w_0`), as `DESIGN.md` asks: a long name truncates with an ellipsis and the
// value next to it keeps its width.

/// A table cell with a fixed width. Its text stays on one line and truncates.
pub fn cell(width: f32) -> Div {
    div().w(px(width)).flex_none().min_w_0().truncate()
}

/// A table cell with a fixed width for a sentence: the text wraps in the cell.
pub fn wrap_cell(width: f32) -> Div {
    div().w(px(width)).flex_none().min_w_0()
}

/// The table cell that takes the rest of the row. Its text stays on one line and truncates.
pub fn wide_cell() -> Div {
    div().flex_1().min_w_0().truncate()
}

/// A text next to an icon or a control in a row: it shrinks and truncates.
pub fn label(text: impl Into<SharedString>) -> Div {
    div().min_w_0().truncate().child(text.into())
}

/// The track of a segmented control: its items are `seg_item`.
pub fn seg(p: &Palette) -> Div {
    h_flex()
        .flex_none()
        .p(px(3.))
        .gap(px(3.))
        .rounded(px(9.))
        .bg(p.track)
}

/// One item of a segmented control: 28 px, bold. The selected item has the `card` fill. The
/// caller adds the click handler of an enabled item.
pub fn seg_item(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    on: bool,
    enabled: bool,
    p: &Palette,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.))
        .px(px(14.))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(7.))
        .font_weight(FontWeight::BOLD)
        .whitespace_nowrap()
        .when(on, |item| {
            item.bg(p.card).shadow(vec![BoxShadow {
                color: rgba(0x0000_0633).into(),
                offset: point(px(0.), px(1.)),
                blur_radius: px(3.),
                spread_radius: px(0.),
                inset: false,
            }])
        })
        .when(enabled && !on, |item| item.cursor_pointer())
        .when(!enabled, |item| item.opacity(0.5))
        .child(text.into())
}

/// The closed look of a select: the `ground` fill, the text, an optional value in bold and a
/// chevron at the right. 34 px high, or 28 px for a select in a table row. A long text
/// truncates and the value keeps its width.
pub fn select_trigger(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    value: Option<SharedString>,
    small: bool,
    enabled: bool,
    ground: Hsla,
) -> Stateful<Div> {
    h_flex()
        .id(id)
        .w_full()
        .min_w_0()
        .h(px(if small { 28. } else { 34. }))
        .px(px(10.))
        .gap(px(6.))
        .rounded(px(if small { 7. } else { 9. }))
        .bg(ground)
        .when(enabled, |trigger| trigger.cursor_pointer())
        .when(!enabled, |trigger| trigger.opacity(0.5))
        .child(div().flex_1().min_w_0().truncate().child(text.into()))
        .children(value.map(|value| {
            div()
                .flex_none()
                .font_weight(FontWeight::BOLD)
                .child(value)
        }))
        .child(Icon::new(NcIcon::Down).with_size(px(16.)).flex_none())
}

/// The ground of a popover: the `card` fill, 12 px radius and the popover shadow.
pub fn popover(p: &Palette) -> Div {
    let shadow = |color: gpui::Rgba, y: f32, blur: f32, spread: f32| BoxShadow {
        color: color.into(),
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset: false,
    };
    v_flex()
        .rounded(px(12.))
        .bg(p.card)
        .occlude()
        .shadow(if p.dark {
            vec![
                shadow(rgba(0xFFFF_FF14), 0., 0., 1.),
                shadow(rgba(0x0000_0080), 8., 24., 0.),
            ]
        } else {
            vec![
                shadow(rgba(0x0000_060F), 0., 0., 1.),
                shadow(rgba(0x0000_062E), 8., 24., 0.),
            ]
        })
}

/// A control with its popover under it. The popover paints over the content that follows:
/// it does not move it.
pub fn with_popover(control: impl IntoElement, popover: Option<AnyElement>) -> Div {
    div()
        .relative()
        .min_w_0()
        .child(control)
        .children(popover.map(|popover| {
            deferred(
                div()
                    .absolute()
                    .top(relative(1.))
                    .left_0()
                    .pt(px(3.))
                    .child(popover),
            )
            .with_priority(1)
        }))
}

/// The header line of a table: 12 px bold text in `fg-dim`. `widths` has one entry per
/// title. `None` is the cell that takes the rest of the row.
pub fn table_header(titles: &[String], widths: &[Option<f32>], p: &Palette) -> Div {
    let mut line = h_flex()
        .w_full()
        .px(px(18.))
        .py(px(6.))
        .gap(px(12.))
        .border_t_1()
        .border_color(p.border)
        .text_size(px(12.))
        .line_height(px(16.))
        .font_weight(FontWeight::BOLD)
        .text_color(p.fg_dim);
    for (title, width) in titles.iter().zip(widths) {
        line = line.child(
            match width {
                Some(width) => cell(*width),
                None => wide_cell(),
            }
            .child(title.clone()),
        );
    }
    line
}

/// A card: the `card` fill and 12 px radius. The light theme has the Adwaita card shadow. The
/// dark theme has no shadow: the lighter fill separates the card.
pub fn card(p: &Palette) -> Div {
    let shadow = |alpha: u32, y: f32, blur: f32, spread: f32| BoxShadow {
        color: rgba(0x0000_0600 | alpha).into(),
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset: false,
    };
    v_flex()
        .w_full()
        .min_w_0()
        .rounded(px(12.))
        .bg(p.card)
        .when(!p.dark, |card| {
            card.shadow(vec![
                shadow(0x08, 0., 0., 1.),
                shadow(0x12, 1., 3., 1.),
                shadow(0x08, 2., 6., 2.),
            ])
        })
}

/// The header of a card: 40 px, with the title at the left and the controls at the right.
/// The rows under it draw the line.
pub fn card_header(title: impl Into<SharedString>, _p: &Palette) -> Div {
    h_flex()
        .min_h(px(40.))
        .pl(px(18.))
        .pr(px(12.))
        .py(px(6.))
        .gap(px(9.))
        .flex_none()
        .child(heading(title).flex_1().min_w_0())
}

/// A row of a card: a 1 px line above it, and 18 px side padding.
pub fn card_row(p: &Palette) -> Div {
    h_flex()
        .w_full()
        .min_h(px(40.))
        .px(px(18.))
        .py(px(6.))
        .gap(px(12.))
        .items_center()
        .border_t_1()
        .border_color(p.border)
}

/// The severity icon: a triangle for a Warning, an octagon for an Alarm.
pub fn severity_icon(severity: Severity, p: &Palette) -> Div {
    let (name, color): (NcIcon, Hsla) = match severity {
        Severity::Warning => (NcIcon::Warn, p.warning),
        Severity::Alarm => (NcIcon::Alarm, p.alarm),
    };
    div().flex_none().child(icon(name, color))
}

/// The padlock of a field that is read-only, and its text.
pub fn locked(text: impl Into<SharedString>, p: &Palette) -> Div {
    h_flex()
        .gap(px(4.))
        .child(small_icon(NcIcon::Lock, p.fg_dim))
        .child(text.into())
}

/// The write feedback next to a control: check or octagon, and the text. The text takes the
/// width that is left and wraps there.
pub fn write_line(line: &FeedbackLine, p: &Palette) -> Div {
    let color = if line.failure { p.alarm } else { p.fg_dim };
    h_flex()
        .min_w_0()
        .gap(px(6.))
        .items_start()
        .text_size(px(12.))
        .line_height(px(16.))
        .text_color(color)
        .child(
            icon(
                if line.failure {
                    NcIcon::Alarm
                } else {
                    NcIcon::Check
                },
                color,
            )
            .with_size(px(14.))
            .mt(px(1.)),
        )
        .child(
            div()
                .min_w_0()
                .debug_selector(|| "write-line-text".to_owned())
                .child(line.text.clone()),
        )
}

/// The ground of a page: 12 px at the top, 24 px at the sides, 12 px between the cards.
pub fn page() -> Div {
    v_flex()
        .w_full()
        .px(px(24.))
        .pt(px(12.))
        .pb(px(18.))
        .gap(px(12.))
}

/// A dialog of 440 px over a dark scrim. The caller puts it in a container with `relative()`.
/// `body` goes under the text, `buttons` at the right of the last row.
pub fn modal(
    title: impl Into<SharedString>,
    text: impl Into<SharedString>,
    body: Vec<AnyElement>,
    buttons: Vec<AnyElement>,
    p: &Palette,
) -> Div {
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .justify_center()
        .pt(px(120.))
        .bg(gpui::black().opacity(0.4))
        .child(
            v_flex()
                .w(px(440.))
                .h_auto()
                .p(px(18.))
                .gap(px(12.))
                .rounded(px(15.))
                .bg(p.card)
                .border_1()
                .border_color(p.border)
                .child(
                    div()
                        .text_size(px(18.))
                        .font_weight(FontWeight::BOLD)
                        .child(title.into()),
                )
                .child(text.into())
                .children(body)
                .child(h_flex().justify_end().gap(px(8.)).children(buttons)),
        )
}

/// A row with a warning or an information line: the icon and the text.
pub fn notice_row(text: impl Into<SharedString>, warning: bool, p: &Palette) -> Div {
    card_row(p)
        .bg(if warning { p.warning_tint } else { p.selected })
        .child(if warning {
            icon(NcIcon::Warn, p.warning)
        } else {
            icon(NcIcon::Info, p.fg_dim)
        })
        .child(div().flex_1().min_w_0().child(text.into()))
}
