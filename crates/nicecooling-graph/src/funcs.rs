//! The pure functions of the node kinds that have no state. They take numbers and give a number
//! or no value. The no-value rule for connected inputs is in `eval`.

use nicecooling_core::rules::sensor::{Comparison, compare};

use crate::model::Kind;

/// How the round node rounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoundMode {
    /// To the nearest value. A tie goes away from 0.
    Nearest,
    /// Toward minus infinity.
    Down,
    /// Toward plus infinity.
    Up,
}

impl RoundMode {
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "nearest" => Some(Self::Nearest),
            "down" => Some(Self::Down),
            "up" => Some(Self::Up),
            _ => None,
        }
    }
}

/// A result that is not finite is no value.
pub fn finite(value: f64) -> Option<f64> {
    value.is_finite().then_some(value)
}

/// All the inputs, or `None` when one has no value.
pub fn all_values<const N: usize>(inputs: [Option<f64>; N]) -> Option<[f64; N]> {
    let mut out = [0.0; N];
    for (slot, value) in out.iter_mut().zip(inputs) {
        *slot = value?;
    }
    Some(out)
}

/// A logic input is true above 0.
pub fn is_true(value: f64) -> bool {
    value > 0.0
}

/// A logic output is 1 or 0.
pub fn logic(value: bool) -> f64 {
    if value { 1.0 } else { 0.0 }
}

/// Rounds to `decimals` places. A value within 1e-9 (relative) of a tie or of a whole step counts
/// as exactly there, so `0.29` rounded down to 2 places stays `0.29`.
pub fn round_to(value: f64, decimals: u32, mode: RoundMode) -> f64 {
    let factor = 10f64.powi(i32::try_from(decimals).unwrap_or(0));
    let scaled = value * factor;
    let half_steps = (scaled * 2.0).round() / 2.0;
    let scaled = if (scaled - half_steps).abs() <= 1e-9 * scaled.abs().max(1.0) {
        half_steps
    } else {
        scaled
    };
    let rounded = match mode {
        RoundMode::Nearest => scaled.round(),
        RoundMode::Down => scaled.floor(),
        RoundMode::Up => scaled.ceil(),
    };
    rounded / factor
}

/// The value of a sensor source node: the reading with 8 decimals.
pub fn source_value(reading: f64) -> f64 {
    round_to(reading, 8, RoundMode::Nearest)
}

/// A new number of the random node, between `min` and `max`, with 3 decimals.
pub fn random_value(rng: &mut fastrand::Rng, min: f64, max: f64) -> f64 {
    let value = min + rng.f64() * (max - min);
    round_to(value, 3, RoundMode::Nearest).clamp(min, max)
}

fn logarithm(a: f64, base: f64) -> Option<f64> {
    if a <= 0.0 || base <= 0.0 || base == 1.0 {
        return None;
    }
    let result = if base == std::f64::consts::E {
        a.ln()
    } else if base == 10.0 {
        a.log10()
    } else if base == 2.0 {
        a.log2()
    } else {
        let quotient = a.ln() / base.ln();
        // log3(27) is 3.0000000000000004 as a quotient of two logarithms. A result that close to a
        // whole number is the whole number.
        let whole = quotient.round();
        if (quotient - whole).abs() <= 1e-12 {
            whole
        } else {
            quotient
        }
    };
    finite(result)
}

/// The kinds with two inputs that give a number: the arithmetic, the logarithm, the power, the six
/// comparisons, and and or. It gives `None` for a result that is not a number, and for any other
/// kind.
pub fn binary(kind: Kind, a: f64, b: f64) -> Option<f64> {
    let comparison = |c| Some(logic(compare(c, a, b, None)));
    match kind {
        Kind::Add => finite(a + b),
        Kind::Subtract => finite(a - b),
        Kind::Multiply => finite(a * b),
        Kind::Divide => (b != 0.0).then(|| a / b).and_then(finite),
        Kind::Minimum => Some(a.min(b)),
        Kind::Maximum => Some(a.max(b)),
        Kind::Average => finite((a + b) / 2.0),
        Kind::Logarithm => logarithm(a, b),
        Kind::Power => finite(a.powf(b)),
        Kind::Less => comparison(Comparison::Lt),
        Kind::LessThanOrEqual => comparison(Comparison::Le),
        Kind::Equal => comparison(Comparison::Eq),
        Kind::GreaterThanOrEqual => comparison(Comparison::Ge),
        Kind::Greater => comparison(Comparison::Gt),
        Kind::Different => comparison(Comparison::Ne),
        Kind::And => Some(logic(is_true(a) && is_true(b))),
        Kind::Or => Some(logic(is_true(a) || is_true(b))),
        _ => None,
    }
}

/// The kinds with one input and no parameter: the trigonometry in radians, the square root, not.
pub fn unary(kind: Kind, a: f64) -> Option<f64> {
    match kind {
        Kind::Sine => finite(a.sin()),
        Kind::Cosine => finite(a.cos()),
        Kind::Tangent => finite(a.tan()),
        Kind::SquareRoot => (a >= 0.0).then(|| a.sqrt()),
        Kind::Not => Some(logic(!is_true(a))),
        _ => None,
    }
}

/// The limiter node. The validator refuses a minimum above the maximum.
pub fn limiter(a: f64, min: f64, max: f64) -> f64 {
    a.max(min).min(max)
}

/// The switch node: A while S is false, B while S is true.
pub fn switch(s: f64, a: f64, b: f64) -> f64 {
    if is_true(s) { b } else { a }
}

/// The node "has value": 1 when the input has a value and 0 when it has none. It is the one kind
/// that sees a missing value.
pub fn has_value(input: Option<f64>) -> f64 {
    logic(input.is_some())
}

#[cfg(test)]
mod tests {
    use nicecooling_core::rules::sensor::SensorRule;

    use super::*;

    fn math(kind: Kind, a: f64, b: f64) -> Option<f64> {
        binary(kind, a, b)
    }

    #[test]
    fn add_sub_mul_div_table() {
        let cases = [
            (Kind::Add, 10.0, 4.0, 14.0),
            (Kind::Subtract, 10.0, 4.0, 6.0),
            (Kind::Multiply, 10.0, 4.0, 40.0),
            (Kind::Divide, 9.0, 4.0, 2.25),
            (Kind::Add, -2.0, 3.0, 1.0),
            (Kind::Subtract, 1.0, 4.0, -3.0),
            (Kind::Multiply, -2.0, 3.0, -6.0),
            (Kind::Divide, -9.0, 3.0, -3.0),
        ];
        for (kind, a, b, want) in cases {
            assert_eq!(math(kind, a, b), Some(want), "{kind} {a} {b}");
        }
        // A result that is not finite is no value.
        assert_eq!(math(Kind::Multiply, f64::MAX, 2.0), None);
        assert_eq!(math(Kind::Add, f64::MAX, f64::MAX), None);
        assert_eq!(math(Kind::Constant, 1.0, 1.0), None);
    }

    #[test]
    fn division_by_zero_gives_no_value() {
        assert_eq!(math(Kind::Divide, 4.0, 0.0), None);
        assert_eq!(math(Kind::Divide, 0.0, 0.0), None);
        assert_eq!(math(Kind::Divide, 4.0, -0.0), None);
        assert_eq!(math(Kind::Divide, 0.0, 4.0), Some(0.0));
    }

    #[test]
    fn min_max_average() {
        assert_eq!(math(Kind::Minimum, -2.0, 3.0), Some(-2.0));
        assert_eq!(math(Kind::Maximum, -2.0, 3.0), Some(3.0));
        assert_eq!(math(Kind::Average, 10.0, 4.0), Some(7.0));
        assert_eq!(math(Kind::Average, -3.0, 3.0), Some(0.0));
    }

    #[test]
    fn log_base_10() {
        assert_eq!(math(Kind::Logarithm, 1000.0, 10.0), Some(3.0));
        assert_eq!(math(Kind::Logarithm, 8.0, 2.0), Some(3.0));
        assert_eq!(math(Kind::Logarithm, 27.0, 3.0), Some(3.0));
        // The default base is e.
        assert_eq!(math(Kind::Logarithm, 1.0, std::f64::consts::E), Some(0.0));
        assert_eq!(
            math(Kind::Logarithm, std::f64::consts::E, std::f64::consts::E),
            Some(1.0)
        );
        let half = math(Kind::Logarithm, 2.0, 4.0).unwrap();
        assert!((half - 0.5).abs() < 1e-12);
    }

    #[test]
    fn log_domain_gives_no_value() {
        for (a, b) in [
            (0.0, 10.0),
            (-1.0, 10.0),
            (5.0, 0.0),
            (5.0, -2.0),
            (5.0, 1.0),
        ] {
            assert_eq!(math(Kind::Logarithm, a, b), None, "{a} {b}");
        }
    }

    #[test]
    fn power_fraction_gives_no_value() {
        assert_eq!(math(Kind::Power, -8.0, 0.5), None);
        assert_eq!(math(Kind::Power, 2.0, 10.0), Some(1024.0));
        assert_eq!(math(Kind::Power, 3.0, 2.0), Some(9.0));
        assert_eq!(math(Kind::Power, -2.0, 3.0), Some(-8.0));
        // 0 to a negative power and an overflow are not finite.
        assert_eq!(math(Kind::Power, 0.0, -1.0), None);
        assert_eq!(math(Kind::Power, 10.0, 400.0), None);
    }

    #[test]
    fn trig_in_radians() {
        assert_eq!(unary(Kind::Sine, std::f64::consts::FRAC_PI_2), Some(1.0));
        assert_eq!(unary(Kind::Sine, 0.0), Some(0.0));
        assert_eq!(unary(Kind::Cosine, 0.0), Some(1.0));
        let cos_pi = unary(Kind::Cosine, std::f64::consts::PI).unwrap();
        assert!((cos_pi + 1.0).abs() < 1e-12);
        let tan = unary(Kind::Tangent, std::f64::consts::FRAC_PI_4).unwrap();
        assert!((tan - 1.0).abs() < 1e-12);
        // 90 is not 90 degrees.
        let sin90 = unary(Kind::Sine, 90.0).unwrap();
        assert!((sin90 - 0.893_996_663_600_558).abs() < 1e-12);
    }

    #[test]
    fn sqrt_negative_gives_no_value() {
        assert_eq!(unary(Kind::SquareRoot, -4.0), None);
        assert_eq!(unary(Kind::SquareRoot, 16.0), Some(4.0));
        assert_eq!(unary(Kind::SquareRoot, 0.0), Some(0.0));
    }

    #[test]
    fn round_modes_and_ties() {
        use RoundMode::*;
        assert_eq!(round_to(2.25, 1, Nearest), 2.3);
        assert_eq!(round_to(-2.25, 1, Nearest), -2.3);
        assert_eq!(round_to(0.5, 0, Nearest), 1.0);
        assert_eq!(round_to(-0.5, 0, Nearest), -1.0);
        assert_eq!(round_to(2.5, 0, Nearest), 3.0);
        assert_eq!(round_to(-1.5, 0, Down), -2.0);
        assert_eq!(round_to(-1.5, 0, Up), -1.0);
        assert_eq!(round_to(1.9, 0, Down), 1.0);
        assert_eq!(round_to(7.141, 2, Up), 7.15);
        assert_eq!(round_to(7.141, 2, Down), 7.14);
        // A value that is a tie or a whole step in decimal stays there.
        assert_eq!(round_to(0.29, 2, Down), 0.29);
        assert_eq!(round_to(0.29, 2, Up), 0.29);
        assert_eq!(round_to(2.675, 2, Nearest), 2.68);
        assert_eq!(round_to(1.005, 2, Nearest), 1.01);
        assert_eq!(round_to(12345.6789, 8, Nearest), 12345.6789);
        assert_eq!(RoundMode::parse("up"), Some(Up));
        assert_eq!(RoundMode::parse("truncate"), None);
    }

    #[test]
    fn limiter_range() {
        assert_eq!(limiter(95.0, 0.0, 80.0), 80.0);
        assert_eq!(limiter(42.0, 0.0, 80.0), 42.0);
        assert_eq!(limiter(-5.0, 0.0, 80.0), 0.0);
        assert_eq!(limiter(7.0, 7.0, 7.0), 7.0);
    }

    #[test]
    fn equal_uses_tolerance() {
        let node_sum = 0.1 + 0.2;
        assert_eq!(math(Kind::Equal, node_sum, 0.3), Some(1.0));
        assert_eq!(math(Kind::Equal, 5.0, 5.000_001), Some(0.0));
        assert_eq!(math(Kind::Different, 5.0, 5.0), Some(0.0));
        assert_eq!(math(Kind::Different, 5.0, 5.000_001), Some(1.0));
        assert_eq!(math(Kind::Greater, 40.1, 40.0), Some(1.0));
        // The other four compare exactly.
        assert_eq!(math(Kind::Greater, 50.000_000_000_1, 50.0), Some(1.0));
        assert_eq!(math(Kind::Less, 50.0, 50.000_000_000_1), Some(1.0));
    }

    #[test]
    fn compare_same_as_profile_rule() {
        let pairs = [
            (Kind::Less, Comparison::Lt),
            (Kind::LessThanOrEqual, Comparison::Le),
            (Kind::Equal, Comparison::Eq),
            (Kind::GreaterThanOrEqual, Comparison::Ge),
            (Kind::Greater, Comparison::Gt),
            (Kind::Different, Comparison::Ne),
        ];
        let readings = [49.0, 49.999_999_999_9, 50.0, 50.000_000_000_1, 50.1, 51.0];
        for (kind, comparison) in pairs {
            let rule = SensorRule {
                sensor: "s".into(),
                comparison,
                value: 50.0,
                value_max: None,
                hold_s: 0,
            };
            for reading in readings {
                assert_eq!(
                    math(kind, reading, 50.0),
                    Some(logic(rule.compares(reading))),
                    "{kind} {reading}"
                );
            }
        }
    }

    #[test]
    fn logic_threshold_above_zero() {
        assert_eq!(math(Kind::And, -1.0, 1.0), Some(0.0));
        assert_eq!(math(Kind::And, 1.0, 1.0), Some(1.0));
        assert_eq!(math(Kind::And, 1.0, 0.0), Some(0.0));
        assert_eq!(math(Kind::Or, 1.0, 0.0), Some(1.0));
        assert_eq!(math(Kind::Or, 0.001, 0.0), Some(1.0));
        assert_eq!(math(Kind::Or, -3.0, 0.0), Some(0.0));
        assert_eq!(unary(Kind::Not, 0.0), Some(1.0));
        assert_eq!(unary(Kind::Not, 0.5), Some(0.0));
        assert_eq!(unary(Kind::Not, -1.0), Some(1.0));
        assert_eq!(switch(1.0, 10.0, 20.0), 20.0);
        assert_eq!(switch(0.0, 10.0, 20.0), 10.0);
        assert_eq!(switch(-1.0, 10.0, 20.0), 10.0);
        assert_eq!(switch(0.001, 10.0, 20.0), 20.0);
    }

    #[test]
    fn switch_with_no_value_input() {
        // S = 0, A = 10 and B has no value: no value, although B is not the one that is chosen.
        let inputs = all_values([Some(0.0), Some(10.0), None]);
        assert_eq!(inputs, None);
        let [s, a, b] = all_values([Some(0.0), Some(10.0), Some(20.0)]).unwrap();
        assert_eq!(switch(s, a, b), 10.0);
        assert_eq!(all_values::<0>([]), Some([]));
    }

    #[test]
    fn has_value_is_the_exception() {
        assert_eq!(has_value(Some(12.0)), 1.0);
        assert_eq!(has_value(Some(0.0)), 1.0);
        assert_eq!(has_value(None), 0.0);
    }

    #[test]
    fn random_in_range_3_decimals() {
        let mut rng = fastrand::Rng::with_seed(7);
        let (mut low, mut high) = (f64::MAX, f64::MIN);
        for _ in 0..1000 {
            let v = random_value(&mut rng, 10.0, 20.0);
            assert!((10.0..=20.0).contains(&v), "{v}");
            assert!(((v * 1000.0).round() - v * 1000.0).abs() < 1e-6, "{v}");
            low = low.min(v);
            high = high.max(v);
        }
        // The values cover the range.
        assert!(low < 11.0 && high > 19.0, "{low} {high}");
        // The same seed gives the same numbers.
        let mut a = fastrand::Rng::with_seed(9);
        let mut b = fastrand::Rng::with_seed(9);
        assert_eq!(
            random_value(&mut a, -5.0, 5.0),
            random_value(&mut b, -5.0, 5.0)
        );
        assert_eq!(random_value(&mut a, 2.5, 2.5), 2.5);
    }

    #[test]
    fn source_rounded_to_8_decimals() {
        assert_eq!(source_value(46.123_456_789), 46.123_456_79);
        assert_eq!(source_value(46.5), 46.5);
        assert_eq!(source_value(-0.000_000_004), 0.0);
        assert_eq!(source_value(1e-9), 0.0);
    }
}
