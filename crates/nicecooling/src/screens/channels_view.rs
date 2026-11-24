//! The window of the Channels screen, with the Curve editor.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, Bounds, Context, Div, FontWeight, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, SharedString, Window, canvas, div, px, relative, size,
};
use gpui_component::button::ButtonVariants as _;
use gpui_component::slider::Slider;
use gpui_component::tooltip::Tooltip;
use gpui_component::{Disableable as _, h_flex, v_flex};

use crate::icons::{NcIcon, icon};
use crate::model::Severity;
use crate::screen_view::{Drag, InputKey, ScreenView, skeleton};
use crate::screens::channels::{
    ChannelDetail, ChannelRow, ChannelsModel, ChannelsState, CurveDetail, CurvePlot,
};
use crate::screens::controls::act;
use crate::screens::format::plain;
use crate::screens::sensors_view::PickRow;
use crate::screens::ui::{
    TagKind, caption, card, card_row, heading, modal, page, seg, seg_item, select_trigger,
    severity_icon, table_row, tag, with_popover, write_line,
};
use crate::tokens::Palette;
use crate::widgets::paint::{at, hatch, polyline};

type Pick = Rc<dyn Fn(&mut ScreenView, String, &mut Window, &mut Context<ScreenView>)>;

/// The margins of the plot inside its frame, for the axis labels.
const LEFT: f32 = 44.0;
const RIGHT: f32 = 16.0;
const TOP: f32 = 12.0;
const BOTTOM: f32 = 32.0;
pub const PLOT_HEIGHT: f32 = 292.0;
/// The widths of the "rpm" and "Duty" cells of the list.
const RPM: f32 = 44.0;
const DUTY: f32 = 52.0;

/// "Input 31.4 °C" with the value in bold. The text of the locale ends with the value.
fn readout_text(text: &str, value: &str) -> Div {
    let label = text.strip_suffix(value).unwrap_or(text).to_owned();
    h_flex()
        .flex_none()
        .gap(px(4.))
        .child(label.trim_end().to_owned())
        .when(text.ends_with(value), |line| {
            line.child(div().font_weight(FontWeight::BOLD).child(value.to_owned()))
        })
}

impl ScreenView {
    pub(crate) fn render_channels(
        &mut self,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state(cx);
        self.channels.sync(&state);
        let model = self.with_input(cx, |input| ChannelsModel::build(input, &self.channels));
        let view = cx.entity();
        let mut content = page();
        match model.state {
            ChannelsState::Loading => {
                return content
                    .child(
                        card(p)
                            .p(px(18.))
                            .gap(px(12.))
                            .child(skeleton(p, 240.))
                            .child(skeleton(p, 360.)),
                    )
                    .into_any_element();
            }
            ChannelsState::Empty => {
                let go = view.clone();
                return content
                    .child(
                        card(p)
                            .p(px(18.))
                            .gap(px(12.))
                            .child(model.empty_text.clone())
                            .child(
                                h_flex().child(
                                    Self::link_button(
                                        "channels-settings",
                                        model.empty_link.clone(),
                                        p,
                                    )
                                    .on_click(
                                        move |_, _, cx| {
                                            go.update(cx, |this, cx| {
                                                this.go(crate::shell::Page::Settings, cx)
                                            })
                                        },
                                    ),
                                ),
                            ),
                    )
                    .into_any_element();
            }
            ChannelsState::Ready => {}
        }

        if let Some(text) = &model.pump_stop {
            content = content.child(
                h_flex()
                    .w_full()
                    .px(px(18.))
                    .py(px(10.))
                    .gap(px(12.))
                    .rounded(px(12.))
                    .bg(p.alarm_bg)
                    .text_color(p.alarm_fg)
                    .font_weight(FontWeight::BOLD)
                    .child(icon(NcIcon::Alarm, p.alarm_fg))
                    .child(div().flex_1().min_w_0().child(text.clone())),
            );
        }
        let locale = self.model.read(cx).locale;
        let mut list = card(p)
            .child(
                h_flex()
                    .min_h(px(40.))
                    .pl(px(18.))
                    .pr(px(12.))
                    .py(px(6.))
                    .gap(px(9.))
                    .child(heading(model.title.clone()).flex_none())
                    .child(caption(model.board.clone(), p).flex_1().truncate()),
            )
            .child(
                h_flex()
                    .px(px(18.))
                    .py(px(6.))
                    .gap(px(12.))
                    .text_size(px(12.))
                    .line_height(px(16.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.fg_dim)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(locale.tr("channels.columns.channel")),
                    )
                    .child(
                        div()
                            .w(px(RPM))
                            .flex_none()
                            .text_right()
                            .child(locale.tr("channels.columns.rpm")),
                    )
                    .child(
                        div()
                            .w(px(DUTY))
                            .flex_none()
                            .text_right()
                            .child(locale.tr("channels.columns.duty")),
                    ),
            );
        for (index, row) in model.rows.iter().enumerate() {
            list = list.child(self.channel_row(index, row, p, cx));
        }
        // Up and Down move the selection in the list.
        let rows = model.rows.clone();
        let mut list = div()
            .id("channels-list")
            .w(px(360.))
            .flex_none()
            .child(list);
        if let Some(focus) = self.channels_focus.clone() {
            let keys = view.clone();
            list = list.track_focus(&focus).on_key_down(move |event, _, cx| {
                let down = match event.keystroke.key.as_str() {
                    "down" => true,
                    "up" => false,
                    _ => return,
                };
                if let Some(id) = crate::screens::channels::neighbor(&rows, down) {
                    keys.update(cx, |this, cx| {
                        this.channels.select(&id);
                        this.open_picker = None;
                        cx.notify();
                    });
                }
            });
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
                        .map(|detail| self.channel_detail(detail, p, window, cx)),
                ),
        );
        let mut root = div().relative().w_full().child(content);
        if let Some(dialog) = model
            .detail
            .as_ref()
            .and_then(|d| d.release_dialog.as_ref())
        {
            let (cancel, confirm) = (view.clone(), view.clone());
            root = root.child(modal(
                dialog.title.clone(),
                dialog.text.clone(),
                Vec::new(),
                vec![
                    Self::button("release-cancel", dialog.cancel.clone())
                        .on_click(move |_, _, cx| {
                            cancel.update(cx, |this, cx| {
                                this.channels.cancel_release();
                                cx.notify();
                            })
                        })
                        .into_any_element(),
                    Self::button("release-confirm", dialog.confirm.clone())
                        .primary()
                        .on_click(move |_, _, cx| {
                            confirm.update(cx, |this, cx| {
                                let state = this.state(cx);
                                if let Some(command) = this.channels.confirm_release(&state) {
                                    this.send(command);
                                }
                                cx.notify();
                            })
                        })
                        .into_any_element(),
                ],
                p,
            ));
        }
        root.into_any_element()
    }

    fn channel_row(
        &mut self,
        index: usize,
        row: &ChannelRow,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let id = row.id.clone();
        let focus = self.channels_focus.clone();
        // The first line: the state icon, the name and the tag. A long name wraps.
        let mut first = h_flex().flex_wrap().gap_x(px(6.)).items_center();
        if row.line2_warning {
            first = first.child(severity_icon(Severity::Warning, p));
        } else if row.no_signal.is_some() {
            first = first.child(div().flex_none().child(icon(NcIcon::Off, p.offline)));
        }
        first = first
            .child(div().min_w_0().child(row.name.clone()))
            .children(
                row.pump_tag
                    .clone()
                    .map(|text| tag(text, TagKind::Accent, p)),
            );
        // The second line: the control, and the state of the speed signal.
        let second = match &row.no_signal {
            Some(text) => format!("{}, {text}", row.line2),
            None => row.line2.clone(),
        };
        let name = v_flex().flex_1().min_w_0().child(first).child(
            caption(second, p)
                .when(row.selected, |line| line.text_color(p.fg))
                .when(row.line2_warning, |line| line.text_color(p.warning)),
        );
        h_flex()
            .id(("channel-row", index))
            .min_h(px(48.))
            .px(px(18.))
            .py(px(6.))
            .gap(px(12.))
            .items_center()
            .border_t_1()
            .border_color(p.border)
            .cursor_pointer()
            .when(row.selected, |line| line.bg(p.accent_tint))
            .when(!row.selected, |line| line.hover(|line| line.bg(p.hover)))
            .on_click(move |_, window, cx| {
                let id = id.clone();
                if let Some(focus) = &focus {
                    window.focus(focus, cx);
                }
                view.update(cx, |this, cx| {
                    this.channels.select(&id);
                    this.open_picker = None;
                    cx.notify();
                })
            })
            .child(name)
            .child(
                div()
                    .w(px(RPM))
                    .flex_none()
                    .text_right()
                    .when(row.no_signal.is_some(), |cell| cell.text_color(p.fg_dim))
                    .whitespace_nowrap()
                    .child(row.rpm.clone()),
            )
            .child(
                div()
                    .w(px(DUTY))
                    .flex_none()
                    .text_right()
                    .whitespace_nowrap()
                    .child(row.duty.clone()),
            )
            .into_any_element()
    }

    fn channel_detail(
        &mut self,
        detail: &ChannelDetail,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.model.read(cx).locale;
        let view = cx.entity();
        let enabled = detail.editable;
        let reason = detail.disabled_reason.clone().unwrap_or_default();

        // The header: the name, the rename button with its popover, the tag.
        let rename_popover = self.channels.rename.clone().map(|(_, name)| {
            self.rename_popover(
                InputKey::ChannelRename,
                &name,
                locale.tr("sensors.rename_placeholder"),
                p,
                window,
                cx,
            )
        });
        let rename = Self::icon_button("channel-rename", NcIcon::Pencil)
            .disabled(!enabled)
            .tooltip(if reason.is_empty() {
                locale.tr("channels.rename")
            } else {
                reason.clone()
            })
            .on_click(act(&view, |this, _, cx| {
                if this.channels.rename.take().is_none() {
                    let state = this.state(cx);
                    this.channels.open_rename(&state);
                }
            }));
        let header = h_flex()
            .min_h(px(40.))
            .pl(px(18.))
            .pr(px(12.))
            .py(px(6.))
            .gap(px(9.))
            .child(heading(detail.name.clone()))
            .child(with_popover(rename, rename_popover, Self::dismiss(&view)).flex_none())
            .children(
                detail
                    .pump_tag
                    .clone()
                    .map(|text| tag(text, TagKind::Accent, p)),
            );

        // Control: a segmented control.
        let mut group = seg(p);
        for button in &detail.controls {
            let control = button.control;
            let item = seg_item(
                SharedString::from(format!("control-{}", control.id())),
                button.label.clone(),
                button.active,
                enabled,
                p,
            );
            group = group.child(if enabled {
                item.on_click(act(&view, move |this, _, cx| {
                    let state = this.state(cx);
                    if let Some(command) = this.channels.request_control(&state, control) {
                        this.send(command);
                    }
                }))
            } else {
                let reason = reason.clone();
                item.when(!reason.is_empty(), |item| {
                    item.tooltip(move |window, cx| Tooltip::new(reason.clone()).build(window, cx))
                })
            });
        }
        let mut body = v_flex().w_full().child(header).child(
            h_flex()
                .px(px(18.))
                .pb(px(12.))
                .gap(px(12.))
                .items_center()
                .flex_wrap()
                .child(caption(locale.tr("channels.control.label"), p).flex_none())
                .child(group),
        );

        if let Some(text) = &detail.lost_input {
            body = body.child(
                card_row(p)
                    .bg(p.warning_tint)
                    .child(severity_icon(Severity::Warning, p))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(p.fg)
                            .child(text.clone()),
                    ),
            );
        }
        // The write feedback: at the right of the readout row of a Curve, else in its own row.
        let feedback = detail.write_line.as_ref().map(|line| {
            let retry = line.retry.clone();
            h_flex()
                .min_w_0()
                .gap(px(9.))
                .items_center()
                .child(write_line(line, p))
                .children(retry.map(|label| {
                    Self::small_button("channel-retry", label, None).on_click(act(
                        &view,
                        |this, _, _| {
                            if let Some(command) = this.channels.last_command.clone() {
                                this.send(command);
                            }
                        },
                    ))
                }))
        });
        if let Some(curve) = &detail.curve {
            body = body.child(self.curve_section(detail, curve, feedback, p, window, cx));
        } else {
            if let Some(fixed) = &detail.fixed {
                let slider = self.slider(
                    &format!("fixed:{}", detail.id),
                    fixed.duty as f32,
                    window,
                    cx,
                );
                let number = self.input(
                    InputKey::FixedDuty(detail.id.clone()),
                    &plain(locale, fixed.duty),
                    "",
                    window,
                    cx,
                );
                body = body.child(
                    h_flex()
                        .px(px(18.))
                        .pb(px(12.))
                        .gap(px(12.))
                        .items_center()
                        .child(caption(locale.tr("channels.field.fixed"), p).flex_none())
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .child(Slider::new(&slider).disabled(!enabled)),
                        )
                        .child(
                            div().w(px(86.)).flex_none().child(
                                Self::field_input(&number, Some("%".into()), false, p)
                                    .disabled(!enabled),
                            ),
                        ),
                );
                if let Some(text) = &fixed.floor_text {
                    body = body.child(
                        div()
                            .px(px(18.))
                            .pb(px(12.))
                            .child(caption(text.clone(), p)),
                    );
                }
                if let Some(text) = &fixed.error {
                    body = body.child(
                        card_row(p)
                            .bg(p.warning_tint)
                            .child(severity_icon(Severity::Warning, p))
                            .child(div().flex_1().min_w_0().child(text.clone())),
                    );
                }
            }
            if let Some(text) = &detail.hardware_text {
                body = body.child(div().px(px(18.)).pb(px(12.)).child(text.clone()));
            }
            if let Some(feedback) = feedback {
                body = body.child(div().px(px(18.)).pb(px(12.)).child(feedback));
            }
        }

        // BIOS snapshot: a collapsed row.
        let open = detail.bios.open;
        body = body.child(
            card_row(p)
                .id("bios-row")
                .cursor_pointer()
                .hover(|row| row.bg(p.hover))
                .on_click(act(&view, |this, _, _| {
                    this.channels.bios_open = !this.channels.bios_open;
                }))
                .child(icon(
                    if open { NcIcon::Down } else { NcIcon::Right },
                    p.fg_dim,
                ))
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(heading(detail.bios.title.clone()))
                        .child(caption(detail.bios.text.clone(), p)),
                )
                .children(
                    detail
                        .bios
                        .saved
                        .clone()
                        .map(|saved| caption(saved, p).flex_none()),
                ),
        );
        if open {
            if let Some((temperature, duty)) = &detail.bios.columns {
                body = body.child(
                    h_flex()
                        .px(px(18.))
                        .py(px(6.))
                        .gap(px(12.))
                        .border_t_1()
                        .border_color(p.border)
                        .text_size(px(12.))
                        .line_height(px(16.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.fg_dim)
                        .child(div().w(px(120.)).child(temperature.clone()))
                        .child(div().flex_1().child(duty.clone())),
                );
            }
            for (temperature, duty) in &detail.bios.rows {
                body = body.child(
                    table_row(p)
                        .child(div().w(px(120.)).child(temperature.clone()))
                        .child(div().flex_1().child(duty.clone())),
                );
            }
        }
        card(p).flex_1().min_w_0().child(body).into_any_element()
    }

    fn curve_section(
        &mut self,
        detail: &ChannelDetail,
        curve: &CurveDetail,
        feedback: Option<Div>,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.model.read(cx).locale;
        let view = cx.entity();
        let enabled = detail.editable;
        let channel = detail.id.clone();

        // The fields: Input, smoothing, step limit.
        let smoothing = self.input(
            InputKey::CurvePoint(channel.clone(), usize::MAX, 0),
            &plain(locale, curve.smoothing_s),
            "",
            window,
            cx,
        );
        let step = self.input(
            InputKey::CurvePoint(channel.clone(), usize::MAX, 1),
            &plain(locale, curve.step_limit),
            "",
            window,
            cx,
        );
        let input_open = enabled && self.open_picker.as_deref() == Some("curve-input");
        let picker = input_open.then(|| {
            let selected = Some(curve.input.sensor.clone());
            let rows: Vec<PickRow> = self
                .picker_rows(selected, None, cx)
                .into_iter()
                .filter(|row| curve.input.options.iter().any(|option| option.id == row.id))
                .collect();
            let pick: Pick = Rc::new(|this, sensor, _, cx| {
                let state = this.state(cx);
                if let Some(command) = this.channels.set_input(&state, &sensor) {
                    this.send(command);
                }
                this.open_picker = None;
                cx.notify();
            });
            self.picker_list("curve-input", rows, pick, p, window, cx)
        });
        let input_select = select_trigger(
            "curve-input",
            curve.input.name.clone(),
            Some(curve.input.value.clone().into()),
            false,
            enabled,
            p.track,
        )
        .when(enabled, |trigger| {
            trigger.on_click(act(&view, |this, _, _| {
                this.open_picker = if this.open_picker.as_deref() == Some("curve-input") {
                    None
                } else {
                    Some("curve-input".to_owned())
                };
            }))
        });
        // The label wraps: an Italian label is longer than its field at the minimum window.
        let field = |label: String, control: AnyElement, grow: f32| {
            let mut column = v_flex()
                .flex_1()
                .min_w(px(if grow > 1. { 0. } else { 118. }))
                .gap(px(3.))
                .child(caption(label, p))
                .child(control);
            column.style().flex_grow = Some(grow);
            column
        };
        let help = |id: &'static str, text: String, column: Div| {
            column
                .id(id)
                .tooltip(move |window, cx| Tooltip::new(text.clone()).build(window, cx))
        };
        let fields = h_flex()
            .w_full()
            .px(px(18.))
            .pb(px(12.))
            .gap(px(12.))
            .items_end()
            .child(field(
                curve.input.label.clone(),
                with_popover(input_select, picker, Self::dismiss(&view)).into_any_element(),
                2.,
            ))
            .child(help(
                "curve-smoothing",
                curve.smoothing_help.clone(),
                field(
                    curve.smoothing_label.clone(),
                    Self::field_input(
                        &smoothing,
                        Some(curve.smoothing_unit.clone().into()),
                        false,
                        p,
                    )
                    .disabled(!enabled)
                    .into_any_element(),
                    1.,
                ),
            ))
            .child(help(
                "curve-step",
                curve.step_help.clone(),
                field(
                    curve.step_label.clone(),
                    Self::field_input(&step, Some(curve.step_unit.clone().into()), false, p)
                        .disabled(!enabled)
                        .into_any_element(),
                    1.,
                ),
            ));

        let readout = &curve.readout;
        let mut section = v_flex()
            .w_full()
            .child(fields)
            .child(
                div()
                    .px(px(18.))
                    .pb(px(12.))
                    .child(self.curve_plot(&curve.plot, p, window, cx)),
            )
            .child(
                h_flex()
                    .px(px(18.))
                    .pt(px(6.))
                    .pb(px(12.))
                    .gap_x(px(18.))
                    .gap_y(px(6.))
                    .flex_wrap()
                    .items_center()
                    .child(
                        h_flex()
                            .flex_none()
                            .gap(px(4.))
                            .child(readout_text(&readout.input, &readout.values[0]))
                            .child(div().text_color(p.fg_dim).child(readout.smoothing.clone())),
                    )
                    .child(readout_text(&readout.curve, &readout.values[1]))
                    .child(readout_text(&readout.duty, &readout.values[2]))
                    .child(div().flex_1())
                    .children(feedback),
            );

        // The points as number inputs: the plot and the inputs edit the same data.
        let mut points = h_flex()
            .px(px(18.))
            .pb(px(12.))
            .gap_x(px(2.))
            .gap_y(px(6.))
            .flex_wrap()
            .items_center()
            .child(
                v_flex()
                    .w(px(50.))
                    .flex_none()
                    .gap(px(15.))
                    .child(caption(locale.tr("channels.point_input"), p).truncate())
                    .child(caption(locale.tr("channels.point_duty"), p).truncate()),
            );
        for (index, (x, y)) in curve.points.iter().enumerate() {
            let xs = self.input(
                InputKey::CurvePoint(channel.clone(), index, 0),
                &plain(locale, *x),
                "",
                window,
                cx,
            );
            let ys = self.input(
                InputKey::CurvePoint(channel.clone(), index, 1),
                &plain(locale, *y),
                "",
                window,
                cx,
            );
            // The inputs of the selected point have a 2 px accent outline.
            let outline = if curve.plot.selected == index {
                p.accent
            } else {
                gpui::transparent_black()
            };
            let cell = |state, unit: SharedString| {
                div()
                    .w(px(78.))
                    .flex_none()
                    .rounded(px(9.))
                    .border_2()
                    .border_color(outline)
                    .child(Self::field_input(state, Some(unit), true, p).disabled(!enabled))
            };
            points = points.child(
                v_flex()
                    .flex_none()
                    .child(cell(&xs, curve.input_unit.clone().into()))
                    .child(cell(&ys, "%".into())),
            );
        }
        section = section.child(points).child(
            h_flex()
                .px(px(18.))
                .pb(px(12.))
                .gap(px(6.))
                .child(
                    Self::small_button(
                        "point-add",
                        locale.tr("channels.add_point"),
                        Some(NcIcon::Plus),
                    )
                    .disabled(!enabled || !curve.can_add)
                    .on_click(act(&view, |this, _, cx| {
                        let state = this.state(cx);
                        if let Some(command) = this.channels.add_point(&state, None) {
                            this.send(command);
                        }
                    })),
                )
                .child({
                    let selected = curve.plot.selected;
                    Self::small_button(
                        "point-remove",
                        locale.tr("channels.remove_point"),
                        Some(NcIcon::Trash),
                    )
                    .ghost()
                    .disabled(!enabled || !curve.can_remove)
                    .on_click(act(&view, move |this, _, cx| {
                        let state = this.state(cx);
                        if let Some(command) = this.channels.remove_point(&state, selected) {
                            this.send(command);
                        }
                    }))
                }),
        );
        section.into_any_element()
    }

    /// The Curve plot: grid, Curve, Pump floor zone and operating point under the handles.
    fn curve_plot(
        &mut self,
        plot: &CurvePlot,
        p: &Palette,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let scale = plot.scale;
        let frac_x = move |x: f64| (((x - scale.x_min) / scale.span()) as f32).clamp(0.0, 1.0);
        let frac_y = |y: f64| 1.0 - (y / 100.0) as f32;
        let (line, grid, axis, fg, dash, card_color) = (
            p.series[0],
            p.border,
            p.border_strong,
            p.fg,
            p.fg_dim,
            p.card,
        );
        let points = plot.points.clone();
        let floor = plot.floor.as_ref().map(|f| f.0);
        let operating = plot.operating.as_ref().map(|o| (o.x, o.y));
        let cell = self.plot_bounds.clone();
        let (x_min, x_max) = (scale.x_min, scale.x_max);

        let paint = canvas(
            {
                let cell = cell.clone();
                move |bounds: Bounds<Pixels>, _, _| cell.set(bounds)
            },
            move |bounds: Bounds<Pixels>, _, window, _| {
                let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                let px_x = |x: f64| frac_x(x) * w;
                let px_y = |y: f64| frac_y(y) * h;
                // The grid is the snap grid: every 5 units on x, every 20 % on y.
                let mut x = (x_min / 5.0).ceil() * 5.0;
                while x <= x_max {
                    if x > x_min {
                        polyline(
                            window,
                            &[
                                at(bounds.origin, px_x(x), 0.0),
                                at(bounds.origin, px_x(x), h),
                            ],
                            1.0,
                            grid,
                            None,
                        );
                    }
                    x += 5.0;
                }
                for step in 0..5 {
                    let y = h * step as f32 / 5.0;
                    polyline(
                        window,
                        &[at(bounds.origin, 0.0, y), at(bounds.origin, w, y)],
                        1.0,
                        grid,
                        None,
                    );
                }
                if let Some(floor) = floor {
                    let zone = Bounds::new(
                        at(bounds.origin, 0.0, px_y(floor)),
                        size(px(w), px(h - px_y(floor))),
                    );
                    hatch(window, zone, 7.0, dash.opacity(0.3));
                    polyline(
                        window,
                        &[
                            at(bounds.origin, 0.0, px_y(floor)),
                            at(bounds.origin, w, px_y(floor)),
                        ],
                        1.0,
                        dash,
                        Some((5.0, 4.0)),
                    );
                }
                // The axes: the left and the bottom edge.
                polyline(
                    window,
                    &[
                        at(bounds.origin, 0.0, 0.0),
                        at(bounds.origin, 0.0, h),
                        at(bounds.origin, w, h),
                    ],
                    1.0,
                    axis,
                    None,
                );
                // The Curve is horizontal before the first point and after the last.
                if let (Some(first), Some(last)) = (points.first(), points.last()) {
                    let mut trace = vec![at(bounds.origin, 0.0, px_y(first.1))];
                    trace.extend(
                        points
                            .iter()
                            .map(|(x, y)| at(bounds.origin, px_x(*x), px_y(*y))),
                    );
                    trace.push(at(bounds.origin, w, px_y(last.1)));
                    polyline(window, &trace, 2.5, line, None);
                }
                if let Some((x, y)) = operating {
                    polyline(
                        window,
                        &[
                            at(bounds.origin, px_x(x), px_y(y)),
                            at(bounds.origin, px_x(x), h),
                        ],
                        1.0,
                        fg,
                        Some((3.0, 3.0)),
                    );
                    polyline(
                        window,
                        &[
                            at(bounds.origin, 0.0, px_y(y)),
                            at(bounds.origin, px_x(x), px_y(y)),
                        ],
                        1.0,
                        fg,
                        Some((3.0, 3.0)),
                    );
                    crate::widgets::paint::disc(
                        window,
                        at(bounds.origin, px_x(x), px_y(y)),
                        7.0,
                        card_color,
                        None,
                    );
                    crate::widgets::paint::disc(
                        window,
                        at(bounds.origin, px_x(x), px_y(y)),
                        5.0,
                        fg,
                        None,
                    );
                }
            },
        )
        .absolute()
        .size_full();

        let mut area = div()
            .id("curve-plot")
            .absolute()
            .left(px(LEFT))
            .right(px(RIGHT))
            .top(px(TOP))
            .bottom(px(BOTTOM))
            .child(paint);

        // Axis labels: Duty at the left, Input at the bottom.
        let mut frame = div()
            .relative()
            .w_full()
            .h(px(PLOT_HEIGHT))
            .text_size(px(12.))
            .line_height(px(16.))
            .text_color(p.fg_dim);
        for step in 0..=5 {
            let duty = 100 - step * 20;
            frame = frame.child(
                div()
                    .absolute()
                    .left(px(0.))
                    .w(px(LEFT - 6.0))
                    .text_right()
                    .top(px(
                        TOP - 8.0 + (PLOT_HEIGHT - TOP - BOTTOM) * step as f32 / 5.0
                    ))
                    .child(format!("{duty}")),
            );
        }
        let mut x = (scale.x_min / 10.0).ceil() * 10.0;
        while x <= scale.x_max {
            frame = frame.child(
                div()
                    .absolute()
                    .left(px(LEFT))
                    .right(px(RIGHT))
                    .bottom(px(BOTTOM - 4.0))
                    .child(
                        div()
                            .absolute()
                            .left(relative(frac_x(x)))
                            .ml(px(-20.))
                            .w(px(40.))
                            .text_center()
                            .child(format!("{x:.0}")),
                    ),
            );
            x += 10.0;
        }
        frame = frame.child(
            div()
                .absolute()
                .right(px(RIGHT))
                .bottom(px(0.))
                .child(plot.x_title.clone()),
        );
        area = area.child(
            div()
                .absolute()
                .left(px(6.))
                .top(px(2.))
                .child(plot.y_title.clone()),
        );
        if let Some(floor) = &plot.floor {
            area = area.child(
                div()
                    .absolute()
                    .right(px(6.))
                    .top(relative(frac_y(floor.0)))
                    .mt(px(5.))
                    .px(px(4.))
                    .py(px(2.))
                    .rounded(px(4.))
                    .bg(p.card)
                    .child(floor.1.clone()),
            );
        }
        if let Some(operating) = &plot.operating {
            // The label sits at the right of the point and above it, or at the other side
            // near the right or the top edge of the plot.
            let (fx, fy) = (frac_x(operating.x), frac_y(operating.y));
            let label = div()
                .absolute()
                .px(px(8.))
                .py(px(3.))
                .rounded(px(6.))
                .bg(p.fg)
                .text_color(p.card)
                .font_weight(FontWeight::BOLD)
                .whitespace_nowrap()
                .child(operating.label.clone());
            let label = if fx > 0.6 {
                label.right(relative(1.0 - fx)).mr(px(10.))
            } else {
                label.left(relative(fx)).ml(px(10.))
            };
            area = area.child(if fy < 0.15 {
                label.top(relative(fy)).mt(px(8.))
            } else {
                label.top(relative(fy)).mt(px(-28.))
            });
        }

        // Handles: a circle of radius 6 on each point. The selected one has a halo.
        for (index, (x, y)) in plot.points.iter().enumerate() {
            let selected = plot.selected == index;
            let dot = div()
                .size(px(14.))
                .rounded_full()
                .border_2()
                .when(selected, |dot| dot.bg(p.series[0]).border_color(p.card))
                .when(!selected, |dot| dot.bg(p.card).border_color(p.series[0]));
            let handle = div()
                .id(("curve-handle", index))
                .absolute()
                .left(relative(frac_x(*x)))
                .top(relative(frac_y(*y)))
                .ml(px(-11.))
                .mt(px(-11.))
                .size(px(22.))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .when(selected, |h| h.bg(p.accent_tint))
                .child(dot)
                .when(plot.handles, |h| {
                    h.cursor_pointer().on_mouse_down(MouseButton::Left, {
                        let view = view.clone();
                        move |event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            view.update(cx, |this, cx| {
                                this.channels.selected_point = index;
                                if event.click_count == 2 {
                                    let state = this.state(cx);
                                    if let Some(command) = this.channels.remove_point(&state, index)
                                    {
                                        this.send(command);
                                    }
                                } else {
                                    this.drag = Some(Drag::CurvePoint(index));
                                }
                                if let Some(focus) = &this.plot_focus {
                                    window.focus(focus, cx);
                                }
                                cx.notify();
                            })
                        }
                    })
                });
            if plot.handles {
                area = area.child(handle);
            }
        }

        if plot.handles {
            area = area
                .on_mouse_move({
                    let view = view.clone();
                    move |event: &MouseMoveEvent, _, cx| {
                        view.update(cx, |this, cx| this.drag_curve(event.position, cx));
                    }
                })
                .on_mouse_up(MouseButton::Left, {
                    let view = view.clone();
                    move |_: &MouseUpEvent, _, cx| view.update(cx, |this, cx| this.end_drag(cx))
                })
                .on_mouse_up_out(MouseButton::Left, {
                    let view = view.clone();
                    move |_: &MouseUpEvent, _, cx| view.update(cx, |this, cx| this.end_drag(cx))
                })
                // A double click on the line adds a point.
                .on_mouse_down(MouseButton::Left, {
                    let view = view.clone();
                    move |event: &MouseDownEvent, _, cx| {
                        if event.click_count != 2 {
                            return;
                        }
                        let position = event.position;
                        view.update(cx, |this, cx| {
                            let bounds = this.plot_bounds.get();
                            let fx = f32::from(position.x - bounds.origin.x)
                                / f32::from(bounds.size.width);
                            let x = scale.x_min + fx as f64 * scale.span();
                            let state = this.state(cx);
                            if let Some(command) = this.channels.add_point(&state, Some(x)) {
                                this.send(command);
                            }
                            cx.notify();
                        });
                    }
                });
            if let Some(focus) = self.plot_focus.clone() {
                area = area.track_focus(&focus).on_key_down({
                    let view = view.clone();
                    move |event: &KeyDownEvent, _, cx| {
                        view.update(cx, |this, cx| this.curve_key(event, cx))
                    }
                });
            }
        }
        frame.child(area).into_any_element()
    }

    /// A drag step: the point follows the mouse.
    fn drag_curve(&mut self, position: gpui::Point<Pixels>, cx: &mut Context<Self>) {
        let Some(Drag::CurvePoint(index)) = self.drag else {
            return;
        };
        let bounds = self.plot_bounds.get();
        let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let fx = f32::from(position.x - bounds.origin.x) / w;
        let fy = f32::from(position.y - bounds.origin.y) / h;
        let state = self.state(cx);
        let scale = crate::widgets::curve_editor::PlotScale::for_points(
            self.channels.draft.as_ref().map_or(&[][..], |d| &d.points),
            None,
        );
        let scale = self.scale_of(&state, scale);
        let x = scale.x_min + fx as f64 * scale.span();
        let y = (1.0 - fy as f64) * 100.0;
        self.channels.drag_point(&state, index, x, y);
        cx.notify();
    }

    /// The scale of the plot that the model drew: the points of the daemon, not of the draft,
    /// so the axis does not move under the pointer.
    fn scale_of(
        &self,
        state: &crate::model::AppState,
        fallback: crate::widgets::curve_editor::PlotScale,
    ) -> crate::widgets::curve_editor::PlotScale {
        let Some(channel) = self.channels.current(state) else {
            return fallback;
        };
        let live = state.sensors.as_ref().and_then(|sample| {
            channel
                .curve
                .input
                .as_ref()
                .and_then(|id| sample.sensor_value(id))
        });
        crate::widgets::curve_editor::PlotScale::for_points(&channel.curve.points, live)
    }

    /// The drag ends: the Curve goes to the daemon.
    pub(crate) fn end_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(Drag::CurvePoint(_)) = self.drag.take()
            && let Some(command) = self.channels.commit()
        {
            self.send(command);
        }
        cx.notify();
    }

    /// Left and Right select a point. Shift with an arrow moves it by 1. Insert adds a point,
    /// Delete removes the selected one.
    fn curve_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
        let state = self.state(cx);
        let Some(channel) = self.channels.current(&state) else {
            return;
        };
        let count = self
            .channels
            .draft
            .as_ref()
            .map_or(channel.curve.points.len(), |d| d.points.len());
        let selected = self.channels.selected_point.min(count.saturating_sub(1));
        let keys = &event.keystroke;
        let step = |dx: f64, dy: f64, this: &mut Self| {
            let points = this
                .channels
                .draft
                .as_ref()
                .map(|d| d.points.clone())
                .or_else(|| {
                    this.channels
                        .current(&state)
                        .map(|c| c.curve.points.clone())
                })
                .unwrap_or_default();
            if let Some((x, y)) = points.get(selected)
                && let Some(command) = this.channels.set_point(&state, selected, x + dx, y + dy)
            {
                this.send(command);
            }
        };
        match keys.key.as_str() {
            "left" if keys.modifiers.shift => step(-1.0, 0.0, self),
            "right" if keys.modifiers.shift => step(1.0, 0.0, self),
            "up" if keys.modifiers.shift => step(0.0, 1.0, self),
            "down" if keys.modifiers.shift => step(0.0, -1.0, self),
            "left" => self.channels.selected_point = selected.saturating_sub(1),
            "right" => self.channels.selected_point = (selected + 1).min(count.saturating_sub(1)),
            "insert" => {
                if let Some(command) = self.channels.add_point(&state, None) {
                    self.send(command);
                }
            }
            "delete" => {
                if let Some(command) = self.channels.remove_point(&state, selected) {
                    self.send(command);
                }
            }
            _ => return,
        }
        cx.notify();
    }
}
