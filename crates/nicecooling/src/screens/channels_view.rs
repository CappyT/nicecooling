//! The window of the Channels screen, with the Curve editor.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, Bounds, Context, FontWeight, KeyDownEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, SharedString, Window, canvas, div, px, relative, size,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::slider::Slider;
use gpui_component::{Disableable as _, Selectable as _, Sizable as _, h_flex, v_flex};

use crate::model::Severity;
use crate::screen_view::{Drag, InputKey, ScreenView, skeleton};
use crate::screens::channels::{
    ChannelDetail, ChannelRow, ChannelsModel, ChannelsState, CurveDetail, CurvePlot,
};
use crate::screens::format::plain;
use crate::screens::sensors_view::PickRow;
use crate::screens::ui::{
    TagKind, caption, card, card_header, card_row, page, severity_icon, tag, write_line,
};
use crate::tokens::Palette;
use crate::widgets::paint::{at, hatch, polyline};

type Pick = Rc<dyn Fn(&mut ScreenView, String, &mut Window, &mut Context<ScreenView>)>;

/// The margins of the plot inside its frame, for the axis labels.
const LEFT: f32 = 44.0;
const RIGHT: f32 = 16.0;
const TOP: f32 = 14.0;
const BOTTOM: f32 = 30.0;
pub const PLOT_HEIGHT: f32 = 292.0;

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
                                    Button::new("channels-settings")
                                        .ghost()
                                        .label(model.empty_link.clone())
                                        .on_click(move |_, _, cx| {
                                            go.update(cx, |this, cx| {
                                                this.go(crate::shell::Page::Settings, cx)
                                            })
                                        }),
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
                    .rounded(px(12.))
                    .bg(p.alarm_bg)
                    .text_color(p.alarm_fg)
                    .font_weight(FontWeight::BOLD)
                    .child(text.clone()),
            );
        }
        let locale = self.model.read(cx).locale;
        let mut list = card(p).w(px(360.)).flex_none().child(
            h_flex()
                .h(px(36.))
                .px(px(18.))
                .gap(px(12.))
                .items_center()
                .text_size(px(12.))
                .font_weight(FontWeight::BOLD)
                .text_color(p.fg_dim)
                .child(div().flex_1().child(locale.tr("channels.columns.channel")))
                .child(
                    div()
                        .w(px(70.))
                        .text_right()
                        .child(locale.tr("channels.columns.rpm")),
                )
                .child(
                    div()
                        .w(px(50.))
                        .text_right()
                        .child(locale.tr("channels.columns.duty")),
                ),
        );
        for (index, row) in model.rows.iter().enumerate() {
            list = list.child(self.channel_row(index, row, p, cx));
        }
        // Up and Down move the selection in the list.
        let rows = model.rows.clone();
        let mut list = div().id("channels-list").flex_none().child(list);
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
        div().relative().w_full().child(content).into_any_element()
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
        let mut name = v_flex().flex_1().min_w_0().child(
            h_flex()
                .gap(px(6.))
                .items_center()
                .child(
                    div()
                        .truncate()
                        .font_weight(FontWeight::BOLD)
                        .child(row.name.clone()),
                )
                .children(
                    row.pump_tag
                        .clone()
                        .map(|text| tag(text, TagKind::Accent, p)),
                ),
        );
        name = name.child(
            h_flex()
                .gap(px(4.))
                .text_size(px(12.))
                .text_color(p.fg_dim)
                .children(
                    row.line2_warning
                        .then(|| severity_icon(Severity::Warning, p)),
                )
                .child(row.line2.clone()),
        );
        let rpm = match &row.no_signal {
            Some(text) => div()
                .w(px(70.))
                .text_right()
                .text_size(px(11.))
                .text_color(p.offline)
                .child(format!("\u{25CC} {text}")),
            None => div().w(px(70.)).text_right().child(row.rpm.clone()),
        };
        h_flex()
            .id(("channel-row", index))
            .min_h(px(48.))
            .px(px(18.))
            .py(px(4.))
            .gap(px(12.))
            .items_center()
            .border_t_1()
            .border_color(p.border)
            .cursor_pointer()
            .when(row.selected, |line| line.bg(p.accent_tint))
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
            .child(rpm)
            .child(
                div()
                    .w(px(50.))
                    .text_right()
                    .font_weight(FontWeight::BOLD)
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

        let header = card_header(detail.name.clone(), p)
            .children(
                detail
                    .pump_tag
                    .clone()
                    .map(|text| tag(text, TagKind::Accent, p)),
            )
            .child({
                let view = view.clone();
                Button::new("channel-rename")
                    .small()
                    .ghost()
                    .label(locale.tr("channels.rename"))
                    .disabled(!enabled)
                    .tooltip(reason.clone())
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            let state = this.state(cx);
                            this.channels.open_rename(&state);
                            cx.notify();
                        })
                    })
            });
        let mut body = v_flex().w_full().child(header);
        if let Some((_, name)) = self.channels.rename.clone() {
            body = body.child(self.rename_popover(
                InputKey::ChannelRename,
                &name,
                locale.tr("sensors.rename_placeholder"),
                p,
                window,
                cx,
            ));
        }

        // Control: a button group.
        let mut group = h_flex().gap(px(6.));
        for button in &detail.controls {
            let view = view.clone();
            let control = button.control;
            group = group.child(
                Button::new(SharedString::from(format!("control-{}", control.id())))
                    .label(button.label.clone())
                    .selected(button.active)
                    .disabled(!enabled)
                    .tooltip(reason.clone())
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            let state = this.state(cx);
                            if let Some(command) = this.channels.request_control(&state, control) {
                                this.send(command);
                            }
                            cx.notify();
                        })
                    }),
            );
        }
        body = body.child(
            card_row(p)
                .child(
                    div()
                        .w(px(120.))
                        .font_weight(FontWeight::BOLD)
                        .child(locale.tr("channels.control.label")),
                )
                .child(group),
        );

        if let Some(text) = &detail.lost_input {
            body = body.child(
                card_row(p)
                    .bg(p.warning_tint)
                    .child(severity_icon(Severity::Warning, p))
                    .child(div().text_color(p.fg).child(text.clone())),
            );
        }
        if let Some(curve) = &detail.curve {
            body = body.child(self.curve_section(detail, curve, p, window, cx));
        }
        if let Some(fixed) = &detail.fixed {
            let slider = self.slider(
                &format!("fixed:{}", detail.id),
                fixed.duty as f32,
                window,
                cx,
            );
            body = body.child(
                card_row(p)
                    .child(
                        div()
                            .w(px(120.))
                            .font_weight(FontWeight::BOLD)
                            .child(locale.tr("channels.field.fixed")),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(Slider::new(&slider).disabled(!enabled)),
                    )
                    .child(
                        div()
                            .w(px(60.))
                            .text_right()
                            .font_weight(FontWeight::BOLD)
                            .child(crate::screens::loops::floor_text(locale, fixed.duty)),
                    ),
            );
            if let Some(text) = &fixed.floor_text {
                body = body.child(card_row(p).child(caption(text.clone(), p)));
            }
            if let Some(text) = &fixed.error {
                body = body.child(
                    card_row(p)
                        .child(severity_icon(Severity::Warning, p))
                        .child(text.clone()),
                );
            }
        }
        if let Some(text) = &detail.hardware_text {
            body = body.child(card_row(p).child(text.clone()));
        }
        if let Some(line) = &detail.write_line {
            let retry = line.retry.clone();
            body = body.child(card_row(p).child(write_line(line, p)).children(retry.map(
                |label| {
                    let view = view.clone();
                    Button::new("channel-retry")
                        .small()
                        .label(label)
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                if let Some(command) = this.channels.last_command.clone() {
                                    this.send(command);
                                }
                                cx.notify();
                            })
                        })
                },
            )));
        }

        // BIOS snapshot: a collapsed row.
        let open = detail.bios.open;
        body = body.child(
            card_row(p)
                .id("bios-row")
                .cursor_pointer()
                .on_click({
                    let view = view.clone();
                    move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.channels.bios_open = !this.channels.bios_open;
                            cx.notify();
                        })
                    }
                })
                .child(if open { "\u{25BE}" } else { "\u{25B8}" })
                .child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .child(detail.bios.title.clone()),
                ),
        );
        if open {
            body = body.child(card_row(p).child(caption(detail.bios.text.clone(), p)));
        }

        let mut column = card(p).flex_1().min_w_0().child(body);
        if let Some(dialog) = &detail.release_dialog {
            let (cancel, confirm) = (view.clone(), view.clone());
            column = column.child(
                card_row(p)
                    .bg(p.warning_tint)
                    .child(
                        v_flex()
                            .flex_1()
                            .text_color(p.fg)
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .child(dialog.title.clone()),
                            )
                            .child(dialog.text.clone()),
                    )
                    .child(
                        Button::new("release-cancel")
                            .small()
                            .label(dialog.cancel.clone())
                            .on_click(move |_, _, cx| {
                                cancel.update(cx, |this, cx| {
                                    this.channels.cancel_release();
                                    cx.notify();
                                })
                            }),
                    )
                    .child(
                        Button::new("release-confirm")
                            .small()
                            .danger()
                            .label(dialog.confirm.clone())
                            .on_click(move |_, _, cx| {
                                confirm.update(cx, |this, cx| {
                                    let state = this.state(cx);
                                    if let Some(command) = this.channels.confirm_release(&state) {
                                        this.send(command);
                                    }
                                    cx.notify();
                                })
                            }),
                    ),
            );
        }
        column.into_any_element()
    }

    fn curve_section(
        &mut self,
        detail: &ChannelDetail,
        curve: &CurveDetail,
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
        let input_open = self.open_picker.as_deref() == Some("curve-input");
        let fields = h_flex()
            .w_full()
            .gap(px(12.))
            .items_end()
            .child(
                v_flex()
                    .flex_1()
                    .gap(px(4.))
                    .child(caption(curve.input.label.clone(), p))
                    .child({
                        let view = view.clone();
                        Button::new("curve-input")
                            .small()
                            .label(format!("{}  {}", curve.input.name, curve.input.value))
                            .disabled(!enabled)
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.open_picker =
                                        if this.open_picker.as_deref() == Some("curve-input") {
                                            None
                                        } else {
                                            Some("curve-input".to_owned())
                                        };
                                    cx.notify();
                                })
                            })
                    }),
            )
            .child(
                v_flex()
                    .w(px(150.))
                    .gap(px(4.))
                    .child(caption(curve.smoothing_label.clone(), p))
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .items_center()
                            .child(
                                div()
                                    .w(px(60.))
                                    .child(Input::new(&smoothing).disabled(!enabled)),
                            )
                            .child(caption(curve.smoothing_unit.clone(), p)),
                    ),
            )
            .child(
                v_flex()
                    .w(px(150.))
                    .gap(px(4.))
                    .child(caption(curve.step_label.clone(), p))
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .items_center()
                            .child(div().w(px(60.)).child(Input::new(&step).disabled(!enabled)))
                            .child(caption(curve.step_unit.clone(), p)),
                    ),
            );
        let mut section = v_flex()
            .w_full()
            .gap(px(8.))
            .px(px(18.))
            .py(px(12.))
            .border_t_1()
            .border_color(p.border)
            .child(fields);
        if input_open {
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
            section = section.child(self.picker_list("curve-input", rows, pick, p, window, cx));
        }
        section = section
            .child(self.curve_plot(&curve.plot, p, window, cx))
            .child(
                h_flex()
                    .gap(px(18.))
                    .text_size(px(13.))
                    .child(curve.readout.input.clone())
                    .child(curve.readout.smoothing.clone())
                    .child(curve.readout.curve.clone())
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .child(curve.readout.duty.clone()),
                    ),
            );

        // The points as number inputs: the plot and the inputs edit the same data.
        let mut points = v_flex().gap(px(4.));
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
            let selected = curve.plot.selected == index;
            points = points.child(
                h_flex()
                    .gap(px(8.))
                    .items_center()
                    .when(selected, |row| row.bg(p.accent_tint))
                    .rounded(px(6.))
                    .child(
                        div()
                            .w(px(24.))
                            .text_color(p.fg_dim)
                            .child(format!("{}", index + 1)),
                    )
                    .child(div().w(px(80.)).child(Input::new(&xs).disabled(!enabled)))
                    .child(caption(locale.tr("channels.point_input"), p))
                    .child(div().w(px(80.)).child(Input::new(&ys).disabled(!enabled)))
                    .child(caption(locale.tr("channels.point_duty"), p)),
            );
        }
        section = section.child(points).child(
            h_flex()
                .gap(px(8.))
                .child({
                    let view = view.clone();
                    Button::new("point-add")
                        .small()
                        .label(locale.tr("channels.add_point"))
                        .disabled(!enabled || !curve.can_add)
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                let state = this.state(cx);
                                if let Some(command) = this.channels.add_point(&state, None) {
                                    this.send(command);
                                }
                                cx.notify();
                            })
                        })
                })
                .child({
                    let view = view.clone();
                    let selected = curve.plot.selected;
                    Button::new("point-remove")
                        .small()
                        .label(locale.tr("channels.remove_point"))
                        .disabled(!enabled || !curve.can_remove)
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                let state = this.state(cx);
                                if let Some(command) = this.channels.remove_point(&state, selected)
                                {
                                    this.send(command);
                                }
                                cx.notify();
                            })
                        })
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
        let (line, grid, fg, dash, floor_color) = (p.series[0], p.border, p.fg, p.fg_dim, p.fg_dim);
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
                    x += 5.0;
                }
                for step in 0..=5 {
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
                    hatch(window, zone, 8.0, floor_color.opacity(0.35));
                    polyline(
                        window,
                        &[
                            at(bounds.origin, 0.0, px_y(floor)),
                            at(bounds.origin, w, px_y(floor)),
                        ],
                        1.5,
                        dash,
                        Some((6.0, 4.0)),
                    );
                }
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
                        dash,
                        Some((4.0, 3.0)),
                    );
                    polyline(
                        window,
                        &[
                            at(bounds.origin, 0.0, px_y(y)),
                            at(bounds.origin, px_x(x), px_y(y)),
                        ],
                        1.0,
                        dash,
                        Some((4.0, 3.0)),
                    );
                    crate::widgets::paint::disc(
                        window,
                        at(bounds.origin, px_x(x), px_y(y)),
                        6.0,
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
            .rounded(px(6.))
            .bg(p.view)
            .text_size(px(11.))
            .text_color(p.fg_dim);
        for step in 0..=5 {
            let duty = 100 - step * 20;
            frame = frame.child(
                div()
                    .absolute()
                    .left(px(4.))
                    .top(px(
                        TOP - 7.0 + (PLOT_HEIGHT - TOP - BOTTOM) * step as f32 / 5.0
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
                    .bottom(px(BOTTOM - 16.0))
                    .child(
                        div()
                            .absolute()
                            .left(relative(frac_x(x)))
                            .ml(px(-8.))
                            .child(format!("{x:.0}")),
                    ),
            );
            x += 10.0;
        }
        frame = frame
            .child(
                div()
                    .absolute()
                    .left(px(4.))
                    .top(px(0.))
                    .child(plot.y_title.clone()),
            )
            .child(
                div()
                    .absolute()
                    .right(px(RIGHT))
                    .bottom(px(2.))
                    .child(plot.x_title.clone()),
            );
        if let Some(floor) = &plot.floor {
            frame = frame.child(
                div()
                    .absolute()
                    .left(px(LEFT + 6.0))
                    .bottom(px(BOTTOM + 4.0))
                    .child(floor.1.clone()),
            );
        }
        if let Some(operating) = &plot.operating {
            frame = frame.child(
                div()
                    .absolute()
                    .right(px(RIGHT + 6.0))
                    .top(px(TOP + 4.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.fg)
                    .child(operating.label.clone()),
            );
        }

        // Handles: a circle of radius 6 on each point.
        for (index, (x, y)) in plot.points.iter().enumerate() {
            let selected = plot.selected == index;
            let handle = div()
                .id(("curve-handle", index))
                .absolute()
                .left(relative(frac_x(*x)))
                .top(relative(frac_y(*y)))
                .ml(px(-6.))
                .mt(px(-6.))
                .size(px(12.))
                .rounded_full()
                .border_2()
                .border_color(p.series[0])
                .when(selected, |h| h.bg(p.series[0]))
                .when(!selected, |h| h.bg(p.card))
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
            area = area.child(handle);
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
