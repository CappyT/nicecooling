//! Review reproducers: `round_to` moves every value within 1e-9 x the scaled value of a half step
//! onto that half step. At 8 decimals this covers every value from about 2.5 up, so the round
//! node, the sensor source node and the window filters give a number one step off.

use nicecooling_graph::funcs::{RoundMode, round_to, source_value};
use nicecooling_graph::window::{Filter, Stat};
use std::time::Duration;

/// Spec "Round node": the down mode rounds toward minus infinity, so the result is never above A.
#[test]
fn round_down_never_exceeds_the_input() {
    let a = 1.123_456_789;
    let out = round_to(a, 8, RoundMode::Down);
    assert!(out <= a, "down of {a} gave {out}");
    assert_eq!(out, 1.123_456_78);
}

/// Spec "Round node": the up mode is never below A.
#[test]
fn round_up_never_falls_below_the_input() {
    let a = 46.123_456_781;
    let out = round_to(a, 8, RoundMode::Up);
    assert!(out >= a, "up of {a} gave {out}");
    assert_eq!(out, 46.123_456_79);
}

/// Spec "Sensor source node": the value is the reading rounded to 8 decimals.
#[test]
fn source_rounds_to_the_nearest_8_decimals() {
    assert_eq!(source_value(46.123_456_782_6), 46.123_456_78);
}

/// Spec "Round node": the nearest mode with 0 decimals.
#[test]
fn round_nearest_of_a_large_value() {
    assert_eq!(round_to(3e9 + 0.3, 0, RoundMode::Nearest), 3e9);
}

/// Spec "Window filter nodes": the average of the samples, rounded to 5 decimals. A fan at
/// 3000.000003 rpm averages to 3000.
#[test]
fn window_average_of_a_fan_speed() {
    let mut f = Filter::default();
    let out = f.step(Stat::Average, Some(3_000.000_003), 10, Duration::ZERO);
    assert_eq!(out, Some(3000.0));
}
