//! The model of the Loop diagram (`docs/ui/widgets.md`): what the diagram draws, from the
//! Loop and the live values. It never holds a part that the Loop does not have.

use crate::icons::{NcIcon, small_icon};
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
        // Offline: "--" without a unit, and the last value is the caption.
        let shown = |live_value: Option<f64>, unit: &str| -> (String, String) {
            match (live, live_value) {
                (true, Some(v)) => (number(locale, v, unit), unit_text(unit).to_owned()),
                _ => ("--".to_owned(), String::new()),
            }
        };
        let last_value_caption = |v: Option<f64>, unit: &str| -> String {
            v.map_or_else(String::new, |v| {
                locale.fmt(
                    "diagram.caption.last",
                    &[("value", &value_text(locale, v, unit))],
                )
            })
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
                pressure_caption(
                    locale,
                    pressure,
                    state.leakshield_pressure_limits(&device.id),
                )
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
                .map(|channel| channel.fan_sensor.as_ref().and_then(|id| value(id)))
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
    AnyElement, App, AvailableSpace, Bounds, Div, Element, ElementId, FontWeight, GlobalElementId,
    Hsla, InspectorElementId, LayoutId, Pixels, Point, SharedString, Style, Window, canvas, div,
    px, relative, size,
};
use gpui_component::ActiveTheme as _;
use gpui_component::tooltip::Tooltip;

use crate::tokens::Palette;
use crate::widgets::paint::{at, disc, polygon, polyline, rect};

/// What a click on a bubble calls.
type OpenPage = Rc<dyn Fn(Page, &mut Window, &mut App)>;

/// The height of the diagram in px.
pub const HEIGHT: f32 = 168.0;
/// Under this width the diagram has two rows: the Leakshield group above the water path.
pub const NARROW: f32 = 640.0;
/// How far the water path moves down in the narrow diagram.
const ROW: f32 = 44.0;

/// Where the parts sit: the fractions of the width, as in `widgets.md`.
const PUMP: f32 = 0.34;
const FLOW: f32 = 0.58;
const WATER: f32 = 0.80;
const BLOCKS: f32 = 0.69;

/// The y of the bubbles, the supply leg and the return leg.
const BUBBLE_Y: f32 = 46.0;
const SUPPLY_Y: f32 = 100.0;
const RETURN_Y: f32 = 150.0;
/// The outline of a part symbol and of a bubble.
const OUTLINE: f32 = 1.5;

/// Add the points of an arc of radius `r` around `(cx, cy)`. Angles are in degrees: 0 points
/// right and 90 points down.
fn arc(
    points: &mut Vec<Point<Pixels>>,
    o: Point<Pixels>,
    c: (f32, f32),
    r: f32,
    from: f32,
    to: f32,
) {
    let steps = ((to - from).abs() / 15.0).ceil().max(1.0) as usize;
    for step in 1..=steps {
        let angle = (from + (to - from) * step as f32 / steps as f32).to_radians();
        points.push(at(o, c.0 + r * angle.cos(), c.1 + r * angle.sin()));
    }
}

/// A rectangle `[x, y, width, height]` with an outline that is centered on its edge, as an
/// SVG stroke is.
fn outlined(
    window: &mut Window,
    o: Point<Pixels>,
    [x, y, w, h]: [f32; 4],
    r: f32,
    fill: Hsla,
    edge: Hsla,
) {
    let half = OUTLINE / 2.0;
    let bounds = Bounds::new(
        at(o, x - half, y - half),
        size(px(w + OUTLINE), px(h + OUTLINE)),
    );
    rect(window, bounds, r + half, fill, Some((edge, OUTLINE, false)));
}

/// The circle of a bubble, around `o`.
fn bubble_circle(window: &mut Window, o: Point<Pixels>, state: BubbleState, p: &BubbleColors) {
    let center = at(o, 0.0, BUBBLE_Y);
    let (fill, edge) = match state {
        BubbleState::Normal => (p.card, p.fg),
        BubbleState::Warning => (p.warning_bg, p.warning_bg),
        BubbleState::Alarm => (p.alarm_bg, p.alarm_bg),
        BubbleState::Offline => {
            disc(window, center, 18.0, p.card, None);
            let mut ring = vec![at(o, 18.0, BUBBLE_Y)];
            arc(&mut ring, o, (0.0, BUBBLE_Y), 18.0, 0.0, 360.0);
            polyline(window, &ring, OUTLINE, p.offline, Some((4.0, 3.0)));
            return;
        }
    };
    disc(
        window,
        center,
        18.0 + OUTLINE / 2.0,
        fill,
        Some((edge, OUTLINE, false)),
    );
}

#[derive(Clone, Copy)]
struct BubbleColors {
    card: Hsla,
    fg: Hsla,
    offline: Hsla,
    warning_bg: Hsla,
    alarm_bg: Hsla,
}

/// The symbol of a part on the supply leg.
#[derive(Clone, Copy)]
enum Symbol {
    Pump,
    Flow,
    Water,
}

/// A slice of the diagram. The slices are a flex row: each one starts at the fraction of the
/// width that `widgets.md` gives its part. A slice never gets narrower than its text, so in a
/// narrow diagram the parts move to the right and no text reaches the next bubble.
fn slice(basis: f32) -> Div {
    div()
        .relative()
        .h_full()
        .flex_shrink_1()
        .flex_basis(relative(basis))
}

/// The height of a diagram of `width` px.
pub fn height(width: f32) -> f32 {
    if width < NARROW { HEIGHT + ROW } else { HEIGHT }
}

/// The diagram of one Loop. `on_open` gets the page of a clicked bubble. The element takes
/// the width of its parent, and that width chooses the layout.
pub fn element(
    diagram: &LoopDiagram,
    p: &Palette,
    on_open: impl Fn(Page, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Responsive {
    Responsive {
        diagram: diagram.clone(),
        dark: p.dark,
        mono: cx.theme().mono_font_family.clone(),
        on_open: Rc::new(on_open),
    }
}

/// Draws the wide or the narrow diagram, from the width that the layout gives it.
pub struct Responsive {
    diagram: LoopDiagram,
    dark: bool,
    mono: SharedString,
    on_open: OpenPage,
}

impl IntoElement for Responsive {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Responsive {
    type RequestLayoutState = ();
    type PrepaintState = AnyElement;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        _: &mut App,
    ) -> (LayoutId, ()) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        let layout = window.request_measured_layout(style, |known, available, _, _| {
            let width = known.width.unwrap_or(match available.width {
                AvailableSpace::Definite(width) => width,
                _ => px(0.),
            });
            size(width, px(height(f32::from(width))))
        });
        (layout, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        let narrow = f32::from(bounds.size.width) < NARROW;
        let mut drawn = layout(
            &self.diagram,
            &Palette::new(self.dark),
            self.mono.clone(),
            self.on_open.clone(),
            narrow,
        )
        .into_any_element();
        drawn.prepaint_as_root(bounds.origin, bounds.size.into(), window, cx);
        drawn
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        drawn: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) {
        drawn.paint(window, cx);
    }
}

/// The diagram in one of its two layouts. The narrow one keeps the Leakshield, its bubble
/// and its text in a row of their own, and has the water path `ROW` px lower.
fn layout(
    diagram: &LoopDiagram,
    p: &Palette,
    mono: SharedString,
    on_open: OpenPage,
    narrow: bool,
) -> Div {
    let down = if narrow { ROW } else { 0.0 };
    let (supply, back) = (SUPPLY_Y + down, RETURN_Y + down);
    let has_leakshield = diagram.has_leakshield;
    // The reservoir shows its level only while the value is live.
    let level = diagram.fill_level.filter(|_| diagram.live);
    let (water, air, fg, part, card) = (p.water, p.air, p.fg, p.part, p.card);
    let box_edge = p.border_strong;
    let bubble_colors = BubbleColors {
        card: p.card,
        fg: p.fg,
        offline: p.offline,
        warning_bg: p.warning_bg,
        alarm_bg: p.alarm_bg,
    };
    let pt_state = diagram.bubble(Tag::Pt).map(|bubble| bubble.state);

    // The parts that do not move: the water path, the reservoir, the Leakshield and the box.
    let base = canvas(
        |_, _, _| (),
        move |bounds: Bounds<Pixels>, _, window, _| {
            let w = f32::from(bounds.size.width);
            let o = bounds.origin;
            let x0 = if has_leakshield { 88.0 } else { 34.0 };
            let right = w - 34.0;

            // The water path: the supply leg, the corner at the right, the return leg.
            let mut path = vec![at(o, x0, supply), at(o, right, supply)];
            arc(&mut path, o, (right, supply + 10.0), 10.0, -90.0, 0.0);
            path.push(at(o, right + 10.0, back - 10.0));
            arc(&mut path, o, (right, back - 10.0), 10.0, 0.0, 90.0);
            path.push(at(o, x0, back));
            if !has_leakshield {
                arc(&mut path, o, (x0, back - 10.0), 10.0, 90.0, 180.0);
                path.push(at(o, x0 - 10.0, supply + 10.0));
                arc(&mut path, o, (x0, supply + 10.0), 10.0, 180.0, 270.0);
            }
            polyline(window, &path, 3.0, water, None);

            if has_leakshield {
                // The reservoir with its fill level, and the Leakshield capsule over it.
                polyline(
                    window,
                    &[at(o, 56.0, 60.0), at(o, 56.0, 82.0 + down)],
                    2.0,
                    air,
                    Some((5.0, 4.0)),
                );
                outlined(window, o, [24.0, 82.0 + down, 64.0, 78.0], 4.0, part, fg);
                if let Some(level) = level {
                    let height = (76.0 * (level / 100.0).clamp(0.0, 1.0)).round() as f32;
                    let fill = Bounds::new(
                        at(o, 25.0, 159.0 + down - height),
                        size(px(62.), px(height)),
                    );
                    rect(window, fill, 0.0, water.opacity(0.3), None);
                }
                outlined(window, o, [8.0, 32.0, 96.0, 28.0], 14.0, part, fg);
                polyline(
                    window,
                    &[at(o, 104.0, BUBBLE_Y), at(o, 114.0, BUBBLE_Y)],
                    1.0,
                    fg,
                    None,
                );
                if let Some(state) = pt_state {
                    bubble_circle(window, at(o, 132.0, 0.0), state, &bubble_colors);
                }
            }

            // Blocks and radiators: one dashed box that nicecooling does not measure.
            let box_w = (w * 0.3).min(230.0).round();
            let x = (w * BLOCKS - box_w / 2.0).round();
            let (top, bottom, far) = (138.0 + down, 162.0 + down, x + box_w);
            rect(
                window,
                Bounds::new(at(o, x, top), size(px(box_w), px(bottom - top))),
                4.0,
                card,
                None,
            );
            let mut edge = vec![at(o, x + 4.0, top), at(o, far - 4.0, top)];
            arc(&mut edge, o, (far - 4.0, top + 4.0), 4.0, -90.0, 0.0);
            edge.push(at(o, far, bottom - 4.0));
            arc(&mut edge, o, (far - 4.0, bottom - 4.0), 4.0, 0.0, 90.0);
            edge.push(at(o, x + 4.0, bottom));
            arc(&mut edge, o, (x + 4.0, bottom - 4.0), 4.0, 90.0, 180.0);
            edge.push(at(o, x, top + 4.0));
            arc(&mut edge, o, (x + 4.0, top + 4.0), 4.0, 180.0, 270.0);
            polyline(window, &edge, OUTLINE, box_edge, Some((5.0, 4.0)));
        },
    )
    .absolute()
    .size_full();

    let mut root = div()
        .relative()
        .w_full()
        .h(px(HEIGHT + down))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_size(px(12.))
        .line_height(px(16.))
        .text_color(p.fg_dim)
        .debug_selector(|| "loop-diagram".to_owned())
        .child(base)
        .child(
            div()
                .absolute()
                .left(relative(BLOCKS))
                .ml(px(-100.))
                .top(px(142. + down))
                .w(px(200.))
                .text_center()
                .child(diagram.labels.blocks.clone()),
        );

    // The value and the caption of a bubble: `left` px after the start of its slice, beside a
    // bubble at `y`.
    let text_of = |bubble: &Bubble, left: f32, y: f32| {
        let (value_color, caption_color) = match bubble.state {
            BubbleState::Normal => (p.fg, p.fg_dim),
            BubbleState::Warning => (p.fg, p.warning),
            BubbleState::Alarm => (p.alarm, p.alarm),
            BubbleState::Offline => (p.offline, p.fg_dim),
        };
        let marker = match bubble.state {
            BubbleState::Alarm => Some((NcIcon::Alarm, p.alarm)),
            BubbleState::Warning => Some((NcIcon::Warn, p.warning)),
            _ if bubble.info => Some((NcIcon::Info, p.fg_dim)),
            _ => None,
        };
        let name = bubble.tag.text();
        div()
            .flex()
            .flex_col()
            .items_start()
            .mt(px(y - 20.))
            .ml(px(left))
            // The text ends 5 px before the next bubble at least.
            .mr(px(23.))
            .child(
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(4.))
                    .text_size(px(16.))
                    .line_height(px(22.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(value_color)
                    .debug_selector(move || format!("loop-value-{name}"))
                    .child(bubble.value.clone())
                    .children((!bubble.unit.is_empty()).then(|| {
                        div()
                            .text_size(px(12.))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(p.fg_dim)
                            .child(bubble.unit.clone())
                    })),
            )
            .child(
                div()
                    .mt(px(1.))
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_color(caption_color)
                    .debug_selector(move || format!("loop-caption-{name}"))
                    .children(marker.map(|(name, color): (NcIcon, Hsla)| small_icon(name, color)))
                    .child(bubble.caption.clone()),
            )
    };
    // The circle of a bubble takes the click and shows the tooltip. `x`, `y` is its center.
    let tag_of = |index: usize, bubble: &Bubble, x: f32, y: f32| {
        let ink = match bubble.state {
            BubbleState::Normal => p.fg,
            BubbleState::Warning => p.warning_fg,
            BubbleState::Alarm => p.alarm_fg,
            BubbleState::Offline => p.offline,
        };
        let page = bubble.opens.clone();
        let open = on_open.clone();
        let tooltip: SharedString = bubble.tooltip.clone().into();
        let name = bubble.tag.text();
        div()
            .id(("bubble", index))
            .absolute()
            .left(px(x - 18.0))
            .top(px(y - 18.0))
            .size(px(36.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .text_color(ink)
            .font_family(mono.clone())
            .font_weight(FontWeight::BOLD)
            .cursor_pointer()
            .debug_selector(move || format!("loop-bubble-{name}"))
            .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
            .on_click(move |_, window, cx| open(page.clone(), window, cx))
            .child(bubble.tag.text())
    };
    let indexed = |tag: Tag| {
        diagram
            .bubbles
            .iter()
            .enumerate()
            .find(|(_, bubble)| bubble.tag == tag)
    };

    // The first slice: the Leakshield with its bubble, or the text of a Loop without one.
    // In the narrow diagram the text of this group is out of the slice: it has the row for
    // itself, and the slice only keeps the pump clear of the reservoir.
    let mut first = slice(PUMP);
    let mut group: Option<Div> = None;
    if has_leakshield {
        first = first
            .child(
                div()
                    .absolute()
                    .left(px(8.))
                    .top(px(32.))
                    .w(px(96.))
                    .h(px(28.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.fg)
                    .child(diagram.labels.leakshield.clone()),
            )
            .children(level.map(|level| {
                div()
                    .absolute()
                    .left(px(24.))
                    .top(px(110. + down))
                    .w(px(64.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(2.))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(p.fg)
                            .child(format!("{level:.0} %")),
                    )
                    .child(diagram.labels.fill_level.clone())
            }));
        if let Some((index, bubble)) = indexed(Tag::Pt) {
            first = first.child(tag_of(index, bubble, 132.0, BUBBLE_Y));
            group = Some(text_of(bubble, 158.0, BUBBLE_Y));
        }
    } else if let Some((title, text)) = &diagram.no_leakshield {
        group = Some(
            div()
                .mt(px(31.))
                .ml(px(24.))
                .mr(px(26.))
                .child(div().font_weight(FontWeight::BOLD).child(title.clone()))
                .child(div().mt(px(2.)).child(text.clone())),
        );
    }
    if narrow {
        // The pump stays right of the reservoir, or of the corner of a closed water path.
        first = first.min_w(px(if has_leakshield { 130. } else { 70. }));
        root = root.children(group.map(|group| div().absolute().top_0().left_0().child(group)));
    } else {
        first = first.children(group);
    }

    // The slices of the pump, the flow sensor and the water temperature. Each part is at the
    // left edge of its slice.
    let mut row = div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .flex()
        .child(first);
    let parts = [
        (Tag::St, Symbol::Pump, diagram.has_pump, FLOW - PUMP),
        (Tag::Ft, Symbol::Flow, diagram.has_flow, WATER - FLOW),
        (Tag::Tt, Symbol::Water, diagram.has_water, 1.0 - WATER),
    ];
    for (tag, symbol, present, basis) in parts {
        let found = indexed(tag);
        let state = found.map(|(_, bubble)| bubble.state);
        let mut part_slice = slice(basis).child(
            canvas(
                |_, _, _| (),
                move |bounds: Bounds<Pixels>, _, window, _| {
                    let o = bounds.origin;
                    let span = f32::from(bounds.size.width);
                    if matches!(symbol, Symbol::Pump) {
                        // The flow arrows, one on each leg. The narrow diagram has no room
                        // left of the pump: the arrow of the return leg is under the other.
                        let middle = (span / 2.0).round();
                        polyline(
                            window,
                            &[
                                at(o, middle - 6.0, supply - 7.0),
                                at(o, middle + 6.0, supply),
                                at(o, middle - 6.0, supply + 7.0),
                            ],
                            3.0,
                            water,
                            None,
                        );
                        let tip = if narrow { middle - 6.0 } else { -52.0 };
                        polyline(
                            window,
                            &[
                                at(o, tip + 12.0, back - 7.0),
                                at(o, tip, back),
                                at(o, tip + 12.0, back + 7.0),
                            ],
                            3.0,
                            water,
                            None,
                        );
                    }
                    if !present {
                        return;
                    }
                    let leader_end = match symbol {
                        Symbol::Pump => supply - 14.0,
                        Symbol::Flow => supply - 11.0,
                        Symbol::Water => supply - 4.0,
                    };
                    polyline(
                        window,
                        &[at(o, 0.0, 64.0 + down), at(o, 0.0, leader_end)],
                        1.0,
                        fg,
                        None,
                    );
                    match symbol {
                        Symbol::Pump => {
                            disc(
                                window,
                                at(o, 0.0, supply),
                                14.0 + OUTLINE / 2.0,
                                part,
                                Some((fg, OUTLINE, false)),
                            );
                            polygon(
                                window,
                                &[
                                    at(o, -5.0, supply - 7.0),
                                    at(o, 7.0, supply),
                                    at(o, -5.0, supply + 7.0),
                                ],
                                fg,
                            );
                        }
                        Symbol::Flow => {
                            outlined(window, o, [-18.0, supply - 11.0, 36.0, 22.0], 3.0, part, fg);
                        }
                        Symbol::Water => disc(window, at(o, 0.0, supply), 4.5, fg, None),
                    }
                    if let Some(state) = state {
                        bubble_circle(window, at(o, 0.0, down), state, &bubble_colors);
                    }
                },
            )
            .absolute()
            .top_0()
            .left_0()
            .size_full(),
        );
        if let Some((index, bubble)) = found {
            part_slice = part_slice
                .child(tag_of(index, bubble, 0.0, BUBBLE_Y + down))
                .child(
                    div()
                        .absolute()
                        .left(px(-70.))
                        .top(px(116. + down))
                        .w(px(140.))
                        .text_center()
                        .truncate()
                        .child(bubble.name.clone()),
                )
                .child(text_of(bubble, 26.0, BUBBLE_Y + down));
        }
        row = row.child(part_slice);
    }
    root.child(row)
}
