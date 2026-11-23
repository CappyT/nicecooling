//! The History screen (`docs/ui/screens/history.md`): charts of any set of sensors over time.
//! The app records the history, so a gap means that no session ran the app.

use std::collections::BTreeSet;

use crate::history::query::{ChartPoint, Resolution, SeriesData, downsample};
use crate::model::{AppState, Severity};
use crate::screens::ScreenInput;
use crate::screens::format::{number, unit_text, value_text};

/// The ranges of the toolbar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Range {
    Hour,
    Day,
    TwoDays,
    Week,
    Month,
    Year,
}

impl Range {
    pub const ALL: [Self; 6] = [
        Self::Hour,
        Self::Day,
        Self::TwoDays,
        Self::Week,
        Self::Month,
        Self::Year,
    ];

    pub fn seconds(self) -> i64 {
        match self {
            Self::Hour => 3600,
            Self::Day => 24 * 3600,
            Self::TwoDays => 48 * 3600,
            Self::Week => 7 * 24 * 3600,
            Self::Month => 30 * 24 * 3600,
            Self::Year => 365 * 24 * 3600,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Hour => "history.range.1h",
            Self::Day => "history.range.24h",
            Self::TwoDays => "history.range.48h",
            Self::Week => "history.range.7d",
            Self::Month => "history.range.30d",
            Self::Year => "history.range.1y",
        }
    }
}

/// What the user changes on the screen.
#[derive(Clone, Debug)]
pub struct HistoryUi {
    pub selected: Vec<String>,
    pub range: Range,
    /// The end of the window in Unix seconds. `None` follows "Now".
    pub end: Option<i64>,
    /// A zoom: the window that a drag or the wheel chose.
    pub zoom: Option<(i64, i64)>,
    /// The legend rows that the user switched off.
    pub hidden: BTreeSet<String>,
    /// The cursor in Unix seconds.
    pub cursor: Option<i64>,
    pub picker_open: bool,
}

impl Default for HistoryUi {
    fn default() -> Self {
        Self {
            selected: Vec::new(),
            range: Range::Hour,
            end: None,
            zoom: None,
            hidden: BTreeSet::new(),
            cursor: None,
            picker_open: false,
        }
    }
}

impl HistoryUi {
    /// The window in Unix seconds.
    pub fn window(&self, now: i64) -> (i64, i64) {
        if let Some(zoom) = self.zoom {
            return zoom;
        }
        let end = self.end.unwrap_or(now);
        (end - self.range.seconds(), end)
    }

    /// "Now" is active: the chart follows the live values.
    pub fn follows_now(&self) -> bool {
        self.end.is_none() && self.zoom.is_none()
    }

    pub fn set_range(&mut self, range: Range) {
        self.range = range;
        self.zoom = None;
    }

    pub fn back(&mut self, now: i64) {
        let (from, to) = self.window(now);
        self.shift(now, -(to - from));
    }

    pub fn forward(&mut self, now: i64) {
        let (from, to) = self.window(now);
        self.shift(now, to - from);
    }

    /// Shift the window. It never goes past "now".
    pub fn shift(&mut self, now: i64, seconds: i64) {
        let (from, to) = self.window(now);
        let (from, to) = (from + seconds, to + seconds);
        if to >= now {
            match self.zoom {
                Some(_) => self.zoom = Some((now - (to - from), now)),
                None => self.end = None,
            }
            return;
        }
        match self.zoom {
            Some(_) => self.zoom = Some((from, to)),
            None => self.end = Some(to),
        }
    }

    pub fn go_now(&mut self) {
        self.end = None;
        self.zoom = None;
    }

    pub fn reset_zoom(&mut self) {
        self.zoom = None;
    }

    /// Zoom to a time range: a drag in the chart.
    pub fn zoom_to(&mut self, from: i64, to: i64) {
        if to - from >= 10 {
            self.zoom = Some((from, to));
        }
    }

    /// The mouse wheel: `factor` below 1 zooms in, around `around`.
    pub fn zoom_around(&mut self, now: i64, around: i64, factor: f64) {
        let (from, to) = self.window(now);
        let span = ((to - from) as f64 * factor).clamp(10.0, Range::Year.seconds() as f64);
        let ratio = (around - from) as f64 / (to - from).max(1) as f64;
        let new_from = around - (span * ratio) as i64;
        self.zoom = Some((new_from, new_from + span as i64));
    }

    pub fn toggle_sensor(&mut self, sensor: &str) {
        match self.selected.iter().position(|id| id == sensor) {
            Some(index) => {
                self.selected.remove(index);
            }
            None => self.selected.push(sensor.to_owned()),
        }
    }

    pub fn toggle_series(&mut self, sensor: &str) {
        if !self.hidden.remove(sensor) {
            self.hidden.insert(sensor.to_owned());
        }
    }
}

/// The result of reading the database for the selected sensors.
#[derive(Clone, Debug, PartialEq)]
pub enum ChartData {
    Loaded(Vec<SeriesData>),
    /// The database cannot be read: the path and the error.
    Error {
        path: String,
        message: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryState {
    /// No sensor is selected.
    Empty,
    /// No sample exists in the window.
    NoData,
    Ready,
    DatabaseError,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Axis {
    pub unit: String,
    pub unit_text: String,
    pub min: f64,
    pub max: f64,
    pub side: Side,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LegendRow {
    /// "Water temperature 31.4 °C, min 30.9, max 31.8".
    pub text: String,
    pub name: String,
    pub value: String,
    pub extremes: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SeriesModel {
    pub sensor: String,
    pub name: String,
    pub unit: String,
    /// 0 to 5: the colors `s1` to `s6`.
    pub color: usize,
    /// A seventh series repeats the colors with a dashed line.
    pub dashed: bool,
    pub hidden: bool,
    pub resolution: Resolution,
    /// The runs of points: the line does not cross a gap.
    pub segments: Vec<Vec<ChartPoint>>,
    pub legend: LegendRow,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChartModel {
    /// One or two units: the left and the right axis.
    pub axes: Vec<Axis>,
    /// The indexes in `HistoryModel::series`.
    pub series: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AlarmMarker {
    /// Unix seconds.
    pub t: i64,
    pub severity: Severity,
    pub title: String,
    pub instance: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RangeButton {
    pub range: Range,
    pub label: String,
    pub active: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct HistoryModel {
    pub state: HistoryState,
    pub window: (i64, i64),
    pub sensors_label: String,
    pub ranges: Vec<RangeButton>,
    pub now_active: bool,
    pub can_forward: bool,
    pub can_reset_zoom: bool,
    pub empty_text: String,
    pub no_data_text: String,
    pub error_text: String,
    pub error_path: Option<String>,
    pub charts: Vec<ChartModel>,
    pub series: Vec<SeriesModel>,
    /// The regions with no data, in Unix seconds, with the label of the hatched ground.
    pub gaps: Vec<(i64, i64)>,
    pub gap_label: String,
    /// "Older than 48 hours: 1 minute mean, with minimum and maximum."
    pub aggregate_hint: Option<String>,
    pub markers: Vec<AlarmMarker>,
}

/// A nice y range for a unit.
fn nice_range(min: f64, max: f64) -> (f64, f64) {
    if !min.is_finite() || !max.is_finite() {
        return (0.0, 1.0);
    }
    let span = (max - min).max(1.0);
    let step = if span > 200.0 {
        50.0
    } else if span > 50.0 {
        10.0
    } else if span > 10.0 {
        5.0
    } else {
        1.0
    };
    (
        (min / step).floor() * step - step,
        (max / step).ceil() * step + step,
    )
}

impl HistoryModel {
    /// `max_points` is the width of the chart in pixels: a segment keeps no more points.
    pub fn build(
        input: &ScreenInput<'_>,
        ui: &HistoryUi,
        data: &ChartData,
        now: i64,
        max_points: usize,
    ) -> Self {
        let locale = input.locale;
        let window = ui.window(now);
        let ranges = Range::ALL
            .iter()
            .map(|range| RangeButton {
                range: *range,
                label: locale.tr(range.label_key()),
                active: *range == ui.range && ui.zoom.is_none(),
            })
            .collect();
        let sensors_label = if ui.selected.is_empty() {
            locale.tr("history.sensors")
        } else {
            locale.plural("history.sensors_count", ui.selected.len())
        };
        let mut model = Self {
            state: HistoryState::Empty,
            window,
            sensors_label,
            ranges,
            now_active: ui.follows_now(),
            can_forward: ui.end.is_some() || ui.zoom.is_some_and(|z| z.1 < now),
            can_reset_zoom: ui.zoom.is_some(),
            empty_text: locale.tr("history.empty"),
            no_data_text: locale.tr("history.no_data"),
            error_text: locale.tr("history.error"),
            error_path: None,
            charts: Vec::new(),
            series: Vec::new(),
            gaps: Vec::new(),
            gap_label: locale.tr("history.gap"),
            aggregate_hint: None,
            markers: Vec::new(),
        };
        if ui.selected.is_empty() {
            return model;
        }
        let loaded = match data {
            ChartData::Error { path, message } => {
                model.state = HistoryState::DatabaseError;
                model.error_path = Some(format!("{path}: {message}"));
                return model;
            }
            ChartData::Loaded(series) => series,
        };

        let name_of = |id: &str| name_of(input.state, id);
        for (index, id) in ui.selected.iter().enumerate() {
            let Some(data) = loaded.iter().find(|d| &d.sensor == id) else {
                continue;
            };
            let unit = unit_of(input.state, id).unwrap_or_else(|| data.unit.clone());
            let segments: Vec<Vec<ChartPoint>> = data
                .segments
                .iter()
                .map(|segment| downsample(segment, max_points))
                .collect();
            let at_cursor = ui
                .cursor
                .and_then(|t| nearest(&data.segments, t, data.resolution));
            let visible: Vec<&ChartPoint> = data.points().collect();
            let low = visible.iter().map(|p| p.min).fold(f64::INFINITY, f64::min);
            let high = visible
                .iter()
                .map(|p| p.max)
                .fold(f64::NEG_INFINITY, f64::max);
            let name = name_of(id);
            let value =
                at_cursor.map_or_else(|| "--".to_owned(), |p| value_text(locale, p.mean, &unit));
            let extremes = if visible.is_empty() {
                String::new()
            } else {
                locale.fmt(
                    "history.legend.extremes",
                    &[
                        ("min", &number(locale, low, &unit)),
                        ("max", &number(locale, high, &unit)),
                    ],
                )
            };
            model.series.push(SeriesModel {
                sensor: id.clone(),
                legend: LegendRow {
                    text: format!(
                        "{name} {value}{}",
                        if extremes.is_empty() {
                            String::new()
                        } else {
                            format!(", {extremes}")
                        }
                    ),
                    name: name.clone(),
                    value,
                    extremes,
                },
                name,
                unit,
                color: index % 6,
                dashed: index >= 6,
                hidden: ui.hidden.contains(id),
                resolution: data.resolution,
                segments,
            });
        }

        if model.series.iter().all(|s| s.segments.is_empty()) {
            model.state = HistoryState::NoData;
            return model;
        }
        model.state = HistoryState::Ready;

        // One y axis per unit, two axes in the first chart, and a chart for each further unit.
        let mut units: Vec<String> = Vec::new();
        for series in &model.series {
            if !units.contains(&series.unit) {
                units.push(series.unit.clone());
            }
        }
        let axis_of = |unit: &str, side: Side, series: &[SeriesModel]| {
            let points = series
                .iter()
                .filter(|s| s.unit == unit && !s.hidden)
                .flat_map(|s| s.segments.iter().flatten());
            let (min, max) = points.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                (lo.min(p.min), hi.max(p.max))
            });
            let (min, max) = nice_range(min, max);
            Axis {
                unit: unit.to_owned(),
                unit_text: unit_text(unit).to_owned(),
                min,
                max,
                side,
            }
        };
        let indexes = |unit: &str, series: &[SeriesModel]| -> Vec<usize> {
            series
                .iter()
                .enumerate()
                .filter(|(_, s)| s.unit == unit)
                .map(|(i, _)| i)
                .collect()
        };
        let first: Vec<&String> = units.iter().take(2).collect();
        model.charts.push(ChartModel {
            axes: first
                .iter()
                .enumerate()
                .map(|(i, unit)| {
                    axis_of(
                        unit,
                        if i == 0 { Side::Left } else { Side::Right },
                        &model.series,
                    )
                })
                .collect(),
            series: first
                .iter()
                .flat_map(|unit| indexes(unit, &model.series))
                .collect(),
        });
        for unit in units.iter().skip(2) {
            let chart = ChartModel {
                axes: vec![axis_of(unit, Side::Left, &model.series)],
                series: indexes(unit, &model.series),
            };
            model.charts.push(chart);
        }

        // The regions where no selected series has a row.
        let gap = loaded
            .first()
            .map_or(crate::history::query::SAMPLE_GAP_S, |d| {
                d.resolution.gap_s()
            });
        let mut spans: Vec<(i64, i64)> = loaded
            .iter()
            .flat_map(|d| d.segments.iter())
            .filter_map(|s| Some((s.first()?.t, s.last()?.t)))
            .collect();
        spans.sort_unstable();
        let mut cursor = window.0;
        for (start, end) in spans {
            if start - cursor > gap {
                model.gaps.push((cursor, start));
            }
            cursor = cursor.max(end);
        }
        if window.1.min(now) - cursor > gap {
            model.gaps.push((cursor, window.1.min(now)));
        }

        model.aggregate_hint = (model
            .series
            .iter()
            .any(|s| s.resolution == Resolution::Minutes))
        .then(|| locale.tr("history.aggregate_hint"));

        // Warnings and Alarms are vertical markers at their time.
        model.markers = input
            .state
            .snapshot
            .alarms
            .iter()
            .filter(|alarm| alarm.raised_us != 0)
            .map(|alarm| AlarmMarker {
                t: (alarm.raised_us / 1_000_000) as i64,
                severity: alarm.severity,
                title: locale.event_title(&alarm.event),
                instance: alarm.instance.clone(),
            })
            .filter(|marker| marker.t >= window.0 && marker.t <= window.1)
            .collect();
        model
    }
}

fn name_of(state: &AppState, id: &str) -> String {
    state
        .snapshot
        .sensors
        .get(id)
        .map(|sensor| sensor.name.clone())
        .or_else(|| state.snapshot.channels.get(id).map(|c| c.name.clone()))
        .unwrap_or_else(|| id.to_owned())
}

fn unit_of(state: &AppState, id: &str) -> Option<String> {
    state
        .snapshot
        .sensors
        .get(id)
        .map(|sensor| sensor.unit.clone())
        .or_else(|| {
            state
                .snapshot
                .channels
                .contains_key(id)
                .then(|| "%".to_owned())
        })
}

/// The point nearest to a time, when it is not in a gap.
fn nearest(segments: &[Vec<ChartPoint>], t: i64, resolution: Resolution) -> Option<ChartPoint> {
    segments
        .iter()
        .flatten()
        .min_by_key(|p| (p.t - t).abs())
        .filter(|p| (p.t - t).abs() <= resolution.gap_s())
        .copied()
}

/// The sensors that the picker offers: the sensors and the Channels, with their live value.
pub fn picker_rows(input: &ScreenInput<'_>) -> Vec<PickerRow> {
    let locale = input.locale;
    let state = input.state;
    let sample = state.sensors.as_ref();
    let mut rows: Vec<PickerRow> = state
        .snapshot
        .sensors
        .values()
        .map(|sensor| PickerRow {
            id: sensor.id.clone(),
            name: sensor.name.clone(),
            device: sensor.driver.clone(),
            value: sample
                .and_then(|s| s.sensor_value(&sensor.id))
                .map_or_else(|| "--".to_owned(), |v| value_text(locale, v, &sensor.unit)),
        })
        .collect();
    rows.extend(state.snapshot.channels.values().map(|channel| {
        PickerRow {
            id: channel.id.clone(),
            name: channel.name.clone(),
            device: locale.tr("nav.channels"),
            value: sample
                .and_then(|s| s.channel_duty(&channel.id))
                .map_or_else(|| "--".to_owned(), |v| value_text(locale, v, "%")),
        }
    }));
    rows
}

#[derive(Clone, Debug, PartialEq)]
pub struct PickerRow {
    pub id: String,
    pub name: String,
    pub device: String,
    pub value: String,
}
