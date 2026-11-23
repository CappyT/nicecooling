use std::fmt;

/// The default width of the Input moving average, in seconds.
pub const DEFAULT_SMOOTHING_S: f64 = 5.0;
/// The default Duty step limit, in percent per second.
pub const DEFAULT_STEP_LIMIT: f64 = 5.0;

/// The Duty that every fail-safe path of the tick uses, in percent.
pub const FAIL_SAFE_DUTY: f64 = 100.0;

/// The widest Input moving average, in seconds.
pub const MAX_SMOOTHING_S: f64 = 600.0;

/// A mapping from one Input to the Duty of one Channel.
#[derive(Clone, Debug, PartialEq)]
pub struct Curve {
    pub input: String,
    /// `(input, duty)` sorted by input.
    points: Vec<(f64, f64)>,
    pub smoothing_s: f64,
    /// Percent per second.
    pub step_limit: f64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CurveError {
    TooFewPoints,
    NotFinite,
    NotSorted,
    DutyOutOfRange,
    SmoothingOutOfRange,
    StepLimitOutOfRange,
}

impl fmt::Display for CurveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewPoints => write!(f, "a Curve needs at least 2 points"),
            Self::NotFinite => write!(f, "the points must be finite numbers"),
            Self::NotSorted => write!(f, "the points must increase in Input"),
            Self::DutyOutOfRange => write!(f, "a Duty is a percent from 0 to 100"),
            Self::SmoothingOutOfRange => {
                write!(f, "the smoothing is 0 to {MAX_SMOOTHING_S} seconds")
            }
            Self::StepLimitOutOfRange => write!(f, "the step limit is a number above 0"),
        }
    }
}

impl std::error::Error for CurveError {}

impl Curve {
    pub fn new(input: impl Into<String>, points: Vec<(f64, f64)>) -> Result<Self, CurveError> {
        if points.len() < 2 {
            return Err(CurveError::TooFewPoints);
        }
        if points.iter().any(|p| !p.0.is_finite() || !p.1.is_finite()) {
            return Err(CurveError::NotFinite);
        }
        if points.windows(2).any(|w| w[0].0 >= w[1].0) {
            return Err(CurveError::NotSorted);
        }
        if points.iter().any(|p| !(0.0..=100.0).contains(&p.1)) {
            return Err(CurveError::DutyOutOfRange);
        }
        Ok(Self {
            input: input.into(),
            points,
            smoothing_s: DEFAULT_SMOOTHING_S,
            step_limit: DEFAULT_STEP_LIMIT,
        })
    }

    /// Replaces the smoothing and the step limit, each when given. A value that would panic or
    /// freeze the Duty (negative, zero step, not finite, above `MAX_SMOOTHING_S`) is an error and
    /// changes nothing.
    pub fn set_limits(
        &mut self,
        smoothing_s: Option<f64>,
        step_limit: Option<f64>,
    ) -> Result<(), CurveError> {
        if smoothing_s.is_some_and(|s| !(0.0..=MAX_SMOOTHING_S).contains(&s)) {
            return Err(CurveError::SmoothingOutOfRange);
        }
        if step_limit.is_some_and(|s| !(s.is_finite() && s > 0.0)) {
            return Err(CurveError::StepLimitOutOfRange);
        }
        self.smoothing_s = smoothing_s.unwrap_or(self.smoothing_s);
        self.step_limit = step_limit.unwrap_or(self.step_limit);
        Ok(())
    }

    pub fn points(&self) -> &[(f64, f64)] {
        &self.points
    }

    /// The Duty for an Input value. It is linear between points and equals the
    /// Duty of the nearest point outside them. A NaN Input gives 100.
    pub fn duty_at(&self, input: f64) -> f64 {
        let first = self.points[0];
        let last = self.points[self.points.len() - 1];
        if input <= first.0 {
            return first.1;
        }
        if input >= last.0 {
            return last.1;
        }
        let Some(w) = self.points.windows(2).find(|w| input <= w[1].0) else {
            return FAIL_SAFE_DUTY;
        };
        w[0].1 + (input - w[0].0) * (w[1].1 - w[0].1) / (w[1].0 - w[0].0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve() -> Curve {
        Curve::new("t", vec![(40.0, 30.0), (60.0, 70.0)]).unwrap()
    }

    #[test]
    fn interpolation() {
        assert_eq!(curve().duty_at(50.0), 50.0);
        assert_eq!(curve().duty_at(45.0), 40.0);
    }

    #[test]
    fn clamp_both_ends() {
        assert_eq!(curve().duty_at(10.0), 30.0);
        assert_eq!(curve().duty_at(90.0), 70.0);
    }

    #[test]
    fn limits_reject_values_that_panic_or_freeze() {
        let mut c = curve();
        for bad in [-1.0, f64::NAN, f64::INFINITY, 1e300, 600.1] {
            assert_eq!(
                c.set_limits(Some(bad), None),
                Err(CurveError::SmoothingOutOfRange),
                "smoothing {bad}"
            );
        }
        for bad in [-5.0, 0.0, f64::NAN, f64::INFINITY] {
            assert_eq!(
                c.set_limits(None, Some(bad)),
                Err(CurveError::StepLimitOutOfRange),
                "step {bad}"
            );
        }
        assert_eq!((c.smoothing_s, c.step_limit), (5.0, 5.0));
        assert_eq!(c.set_limits(Some(0.0), Some(0.1)), Ok(()));
        assert_eq!(c.set_limits(Some(600.0), Some(1e9)), Ok(()));
        assert_eq!((c.smoothing_s, c.step_limit), (600.0, 1e9));
        assert_eq!(c.set_limits(None, None), Ok(()));
    }

    #[test]
    fn rejects_non_finite_points() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for points in [
                vec![(bad, 10.0), (60.0, 20.0)],
                vec![(40.0, 10.0), (bad, 20.0)],
                vec![(40.0, bad), (60.0, 20.0)],
                vec![(40.0, 10.0), (60.0, bad)],
            ] {
                assert!(Curve::new("t", points.clone()).is_err(), "{points:?}");
            }
        }
        assert_eq!(
            Curve::new("t", vec![(f64::NAN, 10.0), (f64::NAN, 20.0)]),
            Err(CurveError::NotFinite)
        );
        assert_eq!(
            Curve::new("t", vec![(f64::NEG_INFINITY, 20.0), (80.0, 100.0)]),
            Err(CurveError::NotFinite)
        );
    }

    #[test]
    fn rejects_bad_points() {
        assert_eq!(
            Curve::new("t", vec![(1.0, 1.0)]),
            Err(CurveError::TooFewPoints)
        );
        assert_eq!(
            Curve::new("t", vec![(2.0, 1.0), (1.0, 2.0)]),
            Err(CurveError::NotSorted)
        );
        assert_eq!(
            Curve::new("t", vec![(1.0, 1.0), (2.0, 101.0)]),
            Err(CurveError::DutyOutOfRange)
        );
    }
}
