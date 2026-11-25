//! The time window of the filter nodes: peak, minimum, average, low pass and trend. A window
//! gets one sample per whole second while its input has a value, and it drops every sample that is older
//! than its span at each evaluation.

use std::collections::VecDeque;
use std::time::Duration;

use crate::funcs::{RoundMode, round_to};

/// What a filter node computes from its window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stat {
    Peak,
    Minimum,
    Average,
    Trend,
    /// The damping is a percent from 0 to 99.99.
    LowPass {
        damping: f64,
    },
}

/// The samples of one window, oldest first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Window {
    samples: VecDeque<(Duration, f64)>,
}

impl Window {
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Drops every sample that is older than `span` at `now`. A sample that is exactly `span`
    /// old stays: a window of 3 s holds 4 samples at one per second.
    pub fn age(&mut self, span: Duration, now: Duration) {
        while self
            .samples
            .front()
            .is_some_and(|(at, _)| now.saturating_sub(*at) > span)
        {
            self.samples.pop_front();
        }
    }

    /// Adds a sample. A second sample in the same whole second replaces the first, so the window
    /// holds at most its span in seconds plus 1 samples at any call rate.
    pub fn push(&mut self, now: Duration, value: f64) {
        if self
            .samples
            .back()
            .is_some_and(|(at, _)| at.as_secs() == now.as_secs())
        {
            self.samples.pop_back();
        }
        self.samples.push_back((now, value));
    }

    fn values(&self) -> impl Iterator<Item = f64> + Clone + '_ {
        self.samples.iter().map(|(_, v)| *v)
    }

    pub fn peak(&self) -> Option<f64> {
        self.values().reduce(f64::max)
    }

    pub fn minimum(&self) -> Option<f64> {
        self.values().reduce(f64::min)
    }

    pub fn mean(&self) -> Option<f64> {
        mean_of(self.values(), self.len())
    }

    /// The mean of the samples after the lowest and the highest are dropped in pairs.
    pub fn trimmed_mean(&self) -> Option<f64> {
        let mut sorted: Vec<f64> = self.values().collect();
        sorted.sort_by(f64::total_cmp);
        let pairs = trim_pairs(sorted.len());
        let kept = sorted.get(pairs..sorted.len() - pairs)?;
        mean_of(kept.iter().copied(), kept.len())
    }

    /// The least-squares slope of the samples against the sample number. With fewer than 3
    /// samples it is 0. `None` when the slope is not a finite number.
    pub fn slope(&self) -> Option<f64> {
        let n = self.len();
        if n < 3 {
            return Some(0.0);
        }
        let raw = slope_of(self.values(), n, 1.0);
        if raw.is_some() {
            return raw;
        }
        // A difference of two large samples overflows. The slope of the samples divided by a
        // power of 2 is the slope divided by the same number.
        let scale = scale_of(self.values());
        let slope = slope_of(self.values(), n, scale)? * scale;
        slope.is_finite().then_some(slope)
    }
}

/// A power of 2 that brings every sample into -2 to 2, and 1 for samples below 2. Dividing by it
/// is exact.
fn scale_of<I: Iterator<Item = f64>>(values: I) -> f64 {
    let largest = values.fold(0.0_f64, |m, v| m.max(v.abs()));
    2.0_f64.powf((largest.log2().ceil() - 1.0).clamp(0.0, 1023.0))
}

/// The mean of `count` finite numbers. The sum overflows for large samples although their mean is
/// finite. Then the samples are scaled down first. `None` for no samples and for a mean that is
/// not finite.
fn mean_of<I: Iterator<Item = f64> + Clone>(values: I, count: usize) -> Option<f64> {
    if count == 0 {
        return None;
    }
    let n = count as f64;
    let sum: f64 = values.clone().sum();
    let mean = if sum.is_finite() {
        sum / n
    } else {
        let scale = scale_of(values.clone());
        values.map(|v| v / scale).sum::<f64>() / n * scale
    };
    mean.is_finite().then_some(mean)
}

/// The least-squares slope of the `n` samples divided by `scale`, against the sample number. It
/// takes the difference to the first sample, so equal samples give exactly 0. `None` when a step
/// overflows.
fn slope_of<I: Iterator<Item = f64> + Clone>(values: I, n: usize, scale: f64) -> Option<f64> {
    let mean_x = (n as f64 - 1.0) / 2.0;
    let first = values.clone().next()? / scale;
    let (mut num, mut den) = (0.0, 0.0);
    for (i, y) in values.enumerate() {
        let dx = i as f64 - mean_x;
        num += dx * (y / scale - first);
        den += dx * dx;
    }
    let slope = num / den;
    slope.is_finite().then_some(slope)
}

/// How many pairs of the lowest and the highest sample the low pass drops: as many as leave at
/// least a third of the samples.
pub fn trim_pairs(samples: usize) -> usize {
    let mut pairs = 0;
    while samples >= 2 * (pairs + 1) && (samples - 2 * (pairs + 1)) * 3 >= samples {
        pairs += 1;
    }
    pairs
}

fn round5(value: f64) -> f64 {
    round_to(value, 5, RoundMode::Nearest)
}

/// A filter node: its window and, for the low pass, its damped output.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Filter {
    window: Window,
    damped: Option<f64>,
}

impl Filter {
    pub fn window(&self) -> &Window {
        &self.window
    }

    /// One evaluation. The samples age in every evaluation. An input with no value gives no value
    /// and adds no sample. The peak, the minimum, the average and the trend are rounded to 5
    /// decimals.
    pub fn step(
        &mut self,
        stat: Stat,
        input: Option<f64>,
        span_s: u32,
        now: Duration,
    ) -> Option<f64> {
        self.window.age(Duration::from_secs(u64::from(span_s)), now);
        if self.window.is_empty() {
            self.damped = None;
        }
        self.window.push(now, input.filter(|v| v.is_finite())?);
        match stat {
            Stat::Peak => self.window.peak().map(round5),
            Stat::Minimum => self.window.minimum().map(round5),
            Stat::Average => self.window.mean().map(round5),
            Stat::Trend => self.window.slope().map(round5),
            Stat::LowPass { damping } => self.low_pass(damping),
        }
    }

    fn low_pass(&mut self, damping: f64) -> Option<f64> {
        let mean = self.window.mean()?;
        if self.window.len() < 3 {
            // The damped output starts at the first mean.
            self.damped.get_or_insert(mean);
            return Some(mean);
        }
        let target = self.window.trimmed_mean()?;
        let output = match self.damped {
            Some(last) => {
                let moved = last + (target - last) * (1.0 - damping / 100.0);
                // The distance between two large numbers of opposite sign overflows. A weighted
                // sum of the two stays between them.
                if moved.is_finite() {
                    moved
                } else {
                    last * (damping / 100.0) + target * (1.0 - damping / 100.0)
                }
            }
            None => target,
        };
        self.damped = Some(output);
        Some(output)
    }
}

#[cfg(test)]
mod tests {
    use nicecooling_core::clock::{Clock, SimClock};

    use super::*;

    /// Feeds one value per second and returns the output of each step.
    fn feed(
        filter: &mut Filter,
        stat: Stat,
        span_s: u32,
        inputs: &[Option<f64>],
    ) -> Vec<Option<f64>> {
        let clock = SimClock::default();
        inputs
            .iter()
            .map(|input| {
                let out = filter.step(stat, *input, span_s, clock.now());
                clock.advance(Duration::from_secs(1));
                out
            })
            .collect()
    }

    fn some(values: &[f64]) -> Vec<Option<f64>> {
        values.iter().map(|v| Some(*v)).collect()
    }

    #[test]
    fn old_samples_leave_each_second() {
        let mut filter = Filter::default();
        feed(
            &mut filter,
            Stat::Average,
            3,
            &some(&[1.0, 2.0, 3.0, 4.0, 5.0]),
        );
        // The span is 3 s: the samples of seconds 1 to 4 are left.
        assert_eq!(filter.window().len(), 4);
        let mut window = Window::default();
        for t in 0..10 {
            window.push(Duration::from_secs(t), t as f64);
        }
        window.age(Duration::from_secs(3), Duration::from_secs(9));
        assert_eq!(window.len(), 4);
        assert_eq!(window.minimum(), Some(6.0));
        // One more second and one more sample leave.
        window.age(Duration::from_secs(3), Duration::from_secs(10));
        assert_eq!(window.len(), 3);
        window.age(Duration::from_secs(3), Duration::from_secs(1000));
        assert!(window.is_empty());
        assert_eq!(window.mean(), None);
    }

    #[test]
    fn one_sample_per_whole_second_and_the_last_one_stays() {
        let mut window = Window::default();
        window.push(Duration::from_millis(1_100), 1.0);
        window.push(Duration::from_millis(1_900), 2.0);
        assert_eq!(window.len(), 1);
        assert_eq!(window.peak(), Some(2.0));
        window.push(Duration::from_millis(2_000), 3.0);
        assert_eq!(window.len(), 2);
    }

    #[test]
    fn gap_then_value_gives_that_value() {
        // The span is 3 s, the values 1, 2, 3 arrive at seconds 0 to 2, nothing until second 9,
        // and 7 arrives at second 10.
        let mut inputs = some(&[1.0, 2.0, 3.0]);
        inputs.extend(vec![None; 7]);
        inputs.push(Some(7.0));
        let mut filter = Filter::default();
        let out = feed(&mut filter, Stat::Average, 3, &inputs);
        assert_eq!(out[10], Some(7.0));
        assert_eq!(filter.window().len(), 1);
    }

    #[test]
    fn no_value_adds_no_sample_and_samples_age() {
        // The span is 2 s. The values of seconds 0 and 1 are there, then 4 s of no value.
        let mut filter = Filter::default();
        let inputs = [Some(1.0), Some(2.0), None, None, None, None, Some(9.0)];
        let out = feed(&mut filter, Stat::Peak, 2, &inputs);
        assert_eq!(out[..2], some(&[1.0, 2.0])[..]);
        // No value gives no value.
        assert!(out[2..6].iter().all(Option::is_none));
        // The no-value ticks added no sample, and by second 6 both old samples are gone.
        assert_eq!(out[6], Some(9.0));
        assert_eq!(filter.window().len(), 1);
        // The samples age in a tick with no value: at second 3 the sample of second 0 is gone.
        let mut aging = Filter::default();
        aging.step(Stat::Peak, Some(1.0), 2, Duration::ZERO);
        aging.step(Stat::Peak, Some(2.0), 2, Duration::from_secs(1));
        assert_eq!(
            aging.step(Stat::Peak, None, 2, Duration::from_secs(3)),
            None
        );
        assert_eq!(aging.window().len(), 1);
        assert_eq!(
            aging.step(Stat::Peak, None, 2, Duration::from_secs(4)),
            None
        );
        assert!(aging.window().is_empty());
    }

    #[test]
    fn average_3_5_at_second_4() {
        let mut filter = Filter::default();
        let out = feed(
            &mut filter,
            Stat::Average,
            3,
            &some(&[1.0, 2.0, 3.0, 4.0, 5.0]),
        );
        assert_eq!(out[4], Some(3.5));
        assert_eq!(out[0], Some(1.0));
        assert_eq!(out[2], Some(2.0));
        assert_eq!(out[3], Some(2.5));
    }

    #[test]
    fn peak_and_minimum() {
        let mut peak = Filter::default();
        let out = feed(&mut peak, Stat::Peak, 10, &some(&[4.0, 9.0, 6.0]));
        assert_eq!(out, some(&[4.0, 9.0, 9.0]));
        let mut minimum = Filter::default();
        let out = feed(&mut minimum, Stat::Minimum, 10, &some(&[4.0, 9.0, 6.0]));
        assert_eq!(out, some(&[4.0, 4.0, 4.0]));
        // The peak leaves the window with its sample.
        let mut peak = Filter::default();
        let out = feed(&mut peak, Stat::Peak, 2, &some(&[9.0, 1.0, 2.0, 3.0]));
        assert_eq!(out, some(&[9.0, 9.0, 9.0, 3.0]));
    }

    #[test]
    fn low_pass_starts_again_after_a_gap() {
        let stat = Stat::LowPass { damping: 50.0 };
        let mut filter = Filter::default();
        for t in 0..6 {
            filter.step(stat, Some(100.0), 3, Duration::from_secs(t));
        }
        // The window is empty at second 20: the next output starts from the first mean.
        assert_eq!(filter.step(stat, None, 3, Duration::from_secs(20)), None);
        for (t, want) in [(21, 10.0), (22, 10.0), (23, 10.0)] {
            assert_eq!(
                filter.step(stat, Some(10.0), 3, Duration::from_secs(t)),
                Some(want)
            );
        }
    }

    #[test]
    fn low_pass_under_3_samples_is_mean() {
        let mut filter = Filter::default();
        let out = feed(
            &mut filter,
            Stat::LowPass { damping: 25.0 },
            10,
            &some(&[10.0, 20.0]),
        );
        assert_eq!(out, some(&[10.0, 15.0]));
        // A gap that leaves fewer than 3 samples gives the mean again.
        let mut filter = Filter::default();
        let mut inputs = some(&[10.0, 20.0, 30.0]);
        inputs.extend(vec![None; 9]);
        inputs.push(Some(40.0));
        let out = feed(&mut filter, Stat::LowPass { damping: 25.0 }, 5, &inputs);
        assert_eq!(out[12], Some(40.0));
    }

    #[test]
    fn low_pass_trimmed_mean_damped_17_5() {
        // The window holds 10, 20 and 30, the damping is 25 and the last output was 10: the
        // trimmed mean is 20 and the output moves 75% of the way from 10 to 20.
        let mut filter = Filter::default();
        let out = feed(
            &mut filter,
            Stat::LowPass { damping: 25.0 },
            10,
            &some(&[10.0, 20.0, 30.0]),
        );
        assert_eq!(out[2], Some(17.5));
        // The same state set by hand.
        let mut set = Filter {
            damped: Some(10.0),
            ..Filter::default()
        };
        for (t, v) in [(0, 10.0), (1, 20.0)] {
            set.window.push(Duration::from_secs(t), v);
        }
        let out = set.step(
            Stat::LowPass { damping: 25.0 },
            Some(30.0),
            10,
            Duration::from_secs(2),
        );
        assert_eq!(out, Some(17.5));
    }

    #[test]
    fn low_pass_damping_zero() {
        let mut filter = Filter::default();
        let out = feed(
            &mut filter,
            Stat::LowPass { damping: 0.0 },
            10,
            &some(&[10.0, 20.0, 30.0, 40.0, 50.0]),
        );
        // 10, 20, 30: trimmed mean 20. Then 10..40: drops 10 and 40, mean 25. Then 10..50: 30.
        assert_eq!(out[2], Some(20.0));
        assert_eq!(out[3], Some(25.0));
        assert_eq!(out[4], Some(30.0));
    }

    #[test]
    fn trim_pairs_for_3_6_9_samples() {
        assert_eq!(trim_pairs(3), 1);
        assert_eq!(trim_pairs(6), 2);
        assert_eq!(trim_pairs(9), 3);
        assert_eq!(trim_pairs(0), 0);
        assert_eq!(trim_pairs(1), 0);
        assert_eq!(trim_pairs(2), 0);
        assert_eq!(trim_pairs(4), 1);
        assert_eq!(trim_pairs(5), 1);
        // The trimmed mean of 6 samples keeps the middle 2.
        let mut window = Window::default();
        for (t, v) in [50.0, 1.0, 4.0, 3.0, 100.0, 2.0].into_iter().enumerate() {
            window.push(Duration::from_secs(t as u64), v);
        }
        assert_eq!(window.trimmed_mean(), Some(3.5));
    }

    #[test]
    fn trend_slope_rising_falling() {
        let mut filter = Filter::default();
        let out = feed(&mut filter, Stat::Trend, 10, &some(&[1.0, 2.0, 3.0, 4.0]));
        assert_eq!(out[3], Some(1.0));
        let mut filter = Filter::default();
        let out = feed(&mut filter, Stat::Trend, 10, &some(&[6.0, 4.0, 2.0]));
        assert_eq!(out[2], Some(-2.0));
        let mut flat = Filter::default();
        let out = feed(&mut flat, Stat::Trend, 10, &some(&[5.0, 5.0, 5.0, 5.0]));
        assert_eq!(out[3], Some(0.0));
    }

    #[test]
    fn trend_under_3_samples_gives_zero() {
        let mut filter = Filter::default();
        let out = feed(&mut filter, Stat::Trend, 10, &some(&[5.0, 9.0]));
        assert_eq!(out, some(&[0.0, 0.0]));
        // No value stays no value.
        assert_eq!(
            filter.step(Stat::Trend, None, 10, Duration::from_secs(2)),
            None
        );
    }

    #[test]
    fn window_results_rounded_to_5_decimals() {
        let third = 1.0 / 3.0;
        let mut average = Filter::default();
        assert_eq!(
            average.step(Stat::Average, Some(third), 10, Duration::ZERO),
            Some(0.33333)
        );
        let mut peak = Filter::default();
        assert_eq!(
            peak.step(Stat::Peak, Some(2.0 / 3.0), 10, Duration::ZERO),
            Some(0.66667)
        );
        let mut minimum = Filter::default();
        assert_eq!(
            minimum.step(Stat::Minimum, Some(0.123_456_789), 10, Duration::ZERO),
            Some(0.12346)
        );
        let mut trend = Filter::default();
        let out = feed(
            &mut trend,
            Stat::Trend,
            10,
            &some(&[0.0, third, 2.0 * third]),
        );
        assert_eq!(out[2], Some(0.33333));
    }
}
