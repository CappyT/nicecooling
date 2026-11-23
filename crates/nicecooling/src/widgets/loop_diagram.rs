//! The model of the Loop diagram (`docs/ui/widgets.md`): what the diagram draws, from the
//! Loop and the live values. It never holds a part that the Loop does not have.

use crate::model::{AlarmEntry, ChannelInfo, LoopInfo, Severity};
use crate::screens::ScreenInput;
use crate::screens::format::{clock, number, optional_value, unit_text, value_text};
use crate::shell::Page;

/// The instrument tag of a bubble.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tag {
    /// Pressure.
    Pt,
    /// Pump speed.
    St,
    /// Flow.
    Ft,
    /// Water temperature.
    Tt,
    /// Fill level.
    Lt,
}

impl Tag {
    pub fn text(self) -> &'static str {
        match self {
            Self::Pt => "PT",
            Self::St => "ST",
            Self::Ft => "FT",
            Self::Tt => "TT",
            Self::Lt => "LT",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BubbleState {
    Normal,
    Warning,
    Alarm,
    /// Dashed outline and "--": no live value.
    Offline,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bubble {
    pub tag: Tag,
    /// The display name of the part, under the part.
    pub name: String,
    /// The number, or "--".
    pub value: String,
    pub unit: String,
    pub caption: String,
    /// The information icon before the caption: a note, not a Warning.
    pub info: bool,
    pub state: BubbleState,
    /// The page that a click on the bubble opens.
    pub opens: Page,
    pub tooltip: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LoopDiagram {
    pub loop_name: String,
    pub live: bool,
    pub has_leakshield: bool,
    /// The fill level in percent. `None` without a Leakshield, or without a value.
    pub fill_level: Option<f64>,
    pub has_pump: bool,
    pub pump_count: usize,
    pub has_flow: bool,
    pub has_water: bool,
    /// "No Leakshield" and "Pressure is not measured.", for a Loop without a Leakshield.
    pub no_leakshield: Option<(String, String)>,
    pub bubbles: Vec<Bubble>,
    pub accessible: String,
    pub labels: DiagramLabels,
}

/// The texts that the diagram draws itself.
#[derive(Clone, Debug, PartialEq)]
pub struct DiagramLabels {
    pub leakshield: String,
    pub fill_level: String,
    pub blocks: String,
}

impl LoopDiagram {
    pub fn bubble(&self, tag: Tag) -> Option<&Bubble> {
        self.bubbles.iter().find(|bubble| bubble.tag == tag)
    }

    pub fn build(input: &ScreenInput<'_>, info: &LoopInfo) -> Self {
        let locale = input.locale;
        let state = input.state;
        let live = state.is_online();
        // The values stay on screen, dim, while the daemon is away.
        let sample = state.sensors.as_ref().or(state.last_sensors.as_ref());
        let value = |id: &str| sample.and_then(|s| s.sensor_value(id));
        let unit_of = |id: &str| {
            state
                .snapshot
                .sensors
                .get(id)
                .map_or("", |sensor| sensor.unit.as_str())
                .to_owned()
        };
        let sensor_name = |id: &str| {
            state
                .snapshot
                .sensors
                .get(id)
                .map_or_else(|| id.to_owned(), |sensor| sensor.name.clone())
        };

        let leakshield = info.leakshield.as_ref().and_then(|id| {
            state
                .snapshot
                .leakshields
                .iter()
                .find(|device| &device.id == id)
        });
        let alarms: Vec<&AlarmEntry> = state
            .snapshot
            .alarms
            .iter()
            .filter(|alarm| {
                alarm.source == info.id
                    || info.leakshield.as_deref() == Some(alarm.source.as_str())
                    || info.pump_channels.contains(&alarm.source)
            })
            .collect();
        let worst = |tag: Tag| {
            alarms
                .iter()
                .filter(|alarm| part_of(&alarm.event, &alarm.source, info) == Some(tag))
                .map(|alarm| alarm.severity)
                .max()
        };
        let unreachable = alarms
            .iter()
            .any(|alarm| alarm.event == "leakshield-unreachable");

        let state_of = |tag: Tag, has_value: bool| {
            if !live || !has_value {
                BubbleState::Offline
            } else {
                match worst(tag) {
                    Some(Severity::Alarm) => BubbleState::Alarm,
                    Some(Severity::Warning) => BubbleState::Warning,
                    None => BubbleState::Normal,
                }
            }
        };
        // Offline: no number, and the last value is the caption.
        let shown = |live_value: Option<f64>, unit: &str| -> (String, String) {
            match (live, live_value) {
                (true, Some(v)) => (number(locale, v, unit), unit_text(unit).to_owned()),
                _ => ("--".to_owned(), unit_text(unit).to_owned()),
            }
        };
        let last_value_caption = |v: Option<f64>, unit: &str| -> String {
            v.map_or_else(String::new, |v| value_text(locale, v, unit))
        };
        let age = |_: &str| -> String {
            sample.map_or_else(String::new, |s| {
                let seconds = input.now_us.saturating_sub(s.time_us) / 1_000_000;
                locale.fmt("diagram.age", &[("seconds", &seconds.to_string())])
            })
        };

        let mut bubbles = Vec::new();
        let mut accessible = vec![info.name.clone()];

        // Pressure and fill level: only with a Leakshield.
        let mut fill_level = None;
        if let Some(device) = leakshield {
            let pressure_id = format!("{}_pressure", device.id);
            let pressure = if unreachable {
                None
            } else {
                value(&pressure_id)
            };
            let unit = "mbar";
            let (text, unit_label) = shown(pressure, unit);
            let st = if unreachable {
                BubbleState::Offline
            } else {
                state_of(Tag::Pt, pressure.is_some())
            };
            let caption = if !live || unreachable {
                if unreachable {
                    locale.tr("diagram.caption.no_data")
                } else {
                    last_value_caption(value(&pressure_id), unit)
                }
            } else if device.mode == "release" {
                locale.tr("diagram.caption.no_protection")
            } else {
                pressure_caption(locale, pressure, device.pressure)
            };
            let state_text = state_text(locale, st);
            accessible.push(format!(
                "{}{}{}",
                locale.fmt(
                    "diagram.accessible.pressure",
                    &[(
                        "value",
                        &optional_value(locale, pressure.filter(|_| live), unit)
                    )]
                ),
                state_text
                    .as_deref()
                    .map_or_else(String::new, |text| format!(", {text}")),
                if caption.is_empty() {
                    String::new()
                } else {
                    format!(", {caption}")
                }
            ));
            bubbles.push(Bubble {
                tag: Tag::Pt,
                name: locale.tr("nav.leakshield"),
                value: text,
                unit: unit_label,
                caption,
                info: false,
                state: st,
                opens: Page::Leakshield(device.id.clone()),
                tooltip: format!(
                    "{}: {}. {}",
                    locale.tr("nav.leakshield"),
                    optional_value(locale, pressure.filter(|_| live), unit),
                    age(&pressure_id)
                ),
            });

            if !unreachable {
                let level_id = format!("{}_fill_level", device.id);
                let level = value(&level_id);
                fill_level = level;
                let (text, unit_label) = shown(level, "%");
                bubbles.push(Bubble {
                    tag: Tag::Lt,
                    name: locale.tr("diagram.reservoir"),
                    value: text,
                    unit: unit_label,
                    caption: if live {
                        String::new()
                    } else {
                        last_value_caption(level, "%")
                    },
                    info: false,
                    state: state_of(Tag::Lt, level.is_some()),
                    opens: Page::Leakshield(device.id.clone()),
                    tooltip: format!(
                        "{}: {}. {}",
                        locale.tr("diagram.reservoir"),
                        optional_value(locale, level.filter(|_| live), "%"),
                        age(&level_id)
                    ),
                });
                if let Some(level) = level {
                    accessible.push(locale.fmt(
                        "diagram.accessible.fill_level",
                        &[("value", &value_text(locale, level, "%"))],
                    ));
                }
            }
        }

        // Pump: one symbol, the lowest speed.
        let pumps: Vec<&ChannelInfo> = info
            .pump_channels
            .iter()
            .filter_map(|id| state.snapshot.channels.get(id))
            .collect();
        let has_pump = !pumps.is_empty();
        if has_pump {
            let speeds: Vec<Option<f64>> = pumps
                .iter()
                .map(|channel| fan_sensor(&channel.id).and_then(|id| value(&id)))
                .collect();
            let known: Vec<f64> = speeds.iter().flatten().copied().collect();
            let lowest = known.iter().copied().reduce(f64::min);
            let (text, unit_label) = shown(lowest, "rpm");
            let duty = sample.and_then(|s| s.channel_duty(&pumps[0].id));
            let caption = if !live {
                last_value_caption(lowest, "rpm")
            } else if pumps.len() > 1 {
                locale.plural("diagram.caption.pumps", pumps.len())
            } else if lowest == Some(0.0) {
                locale.tr("diagram.caption.stopped")
            } else if let Some(duty) = duty {
                locale.fmt(
                    "diagram.caption.duty",
                    &[("value", &number(locale, duty, "%"))],
                )
            } else {
                String::new()
            };
            let name = if pumps.len() > 1 {
                locale.plural("diagram.caption.pumps", pumps.len())
            } else {
                pumps[0].name.clone()
            };
            let st = state_of(Tag::St, lowest.is_some());
            if let Some(lowest) = lowest {
                accessible.push(locale.fmt(
                    "diagram.accessible.pump",
                    &[("value", &value_text(locale, lowest, "rpm"))],
                ));
            }
            bubbles.push(Bubble {
                tag: Tag::St,
                name: name.clone(),
                value: text,
                unit: unit_label,
                caption,
                info: false,
                state: st,
                opens: Page::Channels,
                tooltip: format!(
                    "{name}: {}. {}",
                    optional_value(locale, lowest.filter(|_| live), "rpm"),
                    sample.map_or_else(String::new, |_| age(""))
                ),
            });
        }

        // Flow: the calibrated sensor of the Loop.
        let flow_id = info.flow.clone();
        let has_flow = flow_id.is_some();
        if let Some(id) = flow_id {
            let flow = value(&id);
            let unit = "l/h";
            let (text, unit_label) = shown(flow, unit);
            let out_of_range =
                sample.is_some_and(|s| s.sensor_flags(&id) & crate::model::FLAG_OUT_OF_RANGE != 0);
            let first_flow = state
                .snapshot
                .calibration_tables
                .iter()
                .find(|table| table.id == info.calibration_table)
                .and_then(|table| table.points.first().map(|point| point.1));
            let (caption, note) = if !live {
                (last_value_caption(flow, unit), false)
            } else if out_of_range {
                (locale.tr("diagram.caption.above_table"), true)
            } else if let (Some(flow), Some(first)) = (flow, first_flow)
                && flow < first
            {
                (
                    locale.fmt(
                        "diagram.caption.below_flow",
                        &[("value", &number(locale, first, unit))],
                    ),
                    false,
                )
            } else {
                (locale.tr("diagram.caption.calibrated"), false)
            };
            if let Some(flow) = flow {
                accessible.push(locale.fmt(
                    "diagram.accessible.flow",
                    &[("value", &value_text(locale, flow, unit))],
                ));
            }
            bubbles.push(Bubble {
                tag: Tag::Ft,
                name: sensor_name(&id),
                value: text,
                unit: unit_label,
                caption,
                info: note,
                state: state_of(Tag::Ft, flow.is_some()),
                opens: Page::Sensors,
                tooltip: format!(
                    "{}: {}. {}",
                    sensor_name(&id),
                    optional_value(locale, flow.filter(|_| live), unit),
                    age(&id)
                ),
            });
        }

        // Water temperature.
        let water_id = info.water_temperature.clone();
        let has_water = water_id.is_some();
        if let Some(id) = water_id {
            let temperature = value(&id);
            let unit = unit_of(&id);
            let unit = if unit.is_empty() {
                "degC".to_owned()
            } else {
                unit
            };
            let (text, unit_label) = shown(temperature, &unit);
            if let Some(temperature) = temperature {
                accessible.push(locale.fmt(
                    "diagram.accessible.water",
                    &[("value", &value_text(locale, temperature, &unit))],
                ));
            }
            bubbles.push(Bubble {
                tag: Tag::Tt,
                name: sensor_name(&id),
                value: text,
                unit: unit_label,
                caption: if live {
                    locale.tr("diagram.caption.water")
                } else {
                    last_value_caption(temperature, &unit)
                },
                info: false,
                state: state_of(Tag::Tt, temperature.is_some()),
                opens: Page::Sensors,
                tooltip: format!(
                    "{}: {}. {}",
                    sensor_name(&id),
                    optional_value(locale, temperature.filter(|_| live), &unit),
                    age(&id)
                ),
            });
        }

        let has_leakshield = leakshield.is_some();
        Self {
            loop_name: info.name.clone(),
            live,
            has_leakshield,
            fill_level,
            has_pump,
            pump_count: pumps.len(),
            has_flow,
            has_water,
            no_leakshield: (!has_leakshield).then(|| {
                (
                    locale.tr("diagram.no_leakshield.title"),
                    locale.tr("diagram.no_leakshield.text"),
                )
            }),
            bubbles,
            accessible: accessible.join(". ") + ".",
            labels: DiagramLabels {
                leakshield: locale.tr("nav.leakshield"),
                fill_level: locale.tr("diagram.fill_level"),
                blocks: locale.tr("diagram.blocks"),
            },
        }
    }
}

/// The speed sensor of a Channel: `nct6798_pwm6` pairs with `nct6798_fan6`.
pub fn fan_sensor(channel: &str) -> Option<String> {
    let (chip, number) = channel.rsplit_once("_pwm")?;
    Some(format!("{chip}_fan{number}"))
}

/// The part that an event marks in the diagram.
fn part_of(event: &str, source: &str, info: &LoopInfo) -> Option<Tag> {
    match event {
        "pump-stop" => Some(Tag::St),
        "leakshield-unreachable" => Some(Tag::Pt),
        "lost-input" | "another-writer" | "manual-at-snapshot"
            if info.pump_channels.iter().any(|id| id == source) =>
        {
            Some(Tag::St)
        }
        e if e.starts_with("leakshield.fill-level") => Some(Tag::Lt),
        e if e.starts_with("leakshield.") => Some(Tag::Pt),
        _ => None,
    }
}

fn pressure_caption(
    locale: crate::i18n::Locale,
    pressure: Option<f64>,
    limits: Option<crate::model::PressureLimits>,
) -> String {
    let (Some(pressure), Some(limits)) = (pressure, limits) else {
        return String::new();
    };
    let n = |value: f64| number(locale, value, "mbar");
    if pressure < limits.min {
        locale.fmt("diagram.caption.below_min", &[("value", &n(limits.min))])
    } else if pressure > limits.max {
        locale.fmt("diagram.caption.above_max", &[("value", &n(limits.max))])
    } else {
        locale.fmt("diagram.caption.target", &[("value", &n(limits.target))])
    }
}

fn state_text(locale: crate::i18n::Locale, state: BubbleState) -> Option<String> {
    match state {
        BubbleState::Alarm => Some(locale.tr("diagram.state.alarm")),
        BubbleState::Warning => Some(locale.tr("diagram.state.warning")),
        BubbleState::Offline => Some(locale.tr("diagram.state.no_value")),
        BubbleState::Normal => None,
    }
}

/// The time of the last values: "21:40:12".
pub fn last_value_time(input: &ScreenInput<'_>) -> Option<String> {
    input
        .state
        .last_sensors
        .as_ref()
        .map(|sample| clock(sample.time_us))
}

// ---- The element ----

use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    App, Bounds, Div, FontWeight, Hsla, SharedString, Window, canvas, div, px, relative, size,
};
use gpui_component::tooltip::Tooltip;

use crate::tokens::Palette;
use crate::widgets::paint::{at, disc, polygon, polyline, rect};

/// What a click on a bubble calls.
type OpenPage = Rc<dyn Fn(Page, &mut Window, &mut App)>;

/// The height of the diagram in px.
pub const HEIGHT: f32 = 168.0;

/// Where the parts sit: the fractions of the width, as in `widgets.md`.
const PUMP: f32 = 0.34;
const FLOW: f32 = 0.58;
const WATER: f32 = 0.80;
const BLOCKS: f32 = 0.69;

fn overlay(left: gpui::DefiniteLength, shift: f32, top: f32) -> Div {
    div().absolute().left(left).ml(px(shift)).top(px(top))
}

/// The diagram of one Loop. `on_open` gets the page of a clicked bubble.
pub fn element(
    diagram: &LoopDiagram,
    p: &Palette,
    on_open: impl Fn(Page, &mut Window, &mut App) + 'static,
) -> Div {
    let on_open: OpenPage = Rc::new(on_open);
    let has_leakshield = diagram.has_leakshield;
    let level = diagram.fill_level;
    let (has_pump, has_flow, has_water) = (diagram.has_pump, diagram.has_flow, diagram.has_water);
    let colors = (p.water, p.air, p.fg, p.part, p.card, p.offline);

    let paint = canvas(
        |_, _, _| (),
        move |bounds: Bounds<gpui::Pixels>, _, window, _| {
            let (water, air, fg, part, card, offline) = colors;
            let w = f32::from(bounds.size.width);
            let o = bounds.origin;
            let x0 = if has_leakshield { 88.0 } else { 34.0 };
            let pump_x = (w * PUMP).round();
            let flow_x = (w * FLOW).round();
            let water_x = (w * WATER).round();

            // The water path: the supply leg, the corner at the right, the return leg.
            let mut loop_path = vec![at(o, x0, 100.0), at(o, w - 34.0, 100.0)];
            for (dx, dy) in [(3.0, 1.0), (7.0, 3.0), (10.0, 10.0)] {
                loop_path.push(at(o, w - 34.0 + dx, 100.0 + dy));
            }
            loop_path.push(at(o, w - 24.0, 140.0));
            for (dx, dy) in [(0.0, 7.0), (3.0, 9.0), (-10.0, 10.0)] {
                loop_path.push(at(o, w - 24.0 + dx, 140.0 + dy));
            }
            loop_path.push(at(o, x0, 150.0));
            if !has_leakshield {
                for (dx, dy) in [(-7.0, 0.0), (-10.0, -10.0)] {
                    loop_path.push(at(o, x0 + dx, 150.0 + dy));
                }
                loop_path.push(at(o, x0 - 10.0, 110.0));
                loop_path.push(at(o, x0, 100.0));
            }
            polyline(window, &loop_path, 3.0, water, None);

            // The flow arrows, one on each leg.
            let arrow_x = ((pump_x + flow_x) / 2.0).round();
            polyline(
                window,
                &[
                    at(o, arrow_x - 6.0, 93.0),
                    at(o, arrow_x + 6.0, 100.0),
                    at(o, arrow_x - 6.0, 107.0),
                ],
                3.0,
                water,
                None,
            );
            polyline(
                window,
                &[
                    at(o, pump_x - 52.0, 143.0),
                    at(o, pump_x - 64.0, 150.0),
                    at(o, pump_x - 52.0, 157.0),
                ],
                3.0,
                water,
                None,
            );

            if has_leakshield {
                // The reservoir with its fill level, and the Leakshield capsule over it.
                polyline(
                    window,
                    &[at(o, 56.0, 60.0), at(o, 56.0, 82.0)],
                    1.5,
                    air,
                    Some((4.0, 3.0)),
                );
                let reservoir = Bounds::new(at(o, 24.0, 82.0), size(px(64.), px(78.)));
                rect(window, reservoir, 4.0, part, Some((fg, 1.5, false)));
                if let Some(level) = level {
                    let height = (76.0 * (level / 100.0).clamp(0.0, 1.0)) as f32;
                    let fill = Bounds::new(at(o, 25.0, 159.0 - height), size(px(62.), px(height)));
                    rect(window, fill, 0.0, water.opacity(0.3), None);
                }
                let capsule = Bounds::new(at(o, 8.0, 32.0), size(px(96.), px(28.)));
                rect(window, capsule, 14.0, part, Some((fg, 1.5, false)));
                polyline(
                    window,
                    &[at(o, 104.0, 46.0), at(o, 114.0, 46.0)],
                    1.0,
                    fg,
                    None,
                );
            }

            // Blocks and radiators: one dashed box that nicecooling does not measure.
            let box_w = (w * 0.3).min(230.0);
            let box_x = (w * BLOCKS - box_w / 2.0).round();
            let blocks = Bounds::new(at(o, box_x, 138.0), size(px(box_w), px(24.)));
            rect(window, blocks, 4.0, card, Some((offline, 1.0, true)));

            if has_pump {
                polyline(
                    window,
                    &[at(o, pump_x, 64.0), at(o, pump_x, 86.0)],
                    1.0,
                    fg,
                    None,
                );
                disc(
                    window,
                    at(o, pump_x, 100.0),
                    14.0,
                    part,
                    Some((fg, 1.5, false)),
                );
                polygon(
                    window,
                    &[
                        at(o, pump_x - 5.0, 93.0),
                        at(o, pump_x + 7.0, 100.0),
                        at(o, pump_x - 5.0, 107.0),
                    ],
                    fg,
                );
            }
            if has_flow {
                polyline(
                    window,
                    &[at(o, flow_x, 64.0), at(o, flow_x, 89.0)],
                    1.0,
                    fg,
                    None,
                );
                let sensor = Bounds::new(at(o, flow_x - 18.0, 89.0), size(px(36.), px(22.)));
                rect(window, sensor, 3.0, part, Some((fg, 1.5, false)));
            }
            if has_water {
                polyline(
                    window,
                    &[at(o, water_x, 64.0), at(o, water_x, 96.0)],
                    1.0,
                    fg,
                    None,
                );
                disc(window, at(o, water_x, 100.0), 4.5, fg, None);
            }
        },
    )
    .absolute()
    .size_full();

    let mut root = div()
        .relative()
        .w_full()
        .h(px(HEIGHT))
        .text_size(px(12.))
        .text_color(p.fg)
        .child(paint);

    if has_leakshield {
        root = root
            .child(
                overlay(px(8.).into(), 0.0, 32.0)
                    .w(px(96.))
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_weight(FontWeight::BOLD)
                    .child(diagram.labels.leakshield.clone()),
            )
            .child(
                overlay(px(24.).into(), 0.0, 112.0)
                    .w(px(64.))
                    .flex()
                    .justify_center()
                    .font_weight(FontWeight::BOLD)
                    .children(diagram.fill_level.map(|v| format!("{v:.0} %"))),
            )
            .child(
                overlay(px(24.).into(), 0.0, 130.0)
                    .w(px(64.))
                    .flex()
                    .justify_center()
                    .text_color(p.fg_dim)
                    .child(diagram.labels.fill_level.clone()),
            );
    } else if let Some((title, text)) = &diagram.no_leakshield {
        root = root
            .child(
                overlay(px(24.).into(), 0.0, 30.0)
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.fg_dim)
                    .child(title.clone()),
            )
            .child(overlay(px(24.).into(), 0.0, 48.0).child(text.clone()));
    }

    // Under the parts: the name of each part, and the dashed box.
    let name_of = |tag: Tag| diagram.bubble(tag).map(|b| b.name.clone());
    for (tag, frac) in [(Tag::St, PUMP), (Tag::Ft, FLOW), (Tag::Tt, WATER)] {
        if let Some(name) = name_of(tag) {
            root = root.child(
                overlay(relative(frac), -70.0, 120.0)
                    .w(px(140.))
                    .text_center()
                    .truncate()
                    .child(name),
            );
        }
    }
    root = root.child(
        overlay(relative(BLOCKS), -100.0, 142.0)
            .w(px(200.))
            .text_center()
            .text_color(p.offline)
            .child(diagram.labels.blocks.clone()),
    );

    for (index, bubble) in diagram.bubbles.iter().enumerate() {
        // The fill level shows as text on the reservoir: it has no bubble of its own.
        let left: gpui::DefiniteLength = match bubble.tag {
            Tag::Pt => px(132.).into(),
            Tag::St => relative(PUMP),
            Tag::Ft => relative(FLOW),
            Tag::Tt => relative(WATER),
            Tag::Lt => continue,
        };
        let top = 46.0;
        let (fill, ink, edge, value_color) = match bubble.state {
            BubbleState::Normal => (p.card, p.fg, p.fg, p.fg),
            BubbleState::Warning => (p.warning_bg, p.fg, p.fg, p.fg),
            BubbleState::Alarm => (p.alarm_bg, p.alarm_fg, p.alarm_bg, p.alarm),
            BubbleState::Offline => (p.card, p.offline, p.offline, p.offline),
        };
        let page = bubble.opens.clone();
        let open = on_open.clone();
        let tooltip: SharedString = bubble.tooltip.clone().into();
        let circle = overlay(left, -18.0, top - 18.0)
            .id(("bubble", index))
            .size(px(36.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(fill)
            .border_color(edge)
            .when(bubble.state == BubbleState::Offline, |d| d.border_dashed())
            .border_2()
            .text_color(ink)
            .font_family("monospace")
            .font_weight(FontWeight::BOLD)
            .cursor_pointer()
            .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
            .on_click(move |_, window, cx| open(page.clone(), window, cx))
            .child(bubble.tag.text());
        root = root.child(circle);

        let value_left = if bubble.tag == Tag::Pt { 158.0 } else { 26.0 };
        let (anchor, shift_value): (gpui::DefiniteLength, f32) = if bubble.tag == Tag::Pt {
            (px(value_left).into(), 0.0)
        } else {
            (left, value_left)
        };
        root = root.child(
            overlay(anchor, shift_value, top - 20.0)
                .flex()
                .items_baseline()
                .gap(px(4.))
                .text_color(value_color)
                .child(
                    div()
                        .text_size(px(16.))
                        .font_weight(FontWeight::BOLD)
                        .child(bubble.value.clone()),
                )
                .child(
                    div()
                        .text_color(p.fg_dim)
                        .font_weight(FontWeight::NORMAL)
                        .child(bubble.unit.clone()),
                ),
        );
        if !bubble.caption.is_empty() || bubble.info {
            let marker = match bubble.state {
                BubbleState::Alarm => Some(("\u{2B22}", p.alarm)),
                BubbleState::Warning => Some(("\u{25B2}", p.warning)),
                _ if bubble.info => Some(("\u{24D8}", p.fg_dim)),
                _ => None,
            };
            root =
                root.child(
                    overlay(anchor, shift_value, top + 3.0)
                        .flex()
                        .gap(px(4.))
                        .text_color(p.fg_dim)
                        .children(marker.map(|(glyph, color): (&str, Hsla)| {
                            div().text_color(color).child(glyph)
                        }))
                        .child(bubble.caption.clone()),
                );
        }
    }
    root
}
