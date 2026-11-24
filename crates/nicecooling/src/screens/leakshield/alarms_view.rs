//! The window of the Alarms tab of a Leakshield.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, px};
use gpui_component::slider::Slider;
use gpui_component::switch::Switch;
use gpui_component::{Disableable as _, Sizable as _, h_flex, v_flex};

use crate::icons::{NcIcon, icon};
use crate::model::Response;
use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::controls::{FieldMode, Pick, act};
use crate::screens::format::plain;
use crate::screens::leakshield::TabState;
use crate::screens::leakshield::alarms::{AlarmsModel, EventRow, RecommendationKind};
use crate::screens::ui::Tip as _;
use crate::screens::ui::{
    caption, card, card_header, card_row, locked, notice_row, page, severity_icon, table_header,
    write_line,
};
use crate::tokens::Palette;

/// The widths of the table columns.
const THRESHOLD: f32 = 170.0;
const RESPONSE: f32 = 190.0;

impl ScreenView {
    pub(crate) fn render_ls_alarms(
        &mut self,
        id: &str,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = self.with_input(cx, |input| {
            AlarmsModel::build(input, &self.leakshield.alarms, id)
        });
        let mut content = page();
        if model.state == TabState::Loading {
            return content
                .child(
                    card(p)
                        .p(px(18.))
                        .gap(px(12.))
                        .child(skeleton(p, 240.))
                        .child(skeleton(p, 520.)),
                )
                .into_any_element();
        }
        if let Some(reset) = &model.reset {
            let view = cx.entity();
            let command = reset.command.clone();
            let mut row = notice_row(reset.text.clone(), true, p).child(
                Self::small_button("ls-reset-alarms", reset.label.clone(), None)
                    .disabled(!reset.enabled)
                    .on_click(act(&view, move |this, _, _| this.send(command.clone()))),
            );
            if let Some(line) = &reset.write_line {
                row = row.child(write_line(line, p));
            }
            content = content.child(card(p).child(row));
        }
        if !model.recommendations.is_empty() {
            let view = cx.entity();
            let mut recommendation =
                card(p).child(card_header(model.recommendation_title.clone(), p));
            for (index, item) in model.recommendations.iter().enumerate() {
                let mut row = match item.kind {
                    RecommendationKind::Warning => notice_row(item.text.clone(), true, p),
                    RecommendationKind::Information => notice_row(item.text.clone(), false, p),
                    RecommendationKind::Success => card_row(p)
                        .child(icon(NcIcon::Check, p.success))
                        .child(div().flex_1().child(item.text.clone())),
                };
                if let Some((label, command)) = item.button.clone() {
                    row = row.child(
                        Self::small_button(
                            SharedString::from(format!("ls-rec-{index}")),
                            label,
                            None,
                        )
                        .disabled(model.state != TabState::Ready)
                        .on_click(act(&view, move |this, _, _| this.send(command.clone()))),
                    );
                }
                if let Some(line) = &item.write_line {
                    row = row.child(write_line(line, p));
                }
                recommendation = recommendation.child(row);
            }
            content = content.child(recommendation);
        }
        for group in &model.groups {
            let mut card = card(p).child(card_header(group.title.clone(), p)).child(
                table_header(
                    &model.columns,
                    &[None, Some(THRESHOLD), Some(RESPONSE), Some(RESPONSE)],
                    p,
                )
                .border_t_0(),
            );
            for row in &group.rows {
                card = card.child(self.alarm_event_row(id, row, &model, p, window, cx));
            }
            content = content.child(card);
        }
        content = content.child(caption(model.device_help.clone(), p).px(px(6.)));
        content.into_any_element()
    }

    fn alarm_event_row(
        &mut self,
        device: &str,
        row: &EventRow,
        model: &AlarmsModel,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let ready = model.state == TabState::Ready;
        let event = row.event.clone();

        let mut title = v_flex().flex_1().min_w_0().gap(px(3.)).child(
            h_flex()
                .gap(px(6.))
                .child(severity_icon(row.severity, p))
                .child(div().font_weight(FontWeight::BOLD).child(row.title.clone())),
        );
        title = title.child(caption(row.caption.clone(), p));
        if let Some(active) = &row.active {
            title = title.child(
                div()
                    .text_size(px(12.))
                    .text_color(if row.severity == crate::model::Severity::Alarm {
                        p.alarm
                    } else {
                        p.warning
                    })
                    .child(active.clone()),
            );
        }

        let mut threshold = v_flex().w(px(THRESHOLD)).flex_none().gap(px(6.));
        if let Some(ctl) = &row.threshold {
            let slider = self.range_slider(
                &format!("ls:threshold:{}", ctl.field),
                (ctl.min as f32, ctl.max as f32),
                ctl.value as f32,
                window,
                cx,
            );
            let input = self.input(
                InputKey::Ls(format!("threshold:{}", ctl.field)),
                &plain(self.model.read(cx).locale, ctl.value),
                "",
                window,
                cx,
            );
            threshold = threshold
                .child(Slider::new(&slider).disabled(!(ctl.writable && ready)))
                .child(div().w(px(110.)).flex_none().child(Self::mode_input(
                    &input,
                    &ctl.unit,
                    ctl.writable && ready,
                    ctl.error.is_some(),
                    false,
                    p,
                )));
            if let Some(reason) = &ctl.reason {
                threshold = threshold.child(
                    locked(reason.clone(), p)
                        .text_size(px(12.))
                        .text_color(p.fg_dim),
                );
            }
            if let Some(error) = &ctl.error {
                threshold = threshold.child(caption(error.clone(), p).text_color(p.alarm));
            }
        }

        let mut device_column = v_flex().w(px(RESPONSE)).flex_none().gap(px(6.));
        if let Some(ctl) = &row.device {
            let pick: Pick = {
                let device = device.to_owned();
                let event = event.clone();
                Rc::new(move |this, response, _, cx| {
                    let command = this.with_input(cx, |input| {
                        this.leakshield
                            .alarms
                            .set_device_response(input, &device, &event, &response)
                    });
                    if let Some(command) = command {
                        this.send(command);
                    }
                })
            };
            let shown = ctl
                .options
                .iter()
                .find(|(_, _, selected)| *selected)
                .map_or_else(String::new, |(_, text, _)| text.clone());
            let select = self.select_field(
                &format!("ls-device-{}", row.event),
                &shown,
                ctl.options.clone(),
                ctl.writable && ready,
                false,
                p.track,
                pick,
                p,
                cx,
            );
            // The reason of a read-only field is the tooltip of its select.
            let reason = ctl.reason.clone();
            device_column = device_column.child(
                div()
                    .id(SharedString::from(format!(
                        "ls-device-reason-{}",
                        row.event
                    )))
                    .when_some(reason, |select, reason| select.tip(reason))
                    .child(select),
            );
            if let Some(sound) = &row.sound {
                let device = device.to_owned();
                let event = event.clone();
                let view = view.clone();
                device_column = device_column.child(
                    h_flex()
                        .gap(px(8.))
                        .items_center()
                        .child(
                            Switch::new(SharedString::from(format!("ls-sound-{}", row.event)))
                                .large()
                                .checked(sound.on)
                                .disabled(!(sound.writable && ready))
                                .on_click(move |on, _, cx| {
                                    let on = *on;
                                    let (device, event) = (device.clone(), event.clone());
                                    view.update(cx, |this, cx| {
                                        let command = this.with_input(cx, |input| {
                                            this.leakshield
                                                .alarms
                                                .set_sound(input, &device, &event, on)
                                        });
                                        if let Some(command) = command {
                                            this.send(command);
                                        }
                                    });
                                }),
                        )
                        .child(model.sound_label.clone()),
                );
            }
        }

        let response = &row.response;
        let pick: Pick = {
            let event = response.event.clone();
            let device = device.to_owned();
            Rc::new(move |this, choice, _, cx| {
                let response = match choice.as_str() {
                    "poweroff" => Response::Poweroff,
                    "notify" => Response::Notify,
                    _ => Response::None,
                };
                let command = this.with_input(cx, |input| {
                    this.leakshield
                        .alarms
                        .set_alarm_response(input, &device, &event, response, None)
                });
                this.send(command);
            })
        };
        let shown = response
            .options
            .iter()
            .find(|(_, _, selected)| *selected)
            .map_or_else(String::new, |(_, text, _)| text.clone());
        let select = self.select_field(
            &format!("ls-response-{}", response.event),
            &shown,
            response
                .options
                .iter()
                .map(|(option, text, selected)| (option.id().to_owned(), text.clone(), *selected))
                .collect(),
            self.with_input(cx, |input| input.writes().enabled),
            false,
            p.track,
            pick,
            p,
            cx,
        );
        let delay = self.input(
            InputKey::Ls(format!("alarm-delay:{}", response.event)),
            &response.delay_s.to_string(),
            "",
            window,
            cx,
        );
        let mut response_column = v_flex()
            .w(px(RESPONSE))
            .flex_none()
            .gap(px(6.))
            .child(select)
            .child(div().w(px(90.)).flex_none().child(Self::mode_input(
                &delay,
                "s",
                if response.delay_enabled {
                    FieldMode::Editable
                } else {
                    FieldMode::Off
                },
                false,
                false,
                p,
            )));
        if let Some(text) = &response.delay_warning {
            response_column = response_column.child(caption(text.clone(), p).text_color(p.warning));
        }
        if let Some(command) = response.reset.clone() {
            response_column = response_column.child(
                Self::link_button(
                    SharedString::from(format!("ls-response-reset-{}", response.event)),
                    self.with_input(cx, |input| input.locale.tr("alarms.reset")),
                    p,
                )
                .on_click(act(&view, move |this, _, _| this.send(command.clone()))),
            );
        }
        if let Some(line) = &response.write_line {
            response_column = response_column.child(write_line(line, p));
        }
        if let Some(line) = &row.write_line {
            device_column = device_column.child(write_line(line, p));
        }

        card_row(p)
            .py(px(9.))
            .gap(px(18.))
            .items_start()
            .child(title)
            .child(threshold)
            .child(device_column)
            .child(response_column)
            .into_any_element()
    }
}
