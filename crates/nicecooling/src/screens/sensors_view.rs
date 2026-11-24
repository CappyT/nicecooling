//! The window of the Sensors screen: the table by device, and the Derived sensor editor.

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{AnyElement, Bounds, Context, FontWeight, Hsla, SharedString, Window, canvas, div, px};
use gpui_component::button::ButtonVariants as _;
use gpui_component::switch::Switch;
use gpui_component::{Disableable as _, h_flex, v_flex};

use crate::command::Command;
use crate::icons::{NcIcon, icon, small_icon};
use crate::model::Severity;
use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::controls::act;
use crate::screens::sensors::{
    DerivedEditor, DerivedEditorModel, FUNCTIONS, SensorRow, SensorSection, SensorsModel,
    SensorsState,
};
use crate::screens::ui::{
    caption, card, heading, page, popover, seg, seg_item, select_trigger, severity_icon, table_row,
    with_popover,
};
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

/// The widths of the cells after the name. The name takes the rest of the row.
const VALUE: f32 = 96.;
const CHART: f32 = 120.;
const USED_BY: f32 = 220.;
const BUTTONS: f32 = 90.;

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
            .child(div().w(px(260.)).flex_none().child(
                Self::field_input(&search, None, false, p).prefix(icon(NcIcon::Search, p.fg_dim)),
            ))
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
                Self::button("new-derived", locale.tr("sensors.new_derived"))
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
            .px(px(18.))
            .py(px(6.))
            .gap(px(12.))
            .items_center()
            .text_size(px(12.))
            .line_height(px(16.))
            .font_weight(FontWeight::BOLD)
            .text_color(p.fg_dim)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(locale.tr("sensors.columns.sensor")),
            )
            .child(
                div()
                    .w(px(VALUE))
                    .flex_none()
                    .text_right()
                    .child(locale.tr("sensors.columns.value")),
            )
            .child(
                div()
                    .w(px(CHART))
                    .flex_none()
                    .truncate()
                    .child(locale.tr("sensors.columns.last_10_minutes")),
            )
            .child(
                div()
                    .w(px(USED_BY))
                    .flex_none()
                    .truncate()
                    .child(locale.tr("sensors.columns.used_by")),
            )
            .child(div().w(px(BUTTONS)).flex_none())
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
                .min_h(px(30.))
                .px(px(18.))
                .py(px(3.))
                .gap(px(9.))
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
                    p.fg_dim,
                ))
                .child(heading(group.title.clone()).flex_none())
                .children(
                    group
                        .caption
                        .clone()
                        .map(|text| caption(text, p).flex_1().truncate()),
                ),
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
        let mut name = v_flex().w_full().min_w_0().child(
            h_flex()
                .gap(px(6.))
                .items_center()
                .children(row.severity.map(|severity| severity_icon(severity, p)))
                .child(div().min_w_0().truncate().child(row.name.clone())),
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
            .w(px(VALUE))
            .flex_none()
            .justify_end()
            .gap(px(4.))
            .children(row.no_value.then(|| small_icon(NcIcon::Off, p.offline)))
            .child(div().text_color(ink).child(row.value.clone()));
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
        .w(px(CHART))
        .h(px(24.));

        let rename_open = self
            .sensors
            .rename
            .as_ref()
            .is_some_and(|(sensor, _)| sensor == &row.id);
        // The buttons: an icon each, with the name of the command as tooltip.
        let mut buttons = h_flex()
            .w(px(BUTTONS))
            .flex_none()
            .gap(px(3.))
            .justify_end();
        let enabled = row.rename_enabled;
        buttons = buttons.child({
            let (sensor, current) = (row.id.clone(), row.name.clone());
            Self::icon_button(
                SharedString::from(format!("rename-{}", row.id)),
                NcIcon::Pencil,
            )
            .tooltip(locale.tr("sensors.rename"))
            .disabled(!enabled)
            .on_click({
                let view = view.clone();
                move |click, _, cx| {
                    view.update(cx, |this, cx| {
                        let open = this
                            .sensors
                            .rename
                            .as_ref()
                            .is_some_and(|(open, _)| open == &sensor);
                        if open {
                            this.sensors.rename = None;
                        } else if !this.closed_rename_by(click, &sensor) {
                            // The press on this button did not close its own popover.
                            this.sensors.open_rename(&sensor, &current);
                        }
                        cx.notify();
                    });
                }
            })
        });
        if row.derived.is_some() {
            buttons = buttons.child({
                let sensor = row.id.clone();
                Self::icon_button(
                    SharedString::from(format!("edit-{}", row.id)),
                    NcIcon::Sliders,
                )
                .tooltip(locale.tr("sensors.edit"))
                .disabled(!enabled)
                .on_click(act(&view, move |this, _, cx| {
                    let state = this.state(cx);
                    this.sensors.editor = state
                        .snapshot
                        .sensors
                        .get(&sensor)
                        .and_then(DerivedEditor::edit);
                }))
            });
            buttons = buttons.child({
                let sensor = row.id.clone();
                Self::icon_button(
                    SharedString::from(format!("delete-{}", row.id)),
                    NcIcon::Trash,
                )
                .tooltip(
                    row.delete_reason
                        .clone()
                        .unwrap_or_else(|| locale.tr("sensors.delete")),
                )
                .disabled(!enabled || row.delete_reason.is_some())
                .on_click(act(&view, move |this, _, _| {
                    this.send(Command::DeleteDerivedSensor {
                        sensor: sensor.clone(),
                    })
                }))
            });
        } else {
            let (label_key, disabled) = if row.hidden {
                ("sensors.show", !enabled)
            } else {
                ("sensors.hide", !row.hide_enabled)
            };
            buttons = buttons.child({
                let sensor = row.id.clone();
                Self::icon_button(SharedString::from(format!("hide-{}", row.id)), NcIcon::Eye)
                    .tooltip(
                        row.hide_reason
                            .clone()
                            .unwrap_or_else(|| locale.tr(label_key)),
                    )
                    .disabled(disabled)
                    .on_click(act(&view, move |this, _, cx| {
                        this.sensors.toggle_hidden(&sensor);
                        this.persist_hidden(cx);
                    }))
            });
        }

        let rename = rename_open.then(|| {
            self.rename_popover(
                InputKey::SensorRename,
                &row.name,
                locale.tr("sensors.rename_placeholder"),
                p,
                window,
                cx,
            )
        });
        let mut line = table_row(p)
            .id(SharedString::from(format!("sensor-{}", row.id)))
            .text_color(ink)
            .when(row.selected, |line| line.bg(p.accent_tint))
            .when(!row.selected, |line| line.hover(|line| line.bg(p.hover)))
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
            .child(with_popover(name, rename, Self::dismiss(&view)).flex_1())
            .child(value)
            .child(div().w(px(CHART)).flex_none().child(spark))
            .child(
                div()
                    .w(px(USED_BY))
                    .flex_none()
                    .truncate()
                    .text_color(p.fg_dim)
                    .child(row.used_by.clone()),
            )
            .child(buttons);
        if row.hidden {
            line = line.opacity(0.6);
        }
        let mut column = v_flex().w_full().child(line);
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

        let mut functions = seg(p);
        for function in FUNCTIONS {
            let active = model.function == function;
            functions = functions.child(
                seg_item(
                    SharedString::from(format!("fn-{function}")),
                    locale.tr(&format!("sensors.function.{function}")),
                    active,
                    true,
                    p,
                )
                .on_click(act(&view, move |this, _, _| {
                    if let Some(editor) = this.sensors.editor.as_mut() {
                        editor.set_function(function);
                    }
                })),
            );
        }

        let mut sources = v_flex().gap(px(6.));
        for (slot, source) in model.slots.iter().enumerate() {
            let picker_key = format!("derived-source-{slot}");
            let open = self.open_picker.as_deref() == Some(picker_key.as_str());
            let picker = open.then(|| {
                let rows = self.picker_rows(Some(source.sensor.clone()), None, cx);
                let pick: Pick = Rc::new(move |this, sensor, _, cx| {
                    if let Some(editor) = this.sensors.editor.as_mut() {
                        editor.set_source(slot, Some(sensor));
                    }
                    this.open_picker = None;
                    cx.notify();
                });
                self.picker_list(&picker_key, rows, pick, p, window, cx)
            });
            let trigger = select_trigger(
                SharedString::from(format!("source-{slot}")),
                source
                    .sensor_name
                    .clone()
                    .unwrap_or_else(|| locale.tr("sensors.editor.choose")),
                source.value.clone().map(SharedString::from),
                false,
                true,
                p.track,
            )
            .on_click({
                let key = picker_key.clone();
                act(&view, move |this, _, _| {
                    this.open_picker = if this.open_picker.as_deref() == Some(key.as_str()) {
                        None
                    } else {
                        Some(key.clone())
                    };
                })
            });
            let mut line = h_flex()
                .gap(px(9.))
                .items_center()
                .child(caption(source.label.clone(), p).w(px(90.)).flex_none())
                .child(with_popover(trigger, picker, Self::dismiss(&view)).flex_1());
            if source.removable {
                line = line.child(
                    Self::icon_button(
                        SharedString::from(format!("source-remove-{slot}")),
                        NcIcon::X,
                    )
                    .on_click(act(&view, move |this, _, _| {
                        if let Some(editor) = this.sensors.editor.as_mut() {
                            editor.remove_source(slot);
                        }
                    })),
                );
            }
            sources = sources.child(line);
        }
        if model.can_add_source {
            sources = sources.child({
                let view = view.clone();
                Self::small_button(
                    "add-source",
                    locale.tr("sensors.editor.add_source"),
                    Some(NcIcon::Plus),
                )
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
                    .child(Self::field_input(&name, None, false, p).disabled(!model.name_editable)),
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
                        Self::button("derived-cancel", locale.tr("sensors.editor.cancel")).on_click(
                            move |_, _, cx| {
                                view.update(cx, |this, cx| {
                                    this.sensors.editor = None;
                                    this.open_picker = None;
                                    cx.notify();
                                })
                            },
                        )
                    })
                    .child({
                        let view = view.clone();
                        let mut save =
                            Self::button("derived-save", locale.tr("sensors.editor.save"))
                                .primary()
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
            .max_h(px(360.))
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
                    .min_h(px(34.))
                    .px(px(12.))
                    .gap(px(9.))
                    .items_center()
                    .cursor_pointer()
                    .hover(|row| row.bg(p.hover))
                    .when(row.selected, |line| line.bg(p.accent_tint))
                    .on_click(move |_, window, cx| {
                        let (id, pick) = (id.clone(), pick.clone());
                        view.update(cx, |this, cx| pick(this, id, window, cx))
                    })
                    // The check mark of a selected row. Its place stays empty in the others.
                    .child(
                        div()
                            .w(px(16.))
                            .flex_none()
                            .children(row.selected.then(|| icon(NcIcon::Check, p.accent))),
                    )
                    .child(div().flex_1().min_w_0().truncate().child(row.name))
                    .child(caption(row.caption, p).flex_none())
                    .child(
                        div()
                            .w(px(70.))
                            .flex_none()
                            .text_right()
                            .font_weight(FontWeight::BOLD)
                            .child(row.value),
                    ),
            );
        }
        popover(p)
            .w(px(360.))
            .overflow_hidden()
            .child(div().p(px(6.)).child(
                Self::field_input(&search, None, false, p).prefix(icon(NcIcon::Search, p.fg_dim)),
            ))
            .child(list)
            .into_any_element()
    }
}
