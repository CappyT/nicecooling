//! The window of the parameter panel: the header of the node, its label, one row per parameter,
//! its guide, the section "Inputs" and "Delete node".

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, Bounds, Context, Div, Entity, FontWeight, Pixels, SharedString, Window, canvas,
    div, px, relative,
};
use gpui_component::button::ButtonVariants as _;
use gpui_component::switch::Switch;
use gpui_component::{Disableable as _, h_flex, v_flex};
use nicecooling_graph::model::Kind;

use super::params::{
    Control, EndSensor, EndSwitch, Guide, InputRow, LabelRow, Panel, ParamRow, arg_key, label_key,
    pair_key, param_key,
};
use super::response_panel::Warning;
use super::try_run::{for_key, test_key};
use crate::app::now_us;
use crate::icons::{NcIcon, icon};
use crate::screen_view::{InputKey, ScreenView};
use crate::screens::controls::{Pick, act};
use crate::screens::sensors_view::PickRow;
use crate::screens::ui::{
    Tip as _, caption, heading, label, seg, seg_item, select_trigger, with_menu,
};
use crate::shell::Page;
use crate::tokens::Palette;
use crate::widgets::paint::{at, disc, polyline};

/// The width of the panel, and of the control of a row.
pub(super) const PANEL_WIDTH: f32 = 272.;
const CONTROL_WIDTH: f32 = 124.;

/// One row of the panel: the name at the left and the control at the right.
fn panel_row(name: impl Into<SharedString>, control: impl IntoElement) -> Div {
    h_flex()
        .w_full()
        .min_h(px(34.))
        .gap(px(9.))
        .items_center()
        .child(label(name).flex_1())
        .child(div().w(px(CONTROL_WIDTH)).flex_none().child(control))
}

/// The range of a field that holds a value that did not commit.
fn range_line(text: &Option<String>, p: &Palette) -> Option<Div> {
    text.as_ref().map(|text| {
        div()
            .w_full()
            .text_right()
            .text_size(px(12.))
            .line_height(px(16.))
            .text_color(p.alarm)
            .child(text.clone())
    })
}

/// A group of rows of the panel, with a line above it.
fn section(p: &Palette) -> Div {
    v_flex()
        .w_full()
        .px(px(12.))
        .py(px(9.))
        .gap(px(3.))
        .border_t_1()
        .border_color(p.border)
}

/// The band of a Limiter: the scale, the band with the texts of its ends, and the marker of
/// the input with its value.
fn band_guide(guide: &Guide, p: &Palette) -> Option<Div> {
    let Guide::Band {
        band,
        minimum,
        maximum,
        input,
    } = guide
    else {
        return None;
    };
    let (start, end) = (band.place(band.min), band.place(band.max));
    let (left, width) = (start.min(end), (end - start).abs());
    let marker = band.input.map(|value| band.place(value));
    let mut scale = div()
        .relative()
        .w_full()
        .h(px(22.))
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(px(11.))
                .h(px(1.))
                .bg(p.border_strong),
        )
        .child(
            div()
                .absolute()
                .left(relative(left))
                .w(relative(width))
                .top(px(4.))
                .h(px(14.))
                .rounded(px(2.))
                .bg(p.accent_tint)
                .border_1()
                .border_color(p.accent),
        );
    if let Some(place) = marker {
        scale = scale.child(
            div()
                .absolute()
                .left(relative(place))
                .ml(px(-3.))
                .top_0()
                .w(px(6.))
                .h(px(22.))
                .rounded(px(2.))
                .bg(p.fg),
        );
    }
    Some(
        v_flex()
            .w_full()
            .h(px(56.))
            .justify_center()
            .text_size(px(12.))
            .line_height(px(16.))
            .child(
                div()
                    .w_full()
                    .h(px(16.))
                    .text_center()
                    .font_weight(FontWeight::BOLD)
                    .children(input.clone()),
            )
            .child(scale)
            .child(
                h_flex()
                    .w_full()
                    .h(px(16.))
                    .justify_between()
                    .text_color(p.fg_dim)
                    .child(minimum.clone())
                    .child(maximum.clone()),
            ),
    )
}

/// The pairs of a Table as a line, with a dot at the input A.
fn table_guide(guide: &Guide, p: &Palette) -> Option<AnyElement> {
    let Guide::Table { pairs, step, input } = guide else {
        return None;
    };
    let (pairs, step, input) = (pairs.clone(), *step, *input);
    let (line, grid, dot) = (p.accent, p.border, p.fg);
    let plot = canvas(
        |_: Bounds<Pixels>, _, _| {},
        move |bounds: Bounds<Pixels>, _, window, _| {
            let (w, h) = (f32::from(bounds.size.width), f32::from(bounds.size.height));
            let span = |values: Vec<f64>| {
                let low = values.iter().copied().fold(f64::INFINITY, f64::min);
                let high = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                (low, (high - low).max(f64::EPSILON))
            };
            let (x_low, x_span) = span(pairs.iter().map(|pair| pair.0).collect());
            let (y_low, y_span) = span(pairs.iter().map(|pair| pair.1).collect());
            let px_x = |x: f64| (((x - x_low) / x_span).clamp(0.0, 1.0) as f32) * w;
            let px_y = |y: f64| h - (((y - y_low) / y_span).clamp(0.0, 1.0) as f32) * h;
            for row in 0..=4 {
                let y = h * row as f32 / 4.0;
                polyline(
                    window,
                    &[at(bounds.origin, 0.0, y), at(bounds.origin, w, y)],
                    1.0,
                    grid,
                    None,
                );
            }
            let mut points = Vec::new();
            for (index, (x, y)) in pairs.iter().enumerate() {
                // The step mode holds the output of the lower pair up to the next input.
                if step && index > 0 {
                    points.push(at(bounds.origin, px_x(*x), px_y(pairs[index - 1].1)));
                }
                points.push(at(bounds.origin, px_x(*x), px_y(*y)));
            }
            polyline(window, &points, 2.0, line, None);
            if let Some(input) = input {
                let output = pairs
                    .windows(2)
                    .find(|pair| input >= pair[0].0 && input <= pair[1].0)
                    .map(|pair| {
                        if step {
                            pair[0].1
                        } else {
                            let share = (input - pair[0].0) / (pair[1].0 - pair[0].0);
                            pair[0].1 + share * (pair[1].1 - pair[0].1)
                        }
                    })
                    .or_else(|| {
                        let (first, last) = (pairs.first()?, pairs.last()?);
                        Some(if input < first.0 { first.1 } else { last.1 })
                    });
                if let Some(output) = output {
                    disc(
                        window,
                        at(bounds.origin, px_x(input), px_y(output)),
                        4.0,
                        dot,
                        None,
                    );
                }
            }
        },
    )
    .w_full()
    .h(px(120.));
    Some(plot.into_any_element())
}

impl ScreenView {
    /// The parameter panel of the editor.
    pub(crate) fn graph_panel(
        &mut self,
        panel: &Panel,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let node = panel.node;
        let focus = self
            .graph_editor
            .as_ref()
            .map(|host| host.panel_focus.clone());
        let mut header = v_flex()
            .w_full()
            .px(px(12.))
            .py(px(9.))
            .gap(px(3.))
            .child(
                h_flex()
                    .w_full()
                    .gap(px(9.))
                    .items_center()
                    .child(heading(panel.title.clone()).flex_none())
                    .children(
                        panel
                            .family
                            .clone()
                            .map(|family| caption(family, p).truncate()),
                    )
                    .child(div().flex_1())
                    .children(
                        panel
                            .end
                            .as_ref()
                            .map(|end| Self::graph_end_switch(end, p, window, cx)),
                    ),
            )
            .child(caption(panel.help.clone(), p));
        if let Some(row) = &panel.label {
            header = header.children(self.graph_label_row(node, row, p, window, cx));
        }
        let response = panel.response.as_ref();
        for row in &panel.rows {
            // The base topic shows before the field "Topic" of an MQTT message.
            let prefix = response
                .and_then(|response| response.topic_prefix.clone())
                .filter(|_| row.name == "topic");
            header = header.children(self.graph_param_row(node, row, prefix, p, window, cx));
            // The warning of the kind is under the select "Kind".
            if row.name == "kind"
                && let Some(warning) = response.and_then(|response| response.warning.as_ref())
            {
                header = header.child(Self::graph_warning(warning, &view, p));
            }
        }
        if let Some(text) = response.and_then(|response| response.placeholders.clone()) {
            header = header.child(caption(text, p));
        }
        // "Run once" with its caption, and the newest rows of the response log.
        let once = response.map(|response| {
            section(p)
                .child(h_flex().child(
                    Self::small_button("graph-run-once", response.run_once.clone(), None).on_click(
                        act(&view, |this, _, cx| {
                            let state = this.state(cx);
                            let command = this
                                .graph_editor
                                .as_mut()
                                .and_then(|host| host.ui.run_once(&state));
                            if let Some(command) = command {
                                this.send_graph(command, cx);
                            }
                        }),
                    ),
                ))
                .child(caption(response.run_once_caption.clone(), p))
                .children(
                    response
                        .run_once_blocked
                        .clone()
                        .map(|text| caption(text, p).text_color(p.warning)),
                )
        });
        let runs = response.map(|response| {
            let mut list = section(p).child(
                div()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.fg_dim)
                    .child(response.runs_title.clone()),
            );
            for row in &response.runs {
                list = list.child(
                    h_flex()
                        .w_full()
                        .min_h(px(22.))
                        .gap(px(9.))
                        .items_center()
                        .text_size(px(12.))
                        .child(
                            caption(row.time.clone(), p)
                                .flex_1()
                                .font_family("Adwaita Mono"),
                        )
                        .child(div().flex_none().child(row.text.clone())),
                );
            }
            if let Some(text) = &response.no_run {
                list = list.child(caption(text.clone(), p));
            }
            list
        });
        if let Some(guide) = &panel.guide {
            header = header
                .children(band_guide(guide, p))
                .children(table_guide(guide, p));
        }
        if let Some(sensor) = &panel.sensor {
            header = header.children(Self::graph_end_sensor(sensor, p));
        }
        let mut inputs = section(p).child(
            div()
                .text_size(px(12.))
                .font_weight(FontWeight::BOLD)
                .text_color(p.fg_dim)
                .child(panel.inputs_title.clone()),
        );
        for row in &panel.inputs {
            inputs = inputs.child(self.graph_input_row(node, row, p, cx));
        }
        // "Test value" and "For" of a Sensor node. They are not part of the draft.
        let tests = panel.tests.as_ref().map(|rows| {
            let value = self.graph_field(test_key(node), &rows.value, "", false, p, window, cx);
            let seconds = self.graph_field(
                for_key(node),
                &rows.seconds,
                &rows.unit,
                rows.error.is_some(),
                p,
                window,
                cx,
            );
            section(p)
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(p.fg_dim)
                        .child(rows.title.clone()),
                )
                .child(panel_row(rows.value_label.clone(), value))
                .child(panel_row(rows.seconds_label.clone(), seconds))
                .children(range_line(&rows.error, p))
        });
        let delete = panel.delete.clone().map(|text| {
            section(p).child(
                h_flex().child(
                    Self::small_button("graph-delete-node", text, Some(NcIcon::Trash))
                        .ghost()
                        .on_click(act(&view, move |this, _, _| {
                            if let Some(host) = this.graph_editor.as_mut() {
                                host.ui.remove_node(node);
                            }
                        })),
                ),
            )
        });
        v_flex()
            .id("graph-panel")
            .w(px(PANEL_WIDTH))
            .flex_none()
            .overflow_y_scroll()
            .border_l_1()
            .border_color(p.border)
            .bg(p.headerbar)
            .debug_selector(|| "graph-panel".to_owned())
            .when_some(focus, |panel, focus| panel.track_focus(&focus))
            .child(header)
            .children(once)
            .when(!panel.inputs.is_empty(), |list| list.child(inputs))
            .children(tests)
            .children(runs)
            .children(delete)
            .into_any_element()
    }

    /// The segmented control of an end node: "Sensor" and "Response".
    fn graph_end_switch(
        end: &EndSwitch,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let item = |id: &'static str,
                    text: &str,
                    on: bool,
                    kind: Kind,
                    window: &mut Window,
                    cx: &mut Context<Self>| {
            seg_item(id, text.to_owned(), on, true, p, window, cx).on_click(act(
                &view,
                move |this, _, _| {
                    if let Some(host) = this.graph_editor.as_mut() {
                        host.ui.draft.set_end_kind(kind);
                    }
                },
            ))
        };
        let sensor = item(
            "graph-end-sensor",
            &end.sensor,
            end.is_sensor,
            Kind::SensorEnd,
            window,
            cx,
        );
        let response = item(
            "graph-end-response",
            &end.response,
            !end.is_sensor,
            Kind::ResponseEnd,
            window,
            cx,
        );
        seg("graph-end-kind", p, window, cx)
            .child(sensor)
            .child(response)
            .into_any_element()
    }

    /// A field of the panel. The text commits on Enter and when the field loses the focus.
    #[allow(clippy::too_many_arguments)]
    fn graph_field(
        &mut self,
        key: String,
        text: &str,
        unit: &str,
        invalid: bool,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let state = self.input(InputKey::Graph(key), text, "", window, cx);
        Self::mode_input(&state, unit, true, invalid, true, p).into_any_element()
    }

    /// The field "Label" with its caption.
    fn graph_label_row(
        &mut self,
        node: u32,
        row: &LabelRow,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let state = self.input(InputKey::Graph(label_key(node)), &row.text, "", window, cx);
        let field = Self::mode_input(&state, "", true, row.error.is_some(), true, p)
            .font_family("Adwaita Mono");
        let mut rows = vec![panel_row(row.label.clone(), field)];
        rows.extend(range_line(&row.error, p));
        rows.push(caption(row.caption.clone(), p));
        rows
    }

    /// The warning of a response kind: the Warning icon and the text on the tint, and the link
    /// "Open Settings" for an account that Settings does not have.
    fn graph_warning(warning: &Warning, view: &Entity<Self>, p: &Palette) -> Div {
        h_flex()
            .w_full()
            .px(px(9.))
            .py(px(6.))
            .gap(px(9.))
            .items_start()
            .rounded(px(6.))
            .bg(p.warning_tint)
            .child(icon(NcIcon::Warn, p.warning))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(warning.text.clone())
                    .children(warning.link.clone().map(|text| {
                        h_flex().child(
                            Self::link_button("graph-open-settings", text, p)
                                .on_click(act(view, |this, _, cx| this.go(Page::Settings, cx))),
                        )
                    })),
            )
    }

    /// One parameter of the node: its name and its control, and the range under a field that
    /// holds a value that did not commit. `prefix` is a dim text before the field.
    #[allow(clippy::too_many_arguments)]
    fn graph_param_row(
        &mut self,
        node: u32,
        row: &ParamRow,
        prefix: Option<String>,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<Div> {
        let view = cx.entity();
        let name = row.name;
        let invalid = row.error.is_some();
        let mut rows = Vec::new();
        match &row.control {
            Control::Field { text, unit } => {
                let field = self.graph_field(
                    param_key(node, name),
                    text,
                    unit.as_deref().unwrap_or_default(),
                    invalid,
                    p,
                    window,
                    cx,
                );
                rows.push(match prefix {
                    Some(prefix) => panel_row(
                        row.label.clone(),
                        h_flex()
                            .gap(px(3.))
                            .items_center()
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(12.))
                                    .text_color(p.fg_dim)
                                    .child(prefix),
                            )
                            .child(div().flex_1().min_w_0().child(field)),
                    ),
                    None => panel_row(row.label.clone(), field),
                });
            }
            Control::Select { shown, choices } => {
                let pick: Pick = Rc::new(move |this, value, _, cx| {
                    if let Some(host) = this.graph_editor.as_mut() {
                        host.ui.choose_param(node, name, &value);
                    }
                    cx.notify();
                });
                let select = self.select_field(
                    &format!("graph-param-{node}-{name}"),
                    shown,
                    choices.clone(),
                    true,
                    true,
                    p.track,
                    pick,
                    p,
                    cx,
                );
                rows.push(panel_row(row.label.clone(), select));
            }
            Control::Sensor { shown, choices } => {
                let key = format!("graph-sensor-{node}");
                let open = self.open_picker.as_deref() == Some(key.as_str());
                let picker = open.then(|| {
                    let rows: Vec<PickRow> = choices
                        .iter()
                        .map(|choice| PickRow {
                            id: choice.id.clone(),
                            name: choice.name.clone(),
                            caption: choice.caption.clone(),
                            value: choice.value.clone(),
                            selected: choice.selected,
                        })
                        .collect();
                    let pick: Pick = Rc::new(move |this, sensor, _, cx| {
                        if let Some(host) = this.graph_editor.as_mut() {
                            host.ui.choose_param(node, name, &sensor);
                        }
                        this.open_picker = None;
                        cx.notify();
                    });
                    self.picker_list(&key, rows, pick, p, window, cx)
                });
                let trigger = select_trigger(
                    SharedString::from(key.clone()),
                    shown.clone(),
                    None,
                    true,
                    true,
                    p.track,
                    window,
                    cx,
                );
                let toggled = key.clone();
                let trigger = Self::opens_picker(trigger, &view, &key, open, move |this| {
                    this.open_picker = if this.open_picker.as_deref() == Some(toggled.as_str()) {
                        None
                    } else {
                        Some(toggled.clone())
                    };
                });
                // The picker is wider than the panel: it opens to the left of the select.
                rows.push(panel_row(
                    row.label.clone(),
                    with_menu(trigger, picker, Self::dismiss(&view)),
                ));
            }
            Control::Pairs {
                pairs,
                can_add,
                can_remove,
            } => {
                let locale = self.model.read(cx).locale;
                rows.push(
                    h_flex()
                        .w_full()
                        .min_h(px(28.))
                        .items_center()
                        .child(label(row.label.clone())),
                );
                for (index, (input, output)) in pairs.iter().enumerate() {
                    let bad = |column: usize| {
                        self.graph_editor.as_ref().is_some_and(|host| {
                            host.ui
                                .field_error
                                .as_ref()
                                .is_some_and(|(key, _)| *key == pair_key(node, index, column))
                        })
                    };
                    let (bad_input, bad_output) = (bad(0), bad(1));
                    let first = self.graph_field(
                        pair_key(node, index, 0),
                        input,
                        "",
                        bad_input,
                        p,
                        window,
                        cx,
                    );
                    let second = self.graph_field(
                        pair_key(node, index, 1),
                        output,
                        "",
                        bad_output,
                        p,
                        window,
                        cx,
                    );
                    rows.push(
                        h_flex()
                            .w_full()
                            .gap(px(6.))
                            .items_center()
                            .child(div().flex_1().min_w_0().child(first))
                            .child(div().flex_1().min_w_0().child(second))
                            .child(
                                Self::icon_button(
                                    SharedString::from(format!("graph-pair-remove-{index}")),
                                    NcIcon::X,
                                )
                                .disabled(!can_remove)
                                .tip(locale.tr("graph_editor.panel.remove_pair"))
                                .on_click(act(
                                    &view,
                                    move |this, _, _| {
                                        if let Some(host) = this.graph_editor.as_mut() {
                                            host.ui.remove_pair(node, index);
                                        }
                                    },
                                )),
                            ),
                    );
                }
                rows.push(
                    h_flex().w_full().child(
                        Self::small_button(
                            "graph-pair-add",
                            locale.tr("graph_editor.panel.add_pair"),
                            Some(NcIcon::Plus),
                        )
                        .disabled(!can_add)
                        .on_click(act(&view, move |this, _, _| {
                            if let Some(host) = this.graph_editor.as_mut() {
                                host.ui.add_pair(node);
                            }
                        })),
                    ),
                );
            }
            Control::Args { args, can_add } => {
                let locale = self.model.read(cx).locale;
                rows.push(
                    h_flex()
                        .w_full()
                        .min_h(px(28.))
                        .items_center()
                        .child(label(row.label.clone())),
                );
                for (index, arg) in args.iter().enumerate() {
                    let field =
                        self.graph_field(arg_key(node, index), arg, "", false, p, window, cx);
                    rows.push(
                        h_flex()
                            .w_full()
                            .gap(px(6.))
                            .items_center()
                            .child(div().flex_1().min_w_0().child(field))
                            .child(
                                Self::icon_button(
                                    SharedString::from(format!("graph-arg-remove-{index}")),
                                    NcIcon::X,
                                )
                                .tip(locale.tr("graph_editor.panel.remove_argument"))
                                .on_click(act(
                                    &view,
                                    move |this, _, _| {
                                        if let Some(host) = this.graph_editor.as_mut() {
                                            host.ui.remove_arg(node, index);
                                        }
                                    },
                                )),
                            ),
                    );
                }
                rows.push(
                    h_flex().w_full().child(
                        Self::small_button(
                            "graph-arg-add",
                            locale.tr("graph_editor.panel.add_argument"),
                            Some(NcIcon::Plus),
                        )
                        .disabled(!can_add)
                        .on_click(act(&view, move |this, _, _| {
                            if let Some(host) = this.graph_editor.as_mut() {
                                host.ui.add_arg(node);
                            }
                        })),
                    ),
                );
            }
            Control::Optional { on, text } => {
                let locale = self.model.read(cx).locale;
                let switch = Switch::new(SharedString::from(format!("graph-switch-{node}-{name}")))
                    .checked(*on)
                    .on_click({
                        let view = view.clone();
                        move |on, _, cx| {
                            let on = *on;
                            view.update(cx, |this, cx| {
                                if let Some(host) = this.graph_editor.as_mut() {
                                    host.ui.switch_param(node, name, on);
                                }
                                cx.notify();
                            })
                        }
                    });
                let control = if *on {
                    let field = self.graph_field(
                        param_key(node, name),
                        text,
                        &locale.tr("graph_editor.panel.seconds"),
                        invalid,
                        p,
                        window,
                        cx,
                    );
                    h_flex()
                        .gap(px(6.))
                        .items_center()
                        .child(div().flex_none().child(switch))
                        .child(div().flex_1().min_w_0().child(field))
                } else {
                    h_flex().justify_end().child(switch)
                };
                rows.push(panel_row(row.label.clone(), control));
            }
        }
        rows.extend(range_line(&row.error, p));
        rows
    }

    /// The Graph sensor of a sensor end node: its id, and what reads it.
    fn graph_end_sensor(sensor: &EndSensor, p: &Palette) -> Vec<Div> {
        let id = div().truncate().child(sensor.id.clone());
        let id = if sensor.has_id {
            id.font_family("Adwaita Mono")
        } else {
            id.text_color(p.fg_dim)
        };
        let mut used = v_flex().w_full();
        for text in &sensor.used {
            used = used.child(label(text.clone()));
        }
        vec![
            panel_row(sensor.label.clone(), id),
            panel_row(sensor.used_label.clone(), used),
        ]
    }

    /// One row of the section "Inputs": the name of the pin, and a select with "None" and each
    /// other node that has an output pin.
    fn graph_input_row(
        &mut self,
        node: u32,
        row: &InputRow,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> Div {
        let pin = row.pin;
        let pick: Pick = Rc::new(move |this, value, _, cx| {
            if let Some(host) = this.graph_editor.as_mut() {
                host.ui
                    .choose_input(node, pin, value.parse::<u32>().ok(), now_us());
            }
            cx.notify();
        });
        let select = self.select_field(
            &format!("graph-input-{node}-{pin}"),
            &row.shown,
            row.choices.clone(),
            true,
            true,
            p.track,
            pick,
            p,
            cx,
        );
        panel_row(row.label.clone(), select)
    }
}
