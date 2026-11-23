//! The window of the Status tab: the tiles with their chart, the schematic and the events.

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, px};
use gpui_component::{h_flex, v_flex};

use crate::screen_view::{ScreenView, skeleton};
use crate::screens::leakshield::TabState;
use crate::screens::leakshield::charts::{bands, line_chart};
use crate::screens::leakshield::sensors_view::header_row;
use crate::screens::leakshield::status::{StatusModel, Tile, export_to};
use crate::screens::ui::{caption, card, card_header, card_row, page, severity_icon};
use crate::shell::Page;
use crate::tokens::Palette;

impl ScreenView {
    pub(crate) fn render_ls_status(
        &mut self,
        id: &str,
        p: &Palette,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = self.with_input(cx, |input| {
            StatusModel::build(input, &self.leakshield.status, id)
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

        // The tiles, and the chart of the selected one.
        let mut tiles = h_flex().gap(px(8.)).p(px(12.));
        for tile in &model.tiles {
            let kind = tile.tile;
            let view = view.clone();
            let mut body = v_flex()
                .id(SharedString::from(format!("ls-tile-{kind:?}")))
                .flex_1()
                .p(px(10.))
                .rounded(px(9.))
                .border_1()
                .border_color(p.border)
                .cursor_pointer()
                .when(tile.selected, |t| {
                    t.bg(p.accent_tint).border_color(p.accent)
                })
                .on_click(move |_, _, cx| {
                    view.update(cx, |this, cx| {
                        this.leakshield.status.selected = kind;
                        cx.notify();
                    });
                })
                .child(
                    h_flex()
                        .gap(px(6.))
                        .text_size(px(12.))
                        .text_color(p.fg_dim)
                        .when_some(tile.severity, |label, severity| {
                            label.child(severity_icon(severity, p))
                        })
                        .child(tile.label.clone()),
                )
                .child(
                    div()
                        .text_size(px(20.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(if dim { p.fg_dim } else { p.fg })
                        .child(tile.value.clone()),
                );
            if tile.selected {
                body = body.border_2();
            }
            tiles = tiles.child(body);
        }
        let traces = self.leakshield.traces.get(id);
        let chart: AnyElement = match model.selected {
            Tile::Status => {
                let items: Vec<(f64, gpui::Hsla)> = traces
                    .map(|t| {
                        t.status
                            .iter()
                            .map(|(time, key)| {
                                let color = match key.as_str() {
                                    "protecting" | "monitoring" => p.accent_tint,
                                    "building" => p.warning_tint,
                                    _ => p.selected,
                                };
                                (*time as f64, color)
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let end = traces
                    .and_then(|t| t.pressure.points.back().map(|(t, _)| *t as f64))
                    .or_else(|| items.last().map(|(t, _)| *t + 1.0))
                    .unwrap_or(0.0);
                bands(items, end, model.chart_height)
            }
            tile => {
                let points = traces
                    .map(|t| match tile {
                        Tile::Pressure => &t.pressure,
                        Tile::VolumeChange => &t.volume_change,
                        _ => &t.pressure_change,
                    })
                    .map(|trace| trace.points.iter().map(|(t, v)| (*t as f64, *v)).collect())
                    .unwrap_or_default();
                line_chart(points, Vec::new(), p.accent, None, model.chart_height)
            }
        };
        let operating = card(p)
            .child(card_header(model.operating_title.clone(), p))
            .child(tiles)
            .child(div().px(px(18.)).pb(px(12.)).child(chart))
            .child(card_row(p).child(caption(model.operating_caption.clone(), p)));
        content = content.child(operating);

        // The schematic.
        let schematic = &model.schematic;
        let mut parts = h_flex().gap(px(10.)).p(px(18.)).items_center();
        for part in &schematic.parts {
            parts = parts.child(
                div()
                    .px(px(10.))
                    .py(px(8.))
                    .rounded(px(8.))
                    .border_1()
                    .when(part.active, |b| b.bg(p.accent_tint).border_color(p.accent))
                    .when(!part.active, |b| b.bg(p.part).border_color(p.border))
                    .text_color(if schematic.dim { p.fg_dim } else { p.fg })
                    .text_size(px(12.))
                    .child(part.label.clone()),
            );
        }
        let mut bubbles = h_flex().gap(px(12.)).px(px(18.)).pb(px(12.));
        for bubble in &schematic.bubbles {
            let color = match bubble.severity {
                Some(crate::model::Severity::Alarm) => p.alarm,
                Some(crate::model::Severity::Warning) => p.warning,
                None => p.fg,
            };
            bubbles = bubbles.child(
                v_flex()
                    .gap(px(2.))
                    .p(px(8.))
                    .rounded(px(18.))
                    .border_1()
                    .border_color(if schematic.dim { p.offline } else { color })
                    .when(schematic.dim, |b| b.border_dashed())
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(p.fg_dim)
                            .child(bubble.tag),
                    )
                    .child(div().font_weight(FontWeight::BOLD).text_color(color).child(
                        if schematic.dim {
                            "--".to_owned()
                        } else {
                            bubble.value.clone()
                        },
                    ))
                    .child(caption(bubble.caption.clone(), p)),
            );
        }
        content = content.child(
            card(p)
                .child(card_header(schematic.title.clone(), p))
                .child(parts)
                .child(bubbles)
                .child(card_row(p).child(caption(schematic.caption.clone(), p))),
        );

        // The events.
        let export = {
            let (view, id) = (view.clone(), id.to_owned());
            Self::link_button("ls-status-export", model.events.export.clone()).on_click(
                move |_, _, cx| {
                    let (view, id) = (view.clone(), id.clone());
                    let (info, events, locale) = view.read_with(cx, |this, cx| {
                        let state = this.state(cx);
                        (
                            state
                                .snapshot
                                .leakshields
                                .iter()
                                .find(|d| d.id == id)
                                .cloned(),
                            state
                                .snapshot
                                .leakshield_data
                                .get(&id)
                                .map(|d| d.events.clone())
                                .unwrap_or_default(),
                            this.model.read(cx).locale,
                        )
                    });
                    let Some(info) = info else { return };
                    let path = cx
                        .prompt_for_new_path(&std::env::temp_dir(), Some("leakshield-events.txt"));
                    cx.spawn(async move |_| {
                        if let Ok(Ok(Some(path))) = path.await
                            && let Err(error) = export_to(&path, locale, &info, &events)
                        {
                            eprintln!("nicecooling: cannot save the event list: {error}");
                        }
                    })
                    .detach();
                },
            )
        };
        let mut events = card(p)
            .child(card_header(model.events.title.clone(), p).child(export))
            .child(header_row(&model.events.columns, &[170., 260., 0.], p));
        if let Some(text) = &model.events.empty {
            events = events.child(card_row(p).child(text.clone()));
        }
        for row in &model.events.rows {
            let view = view.clone();
            let opens = row.opens_alarms;
            events = events.child(
                card_row(p)
                    .id(SharedString::from(format!("ls-event-{}", row.time)))
                    .text_color(if dim { p.fg_dim } else { p.fg })
                    .when(opens, |line| line.cursor_pointer())
                    .on_click(move |_, _, cx| {
                        if opens {
                            view.update(cx, |this, cx| this.go(Page::Alarms, cx));
                        }
                    })
                    .child(div().w(px(170.)).child(row.time.clone()))
                    .child(
                        h_flex()
                            .w(px(260.))
                            .gap(px(6.))
                            .when_some(row.severity, |label, severity| {
                                label.child(severity_icon(severity, p))
                            })
                            .child(row.title.clone()),
                    )
                    .child(div().flex_1().child(row.details.clone())),
            );
        }
        content = content.child(events);
        content.into_any_element()
    }
}
