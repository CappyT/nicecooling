//! Small composites of the Leakshield tabs, with the sizes of the comps in `docs/ui/comps/`.

use gpui::prelude::*;
use gpui::{
    App, BoxShadow, Div, Entity, Focusable as _, FontWeight, SharedString, Stateful, Window, div,
    point, px, rgba,
};
use gpui_component::button::Button;
use gpui_component::input::{Input, InputState};
use gpui_component::{h_flex, v_flex};

use crate::icons::{NcIcon, icon, small_icon};
use crate::screen_view::skeleton;
use crate::screens::ui::{TagKind, card, tag};
use crate::tokens::Palette;

/// A button of 34 px with bold text.
pub fn button(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Button {
    Button::new(id.into())
        .label(label.into())
        .h(px(34.))
        .px(px(15.))
        .font_weight(FontWeight::BOLD)
}

/// A small button of 28 px with bold 13 px text, for a card header or a table row.
pub fn small_button(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Button {
    Button::new(id.into())
        .label(label.into())
        .h(px(28.))
        .px(px(10.))
        .rounded(px(7.))
        .text_size(px(13.))
        .font_weight(FontWeight::BOLD)
}

/// The track of a segmented control. Its children are `segment`s.
pub fn segmented(p: &Palette) -> Div {
    h_flex()
        .flex_none()
        .p(px(3.))
        .gap(px(3.))
        .rounded(px(9.))
        .bg(p.track)
}

/// One choice of a segmented control. The selected one has the `card` fill and a shadow.
pub fn segment(
    id: impl Into<SharedString>,
    label: impl Into<SharedString>,
    selected: bool,
    enabled: bool,
    p: &Palette,
) -> Stateful<Div> {
    h_flex()
        .id(id.into())
        .h(px(28.))
        .px(px(14.))
        .flex_none()
        .rounded(px(7.))
        .font_weight(FontWeight::BOLD)
        .when(selected, |segment| {
            segment.bg(p.card).shadow(vec![BoxShadow {
                color: rgba(0x0000_0633).into(),
                offset: point(px(0.), px(1.)),
                blur_radius: px(3.),
                spread_radius: px(0.),
                inset: false,
            }])
        })
        .when(enabled, |segment| segment.cursor_pointer())
        .when(!enabled, |segment| segment.opacity(0.5))
        .child(label.into())
}

/// A tag with the check before its text, when the tag says that a state is good.
pub fn state_tag(text: impl Into<SharedString>, kind: TagKind, p: &Palette) -> Div {
    if kind != TagKind::Success {
        return tag(text, kind, p);
    }
    div()
        .h(px(22.))
        .flex_none()
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(4.))
        .rounded_full()
        .border_1()
        .border_color(p.success)
        .text_color(p.success)
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .child(small_icon(NcIcon::Check, p.success))
        .child(text.into())
}

/// The header of a card with a caption after its title: 40 px, the title bold, the caption in
/// `fg-dim`. The caption takes the width that is left, so the controls of the caller go at the
/// right.
pub fn header(
    title: impl Into<SharedString>,
    caption: impl Into<SharedString>,
    p: &Palette,
) -> Div {
    h_flex()
        .min_h(px(40.))
        .pl(px(18.))
        .pr(px(12.))
        .py(px(6.))
        .gap(px(9.))
        .flex_none()
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::BOLD)
                .child(title.into()),
        )
        .child(
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

/// A row of a settings card: 52 px, a 1 px line above it. The name and its caption go at the
/// left, the control at the right.
pub fn field_row(p: &Palette) -> Div {
    h_flex()
        .w_full()
        .min_h(px(52.))
        .px(px(18.))
        .py(px(8.))
        .gap(px(18.))
        .border_t_1()
        .border_color(p.border)
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

/// What an input takes: a value, nothing because it does not apply now, or nothing because
/// its field is read-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldMode {
    Editable,
    /// The setting does not apply now: the input is dim.
    Off,
    /// The field takes no write: no fill and a dashed outline.
    ReadOnly,
}

impl From<bool> for FieldMode {
    fn from(writable: bool) -> Self {
        if writable {
            Self::Editable
        } else {
            Self::ReadOnly
        }
    }
}

/// A text or number input of 34 px: the `track` fill, no outline, the unit at the right.
/// `invalid` gives it the red outline of a value out of range.
#[allow(clippy::too_many_arguments)]
pub fn field_input(
    state: &Entity<InputState>,
    unit: &str,
    mode: impl Into<FieldMode>,
    invalid: bool,
    width: f32,
    p: &Palette,
    window: &Window,
    cx: &App,
) -> Div {
    let mode = mode.into();
    let focused = state.read(cx).focus_handle(cx).is_focused(window);
    h_flex()
        .w(px(width))
        .h(px(34.))
        .flex_none()
        .rounded(px(9.))
        .border_1()
        .map(|field| match mode {
            FieldMode::ReadOnly => field
                .border_dashed()
                .border_color(p.border_strong)
                .text_color(p.fg_dim),
            FieldMode::Off => field.bg(p.track).border_color(p.track).opacity(0.5),
            FieldMode::Editable if invalid => field.bg(p.track).border_color(p.alarm),
            FieldMode::Editable if focused => field.bg(p.track).border_color(p.accent_bg),
            FieldMode::Editable => field.bg(p.track).border_color(p.track),
        })
        .child(
            div().flex_1().min_w_0().child(
                Input::new(state)
                    .appearance(false)
                    .disabled(mode != FieldMode::Editable),
            ),
        )
        .when(!unit.is_empty(), |field| {
            field.child(
                div()
                    .flex_none()
                    .pr(px(10.))
                    .text_color(p.fg_dim)
                    .child(unit.to_owned()),
            )
        })
}

/// A row of a table: 36 px, a 1 px line above it, and the gaps of `table_header`.
pub fn table_row(p: &Palette) -> Div {
    h_flex()
        .w_full()
        .min_h(px(36.))
        .px(px(18.))
        .py(px(3.))
        .gap(px(12.))
        .border_t_1()
        .border_color(p.border)
}
