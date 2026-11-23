//! The window of the Overview tab: the gauge, the status, the Mode and the Actions.

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, px};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::progress::Progress;
use gpui_component::{Disableable as _, Sizable as _, h_flex, v_flex};

use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::controls::act;
use crate::screens::leakshield::TabState;
use crate::screens::leakshield::charts::{gauge, line_chart};
use crate::screens::leakshield::overview::{
    ActionView, Confirm, GaugeState, ModeCard, OverviewModel, TestSource,
};
use crate::screens::ui::{
    caption, card, card_header, card_row, modal, notice_row, page, severity_icon, tag, write_line,
};
use crate::tokens::Palette;

impl ScreenView {
    pub(crate) fn render_ls_overview(
        &mut self,
        id: &str,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = self.with_input(cx, |input| {
            OverviewModel::build(input, &self.leakshield.overview, id)
        });
        let view = cx.entity();
        let mut content = page();
        if model.state == TabState::Loading {
            return content
                .child(
                    card(p)
                        .p(px(18.))
                        .gap(px(12.))
                        .child(div().child(model.loading_text.clone()))
                        .child(skeleton(p, 320.)),
                )
                .into_any_element();
        }
        let dim = model.state != TabState::Ready;
        let pressure = &model.pressure;
        let value_color = if pressure.alarm {
            p.alarm
        } else if dim {
            p.fg_dim
        } else {
            p.fg
        };
        let pressure_card = card(p)
            .child(card_header(pressure.title.clone(), p).child(tag(
                pressure.tag.clone(),
                pressure.tag_kind,
                p,
            )))
            .child(
                v_flex()
                    .p(px(18.))
                    .gap(px(8.))
                    .child(
                        div()
                            .text_size(px(30.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(value_color)
                            .child(pressure.value_text.clone()),
                    )
                    .child(gauge(
                        pressure
                            .value
                            .filter(|_| pressure.gauge != GaugeState::Offline),
                        pressure.limits,
                        pressure.alarm,
                        p,
                    ))
                    .child(caption(pressure.tooltip.clone(), p)),
            );
        let mut status_card = card(p).child(card_header(model.status.title.clone(), p).child(tag(
            model.status.tag.clone(),
            model.status.tag_kind,
            p,
        )));
        for row in &model.status.rows {
            status_card = status_card.child(
                card_row(p)
                    .text_color(if dim { p.fg_dim } else { p.fg })
                    .child(
                        h_flex()
                            .flex_1()
                            .gap(px(6.))
                            .when_some(row.severity, |label, severity| {
                                label.child(severity_icon(severity, p))
                            })
                            .child(row.label.clone()),
                    )
                    .child(div().font_weight(FontWeight::BOLD).child(row.value.clone())),
            );
        }
        content = content.child(
            h_flex()
                .w_full()
                .gap(px(12.))
                .items_start()
                .child(div().flex_1().child(pressure_card))
                .child(div().flex_1().child(status_card)),
        );
        if let Some(text) = &model.mode.release_banner {
            content = content.child(card(p).child(notice_row(text.clone(), true, p)));
        }
        content = content.child(self.mode_card(id, &model.mode, p, cx));

        content = content.child(match &model.action_view {
            Some(action) => self.action_view(id, action, p, window, cx),
            None => self.charts_row(id, &model, p, cx),
        });

        let mut root = div().relative().w_full().child(content);
        if let Some(dialog) = &model.dialog {
            let (cancel, confirm) = (view.clone(), view.clone());
            let id = id.to_owned();
            let mut buttons = vec![
                Button::new("ls-dialog-cancel")
                    .small()
                    .label(dialog.cancel.clone())
                    .on_click(act(&cancel, |this, _, _| {
                        this.leakshield.overview.cancel_dialog()
                    }))
                    .into_any_element(),
            ];
            if let Some(label) = &dialog.confirm {
                buttons.push(
                    Button::new("ls-dialog-confirm")
                        .small()
                        .label(label.clone())
                        .on_click(act(&confirm, move |this, _, _| {
                            if let Some(command) = this.leakshield.overview.confirm(&id) {
                                this.send(command);
                            }
                        }))
                        .into_any_element(),
                );
            }
            let steps = dialog
                .steps
                .iter()
                .map(|step| div().child(step.clone()).into_any_element())
                .collect();
            root = root.child(modal(
                dialog.title.clone(),
                dialog.text.clone(),
                steps,
                buttons,
                p,
            ));
        }
        root.into_any_element()
    }

    fn mode_card(
        &mut self,
        id: &str,
        mode: &ModeCard,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let mut buttons = h_flex().gap(px(6.));
        for button in &mode.buttons {
            let (device, name) = (id.to_owned(), button.mode.clone());
            let label = if button.padlock {
                format!("\u{1F512} {}", button.label)
            } else {
                button.label.clone()
            };
            let mut ui_button = Button::new(SharedString::from(format!("ls-mode-{}", button.mode)))
                .label(label)
                .disabled(!button.enabled)
                .on_click(act(&view, move |this, _, cx| {
                    let state = this.state(cx);
                    if let Some(command) = this
                        .leakshield
                        .overview
                        .request_mode(&state, &device, &name)
                    {
                        this.send(command);
                    }
                }));
            if button.selected {
                ui_button = ui_button.primary();
            }
            if let Some(tooltip) = &mode.tooltip {
                ui_button = ui_button.tooltip(tooltip.clone());
            }
            buttons = buttons.child(ui_button);
        }
        let mut actions = h_flex().gap(px(6.));
        if let Some(cancel) = &mode.cancel {
            let device = id.to_owned();
            actions = actions.child(
                Button::new("ls-action-cancel")
                    .label(cancel.clone())
                    .on_click(act(&view, move |this, _, _| {
                        this.send(crate::command::Command::CancelLeakshieldAction {
                            leakshield: device.clone(),
                        });
                    })),
            );
        } else {
            for action in &mode.actions {
                let (device, name) = (id.to_owned(), action.action.clone());
                let mut button =
                    Button::new(SharedString::from(format!("ls-action-{}", action.action)))
                        .label(action.label.clone())
                        .disabled(!action.enabled)
                        .on_click(act(&view, move |this, _, _| {
                            if let Some(command) =
                                this.leakshield.overview.request_action(&device, &name)
                            {
                                this.send(command);
                            }
                        }));
                if let Some(tooltip) = &mode.tooltip {
                    button = button.tooltip(tooltip.clone());
                }
                actions = actions.child(button);
            }
        }
        let mut card = card(p)
            .child(card_header(mode.title.clone(), p))
            .child(
                card_row(p)
                    .child(buttons)
                    .child(div().flex_1())
                    .child(actions),
            )
            .child(card_row(p).child(caption(mode.caption.clone(), p)))
            .child(card_row(p).child(caption(mode.lock_hint.clone(), p)))
            .child(card_row(p).child(caption(mode.action_hint.clone(), p)));
        if let Some((text, link)) = &mode.read_only {
            card = card.child(
                notice_row(text.clone(), false, p).child(
                    Self::link_button("ls-open-fields-mode", link.clone())
                        .on_click(act(&view, |this, _, _| {
                            this.leakshield.open_fields(Some("default_mode"))
                        })),
                ),
            );
        }
        if let Some(text) = &mode.denied {
            card = card.child(card_row(p).text_color(p.alarm).child(text.clone()));
        } else if let Some(line) = &mode.write_line {
            card = card.child(card_row(p).child(write_line(line, p)));
        }
        if let Some(line) = &mode.action_line {
            card = card.child(card_row(p).child(write_line(line, p)));
        }
        card = card.child(
            card_row(p)
                .child(div().flex_1().child(mode.refill.text.clone()))
                .child(
                    Self::link_button("ls-refill-steps", mode.refill.link.clone()).on_click(act(
                        &view,
                        |this, _, _| {
                            this.leakshield.overview.confirm = Some(Confirm::RefillSteps);
                        },
                    )),
                ),
        );
        card.into_any_element()
    }

    fn charts_row(
        &mut self,
        id: &str,
        model: &OverviewModel,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let points: Vec<(f64, f64)> = self
            .leakshield
            .traces
            .get(id)
            .map(|t| {
                t.pressure
                    .points
                    .iter()
                    .map(|(t, v)| (*t as f64, *v))
                    .collect()
            })
            .unwrap_or_default();
        let mut lines = Vec::new();
        if let Some(limits) = model.pressure.limits {
            lines.push((limits.target, p.accent));
            lines.push((limits.min, p.fg_dim));
            lines.push((limits.max, p.fg_dim));
        }
        let mut ranges = h_flex().gap(px(4.));
        for (range, label, selected) in &model.charts.ranges {
            let range = *range;
            let mut button = Button::new(SharedString::from(format!("ls-range-{range:?}")))
                .small()
                .label(label.clone())
                .on_click(act(&view, move |this, _, _| {
                    this.leakshield.overview.range = range;
                }));
            if *selected {
                button = button.primary();
            }
            ranges = ranges.child(button);
        }
        let pressure = card(p)
            .child(card_header(model.charts.pressure_title.clone(), p).child(ranges))
            .child(div().p(px(18.)).child(line_chart(
                points,
                lines,
                p.accent,
                Some(p.accent_tint),
                116.,
            )));
        let runtime = card(p)
            .child(card_header(model.charts.runtime_title.clone(), p))
            .child(
                v_flex()
                    .p(px(18.))
                    .gap(px(6.))
                    .child(caption(model.charts.runtime_empty.clone(), p))
                    .child(caption(model.charts.runtime_limit.clone(), p)),
            );
        h_flex()
            .w_full()
            .gap(px(12.))
            .items_start()
            .child(div().flex_1().child(pressure))
            .child(div().flex_1().child(runtime))
            .into_any_element()
    }

    fn action_view(
        &mut self,
        id: &str,
        action: &ActionView,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        match action {
            ActionView::TestSetup {
                title,
                pressure,
                text,
                error,
                start,
                cancel,
                ..
            } => {
                let input = self.input(
                    InputKey::Ls("test-pressure".into()),
                    pressure,
                    "",
                    window,
                    cx,
                );
                let device = id.to_owned();
                let mut body = v_flex()
                    .p(px(18.))
                    .gap(px(10.))
                    .child(
                        h_flex()
                            .gap(px(8.))
                            .items_center()
                            .child(div().w(px(100.)).child(Input::new(&input)))
                            .child(div().text_color(p.fg_dim).child("mbar")),
                    )
                    .child(caption(text.clone(), p));
                if let Some(error) = error {
                    body = body.child(div().text_color(p.alarm).child(error.clone()));
                }
                body = body.child(
                    h_flex()
                        .gap(px(8.))
                        .child(
                            Button::new("ls-test-start")
                                .primary()
                                .label(start.clone())
                                .disabled(error.is_some())
                                .on_click(act(&view, move |this, _, cx| {
                                    let mut ui = std::mem::take(&mut this.leakshield.overview);
                                    let command =
                                        this.with_input(cx, |input| ui.start_test(input, &device));
                                    this.leakshield.overview = ui;
                                    if let Some(command) = command {
                                        this.send(command);
                                    }
                                })),
                        )
                        .child(
                            Button::new("ls-test-cancel")
                                .label(cancel.clone())
                                .on_click(act(&view, |this, _, _| {
                                    this.leakshield.overview.cancel_test_setup()
                                })),
                        ),
                );
                card(p)
                    .child(card_header(title.clone(), p))
                    .child(body)
                    .into_any_element()
            }
            ActionView::TestRunning {
                source_label,
                sources,
                reference,
                reset_zoom,
                cancel,
                reference_value,
            } => {
                let traces = self.leakshield.traces.get(id);
                let source = sources
                    .iter()
                    .find(|(_, _, selected)| *selected)
                    .map_or(TestSource::Pressure, |(s, _, _)| *s);
                let points: Vec<(f64, f64)> = traces
                    .map(|t| match source {
                        TestSource::Pressure => t
                            .pressure
                            .points
                            .iter()
                            .map(|(t, v)| (*t as f64, *v))
                            .collect(),
                        TestSource::Difference => t
                            .pressure
                            .points
                            .iter()
                            .map(|(t, v)| (*t as f64, v - reference_value.unwrap_or(*v)))
                            .collect(),
                        TestSource::Change => t
                            .pressure_change
                            .points
                            .iter()
                            .map(|(t, v)| (*t as f64, *v))
                            .collect(),
                    })
                    .unwrap_or_default();
                let mut choices = h_flex()
                    .gap(px(4.))
                    .items_center()
                    .child(source_label.clone());
                for (kind, label, selected) in sources {
                    let kind = *kind;
                    let mut button = Button::new(SharedString::from(format!("ls-source-{kind:?}")))
                        .small()
                        .label(label.clone())
                        .on_click(act(&view, move |this, _, _| {
                            this.leakshield.overview.source = kind;
                        }));
                    if *selected {
                        button = button.primary();
                    }
                    choices = choices.child(button);
                }
                let device = id.to_owned();
                let current = self
                    .leakshield
                    .traces
                    .get(id)
                    .and_then(|t| t.pressure.points.back().map(|(_, v)| *v));
                card(p)
                    .child(
                        card_row(p)
                            .child(choices)
                            .child(div().flex_1())
                            .child(
                                Button::new("ls-test-reference")
                                    .small()
                                    .label(reference.clone())
                                    .on_click(act(&view, move |this, _, _| {
                                        this.leakshield.overview.reference = current;
                                    })),
                            )
                            .child(
                                Button::new("ls-test-zoom")
                                    .small()
                                    .label(reset_zoom.clone()),
                            )
                            .child({
                                let device = device.clone();
                                Button::new("ls-test-cancel-run")
                                    .small()
                                    .label(cancel.clone())
                                    .on_click(act(&view, move |this, _, _| {
                                        this.send(
                                            crate::command::Command::CancelLeakshieldAction {
                                                leakshield: device.clone(),
                                            },
                                        );
                                    }))
                            }),
                    )
                    .child(div().p(px(18.)).child(line_chart(
                        points,
                        Vec::new(),
                        p.accent,
                        None,
                        140.,
                    )))
                    .into_any_element()
            }
            ActionView::MeasureFillLevel {
                title,
                step,
                banner,
                result,
            } => card(p)
                .child(card_header(title.clone(), p))
                .child(notice_row(banner.clone(), true, p))
                .child(
                    v_flex()
                        .p(px(18.))
                        .gap(px(8.))
                        .child(step.clone())
                        .child(Progress::new("ls-measure-progress").value(50.))
                        .when_some(result.clone(), |body, text| {
                            body.child(div().font_weight(FontWeight::BOLD).child(text))
                        }),
                )
                .into_any_element(),
            ActionView::Deaerate {
                title,
                current,
                overall,
            } => card(p)
                .child(card_header(title.clone(), p))
                .child(
                    v_flex()
                        .p(px(18.))
                        .gap(px(8.))
                        .child(current.clone())
                        .child(Progress::new("ls-deaerate-current").value(50.))
                        .child(overall.clone())
                        .child(Progress::new("ls-deaerate-overall").value(25.)),
                )
                .into_any_element(),
        }
    }
}
