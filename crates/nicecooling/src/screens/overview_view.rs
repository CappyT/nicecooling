//! The window of the Overview screen.

use gpui::prelude::*;
use gpui::{AnyElement, Bounds, Context, FontWeight, SharedString, Window, canvas, div, px};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::{Sizable as _, h_flex, v_flex};

use crate::screen_view::{ScreenView, skeleton};
use crate::screens::overview::{
    AlarmRow, KeyTemperature, LoopCard, OverviewModel, OverviewState, PressureCard, RowTone,
    TagTone,
};
use crate::screens::ui::{TagKind, caption, card, card_header, card_row, page, severity_icon, tag};
use crate::shell::Page;
use crate::tokens::Palette;
use crate::widgets::loop_diagram;
use crate::widgets::paint::{at, polyline};

impl ScreenView {
    pub(crate) fn render_overview(
        &mut self,
        p: &Palette,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = self.with_input(cx, OverviewModel::build);
        let locale = self.model.read(cx).locale;
        let mut content = page();

        match model.state {
            OverviewState::Loading => {
                for index in 0..2usize {
                    content = content.child(
                        card(p)
                            .id(("loading", index))
                            .p(px(18.))
                            .gap(px(12.))
                            .child(skeleton(p, 160.))
                            .child(skeleton(p, 420.))
                            .child(skeleton(p, 300.)),
                    );
                }
                return content.into_any_element();
            }
            OverviewState::Empty => {
                if let Some(empty) = &model.empty {
                    let view = cx.entity();
                    let target = empty.open.clone();
                    content = content.child(
                        card(p)
                            .p(px(18.))
                            .gap(px(12.))
                            .child(
                                div()
                                    .font_weight(FontWeight::BOLD)
                                    .child(empty.title.clone()),
                            )
                            .child(empty.text.clone())
                            .child(
                                h_flex().child(
                                    Button::new("overview-setup")
                                        .label(empty.button.clone())
                                        .on_click(move |_, _, cx| {
                                            view.update(cx, |this, cx| this.go(target.clone(), cx))
                                        }),
                                ),
                            ),
                    );
                }
            }
            OverviewState::Ready => {
                for (index, card_model) in model.loops.iter().enumerate() {
                    content = content.child(self.loop_card(index, card_model, p, cx));
                }
            }
        }

        if let Some(pressure) = &model.pressure {
            content = content.child(self.pressure_card(pressure, p, cx));
        }

        let first_leakshield = self
            .state(cx)
            .snapshot
            .leakshields
            .first()
            .map(|device| device.id.clone());
        let alarms = card(p)
            .flex_1()
            .child(card_header(locale.tr("overview.alarms_title"), p))
            .children(if model.alarms.is_empty() {
                vec![
                    card_row(p)
                        .child(model.no_alarms.clone())
                        .into_any_element(),
                ]
            } else {
                model
                    .alarms
                    .iter()
                    .enumerate()
                    .map(|(index, row)| self.alarm_row(index, row, p, cx))
                    .collect()
            })
            .children(first_leakshield.map(|id| {
                let view = cx.entity();
                card_row(p).child(
                    Button::new("open-leakshield")
                        .ghost()
                        .small()
                        .label(locale.tr("overview.open_leakshield"))
                        .on_click(move |_, _, cx| {
                            let id = id.clone();
                            view.update(cx, |this, cx| this.go(Page::Leakshield(id), cx))
                        }),
                )
            }));
        let temperatures = card(p)
            .flex_1()
            .child(card_header(locale.tr("overview.temperatures_title"), p))
            .children(model.temperatures.iter().map(|t| temperature_row(t, p, cx)));
        let profile = card(p)
            .flex_1()
            .child(card_header(locale.tr("overview.profile_title"), p))
            .child(
                card_row(p)
                    .child(
                        div()
                            .flex_1()
                            .font_weight(FontWeight::BOLD)
                            .child(model.profile.name.clone()),
                    )
                    .children(
                        model
                            .profile
                            .tag
                            .clone()
                            .map(|text| tag(text, TagKind::Accent, p)),
                    ),
            )
            .children(
                model
                    .profile
                    .detail
                    .clone()
                    .map(|text| card_row(p).child(caption(text, p))),
            )
            .child({
                let view = cx.entity();
                card_row(p).child(
                    Button::new("open-profiles")
                        .ghost()
                        .small()
                        .label(locale.tr("overview.open_profiles"))
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| this.go(Page::Profiles, cx))
                        }),
                )
            });
        content
            .child(
                h_flex()
                    .w_full()
                    .gap(px(12.))
                    .items_start()
                    .child(alarms)
                    .child(temperatures)
                    .child(profile),
            )
            .into_any_element()
    }

    fn loop_card(
        &mut self,
        index: usize,
        card_model: &LoopCard,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.model.read(cx).locale;
        let loop_id = card_model.id.clone();
        let view = cx.entity();
        let mut header =
            card_header(card_model.name.clone(), p).child(caption(card_model.caption.clone(), p));
        if let Some(mode) = &card_model.mode_tag {
            header = header.child(tag(
                mode.text.clone(),
                if mode.tone == TagTone::Warning {
                    TagKind::Warning
                } else {
                    TagKind::Neutral
                },
                p,
            ));
        }
        if let Some(text) = &card_model.not_live {
            header = header.child(tag(text.clone(), TagKind::Offline, p));
        }
        if let Some(text) = &card_model.last_values {
            header = header.child(caption(text.clone(), p));
        }
        header = header.child(
            Button::new(("open-loop", index))
                .ghost()
                .small()
                .label(locale.tr("overview.open_loop"))
                .on_click(move |_, _, cx| {
                    let loop_id = loop_id.clone();
                    view.update(cx, |this, cx| {
                        this.loops.select(&loop_id);
                        this.go(Page::Loops, cx);
                    })
                }),
        );
        card(p)
            .child(header)
            .child(
                div()
                    .w_full()
                    .px(px(18.))
                    .py(px(6.))
                    .child(loop_diagram::element(
                        &card_model.diagram,
                        p,
                        self.navigator(),
                    )),
            )
            .into_any_element()
    }

    fn alarm_row(
        &mut self,
        index: usize,
        row: &AlarmRow,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let severity = match row.tone {
            RowTone::Alarm => crate::model::Severity::Alarm,
            RowTone::Warning => crate::model::Severity::Warning,
        };
        let opens = row.opens.clone();
        let view = cx.entity();
        card_row(p)
            .id(("alarm-row", index))
            .when(opens.is_some(), |row| row.cursor_pointer())
            .on_click(move |_, _, cx| {
                if let Some(page) = opens.clone() {
                    view.update(cx, |this, cx| this.go(page, cx));
                }
            })
            .child(severity_icon(severity, p))
            .child(
                v_flex()
                    .flex_1()
                    .child(div().font_weight(FontWeight::BOLD).child(row.title.clone()))
                    .child(caption(row.detail.clone(), p)),
            )
            .into_any_element()
    }

    fn pressure_card(
        &mut self,
        card_model: &PressureCard,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let now = (crate::app::now_us() / 1_000_000) as i64;
        let points = self.recent_points(&card_model.sensor, now - 600, now, cx);
        let limit = card_model.limit.as_ref().map(|(value, _)| *value);
        let raised = card_model
            .raised
            .as_ref()
            .map(|(t, _)| (*t / 1_000_000) as i64);
        let (line, dash, alarm) = (p.accent, p.fg_dim, p.alarm);
        let chart = canvas(
            |_, _, _| (),
            move |bounds: Bounds<gpui::Pixels>, _, window, _| {
                let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                let values = points.iter().map(|point| point.1).chain(limit);
                let (low, high) = values.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                    (lo.min(v), hi.max(v))
                });
                let (low, high) = if low.is_finite() {
                    (low - 10.0, high + 10.0)
                } else {
                    (0.0, 1.0)
                };
                let y = |v: f64| h - (((v - low) / (high - low)) as f32 * h);
                let x = |t: i64| ((t - (now - 600)) as f32 / 600.0 * w).clamp(0.0, w);
                let trace: Vec<_> = points
                    .iter()
                    .map(|(t, v)| at(bounds.origin, x(*t as i64), y(*v)))
                    .collect();
                polyline(window, &trace, 2.5, line, None);
                if let Some(limit) = limit {
                    polyline(
                        window,
                        &[
                            at(bounds.origin, 0.0, y(limit)),
                            at(bounds.origin, w, y(limit)),
                        ],
                        1.5,
                        dash,
                        Some((6.0, 4.0)),
                    );
                }
                if let Some(t) = raised {
                    polyline(
                        window,
                        &[at(bounds.origin, x(t), 0.0), at(bounds.origin, x(t), h)],
                        1.5,
                        alarm,
                        None,
                    );
                }
            },
        )
        .w_full()
        .h(px(116.));
        let mut notes = h_flex().gap(px(18.)).px(px(18.)).pb(px(12.));
        if let Some((_, label)) = &card_model.limit {
            notes = notes.child(caption(label.clone(), p));
        }
        for label in [&card_model.target, &card_model.minimum]
            .into_iter()
            .flatten()
        {
            notes = notes.child(caption(label.clone(), p));
        }
        if let Some((_, label)) = &card_model.raised {
            notes = notes.child(caption(label.clone(), p).text_color(p.alarm));
        }
        card(p)
            .child(
                card_header(card_model.title.clone(), p)
                    .child(caption(card_model.subtitle.clone(), p)),
            )
            .child(div().px(px(18.)).py(px(12.)).child(chart))
            .child(notes)
            .into_any_element()
    }

    /// The `(time, value)` points of a sensor from the history, for a small chart.
    pub(crate) fn recent_points(
        &mut self,
        sensor: &str,
        from: i64,
        to: i64,
        cx: &mut Context<Self>,
    ) -> Vec<(f64, f64)> {
        let Some(path) = self.model.read(cx).history_path.clone() else {
            return Vec::new();
        };
        if self.history_db.is_none() {
            self.history_db = crate::history::HistoryDb::open(&path).ok();
        }
        let Some(db) = &self.history_db else {
            return Vec::new();
        };
        crate::history::query::load_series(db, sensor, from, to, to)
            .map(|series| {
                series
                    .points()
                    .map(|point| (point.t as f64, point.mean))
                    .collect()
            })
            .unwrap_or_default()
    }
}

fn temperature_row(row: &KeyTemperature, p: &Palette, cx: &mut Context<ScreenView>) -> AnyElement {
    let view = cx.entity();
    card_row(p)
        .id(SharedString::from(row.sensor.clone()))
        .cursor_pointer()
        .on_click(move |_, _, cx| view.update(cx, |this, cx| this.go(Page::Sensors, cx)))
        .child(div().flex_1().child(row.label.clone()))
        .child(div().font_weight(FontWeight::BOLD).child(row.value.clone()))
        .into_any_element()
}
