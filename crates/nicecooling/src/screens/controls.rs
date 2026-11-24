//! Controls that the new screens share: a click handler on the view, and the select.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Context, Entity, FontWeight, SharedString, Window, div, px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{Disableable as _, Sizable as _, v_flex};

use crate::icons::{NcIcon, text_icon};
use crate::screen_view::ScreenView;
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

    /// A ghost button, used for the small actions of a row.
    pub(crate) fn link_button(
        id: impl Into<SharedString>,
        label: impl Into<SharedString>,
    ) -> Button {
        Button::new(id.into()).small().ghost().label(label.into())
    }
}
