//! Controls that the new screens share: a click handler on the view, and the select.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, Hsla, SharedString, Window, div, px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputState};
use gpui_component::{Disableable as _, Sizable as _, v_flex};

use crate::icons::{NcIcon, icon, text_icon};
use crate::screen_view::ScreenView;
use crate::screens::ui::{card_row, popover, select_trigger, with_popover};
use crate::tokens::Palette;

pub(crate) type Pick = Rc<dyn Fn(&mut ScreenView, String, &mut Window, &mut Context<ScreenView>)>;

/// A click handler that runs `f` on the view and redraws.
pub(crate) fn act(
    view: &Entity<ScreenView>,
    f: impl Fn(&mut ScreenView, &mut Window, &mut Context<ScreenView>) + 'static,
) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let view = view.clone();
    move |_, window, cx| {
        view.update(cx, |this, cx| {
            f(this, window, cx);
            cx.notify();
        });
    }
}

/// One choice of a select: identifier, text and whether it is the current one.
pub(crate) type Choice = (String, String, bool);

impl ScreenView {
    /// A select: a button with the shown text. A click opens the list of choices under it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn select(
        &mut self,
        key: &str,
        shown: &str,
        choices: Vec<Choice>,
        enabled: bool,
        tooltip: &str,
        pick: Pick,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let open = self.open_picker.as_deref() == Some(key);
        let name = key.to_owned();
        let mut column = v_flex().gap(px(2.)).child(
            Button::new(SharedString::from(format!("select-{key}")))
                .small()
                .label(shown.to_owned())
                .child(text_icon(NcIcon::Down))
                .disabled(!enabled)
                .tooltip(tooltip.to_owned())
                .on_click(act(&view, move |this, _, _| {
                    this.open_picker = if this.open_picker.as_deref() == Some(name.as_str()) {
                        None
                    } else {
                        Some(name.clone())
                    };
                })),
        );
        if open && enabled {
            let mut list = v_flex()
                .min_w(px(180.))
                .rounded(px(9.))
                .bg(p.card)
                .border_1()
                .border_color(p.border)
                .p(px(4.));
            for (index, (id, text, selected)) in choices.into_iter().enumerate() {
                let pick = pick.clone();
                let view = view.clone();
                let hover = p.hover;
                let marked = p.accent_tint;
                list = list.child(
                    div()
                        .id(SharedString::from(format!("choice-{key}-{index}")))
                        .h(px(30.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .rounded(px(6.))
                        .cursor_pointer()
                        .hover(move |row| row.bg(hover))
                        .when(selected, move |row| {
                            row.bg(marked).font_weight(FontWeight::BOLD)
                        })
                        .on_click(move |_, window, cx| {
                            let (id, pick) = (id.clone(), pick.clone());
                            view.update(cx, |this, cx| {
                                this.open_picker = None;
                                pick(this, id, window, cx);
                                cx.notify();
                            });
                        })
                        .child(text),
                );
            }
            column = column.child(list);
        }
        column.into_any_element()
    }

    /// A select with the look of the design: the `ground` fill, the text at the left and a
    /// chevron at the right. It takes the width of its container. The list of choices opens
    /// in a popover under it.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn select_field(
        &mut self,
        key: &str,
        shown: &str,
        choices: Vec<Choice>,
        enabled: bool,
        small: bool,
        ground: Hsla,
        pick: Pick,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let open = enabled && self.open_picker.as_deref() == Some(key);
        let name = key.to_owned();
        let trigger = select_trigger(
            SharedString::from(format!("select-{key}")),
            shown.to_owned(),
            None,
            small,
            enabled,
            ground,
        )
        .when(enabled, |trigger| {
            trigger.on_click(act(&view, move |this, _, _| {
                this.open_picker = if this.open_picker.as_deref() == Some(name.as_str()) {
                    None
                } else {
                    Some(name.clone())
                };
            }))
        });
        let list = open.then(|| {
            let mut list = popover(p).min_w(px(180.)).p(px(4.));
            for (index, (id, text, selected)) in choices.into_iter().enumerate() {
                let pick = pick.clone();
                let view = view.clone();
                let hover = p.hover;
                let marked = p.accent_tint;
                list = list.child(
                    div()
                        .id(SharedString::from(format!("choice-{key}-{index}")))
                        .h(px(30.))
                        .px(px(10.))
                        .flex()
                        .items_center()
                        .rounded(px(6.))
                        .whitespace_nowrap()
                        .cursor_pointer()
                        .hover(move |row| row.bg(hover))
                        .when(selected, move |row| {
                            row.bg(marked).font_weight(FontWeight::BOLD)
                        })
                        .on_click(move |_, window, cx| {
                            let (id, pick) = (id.clone(), pick.clone());
                            view.update(cx, |this, cx| {
                                this.open_picker = None;
                                pick(this, id, window, cx);
                                cx.notify();
                            });
                        })
                        .child(text),
                );
            }
            list.into_any_element()
        });
        with_popover(trigger, list).w_full().into_any_element()
    }

    /// A text input with the look of the design: the `track` fill, no outline, and the unit
    /// at the right in `fg-dim`. 34 px high, or 28 px for an inline input.
    pub(crate) fn field_input(
        state: &Entity<InputState>,
        unit: Option<SharedString>,
        small: bool,
        p: &Palette,
    ) -> Input {
        let input = Input::new(state)
            .bg(p.track)
            .border_color(p.track)
            .min_h(px(if small { 28. } else { 34. }))
            .rounded(px(if small { 7. } else { 9. }));
        let input = if small { input.small() } else { input };
        match unit {
            Some(unit) => input.suffix(
                div()
                    .flex_none()
                    .whitespace_nowrap()
                    .text_color(p.fg_dim)
                    .child(unit),
            ),
            None => input,
        }
    }

    /// A button of 34 px with bold text: the default button of the design.
    pub(crate) fn button(id: impl Into<SharedString>, label: impl Into<SharedString>) -> Button {
        Button::new(id.into())
            .label(label.into())
            .h(px(34.))
            .px(px(15.))
            .font_weight(FontWeight::BOLD)
    }

    /// A small button: 28 px, 13 px bold text, 7 px radius. `icon` goes before the label.
    pub(crate) fn small_button(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        icon: Option<NcIcon>,
    ) -> Button {
        let button = Button::new(id.into())
            .label(label.into())
            .h(px(28.))
            .px(px(10.))
            .rounded(px(7.))
            .text_size(px(13.))
            .font_weight(FontWeight::BOLD);
        match icon {
            Some(icon) => button.icon(text_icon(icon)),
            None => button,
        }
    }

    /// A small button with an icon only: 28 px square, no fill.
    pub(crate) fn icon_button(id: impl Into<SharedString>, icon: NcIcon) -> Button {
        Button::new(id.into())
            .ghost()
            .icon(text_icon(icon))
            .size(px(28.))
            .rounded(px(7.))
    }

    /// A link of a card: accent, bold, no fill. It opens a page, a tab or a dialog.
    pub(crate) fn link_button(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
        p: &Palette,
    ) -> Button {
        // The text variant has no fill, no padding and no underline. The label has its own
        // color, so a hover does not change it.
        Button::new(id.into()).text().child(
            div()
                .text_color(p.accent)
                .font_weight(FontWeight::BOLD)
                .child(label.into()),
        )
    }

    /// A flat button: no fill. It is for a secondary command of a row.
    pub(crate) fn flat_button(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
    ) -> Button {
        Self::small_button(id, label, None).ghost()
    }

    /// A row of a card that opens a page: the link at the left and a chevron at the right.
    /// A click anywhere in the row opens the page.
    pub(crate) fn link_row(
        id: &'static str,
        label: impl Into<SharedString>,
        open: impl Fn(&mut Window, &mut App) + 'static,
        p: &Palette,
    ) -> AnyElement {
        let open = Rc::new(open);
        let from_link = open.clone();
        let hover = p.hover;
        card_row(p)
            .id(SharedString::from(format!("{id}-row")))
            .cursor_pointer()
            .hover(move |row| row.bg(hover))
            .on_click(move |_, window, cx| open(window, cx))
            .child(
                Self::link_button(id, label, p).on_click(move |_, window, cx| {
                    cx.stop_propagation();
                    from_link(window, cx);
                }),
            )
            .child(div().flex_1())
            .child(icon(NcIcon::Right, p.fg_dim))
            .into_any_element()
    }
}
