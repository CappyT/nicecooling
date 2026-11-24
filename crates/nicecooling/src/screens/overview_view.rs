//! The window of the Overview screen.

use gpui::prelude::*;
use gpui::{
    AnyElement, Bounds, Context, FontWeight, SharedString, Window, canvas, div, px, relative,
};
use gpui_component::button::Button;
use gpui_component::{h_flex, v_flex};

use crate::icons::{NcIcon, icon};
use crate::screen_view::{ScreenView, skeleton};
use crate::screens::format::clock_minutes;
use crate::screens::overview::{
    AlarmRow, KeyTemperature, LoopCard, OverviewModel, OverviewState, PressureCard, RowTone,
    TagTone,
};
use crate::screens::ui::{
    TagKind, caption, card, card_row, icon_tag, page, severity_icon, tag, title_header,
};
use crate::shell::Page;
use crate::tokens::Palette;
use crate::widgets::loop_diagram;
use crate::widgets::paint::{at, disc, polygon, polyline};

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
        let mut alarms_header = title_header(locale.tr("overview.alarms_title"));
        if let Some((count, tone)) = model.alarm_count {
            let kind = match tone {
                RowTone::Alarm => TagKind::Alarm,
                RowTone::Warning => TagKind::Warning,
            };
            alarms_header = alarms_header.child(tag(count.to_string(), kind, p));
        }
        let alarms = card(p)
            .flex_1()
            .child(alarms_header)
            .children(if model.alarms.is_empty() {
                vec![
                    card_row(p)
                        .child(icon(NcIcon::Check, p.success))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_color(p.fg_dim)
                                .child(model.no_alarms.clone()),
                        )
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
                Self::link_row(
                    "open-leakshield",
                    locale.tr("overview.open_leakshield"),
                    move |_, cx| {
                        let id = id.clone();
                        view.update(cx, |this, cx| this.go(Page::Leakshield(id), cx))
                    },
                    p,
                )
            }));
        let temperatures = card(p)
            .flex_1()
            .child(title_header(locale.tr("overview.temperatures_title")))
            .children(model.temperatures.iter().map(|t| temperature_row(t, p, cx)));
        let profile_tag = model.profile.tag.clone().map(|text| {
            let kind = if model.profile.tag_accent {
                TagKind::Accent
            } else {
                TagKind::Neutral
            };
            tag(text, kind, p)
        });
        let profile = card(p)
            .flex_1()
            .child(title_header(locale.tr("overview.profile_title")))
            .child(
                card_row(p).child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            h_flex()
                                .gap(px(6.))
                                .child(
                                    div()
                                        .min_w_0()
                                        .truncate()
                                        .font_weight(FontWeight::BOLD)
                                        .when(model.profile.none, |name| name.text_color(p.fg_dim))
                                        .child(model.profile.name.clone()),
                                )
                                .children(profile_tag),
                        )
                        .children(model.profile.detail.clone().map(|text| caption(text, p))),
                ),
            )
            .child({
                let view = cx.entity();
                Self::link_row(
                    "open-profiles",
                    locale.tr("overview.open_profiles"),
                    move |_, cx| view.update(cx, |this, cx| this.go(Page::Profiles, cx)),
                    p,
                )
            });
        // The three cards have the height of the tallest one.
        content
            .child(
                div()
                    .flex()
                    .w_full()
                    .gap(px(12.))
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
        // The header: the name, the Mode tag, the caption, and the link at the right.
        let mut top = title_header(card_model.name.clone());
        if let Some(mode) = &card_model.mode_tag {
            top = top.child(match mode.tone {
                // A Mode name is in capitals and bold.
                TagTone::Warning => {
                    tag(mode.text.clone(), TagKind::Warning, p).font_weight(FontWeight::BOLD)
                }
                TagTone::Neutral if mode.mode => {
                    tag(mode.text.clone(), TagKind::Neutral, p).font_weight(FontWeight::BOLD)
                }
                TagTone::Neutral => tag(mode.text.clone(), TagKind::Neutral, p),
            });
        }
        if let Some(text) = &card_model.not_live {
            top = top.child(icon_tag(NcIcon::Off, text.clone(), TagKind::Offline, p));
        }
        let text = card_model
            .last_values
            .clone()
            .unwrap_or_else(|| card_model.caption.clone());
        top = top.child(caption(text, p).flex_1().truncate()).child(
            Self::link_button(
                SharedString::from(format!("open-loop-{index}")),
                locale.tr("overview.open_loop"),
                p,
            )
            .on_click(move |_, _, cx| {
                let loop_id = loop_id.clone();
                view.update(cx, |this, cx| {
                    this.loops.select(&loop_id);
                    this.go(Page::Loops, cx);
                })
            }),
        );
        card(p)
            .child(top)
            .child(loop_diagram::element(
                &card_model.diagram,
                p,
                self.navigator(),
                cx,
            ))
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
                    .min_w_0()
                    .child(div().font_weight(FontWeight::BOLD).child(row.title.clone()))
                    .child(caption(row.detail.clone(), p)),
            )
            .into_any_element()
    }

    /// The pressure of the last 10 minutes: the series, the limit as a dashed line and a
    /// vertical line at the time of the Alarm.
    fn pressure_card(
        &mut self,
        card_model: &PressureCard,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // The plot: the axis is `AXIS` px under the top, and the values use the px above it.
        const PLOT: f32 = 116.0;
        const AXIS: f32 = 92.0;
        const TOP: f32 = 8.0;
        let now = (crate::app::now_us() / 1_000_000) as i64;
        let start = now - 600;
        let points = self.recent_points(&card_model.sensor, start, now, cx);
        let limit = card_model.limit.as_ref().map(|(value, _)| *value);
        let raised = card_model
            .raised
            .as_ref()
            .map(|(t, _)| (*t / 1_000_000) as i64)
            .filter(|t| (start..=now).contains(t));
        let (low, high) = points
            .iter()
            .map(|point| point.1)
            .chain(limit)
            .chain(card_model.target_value)
            .chain(card_model.minimum_value)
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), v| {
                (lo.min(v), hi.max(v))
            });
        let (low, high) = if low.is_finite() {
            let margin = ((high - low) * 0.1).max(5.0);
            (low - margin, high + margin)
        } else {
            (0.0, 1.0)
        };
        let y = move |v: f64| AXIS - ((v - low) / (high - low)) as f32 * (AXIS - TOP);
        let fraction = move |t: i64| ((t - start) as f32 / 600.0).clamp(0.0, 1.0);

        let (series, grid, axis, alarm) = (p.series[0], p.border, p.border_strong, p.alarm);
        let chart = canvas(
            |_, _, _| (),
            move |bounds: Bounds<gpui::Pixels>, _, window, _| {
                let w = f32::from(bounds.size.width);
                let o = bounds.origin;
                let across = |window: &mut Window, y: f32, color, dash| {
                    polyline(window, &[at(o, 0.0, y), at(o, w, y)], 1.0, color, dash)
                };
                for line in [20.0, 44.0, 68.0] {
                    across(window, line, grid, None);
                }
                across(window, AXIS, axis, None);
                if let Some(limit) = limit {
                    across(window, y(limit), alarm, Some((5.0, 4.0)));
                }
                if let Some(t) = raised {
                    let x = fraction(t) * w;
                    polyline(window, &[at(o, x, 4.0), at(o, x, AXIS)], 1.0, alarm, None);
                }
                let trace: Vec<_> = points
                    .iter()
                    .map(|(t, v)| at(o, fraction(*t as i64) * w, y(*v)))
                    .collect();
                if let (Some(first), Some(last)) = (trace.first(), trace.last()) {
                    let mut area = vec![gpui::point(first.x, o.y + px(AXIS))];
                    area.extend(trace.iter().copied());
                    area.push(gpui::point(last.x, o.y + px(AXIS)));
                    polygon(window, &area, series.opacity(0.12));
                    polyline(window, &trace, 2.0, series, None);
                    disc(window, *last, 4.0, alarm, None);
                }
            },
        )
        .absolute()
        .size_full();

        let label = |top: f32| div().absolute().top(px(top.max(0.0)));
        let mut plot = div()
            .relative()
            .w_full()
            .h(px(PLOT))
            .text_size(px(12.))
            .line_height(px(16.))
            .text_color(p.fg_dim)
            .whitespace_nowrap()
            .child(chart)
            .child(
                label(98.0)
                    .left_0()
                    .child(clock_minutes(start as u64 * 1_000_000)),
            )
            .child(
                label(98.0)
                    .right_0()
                    .child(clock_minutes(now as u64 * 1_000_000)),
            );
        // The label of a value is above its line, and the label of the limit is under it.
        if let (Some(target), Some(text)) = (card_model.target_value, &card_model.target) {
            plot = plot.child(label(y(target) - 18.0).left(px(4.)).child(text.clone()));
        }
        if let Some((value, text)) = &card_model.limit {
            let text = match (&card_model.minimum, card_model.minimum_value) {
                (Some(minimum), Some(low)) if low == *value => minimum,
                _ => text,
            };
            plot = plot.child(label(y(*value) + 2.0).left(px(4.)).child(text.clone()));
        }
        if let (Some(t), Some((_, text))) = (raised, &card_model.raised) {
            plot = plot.child(
                label(2.0)
                    .right(relative(1.0 - fraction(t)))
                    .mr(px(6.))
                    .child(text.clone()),
            );
        }
        let mut top = title_header(card_model.title.clone())
            .child(caption(card_model.subtitle.clone(), p).flex_1().truncate());
        if let Some(text) = &card_model.change {
            top = top.child(caption(text.clone(), p).flex_none());
        }
        card(p)
            .child(top)
            .child(div().px(px(18.)).pb(px(12.)).child(plot))
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

/// A row of 30 px: the name of the sensor and its value. A sensor without a value is dim.
fn temperature_row(row: &KeyTemperature, p: &Palette, cx: &mut Context<ScreenView>) -> AnyElement {
    let view = cx.entity();
    let ink = if row.value == "--" { p.fg_dim } else { p.fg };
    card_row(p)
        .id(SharedString::from(row.sensor.clone()))
        .min_h(px(30.))
        .py(px(3.))
        .text_color(ink)
        .cursor_pointer()
        .on_click(move |_, _, cx| view.update(cx, |this, cx| this.go(Page::Sensors, cx)))
        .child(div().flex_1().min_w_0().truncate().child(row.label.clone()))
        .child(
            div()
                .flex_none()
                .font_weight(FontWeight::BOLD)
                .child(row.value.clone()),
        )
        .into_any_element()
}
