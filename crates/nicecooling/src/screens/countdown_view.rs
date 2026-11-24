//! The poweroff countdown dialog (`docs/ui/widgets.md`, "Countdown dialog"): the Alarm field
//! as its head, the seconds, the facts of the Alarm and the two buttons.

use gpui::prelude::*;
use gpui::{AnyElement, App, FocusHandle, FontWeight, Window, div, px};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{Sizable as _, h_flex, v_flex};

use crate::icons::{NcIcon, icon};
use crate::screens::alarms::CountdownDialog;
use crate::screens::ui::{caption, pop_shadow, scrim};
use crate::tokens::Palette;

/// The dialog over the scrim. "Cancel poweroff" has `focus` when the dialog opens.
pub fn countdown_overlay(
    dialog: &CountdownDialog,
    scope: &FocusHandle,
    focus: &FocusHandle,
    on_cancel: impl Fn(&mut Window, &mut App) + 'static,
    on_hide: impl Fn(&mut Window, &mut App) + 'static,
    p: &Palette,
) -> AnyElement {
    let head = h_flex()
        .px(px(24.))
        .py(px(18.))
        .gap(px(12.))
        // A clip is a rectangle: the head has the radius of the dialog itself.
        .rounded_t(px(15.))
        .bg(p.alarm_bg)
        .text_color(p.alarm_fg)
        .child(icon(NcIcon::Alarm, p.alarm_fg).with_size(px(32.)))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(18.))
                        .line_height(px(24.))
                        .font_weight(FontWeight::BOLD)
                        .child(dialog.title.clone()),
                )
                .child(dialog.text.clone())
                .children(dialog.more.clone()),
        );
    let seconds = v_flex()
        .px(px(24.))
        .pt(px(18.))
        .pb(px(6.))
        .items_center()
        .child(
            div()
                .text_size(px(72.))
                .line_height(px(76.))
                .font_weight(FontWeight::EXTRA_BOLD)
                .child(dialog.seconds.to_string()),
        )
        .child(
            div()
                .text_color(p.fg_dim)
                .child(dialog.seconds_label.clone()),
        )
        .child(
            div()
                .mt(px(12.))
                .mb(px(6.))
                .h(px(6.))
                .w_full()
                .rounded(px(3.))
                .overflow_hidden()
                .bg(p.track)
                .child(
                    div()
                        .h_full()
                        .w(gpui::relative(dialog.fraction))
                        .bg(p.alarm_bg),
                ),
        );
    // Two columns: the names keep their width, and the values take the rest.
    let line = |text: &str| div().truncate().child(text.to_owned());
    let facts = h_flex()
        .px(px(24.))
        .py(px(12.))
        .gap(px(18.))
        .items_start()
        .child(
            v_flex()
                .flex_none()
                .gap(px(3.))
                .text_color(p.fg_dim)
                .children(dialog.rows.iter().map(|(name, _)| line(name))),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(3.))
                .children(dialog.rows.iter().map(|(_, value)| line(value))),
        );
    let mut frame = v_flex()
        .w(px(440.))
        .flex_none()
        .rounded(px(15.))
        .overflow_hidden()
        .bg(p.card)
        .shadow(pop_shadow(p))
        .child(head)
        .child(seconds)
        .child(facts);
    if dialog.ended {
        frame = frame.child(div().h(px(18.)));
    } else {
        frame = frame.child(
            v_flex()
                .px(px(24.))
                .pt(px(6.))
                .pb(px(24.))
                .gap(px(6.))
                .child(
                    Button::new("countdown-cancel")
                        .debug_selector(|| "countdown-cancel".to_owned())
                        .primary()
                        .track_focus(focus)
                        .w_full()
                        .h(px(46.))
                        .rounded(px(9.))
                        .child(
                            div()
                                .text_size(px(16.))
                                .font_weight(FontWeight::BOLD)
                                .child(dialog.cancel.clone()),
                        )
                        .on_click(move |_, window, cx| on_cancel(window, cx)),
                )
                .child(
                    Button::new("countdown-hide")
                        .debug_selector(|| "countdown-hide".to_owned())
                        .w_full()
                        .h(px(34.))
                        .rounded(px(9.))
                        .child(
                            div()
                                .font_weight(FontWeight::BOLD)
                                .child(dialog.hide.clone()),
                        )
                        .on_click(move |_, window, cx| on_hide(window, cx)),
                )
                .child(caption(dialog.hint.clone(), p).text_center()),
        );
    }
    div()
        // Tab stays in the dialog: the shell cycles the focus through `scope`.
        .track_focus(scope)
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        // The scrim takes the clicks: the screen under it does not react.
        .occlude()
        .bg(scrim(p))
        .child(frame)
        .into_any_element()
}
