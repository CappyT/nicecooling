//! The window of the Sensors tab: the table of the device sensors and the Software sensors.

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, px};
use gpui_component::{Disableable as _, h_flex, v_flex};

use crate::model::Severity;
use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::controls::{Pick, act};
use crate::screens::leakshield::TabState;
use crate::screens::leakshield::sensors::{SensorsModel, SoftwareRow, set_source};
use crate::screens::ui::{
    caption, card, card_header, cell, label, modal, page, severity_icon, table_header, table_row,
    write_line,
};
use crate::tokens::Palette;

/// The widths of the table columns.
const NUMBER: f32 = 34.0;
const VALUE: f32 = 110.0;
const KIND: f32 = 120.0;
const SLOT_NAME: f32 = 150.0;
const UNIT: f32 = 50.0;
const SCALE: f32 = 76.0;

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
            .child(
                table_header(
                    &model.device_columns,
                    &[Some(NUMBER), None, Some(VALUE), Some(KIND)],
                    p,
                )
                .border_t_0(),
            );
        for row in &model.device_rows {
            let input = self.input(
                InputKey::Ls(format!("sensor-name:{}", row.sensor)),
                &row.name,
                "",
                window,
                cx,
            );
            // An inline input of a table row is 28 px high.
            let name =
                div()
                    .w_full()
                    .child(Self::mode_input(&input, "", row.enabled, false, true, p));
            let mut line = table_row(p)
                .text_color(if row.dim { p.fg_dim } else { p.fg })
                .child(
                    cell(NUMBER)
                        .text_color(p.fg_dim)
                        .child(row.number.to_string()),
                )
                .child(div().flex_1().min_w_0().child(name))
                .child(
                    cell(VALUE)
                        .text_right()
                        .font_weight(FontWeight::BOLD)
                        .child(row.value.clone()),
                )
                .child(cell(KIND).text_color(p.fg_dim).child(row.kind.clone()));
            if let Some(line_text) = &row.write_line {
                line = line.child(write_line(line_text, p));
            }
            device = device.child(line);
        }

        let mut software = card(p)
            .child(card_header(model.software_title.clone(), p))
            .child(
                table_header(
                    &model.software_columns,
                    &[
                        Some(NUMBER),
                        Some(SLOT_NAME),
                        None,
                        Some(UNIT),
                        Some(SCALE),
                        Some(SCALE),
                        Some(VALUE),
                    ],
                    p,
                )
                .border_t_0(),
            );
        for row in &model.software_rows {
            software = software.child(self.software_row(row, &model.clear_label, p, window, cx));
        }
        content = content
            .child(device)
            .child(software)
            .child(caption(model.caption.clone(), p).px(px(6.)));

        let mut root = div().relative().w_full().child(content);
        if let Some(dialog) = &model.dialog {
            let (cancel, confirm) = (view.clone(), view.clone());
            let id = id.to_owned();
            root = root.child(modal(
                dialog.title.clone(),
                dialog.text.clone(),
                Vec::new(),
                vec![
                    Self::small_button("ls-sensors-clear-cancel", dialog.cancel.clone(), None)
                        .on_click(act(&cancel, |this, _, _| this.leakshield.sensors.cancel()))
                        .into_any_element(),
                    Self::small_button("ls-sensors-clear-confirm", dialog.confirm.clone(), None)
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let slot = row.slot;
        let mut scale_cells = Vec::new();
        for (name, text) in [("scale", &row.scale), ("offset", &row.offset)] {
            let input = self.input(
                InputKey::Ls(format!("software-{name}:{slot}")),
                text,
                "",
                window,
                cx,
            );
            scale_cells.push(div().w(px(SCALE)).flex_none().child(Self::mode_input(
                &input,
                "",
                row.enabled,
                false,
                true,
                p,
            )));
        }
        let device = self.current_leakshield(cx);
        let pick: Pick = {
            let device = device.clone();
            std::rc::Rc::new(move |this, sensor, _, _| {
                this.send(set_source(&device, slot, &sensor));
            })
        };
        let select = self.select_field(
            &format!("ls-software-{slot}"),
            &row.source_text,
            row.choices.clone(),
            row.enabled,
            true,
            p.track,
            pick,
            p,
            cx,
        );
        let mut name = v_flex()
            .w(px(SLOT_NAME))
            .flex_none()
            .min_w_0()
            .child(label(row.name.clone()).font_weight(FontWeight::BOLD));
        for text in &row.used_by {
            name = name.child(caption(text.clone(), p));
        }
        let mut line = table_row(p)
            .min_h(px(if row.used_by.is_empty() { 36. } else { 48. }))
            .child(cell(NUMBER).text_color(p.fg_dim).child(slot.to_string()))
            .child(name)
            .child(div().flex_1().min_w_0().child(select))
            .child(cell(UNIT).text_color(p.fg_dim).child(row.unit.clone()))
            .children(scale_cells)
            .child(
                h_flex()
                    .w(px(VALUE))
                    .flex_none()
                    .debug_selector(move || format!("software-value-{slot}"))
                    .gap(px(6.))
                    .justify_end()
                    .font_weight(FontWeight::BOLD)
                    .when_some(row.warning.clone(), |value, _| {
                        value.child(severity_icon(Severity::Warning, p))
                    })
                    .child(row.value.clone()),
            );
        if row.source.is_some() {
            let device = device.clone();
            line = line.child(
                div()
                    .flex_none()
                    .debug_selector(move || format!("software-clear-{slot}"))
                    .child(
                        Self::flat_button(
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
                    ),
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
