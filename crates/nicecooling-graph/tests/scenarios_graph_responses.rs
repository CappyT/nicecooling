//! One test for each scenario of `specs/graph-responses/spec.md` that the unit tests of the crate
//! do not check with the values of the scenario. The test names follow the scenario names.

use std::time::Duration;

use nicecooling_graph::pause::{Pause, PowerKind};
use nicecooling_graph::response::{Action, Response, RunState, Timing};

/// The wall clock of the tests: 1e15 us plus the seconds of the monotonic clock.
const BASE_US: u64 = 1_000_000_000_000_000;

fn secs(s: u64) -> Duration {
    Duration::from_secs(s)
}

/// One evaluation at second `t`. The wall clock is `BASE_US` plus `t` seconds.
fn at(r: &mut Response, timing: &Timing, t: u64, input: bool) -> Action {
    r.step(timing, Some(input), secs(t), BASE_US + t * 1_000_000)
}

#[test]
fn repeat_after_cancel() {
    // A poweroff response with repeat on at 600 s and the default minimum interval of 600 s. The
    // input rises at second 10 and the countdown starts. The user cancels at second 22.
    let timing = Timing {
        hold: secs(0),
        repeat: Some(secs(600)),
        min_interval: secs(600),
    };
    let mut response = Response::default();
    let mut pause = Pause::default();
    assert_eq!(at(&mut response, &timing, 10, true), Action::Run);
    pause.start_run(PowerKind::Poweroff);
    // The cancel leaves the node in the state "run" and gives the count back.
    pause.cancel_run(PowerKind::Poweroff);
    assert_eq!(pause.count, 0);
    for t in 11..610 {
        assert_eq!(
            at(&mut response, &timing, t, true),
            Action::Nothing,
            "second {t}"
        );
    }
    assert_eq!(response.state(), RunState::Run);
    // The response can run again after the repeat interval while the input is true.
    assert_eq!(at(&mut response, &timing, 610, true), Action::Run);
}
