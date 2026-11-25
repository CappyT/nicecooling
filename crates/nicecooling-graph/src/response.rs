//! The state machine of a response end node: hold time, repeat and minimum interval. It decides
//! when a response runs. It does not run one: the app does.
//!
//! The hold time and the repeat interval count seconds of the monotonic clock. The minimum
//! interval counts wall clock time, so it holds over a reboot.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::model::Node;

/// Whether the response has run for the input that is true now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Run,
    NotRun,
}

/// The stored run state of one response end node, as `graph-state.json` holds it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunRecord {
    pub state: RunState,
    /// The wall clock time of the last run, in microseconds since the Unix epoch.
    #[serde(default)]
    pub last_run_us: Option<u64>,
}

/// The timing settings of a response end node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    pub hold: Duration,
    /// `None` when repeat is off.
    pub repeat: Option<Duration>,
    pub min_interval: Duration,
}

impl Timing {
    /// The settings of a node, with the defaults of its response kind for a missing value.
    pub fn from_node(node: &Node) -> Self {
        let seconds = |name: &str| Duration::from_secs(node.number(name).max(0.0) as u64);
        let repeat = node
            .params
            .get("repeat_s")
            .and_then(serde_json::Value::as_f64)
            .map(|s| Duration::from_secs(s.max(0.0) as u64));
        Self {
            hold: seconds("hold_s"),
            repeat,
            min_interval: seconds("min_interval_s"),
        }
    }
}

/// What a step asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Nothing,
    /// Run the response and write a log row.
    Run,
    /// The minimum interval is not over. The run is dropped and gets no log row.
    Dropped,
}

/// The state of one response end node.
#[derive(Clone, Debug, PartialEq)]
pub struct Response {
    state: RunState,
    /// When the input became true again, on the monotonic clock.
    trigger: Option<Duration>,
    /// The last run or dropped run, on the monotonic clock. The repeat counts from it.
    last_attempt: Option<Duration>,
    last_run_us: Option<u64>,
}

impl Default for Response {
    fn default() -> Self {
        Self {
            state: RunState::NotRun,
            trigger: None,
            last_attempt: None,
            last_run_us: None,
        }
    }
}

impl Response {
    /// The state after a start of the app. With the stored state "run" the repeat interval counts
    /// from `now`.
    pub fn restore(record: &RunRecord, now: Duration) -> Self {
        Self {
            state: record.state,
            trigger: None,
            last_attempt: (record.state == RunState::Run).then_some(now),
            last_run_us: record.last_run_us,
        }
    }

    pub fn record(&self) -> RunRecord {
        RunRecord {
            state: self.state,
            last_run_us: self.last_run_us,
        }
    }

    pub fn state(&self) -> RunState {
        self.state
    }

    /// One evaluation. `input` is the input of the end node: `None` for no value. A false input
    /// clears the trigger time and the state. An input with no value keeps the state and clears the
    /// trigger time. In the state "not run" the response runs when the input has been true for the
    /// hold time. In the state "run" with repeat on it runs again after the repeat interval.
    ///
    /// The minimum interval compares `wall_us` with the last run. A last run that is later than
    /// `wall_us` moves back to `wall_us`: the wait is then at most one interval, not the time that
    /// the clock went back.
    pub fn step(
        &mut self,
        timing: &Timing,
        input: Option<bool>,
        now: Duration,
        wall_us: u64,
    ) -> Action {
        match input {
            Some(false) => {
                self.trigger = None;
                self.state = RunState::NotRun;
                Action::Nothing
            }
            None => {
                self.trigger = None;
                Action::Nothing
            }
            Some(true) => {
                let trigger = *self.trigger.get_or_insert(now);
                let due = match self.state {
                    RunState::NotRun => now.saturating_sub(trigger) >= timing.hold,
                    RunState::Run => matches!(
                        (timing.repeat, self.last_attempt),
                        (Some(repeat), Some(last)) if now.saturating_sub(last) >= repeat
                    ),
                };
                if due {
                    self.attempt(timing, now, wall_us)
                } else {
                    Action::Nothing
                }
            }
        }
    }

    fn attempt(&mut self, timing: &Timing, now: Duration, wall_us: u64) -> Action {
        self.state = RunState::Run;
        self.last_attempt = Some(now);
        let interval_us = u64::try_from(timing.min_interval.as_micros()).unwrap_or(u64::MAX);
        if let Some(last) = self.last_run_us.as_mut() {
            *last = (*last).min(wall_us);
        }
        if let Some(last) = self.last_run_us
            && wall_us < last.saturating_add(interval_us)
        {
            return Action::Dropped;
        }
        self.last_run_us = Some(wall_us);
        Action::Run
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wall clock of the tests: 1e15 us plus the seconds of the monotonic clock.
    const BASE_US: u64 = 1_000_000_000_000_000;

    fn secs(s: u64) -> Duration {
        Duration::from_secs(s)
    }

    fn timing(hold: u64, repeat: Option<u64>, min_interval: u64) -> Timing {
        Timing {
            hold: secs(hold),
            repeat: repeat.map(secs),
            min_interval: secs(min_interval),
        }
    }

    /// One evaluation at second `t`. The wall clock is `BASE_US` plus `t` seconds.
    fn at(r: &mut Response, timing: &Timing, t: u64, input: Option<bool>) -> Action {
        r.step(timing, input, secs(t), BASE_US + t * 1_000_000)
    }

    /// The seconds at which the response runs when `input(t)` is the input for `t` in `range`.
    fn runs(
        r: &mut Response,
        timing: &Timing,
        range: std::ops::Range<u64>,
        input: impl Fn(u64) -> Option<bool>,
    ) -> Vec<u64> {
        range
            .filter(|t| at(r, timing, *t, input(*t)) == Action::Run)
            .collect()
    }

    #[test]
    fn hold_time_10s_runs_once() {
        let mut r = Response::default();
        let t = timing(10, None, 0);
        let got = runs(&mut r, &t, 90..200, |s| Some(s >= 100));
        assert_eq!(got, [110]);
        assert_eq!(r.state(), RunState::Run);
    }

    #[test]
    fn input_falls_inside_hold_time() {
        let mut r = Response::default();
        let t = timing(10, None, 0);
        // True from second 100 to 105, then false.
        let got = runs(&mut r, &t, 90..200, |s| Some((100..=105).contains(&s)));
        assert!(got.is_empty());
        assert_eq!(r.state(), RunState::NotRun);
    }

    #[test]
    fn repeat_every_60s() {
        let mut r = Response::default();
        let t = timing(10, Some(60), 0);
        let got = runs(&mut r, &t, 90..260, |s| Some(s >= 100));
        assert_eq!(got, [110, 170, 230]);
        // The repeat stops when the input falls.
        let got = runs(&mut r, &t, 260..400, |_| Some(false));
        assert!(got.is_empty());
    }

    #[test]
    fn no_repeat_runs_once() {
        let mut r = Response::default();
        let t = timing(0, None, 0);
        let got = runs(&mut r, &t, 0..3600, |_| Some(true));
        assert_eq!(got, [0]);
        // The next rise runs again.
        at(&mut r, &t, 3600, Some(false));
        assert_eq!(at(&mut r, &t, 3601, Some(true)), Action::Run);
    }

    #[test]
    fn no_run_on_falling_edge() {
        let mut r = Response::default();
        let t = timing(0, None, 0);
        assert_eq!(at(&mut r, &t, 0, Some(true)), Action::Run);
        assert_eq!(at(&mut r, &t, 1, Some(false)), Action::Nothing);
        assert_eq!(r.state(), RunState::NotRun);
        assert_eq!(at(&mut r, &t, 2, Some(false)), Action::Nothing);
    }

    #[test]
    fn no_value_in_state_run_keeps_state() {
        let mut r = Response::default();
        let t = timing(0, None, 0);
        assert_eq!(at(&mut r, &t, 0, Some(true)), Action::Run);
        // No value for 30 s, then the input reads 1 again.
        for s in 1..=30 {
            assert_eq!(at(&mut r, &t, s, None), Action::Nothing);
        }
        assert_eq!(r.state(), RunState::Run);
        assert_eq!(at(&mut r, &t, 31, Some(true)), Action::Nothing);
        assert_eq!(r.state(), RunState::Run);
    }

    #[test]
    fn no_value_in_hold_time_restarts_it() {
        // True from second 100, no value at 105, true again from 106: the run is at 116.
        let mut r = Response::default();
        let t = timing(10, None, 0);
        let got = runs(&mut r, &t, 95..130, |s| match s {
            ..=99 => Some(false),
            105 => None,
            _ => Some(true),
        });
        assert_eq!(got, [116]);
    }

    #[test]
    fn minimum_interval_drops_run() {
        let mut r = Response::default();
        let t = timing(0, None, 300);
        assert_eq!(at(&mut r, &t, 100, Some(true)), Action::Run);
        assert_eq!(at(&mut r, &t, 150, Some(false)), Action::Nothing);
        // The input rises at second 200: inside the interval.
        assert_eq!(at(&mut r, &t, 200, Some(true)), Action::Dropped);
        // The dropped run leaves the state at "run" and keeps the time of the last real run.
        assert_eq!(r.state(), RunState::Run);
        assert_eq!(r.record().last_run_us, Some(BASE_US + 100_000_000));
        // The interval is over at second 420.
        at(&mut r, &t, 300, Some(false));
        assert_eq!(at(&mut r, &t, 399, Some(true)), Action::Dropped);
        at(&mut r, &t, 400, Some(false));
        assert_eq!(at(&mut r, &t, 420, Some(true)), Action::Run);
    }

    #[test]
    fn minimum_interval_over_a_reboot() {
        // A poweroff ran at 10:00:00 with the interval 600 s. The PC starts at 10:03:00.
        let ten = BASE_US;
        let record = RunRecord {
            state: RunState::NotRun,
            last_run_us: Some(ten),
        };
        let t = timing(0, None, 600);
        let mut r = Response::restore(&record, Duration::ZERO);
        let three_minutes = ten + 180 * 1_000_000;
        assert_eq!(
            r.step(&t, Some(true), Duration::ZERO, three_minutes),
            Action::Dropped
        );
        // The response does not run before 10:10:00, and runs after.
        let nine_fifty_nine = ten + 599 * 1_000_000;
        assert_eq!(
            r.step(&t, Some(false), secs(1), nine_fifty_nine),
            Action::Nothing
        );
        assert_eq!(
            r.step(&t, Some(true), secs(2), nine_fifty_nine),
            Action::Dropped
        );
        assert_eq!(
            r.step(&t, Some(false), secs(3), nine_fifty_nine),
            Action::Nothing
        );
        assert_eq!(
            r.step(&t, Some(true), secs(4), ten + 600_000_000),
            Action::Run
        );
        // A wall clock that went back moves the last run back with it: the interval starts there.
        let mut r = Response::restore(&record, Duration::ZERO);
        assert_eq!(
            r.step(&t, Some(true), Duration::ZERO, ten - 5_000_000),
            Action::Dropped
        );
    }

    fn response_timing(kind: &str, min_interval_s: u64) -> Timing {
        use crate::model::{Kind, Node};
        let node = Node::new(1, Kind::ResponseEnd)
            .with_param("kind", kind)
            .with_param("min_interval_s", min_interval_s);
        Timing::from_node(&node)
    }

    #[test]
    fn wall_clock_back_waits_one_interval_for_a_power_response() {
        // The last poweroff ran at BASE_US and the clock went back one day. The default interval
        // is 600 s: the wait is 600 s of the new clock, not a day.
        let record = RunRecord {
            state: RunState::NotRun,
            last_run_us: Some(BASE_US),
        };
        let t = response_timing("poweroff", 600);
        assert_eq!(t.min_interval, secs(600));
        let back = BASE_US - 86_400 * 1_000_000;
        let mut r = Response::restore(&record, Duration::ZERO);
        assert_eq!(r.step(&t, Some(true), secs(0), back), Action::Dropped);
        assert_eq!(r.record().last_run_us, Some(back));
        assert_eq!(r.step(&t, Some(false), secs(1), back), Action::Nothing);
        let almost = back + 599 * 1_000_000;
        assert_eq!(r.step(&t, Some(true), secs(2), almost), Action::Dropped);
        assert_eq!(r.step(&t, Some(false), secs(3), almost), Action::Nothing);
        assert_eq!(
            r.step(&t, Some(true), secs(4), back + 600 * 1_000_000),
            Action::Run
        );
    }

    #[test]
    fn wall_clock_back_runs_a_notification_with_no_interval_at_once() {
        let record = RunRecord {
            state: RunState::NotRun,
            last_run_us: Some(BASE_US),
        };
        let t = response_timing("notification", 0);
        assert_eq!(t.min_interval, Duration::ZERO);
        let back = BASE_US - 86_400 * 1_000_000;
        let mut r = Response::restore(&record, Duration::ZERO);
        assert_eq!(r.step(&t, Some(true), secs(0), back), Action::Run);
        assert_eq!(r.record().last_run_us, Some(back));
    }

    #[test]
    fn dropped_first_run_is_not_retried() {
        let record = RunRecord {
            state: RunState::NotRun,
            last_run_us: Some(BASE_US),
        };
        let t = timing(0, None, 600);
        let mut r = Response::restore(&record, Duration::ZERO);
        assert_eq!(at(&mut r, &t, 0, Some(true)), Action::Dropped);
        // The input stays true for an hour and the interval ends: no new try.
        let got = runs(&mut r, &t, 1..3600, |_| Some(true));
        assert!(got.is_empty());
        // With repeat on, the repeat interval tries again, and the run goes through.
        let t = timing(0, Some(700), 600);
        let mut r = Response::restore(&record, Duration::ZERO);
        assert_eq!(at(&mut r, &t, 0, Some(true)), Action::Dropped);
        assert_eq!(at(&mut r, &t, 699, Some(true)), Action::Nothing);
        assert_eq!(at(&mut r, &t, 700, Some(true)), Action::Run);
    }

    #[test]
    fn stored_run_state_blocks_rerun() {
        // A response ran, the app restarts and the input is still true.
        let t = timing(0, None, 0);
        let mut r = Response::default();
        assert_eq!(at(&mut r, &t, 0, Some(true)), Action::Run);
        let mut restarted = Response::restore(&r.record(), Duration::ZERO);
        let got = runs(&mut restarted, &t, 0..100, |_| Some(true));
        assert!(got.is_empty());
        assert_eq!(restarted.state(), RunState::Run);
        // The condition is gone after the restart: the state is "not run".
        let mut restarted = Response::restore(&r.record(), Duration::ZERO);
        at(&mut restarted, &t, 0, Some(false));
        assert_eq!(restarted.state(), RunState::NotRun);
        assert_eq!(at(&mut restarted, &t, 1, Some(true)), Action::Run);
    }

    #[test]
    fn stored_not_run_with_true_input_rises() {
        let record = RunRecord {
            state: RunState::NotRun,
            last_run_us: None,
        };
        let t = timing(0, None, 0);
        let mut r = Response::restore(&record, Duration::ZERO);
        // The first evaluation sees a rise and, with hold time 0, runs at once.
        assert_eq!(at(&mut r, &t, 0, Some(true)), Action::Run);
        // With a hold time the run comes after it.
        let t = timing(5, None, 0);
        let mut r = Response::restore(&record, Duration::ZERO);
        let got = runs(&mut r, &t, 0..20, |_| Some(true));
        assert_eq!(got, [5]);
    }

    #[test]
    fn repeat_counts_from_the_app_start_after_a_restart() {
        // Repeat 60 s, stored state "run", the app starts at second 500 with the input true.
        let record = RunRecord {
            state: RunState::Run,
            last_run_us: Some(BASE_US),
        };
        let t = timing(0, Some(60), 0);
        let mut r = Response::restore(&record, secs(500));
        let got = runs(&mut r, &t, 500..700, |_| Some(true));
        assert_eq!(got, [560, 620, 680]);
    }

    #[test]
    fn run_state_json_roundtrip() {
        let record = RunRecord {
            state: RunState::Run,
            last_run_us: Some(1_766_000_000_000_000),
        };
        let text = serde_json::to_string(&record).unwrap();
        assert_eq!(text, r#"{"state":"run","last_run_us":1766000000000000}"#);
        assert_eq!(serde_json::from_str::<RunRecord>(&text).unwrap(), record);
        let never = RunRecord {
            state: RunState::NotRun,
            last_run_us: None,
        };
        let text = serde_json::to_string(&never).unwrap();
        assert_eq!(text, r#"{"state":"not_run","last_run_us":null}"#);
        assert_eq!(serde_json::from_str::<RunRecord>(&text).unwrap(), never);
        // The key is optional when the file is written by hand.
        let short: RunRecord = serde_json::from_str(r#"{"state":"not_run"}"#).unwrap();
        assert_eq!(short, never);
        // The record of a live response is what the app stores.
        let mut r = Response::default();
        at(&mut r, &timing(0, None, 0), 5, Some(true));
        assert_eq!(r.record().state, RunState::Run);
        assert_eq!(r.record().last_run_us, Some(BASE_US + 5_000_000));
    }

    #[test]
    fn timing_comes_from_the_node() {
        use crate::model::{Kind, ResponseKind, default_params_for};
        let mut node = Node::new(1, Kind::ResponseEnd);
        let t = Timing::from_node(&node);
        assert_eq!(t, timing(0, None, 0));
        node.params = default_params_for(Kind::ResponseEnd, ResponseKind::Reboot);
        assert_eq!(Timing::from_node(&node), timing(0, None, 600));
        node.params.insert("hold_s".into(), 10.into());
        node.params.insert("repeat_s".into(), 60.into());
        node.params.insert("min_interval_s".into(), 5.into());
        assert_eq!(Timing::from_node(&node), timing(10, Some(60), 5));
    }
}
