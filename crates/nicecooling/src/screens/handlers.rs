//! What the text inputs and the sliders of the screens do.

use gpui::{Context, Entity, Window};
use gpui_component::input::{InputEvent, InputState};

use crate::command::Command;
use crate::screen_view::{InputKey, ScreenView, with_screen_input};

/// A number as the user types it: a comma or a point.
fn parse(text: &str) -> Option<f64> {
    text.trim().replace(',', ".").parse().ok()
}

impl ScreenView {
    fn number_of(&self, key: &InputKey, cx: &Context<Self>) -> Option<f64> {
        parse(self.inputs.get(key)?.read(cx).value().as_ref())
    }

    pub(crate) fn on_input(
        &mut self,
        key: &InputKey,
        state: &Entity<InputState>,
        event: &InputEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let value = state.read(cx).value().to_string();
        let enter = matches!(event, InputEvent::PressEnter { .. });
        let commits = enter || matches!(event, InputEvent::Blur);
        let changes = matches!(event, InputEvent::Change);
        match key {
            InputKey::SensorSearch if changes => self.sensors.search = value,
            InputKey::SensorRename => {
                if changes && let Some(rename) = self.sensors.rename.as_mut() {
                    rename.1 = value;
                } else if enter && let Some(command) = self.sensors.rename_command() {
                    self.send(command);
                }
            }
            InputKey::DerivedName if changes => {
                if let Some(editor) = self.sensors.editor.as_mut() {
                    editor.name = value;
                }
            }
            InputKey::ChannelRename => {
                if changes && let Some(rename) = self.channels.rename.as_mut() {
                    rename.1 = value;
                } else if enter && let Some(command) = self.channels.rename_command() {
                    self.send(command);
                }
            }
            InputKey::LoopRename => {
                if changes && let Some(rename) = self.loops.rename.as_mut() {
                    rename.1 = value;
                } else if enter && let Some(command) = self.loops.rename_command() {
                    self.send(command);
                }
            }
            InputKey::ProfileRename => {
                if changes && let Some(rename) = self.profiles.rename.as_mut() {
                    rename.1 = value;
                } else if enter && let Some(command) = self.profiles.rename_command() {
                    self.send(command);
                }
            }
            InputKey::RuleDelay | InputKey::ProfileDwell if commits => {
                if let Some(number) = parse(&value).filter(|n| *n >= 0.0) {
                    let state = self.state(cx);
                    let number = number.round() as u32;
                    let command = if *key == InputKey::RuleDelay {
                        self.profiles.set_timing(&state, Some(number), None)
                    } else {
                        self.profiles.set_timing(&state, None, Some(number))
                    };
                    let unchanged = matches!(
                        &command,
                        Command::SetRuleTiming { rule_delay_s, dwell_s }
                            if *rule_delay_s == state.snapshot.rule_delay_s
                                && *dwell_s == state.snapshot.dwell_s
                    );
                    if !unchanged {
                        self.send(command);
                    }
                }
            }
            InputKey::RuleField(field) if changes => {
                if let Some(rule) = self.profiles.rule_draft() {
                    crate::screens::profiles::set_rule_field(rule, field, &value);
                }
            }
            InputKey::ResponseDelay(event) if commits => {
                if let Some(seconds) = parse(&value).filter(|n| *n >= 0.0) {
                    let state = self.state(cx);
                    let (event, source) = event.split_once('@').unwrap_or((event, ""));
                    let current = state
                        .snapshot
                        .responses
                        .iter()
                        .find(|r| r.event == event && r.source == source);
                    if let Some(rule) = current
                        && rule.delay_s != seconds.round() as u32
                    {
                        self.send(Command::SetAlarmResponse {
                            event: event.to_owned(),
                            source: source.to_owned(),
                            response: rule.response,
                            delay_s: seconds.round() as u32,
                        });
                    }
                }
            }
            InputKey::CurvePoint(channel, index, which) if commits => {
                let state = self.state(cx);
                let command = if *index == usize::MAX {
                    let Some(number) = parse(&value) else { return };
                    let current = self.channels.current(&state).map(|c| c.curve.clone());
                    let draft = self.channels.draft.as_ref();
                    let (smoothing, step) = draft.map_or_else(
                        || current.map_or((0.0, 0.0), |c| (c.smoothing_s, c.step_limit)),
                        |d| (d.smoothing_s, d.step_limit),
                    );
                    if *which == 0 && number != smoothing {
                        self.channels.set_smoothing(&state, number)
                    } else if *which == 1 && number != step {
                        self.channels.set_step_limit(&state, number)
                    } else {
                        None
                    }
                } else {
                    let x = self.number_of(&InputKey::CurvePoint(channel.clone(), *index, 0), cx);
                    let y = self.number_of(&InputKey::CurvePoint(channel.clone(), *index, 1), cx);
                    match (x, y) {
                        (Some(x), Some(y)) => self.channels.set_point(&state, *index, x, y),
                        _ => None,
                    }
                };
                if let Some(command) = command {
                    self.send(command);
                }
            }
            InputKey::CalibrationCell(loop_id, row, _) if commits => {
                let rpm = self.number_of(&InputKey::CalibrationCell(loop_id.clone(), *row, 0), cx);
                let flow = self.number_of(&InputKey::CalibrationCell(loop_id.clone(), *row, 1), cx);
                if let (Some(rpm), Some(flow)) = (rpm, flow) {
                    let model = self.model.clone();
                    let command = with_screen_input(&model, cx, |input| {
                        self.loops
                            .calibration
                            .set_row(input, loop_id, *row, rpm, flow)
                    });
                    if let Some(command) = command {
                        self.send(command);
                    }
                }
            }
            InputKey::Ls(name) => self.on_ls_input(name, &value, changes, commits, cx),
            _ => {}
        }
        cx.notify();
    }

    /// A slider reports the value when the user releases it.
    pub(crate) fn on_slider(&mut self, key: &str, value: f32, cx: &mut Context<Self>) {
        let duty = f64::from(value).round();
        if let Some(name) = key.strip_prefix("ls:") {
            self.on_ls_slider(name, f64::from(value), cx);
        } else if let Some(loop_id) = key.strip_prefix("floor:") {
            self.send(Command::SetPumpFloor {
                loop_id: loop_id.to_owned(),
                duty,
            });
        } else if key == "first-run-floor" {
            self.first_run.floor = Some(duty);
        } else if key.starts_with("fixed:") {
            let model = self.model.clone();
            let command = with_screen_input(&model, cx, |input| {
                self.channels.set_fixed_duty(input, duty)
            });
            if let Some(command) = command {
                self.send(command);
            }
        }
        cx.notify();
    }
}
