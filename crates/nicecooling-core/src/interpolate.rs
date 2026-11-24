//! The one interpolation function of the Curve, the Curve editor and the table node of a Graph.

/// How the output moves between two pairs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Interpolation {
    /// A straight line between the two neighbor pairs.
    #[default]
    Linear,
    /// The output of the lower pair holds until the next pair.
    Step,
}

/// The output for `input`, from `pairs` sorted by input.
///
/// Below the first pair and above the last pair the output is the first and the last output.
/// An input that is not a number, and an empty list, give no result.
pub fn interpolate(pairs: &[(f64, f64)], mode: Interpolation, input: f64) -> Option<f64> {
    if input.is_nan() {
        return None;
    }
    let first = pairs.first()?;
    let last = pairs.last()?;
    if input <= first.0 {
        return Some(first.1);
    }
    if input >= last.0 {
        return Some(last.1);
    }
    let w = pairs.windows(2).find(|w| input <= w[1].0)?;
    Some(match mode {
        Interpolation::Step if input < w[1].0 => w[0].1,
        Interpolation::Step => w[1].1,
        Interpolation::Linear => w[0].1 + (input - w[0].0) * (w[1].1 - w[0].1) / (w[1].0 - w[0].0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAIRS: [(f64, f64); 2] = [(20.0, 30.0), (40.0, 80.0)];

    #[test]
    fn linear_between_pairs() {
        assert_eq!(interpolate(&PAIRS, Interpolation::Linear, 30.0), Some(55.0));
        assert_eq!(interpolate(&PAIRS, Interpolation::Linear, 40.0), Some(80.0));
        let three = [(0.0, 0.0), (10.0, 10.0), (20.0, 40.0)];
        assert_eq!(interpolate(&three, Interpolation::Linear, 15.0), Some(25.0));
    }

    #[test]
    fn step_takes_lower_pair() {
        assert_eq!(interpolate(&PAIRS, Interpolation::Step, 39.9), Some(30.0));
        assert_eq!(interpolate(&PAIRS, Interpolation::Step, 20.1), Some(30.0));
        // At a pair input the output of that pair holds.
        assert_eq!(interpolate(&PAIRS, Interpolation::Step, 40.0), Some(80.0));
    }

    #[test]
    fn clamps_both_ends() {
        for mode in [Interpolation::Linear, Interpolation::Step] {
            assert_eq!(interpolate(&PAIRS, mode, 10.0), Some(30.0));
            assert_eq!(interpolate(&PAIRS, mode, 20.0), Some(30.0));
            assert_eq!(interpolate(&PAIRS, mode, 90.0), Some(80.0));
            assert_eq!(interpolate(&PAIRS, mode, f64::NEG_INFINITY), Some(30.0));
            assert_eq!(interpolate(&PAIRS, mode, f64::INFINITY), Some(80.0));
        }
    }

    #[test]
    fn nan_input_gives_no_result() {
        for mode in [Interpolation::Linear, Interpolation::Step] {
            assert_eq!(interpolate(&PAIRS, mode, f64::NAN), None);
        }
    }

    #[test]
    fn empty_list_gives_no_result() {
        for mode in [Interpolation::Linear, Interpolation::Step] {
            assert_eq!(interpolate(&[], mode, 5.0), None);
        }
    }
}
