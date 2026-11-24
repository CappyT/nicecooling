//! The window of the Display tab: the grid of pages, the selected page, the settings and the
//! dialogs.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, Context, Focusable as _, FontWeight, SharedString, Window, div, px};
use gpui_component::button::ButtonVariants as _;
use gpui_component::slider::Slider;
use gpui_component::switch::Switch;
use gpui_component::{Disableable as _, Sizable as _, h_flex, v_flex};

use crate::bitmap::import_png_file;
use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::controls::{Pick, act};
use crate::screens::leakshield::display::{
    AUTO_INVERT, Action, DialogModel, DisplayModel, FlagSwitch, INVERT, ImportTarget, NO_ASSET,
    OFF_IN_STANDBY, ROTATE, SelectedPage, Switch as Toggle, set_brightness,
};
use crate::screens::leakshield::display_art::{framed, schematic, screen_view};
use crate::screens::leakshield::{Tab, TabState};
use crate::screens::ui::{
    Tip as _, caption, card, card_header, card_row, cell, field_row, modal, notice_row, page,
    table_header, table_row, write_line,
};
use crate::tokens::Palette;

/// The width of a select in a row of a card.
const SELECT: f32 = 240.0;
/// The widths of the columns of the value table.
const VALUE_NAME: f32 = 90.0;
const DECIMALS: f32 = 100.0;
const RANGE: f32 = 110.0;
/// The width of the name of a text, an icon or the logo.
const SLOT_NAME: f32 = 110.0;
/// The slider of the import dialog.
pub(crate) const THRESHOLD_SLIDER: &str = "ls:display-threshold";

impl ScreenView {
    /// Select a page of the Display tab and read its image from the device.
    pub(crate) fn select_display_page(&mut self, id: &str, page: usize, cx: &mut Context<Self>) {
        self.leakshield.display.select(page);
        self.read_display_preview(id, cx);
    }

    /// Read the image of the selected page, when the device takes the command.
    pub(crate) fn read_display_preview(&mut self, id: &str, cx: &mut Context<Self>) {
        let command = self.with_input(cx, |input| {
            self.leakshield.display.preview_command(input, id)
        });
        if let Some(command) = command {
            self.send(command);
        }
    }

    /// Run `f` on the state of the Display tab with the input of the screens, and send the
    /// command that it gives.
    fn display_command(
        &mut self,
        cx: &mut Context<Self>,
        f: impl FnOnce(
            &mut crate::screens::leakshield::display::DisplayUi,
            &crate::screens::ScreenInput<'_>,
        ) -> Option<crate::command::Command>,
    ) {
        let mut ui = std::mem::take(&mut self.leakshield.display);
        let command = self.with_input(cx, |input| f(&mut ui, input));
        self.leakshield.display = ui;
        if let Some(command) = command {
            self.send(command);
        }
    }

    pub(crate) fn render_ls_display(
        &mut self,
        id: &str,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = self.with_input(cx, |input| {
            DisplayModel::build(input, &self.leakshield.display, id)
        });
        let view = cx.entity();
        let mut content = page();
        if model.state == TabState::Loading {
            return content
                .child(card(p).p(px(18.)).gap(px(12.)).child(skeleton(p, 520.)))
                .into_any_element();
        }
        if let Some(reason) = &model.reason {
            content = content.child(card(p).child(notice_row(reason.clone(), true, p)));
        }

        // The grid: 5 pages in a row.
        let mut grid = v_flex().gap(px(6.)).px(px(12.)).pb(px(12.));
        for chunk in model.pages.chunks(5) {
            let mut row = h_flex().gap(px(6.)).items_start();
            for card_model in chunk {
                let index = card_model.index;
                let (view, device) = (view.clone(), id.to_owned());
                let mut tile = v_flex()
                    .id(SharedString::from(format!("ls-page-{index}")))
                    .debug_selector(move || format!("ls-page-{index}"))
                    .gap(px(3.))
                    .p(px(3.))
                    .rounded(px(9.))
                    .border_2()
                    .border_color(if card_model.selected {
                        p.accent_bg
                    } else {
                        gpui::transparent_black()
                    })
                    .cursor_pointer()
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.select_display_page(&device, index, cx);
                            cx.notify();
                        });
                    })
                    .child(screen_view(
                        &card_model.screen,
                        1.0,
                        card_model.off.is_some(),
                        p,
                    ))
                    .child(
                        h_flex()
                            .w(px(136.))
                            .gap(px(6.))
                            .text_size(px(12.))
                            .line_height(px(16.))
                            .child(
                                div()
                                    .flex_none()
                                    .font_weight(FontWeight::BOLD)
                                    .child(card_model.number.to_string()),
                            )
                            .child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_color(p.fg_dim)
                                    .child(card_model.template_name.clone()),
                            ),
                    );
                if let Some(text) = &card_model.off {
                    tile = tile.child(caption(text.clone(), p));
                }
                row = row.child(tile);
            }
            grid = grid.child(row);
        }
        content = content.child(
            card(p)
                .child(card_header(model.pages_title.clone(), p))
                .child(grid),
        );
        content = content.child(self.display_page_card(&model.page, id, p, window, cx));

        // The settings.
        let settings = &model.settings;
        let mut settings_card = card(p).child(card_header(settings.title.clone(), p));
        for choice in &settings.brightness {
            let pick: Pick = {
                let (device, field) = (id.to_owned(), choice.field.clone());
                Rc::new(move |this, level, _, cx| {
                    let command =
                        this.with_input(cx, |input| set_brightness(input, &device, &field, &level));
                    if let Some(command) = command {
                        this.send(command);
                    }
                })
            };
            let select = self.select_field(
                &format!("ls-{}", choice.field),
                &choice.shown,
                choice.options.clone(),
                choice.writable,
                false,
                p.track,
                pick,
                p,
                cx,
            );
            settings_card = settings_card.child(
                field_row(p)
                    .child(name_column(&choice.label, None, p))
                    .child(div().w(px(SELECT)).flex_none().child(select)),
            );
        }
        let interval = self.input(
            InputKey::Ls("display-interval".into()),
            &settings.interval,
            "",
            window,
            cx,
        );
        settings_card = settings_card
            .child(self.auto_page_row(&settings.auto_page, id, p, cx))
            .child(
                field_row(p)
                    .child(name_column(
                        &settings.interval_label,
                        Some(&settings.interval_text),
                        p,
                    ))
                    .child(div().w(px(104.)).flex_none().child(Self::mode_input(
                        &interval,
                        "s",
                        settings.interval_writable,
                        false,
                        false,
                        p,
                    ))),
            );
        for (flag, toggle) in [
            (ROTATE, &settings.rotate),
            (INVERT, &settings.invert),
            (AUTO_INVERT, &settings.auto_invert),
            (OFF_IN_STANDBY, &settings.standby),
        ] {
            let text = match flag {
                INVERT => settings.invert_text.as_deref(),
                AUTO_INVERT => Some(settings.auto_invert_text.as_str()),
                OFF_IN_STANDBY => Some(settings.standby_text.as_str()),
                _ => None,
            };
            settings_card = settings_card.child(self.flag_row(flag, toggle, text, id, p, cx));
        }
        if let Some(line) = &settings.write_line {
            settings_card = settings_card.child(card_row(p).child(write_line(line, p)));
        }
        content = content
            .child(settings_card)
            .child(caption(model.hint.clone(), p).px(px(6.)));

        let focus = self.ls_focus.clone();
        let mut root = div().id("ls-display").relative().w_full().child(content);
        if let Some(focus) = focus {
            let (key_view, device) = (view.clone(), id.to_owned());
            let grid_focus = focus.clone();
            root = root
                .track_focus(&focus)
                .on_key_down(move |event, window, cx| {
                    // The keys of the grid: an input or a select of the tab keeps its own keys.
                    if !grid_focus.is_focused(window) {
                        return;
                    }
                    let (dx, dy) = match event.keystroke.key.as_str() {
                        "left" => (-1, 0),
                        "right" => (1, 0),
                        "up" => (0, -1),
                        "down" => (0, 1),
                        "enter" => {
                            key_view.update(cx, |this, cx| {
                                this.leakshield.display.gallery = true;
                                cx.notify();
                            });
                            return;
                        }
                        _ => return,
                    };
                    key_view.update(cx, |this, cx| {
                        let before = this.leakshield.display.page;
                        this.leakshield.display.step(dx, dy);
                        if this.leakshield.display.page != before {
                            this.read_display_preview(&device, cx);
                        }
                        cx.notify();
                    });
                });
        }
        if let Some(dialog) = &model.dialog {
            root = root.child(self.display_dialog(dialog, id, p, window, cx));
        }
        root.into_any_element()
    }

    /// The card of the selected page: the preview, the template, the two options and the slots
    /// that the template uses.
    fn display_page_card(
        &mut self,
        selected: &SelectedPage,
        id: &str,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let preview = &selected.preview;
        let refresh = {
            let device = id.to_owned();
            action_button("ls-preview-refresh", &preview.refresh)
                .on_click(act(&view, move |this, _, cx| {
                    this.read_display_preview(&device, cx)
                }))
        };
        let left = v_flex()
            .flex_none()
            .debug_selector(|| "display-preview-panel".to_owned())
            .w(px(300.))
            .p(px(18.))
            .gap(px(6.))
            .child(screen_view(&preview.screen, 2.0, false, p))
            .when_some(preview.note.clone(), |column, note| {
                column.child(
                    div()
                        .min_w_0()
                        .text_size(px(12.))
                        .line_height(px(16.))
                        .debug_selector(|| "display-preview-note".to_owned())
                        .child(note),
                )
            })
            .child(caption(preview.caption.clone(), p))
            .child(h_flex().child(refresh));

        let mut rows = v_flex().flex_1().min_w_0().child(
            field_row(p)
                .border_t_0()
                .child(
                    div()
                        .flex_none()
                        .font_weight(FontWeight::BOLD)
                        .child(selected.template_label.clone()),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(selected.template_name.clone()),
                )
                .child(
                    div()
                        .flex_none()
                        .debug_selector(|| "display-template-change".to_owned())
                        .child(
                            action_button("ls-template-change", &selected.change)
                                .on_click(act(&view, |this, _, _| {
                                    this.leakshield.display.gallery = true
                                })),
                        ),
                ),
        );
        for (key, toggle, bit) in [
            ("standby", &selected.standby, 1u32),
            ("hide-normal", &selected.hide_normal, 2u32),
        ] {
            rows = rows.child(self.toggle_row(&format!("ls-option-{key}"), toggle, id, bit, p, cx));
        }
        let mut page_card = card(p).child(card_header(selected.title.clone(), p)).child(
            h_flex()
                .w_full()
                .items_start()
                .border_t_1()
                .border_color(p.border)
                .child(left)
                .child(rows),
        );

        // The values: data source, decimals and the range of a bar.
        if !selected.rows.is_empty() {
            let ranges = selected.value_columns.len() > 3;
            let mut widths = vec![Some(VALUE_NAME), None, Some(DECIMALS)];
            if ranges {
                widths.extend([Some(RANGE), Some(RANGE)]);
            }
            page_card = page_card.child(table_header(&selected.value_columns, &widths, p));
        }
        let page_index = self.leakshield.display.page;
        for row in &selected.rows {
            let slot = row.slot;
            let source_pick: Pick = {
                let device = id.to_owned();
                Rc::new(move |this, choice, _, cx| {
                    if let Ok(source) = choice.parse::<u8>() {
                        this.display_command(cx, |ui, input| {
                            ui.set_source(input, &device, slot, source)
                        });
                    }
                })
            };
            let source = self.select_field(
                &format!("ls-source-{slot}"),
                &row.source_text,
                row.sources
                    .iter()
                    .map(|(index, text, current)| (index.to_string(), text.clone(), *current))
                    .collect(),
                selected.sources_writable,
                true,
                p.track,
                source_pick,
                p,
                cx,
            );
            let decimals_pick: Pick = {
                let device = id.to_owned();
                Rc::new(move |this, choice, _, cx| {
                    if let Ok(decimals) = choice.parse::<u8>() {
                        this.display_command(cx, |ui, input| {
                            ui.set_decimals(input, &device, slot, decimals)
                        });
                    }
                })
            };
            let decimals = self.select_field(
                &format!("ls-decimals-{slot}"),
                &row.decimals_text,
                row.decimals
                    .iter()
                    .map(|(number, text, current)| (number.to_string(), text.clone(), *current))
                    .collect(),
                selected.decimals_writable,
                true,
                p.track,
                decimals_pick,
                p,
                cx,
            );
            let mut line = table_row(p)
                .child(
                    cell(VALUE_NAME)
                        .font_weight(FontWeight::BOLD)
                        .child(row.label.clone()),
                )
                .child(div().flex_1().min_w_0().child(source))
                .child(
                    div()
                        .w(px(DECIMALS))
                        .flex_none()
                        .debug_selector(move || format!("display-decimals-{slot}"))
                        .child(decimals),
                );
            if selected.value_columns.len() > 3 {
                for (which, text) in [
                    ("min", row.range.as_ref().map(|r| &r.0)),
                    ("max", row.range.as_ref().map(|r| &r.1)),
                ] {
                    let mut range_cell = div()
                        .w(px(RANGE))
                        .flex_none()
                        .debug_selector(move || format!("display-{which}-{slot}"));
                    if let Some(text) = text {
                        let input = self.input(
                            InputKey::Ls(format!("display-{which}:{page_index}:{slot}")),
                            text,
                            "",
                            window,
                            cx,
                        );
                        range_cell = range_cell.child(Self::mode_input(
                            &input,
                            "",
                            selected.ranges_writable,
                            false,
                            true,
                            p,
                        ));
                    }
                    line = line.child(range_cell);
                }
            }
            page_card = page_card.child(line);
            if let Some(text) = &row.warning {
                page_card = page_card.child(
                    card_row(p)
                        .text_color(p.warning)
                        .child(div().flex_1().min_w_0().child(text.clone()))
                        .child(
                            Self::link_button(
                                SharedString::from(format!("ls-open-sensors-{slot}")),
                                selected.link.clone(),
                                p,
                            )
                            .on_click(act(&view, |this, _, _| this.leakshield.tab = Tab::Sensors)),
                        ),
                );
            }
        }

        // The texts: one of the names of the device, or an own text.
        for row in &selected.texts {
            let slot = row.slot;
            let pick: Pick = {
                let device = id.to_owned();
                Rc::new(move |this, choice, _, cx| {
                    if let Ok(name) = choice.parse::<u8>() {
                        this.display_command(cx, |ui, input| {
                            ui.set_text(input, &device, slot, name)
                        });
                    }
                })
            };
            let select = self.select_field(
                &format!("ls-text-{slot}"),
                &row.shown,
                row.choices
                    .iter()
                    .map(|(index, text, current)| (index.to_string(), text.clone(), *current))
                    .collect(),
                selected.texts_writable,
                true,
                p.track,
                pick,
                p,
                cx,
            );
            page_card = page_card.child(
                card_row(p)
                    .child(
                        cell(SLOT_NAME)
                            .font_weight(FontWeight::BOLD)
                            .child(row.label.clone()),
                    )
                    .child(div().w(px(SELECT)).flex_none().child(select))
                    .child(
                        div()
                            .flex_none()
                            .debug_selector(move || format!("display-own-text-{slot}"))
                            .child(
                                action_button(
                                    SharedString::from(format!("ls-own-text-{slot}")),
                                    &row.own,
                                )
                                .on_click(act(
                                    &view,
                                    move |this, _, _| {
                                        this.leakshield.display.open_own_text(slot);
                                        this.focus_input =
                                            Some(InputKey::Ls("display-own-text".into()));
                                    },
                                )),
                            ),
                    )
                    .child(div().flex_1()),
            );
        }

        // The icons.
        for row in &selected.icons {
            let slot = row.slot;
            page_card = page_card.child(
                card_row(p)
                    .child(
                        cell(SLOT_NAME)
                            .font_weight(FontWeight::BOLD)
                            .child(row.label.clone()),
                    )
                    .children(row.bitmap.as_ref().map(|bitmap| framed(bitmap, 1.0, p)))
                    .child(div().flex_1().min_w_0().child(row.name.clone()))
                    .child(
                        div()
                            .flex_none()
                            .debug_selector(move || format!("display-change-icon-{slot}"))
                            .child(
                                action_button(
                                    SharedString::from(format!("ls-change-icon-{slot}")),
                                    &row.change,
                                )
                                .on_click(act(&view, move |this, _, _| {
                                    this.leakshield.display.open_icons(slot)
                                })),
                            ),
                    ),
            );
        }

        // The logo.
        if let Some(logo) = &selected.logo {
            page_card = page_card.child(
                card_row(p)
                    .child(
                        cell(SLOT_NAME)
                            .font_weight(FontWeight::BOLD)
                            .child(logo.label.clone()),
                    )
                    .children(logo.bitmap.as_ref().map(|bitmap| framed(bitmap, 1.0, p)))
                    .child(caption(logo.text.clone(), p).flex_1())
                    .child(
                        div()
                            .flex_none()
                            .debug_selector(|| "display-change-logo".to_owned())
                            .child(action_button("ls-change-logo", &logo.change).on_click(act(
                                &view,
                                |this, _, _| {
                                    this.leakshield.display.open_import(ImportTarget::Logo)
                                },
                            ))),
                    ),
            );
        }
        if let Some(text) = &selected.no_settings {
            page_card = page_card.child(card_row(p).child(caption(text.clone(), p)));
        }
        if let Some(line) = &selected.write_line {
            page_card = page_card.child(card_row(p).child(write_line(line, p)));
        }
        page_card.into_any_element()
    }

    /// The dialog that is open over the tab.
    fn display_dialog(
        &mut self,
        dialog: &DialogModel,
        id: &str,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let cancel_button = |label: &str| {
            Self::small_button("ls-display-cancel", label.to_owned(), None)
                .on_click(act(&view, |this, _, _| {
                    this.leakshield.display.close_dialog()
                }))
                .into_any_element()
        };
        match dialog {
            DialogModel::Gallery {
                title,
                items,
                cancel,
            } => {
                let mut grid = v_flex().gap(px(6.));
                for chunk in items.chunks(3) {
                    let mut row = h_flex().gap(px(6.)).items_start();
                    for item in chunk {
                        let (device, template) = (id.to_owned(), item.template.clone());
                        let name = format!("ls-gallery-{}", item.template);
                        row = row.child(
                            v_flex()
                                .id(SharedString::from(name.clone()))
                                .debug_selector(move || name)
                                .w(px(114.))
                                .gap(px(2.))
                                .p(px(3.))
                                .rounded(px(9.))
                                .border_2()
                                .border_color(if item.selected {
                                    p.accent_bg
                                } else {
                                    gpui::transparent_black()
                                })
                                .cursor_pointer()
                                .on_click(act(&view, move |this, _, cx| {
                                    this.display_command(cx, |ui, input| {
                                        ui.set_template(input, &device, &template)
                                    });
                                }))
                                .child(schematic(&item.template, p))
                                .child(
                                    div()
                                        .w_full()
                                        .min_w_0()
                                        .truncate()
                                        .text_size(px(12.))
                                        .line_height(px(16.))
                                        .child(item.label.clone()),
                                ),
                        );
                    }
                    grid = grid.child(row);
                }
                modal(
                    title.clone(),
                    String::new(),
                    vec![grid.into_any_element()],
                    vec![cancel_button(cancel)],
                    p,
                )
                .into_any_element()
            }
            DialogModel::OwnText {
                title,
                text,
                value,
                note,
                save,
                cancel,
            } => {
                let key = InputKey::Ls("display-own-text".into());
                let input = self.input(key.clone(), value, "", window, cx);
                // The dialog takes the focus when it opens. The input gets it after that frame,
                // so the user types at once.
                if self.focus_input.as_ref() == Some(&key) {
                    self.focus_input = None;
                    let input = input.clone();
                    cx.defer_in(window, move |_, window, cx| {
                        let focus = input.read(cx).focus_handle(cx);
                        window.focus(&focus, cx);
                    });
                }
                let mut body = vec![Self::field_input(&input, None, false, p).into_any_element()];
                if let Some(note) = note {
                    body.push(caption(note.clone(), p).into_any_element());
                }
                if let Some(reason) = &save.reason {
                    body.push(
                        div()
                            .text_size(px(12.))
                            .line_height(px(16.))
                            .text_color(p.warning)
                            .child(reason.clone())
                            .into_any_element(),
                    );
                }
                let device = id.to_owned();
                modal(
                    title.clone(),
                    text.clone(),
                    body,
                    vec![
                        cancel_button(cancel),
                        action_button("ls-own-text-save", save)
                            .on_click(act(&view, move |this, _, cx| {
                                this.display_command(cx, |ui, input| {
                                    ui.save_own_text(input, &device)
                                });
                            }))
                            .into_any_element(),
                    ],
                    p,
                )
                .into_any_element()
            }
            DialogModel::Icons {
                title,
                text,
                icons,
                none,
                import,
                cancel,
            } => {
                let mut grid = v_flex().gap(px(4.));
                for chunk in icons.chunks(6) {
                    let mut row = h_flex().gap(px(4.));
                    for (index, bitmap, current) in chunk {
                        let (device, index) = (id.to_owned(), *index);
                        let slot = self.icon_dialog_slot();
                        row = row.child(
                            div()
                                .id(SharedString::from(format!("ls-icon-{index}")))
                                .p(px(2.))
                                .rounded(px(9.))
                                .border_2()
                                .border_color(if *current {
                                    p.accent_bg
                                } else {
                                    gpui::transparent_black()
                                })
                                .when(none.enabled, |tile| {
                                    tile.cursor_pointer().on_click(act(
                                        &view,
                                        move |this, _, cx| {
                                            this.display_command(cx, |ui, input| {
                                                ui.set_icon(input, &device, slot, index)
                                            });
                                        },
                                    ))
                                })
                                .child(framed(bitmap, 2.0, p)),
                        );
                    }
                    grid = grid.child(row);
                }
                let slot = self.icon_dialog_slot();
                let device = id.to_owned();
                let actions = h_flex()
                    .gap(px(8.))
                    .child(action_button("ls-icon-none", none).on_click(act(
                        &view,
                        move |this, _, cx| {
                            this.display_command(cx, |ui, input| {
                                ui.set_icon(input, &device, slot, NO_ASSET)
                            });
                        },
                    )))
                    .child(action_button("ls-icon-import", import).on_click(act(
                        &view,
                        move |this, _, _| {
                            this.leakshield
                                .display
                                .open_import(ImportTarget::Icon { slot })
                        },
                    )));
                let mut body = vec![grid.into_any_element(), actions.into_any_element()];
                if let Some(reason) = import.reason.as_ref().or(none.reason.as_ref()) {
                    body.push(caption(reason.clone(), p).into_any_element());
                }
                modal(
                    title.clone(),
                    text.clone(),
                    body,
                    vec![cancel_button(cancel)],
                    p,
                )
                .into_any_element()
            }
            DialogModel::Import {
                title,
                text,
                choose,
                file,
                result,
                note,
                error,
                threshold_label,
                threshold,
                invert,
                write,
                cancel,
            } => {
                let pick = Self::small_button("ls-import-choose", choose.clone(), None)
                    .on_click(act(&view, |this, _, cx| this.pick_display_image(cx)));
                let mut body = vec![
                    h_flex()
                        .gap(px(8.))
                        .child(pick)
                        .child(
                            caption(file.clone().unwrap_or_default(), p)
                                .flex_1()
                                .truncate(),
                        )
                        .into_any_element(),
                ];
                if let Some(error) = error {
                    body.push(
                        div()
                            .text_color(p.alarm)
                            .debug_selector(|| "display-import-error".to_owned())
                            .child(error.clone())
                            .into_any_element(),
                    );
                }
                if let Some(note) = note {
                    body.push(caption(note.clone(), p).into_any_element());
                }
                if let Some((bitmap, scale)) = result {
                    body.push(
                        h_flex()
                            .justify_center()
                            .child(
                                framed(bitmap, *scale as f32, p)
                                    .debug_selector(|| "display-import-result".to_owned()),
                            )
                            .into_any_element(),
                    );
                    let slider = self.slider(THRESHOLD_SLIDER, f32::from(*threshold), window, cx);
                    body.push(
                        v_flex()
                            .gap(px(3.))
                            .child(caption(threshold_label.clone(), p))
                            .child(Slider::new(&slider))
                            .into_any_element(),
                    );
                    let toggle = view.clone();
                    body.push(
                        h_flex()
                            .gap(px(12.))
                            .child(div().flex_1().min_w_0().child(invert.label.clone()))
                            .child(
                                Switch::new("ls-import-invert")
                                    .large()
                                    .checked(invert.on)
                                    .on_click(move |on, _, cx| {
                                        let on = *on;
                                        toggle.update(cx, |this, cx| {
                                            this.leakshield.display.set_invert(on);
                                            cx.notify();
                                        });
                                    }),
                            )
                            .into_any_element(),
                    );
                }
                if let Some(reason) = write.reason.as_ref().filter(|_| result.is_some()) {
                    body.push(caption(reason.clone(), p).into_any_element());
                }
                let device = id.to_owned();
                modal(
                    title.clone(),
                    text.clone(),
                    body,
                    vec![
                        cancel_button(cancel),
                        action_button("ls-import-write", write)
                            .primary()
                            .on_click(act(&view, move |this, _, cx| {
                                this.display_command(cx, |ui, input| {
                                    ui.write_import(input, &device)
                                });
                            }))
                            .into_any_element(),
                    ],
                    p,
                )
                .into_any_element()
            }
        }
    }

    /// The icon slot of the open icon dialog.
    fn icon_dialog_slot(&self) -> usize {
        match &self.leakshield.display.dialog {
            Some(crate::screens::leakshield::display::Dialog::Icons { slot }) => *slot,
            _ => 0,
        }
    }

    /// "Choose a file" of the import dialog: the file dialog of the system, then the decode of
    /// the file in the background.
    fn pick_display_image(&mut self, cx: &mut Context<Self>) {
        let Some(crate::screens::leakshield::display::Dialog::Import(import)) =
            &self.leakshield.display.dialog
        else {
            return;
        };
        let (width, height) = import.target.size();
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(mut paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.pop() else { return };
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let result = cx
                .background_executor()
                .spawn(async move { import_png_file(&path, width, height) })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.leakshield.display.import_loaded(name, result);
                cx.notify();
            });
        })
        .detach();
    }

    /// A switch of `page_options` of the selected page.
    fn toggle_row(
        &mut self,
        id_text: &str,
        toggle: &Toggle,
        device: &str,
        bit: u32,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let device = device.to_owned();
        field_row(p)
            .child(name_column(&toggle.label, None, p))
            .child(
                Switch::new(SharedString::from(id_text.to_owned()))
                    .large()
                    .checked(toggle.on)
                    .disabled(!toggle.writable)
                    .on_click(move |on, _, cx| {
                        let (on, device) = (*on, device.clone());
                        view.update(cx, |this, cx| {
                            this.display_command(cx, |ui, input| {
                                ui.set_option(input, &device, bit, on)
                            });
                        });
                    }),
            )
            .into_any_element()
    }

    /// The switch "Automatic page change": off is 30 s in `display_page_time`.
    fn auto_page_row(
        &mut self,
        toggle: &Toggle,
        device: &str,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let device = device.to_owned();
        field_row(p)
            .child(name_column(&toggle.label, None, p))
            .child(
                Switch::new("ls-flag-auto_page")
                    .large()
                    .checked(toggle.on)
                    .disabled(!toggle.writable)
                    .on_click(move |on, _, cx| {
                        let (on, device) = (*on, device.clone());
                        view.update(cx, |this, cx| {
                            let mut ui = std::mem::take(&mut this.leakshield.display);
                            let command =
                                this.with_input(cx, |input| ui.set_auto_page(input, &device, on));
                            this.leakshield.display = ui;
                            if let Some(command) = command {
                                this.send(command);
                            }
                        });
                    }),
            )
            .into_any_element()
    }

    /// A switch of `display_flags`.
    fn flag_row(
        &mut self,
        flag: FlagSwitch,
        toggle: &Toggle,
        text: Option<&str>,
        device: &str,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let device = device.to_owned();
        field_row(p)
            .child(name_column(&toggle.label, text, p))
            .child(
                Switch::new(SharedString::from(format!("ls-flag-{}", flag.key)))
                    .large()
                    .checked(toggle.on)
                    .disabled(!toggle.writable)
                    .on_click(move |on, _, cx| {
                        let (on, device) = (*on, device.clone());
                        view.update(cx, |this, cx| {
                            let command = this.with_input(cx, |input| flag.set(input, &device, on));
                            if let Some(command) = command {
                                this.send(command);
                            }
                        });
                    }),
            )
            .into_any_element()
    }
}

/// The name of a setting in bold, and its caption under it.
fn name_column(name: &str, text: Option<&str>, p: &Palette) -> gpui::Div {
    v_flex()
        .flex_1()
        .min_w_0()
        .gap(px(3.))
        .child(div().font_weight(FontWeight::BOLD).child(name.to_owned()))
        .when_some(text, |column, text| {
            column.child(caption(text.to_owned(), p))
        })
}

/// A small button of a card, disabled with its reason as the tooltip.
fn action_button(id: impl Into<SharedString>, action: &Action) -> gpui_component::button::Button {
    ScreenView::small_button(id, action.label.clone(), None)
        .disabled(!action.enabled)
        .tip(action.reason.clone().unwrap_or_default())
}
