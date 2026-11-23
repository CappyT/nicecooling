//! The window of the Sensors tab: the table of the device sensors and the Software sensors.

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, px};
use gpui_component::button::Button;
use gpui_component::input::Input;
use gpui_component::{Disableable as _, Sizable as _, h_flex, v_flex};

use crate::model::Severity;
use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::controls::{Pick, act};
use crate::screens::leakshield::TabState;
use crate::screens::leakshield::sensors::{SensorsModel, SoftwareRow, set_source};
use crate::screens::ui::{
    caption, card, card_header, card_row, modal, page, severity_icon, write_line,
};
use crate::tokens::Palette;

impl ScreenView {
    pub(crate) fn render_ls_sensors(
        &mut self,
        id: &str,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = self.with_input(cx, |input| {
            SensorsModel::build(input, &self.leakshield.sensors, id)
        });
        let view = cx.entity();
        let mut content = page();
        if model.state == TabState::Loading {
            return content
                .child(
                    card(p)
                        .p(px(18.))
                        .gap(px(12.))
                        .child(skeleton(p, 240.))
                        .child(skeleton(p, 480.)),
                )
                .into_any_element();
        }

        let mut device = card(p)
            .child(card_header(model.device_title.clone(), p))
            .child(header_row(&model.device_columns, &[40., 0., 120., 120.], p));
        for row in &model.device_rows {
            let input = self.input(
                InputKey::Ls(format!("sensor-name:{}", row.sensor)),
                &row.name,
                "",
                window,
                cx,
            );
            let mut line = card_row(p)
                .text_color(if row.dim { p.fg_dim } else { p.fg })
                .child(div().w(px(40.)).child(row.number.to_string()))
                .child(
                    div()
                        .flex_1()
                        .child(Input::new(&input).disabled(!row.enabled)),
                )
                .child(div().w(px(120.)).text_right().child(row.value.clone()))
                .child(div().w(px(120.)).child(row.kind.clone()));
            if let Some(line_text) = &row.write_line {
                line = line.child(write_line(line_text, p));
            }
            device = device.child(line);
        }

        let mut software = card(p)
            .child(card_header(model.software_title.clone(), p))
            .child(header_row(
                &model.software_columns,
                &[40., 150., 260., 70., 110.],
                p,
            ));
        for row in &model.software_rows {
            software = software.child(self.software_row(row, &model.clear_label, p, cx));
        }
        software = software.child(card_row(p).child(caption(model.caption.clone(), p)));
        content = content.child(device).child(software);

        let mut root = div().relative().w_full().child(content);
        if let Some(dialog) = &model.dialog {
            let (cancel, confirm) = (view.clone(), view.clone());
            let id = id.to_owned();
            root = root.child(modal(
                dialog.title.clone(),
                dialog.text.clone(),
                Vec::new(),
                vec![
                    Button::new("ls-sensors-clear-cancel")
                        .small()
                        .label(dialog.cancel.clone())
                        .on_click(act(&cancel, |this, _, _| this.leakshield.sensors.cancel()))
                        .into_any_element(),
                    Button::new("ls-sensors-clear-confirm")
                        .small()
                        .label(dialog.confirm.clone())
                        .on_click(act(&confirm, move |this, _, _| {
                            if let Some(command) = this.leakshield.sensors.confirm_clear(&id) {
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

    fn software_row(
        &mut self,
        row: &SoftwareRow,
        clear_label: &str,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let slot = row.slot;
        let device = self.current_leakshield(cx);
        let pick: Pick = {
            let device = device.clone();
            std::rc::Rc::new(move |this, sensor, _, _| {
                this.send(set_source(&device, slot, &sensor));
            })
        };
        let select = self.select(
            &format!("ls-software-{slot}"),
            &row.source_text,
            row.choices.clone(),
            row.enabled,
            "",
            pick,
            p,
            cx,
        );
        let mut name = v_flex()
            .w(px(150.))
            .child(div().font_weight(FontWeight::BOLD).child(row.name.clone()));
        for text in &row.used_by {
            name = name.child(caption(text.clone(), p));
        }
        let mut line = card_row(p)
            .child(div().w(px(40.)).child(slot.to_string()))
            .child(name)
            .child(div().w(px(260.)).child(select))
            .child(div().w(px(70.)).child(row.unit.clone()))
            .child(
                h_flex()
                    .w(px(110.))
                    .gap(px(6.))
                    .justify_end()
                    .when_some(row.warning.clone(), |value, _| {
                        value.child(severity_icon(Severity::Warning, p))
                    })
                    .child(row.value.clone()),
            );
        if row.source.is_some() {
            let device = device.clone();
            line = line.child(
                Self::link_button(
                    SharedString::from(format!("ls-software-clear-{slot}")),
                    clear_label.to_owned(),
                )
                .disabled(!row.enabled)
                .on_click(act(&view, move |this, _, cx| {
                    let state = this.state(cx);
                    if let Some(command) =
                        this.leakshield.sensors.request_clear(&state, &device, slot)
                    {
                        this.send(command);
                    }
                })),
            );
        }
        if let Some(text) = &row.warning {
            line = line.child(caption(text.clone(), p));
        }
        if let Some(text) = &row.write_line {
            line = line.child(write_line(text, p));
        }
        line.into_any_element()
    }
}

/// The captions of a table: a width of 0 takes the rest of the row.
pub(crate) fn header_row(names: &[String], widths: &[f32], p: &Palette) -> AnyElement {
    let mut line = h_flex()
        .h(px(32.))
        .px(px(18.))
        .gap(px(12.))
        .items_center()
        .text_size(px(12.))
        .font_weight(FontWeight::BOLD)
        .text_color(p.fg_dim);
    for (name, width) in names.iter().zip(widths) {
        line = line.child(if *width == 0.0 {
            div().flex_1().child(name.clone())
        } else {
            div().w(px(*width)).child(name.clone())
        });
    }
    line.into_any_element()
}
