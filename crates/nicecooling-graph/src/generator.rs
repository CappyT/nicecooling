//! The five signal generators. All of them have the inputs enable, reset and period, and a phase
//! `t` in seconds that advances with the clock while enable is true.

use std::time::Duration;

use crate::funcs::is_true;
use crate::state::normalize_period;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Rises once from 0 to 1 and stays at 1.
    Linear,
    Sawtooth,
    Triangle,
    Sine,
    Rectangle,
}

/// The output of a generator at phase `t` with a period in seconds. The period is the one that
/// `normalize_period` gives.
pub fn shape_value(shape: Shape, t: f64, period: f64) -> f64 {
    // The remainder of the division, so that t = 14 s and a period of 10 s is exactly 0.4.
    let u = (t % period) / period;
    match shape {
        Shape::Linear => (t / period).min(1.0),
        Shape::Sawtooth => u,
        Shape::Triangle if u < 0.5 => 2.0 * u,
        Shape::Triangle => 2.0 * (1.0 - u),
        Shape::Sine => (1.0 + (std::f64::consts::TAU * t / period).sin()) / 2.0,
        Shape::Rectangle if u < 0.5 => 1.0,
        Shape::Rectangle => 0.0,
    }
}

/// The phase of one generator node.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Generator {
    t: f64,
    last: Option<Duration>,
}

impl Generator {
    /// The phase in seconds.
    pub fn phase(&self) -> f64 {
        self.t
    }

    /// One evaluation at `now`. Reset true sets the phase to 0. While enable is false the output
    /// is 0 and the phase holds. An input with no value gives no value, and the phase does not
    /// advance in that tick.
    pub fn step(
        &mut self,
        shape: Shape,
        enable: Option<f64>,
        reset: Option<f64>,
        period: Option<f64>,
        now: Duration,
    ) -> Option<f64> {
        let elapsed = self
            .last
            .map_or(0.0, |last| now.saturating_sub(last).as_secs_f64());
        self.last = Some(now);
        let (enable, reset, period) = (is_true(enable?), is_true(reset?), period?);
        if reset {
            self.t = 0.0;
        } else if enable {
            self.t += elapsed;
        }
        Some(if enable {
            shape_value(shape, self.t, normalize_period(period))
        } else {
            0.0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn shapes_at_t4_period10() {
        assert!(close(shape_value(Shape::Linear, 4.0, 10.0), 0.4));
        assert!(close(shape_value(Shape::Sawtooth, 4.0, 10.0), 0.4));
        assert!(close(shape_value(Shape::Triangle, 4.0, 10.0), 0.8));
        let sine = shape_value(Shape::Sine, 4.0, 10.0);
        assert!((sine - 0.793_892_626).abs() < 1e-6, "{sine}");
        assert_eq!(shape_value(Shape::Rectangle, 4.0, 10.0), 1.0);
        // The triangle comes down again in the second half.
        assert!(close(shape_value(Shape::Triangle, 7.5, 10.0), 0.5));
        assert!(close(shape_value(Shape::Triangle, 10.0, 10.0), 0.0));
    }

    #[test]
    fn second_period_repeats_and_linear_stays_1() {
        assert!(close(shape_value(Shape::Sawtooth, 14.0, 10.0), 0.4));
        assert_eq!(shape_value(Shape::Linear, 14.0, 10.0), 1.0);
        assert_eq!(shape_value(Shape::Linear, 10.0, 10.0), 1.0);
        assert!(close(shape_value(Shape::Triangle, 14.0, 10.0), 0.8));
        assert_eq!(shape_value(Shape::Rectangle, 14.0, 10.0), 1.0);
        let sine = shape_value(Shape::Sine, 14.0, 10.0);
        assert!((sine - shape_value(Shape::Sine, 4.0, 10.0)).abs() < 1e-9);
    }

    #[test]
    fn rectangle_second_half() {
        assert_eq!(shape_value(Shape::Rectangle, 7.0, 10.0), 0.0);
        assert_eq!(shape_value(Shape::Rectangle, 5.0, 10.0), 0.0);
        assert_eq!(shape_value(Shape::Rectangle, 4.9, 10.0), 1.0);
        assert_eq!(shape_value(Shape::Rectangle, 17.0, 10.0), 0.0);
    }

    #[test]
    fn sine_starts_at_half() {
        assert_eq!(shape_value(Shape::Sine, 0.0, 10.0), 0.5);
        let mut generator = Generator::default();
        let out = generator.step(
            Shape::Sine,
            Some(1.0),
            Some(0.0),
            Some(10.0),
            Duration::ZERO,
        );
        assert_eq!(out, Some(0.5));
        // At a quarter period the sine is at the top, at the three quarters at the bottom.
        assert!(close(shape_value(Shape::Sine, 2.5, 10.0), 1.0));
        assert!(close(shape_value(Shape::Sine, 7.5, 10.0), 0.0));
    }

    #[test]
    fn period_rounds_to_tenth_with_floor() {
        // The period 0.04 s is 0.1 s. A tick of 250 ms is half a period after the first tick.
        let mut generator = Generator::default();
        let tick = Duration::from_millis(250);
        generator.step(
            Shape::Sawtooth,
            Some(1.0),
            Some(0.0),
            Some(0.04),
            Duration::ZERO,
        );
        let out = generator.step(Shape::Sawtooth, Some(1.0), Some(0.0), Some(0.04), tick);
        assert!(close(out.unwrap(), 0.5), "{out:?}");
        // The period 0.26 s is 0.3 s.
        let mut generator = Generator::default();
        generator.step(
            Shape::Linear,
            Some(1.0),
            Some(0.0),
            Some(0.26),
            Duration::ZERO,
        );
        let out = generator.step(
            Shape::Linear,
            Some(1.0),
            Some(0.0),
            Some(0.26),
            Duration::from_millis(150),
        );
        assert!(close(out.unwrap(), 0.5), "{out:?}");
        // A negative period and a period that is not a number give the floor too.
        let mut generator = Generator::default();
        let out = generator.step(
            Shape::Sawtooth,
            Some(1.0),
            Some(0.0),
            Some(-5.0),
            Duration::ZERO,
        );
        assert_eq!(out, Some(0.0));
    }

    #[test]
    fn disabled_generator_holds_phase() {
        let mut generator = Generator::default();
        let secs = Duration::from_secs;
        for t in 0..=4 {
            generator.step(Shape::Linear, Some(1.0), Some(0.0), Some(10.0), secs(t));
        }
        assert_eq!(generator.phase(), 4.0);
        // Disabled: the output is 0 and t stays 4 s, however long it takes.
        for t in 5..9 {
            let out = generator.step(Shape::Linear, Some(0.0), Some(0.0), Some(10.0), secs(t));
            assert_eq!(out, Some(0.0));
        }
        assert_eq!(generator.phase(), 4.0);
        // Enabled again: it goes on from 4 s plus the time of that tick.
        let out = generator.step(Shape::Linear, Some(1.0), Some(0.0), Some(10.0), secs(9));
        assert_eq!(generator.phase(), 5.0);
        assert_eq!(out, Some(0.5));
        // An input with no value gives no value and holds the phase.
        assert_eq!(
            generator.step(Shape::Linear, None, Some(0.0), Some(10.0), secs(10)),
            None
        );
        assert_eq!(generator.phase(), 5.0);
    }

    #[test]
    fn reset_sets_phase_zero() {
        let mut generator = Generator::default();
        let secs = Duration::from_secs;
        for t in 0..=4 {
            generator.step(Shape::Sawtooth, Some(1.0), Some(0.0), Some(10.0), secs(t));
        }
        assert_eq!(generator.phase(), 4.0);
        let out = generator.step(Shape::Sawtooth, Some(1.0), Some(1.0), Some(10.0), secs(5));
        assert_eq!(generator.phase(), 0.0);
        assert_eq!(out, Some(0.0));
        // The phase advances again when reset is false.
        generator.step(Shape::Sawtooth, Some(1.0), Some(0.0), Some(10.0), secs(6));
        assert_eq!(generator.phase(), 1.0);
        // Reset also works while the generator is disabled.
        generator.step(Shape::Sawtooth, Some(0.0), Some(1.0), Some(10.0), secs(7));
        assert_eq!(generator.phase(), 0.0);
    }
}
