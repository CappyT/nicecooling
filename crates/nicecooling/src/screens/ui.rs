//! Small building blocks that the screens share. They paint with the tokens of `Palette`.

use gpui::prelude::*;
use gpui::{Div, FontWeight, Hsla, SharedString, div, px};
use gpui_component::{h_flex, v_flex};

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
}

/// A 22 px pill with 12 px text.
pub fn tag(text: impl Into<SharedString>, kind: TagKind, p: &Palette) -> Div {
    let (fill, ink) = match kind {
        TagKind::Neutral => (p.selected, p.fg),
        TagKind::Accent => (p.accent_tint, p.accent),
        TagKind::Warning => (p.warning_tint, p.warning),
        TagKind::Alarm => (p.alarm_bg, p.alarm_fg),
        TagKind::Offline => (p.offline_tint, p.offline),
    };
    div()
        .h(px(22.))
        .px(px(9.))
        .flex()
        .items_center()
        .rounded_full()
        .bg(fill)
        .text_color(ink)
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .when(kind == TagKind::Offline, |tag| {
            tag.border_1().border_dashed().border_color(p.offline)
        })
        .child(text.into())
}

/// 12 px text in `fg-dim`.
pub fn caption(text: impl Into<SharedString>, p: &Palette) -> Div {
    div()
        .text_size(px(12.))
        .text_color(p.fg_dim)
        .child(text.into())
}

pub fn heading(text: impl Into<SharedString>) -> Div {
    div().font_weight(FontWeight::BOLD).child(text.into())
}

/// A card: the `card` fill, 12 px radius and a 1 px border.
pub fn card(p: &Palette) -> Div {
    v_flex()
        .w_full()
        .rounded(px(12.))
        .bg(p.card)
        .border_1()
        .border_color(p.border)
}

/// The header of a card: 40 px, with the title at the left and the controls at the right.
pub fn card_header(title: impl Into<SharedString>, p: &Palette) -> Div {
    h_flex()
        .h(px(40.))
        .px(px(18.))
        .gap(px(12.))
        .items_center()
        .flex_none()
        .border_b_1()
        .border_color(p.border)
        .child(heading(title).flex_1())
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

/// The text of the severity icon: a triangle for a Warning, an octagon for an Alarm.
pub fn severity_icon(severity: Severity, p: &Palette) -> Div {
    let (glyph, color): (&str, Hsla) = match severity {
        Severity::Warning => ("\u{25B2}", p.warning),
        Severity::Alarm => ("\u{2B22}", p.alarm),
    };
    div().text_color(color).text_size(px(12.)).child(glyph)
}

/// The write feedback next to a control: check or octagon, and the text.
pub fn write_line(line: &FeedbackLine, p: &Palette) -> Div {
    let color = if line.failure { p.alarm } else { p.fg_dim };
    h_flex()
        .gap(px(6.))
        .items_center()
        .text_size(px(12.))
        .text_color(color)
        .child(if line.failure { "\u{2B22}" } else { "\u{2713}" })
        .child(line.text.clone())
}

/// The ground of a page: 12 px at the top, 24 px at the sides, 12 px between the cards.
pub fn page() -> Div {
    v_flex().w_full().px(px(24.)).py(px(12.)).gap(px(12.))
}
