//! The window of the Loops screen, and the Calibration table editor.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, Bounds, Context, FontWeight, SharedString, Window, canvas, div, px};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::slider::Slider;
use gpui_component::{Disableable as _, Sizable as _, h_flex, v_flex};

use crate::icons::{NcIcon, icon, text_icon};
use crate::model::Severity;
use crate::screen_view::{InputKey, ScreenView, skeleton, with_screen_input};
use crate::screens::format::plain;
use crate::screens::loops::{Choice, LoopDetail, LoopRow, LoopsModel, LoopsState, PartRow};
use crate::screens::sensors_view::PickRow;
use crate::screens::ui::{
    TagKind, caption, card, card_header, card_row, page, severity_icon, tag, write_line,
};
use crate::tokens::Palette;
use crate::widgets::calibration_editor::CalibrationEditorModel;
use crate::widgets::loop_diagram;
use crate::widgets::paint::{at, disc, polyline};

type Pick = Rc<dyn Fn(&mut ScreenView, String, &mut Window, &mut Context<ScreenView>)>;

impl ScreenView {
    pub(crate) fn render_loops(
        &mut self,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state(cx);
        if let Some(info) = self.loops.current(&state) {
            let id = info.id.clone();
            self.loops.calibration.sync(&state, &id);
        }
        let model = self.with_input(cx, |input| LoopsModel::build(input, &self.loops));
        let locale = self.model.read(cx).locale;
        let view = cx.entity();

        let mut content = page();
        match model.state {
            LoopsState::Loading => {
                return content
                    .child(
                        card(p)
                            .p(px(18.))
                            .gap(px(12.))
                            .child(skeleton(p, 200.))
                            .child(skeleton(p, 400.)),
                    )
                    .into_any_element();
            }
            LoopsState::Empty => {
                let view = view.clone();
                return content
                    .child(
                        card(p)
                            .p(px(18.))
                            .gap(px(12.))
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .child(model.empty_title.clone()),
                            )
                            .child(
                                h_flex().child(
                                    Button::new("loops-setup")
                                        .label(model.empty_button.clone())
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| {
                                                let command = this.with_input(cx, |input| {
                                                    this.loops.new_loop(input)
                                                });
                                                this.send(command);
                                                cx.notify();
                                            });
                                        }),
                                ),
                            ),
                    )
                    .into_any_element();
            }
            LoopsState::Ready => {}
        }

        let enabled = self.with_input(cx, |input| input.writes().enabled);
        let new_loop = {
            let view = view.clone();
            Button::new("loop-new")
                .small()
                .icon(text_icon(NcIcon::Plus))
                .label(locale.tr("loops.new"))
                .disabled(!enabled)
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        let command = this.with_input(cx, |input| this.loops.new_loop(input));
                        this.send(command);
                        cx.notify();
                    })
                })
        };
        let mut list = card(p)
            .w(px(292.))
            .flex_none()
            .child(card_header(locale.tr("nav.loops"), p).child(new_loop));
        for (index, row) in model.rows.iter().enumerate() {
            list = list.child(self.loop_row(index, model.rows.len(), row, p, cx));
        }
        content = content.child(
            h_flex()
                .w_full()
                .gap(px(12.))
                .items_start()
                .child(list)
                .children(
                    model
                        .detail
                        .as_ref()
                        .map(|detail| self.loop_detail(detail, p, window, cx)),
                ),
        );
        div().relative().w_full().child(content).into_any_element()
    }

    fn loop_row(
        &mut self,
        index: usize,
        count: usize,
        row: &LoopRow,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let id = row.id.clone();
        let (hover, last) = (p.hover, index + 1 == count);
        card_row(p)
            .id(("loop-row", index))
            .cursor_pointer()
            .when(last, |line| line.rounded_b(px(12.)))
            .map(|line| {
                if row.selected {
                    line.bg(p.accent_tint)
                } else {
                    line.hover(move |line| line.bg(hover))
                }
            })
            .on_click(move |_, _, cx| {
                let id = id.clone();
                view.update(cx, |this, cx| {
                    this.loops.select(&id);
                    this.open_picker = None;
                    cx.notify();
                })
            })
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .font_weight(FontWeight::BOLD)
                            .child(row.name.clone()),
                    )
                    // On the tint of the selected row a caption has the text color.
                    .child(
                        caption(row.caption.clone(), p)
                            .when(row.selected, |caption| caption.text_color(p.fg)),
                    ),
            )
            .into_any_element()
    }

    fn loop_detail(
        &mut self,
        detail: &LoopDetail,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.model.read(cx).locale;
        let view = cx.entity();
        let enabled = detail.editable;
        let tooltip = detail.disabled_reason.clone().unwrap_or_default();

        // The first card: the name with the rename button, the menu, and the diagram.
        let menu_key = "loop-menu";
        let menu_open = self.open_picker.as_deref() == Some(menu_key);
        let header = card_header(detail.name.clone(), p)
            .child({
                let view = view.clone();
                Button::new("loop-rename")
                    .small()
                    .ghost()
                    .icon(text_icon(NcIcon::Pencil))
                    .disabled(!enabled)
                    .tooltip(if enabled {
                        locale.tr("loops.rename")
                    } else {
                        tooltip.clone()
                    })
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            let state = this.state(cx);
                            this.loops.open_rename(&state);
                            cx.notify();
                        })
                    })
            })
            .child({
                let view = view.clone();
                Button::new("loop-menu")
                    .small()
                    .ghost()
                    .icon(text_icon(NcIcon::Menu))
                    .tooltip(locale.tr("loops.menu"))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.open_picker = if this.open_picker.as_deref() == Some(menu_key) {
                                None
                            } else {
                                Some(menu_key.to_owned())
                            };
                            cx.notify();
                        })
                    })
            });
        let mut preview = card(p).child(header);
        if menu_open {
            let view = view.clone();
            preview = preview.child(
                h_flex().justify_end().px(px(12.)).pb(px(6.)).child(
                    Button::new("loop-delete")
                        .small()
                        .ghost()
                        .icon(text_icon(NcIcon::Trash))
                        .label(locale.tr("loops.delete_loop"))
                        .disabled(!enabled)
                        .tooltip(tooltip.clone())
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                this.open_picker = None;
                                this.loops.delete_dialog = true;
                                cx.notify();
                            })
                        }),
                ),
            );
        }
        if let Some((_, name)) = self.loops.rename.clone() {
            preview = preview.child(self.rename_popover(
                InputKey::LoopRename,
                &name,
                locale.tr("sensors.rename_placeholder"),
                p,
                window,
                cx,
            ));
        }
        preview = preview.child(loop_diagram::element(
            &detail.diagram,
            p,
            self.navigator(),
            cx,
        ));

        let mut column = v_flex().flex_1().min_w_0().gap(px(12.));
        if let Some(text) = &detail.alarm {
            column = column.child(
                h_flex()
                    .px(px(18.))
                    .py(px(10.))
                    .gap(px(12.))
                    .rounded(px(12.))
                    .bg(p.alarm_bg)
                    .text_color(p.alarm_fg)
                    .font_weight(FontWeight::BOLD)
                    .child(icon(NcIcon::Alarm, p.alarm_fg).flex_none())
                    .child(div().flex_1().min_w_0().child(text.clone())),
            );
        }
        column = column.child(preview);

        // Parts.
        let mut parts = card(p).child(card_header(detail.parts_title.clone(), p));
        parts =
            parts.child(self.part_row("pumps", &detail.pumps, enabled, &tooltip, p, window, cx));
        parts = parts.child(self.part_row(
            "flow",
            &detail.flow_sensor,
            enabled,
            &tooltip,
            p,
            window,
            cx,
        ));
        parts =
            parts.child(self.part_row("water", &detail.water, enabled, &tooltip, p, window, cx));
        parts = parts.child(self.part_row(
            "leakshield",
            &detail.leakshield,
            enabled,
            &tooltip,
            p,
            window,
            cx,
        ));
        column = column.child(parts);

        // Pump floor.
        let slider = self.slider(
            &format!("floor:{}", detail.id),
            detail.floor.value as f32,
            window,
            cx,
        );
        let mut floor = card(p)
            .child(card_header(detail.floor.title.clone(), p))
            .child(
                card_row(p)
                    .child(
                        div()
                            .flex_1()
                            .child(Slider::new(&slider).disabled(!enabled)),
                    )
                    .child(
                        div()
                            .w(px(70.))
                            .text_right()
                            .font_weight(FontWeight::BOLD)
                            .child(crate::screens::loops::floor_text(
                                locale,
                                detail.floor.value,
                            )),
                    ),
            )
            .child(card_row(p).child(caption(detail.floor.text.clone(), p)));
        if let Some(text) = &detail.floor.warning {
            floor = floor.child(
                card_row(p)
                    .child(severity_icon(Severity::Warning, p))
                    .child(text.clone()),
            );
        }
        if let Some(line) = &detail.floor.write_line {
            floor = floor.child(card_row(p).child(write_line(line, p)));
        }
        column = column.child(floor);

        if let Some(editor) = &detail.calibration {
            column = column.child(self.calibration_card(editor, p, window, cx));
        }
        if let Some(feed) = &detail.feed {
            let stale = if feed.stale { p.warning } else { p.fg_dim };
            column = column.child(
                card(p)
                    .child(card_header(feed.title.clone(), p))
                    .child(
                        card_row(p)
                            .child(div().flex_1().child(feed.feed_pump.clone()))
                            .child(div().flex_1().child(feed.flow.clone()))
                            .child(div().text_color(stale).child(feed.sent.clone())),
                    )
                    .child(card_row(p).child(caption(feed.caption.clone(), p))),
            );
        }
        if let Some(dialog) = &detail.delete_dialog {
            let (cancel, confirm) = (view.clone(), view.clone());
            let overlay = div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .flex()
                .justify_center()
                .pt(px(120.))
                .bg(gpui::black().opacity(0.4))
                .child(
                    v_flex()
                        .w(px(440.))
                        .h_auto()
                        .p(px(18.))
                        .gap(px(12.))
                        .rounded(px(15.))
                        .bg(p.card)
                        .border_1()
                        .border_color(p.border)
                        .child(
                            div()
                                .text_size(px(18.))
                                .font_weight(FontWeight::BOLD)
                                .child(dialog.title.clone()),
                        )
                        .child(dialog.text.clone())
                        .child(
                            h_flex()
                                .justify_end()
                                .gap(px(8.))
                                .child(
                                    Button::new("delete-cancel")
                                        .label(dialog.cancel.clone())
                                        .on_click(move |_, _, cx| {
                                            cancel.update(cx, |this, cx| {
                                                this.loops.delete_dialog = false;
                                                cx.notify();
                                            })
                                        }),
                                )
                                .child(
                                    Button::new("delete-confirm")
                                        .danger()
                                        .label(dialog.confirm.clone())
                                        .on_click(move |_, _, cx| {
                                            confirm.update(cx, |this, cx| {
                                                let state = this.state(cx);
                                                if let Some(command) =
                                                    this.loops.confirm_delete(&state)
                                                {
                                                    this.send(command);
                                                }
                                                cx.notify();
                                            })
                                        }),
                                ),
                        ),
                );
            column = column.child(overlay);
        }
        column.into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    fn part_row(
        &mut self,
        name: &str,
        row: &PartRow,
        enabled: bool,
        tooltip: &str,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let key = format!("part-{name}");
        let open = self.open_picker.as_deref() == Some(key.as_str());
        let line = card_row(p)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .font_weight(FontWeight::BOLD)
                    .child(row.label.clone()),
            )
            .child({
                let view = view.clone();
                let key = key.clone();
                Button::new(SharedString::from(format!("{key}-button")))
                    .small()
                    .label(row.shown.clone())
                    .child(text_icon(NcIcon::Down))
                    .disabled(!enabled)
                    .tooltip(tooltip.to_owned())
                    .on_click(move |_, _, cx| {
                        let key = key.clone();
                        view.update(cx, |this, cx| {
                            this.open_picker = if this.open_picker.as_deref() == Some(key.as_str())
                            {
                                None
                            } else {
                                Some(key)
                            };
                            cx.notify();
                        })
                    })
            });
        let mut column = v_flex().w_full().child(line);
        if let Some(line_text) = &row.write_line {
            column = column.child(card_row(p).child(write_line(line_text, p)));
        }
        if let Some(text) = &row.warning {
            column = column.child(
                card_row(p)
                    .child(severity_icon(Severity::Warning, p))
                    .child(text.clone()),
            );
        }
        if let Some(text) = &row.note {
            column = column.child(card_row(p).child(caption(text.clone(), p)));
        }
        if open {
            let rows: Vec<PickRow> = row.choices.iter().map(pick_row).collect();
            let multiple = row.multiple;
            let name = name.to_owned();
            let pick: Pick = Rc::new(move |this, id, _, cx| {
                let state = this.state(cx);
                let none = id.is_empty();
                let command = match name.as_str() {
                    "pumps" => this.loops.toggle_pump_channel(&state, &id),
                    "flow" => this
                        .loops
                        .set_flow_sensor(&state, (!none).then_some(id.as_str())),
                    "water" => this
                        .loops
                        .set_water_temperature(&state, (!none).then_some(id.as_str())),
                    _ => this
                        .loops
                        .set_leakshield(&state, (!none).then_some(id.as_str())),
                };
                if let Some(command) = command {
                    this.send(command);
                }
                if !multiple {
                    this.open_picker = None;
                }
                cx.notify();
            });
            column = column.child(
                div()
                    .px(px(18.))
                    .pb(px(8.))
                    .child(self.picker_list(&key, rows, pick, p, window, cx)),
            );
        }
        column.into_any_element()
    }

    /// The Calibration table editor: a table and a plot side by side.
    pub(crate) fn calibration_card(
        &mut self,
        model: &CalibrationEditorModel,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.model.read(cx).locale;
        let view = cx.entity();
        let enabled = model.editable;
        let loop_id = model.loop_id.clone();

        let mut header =
            card_header(model.title.clone(), p).child(caption(model.table_name.clone(), p));
        header = header
            .child({
                let view = view.clone();
                Button::new(SharedString::from(format!("cal-preset-{loop_id}")))
                    .small()
                    .ghost()
                    .label(model.preset_label.clone())
                    .disabled(!enabled || model.presets.is_empty())
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.preset_menu_open = !this.preset_menu_open;
                            cx.notify();
                        })
                    })
            })
            .child({
                let (view, loop_id) = (view.clone(), loop_id.clone());
                Button::new(SharedString::from(format!("cal-add-{loop_id}")))
                    .small()
                    .label(model.add_label.clone())
                    .disabled(!enabled)
                    .on_click(move |_, _, cx| {
                        let loop_id = loop_id.clone();
                        view.update(cx, |this, cx| {
                            let model = this.model.clone();
                            let command = with_screen_input(&model, cx, |input| {
                                this.loops.calibration.add_row(input, &loop_id)
                            });
                            if let Some(command) = command {
                                this.send(command);
                            }
                            cx.notify();
                        })
                    })
            });

        let mut table = v_flex().w(px(300.)).gap(px(4.)).child(
            h_flex()
                .gap(px(8.))
                .text_size(px(12.))
                .font_weight(FontWeight::BOLD)
                .text_color(p.fg_dim)
                .child(div().w(px(110.)).child(model.x_header.clone()))
                .child(div().w(px(90.)).child(model.y_header.clone())),
        );
        for (index, (rpm, flow)) in model.rows.iter().enumerate() {
            let x = self.input(
                InputKey::CalibrationCell(loop_id.clone(), index, 0),
                &plain(locale, *rpm),
                "",
                window,
                cx,
            );
            let y = self.input(
                InputKey::CalibrationCell(loop_id.clone(), index, 1),
                &plain(locale, *flow),
                "",
                window,
                cx,
            );
            let (view, loop_id) = (view.clone(), loop_id.clone());
            table = table.child(
                h_flex()
                    .id(SharedString::from(format!("cal-row-{index}")))
                    .gap(px(8.))
                    .items_center()
                    .rounded(px(6.))
                    .when(model.plot.selected == Some(index), |row| {
                        row.bg(p.accent_tint)
                    })
                    .child(div().w(px(110.)).child(Input::new(&x).disabled(!enabled)))
                    .child(div().w(px(90.)).child(Input::new(&y).disabled(!enabled)))
                    .child(
                        Button::new(SharedString::from(format!("cal-remove-{index}")))
                            .ghost()
                            .small()
                            .icon(text_icon(NcIcon::X))
                            .disabled(!enabled)
                            .on_click(move |_, _, cx| {
                                let loop_id = loop_id.clone();
                                view.update(cx, |this, cx| {
                                    let model = this.model.clone();
                                    let command = with_screen_input(&model, cx, |input| {
                                        this.loops.calibration.remove_row(input, &loop_id, index)
                                    });
                                    if let Some(command) = command {
                                        this.send(command);
                                    }
                                    cx.notify();
                                })
                            }),
                    ),
            );
        }

        let mut body = h_flex()
            .w_full()
            .p(px(18.))
            .gap(px(18.))
            .items_start()
            .child(table)
            .child(calibration_plot(model, p));
        if self.preset_menu_open {
            let mut menu = v_flex().px(px(18.)).gap(px(2.));
            for preset in &model.presets {
                let (view, id) = (view.clone(), preset.id.clone());
                menu = menu.child(
                    Button::new(SharedString::from(format!("cal-preset-{}", preset.id)))
                        .ghost()
                        .small()
                        .label(preset.name.clone())
                        .on_click(move |_, _, cx| {
                            let id = id.clone();
                            view.update(cx, |this, cx| {
                                this.loops.calibration.request_preset(&id);
                                this.preset_menu_open = false;
                                cx.notify();
                            })
                        }),
                );
            }
            body = h_flex()
                .w_full()
                .child(v_flex().w_full().child(menu).child(body));
        }

        let mut card_view = card(p).child(header).child(body);
        if let Some(confirm) = &model.confirm {
            let (cancel, accept, loop_id) = (view.clone(), view.clone(), loop_id.clone());
            card_view = card_view.child(
                card_row(p)
                    .child(
                        div()
                            .flex_1()
                            .child(format!("{} {}", confirm.title, confirm.text)),
                    )
                    .child(
                        Button::new("cal-confirm-cancel")
                            .small()
                            .label(confirm.cancel.clone())
                            .on_click(move |_, _, cx| {
                                cancel.update(cx, |this, cx| {
                                    this.loops.calibration.cancel_preset();
                                    cx.notify();
                                })
                            }),
                    )
                    .child(
                        Button::new("cal-confirm-ok")
                            .small()
                            .primary()
                            .label(confirm.confirm.clone())
                            .on_click(move |_, _, cx| {
                                let loop_id = loop_id.clone();
                                accept.update(cx, |this, cx| {
                                    let model = this.model.clone();
                                    let command = with_screen_input(&model, cx, |input| {
                                        this.loops.calibration.confirm_preset(input, &loop_id)
                                    });
                                    if let Some(command) = command {
                                        this.send(command);
                                    }
                                    cx.notify();
                                })
                            }),
                    ),
            );
        }
        if let Some(text) = &model.out_of_range {
            card_view =
                card_view.child(card_row(p).child(div().text_color(p.warning).child(text.clone())));
        }
        if let Some(text) = &model.error {
            card_view = card_view.child(
                card_row(p)
                    .child(severity_icon(Severity::Warning, p))
                    .child(text.clone()),
            );
        }
        if let Some(line) = &model.write_line {
            card_view = card_view.child(card_row(p).child(write_line(line, p)));
        }
        let _ = tag("", TagKind::Neutral, p);
        card_view.into_any_element()
    }
}

fn pick_row(choice: &Choice) -> PickRow {
    PickRow {
        id: choice.id.clone(),
        name: choice.name.clone(),
        caption: String::new(),
        value: choice.value.clone().unwrap_or_default(),
        selected: choice.selected,
    }
}

/// The plot of the Calibration table: points and segments, the dashed ends, the live marker.
fn calibration_plot(model: &CalibrationEditorModel, p: &Palette) -> AnyElement {
    let plot = model.plot.clone();
    let (line, dash, ok, warn, fg, border) = (p.accent, p.fg_dim, p.fg, p.warning, p.fg, p.border);
    let marker_label = plot
        .marker
        .as_ref()
        .map(|m| (m.label.clone(), m.out_of_range));
    let extension_label = plot.extension_label.clone();
    let has_extension = plot.extension.is_some();
    let chart = canvas(
        |_, _, _| (),
        move |bounds: Bounds<gpui::Pixels>, _, window, _| {
            let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let x = |v: f64| (v / plot.x_max) as f32 * (w - 8.0) + 4.0;
            let y = |v: f64| h - 4.0 - (v / plot.y_max) as f32 * (h - 8.0);
            let pt = |(rx, fy): (f64, f64)| at(bounds.origin, x(rx), y(fy));
            // The grid: 4 lines each way.
            for step in 0..=4 {
                let t = step as f32 / 4.0;
                polyline(
                    window,
                    &[
                        at(bounds.origin, 0.0, t * (h - 4.0) + 2.0),
                        at(bounds.origin, w, t * (h - 4.0) + 2.0),
                    ],
                    1.0,
                    border,
                    None,
                );
            }
            if let Some((a, b)) = plot.origin {
                polyline(window, &[pt(a), pt(b)], 2.0, dash, Some((5.0, 4.0)));
            }
            let trace: Vec<_> = plot.points.iter().copied().map(pt).collect();
            polyline(window, &trace, 2.5, line, None);
            if let Some((a, b)) = plot.extension {
                polyline(window, &[pt(a), pt(b)], 2.0, dash, Some((5.0, 4.0)));
            }
            for (index, point) in plot.points.iter().enumerate() {
                let selected = plot.selected == Some(index);
                disc(
                    window,
                    pt(*point),
                    if selected { 6.0 } else { 4.5 },
                    if selected { line } else { fg },
                    None,
                );
            }
            if let Some(marker) = &plot.marker {
                let color = if marker.out_of_range { warn } else { ok };
                polyline(
                    window,
                    &[
                        at(bounds.origin, x(marker.raw), 0.0),
                        at(bounds.origin, x(marker.raw), h),
                    ],
                    1.5,
                    color,
                    None,
                );
            }
        },
    )
    .absolute()
    .size_full();
    let mut view = div()
        .relative()
        .flex_1()
        .h(px(220.))
        .rounded(px(6.))
        .bg(p.view)
        .child(chart);
    if let Some((label, out)) = marker_label {
        view = view.child(
            div()
                .absolute()
                .top(px(6.))
                .right(px(8.))
                .text_size(px(12.))
                .font_weight(FontWeight::BOLD)
                .text_color(if out { p.warning } else { p.fg })
                .child(label),
        );
    }
    if has_extension {
        view = view.child(
            div()
                .absolute()
                .bottom(px(6.))
                .right(px(8.))
                .text_size(px(12.))
                .text_color(p.fg_dim)
                .child(extension_label),
        );
    }
    view.into_any_element()
}
