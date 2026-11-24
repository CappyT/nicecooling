//! Small composites of the Leakshield tabs, with the sizes of the comps in `docs/ui/comps/`.

use gpui::prelude::*;
use gpui::{Div, FontWeight, SharedString, div, px};
use gpui_component::{h_flex, v_flex};

use crate::icons::{NcIcon, icon};
use crate::screen_view::skeleton;
use crate::screens::ui::{TagKind, card, icon_tag, tag, title_header};
use crate::tokens::Palette;

/// A tag with the check before its text, when the tag says that a state is good.
pub fn state_tag(text: impl Into<SharedString>, kind: TagKind, p: &Palette) -> Div {
    if kind == TagKind::Success {
        icon_tag(NcIcon::Check, text, kind, p)
    } else {
        tag(text, kind, p)
    }
}

/// The header of a card with a caption after its title: 40 px, the title bold, the caption in
/// `fg-dim`. The caption takes the width that is left, so the controls of the caller go at the
/// right.
pub fn header(
    title: impl Into<SharedString>,
    caption: impl Into<SharedString>,
    p: &Palette,
) -> Div {
    title_header(title).child(
        div()
            .flex_1()
            .min_w_0()
            .truncate()
            .text_size(px(12.))
            .line_height(px(16.))
            .text_color(p.fg_dim)
            .child(caption.into()),
    )
}

/// The Loading state of a tab: the text and grey bars in a card.
pub fn loading_card(text: impl Into<SharedString>, p: &Palette) -> Div {
    card(p)
        .p(px(18.))
        .gap(px(12.))
        .child(v_flex().child(text.into()))
        .child(skeleton(p, 320.))
}

/// The badge next to a field name: the shield for a Verified field, the padlock of a
/// read-only field.
pub fn field_badge(text: impl Into<SharedString>, verified: bool, p: &Palette) -> Div {
    let (name, color) = if verified {
        (NcIcon::ShieldOk, p.success)
    } else {
        (NcIcon::Lock, p.fg_dim)
    };
    h_flex()
        .flex_none()
        .gap(px(4.))
        .text_size(px(12.))
        .line_height(px(16.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(color)
        .child(icon(name, color))
        .child(text.into())
}

/// The mark of a radio row or tile: a ring, filled when it is the selected one.
pub fn radio_mark(selected: bool, p: &Palette) -> Div {
    div().size(px(16.)).flex_none().rounded_full().map(|mark| {
        if selected {
            mark.border(px(5.)).border_color(p.accent_bg)
        } else {
            mark.border(px(1.5)).border_color(p.border_strong)
        }
    })
}
