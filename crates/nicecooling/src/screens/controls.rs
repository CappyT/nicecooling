//! Controls that the new screens share: a click handler on the view, and the select.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, ClickEvent, Context, Div, Entity, Focusable as _, FontWeight, Hsla,
    KeyDownEvent, MouseDownEvent, SharedString, Stateful, Window, div, px,
};
use gpui_component::Sizable as _;
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::{Input, InputState};

use crate::icons::{NcIcon, icon, text_icon};
use crate::screen_view::{InputKey, ScreenView};
use crate::screens::ui::{card_row, popover, select_trigger_with, with_popover};
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

impl ScreenView {
    /// The `close` of a popover: a press outside it closes the popovers of the screens.
    pub(crate) fn dismiss(
        view: &Entity<Self>,
    ) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        let view = view.clone();
        move |event, _, cx| {
            view.update(cx, |this, cx| {
                let renamed = this.sensors.rename.take().map(|(sensor, _)| sensor);
                this.closing_press = Some((event.position, renamed));
                this.close_popovers();
                cx.notify();
            });
        }
    }

    /// Close every open popover and menu of the screens. A dialog stays open.
    pub(crate) fn close_popovers(&mut self) {
        self.open_picker = None;
        self.history.picker_open = false;
        self.profiles.menu_open = false;
        self.profiles.rename = None;
        self.sensors.rename = None;
        self.channels.rename = None;
    }

    /// True when the press of `click` closed the rename popover of `sensor`. The rename
    /// button is not next to its popover, so it asks this: the click does not open the
    /// popover again.
    pub(crate) fn closed_rename_by(&self, click: &ClickEvent, sensor: &str) -> bool {
        match (click, &self.closing_press) {
            (ClickEvent::Mouse(mouse), Some((position, Some(closed)))) => {
                *position == mouse.down.position && closed == sensor
            }
            _ => false,
        }
    }
}

/// The key of the sensor picker of History: its open state is not in `open_picker`.
pub(crate) const HISTORY_PICKER: &str = "history";

/// What a key does in the open list of a select or a picker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ListKey {
    /// Up or Down: the row that the key marks.
    Mark(usize),
    /// Enter: pick the marked row.
    Pick,
    Close,
}

/// The meaning of `event` for an open list of `count` rows with the mark on `cursor`.
pub(crate) fn list_key(event: &KeyDownEvent, cursor: usize, count: usize) -> Option<ListKey> {
    let keys = &event.keystroke;
    if keys.modifiers.modified() {
        return None;
    }
    match keys.key.as_str() {
        "down" => Some(ListKey::Mark((cursor + 1).min(count.saturating_sub(1)))),
        "up" => Some(ListKey::Mark(cursor.saturating_sub(1))),
        "enter" => Some(ListKey::Pick),
        "escape" => Some(ListKey::Close),
        _ => None,
    }
}

impl ScreenView {
    /// True when the list of the select or the picker `key` is open.
    pub(crate) fn picker_open(&self, key: &str) -> bool {
        self.open_picker.as_deref() == Some(key)
            || (key == HISTORY_PICKER && self.history.picker_open)
    }

    /// The row that the keyboard marks in the list `key`. `start` is the row of a list that
    /// has no mark yet. The mark stays in the `count` rows.
    pub(crate) fn cursor(&self, key: &str, start: usize, count: usize) -> usize {
        let cursor = match &self.picker_cursor {
            Some((list, row)) if list == key => *row,
            _ => start,
        };
        cursor.min(count.saturating_sub(1))
    }

    /// Give the focus back to the select of a picker that the keyboard opened, when the
    /// picker closes. A focus that moved to another control stays there.
    pub(crate) fn return_picker_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((key, select, _)) = &self.picker_keys else {
            return;
        };
        if self.picker_open(key) {
            return;
        }
        let search = self.inputs.get(&InputKey::PickerSearch(key.clone()));
        if search.is_none_or(|search| search.read(cx).focus_handle(cx).is_focused(window)) {
            window.focus(select, cx);
        }
        self.picker_keys = None;
    }

    /// The handlers of the select of a sensor picker. A press, Enter and Space run `toggle`.
    /// Down opens a closed picker. A picker that the keyboard opens takes the focus in its
    /// search input: Up and Down then move in its list, Enter picks and Escape closes.
    pub(crate) fn opens_picker(
        trigger: Stateful<Div>,
        view: &Entity<Self>,
        key: &str,
        open: bool,
        toggle: impl Fn(&mut Self) + 'static,
    ) -> Stateful<Div> {
        let run = {
            let (view, key) = (view.clone(), key.to_owned());
            Rc::new(move |by_keys: bool, window: &mut Window, cx: &mut App| {
                let select = window.focused(cx).filter(|_| by_keys && !open);
                view.update(cx, |this, cx| {
                    if let Some(select) = select {
                        this.picker_cursor = None;
                        this.picker_keys = Some((key.clone(), select, false));
                    }
                    toggle(this);
                    cx.notify();
                });
            })
        };
        let from_key = run.clone();
        trigger
            .on_click(move |click, window, cx| run(click.is_keyboard(), window, cx))
            .capture_key_down(move |event, window, cx| {
                let keys = &event.keystroke;
                if !open && keys.key == "down" && !keys.modifiers.modified() {
                    from_key(true, window, cx);
                    cx.stop_propagation();
                }
            })
    }
}

/// One choice of a select: identifier, text and whether it is the current one.
pub(crate) type Choice = (String, String, bool);

/// What an input takes: a value, nothing because it does not apply now, or nothing because
/// its field is read-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FieldMode {
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

impl ScreenView {
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
        let (trigger_name, list_name) = (format!("select-{key}"), format!("select-list-{key}"));
        // The select keeps the focus while its list is open: the keys go to the select.
        let focus = enabled.then(|| {
            let focus = self
                .select_focus
                .entry(key.to_owned())
                .or_insert_with(|| cx.focus_handle().tab_stop(true))
                .clone();
            let focused = self.focused.as_ref() == Some(&focus);
            (focus, focused)
        });
        let current = choices.iter().position(|(_, _, selected)| *selected);
        let cursor = self.cursor(key, current.unwrap_or(0), choices.len());
        let ids: Vec<String> = choices.iter().map(|(id, _, _)| id.clone()).collect();
        let trigger = select_trigger_with(
            SharedString::from(format!("select-{key}")),
            shown.to_owned(),
            None,
            small,
            ground,
            focus,
        )
        .debug_selector(move || trigger_name)
        .when(enabled, |trigger| {
            let (view, name, pick) = (view.clone(), name.clone(), pick.clone());
            let toggle = name.clone();
            trigger
                .on_click(act(&view, move |this, _, _| {
                    this.picker_cursor = None;
                    this.open_picker = if this.open_picker.as_deref() == Some(toggle.as_str()) {
                        None
                    } else {
                        Some(toggle.clone())
                    };
                }))
                // Down opens the list. In the open list Up and Down move the mark, Enter and
                // Space pick the marked row. Escape goes to the shell: it closes the list.
                .capture_key_down(move |event, window, cx| {
                    let keys = &event.keystroke;
                    let action = if !open {
                        (keys.key == "down" && !keys.modifiers.modified())
                            .then_some(ListKey::Mark(cursor))
                    } else if keys.key == "space" && !keys.modifiers.modified() {
                        Some(ListKey::Pick)
                    } else {
                        list_key(event, cursor, ids.len()).filter(|key| *key != ListKey::Close)
                    };
                    let Some(action) = action else {
                        return;
                    };
                    cx.stop_propagation();
                    view.update(cx, |this, cx| {
                        match action {
                            ListKey::Mark(row) => {
                                this.open_picker = Some(name.clone());
                                this.picker_cursor = Some((name.clone(), row));
                            }
                            ListKey::Pick => {
                                this.open_picker = None;
                                if let Some(id) = ids.get(cursor) {
                                    pick(this, id.clone(), window, cx);
                                }
                            }
                            ListKey::Close => {}
                        }
                        cx.notify();
                    });
                })
        });
        let list = open.then(|| {
            let mut list = popover(p)
                .min_w(px(180.))
                .p(px(4.))
                .debug_selector(move || list_name);
            for (index, (id, text, selected)) in choices.into_iter().enumerate() {
                let pick = pick.clone();
                let view = view.clone();
                let hover = p.hover;
                let marked = p.accent_tint;
                let row_name = format!("choice-{key}-{index}");
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
                        // The row that the keyboard marks has the hover fill.
                        .when(index == cursor, move |row| {
                            row.bg(hover).debug_selector(move || row_name)
                        })
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
        with_popover(trigger, list, Self::dismiss(&view))
            .w_full()
            .into_any_element()
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

    /// A `field_input` in the look of its mode. `invalid` gives it the red outline of a value
    /// out of range.
    pub(crate) fn mode_input(
        state: &Entity<InputState>,
        unit: &str,
        mode: impl Into<FieldMode>,
        invalid: bool,
        small: bool,
        p: &Palette,
    ) -> Input {
        let mode = mode.into();
        let unit = (!unit.is_empty()).then(|| SharedString::from(unit.to_owned()));
        let input = Self::field_input(state, unit, small, p).disabled(mode != FieldMode::Editable);
        match mode {
            FieldMode::ReadOnly => input
                .bg(gpui::transparent_black())
                .border_dashed()
                .border_color(p.border_strong)
                .text_color(p.fg_dim),
            FieldMode::Off => input.opacity(0.5),
            FieldMode::Editable if invalid => input.border_color(p.alarm),
            FieldMode::Editable => input,
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
        let label: SharedString = label.into();
        let id = id.into();
        let name = id.to_string();
        // The label of a gpui-component button has the size of the button variant. The text
        // is a child here, so it takes the 13 px of the design.
        let button = Button::new(id)
            .debug_selector(move || name)
            .accessibility_label(label.clone())
            .h(px(28.))
            .px(px(10.))
            .rounded(px(7.))
            .font_weight(FontWeight::BOLD)
            .child(
                div()
                    .min_w_0()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_size(px(13.))
                    .child(label),
            );
        match icon {
            Some(icon) => button.icon(text_icon(icon)),
            None => button,
        }
    }

    /// A small button with an icon only: 28 px square, no fill.
    pub(crate) fn icon_button(id: impl Into<SharedString>, icon: NcIcon) -> Button {
        let id = id.into();
        let name = id.to_string();
        Button::new(id)
            .debug_selector(move || name)
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
