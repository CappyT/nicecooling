//! The window of the Display tab: the grid of pages, the selected page and the settings.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, px};
use gpui_component::switch::Switch;
use gpui_component::{Disableable as _, Sizable as _, h_flex, v_flex};

use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::controls::{Pick, act};
use crate::screens::leakshield::display::{
    DisplayModel, PageCard, Switch as Toggle, set_brightness, set_flag,
};
use crate::screens::leakshield::parts::{field_input, field_row, small_button};
use crate::screens::leakshield::{Tab, TabState};
use crate::screens::ui::{
    caption, card, card_header, card_row, modal, notice_row, page, write_line,
};
use crate::tokens::Palette;

impl ScreenView {
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
                let view = view.clone();
                let screen = preview(card_model, 1.0);
                let mut tile = v_flex()
                    .id(SharedString::from(format!("ls-page-{index}")))
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
                            this.leakshield.display.select(index);
                            cx.notify();
                        });
                    })
                    .child(screen)
                    .child(
                        h_flex()
                            .w(px(128.))
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

        // The selected page.
        let selected = &model.page;
        let gallery = {
            let view = view.clone();
            move |_: &gpui::ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                view.update(cx, |this, cx| {
                    this.leakshield.display.gallery = true;
                    cx.notify();
                });
            }
        };
        let large = model
            .pages
            .iter()
            .find(|page| page.selected)
            .map(|page| preview(page, 2.0));
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
                    small_button(
                        "ls-template-change",
                        selected
                            .choose
                            .clone()
                            .unwrap_or_else(|| selected.change.clone()),
                    )
                    .on_click(act(&view, |this, _, _| {
                        this.leakshield.display.gallery = true
                    })),
                ),
        );
        for row in &selected.rows {
            let slot = row.slot;
            let pick: Pick = {
                let device = id.to_owned();
                Rc::new(move |this, choice, _, cx| {
                    if let Ok(source) = choice.parse::<u8>() {
                        let command = this.with_input(cx, |input| {
                            this.leakshield
                                .display
                                .set_source(input, &device, slot, source)
                        });
                        if let Some(command) = command {
                            this.send(command);
                        }
                    }
                })
            };
            let select = self.select(
                &format!("ls-source-{slot}"),
                &row.source_text,
                row.sources
                    .iter()
                    .map(|(index, text, current)| (index.to_string(), text.clone(), *current))
                    .collect(),
                true,
                "",
                pick,
                p,
                cx,
            );
            let mut line = field_row(p)
                .child(
                    div()
                        .w(px(110.))
                        .flex_none()
                        .font_weight(FontWeight::BOLD)
                        .child(row.label.clone()),
                )
                .child(select)
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_right()
                        .text_color(p.fg_dim)
                        .child(row.value.clone()),
                );
            if let Some(text) = &row.warning {
                line = line.child(
                    h_flex()
                        .gap(px(8.))
                        .text_color(p.warning)
                        .child(text.clone())
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
            rows = rows.child(line);
        }
        if let Some(text) = &selected.logo_note {
            rows = rows.child(notice_row(text.clone(), false, p));
        }
        // "Show in normal operation" is the bit that hides the page: its value is inverted.
        for (key, toggle, bit, invert) in [
            ("normal", &selected.normal, 2u32, true),
            ("standby", &selected.standby, 1u32, false),
        ] {
            rows = rows.child(self.toggle_row(
                &format!("ls-option-{key}"),
                toggle,
                id,
                bit,
                invert,
                p,
                cx,
            ));
        }
        if let Some(line) = &selected.write_line {
            rows = rows.child(card_row(p).child(write_line(line, p)));
        }
        // The large preview at the left opens the template gallery.
        let page_card = card(p).child(card_header(selected.title.clone(), p)).child(
            h_flex()
                .w_full()
                .items_start()
                .border_t_1()
                .border_color(p.border)
                .when_some(large, |body, large| {
                    body.child(
                        div()
                            .id("ls-page-preview")
                            .flex_none()
                            .p(px(18.))
                            .cursor_pointer()
                            .on_click(gallery)
                            .child(large),
                    )
                })
                .child(rows),
        );
        content = content.child(page_card);

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
            let select = self.select(
                &format!("ls-{}", choice.field),
                &choice.shown,
                choice.options.clone(),
                choice.writable,
                "",
                pick,
                p,
                cx,
            );
            settings_card = settings_card.child(
                field_row(p)
                    .child(name_column(&choice.label, None, p))
                    .child(select),
            );
        }
        let interval = self.input(
            InputKey::Ls("display-interval".into()),
            &settings.interval,
            "",
            window,
            cx,
        );
        for (key, toggle, bit, invert) in [
            ("auto_page", &settings.auto_page, 1u32, true),
            ("invert", &settings.invert, 4, false),
            ("auto_invert", &settings.auto_invert, 8, true),
            ("standby", &settings.standby, 2, false),
        ] {
            let text = match key {
                "auto_invert" => Some(settings.auto_invert_text.as_str()),
                "standby" => Some(settings.standby_text.as_str()),
                _ => None,
            };
            settings_card =
                settings_card.child(self.flag_row(key, toggle, text, id, bit, invert, p, cx));
            if key == "auto_page" {
                settings_card = settings_card.child(
                    field_row(p)
                        .child(name_column(&settings.interval_label, None, p))
                        .child(field_input(
                            &interval,
                            "s",
                            settings.interval_writable,
                            false,
                            104.,
                            p,
                            window,
                            cx,
                        )),
                );
            }
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
            let key_view = view.clone();
            root = root.track_focus(&focus).on_key_down(move |event, _, cx| {
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
                    this.leakshield.display.step(dx, dy);
                    cx.notify();
                });
            });
        }
        if let Some(templates) = &model.gallery {
            let mut grid = v_flex().gap(px(6.));
            for chunk in templates.chunks(3) {
                let mut row = h_flex().gap(px(6.));
                for (template, name) in chunk {
                    let (device, template) = (id.to_owned(), template.clone());
                    row = row.child(
                        small_button(
                            SharedString::from(format!("ls-gallery-{template}")),
                            name.clone(),
                        )
                        .on_click(act(&view, move |this, _, cx| {
                            let mut ui = std::mem::take(&mut this.leakshield.display);
                            let command = this
                                .with_input(cx, |input| ui.set_template(input, &device, &template));
                            this.leakshield.display = ui;
                            if let Some(command) = command {
                                this.send(command);
                            }
                        })),
                    );
                }
                grid = grid.child(row);
            }
            let close = view.clone();
            root = root.child(modal(
                model.gallery_title.clone(),
                String::new(),
                vec![grid.into_any_element()],
                vec![
                    small_button(
                        "ls-gallery-close",
                        self.model.read(cx).locale.tr("leakshield.cancel"),
                    )
                    .on_click(act(&close, |this, _, _| {
                        this.leakshield.display.gallery = false
                    }))
                    .into_any_element(),
                ],
                p,
            ));
        }
        root.into_any_element()
    }

    /// A switch of `page_options` of the selected page.
    #[allow(clippy::too_many_arguments)]
    fn toggle_row(
        &mut self,
        id_text: &str,
        toggle: &Toggle,
        device: &str,
        bit: u32,
        invert: bool,
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
                        let on = if invert { !*on } else { *on };
                        let device = device.clone();
                        view.update(cx, |this, cx| {
                            let command = this.with_input(cx, |input| {
                                this.leakshield.display.set_option(input, &device, bit, on)
                            });
                            if let Some(command) = command {
                                this.send(command);
                            }
                        });
                    }),
            )
            .into_any_element()
    }

    /// A switch of `display_flags`. A flag that means "off" has `invert`.
    #[allow(clippy::too_many_arguments)]
    fn flag_row(
        &mut self,
        key: &str,
        toggle: &Toggle,
        text: Option<&str>,
        device: &str,
        bit: u32,
        invert: bool,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let device = device.to_owned();
        field_row(p)
            .child(name_column(&toggle.label, text, p))
            .child(
                Switch::new(SharedString::from(format!("ls-flag-{key}")))
                    .large()
                    .checked(toggle.on)
                    .disabled(!toggle.writable)
                    .on_click(move |on, _, cx| {
                        let on = if invert { !*on } else { *on };
                        let device = device.clone();
                        view.update(cx, |this, cx| {
                            let command =
                                this.with_input(cx, |input| set_flag(input, &device, bit, on));
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

/// The preview of a display page: 128x64 px times `scale`, white on black.
fn preview(page: &PageCard, scale: f32) -> gpui::Div {
    let mut screen = v_flex()
        .w(px(128. * scale))
        .h(px(64. * scale))
        .flex_none()
        .p(px(4. * scale))
        .rounded(px(6.))
        .bg(gpui::black())
        .text_color(gpui::white())
        .text_size(px(9. * scale))
        .line_height(px(12. * scale))
        .overflow_hidden()
        .when(page.off.is_some(), |s| s.opacity(0.4));
    if page.empty {
        screen = screen.child(page.template_name.clone());
    }
    for (name, value) in &page.lines {
        screen = screen.child(
            h_flex()
                .justify_between()
                .gap(px(4. * scale))
                .child(div().min_w_0().truncate().child(name.clone()))
                .child(div().flex_none().child(value.clone())),
        );
    }
    screen
}
