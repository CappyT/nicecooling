use std::fmt;

/// The default width of the Input moving average, in seconds.
pub const DEFAULT_SMOOTHING_S: f64 = 5.0;
/// The default Duty step limit, in percent per second.
pub const DEFAULT_STEP_LIMIT: f64 = 5.0;

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
    NotSorted,
    DutyOutOfRange,
}

impl fmt::Display for CurveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewPoints => write!(f, "a Curve needs at least 2 points"),
            Self::NotSorted => write!(f, "the points must increase in Input"),
            Self::DutyOutOfRange => write!(f, "a Duty is a percent from 0 to 100"),
        }
    }
}

impl std::error::Error for CurveError {}

impl Curve {
    pub fn new(input: impl Into<String>, points: Vec<(f64, f64)>) -> Result<Self, CurveError> {
        if points.len() < 2 {
            return Err(CurveError::TooFewPoints);
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

    pub fn points(&self) -> &[(f64, f64)] {
        &self.points
    }

    /// The Duty for an Input value. It is linear between points and equals the
    /// Duty of the nearest point outside them.
    pub fn duty_at(&self, input: f64) -> f64 {
        let first = self.points[0];
        let last = self.points[self.points.len() - 1];
        if input <= first.0 {
            return first.1;
        }
        if input >= last.0 {
            return last.1;
        }
        let w = self.points.windows(2).find(|w| input <= w[1].0).unwrap();
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
