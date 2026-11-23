mod support;

use nicecooling::history::query::{Resolution, load_series};
use nicecooling::history::{HistoryDb, HistoryWriter};
use nicecooling::i18n::{EN, IT};
use nicecooling::model::{AppState, Response};
use nicecooling::screens::history::{
    ChartData, HistoryModel, HistoryState, HistoryUi, Range, Side, picker_rows,
};
use nicecooling::screens::{Feedback, ScreenInput};
use support::fixtures::{LEAKSHIELD, alarm, full};
use support::labels;

/// A Unix time on a minute boundary.
const T0: i64 = 1_800_000_000;

fn input<'a>(state: &'a AppState, feedback: &'a Feedback) -> ScreenInput<'a> {
    ScreenInput {
        state,
        now_us: (T0 as u64 + 600) * 1_000_000,
        locale: EN,
        down_since_us: None,
        feedback,
    }
}

/// Two sensors with 1 s samples. "temp" has a gap of 60 s after T0 + 100.
fn loaded(ui: &HistoryUi, now: i64) -> ChartData {
    // The data go into one database, as the writer of the app does.
    let mut writer = HistoryWriter::new(HistoryDb::open_in_memory().unwrap());
    for t in T0..T0 + 400 {
        if !(T0 + 100..T0 + 160).contains(&t) {
            writer.record(
                t,
                "nct6798_temp1",
                "degC",
                Some(30.0 + (t - T0) as f64 / 100.0),
            );
        }
        if t % 2 == 0 {
            writer.record(t, "nct6798_pwm6", "%", Some(60.0));
        }
        writer.tick(t).unwrap();
    }
    writer.tick(T0 + 410).unwrap();
    let (from, to) = ui.window(now);
    ChartData::Loaded(
        ui.selected
            .iter()
            .map(|id| load_series(writer.db(), id, from, to, now).unwrap())
            .collect(),
    )
}

#[test]
fn history_states() {
    let state = full();
    let feedback = Feedback::default();
    let now = T0 + 400;
    let mut ui = HistoryUi::default();

    // Empty: no sensor selected, with the picker open.
    let empty = HistoryModel::build(
        &input(&state, &feedback),
        &ui,
        &ChartData::Loaded(vec![]),
        now,
        800,
    );
    assert_eq!(empty.state, HistoryState::Empty);
    assert_eq!(empty.empty_text, "Select sensors to chart.");
    assert_eq!(empty.sensors_label, "Sensors");
    assert_eq!(
        empty
            .ranges
            .iter()
            .map(|r| r.label.as_str())
            .collect::<Vec<_>>(),
        ["1 h", "24 h", "48 h", "7 d", "30 d", "1 y"]
    );
    assert!(empty.ranges[0].active && empty.now_active);

    // Ready: two units give two axes, the series have their colors and their legend.
    ui.toggle_sensor("nct6798_temp1");
    ui.toggle_sensor("nct6798_pwm6");
    ui.cursor = Some(T0 + 50);
    let ready = HistoryModel::build(&input(&state, &feedback), &ui, &loaded(&ui, now), now, 800);
    assert_eq!(ready.state, HistoryState::Ready);
    assert_eq!(ready.sensors_label, "2 sensors");
    assert_eq!(ready.charts.len(), 1);
    let axes = &ready.charts[0].axes;
    assert_eq!((axes[0].unit.as_str(), axes[0].side), ("degC", Side::Left));
    assert_eq!((axes[1].unit.as_str(), axes[1].side), ("%", Side::Right));
    assert_eq!(ready.series[0].color, 0);
    assert_eq!(ready.series[1].color, 1);
    assert!(!ready.series[0].dashed);
    assert_eq!(ready.series[0].name, "Water temperature");
    assert_eq!(ready.series[1].name, "Waterpump");
    assert_eq!(ready.series[0].resolution, Resolution::Samples);
    assert!(ready.aggregate_hint.is_none());
    // The legend shows the value at the cursor with the minimum and the maximum of the window.
    assert_eq!(ready.series[0].legend.value, "30.5 °C");
    assert_eq!(
        ready.series[0].legend.text,
        "Water temperature 30.5 °C, min 30.0, max 34.0"
    );

    // No data: the range holds no sample.
    ui.zoom_to(T0 - 5000, T0 - 4000);
    let none = HistoryModel::build(&input(&state, &feedback), &ui, &loaded(&ui, now), now, 800);
    assert_eq!(none.state, HistoryState::NoData);
    assert_eq!(
        none.no_data_text,
        "No data in this range. The app records the history while it runs."
    );

    // Database error: the text and the path.
    ui.reset_zoom();
    let broken = HistoryModel::build(
        &input(&state, &feedback),
        &ui,
        &ChartData::Error {
            path: "/home/u/.local/share/nicecooling/history.db".into(),
            message: "database disk image is malformed".into(),
        },
        now,
        800,
    );
    assert_eq!(broken.state, HistoryState::DatabaseError);
    assert_eq!(broken.error_text, "The history database cannot be read.");
    assert!(
        broken
            .error_path
            .unwrap()
            .starts_with("/home/u/.local/share/nicecooling/history.db")
    );

    // A third unit gets its own chart under the first. A seventh series repeats the colors.
    let mut wide = HistoryUi::default();
    for id in [
        "nct6798_temp1",
        "nct6798_pwm6",
        "loop_1_flow",
        "a",
        "b",
        "c",
        "d",
    ] {
        wide.toggle_sensor(id);
    }
    let data = ChartData::Loaded(
        wide.selected
            .iter()
            .enumerate()
            .map(|(i, id)| nicecooling::history::query::SeriesData {
                sensor: id.clone(),
                unit: ["degC", "%", "l/h", "degC", "degC", "degC", "degC"][i].into(),
                resolution: Resolution::Samples,
                segments: vec![vec![nicecooling::history::query::ChartPoint {
                    t: T0,
                    mean: 1.0,
                    min: 1.0,
                    max: 1.0,
                }]],
            })
            .collect(),
    );
    let many = HistoryModel::build(&input(&state, &feedback), &wide, &data, now, 800);
    assert_eq!(many.charts.len(), 2);
    assert_eq!(many.charts[1].axes[0].unit, "l/h");
    assert_eq!(many.series[6].color, 0);
    assert!(many.series[6].dashed && !many.series[5].dashed);

    // Hiding a legend row keeps the series, flagged.
    let mut hide = HistoryUi::default();
    hide.toggle_sensor("nct6798_temp1");
    hide.toggle_series("nct6798_temp1");
    assert!(
        HistoryModel::build(
            &input(&state, &feedback),
            &hide,
            &loaded(&hide, now),
            now,
            800
        )
        .series[0]
            .hidden
    );

    // Daemon offline: the chart works, the history is in the app.
    let mut offline = full();
    offline.link = nicecooling::model::Link::Offline;
    let model = HistoryModel::build(
        &input(&offline, &feedback),
        &ui,
        &loaded(&ui, now),
        now,
        800,
    );
    assert_eq!(model.state, HistoryState::Ready);

    // Alarm markers: a vertical line at the raise time, with its title.
    let mut alarmed = full();
    let mut leak = alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        Response::Notify,
        0,
    );
    leak.raised_us = (T0 as u64 + 200) * 1_000_000;
    alarmed.snapshot.alarms = vec![leak];
    let markers = HistoryModel::build(
        &input(&alarmed, &feedback),
        &ui,
        &loaded(&ui, now),
        now,
        800,
    )
    .markers;
    assert_eq!(markers.len(), 1);
    assert_eq!(
        (markers[0].t, markers[0].title.as_str()),
        (T0 + 200, "Large leak Alarm")
    );
    assert_eq!(markers[0].instance, "a1");

    // The aggregates after 48 hours: the hint shows.
    let mut old = HistoryUi::default();
    old.toggle_sensor("nct6798_temp1");
    old.set_range(Range::Week);
    let mut writer = HistoryWriter::new(HistoryDb::open_in_memory().unwrap());
    for t in T0..T0 + 180 {
        writer.record(t, "nct6798_temp1", "degC", Some((t - T0) as f64));
        writer.tick(t).unwrap();
    }
    let later = T0 + 3 * 24 * 3600;
    let (from, to) = old.window(later);
    let series = load_series(writer.db(), "nct6798_temp1", from, to, later).unwrap();
    assert_eq!(series.resolution, Resolution::Minutes);
    let aggregate = HistoryModel::build(
        &input(&state, &feedback),
        &old,
        &ChartData::Loaded(vec![series]),
        later,
        800,
    );
    assert_eq!(
        aggregate.aggregate_hint.as_deref(),
        Some("Older than 48 hours: 1 minute mean, with minimum and maximum.")
    );
    let first = aggregate.series[0].segments[0][0];
    assert!(
        first.min <= first.mean && first.mean <= first.max && first.min < first.max,
        "the band holds the minute"
    );
}

#[test]
fn gap_drawn_as_break() {
    let ui = {
        let mut ui = HistoryUi::default();
        ui.toggle_sensor("nct6798_temp1");
        ui
    };
    let now = T0 + 400;
    let state = full();
    let feedback = Feedback::default();
    let model = HistoryModel::build(&input(&state, &feedback), &ui, &loaded(&ui, now), now, 800);
    let series = &model.series[0];
    // The line does not cross the gap: two runs of points, 100 and 240 samples.
    assert_eq!(series.segments.len(), 2);
    assert_eq!(series.segments[0].len(), 100);
    assert_eq!(series.segments[1].len(), 240);
    assert_eq!(series.segments[0].last().unwrap().t, T0 + 99);
    assert_eq!(series.segments[1][0].t, T0 + 160);
    // The hatched ground covers the gap and the time before the app first ran. The writer
    // keeps the last 9 s of the run in its buffer.
    assert!(
        model.gaps.contains(&(T0 + 99, T0 + 160)),
        "{:?}",
        model.gaps
    );
    assert_eq!(model.gap_label, "No data: the app did not run");
    // A no-value sample is a break too.
    let mut writer = HistoryWriter::new(HistoryDb::open_in_memory().unwrap());
    for t in T0..T0 + 60 {
        writer.record(t, "x", "degC", if t == T0 + 30 { None } else { Some(1.0) });
        writer.tick(t).unwrap();
    }
    let series = load_series(writer.db(), "x", T0, T0 + 60, T0 + 60).unwrap();
    assert_eq!(series.segments.len(), 2);
    assert_eq!(series.segments[0].len(), 30);

    // A picker lists the sensors and the Channels, with their value.
    let rows = picker_rows(&input(&state, &feedback));
    let pump = rows.iter().find(|r| r.id == "nct6798_pwm6").unwrap();
    assert_eq!(
        (pump.name.as_str(), pump.value.as_str()),
        ("Waterpump", "62 %")
    );
}

#[test]
fn the_window_follows_the_toolbar() {
    let now = T0 + 100_000;
    let mut ui = HistoryUi::default();
    assert_eq!(ui.window(now), (now - 3600, now));
    ui.set_range(Range::Day);
    assert_eq!(ui.window(now), (now - 86_400, now));
    ui.back(now);
    assert_eq!(ui.window(now), (now - 2 * 86_400, now - 86_400));
    assert!(!ui.follows_now());
    ui.forward(now);
    assert!(ui.follows_now(), "forward to the end is \"Now\"");
    ui.zoom_to(now - 600, now - 300);
    assert_eq!(ui.window(now), (now - 600, now - 300));
    ui.reset_zoom();
    ui.zoom_around(now, now - 1800, 0.5);
    let (from, to) = ui.window(now);
    assert_eq!(to - from, 43_200);
    ui.go_now();
    assert!(ui.follows_now());
}

#[test]
fn history_labels() {
    let en = labels::load("en");
    let it = labels::load("it");
    // The copy table of docs/ui/screens/history.md.
    let table = [
        ("history.sensors", "Sensors", "Sensori"),
        ("history.range.1h", "1 h", "1 h"),
        ("history.range.24h", "24 h", "24 h"),
        ("history.range.48h", "48 h", "48 h"),
        ("history.range.7d", "7 d", "7 g"),
        ("history.range.30d", "30 d", "30 g"),
        ("history.range.1y", "1 y", "1 a"),
        ("history.now", "Now", "Adesso"),
        ("history.reset_zoom", "Reset zoom", "Reimposta lo zoom"),
        (
            "history.aggregate_hint",
            "Older than 48 hours: 1 minute mean, with minimum and maximum.",
            "Più vecchio di 48 ore: media di 1 minuto, con minimo e massimo.",
        ),
        (
            "history.gap",
            "No data: the app did not run",
            "Nessun dato: l'app non era in esecuzione",
        ),
        (
            "history.empty",
            "Select sensors to chart.",
            "Seleziona i sensori da mostrare.",
        ),
        (
            "history.no_data",
            "No data in this range. The app records the history while it runs.",
            "Nessun dato in questo intervallo. L'app registra la cronologia mentre è in esecuzione.",
        ),
        (
            "history.error",
            "The history database cannot be read.",
            "Il database della cronologia non può essere letto.",
        ),
    ];
    for (key, english, italian) in table {
        assert_eq!(en[key], english, "{key}");
        assert_eq!(it[key], italian, "{key}");
    }
    let state = full();
    let feedback = Feedback::default();
    let mut ui = HistoryUi::default();
    ui.toggle_sensor("nct6798_temp1");
    ui.cursor = Some(T0 + 50);
    let italian = HistoryModel::build(
        &ScreenInput {
            locale: IT,
            ..input(&state, &feedback)
        },
        &ui,
        &loaded(&ui, T0 + 400),
        T0 + 400,
        800,
    );
    assert_eq!(italian.sensors_label, "1 sensore");
    assert_eq!(italian.series[0].legend.value, "30,5 °C");
    assert_eq!(
        italian.series[0].legend.text,
        "Water temperature 30,5 °C, min 30,0, max 34,0"
    );
}
