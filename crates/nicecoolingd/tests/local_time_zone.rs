//! The time rules follow a change of the system time zone and the daylight saving time. One test
//! in this binary reads and writes `TZ`, so that no other thread reads the environment.

use nicecooling_core::rules::time::TimeRule;
use nicecoolingd::rules::local::local_time;

/// 2024-01-01 12:00 UTC.
const NEW_YEAR_NOON: i64 = 1_704_110_400;

fn set_zone(zone: &str) {
    // SAFETY: the only test of this binary. No other thread reads the environment.
    unsafe { std::env::set_var("TZ", zone) };
}

#[test]
fn local_time_follows_a_time_zone_change_and_daylight_saving() {
    set_zone("UTC0");
    let utc = local_time(NEW_YEAR_NOON);
    assert_eq!((utc.weekday, utc.minute), (1, 12 * 60));
    set_zone("JST-9");
    let tokyo = local_time(NEW_YEAR_NOON);
    assert_eq!(
        (tokyo.weekday, tokyo.minute),
        (1, 21 * 60),
        "still {utc:?} after the change"
    );

    // Central Europe. 2024-03-31 01:00 UTC is the end of 02:00 CET and the start of 03:00 CEST.
    set_zone("CET-1CEST,M3.5.0,M10.5.0/3");
    let before = local_time(1_711_846_800 - 1);
    let after = local_time(1_711_846_800);
    assert_eq!((before.weekday, before.minute), (7, 2 * 60 - 1));
    assert_eq!((after.weekday, after.minute), (7, 3 * 60));
    // 2025-01-01 23:30 UTC is a Wednesday. In winter it is already Thursday 00:30 in CET.
    let thursday = local_time(1_735_689_600 + 23 * 3600 + 30 * 60);
    assert_eq!((thursday.weekday, thursday.minute), (4, 30));

    // A window that starts at 02:00 and ends at 03:00 on Sunday. On the day when the clock jumps
    // from 02:00 to 03:00 it never holds. On the day when the clock goes back, it holds twice.
    let window = TimeRule {
        days: vec![7],
        start: 2 * 60,
        end: 3 * 60,
    };
    let minutes_holding = |day_start: i64| {
        (0..24 * 60)
            .filter(|minute| window.holds(local_time(day_start + minute * 60)))
            .count()
    };
    // 2024-03-30 22:00 UTC to 2024-03-31 22:00 UTC covers the local Sunday 2024-03-31.
    assert_eq!(minutes_holding(1_711_836_000), 0);
    // 2024-10-26 22:00 UTC to 2024-10-27 22:00 UTC covers the local Sunday 2024-10-27.
    assert_eq!(minutes_holding(1_729_980_000), 120);
}
