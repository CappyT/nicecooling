//! The window of the Sensors screen: the table by device, and the Derived sensor editor.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, Bounds, Context, FontWeight, Hsla, SharedString, Window, canvas, div, px};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::input::Input;
use gpui_component::switch::Switch;
use gpui_component::{Disableable as _, Selectable as _, Sizable as _, h_flex, v_flex};

use crate::command::Command;
use crate::icons::{NcIcon, icon, small_icon, text_icon};
use crate::model::Severity;
use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::sensors::{
    DerivedEditor, DerivedEditorModel, FUNCTIONS, SensorRow, SensorSection, SensorsModel,
    SensorsState,
};
use crate::screens::ui::{caption, card, page, popover, severity_icon};
use crate::tokens::Palette;
use crate::widgets::paint::{at, polyline};

/// A row of a sensor picker.
#[derive(Clone, Debug)]
pub struct PickRow {
    pub id: String,
    pub name: String,
    pub caption: String,
    pub value: String,
    pub selected: bool,
}

type Pick = Rc<dyn Fn(&mut ScreenView, String, &mut Window, &mut Context<ScreenView>)>;

impl ScreenView {
    pub(crate) fn render_sensors(
        &mut self,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = self.with_input(cx, |input| SensorsModel::build(input, &self.sensors));
        let locale = self.model.read(cx).locale;
        let search = self.input(
            InputKey::SensorSearch,
            &model.search,
            &locale.tr("sensors.search"),
            window,
            cx,
        );

        let view = cx.entity();
        let writes = model.writes_enabled;
        let tooltip = model.writes_tooltip.clone().unwrap_or_default();
        let header = h_flex()
            .w_full()
            .gap(px(12.))
            .items_center()
            .child(div().w(px(260.)).child(Input::new(&search)))
            .child(
                Switch::new("show-hidden")
                    .checked(model.show_hidden)
                    .label(locale.tr("sensors.show_hidden"))
                    .on_click({
                        let view = view.clone();
                        move |checked, _, cx| {
                            let checked = *checked;
                            view.update(cx, |this, cx| {
                                this.sensors.show_hidden = checked;
                                cx.notify();
                            })
                        }
                    }),
            )
            .child(div().flex_1())
            .child({
                let view = view.clone();
                Button::new("new-derived")
                    .label(locale.tr("sensors.new_derived"))
                    .disabled(!writes)
                    .tooltip(tooltip.clone())
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            this.sensors.editor = Some(DerivedEditor::new_sensor());
                            cx.notify();
                        })
                    })
            });

        let mut content = page().child(header);
        match model.state {
            SensorsState::Loading => {
                content = content.child(
                    card(p)
                        .p(px(18.))
                        .gap(px(12.))
                        .child(skeleton(p, 220.))
                        .child(skeleton(p, 520.))
                        .child(skeleton(p, 380.)),
                );
            }
            SensorsState::Empty => {
                content = content.child(card(p).p(px(18.)).child(model.empty_text.clone()));
            }
            SensorsState::Ready => {
                let mut table = card(p).child(self.sensor_columns(p, cx));
                for group in &model.groups {
                    table = table.child(self.sensor_group(group, p, window, cx));
                }
                content = content.child(table);
            }
        }

        let view = cx.entity();
        let mut root = div().id("sensors-page").relative().w_full().child(content);
        if let Some(focus) = self.sensors_focus.clone() {
            root = root.track_focus(&focus).on_key_down(move |event, _, cx| {
                if event.keystroke.key == "enter" {
                    view.update(cx, |this, cx| {
                        if let Some(sensor) = this.sensors.selected.clone() {
                            this.open_history_for(&sensor, cx);
                        }
                    })
                }
            });
        }
        if let Some(editor) = self.sensors.editor.clone() {
            root = root.child(self.derived_dialog(&editor, p, window, cx));
        }
        root.into_any_element()
    }

    fn sensor_columns(&self, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let locale = self.model.read(cx).locale;
        h_flex()
            .h(px(36.))
            .px(px(18.))
            .gap(px(12.))
            .items_center()
            .text_size(px(12.))
            .font_weight(FontWeight::BOLD)
            .text_color(p.fg_dim)
            .child(div().flex_1().child(locale.tr("sensors.columns.sensor")))
            .child(
                div()
                    .w(px(110.))
                    .text_right()
                    .child(locale.tr("sensors.columns.value")),
            )
            .child(
                div()
                    .w(px(140.))
                    .child(locale.tr("sensors.columns.last_10_minutes")),
            )
            .child(
                div()
                    .w(px(180.))
                    .child(locale.tr("sensors.columns.used_by")),
            )
            .child(div().w(px(170.)))
            .into_any_element()
    }

    fn sensor_group(
        &mut self,
        group: &SensorSection,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = group.id.clone();
        let view = cx.entity();
        let folded = group.folded;
        // One shared scale per unit inside a group.
        let mut scales: Vec<(String, f64, f64)> = Vec::new();
        let sparks: Vec<Vec<(f64, f64)>> = group
            .rows
            .iter()
            .map(|row| self.sparkline_points(&row.id, cx))
            .collect();
        for (row, points) in group.rows.iter().zip(&sparks) {
            for (_, v) in points {
                match scales.iter_mut().find(|s| s.0 == row.unit) {
                    Some(scale) => {
                        scale.1 = scale.1.min(*v);
                        scale.2 = scale.2.max(*v);
                    }
                    None => scales.push((row.unit.clone(), *v, *v)),
                }
            }
        }
        let mut column = v_flex().w_full().child(
            h_flex()
                .id(SharedString::from(format!("group-{id}")))
                .h(px(32.))
                .px(px(18.))
                .gap(px(8.))
                .items_center()
                .bg(p.hover)
                .border_t_1()
                .border_color(p.border)
                .cursor_pointer()
                .on_click({
                    let id = id.clone();
                    move |_, _, cx| {
                        let id = id.clone();
                        view.update(cx, |this, cx| {
                            this.sensors.toggle_fold(&id);
                            cx.notify();
                        })
                    }
                })
                .child(icon(
                    if folded { NcIcon::Right } else { NcIcon::Down },
                    p.fg,
                ))
                .child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .child(group.title.clone()),
                )
                .children(group.caption.clone().map(|text| caption(text, p))),
        );
        if !folded {
            for (row, points) in group.rows.iter().zip(sparks) {
                let scale = scales
                    .iter()
                    .find(|s| s.0 == row.unit)
                    .map_or((0.0, 1.0), |s| (s.1, s.2));
                column = column.child(self.sensor_row(row, points, scale, p, window, cx));
            }
        }
        column.into_any_element()
    }

    fn sensor_row(
        &mut self,
        row: &SensorRow,
        points: Vec<(f64, f64)>,
        scale: (f64, f64),
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.model.read(cx).locale;
        let view = cx.entity();
        let id = row.id.clone();
        let dim = row.dim;
        let ink = if dim { p.fg_dim } else { p.fg };
        let mut name = v_flex().flex_1().min_w_0().child(
            h_flex()
                .gap(px(6.))
                .items_center()
                .children(row.severity.map(|severity| severity_icon(severity, p)))
                .child(div().truncate().child(row.name.clone())),
        );
        if let Some(text) = &row.caption {
            name = name.child(caption(text.clone(), p));
        }
        if let Some(text) = &row.warning {
            name = name.child(
                h_flex()
                    .gap(px(4.))
                    .child(severity_icon(Severity::Warning, p))
                    .child(caption(text.clone(), p)),
            );
        }
        if let Some(text) = &row.feedback {
            name = name.child(caption(text.clone(), p).text_color(p.alarm));
        }
        let value = h_flex()
            .w(px(110.))
            .justify_end()
            .gap(px(4.))
            .children(row.no_value.then(|| small_icon(NcIcon::Off, p.offline)))
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .text_color(ink)
                    .child(row.value.clone()),
            );
        let color: Hsla = p.accent;
        let spark = canvas(
            |_, _, _| (),
            move |bounds: Bounds<gpui::Pixels>, _, window, _| {
                if points.len() < 2 {
                    return;
                }
                let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
                let (low, high) = scale;
                let span = (high - low).max(0.5);
                let t0 = points.first().map_or(0.0, |p| p.0);
                let t1 = points.last().map_or(1.0, |p| p.0).max(t0 + 1.0);
                let trace: Vec<_> = points
                    .iter()
                    .map(|(t, v)| {
                        at(
                            bounds.origin,
                            ((t - t0) / (t1 - t0)) as f32 * w,
                            h - (((v - low) / span) as f32 * (h - 4.0) + 2.0),
                        )
                    })
                    .collect();
                polyline(window, &trace, 1.5, color, None);
            },
        )
        .w(px(120.))
        .h(px(24.));

        let rename_open = self
            .sensors
            .rename
            .as_ref()
            .is_some_and(|(sensor, _)| sensor == &row.id);
        let mut buttons = h_flex().w(px(170.)).gap(px(2.)).justify_end();
        let enabled = row.rename_enabled;
        buttons = buttons.child({
            let view = view.clone();
            let (sensor, current) = (row.id.clone(), row.name.clone());
            Button::new(SharedString::from(format!("rename-{}", row.id)))
                .ghost()
                .small()
                .label(locale.tr("sensors.rename"))
                .disabled(!enabled)
                .on_click(move |_, _, cx| {
                    let (sensor, current) = (sensor.clone(), current.clone());
                    view.update(cx, |this, cx| {
                        this.sensors.open_rename(&sensor, &current);
                        cx.notify();
                    })
                })
        });
        if row.derived.is_some() {
            buttons = buttons.child({
                let view = view.clone();
                let sensor = row.id.clone();
                Button::new(SharedString::from(format!("edit-{}", row.id)))
                    .ghost()
                    .small()
                    .label(locale.tr("sensors.edit"))
                    .disabled(!enabled)
                    .on_click(move |_, _, cx| {
                        let sensor = sensor.clone();
                        view.update(cx, |this, cx| {
                            let state = this.state(cx);
                            this.sensors.editor = state
                                .snapshot
                                .sensors
                                .get(&sensor)
                                .and_then(DerivedEditor::edit);
                            cx.notify();
                        })
                    })
            });
            buttons = buttons.child({
                let view = view.clone();
                let sensor = row.id.clone();
                let mut delete = Button::new(SharedString::from(format!("delete-{}", row.id)))
                    .ghost()
                    .small()
                    .label(locale.tr("sensors.delete"))
                    .disabled(!enabled || row.delete_reason.is_some());
                if let Some(reason) = &row.delete_reason {
                    delete = delete.tooltip(reason.clone());
                }
                delete.on_click(move |_, _, cx| {
                    let sensor = sensor.clone();
                    view.update(cx, |this, _| {
                        this.send(Command::DeleteDerivedSensor { sensor })
                    })
                })
            });
        } else {
            let (label_key, disabled) = if row.hidden {
                ("sensors.show", !enabled)
            } else {
                ("sensors.hide", !row.hide_enabled)
            };
            buttons = buttons.child({
                let view = view.clone();
                let sensor = row.id.clone();
                let mut hide = Button::new(SharedString::from(format!("hide-{}", row.id)))
                    .ghost()
                    .small()
                    .label(locale.tr(label_key))
                    .disabled(disabled);
                if let Some(reason) = &row.hide_reason {
                    hide = hide.tooltip(reason.clone());
                }
                hide.on_click(move |_, _, cx| {
                    let sensor = sensor.clone();
                    view.update(cx, |this, cx| {
                        this.sensors.toggle_hidden(&sensor);
                        this.persist_hidden(cx);
                        cx.notify();
                    })
                })
            });
        }

        let mut line = h_flex()
            .id(SharedString::from(format!("sensor-{}", row.id)))
            .w_full()
            .min_h(px(48.))
            .px(px(18.))
            .py(px(4.))
            .gap(px(12.))
            .items_center()
            .border_t_1()
            .border_color(p.border)
            .text_color(ink)
            .when(row.selected, |line| line.bg(p.accent_tint))
            .on_click({
                let view = view.clone();
                let id = id.clone();
                move |_, window, cx| {
                    let id = id.clone();
                    view.update(cx, |this, cx| {
                        this.sensors.selected = Some(id);
                        if let Some(focus) = &this.sensors_focus {
                            window.focus(focus, cx);
                        }
                        cx.notify();
                    })
                }
            })
            .child(name)
            .child(value)
            .child(div().w(px(140.)).child(spark))
            .child(
                div()
                    .w(px(180.))
                    .truncate()
                    .text_color(p.fg_dim)
                    .child(row.used_by.clone()),
            )
            .child(buttons);
        if row.hidden {
            line = line.opacity(0.6);
        }
        let mut column = v_flex().w_full().child(line);
        if rename_open {
            column = column.child(self.rename_popover(
                InputKey::SensorRename,
                &row.name,
                locale.tr("sensors.rename_placeholder"),
                p,
                window,
                cx,
            ));
        }
        // A flow sensor opens a panel with the Calibration table editor.
        if row.selected && row.is_flow {
            match &row.flow_loop {
                Some(loop_id) => {
                    let loop_id = loop_id.clone();
                    let editor = self.with_input(cx, |input| {
                        input
                            .state
                            .snapshot
                            .loops
                            .iter()
                            .find(|info| info.id == loop_id)
                            .and_then(|info| {
                                crate::widgets::calibration_editor::CalibrationEditorModel::build(
                                    input,
                                    &self.loops.calibration,
                                    info,
                                )
                            })
                    });
                    if let Some(editor) = editor {
                        column = column.child(
                            div()
                                .px(px(18.))
                                .pb(px(12.))
                                .child(self.calibration_card(&editor, p, window, cx)),
                        );
                    }
                }
                None => {
                    column = column.child(
                        div()
                            .px(px(18.))
                            .pb(px(12.))
                            .child(caption(locale.tr("sensors.flow_no_loop"), p)),
                    );
                }
            }
        }
        column.into_any_element()
    }

    /// A popover with one input: rename. Enter saves.
    pub(crate) fn rename_popover(
        &mut self,
        key: InputKey,
        value: &str,
        placeholder: String,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = self.input(key, value, &placeholder, window, cx);
        popover(p)
            .w(px(280.))
            .p(px(6.))
            .child(Self::field_input(&input, None, false, p))
            .into_any_element()
    }

    /// The sparkline of the last 10 minutes: refreshed every 10 s, not at every frame.
    fn sparkline_points(&mut self, sensor: &str, cx: &mut Context<Self>) -> Vec<(f64, f64)> {
        let now = (crate::app::now_us() / 1_000_000) as i64;
        self.recent_points(sensor, now - 600, now, cx)
    }

    fn derived_dialog(
        &mut self,
        editor: &DerivedEditor,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let model = self.with_input(cx, |input| DerivedEditorModel::build(input, editor));
        let locale = self.model.read(cx).locale;
        let view = cx.entity();
        let name = self.input(
            InputKey::DerivedName,
            &model.name,
            &locale.tr("sensors.rename_placeholder"),
            window,
            cx,
        );

        let mut functions = h_flex().gap(px(6.));
        for function in FUNCTIONS {
            let view = view.clone();
            let active = model.function == function;
            functions = functions.child(
                Button::new(SharedString::from(format!("fn-{function}")))
                    .small()
                    .label(locale.tr(&format!("sensors.function.{function}")))
                    .selected(active)
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            if let Some(editor) = this.sensors.editor.as_mut() {
                                editor.set_function(function);
                            }
                            cx.notify();
                        })
                    }),
            );
        }

        let mut sources = v_flex().gap(px(6.));
        for (slot, source) in model.slots.iter().enumerate() {
            let picker_key = format!("derived-source-{slot}");
            let open = self.open_picker.as_deref() == Some(picker_key.as_str());
            let mut line = h_flex()
                .gap(px(8.))
                .items_center()
                .child(div().w(px(90.)).child(caption(source.label.clone(), p)));
            line = line.child({
                let view = view.clone();
                let key = picker_key.clone();
                Button::new(SharedString::from(format!("source-{slot}")))
                    .small()
                    .label(
                        source
                            .sensor_name
                            .clone()
                            .unwrap_or_else(|| locale.tr("sensors.editor.choose")),
                    )
                    .on_click(move |_, _, cx| {
                        let key = key.clone();
                        view.update(cx, |this, cx| {
                            this.open_picker = if this.open_picker.as_deref() == Some(key.as_str())
                            {
                                None
                            } else {
                                Some(key)
                            };
                            cx.notify();
                        })
                    })
            });
            if let Some(value) = &source.value {
                line = line.child(caption(value.clone(), p));
            }
            if source.removable {
                line = line.child({
                    let view = view.clone();
                    Button::new(SharedString::from(format!("source-remove-{slot}")))
                        .ghost()
                        .small()
                        .icon(text_icon(NcIcon::X))
                        .on_click(move |_, _, cx| {
                            view.update(cx, |this, cx| {
                                if let Some(editor) = this.sensors.editor.as_mut() {
                                    editor.remove_source(slot);
                                }
                                cx.notify();
                            })
                        })
                });
            }
            sources = sources.child(line);
            if open {
                let rows = self.picker_rows(Some(source.sensor.clone()), None, cx);
                let pick: Pick = Rc::new(move |this, sensor, _, cx| {
                    if let Some(editor) = this.sensors.editor.as_mut() {
                        editor.set_source(slot, Some(sensor));
                    }
                    this.open_picker = None;
                    cx.notify();
                });
                sources = sources.child(self.picker_list(&picker_key, rows, pick, p, window, cx));
            }
        }
        if model.can_add_source {
            sources = sources.child({
                let view = view.clone();
                Button::new("add-source")
                    .ghost()
                    .small()
                    .label(locale.tr("sensors.editor.add_source"))
                    .on_click(move |_, _, cx| {
                        view.update(cx, |this, cx| {
                            if let Some(editor) = this.sensors.editor.as_mut() {
                                editor.add_source();
                            }
                            cx.notify();
                        })
                    })
            });
        }

        let save_command = DerivedEditorModel::save_command(editor);
        let dialog = v_flex()
            .w(px(480.))
            .p(px(18.))
            .gap(px(12.))
            .rounded(px(15.))
            .bg(p.card)
            .border_1()
            .border_color(p.border)
            .child(
                div()
                    .text_size(px(18.))
                    .font_weight(FontWeight::BOLD)
                    .child(model.title.clone()),
            )
            .child(
                v_flex()
                    .gap(px(4.))
                    .child(caption(locale.tr("sensors.editor.name"), p))
                    .child(Input::new(&name).disabled(!model.name_editable)),
            )
            .child(
                v_flex()
                    .gap(px(4.))
                    .child(caption(locale.tr("sensors.editor.function"), p))
                    .child(functions)
                    .child(caption(model.function_help.clone(), p)),
            )
            .child(
                v_flex()
                    .gap(px(4.))
                    .child(caption(locale.tr("sensors.editor.sources"), p))
                    .child(sources),
            )
            .children(model.unit_mismatch.clone().map(|text| {
                h_flex()
                    .gap(px(6.))
                    .child(severity_icon(Severity::Warning, p))
                    .child(text)
            }))
            .child(
                div()
                    .font_weight(FontWeight::BOLD)
                    .child(model.live_result.clone()),
            )
            .children(
                model
                    .feedback
                    .clone()
                    .map(|text| caption(text, p).text_color(p.alarm)),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap(px(8.))
                    .child({
                        let view = view.clone();
                        Button::new("derived-cancel")
                            .label(locale.tr("sensors.editor.cancel"))
                            .on_click(move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.sensors.editor = None;
                                    this.open_picker = None;
                                    cx.notify();
                                })
                            })
                    })
                    .child({
                        let view = view.clone();
                        let mut save = Button::new("derived-save")
                            .primary()
                            .label(locale.tr("sensors.editor.save"))
                            .disabled(!model.can_save);
                        if let Some(reason) = &model.save_reason {
                            save = save.tooltip(reason.clone());
                        }
                        save.on_click(move |_, _, cx| {
                            let command = save_command.clone();
                            view.update(cx, |this, cx| {
                                if let Some(command) = command {
                                    this.send(command);
                                    this.sensors.editor = None;
                                    this.open_picker = None;
                                    cx.notify();
                                }
                            })
                        })
                    }),
            );
        div()
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .justify_center()
            .pt(px(60.))
            .bg(gpui::black().opacity(0.4))
            .child(div().child(dialog))
            .into_any_element()
    }

    /// The rows of a sensor picker, with the live value of each sensor. `unit` limits the
    /// list to one unit.
    pub(crate) fn picker_rows(
        &self,
        selected: Option<Option<String>>,
        unit: Option<&str>,
        cx: &Context<Self>,
    ) -> Vec<PickRow> {
        let selected = selected.flatten();
        self.with_input(cx, |input| {
            let sample = input.state.sensors.as_ref();
            input
                .state
                .snapshot
                .sensors
                .values()
                .filter(|sensor| unit.is_none_or(|unit| sensor.unit == unit))
                .map(|sensor| PickRow {
                    id: sensor.id.clone(),
                    name: sensor.name.clone(),
                    caption: if sensor.driver.is_empty() {
                        sensor.origin.clone()
                    } else {
                        sensor.driver.clone()
                    },
                    value: crate::screens::format::optional_value(
                        input.locale,
                        sample.and_then(|s| s.sensor_value(&sensor.id)),
                        &sensor.unit,
                    ),
                    selected: selected.as_deref() == Some(sensor.id.as_str()),
                })
                .collect()
        })
    }

    /// A list of sensors with a search input: the sensor picker of `widgets.md`, drawn under
    /// the control that opened it.
    pub(crate) fn picker_list(
        &mut self,
        key: &str,
        rows: Vec<PickRow>,
        pick: Pick,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let locale = self.model.read(cx).locale;
        let search = self.input(
            InputKey::PickerSearch(key.to_owned()),
            "",
            &locale.tr("sensors.search"),
            window,
            cx,
        );
        let query = search.read(cx).value().to_lowercase();
        let view = cx.entity();
        let mut list = v_flex()
            .id(SharedString::from(format!("picker-{key}")))
            .max_h(px(260.))
            .overflow_y_scroll();
        for (index, row) in rows
            .into_iter()
            .filter(|row| {
                query.is_empty()
                    || row.name.to_lowercase().contains(&query)
                    || row.caption.to_lowercase().contains(&query)
            })
            .enumerate()
        {
            let pick = pick.clone();
            let view = view.clone();
            let id = row.id.clone();
            list = list.child(
                h_flex()
                    .id(SharedString::from(format!("pick-{key}-{index}")))
                    .h(px(34.))
                    .px(px(12.))
                    .gap(px(8.))
                    .items_center()
                    .cursor_pointer()
                    .hover(|row| row.bg(p.hover))
                    .when(row.selected, |line| line.bg(p.accent_tint))
                    .on_click(move |_, window, cx| {
                        let (id, pick) = (id.clone(), pick.clone());
                        view.update(cx, |this, cx| pick(this, id, window, cx))
                    })
                    .child(div().flex_1().truncate().child(row.name))
                    .child(caption(row.caption, p))
                    .child(
                        div()
                            .w(px(70.))
                            .text_right()
                            .font_weight(FontWeight::BOLD)
                            .child(row.value),
                    ),
            );
        }
        v_flex()
            .w(px(360.))
            .rounded(px(12.))
            .bg(p.card)
            .border_1()
            .border_color(p.border)
            .child(div().p(px(6.)).child(Input::new(&search)))
            .child(list)
            .into_any_element()
    }
}
