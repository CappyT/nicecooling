//! The window of the First run screen: the stepper, the card of the step and the footer.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, Window, div, px};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::slider::Slider;
use gpui_component::spinner::Spinner;
use gpui_component::{Disableable as _, h_flex, v_flex};

use crate::screen_view::ScreenView;
use crate::screens::controls::{Pick, act};
use crate::screens::first_run::{
    BiosRow, BoardStep, ChannelRow, DoneStep, FirstRunModel, LeakshieldStep, LoopStep, Step,
};
use crate::screens::loops::PartRow;
use crate::screens::ui::{caption, card, card_header, card_row, notice_row, page};
use crate::shell::Page;
use crate::tokens::Palette;
use crate::widgets::loop_diagram;

impl ScreenView {
    pub(crate) fn render_first_run(
        &mut self,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.state(cx);
        self.first_run.sync(&state);
        let model = self.with_input(cx, |input| FirstRunModel::build(input, &self.first_run));
        let view = cx.entity();
        let locale = self.model.read(cx).locale;

        // The stepper.
        let mut stepper = h_flex().gap(px(8.)).items_center();
        for label in &model.steps {
            stepper = stepper.child(
                div()
                    .px(px(12.))
                    .py(px(4.))
                    .rounded_full()
                    .when(label.current, |chip| {
                        chip.bg(p.accent_tint).text_color(p.accent)
                    })
                    .when(label.done, |chip| chip.text_color(p.success))
                    .when(!label.current && !label.done, |chip| {
                        chip.text_color(p.fg_dim)
                    })
                    .font_weight(FontWeight::BOLD)
                    .child(label.label.clone()),
            );
        }

        let body: AnyElement = if let Some(offline) = &model.offline {
            card(p)
                .p(px(18.))
                .gap(px(12.))
                .child(offline.title.clone())
                .child(
                    h_flex().child(
                        Button::new("first-run-retry")
                            .label(offline.try_again.clone())
                            .on_click(act(&view, |this, _, cx| {
                                this.model.update(cx, |app, cx| {
                                    app.retry();
                                    cx.notify();
                                });
                            })),
                    ),
                )
                .into_any_element()
        } else {
            match model.step {
                Step::Board => self.board_card(&model.board, p),
                Step::Loop => self.first_run_loop_card(&model.loop_step, p, window, cx),
                Step::Bios => bios_card(&model, p),
                Step::Leakshield => model.leakshield.as_ref().map_or_else(
                    || div().into_any_element(),
                    |step| leakshield_card(step, p, &view),
                ),
                Step::Done => done_card(&model.done, p, &view),
            }
        };

        let footer = &model.footer;
        let mut buttons = h_flex().w_full().gap(px(8.)).child(
            Button::new("first-run-skip")
                .ghost()
                .label(footer.skip.clone())
                .on_click(act(&view, |this, _, cx| {
                    this.first_run.step = Step::Board;
                    this.go(Page::Overview, cx);
                })),
        );
        buttons = buttons.child(div().flex_1());
        if let Some(back) = &footer.back {
            buttons = buttons.child(Button::new("first-run-back").label(back.clone()).on_click(
                act(&view, |this, _, cx| {
                    let state = this.state(cx);
                    this.first_run.back(&state);
                }),
            ));
        }
        if let Some(next) = &footer.next {
            buttons = buttons.child(
                Button::new("first-run-next")
                    .primary()
                    .label(next.clone())
                    .disabled(!footer.next_enabled)
                    .on_click(act(&view, |this, _, cx| {
                        let model = this.model.clone();
                        let command = crate::screen_view::with_screen_input(&model, cx, |input| {
                            this.first_run.next(input)
                        });
                        if let Some(command) = command {
                            this.send(command);
                        }
                    })),
            );
        }
        let _ = locale;
        page()
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight::BOLD)
                    .child(model.title.clone()),
            )
            .child(stepper)
            .child(body)
            .child(buttons)
            .into_any_element()
    }

    fn board_card(&self, board: &BoardStep, p: &Palette) -> AnyElement {
        let mut card = card(p).child(card_header(board.name.clone(), p));
        if let Some(text) = &board.loading {
            return card
                .child(card_row(p).child(Spinner::new()).child(text.clone()))
                .into_any_element();
        }
        card = card.child(card_row(p).child(board.text.clone()));
        if let Some(text) = &board.warning {
            card = card.child(notice_row(text.clone(), true, p));
        }
        card = card.child(card_row(p).child(caption(board.counts.clone(), p)));
        card = card.child(channel_table(&board.columns, &board.channels, p));
        card.into_any_element()
    }

    fn first_run_loop_card(
        &mut self,
        step: &LoopStep,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.model.read(cx).locale;
        let enabled = step.enabled;
        let mut card = card(p).child(card_header(locale.tr("first_run.step.loop"), p));
        let pump_pick: Pick = Rc::new(|this, id, _, _| this.first_run.pump = Some(id));
        card = card.child(self.picker_row("first-pump", &step.pump, enabled, pump_pick, p, cx));
        let flow_pick: Pick = Rc::new(|this, id, _, _| this.first_run.flow = Some(id));
        card = card.child(self.picker_row("first-flow", &step.flow, enabled, flow_pick, p, cx));
        if let Some(preset) = &step.preset {
            let preset_pick: Pick = Rc::new(|this, id, _, _| this.first_run.preset = Some(id));
            let mut row = self.picker_row("first-preset", preset, enabled, preset_pick, p, cx);
            if let Some(text) = &step.live_flow {
                row = v_flex()
                    .w_full()
                    .child(row)
                    .child(card_row(p).child(caption(text.clone(), p)))
                    .into_any_element();
            }
            card = card.child(row);
        }
        let water_pick: Pick = Rc::new(|this, id, _, _| this.first_run.water = Some(id));
        card = card.child(self.picker_row("first-water", &step.water, enabled, water_pick, p, cx));
        let leakshield_pick: Pick = Rc::new(|this, id, _, _| this.first_run.leakshield = Some(id));
        card = card.child(self.picker_row(
            "first-leakshield",
            &step.leakshield,
            enabled,
            leakshield_pick,
            p,
            cx,
        ));
        if let Some(text) = &step.empty_table_note {
            card = card.child(card_row(p).child(caption(text.clone(), p)));
        }
        if let Some(text) = &step.no_leakshield {
            card = card.child(card_row(p).child(caption(text.clone(), p)));
        }
        let slider = self.slider("first-run-floor", step.floor as f32, window, cx);
        card = card.child(
            card_row(p)
                .child(
                    div()
                        .w(px(160.))
                        .font_weight(FontWeight::BOLD)
                        .child(step.floor_label.clone()),
                )
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
                        .child(step.floor_text.clone()),
                ),
        );
        card = card.child(div().p(px(18.)).child(loop_diagram::element(
            &step.diagram,
            p,
            self.navigator(),
            cx,
        )));
        card.into_any_element()
    }

    fn picker_row(
        &mut self,
        key: &str,
        row: &PartRow,
        enabled: bool,
        pick: Pick,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let select = self.select(
            key,
            &row.shown,
            row.choices
                .iter()
                .map(|c| {
                    let text = match &c.value {
                        Some(value) if !value.is_empty() => format!("{}  {value}", c.name),
                        _ => c.name.clone(),
                    };
                    (c.id.clone(), text, c.selected)
                })
                .collect(),
            enabled,
            "",
            pick,
            p,
            cx,
        );
        let mut column = v_flex().w_full().child(
            card_row(p)
                .child(
                    div()
                        .w(px(160.))
                        .font_weight(FontWeight::BOLD)
                        .child(row.label.clone()),
                )
                .child(select),
        );
        if let Some(text) = &row.warning {
            column = column.child(notice_row(text.clone(), true, p));
        }
        column.into_any_element()
    }
}

fn channel_table(columns: &[String], rows: &[ChannelRow], p: &Palette) -> AnyElement {
    let mut table = v_flex().w_full().child(
        card_row(p)
            .text_size(px(12.))
            .font_weight(FontWeight::BOLD)
            .text_color(p.fg_dim)
            .child(
                div()
                    .flex_1()
                    .child(columns.first().cloned().unwrap_or_default()),
            )
            .child(
                div()
                    .w(px(100.))
                    .child(columns.get(1).cloned().unwrap_or_default()),
            )
            .child(
                div()
                    .w(px(100.))
                    .child(columns.get(2).cloned().unwrap_or_default()),
            ),
    );
    for row in rows {
        table = table.child(
            card_row(p)
                .child(div().flex_1().child(row.name.clone()))
                .child(div().w(px(100.)).child(row.rpm.clone()))
                .child(div().w(px(100.)).child(row.duty.clone())),
        );
    }
    table.into_any_element()
}

fn bios_table(columns: &[String], rows: &[BiosRow], p: &Palette) -> AnyElement {
    let mut table = v_flex().w_full().child(
        card_row(p)
            .text_size(px(12.))
            .font_weight(FontWeight::BOLD)
            .text_color(p.fg_dim)
            .child(
                div()
                    .w(px(200.))
                    .child(columns.first().cloned().unwrap_or_default()),
            )
            .child(
                div()
                    .flex_1()
                    .child(columns.get(1).cloned().unwrap_or_default()),
            ),
    );
    for row in rows {
        table = table.child(
            card_row(p)
                .child(div().w(px(200.)).child(row.name.clone()))
                .child(div().flex_1().child(row.curve.clone())),
        );
    }
    table.into_any_element()
}

fn bios_card(model: &FirstRunModel, p: &Palette) -> AnyElement {
    let bios = &model.bios;
    let frame = card(p)
        .child(card_header(
            model
                .steps
                .iter()
                .find(|s| s.step == Step::Bios)
                .map_or_else(String::new, |s| s.label.clone()),
            p,
        ))
        .child(card_row(p).child(bios.text.clone()));
    let frame = match &bios.saved {
        Some(saved) => frame.child(card_row(p).child(caption(saved.clone(), p))),
        None => frame,
    };
    frame
        .child(bios_table(&bios.columns, &bios.rows, p))
        .child(card_row(p).child(caption(bios.no_curve.clone(), p)))
        .into_any_element()
}

fn leakshield_card(
    step: &LeakshieldStep,
    p: &Palette,
    view: &gpui::Entity<ScreenView>,
) -> AnyElement {
    let mut card = card(p)
        .child(card_header(step.serial.clone(), p))
        .child(card_row(p).child(step.status.clone()));
    if step.unknown {
        card = card.child(notice_row(step.status.clone(), true, p));
    }
    card = card.child(card_row(p).child(caption(step.text.clone(), p)));
    if let Some(label) = &step.open_fields {
        let device = step.id.clone();
        card = card.child(
            card_row(p).child(
                Button::new("first-run-fields")
                    .label(label.clone())
                    .on_click(act(view, move |this, _, cx| {
                        this.go(Page::Leakshield(device.clone()), cx);
                    })),
            ),
        );
    }
    card.into_any_element()
}

fn done_card(done: &DoneStep, p: &Palette, view: &gpui::Entity<ScreenView>) -> AnyElement {
    let mut card = card(p);
    for line in &done.summary {
        card = card.child(card_row(p).child(line.clone()));
    }
    card = card.child(card_row(p).child(done.text.clone()));
    let mut buttons = h_flex().gap(px(8.)).child(
        Button::new("first-run-overview")
            .primary()
            .label(done.overview.clone())
            .on_click(act(view, |this, _, cx| {
                this.first_run.step = Step::Board;
                this.go(Page::Overview, cx);
            })),
    );
    if let (Some(label), Some(page)) = (&done.wizard, &done.wizard_page) {
        let page = page.clone();
        buttons = buttons.child(
            Button::new("first-run-wizard")
                .label(label.clone())
                .on_click(act(view, move |this, _, cx| this.go(page.clone(), cx))),
        );
    }
    card.child(card_row(p).child(buttons)).into_any_element()
}
