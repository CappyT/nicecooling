//! The window of the Lighting tab: the LED cells, the controller bars and the parameters.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, hsla, px};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{Disableable as _, h_flex, v_flex};

use crate::icons::NcIcon;
use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::controls::{Pick, act};
use crate::screens::leakshield::TabState;
use crate::screens::leakshield::lighting::{Hsv, LightingModel, ToolbarButton};
use crate::screens::ui::Tip as _;
use crate::screens::ui::{
    caption, card, card_header, card_row, field_row, modal, notice_row, page, write_line,
};
use crate::tokens::Palette;
use gpui_component::slider::Slider;

/// The color of an LED cell: hue 0 to 360, saturation and value 0 to 255.
fn color(value: Hsv) -> gpui::Hsla {
    hsla(
        (value.0 / 360.0) as f32,
        (value.1 / 255.0) as f32,
        ((value.2 / 255.0) * 0.5) as f32,
        1.0,
    )
}

fn toolbar(
    id: &str,
    button: &ToolbarButton,
    handler: impl Fn(&mut ScreenView, &mut Context<ScreenView>) + 'static,
    view: &gpui::Entity<ScreenView>,
) -> Button {
    let mut b = ScreenView::small_button(
        SharedString::from(id.to_owned()),
        button.label.clone(),
        None,
    )
    .disabled(!button.enabled)
    .on_click(act(view, move |this, _, cx| handler(this, cx)));
    if let Some(tooltip) = &button.tooltip {
        b = b.tip(tooltip.clone());
    }
    b
}

impl ScreenView {
    pub(crate) fn render_ls_lighting(
        &mut self,
        id: &str,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = self.with_input(cx, |input| {
            LightingModel::build(input, &self.leakshield.lighting, id)
        });
        let view = cx.entity();
        let mut content = page();
        if model.state == TabState::Loading {
            return content
                .child(card(p).p(px(18.)).gap(px(12.)).child(skeleton(p, 420.)))
                .into_any_element();
        }
        if let Some(reason) = &model.reason {
            content = content.child(card(p).child(notice_row(reason.clone(), true, p)));
        }

        let device = id.to_owned();
        let tools = h_flex()
            .gap(px(6.))
            .child({
                let device = device.clone();
                toolbar(
                    "ls-led-add",
                    &model.add,
                    move |this, cx| {
                        let mut ui = std::mem::take(&mut this.leakshield.lighting);
                        let command = this.with_input(cx, |input| ui.add(input, &device));
                        this.leakshield.lighting = ui;
                        if let Some(command) = command {
                            this.send(command);
                        }
                    },
                    &view,
                )
            })
            .child({
                let device = device.clone();
                toolbar(
                    "ls-led-duplicate",
                    &model.duplicate,
                    move |this, cx| {
                        let mut ui = std::mem::take(&mut this.leakshield.lighting);
                        let command = this.with_input(cx, |input| ui.duplicate(input, &device));
                        this.leakshield.lighting = ui;
                        if let Some(command) = command {
                            this.send(command);
                        }
                    },
                    &view,
                )
            })
            .child({
                let device = device.clone();
                toolbar(
                    "ls-led-delete",
                    &model.delete,
                    move |this, cx| {
                        let mut ui = std::mem::take(&mut this.leakshield.lighting);
                        let command = this.with_input(cx, |input| ui.delete(input, &device));
                        this.leakshield.lighting = ui;
                        if let Some(command) = command {
                            this.send(command);
                        }
                    },
                    &view,
                )
            });

        // The LED cells and the controller bars.
        let mut cells = h_flex().gap(px(2.)).pl(px(120.));
        for cell in &model.cells {
            cells = cells.child(
                div()
                    .w(px(28.))
                    .h(px(28.))
                    .rounded(px(5.))
                    .border_1()
                    .border_color(p.border)
                    .bg(if cell.lit { color(cell.color) } else { p.track }),
            );
        }
        let mut timeline = v_flex().gap(px(6.)).px(px(18.)).pb(px(12.)).child(cells);
        for bar in &model.bars {
            let index = bar.index;
            let view = view.clone();
            let mut swatch = h_flex()
                .id(SharedString::from(format!("ls-bar-{index}")))
                .absolute()
                .left(px(120. + bar.first as f32 * 30.))
                .w(px(bar.count as f32 * 30. - 2.))
                .h(px(28.))
                .rounded(px(6.))
                .cursor_pointer()
                .border_2()
                .border_color(if bar.selected { p.accent_bg } else { p.border })
                .overflow_hidden()
                .when(bar.covered_by.is_some(), |b| b.opacity(0.4))
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.leakshield.lighting.toggle(index);
                        cx.notify();
                    });
                });
            for value in &bar.colors {
                swatch = swatch.child(div().flex_1().bg(color(*value)));
            }
            timeline = timeline.child(
                h_flex()
                    .relative()
                    .h(px(28.))
                    .child(
                        div()
                            .w(px(116.))
                            .truncate()
                            .font_weight(FontWeight::BOLD)
                            .child(bar.title.clone()),
                    )
                    .child(swatch)
                    .when_some(bar.covered_by.clone(), |row, text| {
                        row.child(div().absolute().right_0().child(caption(text, p)))
                    }),
            );
        }
        if let Some((text, button)) = &model.empty {
            let (device, view) = (id.to_owned(), view.clone());
            timeline = timeline.child(
                h_flex()
                    .gap(px(10.))
                    .items_center()
                    .child(text.clone())
                    .child(
                        Self::small_button("ls-led-add-first", button.clone(), None).on_click(act(
                            &view,
                            move |this, _, cx| {
                                let mut ui = std::mem::take(&mut this.leakshield.lighting);
                                let command = this.with_input(cx, |input| ui.add(input, &device));
                                this.leakshield.lighting = ui;
                                if let Some(command) = command {
                                    this.send(command);
                                }
                            },
                        )),
                    ),
            );
        }
        content = content.child(
            card(p)
                .child(card_header(model.title.clone(), p).child(tools))
                .child(timeline)
                .child(
                    v_flex()
                        .px(px(18.))
                        .pb(px(12.))
                        .gap(px(3.))
                        .child(caption(model.priority_hint.clone(), p))
                        .child(caption(model.external.clone(), p)),
                ),
        );
        if let Some((text, button)) = &model.undo {
            let device = id.to_owned();
            content = content.child(card(p).child(notice_row(text.clone(), false, p).child(
                Self::small_button("ls-led-undo", button.clone(), None).on_click(act(
                    &view,
                    move |this, _, _| {
                        if let Some(command) = this.leakshield.lighting.undo(&device) {
                            this.send(command);
                        }
                    },
                )),
            )));
        }

        if let Some(panel) = &model.panel {
            content = content.child(self.lighting_panel(id, panel, p, window, cx));
        }
        if let Some(line) = &model.write_line {
            content = content.child(card(p).child(card_row(p).child(write_line(line, p))));
        }

        let mut root = div().id("ls-lighting").relative().w_full().child(content);
        if let Some(picker) = &model.picker {
            let mut list = v_flex().gap(px(4.));
            let mut strip = h_flex().gap(px(2.));
            for cell in &picker.preview {
                strip = strip.child(div().w(px(24.)).h(px(24.)).rounded(px(4.)).bg(color(*cell)));
            }
            list = list.child(strip);
            for (effect, name, text, selected) in &picker.effects {
                let effect = effect.clone();
                let view = view.clone();
                let mut row = h_flex()
                    .id(SharedString::from(format!("ls-effect-{effect}")))
                    .gap(px(8.))
                    .px(px(8.))
                    .h(px(30.))
                    .items_center()
                    .rounded(px(6.))
                    .cursor_pointer()
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.leakshield.lighting.effect_picker = Some(effect.clone());
                            cx.notify();
                        });
                    })
                    .child(
                        div()
                            .w(px(140.))
                            .font_weight(FontWeight::BOLD)
                            .child(name.clone()),
                    )
                    .child(
                        div()
                            .text_color(p.fg_dim)
                            .text_size(px(12.))
                            .child(text.clone()),
                    );
                if *selected {
                    row = row.bg(p.accent_tint);
                }
                list = list.child(row);
            }
            let (cancel, confirm, device, chosen) = (
                view.clone(),
                view.clone(),
                id.to_owned(),
                picker.current.clone(),
            );
            root = root.child(modal(
                picker.title.clone(),
                String::new(),
                vec![list.into_any_element()],
                vec![
                    Self::small_button("ls-effect-cancel", picker.cancel.clone(), None)
                        .on_click(act(&cancel, |this, _, _| {
                            this.leakshield.lighting.effect_picker = None
                        }))
                        .into_any_element(),
                    Self::small_button("ls-effect-use", picker.confirm.clone(), None)
                        .primary()
                        .on_click(act(&confirm, move |this, _, cx| {
                            let mut ui = std::mem::take(&mut this.leakshield.lighting);
                            let command =
                                this.with_input(cx, |input| ui.set_effect(input, &device, &chosen));
                            this.leakshield.lighting = ui;
                            if let Some(command) = command {
                                this.send(command);
                            }
                        }))
                        .into_any_element(),
                ],
                p,
            ));
        }
        root.into_any_element()
    }

    fn lighting_panel(
        &mut self,
        id: &str,
        panel: &crate::screens::leakshield::lighting::Panel,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let locale = self.model.read(cx).locale;
        let enabled = panel.writable;
        let name = |text: &str| {
            div()
                .flex_1()
                .min_w_0()
                .font_weight(FontWeight::BOLD)
                .child(text.to_owned())
        };
        let mut body = card(p).child(card_header(panel.title.clone(), p));
        body = body.child(
            field_row(p)
                .child(name(&panel.effect_label))
                .child(div().flex_none().child(panel.effect.clone()))
                .child(
                    Self::small_button("ls-effect-change", panel.change.clone(), None)
                        .disabled(!enabled)
                        .on_click(act(&view, |this, _, _| {
                            this.leakshield.lighting.effect_picker = Some("fixed-color".to_owned())
                        })),
                ),
        );
        let first = self.input(
            InputKey::Ls("lighting-first".into()),
            &(panel.first as i64).to_string(),
            "",
            window,
            cx,
        );
        let count = self.input(
            InputKey::Ls("lighting-count".into()),
            &(panel.count as i64).to_string(),
            "",
            window,
            cx,
        );
        let mut buttons = h_flex().gap(px(4.));
        for (name, glyph, dx, dl) in [
            ("ls-led-left", NcIcon::Left, -1isize, 0isize),
            ("ls-led-right", NcIcon::Right, 1, 0),
            ("ls-led-shorter", NcIcon::Minus, 0, -1),
            ("ls-led-longer", NcIcon::Plus, 0, 1),
        ] {
            let device = id.to_owned();
            buttons = buttons.child(
                Self::icon_button(name, glyph)
                    .debug_selector(move || name.to_owned())
                    .disabled(!enabled)
                    .on_click(act(&view, move |this, _, cx| {
                        let mut ui = std::mem::take(&mut this.leakshield.lighting);
                        let command = this.with_input(cx, |input| ui.shift(input, &device, dx, dl));
                        this.leakshield.lighting = ui;
                        if let Some(command) = command {
                            this.send(command);
                        }
                    })),
            );
        }
        for (name, glyph, delta) in [
            ("ls-led-up", NcIcon::Up, -1isize),
            ("ls-led-down", NcIcon::Down, 1),
        ] {
            let device = id.to_owned();
            buttons = buttons.child(
                Self::icon_button(name, glyph)
                    .debug_selector(move || name.to_owned())
                    .disabled(!enabled)
                    .on_click(act(&view, move |this, _, cx| {
                        let mut ui = std::mem::take(&mut this.leakshield.lighting);
                        let command =
                            this.with_input(cx, |input| ui.reorder(input, &device, delta));
                        this.leakshield.lighting = ui;
                        if let Some(command) = command {
                            this.send(command);
                        }
                    })),
            );
        }
        body = body.child(
            field_row(p)
                .child(name(&panel.range_label))
                .child(buttons)
                .child(
                    div()
                        .w(px(70.))
                        .flex_none()
                        .child(Self::mode_input(&first, "", enabled, false, false, p)),
                )
                .child(
                    div()
                        .w(px(70.))
                        .flex_none()
                        .child(Self::mode_input(&count, "", enabled, false, false, p)),
                ),
        );
        for (index, (label, value)) in [
            (&panel.speed_label, panel.speed),
            (&panel.brightness_label, panel.brightness),
        ]
        .iter()
        .enumerate()
        {
            let slider = self.range_slider(
                &format!("ls:light-param:{index}"),
                (0.0, 100.0),
                *value as f32,
                window,
                cx,
            );
            body = body.child(
                field_row(p)
                    .child(name(label))
                    .child(
                        div()
                            .w(px(260.))
                            .flex_none()
                            .child(Slider::new(&slider).disabled(!enabled)),
                    )
                    .child(
                        div()
                            .w(px(40.))
                            .flex_none()
                            .text_right()
                            .font_weight(FontWeight::BOLD)
                            .child(format!("{}", *value as i64)),
                    ),
            );
        }
        let mut colors = h_flex().gap(px(6.));
        for (index, value) in panel.colors.iter().enumerate() {
            let hue = self.input(
                InputKey::Ls(format!("lighting-hue:{index}")),
                &(value.0 as i64).to_string(),
                "",
                window,
                cx,
            );
            colors = colors.child(
                h_flex()
                    .gap(px(4.))
                    .child(
                        div()
                            .size(px(28.))
                            .flex_none()
                            .rounded(px(7.))
                            .border_1()
                            .border_color(p.border)
                            .bg(color(*value)),
                    )
                    .child(
                        div()
                            .w(px(80.))
                            .flex_none()
                            .child(Self::mode_input(&hue, "°", enabled, false, false, p)),
                    ),
            );
        }
        body = body.child(field_row(p).child(name(&panel.color_label)).child(colors));
        // The sensor that drives the effect.
        let pick: Pick = {
            let device = id.to_owned();
            Rc::new(move |this, choice, _, cx| {
                let source = choice.parse::<i32>().unwrap_or(-1);
                let mut ui = std::mem::take(&mut this.leakshield.lighting);
                let command = this.with_input(cx, |input| ui.set_source(input, &device, source));
                this.leakshield.lighting = ui;
                if let Some(command) = command {
                    this.send(command);
                }
            })
        };
        let mut options: Vec<(String, String, bool)> = vec![(
            "-1".to_owned(),
            locale.tr("leakshield.lighting.no_driver"),
            !panel.driven,
        )];
        options.extend(
            panel
                .sources
                .iter()
                .map(|(index, name, selected)| (index.to_string(), name.clone(), *selected)),
        );
        let shown = options
            .iter()
            .find(|(_, _, selected)| *selected)
            .map_or_else(String::new, |(_, text, _)| text.clone());
        let select = self.select_field(
            "ls-led-source",
            &shown,
            options,
            enabled,
            false,
            p.track,
            pick,
            p,
            cx,
        );
        body = body.child(
            field_row(p)
                .child(name(&panel.driven_label))
                .child(div().w(px(240.)).flex_none().child(select)),
        );
        body.into_any_element()
    }
}
