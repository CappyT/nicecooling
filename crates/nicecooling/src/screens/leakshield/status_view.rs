//! The window of the Status tab: the tiles with their chart, the schematic and the events.

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, px};
use gpui_component::{h_flex, v_flex};

use crate::screen_view::ScreenView;
use crate::screens::format::clock_minutes;
use crate::screens::leakshield::charts::{Reference, TimeChart, bands, time_chart};
use crate::screens::leakshield::parts::{header, loading_card};
use crate::screens::leakshield::status::{StatusModel, Tile, export_to};
use crate::screens::leakshield::{TabState, schematic};
use crate::screens::ui::{
    caption, card, card_header, cell, page, severity_icon, table_header, table_row, wide_cell,
};
use crate::shell::Page;
use crate::tokens::Palette;

/// The width of the "Time" column of the events.
const TIME_WIDTH: f32 = 160.0;

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
                .child(loading_card(model.loading_text.clone(), p))
                .into_any_element();
        }
        let dim = model.state != TabState::Ready;

        // The tiles, and the chart of the selected one.
        let mut tiles = h_flex().w_full().gap(px(6.)).px(px(12.)).pb(px(6.));
        for tile in &model.tiles {
            let kind = tile.tile;
            let view = view.clone();
            let radio = div().size(px(16.)).flex_none().rounded_full().map(|radio| {
                if tile.selected {
                    radio.border(px(5.)).border_color(p.accent_bg)
                } else {
                    radio.border(px(1.5)).border_color(p.border_strong)
                }
            });
            tiles = tiles.child(
                h_flex()
                    .id(SharedString::from(format!("ls-tile-{kind:?}")))
                    .flex_1()
                    .min_w_0()
                    .gap(px(9.))
                    .px(px(9.))
                    .py(px(6.))
                    .rounded(px(9.))
                    .cursor_pointer()
                    .when(tile.selected, |t| t.bg(p.accent_tint))
                    .when(!tile.selected, |t| t.hover(|t| t.bg(p.hover)))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.leakshield.status.selected = kind;
                            cx.notify();
                        });
                    })
                    .child(radio)
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .truncate()
                                    .text_size(px(16.))
                                    .line_height(px(22.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(if dim { p.fg_dim } else { p.fg })
                                    .child(tile.value.clone()),
                            )
                            .child(
                                h_flex()
                                    .gap(px(4.))
                                    .when_some(tile.severity, |label, severity| {
                                        label.child(severity_icon(severity, p))
                                    })
                                    .child(
                                        caption(tile.label.clone(), p)
                                            .truncate()
                                            // Never `fg-dim` on a tint.
                                            .when(tile.selected, |c| c.text_color(p.fg)),
                                    ),
                            ),
                    ),
            );
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
                let points: Vec<(f64, f64)> = traces
                    .map(|t| match tile {
                        Tile::Pressure => &t.pressure,
                        Tile::VolumeChange => &t.volume_change,
                        _ => &t.pressure_change,
                    })
                    .map(|trace| trace.points.iter().map(|(t, v)| (*t as f64, *v)).collect())
                    .unwrap_or_default();
                let references = match (tile, model.target) {
                    (Tile::Pressure, Some(value)) => vec![Reference {
                        value,
                        color: p.accent,
                        dash: (5.0, 4.0),
                        label: model.target_label.clone(),
                    }],
                    _ => Vec::new(),
                };
                let x_labels = [points.first(), points.last()]
                    .into_iter()
                    .flatten()
                    .map(|(time, _)| (*time, clock_minutes(*time as u64)))
                    .collect();
                time_chart(
                    TimeChart {
                        runs: vec![points],
                        references,
                        x_labels,
                        height: model.chart_height,
                        ..Default::default()
                    },
                    p.series[0],
                    p,
                )
            }
        };
        let operating = card(p)
            .child(header(
                model.operating_title.clone(),
                model.operating_caption.clone(),
                p,
            ))
            .child(tiles)
            .child(div().px(px(18.)).pb(px(12.)).child(chart));
        content = content.child(operating);

        // The schematic.
        content = content.child(
            card(p)
                .child(header(
                    model.schematic.title.clone(),
                    model.schematic.caption.clone(),
                    p,
                ))
                .child(schematic::element(&model.schematic, p)),
        );

        // The events.
        let export = {
            let (view, id) = (view.clone(), id.to_owned());
            Self::small_button("ls-status-export", model.events.export.clone(), None).on_click(
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
            .child(
                table_header(&model.events.columns, &[Some(TIME_WIDTH), None, None], p)
                    .border_t_0(),
            );
        if let Some(text) = &model.events.empty {
            events = events.child(
                div()
                    .px(px(18.))
                    .py(px(12.))
                    .border_t_1()
                    .border_color(p.border)
                    .text_color(p.fg_dim)
                    .child(text.clone()),
            );
        }
        for (index, row) in model.events.rows.iter().enumerate() {
            let view = view.clone();
            let opens = row.opens_alarms;
            events = events.child(
                table_row(p)
                    .id(SharedString::from(format!("ls-event-{index}")))
                    .text_color(if dim { p.fg_dim } else { p.fg })
                    .when(opens, |line| {
                        line.cursor_pointer().hover(|line| line.bg(p.hover))
                    })
                    .on_click(move |_, _, cx| {
                        if opens {
                            view.update(cx, |this, cx| this.go(Page::Alarms, cx));
                        }
                    })
                    .child(
                        cell(TIME_WIDTH)
                            .font_family("monospace")
                            .child(row.time.clone()),
                    )
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(6.))
                            .when_some(row.severity, |label, severity| {
                                label.child(severity_icon(severity, p))
                            })
                            .child(div().min_w_0().truncate().child(row.title.clone())),
                    )
                    .child(wide_cell().text_color(p.fg_dim).child(row.details.clone())),
            );
        }
        content = content.child(events);
        content.into_any_element()
    }
}
