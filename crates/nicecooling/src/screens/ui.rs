//! Small building blocks that the screens share. They paint with the tokens of `Palette`.

use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, BoxShadow, Div, ElementId, FocusHandle, FontWeight, Global, Hsla,
    KeyDownEvent, MouseDownEvent, Pixels, SharedString, Stateful, Window, WindowId, deferred, div,
    point, px, relative, rgba,
};
use gpui_component::tooltip::Tooltip;
use gpui_component::{Icon, Sizable as _, h_flex, v_flex};

use crate::icons::{NcIcon, icon, small_icon};
use crate::model::Severity;
use crate::screens::FeedbackLine;
use crate::tokens::{Palette, font_features};

/// A tooltip with the tabular figures of the shell. A tooltip draws outside the shell root,
/// so it does not inherit them.
pub fn tooltip(text: impl Into<SharedString>) -> Tooltip {
    Tooltip::new(text.into()).font_features(font_features())
}

/// Every tooltip of the app goes through `tip`.
pub trait Tip: InteractiveElement + Sized {
    /// Show `text` as the tooltip of this element. An empty text gives no tooltip.
    fn tip(mut self, text: impl Into<SharedString>) -> Self {
        let text: SharedString = text.into();
        if !text.is_empty() {
            self.interactivity()
                .tooltip(move |window, cx| tooltip(text.clone()).build(window, cx));
        }
        self
    }
}

impl<E: InteractiveElement> Tip for E {}

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
    tag_frame(kind, p).0.child(text.into())
}

/// A tag with a 12 px icon before its text.
pub fn icon_tag(name: NcIcon, text: impl Into<SharedString>, kind: TagKind, p: &Palette) -> Div {
    let (frame, ink) = tag_frame(kind, p);
    frame.child(small_icon(name, ink)).child(text.into())
}

/// The pill of a tag without its content, and the color of its text.
fn tag_frame(kind: TagKind, p: &Palette) -> (Div, Hsla) {
    let clear = gpui::transparent_black();
    let (fill, ink) = match kind {
        TagKind::Neutral => (p.selected, p.fg),
        TagKind::Accent => (p.accent_tint, p.accent),
        TagKind::Warning => (p.warning_tint, p.warning),
        TagKind::Alarm => (p.alarm_bg, p.alarm_fg),
        TagKind::Offline => (clear, p.offline),
        TagKind::Success => (clear, p.success),
    };
    let frame = div()
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
        });
    (frame, ink)
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

/// The focus of a control that is a plain element. It lives while the control is drawn. GPUI
/// reads the Tab stop from the handle, not from the element that tracks it.
fn keyed_focus(id: &ElementId, tab_stop: bool, window: &mut Window, cx: &mut App) -> FocusHandle {
    window
        .use_keyed_state(id.clone(), cx, move |_, cx| {
            cx.focus_handle().tab_stop(tab_stop)
        })
        .read(cx)
        .clone()
}

/// The focus ring of `DESIGN.md`: 2 px, 2 px from the control. `radius` is the radius of the
/// control.
fn focus_ring(radius: f32, color: Hsla) -> Div {
    div()
        .absolute()
        .top(px(-4.))
        .left(px(-4.))
        .right(px(-4.))
        .bottom(px(-4.))
        .rounded(px(radius + 4.))
        .border_2()
        .border_color(color)
        .debug_selector(|| "nc-focus-ring".to_owned())
}

/// True for Enter and Space without a modifier: the keys that press a control.
fn presses(event: &KeyDownEvent) -> bool {
    let keys = &event.keystroke;
    (keys.key == "enter" || keys.key == "space") && !keys.modifiers.modified()
}

/// Make a plain element a control of the keyboard: a Tab stop with the focus ring. Enter and
/// Space run its click handler, as GPUI does for the element that has the focus. As a button,
/// it takes no focus from a press: the ring shows for the keyboard only.
fn keyboard_control(
    control: Stateful<Div>,
    focus: &FocusHandle,
    radius: f32,
    ring: Hsla,
    focused: bool,
) -> Stateful<Div> {
    control
        .track_focus(focus)
        .on_any_mouse_down(|_, window, _| window.prevent_default())
        // The key stays here: a screen that takes Enter for its list does not get it too.
        .on_key_down(|event, _, cx| {
            if presses(event) {
                cx.stop_propagation();
            }
        })
        .when(focused, |control| control.child(focus_ring(radius, ring)))
}

/// The track of a segmented control: its items are `seg_item`. Tab stops at each item that
/// is enabled. Left and Right move the focus between them and do not leave the control.
pub fn seg(
    id: impl Into<ElementId>,
    p: &Palette,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let id: ElementId = id.into();
    let group = keyed_focus(&id, false, window, cx);
    h_flex()
        .id(id)
        .flex_none()
        .p(px(3.))
        .gap(px(3.))
        .rounded(px(9.))
        .bg(p.track)
        .track_focus(&group)
        .on_key_down(move |event, window, cx| {
            let keys = &event.keystroke;
            let forward = match keys.key.as_str() {
                "right" => true,
                "left" => false,
                _ => return,
            };
            let Some(from) = window.focused(cx) else {
                return;
            };
            if keys.modifiers.modified() || from == group {
                return;
            }
            if forward {
                window.focus_next(cx);
            } else {
                window.focus_prev(cx);
            }
            if !group.contains_focused(window, cx) {
                window.focus(&from, cx);
            }
            cx.stop_propagation();
        })
}

/// One item of a segmented control: 28 px, bold. The selected item has the `card` fill. The
/// caller adds the click handler of an enabled item: Enter and Space run it too. A disabled
/// item takes no focus.
pub fn seg_item(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    on: bool,
    enabled: bool,
    p: &Palette,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let id: ElementId = id.into();
    let focus = enabled.then(|| keyed_focus(&id, true, window, cx));
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
        .when_some(focus, |item, focus| {
            let focused = focus.is_focused(window);
            keyboard_control(item, &focus, 7., p.accent_bg, focused)
        })
}

/// The closed look of a select: the `ground` fill, the text, an optional value in bold and a
/// chevron at the right. 34 px high, or 28 px for a select in a table row. A long text
/// truncates and the value keeps its width. An enabled select is a Tab stop: Enter and Space
/// run the click handler that the caller adds. A disabled select takes no focus.
#[allow(clippy::too_many_arguments)]
pub fn select_trigger(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    value: Option<SharedString>,
    small: bool,
    enabled: bool,
    ground: Hsla,
    window: &mut Window,
    cx: &mut App,
) -> Stateful<Div> {
    let id: ElementId = id.into();
    let focus = enabled.then(|| keyed_focus(&id, true, window, cx));
    let focused = focus.as_ref().is_some_and(|focus| focus.is_focused(window));
    select_trigger_with(id, text, value, small, ground, focus.map(|f| (f, focused)))
}

/// `select_trigger` with the focus of the caller: the handle, and whether it has the focus
/// now. `None` is a disabled select.
pub fn select_trigger_with(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    value: Option<SharedString>,
    small: bool,
    ground: Hsla,
    focus: Option<(FocusHandle, bool)>,
) -> Stateful<Div> {
    let enabled = focus.is_some();
    // `accent-bg` is the same in the light and the dark theme.
    let (radius, ring) = (if small { 7. } else { 9. }, Palette::new(false).accent_bg);
    h_flex()
        .id(id)
        .w_full()
        .min_w_0()
        .h(px(if small { 28. } else { 34. }))
        .px(px(10.))
        .gap(px(6.))
        .rounded(px(radius))
        .bg(ground)
        .when(enabled, |trigger| trigger.cursor_pointer())
        .when(!enabled, |trigger| trigger.opacity(0.5))
        .child(div().flex_1().min_w_0().truncate().child(text.into()))
        .children(value.map(|value| div().flex_none().font_weight(FontWeight::BOLD).child(value)))
        .child(Icon::new(NcIcon::Down).with_size(px(16.)).flex_none())
        .when_some(focus, |trigger, (focus, focused)| {
            keyboard_control(trigger, &focus, radius, ring, focused)
        })
}

/// The shadow of a dialog and a popover (`DESIGN.md`, "Elevation & Depth").
pub fn pop_shadow(p: &Palette) -> Vec<BoxShadow> {
    let shadow = |color: u32, y: f32, blur: f32, spread: f32| BoxShadow {
        color: rgba(color).into(),
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset: false,
    };
    if p.dark {
        vec![
            shadow(0xFFFF_FF14, 0., 0., 1.),
            shadow(0x0000_0080, 8., 24., 0.),
        ]
    } else {
        vec![
            shadow(0x0000_060F, 0., 0., 1.),
            shadow(0x0000_062E, 8., 24., 0.),
        ]
    }
}

/// The color of the scrim under a dialog.
pub fn scrim(p: &Palette) -> Hsla {
    rgba(if p.dark { 0x0000_0099 } else { 0x0000_0680 }).into()
}

/// The ground of a popover: the `card` fill, 12 px radius and the popover shadow.
pub fn popover(p: &Palette) -> Div {
    v_flex()
        .rounded(px(12.))
        .bg(p.card)
        .occlude()
        .shadow(pop_shadow(p))
}

/// A control with its popover under it, at the left edge of the control. The popover paints
/// over the content that follows: it does not move it. `close` gets a press outside the
/// popover. A press on the control is not one: the control closes its own popover.
pub fn with_popover(
    control: impl IntoElement,
    popover: Option<AnyElement>,
    close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
    anchored(control, popover, false, close)
}

/// A control with its menu under it, at the right edge of the control. `close` is the one
/// of `with_popover`.
pub fn with_menu(
    control: impl IntoElement,
    menu: Option<AnyElement>,
    close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
    anchored(control, menu, true, close)
}

fn anchored(
    control: impl IntoElement,
    popover: Option<AnyElement>,
    right: bool,
    close: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> Div {
    let control_bounds = Rc::new(Cell::new(Bounds::<Pixels>::default()));
    let seen = control_bounds.clone();
    div()
        .relative()
        .min_w_0()
        .on_children_prepainted(move |children, _, _| {
            if let Some(first) = children.first() {
                seen.set(*first);
            }
        })
        .child(control)
        .children(popover.map(|popover| {
            deferred(
                div()
                    .absolute()
                    .top(relative(1.))
                    .map(|place| {
                        if right {
                            place.right_0()
                        } else {
                            place.left_0()
                        }
                    })
                    .pt(px(3.))
                    .on_mouse_down_out(move |event, window, cx| {
                        if !control_bounds.get().contains(&event.position) {
                            close(event, window, cx);
                        }
                    })
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

/// The header of a card whose title has a tag or a caption next to it. The title keeps its
/// width. The caller adds the tag, then a caption that takes the rest of the row.
pub fn title_header(title: impl Into<SharedString>) -> Div {
    h_flex()
        .min_h(px(40.))
        .pl(px(18.))
        .pr(px(12.))
        .py(px(6.))
        .gap(px(9.))
        .flex_none()
        .whitespace_nowrap()
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::BOLD)
                .child(title.into()),
        )
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

/// A row of a settings form: 52 px, a 1 px line above it. The name and its caption go at the
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

/// A dialog of 440 px over the scrim. The scrim covers the whole window, as the comps show
/// it: the sidebar and the header bar take no press while a dialog is open. `body` goes under
/// the text, `buttons` at the right of the last row.
pub fn modal(
    title: impl Into<SharedString>,
    text: impl Into<SharedString>,
    body: Vec<AnyElement>,
    buttons: Vec<AnyElement>,
    p: &Palette,
) -> Div {
    let text: SharedString = text.into();
    let dialog = v_flex()
        .w(px(440.))
        .p(px(18.))
        .gap(px(12.))
        .rounded(px(15.))
        .bg(p.card)
        .shadow(pop_shadow(p))
        .debug_selector(|| "modal-dialog".to_owned())
        .child(
            div()
                .text_size(px(18.))
                .line_height(px(24.))
                .font_weight(FontWeight::BOLD)
                .child(title.into()),
        )
        .children((!text.is_empty()).then_some(text))
        .children(body)
        .child(h_flex().justify_end().gap(px(8.)).children(buttons));
    // The page clips its content, so the scrim draws after the window, at the window origin.
    div().absolute().child(deferred(
        gpui::anchored()
            .position(point(px(0.), px(0.)))
            .child(Scrim {
                color: scrim(p),
                dialog: dialog.into_any_element(),
            }),
    ))
}

/// The scrim of `modal`: the size of the window, with the dialog in it.
#[derive(IntoElement)]
struct Scrim {
    color: Hsla,
    dialog: AnyElement,
}

/// The dialog that is open in a window.
struct OpenDialog {
    /// The focus of the scrim: the controls of the dialog are in it.
    scope: FocusHandle,
    /// What had the focus when the dialog opened. It gets the focus back.
    opener: Option<FocusHandle>,
    /// True when this frame drew the dialog.
    drawn: bool,
    /// What had the focus at the start of the frame, when that was in the dialog.
    held: Option<Option<FocusHandle>>,
}

/// The focus of the dialogs, by window. While a dialog is open, the focus stays in it.
#[derive(Default)]
struct DialogFocus {
    open: HashMap<WindowId, OpenDialog>,
    /// A control outside the dialog that stays in the Tab order: "Cancel poweroff" of the
    /// banner, which draws over the scrim.
    extra: HashMap<WindowId, FocusHandle>,
}

impl Global for DialogFocus {}

/// The view that holds the dialogs calls this at the start of its render. When the frame
/// ends without the dialog, the control that opened it gets the focus back.
pub fn dialog_frame(window: &mut Window, cx: &mut App) {
    let id = window.window_handle().window_id();
    let focused = window.focused(cx);
    let Some(open) = cx.default_global::<DialogFocus>().open.get_mut(&id) else {
        return;
    };
    open.drawn = false;
    let inside = focused
        .as_ref()
        .is_none_or(|focused| *focused == open.scope || open.scope.contains(focused, window));
    open.held = inside.then_some(focused);
    window.defer(cx, move |window, cx| {
        let dialogs = cx.default_global::<DialogFocus>();
        if dialogs.open.get(&id).is_some_and(|open| !open.drawn)
            && let Some(closed) = dialogs.open.remove(&id)
            && let Some(opener) = closed.opener
            // A focus that the user moved out of the dialog stays where it is.
            && closed.held.is_some_and(|held| held == window.focused(cx))
        {
            window.focus(&opener, cx);
        }
    });
}

/// Keep `control` in the Tab order of an open dialog, or take it out with `None`.
pub fn set_dialog_extra(control: Option<&FocusHandle>, window: &Window, cx: &mut App) {
    let id = window.window_handle().window_id();
    let extra = &mut cx.default_global::<DialogFocus>().extra;
    match control {
        Some(control) => extra.insert(id, control.clone()),
        None => extra.remove(&id),
    };
}

/// Tab and Shift+Tab while a dialog is open: the focus goes through the controls of the
/// dialog, then to the extra control, and starts again. Returns false when no dialog is
/// open: the caller then moves the focus itself.
pub fn dialog_tab(back: bool, window: &mut Window, cx: &mut App) -> bool {
    let id = window.window_handle().window_id();
    let dialogs = cx.default_global::<DialogFocus>();
    let Some(scope) = dialogs.open.get(&id).map(|open| open.scope.clone()) else {
        return false;
    };
    let extra = dialogs.extra.get(&id).cloned();
    let inside = |window: &Window, cx: &App| {
        window
            .focused(cx)
            .is_some_and(|focused| focused != scope && scope.contains(&focused, window))
    };
    // The first or the last control of the dialog. A dialog without a control keeps the
    // focus on its scrim.
    let edge = |last: bool, window: &mut Window, cx: &mut App| {
        window.focus(&scope, cx);
        let mut found = None;
        for _ in 0..64 {
            window.focus_next(cx);
            if !inside(window, cx) {
                break;
            }
            found = window.focused(cx);
            if !last {
                break;
            }
        }
        window.focus(found.as_ref().unwrap_or(&scope), cx);
    };
    if extra.as_ref().is_some_and(|extra| extra.is_focused(window)) {
        edge(back, window, cx);
        return true;
    }
    if back {
        window.focus_prev(cx);
    } else {
        window.focus_next(cx);
    }
    if !inside(window, cx) {
        match &extra {
            Some(extra) => window.focus(extra, cx),
            None => edge(back, window, cx),
        }
    }
    true
}

impl RenderOnce for Scrim {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let size = window.viewport_size();
        // The focus moves into the dialog when it opens.
        let id = window.window_handle().window_id();
        let focused = window.focused(cx);
        let dialogs = cx.default_global::<DialogFocus>();
        let scope = match dialogs.open.get_mut(&id) {
            Some(open) => {
                open.drawn = true;
                open.scope.clone()
            }
            None => {
                let scope = cx.focus_handle();
                cx.default_global::<DialogFocus>().open.insert(
                    id,
                    OpenDialog {
                        scope: scope.clone(),
                        opener: focused,
                        drawn: true,
                        held: None,
                    },
                );
                window.focus(&scope, cx);
                scope
            }
        };
        div()
            .track_focus(&scope)
            // Tab stays in the dialog. A key that is not text stops here: the screen under
            // the scrim does not get it. Escape goes on: it closes a list in the dialog.
            .on_key_down(|event, window, cx| {
                let keys = &event.keystroke;
                let command =
                    keys.modifiers.control || keys.modifiers.alt || keys.modifiers.platform;
                if keys.key == "tab" && !command {
                    dialog_tab(keys.modifiers.shift, window, cx);
                    cx.stop_propagation();
                } else if keys.key != "escape"
                    && (command || (keys.key != "space" && keys.key.chars().count() != 1))
                {
                    cx.stop_propagation();
                }
            })
            .w(size.width)
            .h(size.height)
            .flex()
            .justify_center()
            // The dialog keeps the height of its content.
            .items_start()
            .pt(px(120.))
            .bg(self.color)
            // The scrim takes the presses: a control under a dialog does not get them.
            .occlude()
            .debug_selector(|| "modal-scrim".to_owned())
            .child(self.dialog)
    }
}

/// A row with a warning or an information line: the icon and the text on the tint of its
/// kind.
pub fn notice_row(text: impl Into<SharedString>, warning: bool, p: &Palette) -> Div {
    card_row(p)
        .bg(if warning {
            p.warning_tint
        } else {
            p.accent_tint
        })
        .child(if warning {
            icon(NcIcon::Warn, p.warning)
        } else {
            icon(NcIcon::Info, p.accent)
        })
        .child(div().flex_1().min_w_0().child(text.into()))
}
