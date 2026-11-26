//! Review 2, the first tick after a pause. Spec (fan-control, "First tick after a pause"): "A
//! pause is a gap of more than 2 seconds between two ticks: the PC resumes from suspend, or the
//! process was stopped. A value from before the pause SHALL count as no value."

mod support;

use std::time::Duration;

use support::Rig;

const PAUSE: Duration = Duration::from_secs(6 * 3600);
const UNREACHABLE: &str = "leakshield-unreachable";

fn clock_s(id: libc::clockid_t) -> f64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `ts` is a valid timespec for the call to write into.
    assert_eq!(unsafe { libc::clock_gettime(id, &mut ts) }, 0);
    ts.tv_sec as f64 + ts.tv_nsec as f64 / 1e9
}

/// The seconds of the clock under an `Instant`. On Linux the `Debug` text of an `Instant` is
/// `Instant { tv_sec: N, tv_nsec: M }`.
fn instant_s(instant: std::time::Instant) -> f64 {
    let text = format!("{instant:?}");
    let number = |key: &str| -> f64 {
        let rest = text.split(key).nth(1).expect("field in the Instant text");
        let digits: String = rest
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(char::is_ascii_digit)
            .collect();
        digits.parse().expect("number in the Instant text")
    };
    number("tv_sec") + number("tv_nsec") / 1e9
}

/// `TokioClock` (the clock of the tick in `main.rs`) is `tokio::time::Instant`, which is
/// `std::time::Instant`, which is `CLOCK_MONOTONIC` on Linux. That clock stops while the PC
/// sleeps, so a resume from suspend gives no gap between two ticks and the pause rule never
/// starts: a value from before the suspend reaches the Curve. `CLOCK_BOOTTIME` counts the sleep.
/// On a host that never slept since its boot, this test cannot fail.
#[test]
fn the_clock_of_the_tick_counts_the_time_of_a_suspend() {
    let tick = instant_s(tokio::time::Instant::now().into_std());
    let boottime = clock_s(libc::CLOCK_BOOTTIME);
    let missed = boottime - tick;
    assert!(
        missed.abs() < 1.0,
        "the clock of the tick misses {missed:.0} s that this PC slept since its boot: a resume \
         from suspend is no pause for the tick"
    );
}

fn unreachable(rig: &mut Rig) -> bool {
    rig.daemon
        .as_mut()
        .unwrap()
        .runner
        .context()
        .alarms
        .is_active(UNREACHABLE, support::LEAKSHIELD)
}

async fn pause_and_tick(rig: &mut Rig) {
    rig.clock.as_ref().unwrap().advance(PAUSE);
    rig.daemon.as_mut().unwrap().run(Some(1)).await;
}

/// The Loop of the rig sends its Feed message to the Leakshield.
fn loop_with_leakshield(_: &nicecooling_hwmon::fake::FakeSysfs, state: &std::path::Path) {
    let text = support::STATE.replace(
        "\"leakshield\": null",
        &format!("\"leakshield\": \"{}\"", support::LEAKSHIELD),
    );
    std::fs::write(state.join("state.json"), text).unwrap();
}

/// The Leakshield answers at once after the pause. Its report 1 from before the pause is 6 hours
/// old, and the wait of the first tick waits only for sensor values.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_leakshield_that_answers_after_a_pause_is_not_unreachable() {
    let mut rig = Rig::start_with_clock().await;
    rig.step(3).await;
    assert!(!unreachable(&mut rig));
    pause_and_tick(&mut rig).await;
    assert!(
        !unreachable(&mut rig),
        "leakshield-unreachable rose in the first tick after a pause"
    );
}

/// One Feed message fails 1 s before the pause, every Feed after it is accepted. The Alarm
/// counts 10 s of failed Feeds (`RAISE_AFTER`); the start of the failed run is from before the
/// pause, so the first tick after the pause sees 6 hours of failures.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_failed_feed_before_a_pause_does_not_raise_unreachable_after_it() {
    let mut rig = Rig::start_with_clock_and(loop_with_leakshield).await;
    rig.step(3).await;
    assert!(!unreachable(&mut rig));
    rig.sim.fail_bulk(true);
    rig.step(1).await;
    rig.sim.fail_bulk(false);
    pause_and_tick(&mut rig).await;
    assert!(
        !unreachable(&mut rig),
        "one failed Feed before the pause raised leakshield-unreachable after it"
    );
}

/// The control of the test above: the same failed Feed with no pause raises no Alarm.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_failed_feed_without_a_pause_raises_nothing() {
    let mut rig = Rig::start_with_clock_and(loop_with_leakshield).await;
    rig.step(3).await;
    rig.sim.fail_bulk(true);
    rig.step(1).await;
    rig.sim.fail_bulk(false);
    rig.step(3).await;
    assert!(!unreachable(&mut rig));
}
