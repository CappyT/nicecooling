//! The pump runtime per day and the time of the last fill level measurement on the Overview tab.

mod support;

use chrono::{Days, Local, NaiveDate, TimeZone as _};
use nicecooling::i18n::{EN, IT, Locale};
use nicecooling::model::{AppState, DeviceEvent, FieldValue, RefillEntry};
use nicecooling::screens::format::clock_minutes;
use nicecooling::screens::leakshield::overview::{
    DAY_CAP_S, DayLevel, OverviewModel, OverviewUi, PUMP_LIMIT_FIELD, RUNTIME_DAYS, day_bars,
};
use nicecooling::screens::{Feedback, ScreenInput};
use support::fixtures::{LEAKSHIELD, with_leakshield};

fn at_noon(date: NaiveDate) -> u64 {
    let time = date.and_hms_opt(12, 0, 0).unwrap();
    Local
        .from_local_datetime(&time)
        .earliest()
        .unwrap()
        .timestamp_micros() as u64
}

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 24).unwrap()
}

fn days_ago(days: u64) -> NaiveDate {
    today().checked_sub_days(Days::new(days)).unwrap()
}

fn refill(date: NaiveDate, duration_s: u32) -> RefillEntry {
    RefillEntry {
        time_us: at_noon(date),
        duration_s,
        pump_turns: 10,
        volume_ml: 5.0,
    }
}

fn model(state: &AppState, locale: Locale) -> OverviewModel {
    let feedback = Feedback::default();
    let input = ScreenInput {
        state,
        now_us: at_noon(today()) + 3_600_000_000,
        locale,
        down_since_us: None,
        feedback: &feedback,
    };
    OverviewModel::build(&input, &OverviewUi::default(), LEAKSHIELD)
}

fn with_refills(refills: Vec<RefillEntry>) -> AppState {
    let mut state = with_leakshield();
    state
        .snapshot
        .leakshield_data
        .get_mut(LEAKSHIELD)
        .unwrap()
        .refill_history = refills;
    state
}

#[test]
fn the_chart_sums_the_durations_of_a_local_day_and_ends_today() {
    let refills = vec![
        refill(days_ago(2), 12),
        refill(days_ago(2), 8),
        refill(days_ago(0), 5),
        // Outside the 14 days.
        refill(days_ago(14), 99),
    ];
    let bars = day_bars(&refills, at_noon(today()) + 3_600_000_000);
    assert_eq!(bars.len() as i64, RUNTIME_DAYS);
    assert_eq!(bars.last().unwrap().date, today());
    assert_eq!(bars.first().unwrap().date, days_ago(13));
    let seconds: Vec<u32> = bars.iter().map(|b| b.seconds).collect();
    assert_eq!(seconds[13], 5);
    assert_eq!(seconds[11], 20, "two refills of the same day add up");
    assert_eq!(seconds.iter().sum::<u32>(), 25);
    assert_eq!(bars[13].label, "24");
}

#[test]
fn a_day_has_a_cap_and_three_classes() {
    let bars = day_bars(
        &[refill(days_ago(1), 700), refill(days_ago(1), 700)],
        at_noon(today()),
    );
    assert_eq!(bars[12].seconds, DAY_CAP_S);
    assert_eq!(DAY_CAP_S, 1000);
    // Up to 30 s is normal, above 30 s concerning, above 60 s critical.
    assert_eq!(DayLevel::of(30), DayLevel::Normal);
    assert_eq!(DayLevel::of(31), DayLevel::Concerning);
    assert_eq!(DayLevel::of(60), DayLevel::Concerning);
    assert_eq!(DayLevel::of(61), DayLevel::Critical);
    assert_eq!(bars[12].level, DayLevel::Critical);
}

#[test]
fn the_overview_shows_the_days_and_the_empty_text() {
    let state = with_refills(Vec::new());
    let shown = model(&state, EN);
    assert_eq!(shown.charts.runtime_days.len(), 14);
    assert_eq!(
        shown.charts.runtime_empty.as_deref(),
        Some("No refill in the last 14 days.")
    );
    let state = with_refills(vec![refill(days_ago(1), 42)]);
    let shown = model(&state, EN);
    assert_eq!(shown.charts.runtime_empty, None);
    assert_eq!(shown.charts.runtime_days[12].seconds, 42);
    assert_eq!(shown.charts.runtime_days[12].level, DayLevel::Concerning);
    assert_eq!(
        model(&with_refills(Vec::new()), IT)
            .charts
            .runtime_empty
            .as_deref(),
        Some("Nessun Rabbocco negli ultimi 14 giorni.")
    );
}

#[test]
fn the_limit_line_comes_from_the_field_of_the_device() {
    let mut state = with_refills(Vec::new());
    assert_eq!(model(&state, EN).charts.runtime_limit, None);
    let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
    data.fields.push(nicecooling::model::FieldEntry {
        id: PUMP_LIMIT_FIELD.into(),
        risk: nicecooling::model::FieldRisk::Safety,
        value: FieldValue::Number(5.0),
        writable: true,
        reason: String::new(),
    });
    assert_eq!(
        model(&state, EN).charts.runtime_limit,
        Some((5.0, "Warning limit: 5 s per day".to_owned()))
    );
    assert_eq!(
        model(&state, IT).charts.runtime_limit,
        Some((5.0, "Limite di avviso: 5 s al giorno".to_owned()))
    );
}

fn event(id: &str, time_us: u64) -> DeviceEvent {
    DeviceEvent {
        time_us,
        id: id.into(),
        details: Default::default(),
    }
}

#[test]
fn the_measured_time_is_the_time_of_the_newest_measurement_event() {
    let mut state = with_leakshield();
    let data = state.snapshot.leakshield_data.get_mut(LEAKSHIELD).unwrap();
    let (old, new) = (1_700_000_000_000_000, 1_700_050_000_000_000);
    data.events = vec![
        event("leakshield.fill-level-measurement-start", new + 10_000_000),
        event("leakshield.fill-level-measurement-finished", new),
        event("leakshield.fill-level-measurement-finished", old),
    ];
    let rows = model(&state, EN).status.rows;
    assert_eq!(
        rows[0].caption,
        Some(format!("measured {}", clock_minutes(new)))
    );
    // The same time with the events in the other order.
    state
        .snapshot
        .leakshield_data
        .get_mut(LEAKSHIELD)
        .unwrap()
        .events
        .reverse();
    assert_eq!(
        model(&state, EN).status.rows[0].caption,
        Some(format!("measured {}", clock_minutes(new)))
    );
}

#[test]
fn without_a_measurement_event_the_row_shows_no_time() {
    let mut state = with_leakshield();
    state
        .snapshot
        .leakshield_data
        .get_mut(LEAKSHIELD)
        .unwrap()
        .events = vec![event("leakshield.fill-level-measurement-start", 1)];
    // The sensors have a sample time. It is not the time of a measurement.
    assert!(state.sensors.is_some());
    assert_eq!(model(&state, EN).status.rows[0].caption, None);
}
