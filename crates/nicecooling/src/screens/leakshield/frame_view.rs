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

    pub(crate) fn render_leakshield(
        &mut self,
        id: &str,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let pending = !self.leakshield.settings.pending.is_empty();
        let frame = self.with_input(cx, |input| Frame::build(input, id, pending));
        self.record_traces(id, cx);
        let view = cx.entity();
        let tab = self.leakshield.tab;

        let mut strip = h_flex().gap(px(4.)).px(px(24.)).pt(px(12.));
        for (entry, label) in &frame.tabs {
            let entry = *entry;
            let view = view.clone();
            let selected = entry == tab;
            strip = strip.child(
                div()
                    .id(SharedString::from(format!("ls-tab-{entry:?}")))
                    .h(px(32.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .when(selected, |tab| {
                        tab.bg(p.accent_tint)
                            .text_color(p.accent)
                            .font_weight(FontWeight::BOLD)
                    })
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.leakshield.tab = entry;
                            cx.notify();
                        });
                    })
                    .child(label.clone()),
            );
        }

        let header = v_flex()
            .px(px(24.))
            .pt(px(12.))
            .child(
                div()
                    .text_size(px(20.))
                    .font_weight(FontWeight::BOLD)
                    .child(frame.title.clone()),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(p.fg_dim)
                    .child(frame.subtitle.clone()),
            );

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
        let mut root = v_flex().w_full().child(header).child(strip);
        if let Some(text) = &frame.offline {
            root = root.child(
                div()
                    .px(px(24.))
                    .pt(px(12.))
                    .child(card(p).child(notice_row(text.clone(), true, p))),
            );
        }
        root.child(content).into_any_element()
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
        } else if let Some(event) = name.strip_prefix("alarm-delay:")
            && commits
        {
            if let Some(seconds) = parse_number(value).filter(|n| *n >= 0.0) {
                let seconds = seconds.round() as u32;
                let command = self.with_input(cx, |input| {
                    let (response, delay) = input
                        .state
                        .snapshot
                        .responses
                        .iter()
                        .find(|rule| rule.event == event)
                        .map_or(
                            (crate::screens::alarms::default_response(event), 30),
                            |rule| (rule.response, rule.delay_s),
                        );
                    (delay != seconds).then(|| crate::command::Command::SetAlarmResponse {
                        event: event.to_owned(),
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
        if let Some(field) = name.strip_prefix("threshold:") {
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
