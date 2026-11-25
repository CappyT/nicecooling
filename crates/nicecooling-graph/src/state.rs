//! The nodes that keep a state between two evaluations: memory, persistent storage, trigger, the
//! two delays and the counter. A step takes the inputs with their no-value state and the time
//! `now` of the monotonic clock (`nicecooling_core::clock::Clock`).

use std::time::Duration;

use nicecooling_core::rules::sensor::Hold;

use crate::funcs::{is_true, logic};

/// The shortest period of a counter or a generator, in seconds.
pub const MIN_PERIOD_S: f64 = 0.1;

/// A period rounds to 0.1 s and is at least 0.1 s. A value that is not a number gives the
/// shortest period.
pub fn normalize_period(period: f64) -> f64 {
    if period.is_nan() {
        return MIN_PERIOD_S;
    }
    ((period * 10.0).round() / 10.0).max(MIN_PERIOD_S)
}

/// The memory node: takes A while S is true and holds it while S is false.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Memory {
    value: f64,
}

impl Memory {
    pub fn with_value(value: f64) -> Self {
        Self { value }
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    /// An input with no value gives no value and leaves the held value as it is.
    pub fn step(&mut self, s: Option<f64>, a: Option<f64>) -> Option<f64> {
        let (s, a) = (s?, a?);
        if is_true(s) {
            self.value = a;
        }
        Some(self.value)
    }
}

/// The persistent storage node. It works as the memory node, and the app stores its value over a
/// restart.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Persistent {
    memory: Memory,
}

impl Persistent {
    /// The node as it was stored.
    pub fn restore(value: f64) -> Self {
        Self {
            memory: Memory::with_value(value),
        }
    }

    pub fn value(&self) -> f64 {
        self.memory.value()
    }

    pub fn step(&mut self, s: Option<f64>, a: Option<f64>) -> Option<f64> {
        self.memory.step(s, a)
    }
}

/// The trigger node: 1 for one tick when A goes from false to true.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Trigger {
    previous: Option<bool>,
}

impl Trigger {
    /// The first tick with a value gives 0 and records A.
    pub fn step(&mut self, a: Option<f64>) -> Option<f64> {
        let now = is_true(a?);
        let rising = self.previous == Some(false) && now;
        self.previous = Some(now);
        Some(logic(rising))
    }
}

/// The on delay node: 1 when A has been true for at least the delay.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RiseDelay {
    hold: Hold,
}

impl RiseDelay {
    /// A false input and an input with no value start the time again.
    pub fn step(&mut self, a: Option<f64>, delay: Duration, now: Duration) -> Option<f64> {
        let Some(a) = a else {
            self.hold.reset();
            return None;
        };
        Some(logic(self.hold.holds(Some(is_true(a)), delay, now)))
    }
}

/// The off delay node: 1 while A is true and for the delay after it becomes false.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FallDelay {
    on: bool,
    false_for: Hold,
}

impl FallDelay {
    /// An input with no value gives no value and drops the delay: the node starts again at 0.
    pub fn step(&mut self, a: Option<f64>, delay: Duration, now: Duration) -> Option<f64> {
        let Some(a) = a else {
            self.on = false;
            self.false_for.reset();
            return None;
        };
        if is_true(a) {
            self.on = true;
            self.false_for.reset();
        } else if self.false_for.holds(Some(true), delay, now) {
            self.on = false;
        }
        Some(logic(self.on))
    }
}

/// The counter node: adds `count` to its output once each period while it is enabled.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Counter {
    output: f64,
    timer: f64,
    last: Option<Duration>,
}

impl Counter {
    /// It adds at most once per tick. While enable is false the output holds and the timer does
    /// not advance. An input with no value gives no value, and the timer does not advance in that
    /// tick either.
    pub fn step(
        &mut self,
        count: Option<f64>,
        enable: Option<f64>,
        reset: Option<f64>,
        period: Option<f64>,
        now: Duration,
    ) -> Option<f64> {
        let elapsed = self
            .last
            .map_or(0.0, |last| now.saturating_sub(last).as_secs_f64());
        self.last = Some(now);
        let (count, enable, reset, period) = (count?, enable?, reset?, period?);
        if is_true(reset) {
            self.output = 0.0;
            self.timer = 0.0;
            return Some(0.0);
        }
        if is_true(enable) {
            self.timer += elapsed;
            let period = normalize_period(period);
            if self.timer >= period {
                let sum = self.output + count;
                if sum.is_finite() {
                    self.output = sum;
                }
                self.timer %= period;
            }
        }
        Some(self.output)
    }
}

#[cfg(test)]
mod tests {
    use nicecooling_core::clock::{Clock, SimClock};

    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    /// Steps `node` once per second over `inputs`, with 1 for true and 0 for false.
    fn run<F: FnMut(Option<f64>, Duration) -> Option<f64>>(
        inputs: &[Option<f64>],
        mut step: F,
    ) -> Vec<Option<f64>> {
        let clock = SimClock::default();
        inputs
            .iter()
            .map(|input| {
                let out = step(*input, clock.now());
                clock.advance(SECOND);
                out
            })
            .collect()
    }

    fn v(values: &[f64]) -> Vec<Option<f64>> {
        values.iter().map(|x| Some(*x)).collect()
    }

    #[test]
    fn memory_takes_and_holds() {
        let mut memory = Memory::default();
        assert_eq!(memory.step(Some(1.0), Some(5.0)), Some(5.0));
        assert_eq!(memory.step(Some(0.0), Some(9.0)), Some(5.0));
        assert_eq!(memory.step(Some(1.0), Some(9.0)), Some(9.0));
        // The start value is 0.
        assert_eq!(Memory::default().step(Some(0.0), Some(4.0)), Some(0.0));
    }

    #[test]
    fn memory_keeps_state_over_no_value() {
        let mut memory = Memory::default();
        assert_eq!(memory.step(Some(1.0), Some(7.0)), Some(7.0));
        for _ in 0..3 {
            assert_eq!(memory.step(None, Some(1.0)), None);
        }
        assert_eq!(memory.step(Some(0.0), Some(1.0)), Some(7.0));
        // An input A with no value also leaves the value alone.
        assert_eq!(memory.step(Some(1.0), None), None);
        assert_eq!(memory.value(), 7.0);
    }

    #[test]
    fn persistent_gives_no_value_without_input() {
        let mut node = Persistent::restore(5.0);
        // After a restart the node gives the stored value while S stays 0.
        assert_eq!(node.step(Some(0.0), Some(9.0)), Some(5.0));
        // S has no value: no value, and the stored value stays.
        assert_eq!(node.step(None, Some(9.0)), None);
        assert_eq!(node.value(), 5.0);
        assert_eq!(node.step(Some(1.0), Some(8.0)), Some(8.0));
        assert_eq!(node.value(), 8.0);
        assert_eq!(Persistent::default().value(), 0.0);
    }

    #[test]
    fn trigger_first_tick_gives_zero() {
        let mut trigger = Trigger::default();
        // True at the start: no edge.
        assert_eq!(trigger.step(Some(1.0)), Some(0.0));
        assert_eq!(trigger.step(Some(1.0)), Some(0.0));
        // A tick with no value is not the first tick with a value.
        let mut trigger = Trigger::default();
        assert_eq!(trigger.step(None), None);
        assert_eq!(trigger.step(Some(1.0)), Some(0.0));
    }

    #[test]
    fn trigger_rising_edge_sequence() {
        let mut trigger = Trigger::default();
        let got: Vec<_> = [0.0, 0.0, 1.0, 1.0, 1.0]
            .into_iter()
            .map(|a| trigger.step(Some(a)))
            .collect();
        assert_eq!(got, v(&[0.0, 0.0, 1.0, 0.0, 0.0]));
        // Falling and rising again gives a second pulse.
        assert_eq!(trigger.step(Some(0.0)), Some(0.0));
        assert_eq!(trigger.step(Some(1.0)), Some(1.0));
        // A gap does not make an edge.
        assert_eq!(trigger.step(None), None);
        assert_eq!(trigger.step(Some(1.0)), Some(0.0));
    }

    #[test]
    fn on_delay_3s() {
        // A becomes 1 at second 10 and stays 1: 0 at 10, 11 and 12, 1 from 13.
        let inputs: Vec<f64> = (0..16).map(|t| if t >= 10 { 1.0 } else { 0.0 }).collect();
        let mut node = RiseDelay::default();
        let out = run(&v(&inputs), |a, now| {
            node.step(a, Duration::from_secs(3), now)
        });
        let want: Vec<f64> = (0..16).map(|t| if t >= 13 { 1.0 } else { 0.0 }).collect();
        assert_eq!(out, v(&want));
        // A false input resets the time.
        let mut node = RiseDelay::default();
        let out = run(&v(&[1.0, 1.0, 0.0, 1.0, 1.0, 1.0, 1.0]), |a, now| {
            node.step(a, Duration::from_secs(3), now)
        });
        assert_eq!(out, v(&[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]));
    }

    #[test]
    fn on_delay_reset_by_no_value() {
        // A is 1 from second 10, has no value at 12 and is 1 again from 13.
        let mut inputs: Vec<Option<f64>> = (0..18)
            .map(|t| Some(if t >= 10 { 1.0 } else { 0.0 }))
            .collect();
        inputs[12] = None;
        let mut node = RiseDelay::default();
        let out = run(&inputs, |a, now| node.step(a, Duration::from_secs(3), now));
        // Seconds 10 and 11 give 0, second 12 has no value, 0 until second 15, 1 from second 16.
        let want = [
            Some(0.0),
            Some(0.0),
            None,
            Some(0.0),
            Some(0.0),
            Some(0.0),
            Some(1.0),
            Some(1.0),
        ];
        assert_eq!(out[10..], want);
    }

    #[test]
    fn off_delay_3s() {
        // A is 1 until second 10 and 0 from second 11: 1 until 13, 0 from 14.
        let inputs: Vec<f64> = (0..18).map(|t| if t <= 10 { 1.0 } else { 0.0 }).collect();
        let mut node = FallDelay::default();
        let out = run(&v(&inputs), |a, now| {
            node.step(a, Duration::from_secs(3), now)
        });
        let want: Vec<f64> = (0..18).map(|t| if t <= 13 { 1.0 } else { 0.0 }).collect();
        assert_eq!(out, v(&want));
        // The node starts at 0: a false input first gives 0.
        let mut node = FallDelay::default();
        assert_eq!(
            node.step(Some(0.0), Duration::from_secs(3), Duration::ZERO),
            Some(0.0)
        );
        // A delay of 0 s follows the input.
        let mut node = FallDelay::default();
        assert_eq!(
            node.step(Some(1.0), Duration::ZERO, Duration::ZERO),
            Some(1.0)
        );
        assert_eq!(node.step(Some(0.0), Duration::ZERO, SECOND), Some(0.0));
    }

    #[test]
    fn off_delay_reset_by_no_value() {
        // A is 1 until second 10 and has no value from second 11.
        let mut inputs: Vec<Option<f64>> = (0..=10).map(|_| Some(1.0)).collect();
        inputs.extend([None, None, Some(0.0), Some(0.0)]);
        let mut node = FallDelay::default();
        let out = run(&inputs, |a, now| node.step(a, Duration::from_secs(3), now));
        assert_eq!(out[10], Some(1.0));
        assert_eq!(out[11], None);
        assert_eq!(out[12], None);
        // The delay is dropped: a false input now gives 0 at once.
        assert_eq!(out[13], Some(0.0));
        assert_eq!(out[14], Some(0.0));
    }

    fn count_run(
        counter: &mut Counter,
        ticks: &[(f64, f64, f64)],
        period: f64,
    ) -> Vec<Option<f64>> {
        let clock = SimClock::default();
        ticks
            .iter()
            .map(|(count, enable, reset)| {
                let out = counter.step(
                    Some(*count),
                    Some(*enable),
                    Some(*reset),
                    Some(period),
                    clock.now(),
                );
                clock.advance(SECOND);
                out
            })
            .collect()
    }

    #[test]
    fn counter_every_5s() {
        // count 2, period 5 s, enable stays 1 for 12 s: 0 at second 0, 2 at 5, 4 at 10.
        let mut counter = Counter::default();
        let ticks = vec![(2.0, 1.0, 0.0); 13];
        let out = count_run(&mut counter, &ticks, 5.0);
        let want = [
            0.0, 0.0, 0.0, 0.0, 0.0, 2.0, 2.0, 2.0, 2.0, 2.0, 4.0, 4.0, 4.0,
        ];
        assert_eq!(out, v(&want));
    }

    #[test]
    fn counter_reset_and_disable() {
        let mut counter = Counter::default();
        let mut ticks = vec![(2.0, 1.0, 0.0); 6];
        // Second 6: reset. The output and the timer go to 0 in that tick.
        ticks.push((2.0, 1.0, 1.0));
        // Disabled for 3 s: the output holds and the timer does not advance.
        ticks.extend([(2.0, 0.0, 0.0); 3]);
        let out = count_run(&mut counter, &ticks, 5.0);
        assert_eq!(out[5], Some(2.0));
        assert_eq!(out[6], Some(0.0));
        assert_eq!(&out[7..], v(&[0.0, 0.0, 0.0]));
        // After the reset the count starts again: 5 s of enable give 2.
        let again = count_run(&mut counter, &[(2.0, 1.0, 0.0); 6], 5.0);
        assert_eq!(again.last(), Some(&Some(2.0)));
        // Held while disabled, with a count that was already added.
        let mut counter = Counter::default();
        let mut ticks = vec![(1.0, 1.0, 0.0); 6];
        ticks.extend([(1.0, 0.0, 0.0); 10]);
        let out = count_run(&mut counter, &ticks, 5.0);
        assert!(out[6..].iter().all(|o| *o == Some(1.0)));
    }

    #[test]
    fn counter_adds_once_per_tick() {
        // A clock that jumps 100 s adds one count and not twenty.
        let mut counter = Counter::default();
        assert_eq!(
            counter.step(Some(3.0), Some(1.0), Some(0.0), Some(5.0), Duration::ZERO),
            Some(0.0)
        );
        let late = Duration::from_secs(100);
        assert_eq!(
            counter.step(Some(3.0), Some(1.0), Some(0.0), Some(5.0), late),
            Some(3.0)
        );
        // An input with no value gives no value, and the elapsed time of that tick is lost.
        assert_eq!(
            counter.step(
                Some(3.0),
                None,
                Some(0.0),
                Some(5.0),
                late + Duration::from_secs(4)
            ),
            None
        );
        assert_eq!(
            counter.step(
                Some(3.0),
                Some(1.0),
                Some(0.0),
                Some(5.0),
                late + Duration::from_secs(5)
            ),
            Some(3.0)
        );
    }

    #[test]
    fn counter_period_half_second_is_accepted() {
        let mut counter = Counter::default();
        let out = count_run(&mut counter, &[(1.0, 1.0, 0.0); 4], 0.5);
        // One tick per second adds once per tick.
        assert_eq!(out, v(&[0.0, 1.0, 2.0, 3.0]));
        assert_eq!(normalize_period(0.5), 0.5);
        assert_eq!(normalize_period(0.04), 0.1);
        assert_eq!(normalize_period(0.26), 0.3);
        assert_eq!(normalize_period(-3.0), 0.1);
        assert_eq!(normalize_period(f64::NAN), 0.1);
        assert_eq!(normalize_period(10.0), 10.0);
    }
}
