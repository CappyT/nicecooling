//! The frame of the Leakshield page: the header, the tab strip and the tab that shows.

use gpui::prelude::*;
use gpui::{AnyElement, Context, FontWeight, SharedString, Window, div, px};
use gpui_component::{h_flex, v_flex};

use crate::screen_view::ScreenView;
use crate::screens::leakshield::{Frame, Tab};
use crate::screens::ui::{card, notice_row};
use crate::tokens::Palette;

impl ScreenView {
    /// Show a tab of the Leakshield page.
    pub fn show_leakshield_tab(&mut self, id: &str, tab: Tab, cx: &mut Context<Self>) {
        self.leakshield.tab = tab;
        self.go(crate::shell::Page::Leakshield(id.to_owned()), cx);
    }

    /// What the user changed on the Leakshield page: a test opens a dialog or a step with it.
    pub fn leakshield_ui(&mut self) -> &mut crate::screens::leakshield::LeakshieldUi {
        &mut self.leakshield
    }

    /// The identifier of the Leakshield that the page shows.
    pub(crate) fn current_leakshield(&self, cx: &gpui::App) -> String {
        match &self.model.read(cx).page {
            crate::shell::Page::Leakshield(id) => id.clone(),
            _ => String::new(),
        }
    }

    /// The tab strip of the Leakshield page and the tab that shows. The strip does not scroll
    /// with the tab. The header bar of the shell has the title and the serial.
    pub(crate) fn render_leakshield(
        &mut self,
        id: &str,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (AnyElement, AnyElement) {
        let pending = !self.leakshield.settings.pending.is_empty();
        let frame = self.with_input(cx, |input| Frame::build(input, id, pending));
        self.record_traces(id, cx);
        let view = cx.entity();
        let tab = self.leakshield.tab;

        let mut strip = h_flex()
            .flex_none()
            .gap(px(3.))
            .px(px(18.))
            .overflow_hidden()
            .debug_selector(|| "leakshield-tabs".to_owned())
            .bg(p.headerbar)
            .border_b_1()
            .border_color(p.border);
        for (entry, label) in &frame.tabs {
            let entry = *entry;
            let view = view.clone();
            let selected = entry == tab;
            strip = strip.child(
                div()
                    .id(SharedString::from(format!("ls-tab-{entry:?}")))
                    .h(px(38.))
                    .flex_none()
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .font_weight(FontWeight::SEMIBOLD)
                    .border_b_2()
                    .map(|tab| {
                        if selected {
                            tab.text_color(p.fg).border_color(p.accent_bg)
                        } else {
                            tab.text_color(p.fg_dim)
                                .border_color(gpui::transparent_black())
                                .hover(|tab| tab.text_color(p.fg))
                        }
                    })
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.open_leakshield_tab(entry, cx);
                            cx.notify();
                        });
                    })
                    .child(label.clone()),
            );
        }

        let content = match tab {
            Tab::Overview => self.render_ls_overview(id, p, window, cx),
            Tab::Status => self.render_ls_status(id, p, window, cx),
            Tab::System => self.render_ls_system(id, p, window, cx),
            Tab::Settings => self.render_ls_settings(id, p, window, cx),
            Tab::Display => self.render_ls_display(id, p, window, cx),
            Tab::Lighting => self.render_ls_lighting(id, p, window, cx),
            Tab::Fields => self.render_ls_fields(id, p, window, cx),
            Tab::Sensors => self.render_ls_sensors(id, p, window, cx),
            Tab::Alarms => self.render_ls_alarms(id, p, window, cx),
        };
        let mut root = v_flex().w_full();
        if let Some(text) = &frame.offline {
            root = root.child(
                div()
                    .px(px(24.))
                    .pt(px(12.))
                    .child(card(p).child(notice_row(text.clone(), true, p))),
            );
        }
        (
            strip.into_any_element(),
            root.child(content).into_any_element(),
        )
    }

    /// Show a tab. The Display tab reads the image of its selected page, when the app has none
    /// for it yet.
    pub(crate) fn open_leakshield_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.leakshield.tab = tab;
        if tab != Tab::Display {
            return;
        }
        let id = self.current_leakshield(cx);
        let page = self.leakshield.display.page as u8;
        let read = self
            .model
            .read(cx)
            .state
            .display_previews
            .contains_key(&(id.clone(), page));
        if !read {
            self.read_display_preview(&id, cx);
        }
    }

    /// Keep the last 10 minutes of the live values for the charts.
    fn record_traces(&mut self, id: &str, cx: &mut Context<Self>) {
        let state = self.state(cx);
        let Some(sample) = state.sensors.as_ref() else {
            return;
        };
        let status = self.with_input(cx, |input| {
            let device = crate::screens::leakshield::Device::new(input, id);
            let limits = crate::screens::leakshield::overview::limits_of(&device);
            let pressure = device.sensor(input, "pressure");
            crate::screens::leakshield::overview::status_key(&device, pressure, limits)
        });
        self.leakshield
            .traces
            .entry(id.to_owned())
            .or_default()
            .record(id, sample, status);
    }

    /// Text input of the Leakshield page: `changes` while the user types, `commits` on Enter
    /// or when the input loses focus.
    pub(crate) fn on_ls_input(
        &mut self,
        name: &str,
        value: &str,
        changes: bool,
        commits: bool,
        cx: &mut Context<Self>,
    ) {
        if name == "lighting-first" || name == "lighting-count" {
            if commits && let Some(number) = parse_number(value) {
                let id = self.current_leakshield(cx);
                let state = self.state(cx);
                let table = state
                    .snapshot
                    .leakshield_data
                    .get(&id)
                    .map(crate::screens::leakshield::lighting::Table::read)
                    .unwrap_or_default();
                if let Some(index) = self.leakshield.lighting.selected.iter().next().copied()
                    && let Some(c) = table.controllers.get(index)
                {
                    let (first, count) = if name == "lighting-first" {
                        (number, c.count)
                    } else {
                        (c.first, number)
                    };
                    let mut ui = std::mem::take(&mut self.leakshield.lighting);
                    let command =
                        self.with_input(cx, |input| ui.set_range(input, &id, first, count));
                    self.leakshield.lighting = ui;
                    if let Some(command) = command {
                        self.send(command);
                    }
                }
            }
        } else if let Some(color) = name.strip_prefix("lighting-hue:") {
            if commits && let (Some(hue), Ok(color)) = (parse_number(value), color.parse::<usize>())
            {
                let id = self.current_leakshield(cx);
                let mut ui = std::mem::take(&mut self.leakshield.lighting);
                let command = self.with_input(cx, |input| ui.set_hue(input, &id, color, hue));
                self.leakshield.lighting = ui;
                if let Some(command) = command {
                    self.send(command);
                }
            }
        } else if name == "display-interval" {
            if commits && let Some(seconds) = parse_number(value) {
                let id = self.current_leakshield(cx);
                let command = self.with_input(cx, |input| {
                    crate::screens::leakshield::display::set_interval(input, &id, seconds)
                });
                if let Some(command) = command {
                    self.send(command);
                }
            }
        } else if let Some(key) = name.strip_prefix("settings:") {
            if commits {
                let id = self.current_leakshield(cx);
                self.settings_edit(&id, key, value, cx);
            }
        } else if name == "reset-word" && changes {
            self.leakshield.system.reset_word = value.to_owned();
        } else if name == "display-own-text" {
            // "Save" writes the text: a commit of the input does not.
            if changes {
                self.leakshield.display.type_own_text(value);
            }
        } else if let Some(rest) = name
            .strip_prefix("display-min:")
            .map(|rest| (rest, false))
            .or_else(|| name.strip_prefix("display-max:").map(|rest| (rest, true)))
            && commits
        {
            let (rest, maximum) = rest;
            let slot = rest.rsplit(':').next().and_then(|slot| slot.parse().ok());
            if let (Some(slot), Some(number)) = (slot, parse_number(value)) {
                let id = self.current_leakshield(cx);
                let command = self.with_input(cx, |input| {
                    self.leakshield
                        .display
                        .set_range(input, &id, slot, maximum, number)
                });
                if let Some(command) = command {
                    self.send(command);
                }
            }
        } else if let Some(rest) = name
            .strip_prefix("software-scale:")
            .map(|rest| (rest, false))
            .or_else(|| {
                name.strip_prefix("software-offset:")
                    .map(|rest| (rest, true))
            })
            && commits
        {
            let (slot, offset) = rest;
            if let (Ok(slot), Some(number)) = (slot.parse::<u16>(), parse_number(value)) {
                let id = self.current_leakshield(cx);
                let state = self.state(cx);
                let command = crate::screens::leakshield::sensors::set_scale(
                    &state, &id, slot, offset, number,
                );
                if let Some(command) = command {
                    self.send(command);
                }
            }
        } else if name == "test-pressure" && changes {
            self.leakshield.overview.test_pressure = Some(value.to_owned());
        } else if name == "fields-search" && changes {
            self.leakshield.fields.search = value.to_owned();
        } else if let Some(field) = name.strip_prefix("threshold:")
            && commits
        {
            if let Some(number) = parse_number(value) {
                self.send_threshold(field, number, cx);
            }
        } else if let Some(field) = name.strip_prefix("limit:")
            && commits
        {
            if let Some(number) = parse_number(value) {
                let id = self.current_leakshield(cx);
                let command = self.with_input(cx, |input| {
                    self.leakshield.alarms.set_limit(input, &id, field, number)
                });
                if let Some(command) = command {
                    self.send(command);
                }
            }
        } else if let Some(event) = name.strip_prefix("alarm-delay:")
            && commits
        {
            if let Some(seconds) = parse_number(value).filter(|n| *n >= 0.0) {
                let seconds = seconds.round() as u32;
                let id = self.current_leakshield(cx);
                let command = self.with_input(cx, |input| {
                    let (response, delay, _) =
                        crate::screens::leakshield::alarms::current_response(input, &id, event);
                    (delay != seconds).then(|| crate::command::Command::SetAlarmResponse {
                        event: event.to_owned(),
                        source: id.clone(),
                        response,
                        delay_s: seconds,
                    })
                });
                if let Some(command) = command {
                    self.send(command);
                }
            }
        } else if let Some(sensor) = name.strip_prefix("sensor-name:")
            && commits
        {
            let state = self.state(cx);
            let current = state.snapshot.sensors.get(sensor).map(|s| s.name.clone());
            if current.as_deref() != Some(value) {
                self.send(crate::command::Command::SetSensorName {
                    sensor: sensor.to_owned(),
                    name: value.to_owned(),
                });
            }
        }
    }

    pub(crate) fn on_ls_slider(&mut self, name: &str, value: f64, cx: &mut Context<Self>) {
        if name == "display-threshold" {
            self.leakshield.display.set_threshold(value);
        } else if let Some(field) = name.strip_prefix("threshold:") {
            self.send_threshold(field, value.round(), cx);
        } else if let Some(which) = name.strip_prefix("light-param:") {
            let id = self.current_leakshield(cx);
            let which = which.parse::<usize>().unwrap_or(0);
            let mut ui = std::mem::take(&mut self.leakshield.lighting);
            let command =
                self.with_input(cx, |input| ui.set_param(input, &id, which, value.round()));
            self.leakshield.lighting = ui;
            if let Some(command) = command {
                self.send(command);
            }
        }
    }

    fn send_threshold(&mut self, field: &str, value: f64, cx: &mut Context<Self>) {
        let id = self.current_leakshield(cx);
        let command = self.with_input(cx, |input| {
            self.leakshield
                .alarms
                .set_threshold(input, &id, field, value)
        });
        if let Some(command) = command {
            self.send(command);
        }
    }
}

/// A number as the user types it: a comma or a point.
fn parse_number(text: &str) -> Option<f64> {
    text.trim().replace(',', ".").parse().ok()
}
