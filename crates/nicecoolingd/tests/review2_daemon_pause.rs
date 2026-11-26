//! Review 2, the tick after a pause. Spec (fan-control, "First tick after a pause"): a pause is a
//! suspend of the PC, or a gap of more than 2 seconds between two ticks. A value from before the
//! pause never counts again.

mod support;

use std::time::Duration;

use nicecooling_api::channel::ChannelProxy;
use nicecooling_api::ids::Kind;
use nicecooling_api::manager::ManagerProxy;
use support::Rig;

const PAUSE: Duration = Duration::from_secs(6 * 3600);
const UNREACHABLE: &str = "leakshield-unreachable";

const PWM1: &str = "nct6798_pwm1";

fn lost_input(rig: &mut Rig) -> bool {
    rig.daemon
        .as_mut()
        .unwrap()
        .runner
        .context()
        .warnings
        .is_active("lost-input", PWM1)
}

fn pwm1_raw(rig: &Rig) -> String {
    std::fs::read_to_string(rig.fake.attr_path("nct6798", "pwm1"))
        .unwrap()
        .trim()
        .to_owned()
}

/// A suspend of the PC: the clock of the tick advances by `gap` and the suspended time by `PAUSE`.
async fn suspend_and_tick(rig: &mut Rig, gap: Duration) {
    rig.suspended.as_ref().unwrap().add(PAUSE);
    rig.clock.as_ref().unwrap().advance(gap);
    rig.daemon.as_mut().unwrap().run(Some(1)).await;
}

/// The clock of the tick (`CLOCK_MONOTONIC`) does not count the time of a suspend. A suspend with
/// a short freeze leaves no gap of more than 2 s between two ticks, so only the suspended time
/// tells the tick about it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_suspend_with_no_gap_in_the_tick_clock_is_a_pause() {
    let mut rig = Rig::start_with_clock().await;
    rig.step(2).await;
    assert!(!lost_input(&mut rig));

    // The sensors read again within the wait of the first tick: no Warning.
    suspend_and_tick(&mut rig, Duration::from_secs(1)).await;
    assert!(!lost_input(&mut rig));
    assert_eq!(pwm1_raw(&rig), "163");

    // The sensor stops reading during the next suspend: the value from before it is lost, though
    // it is only 1 s old.
    let temp = rig.fake.attr_path("k10temp", "temp1_input");
    std::fs::set_permissions(&temp, std::os::unix::fs::PermissionsExt::from_mode(0o644)).unwrap();
    std::fs::write(&temp, "not a number\n").unwrap();
    suspend_and_tick(&mut rig, Duration::from_secs(1)).await;
    assert!(lost_input(&mut rig));
    assert_eq!(pwm1_raw(&rig), "255");
}

async fn graph_sensor_on_pwm1(rig: &Rig) -> ManagerProxy<'static> {
    let manager = ManagerProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    manager.create_graph_sensor("Water", "degC").await.unwrap();
    let pwm1 = ChannelProxy::builder(&rig.client)
        .path(support::path(Kind::Channel, PWM1))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    pwm1.set_curve(
        &support::path(Kind::Sensor, "graph_1"),
        &[(40.0, 20.0), (80.0, 100.0)],
        0.0,
        100.0,
    )
    .await
    .unwrap();
    manager
}

const ENTRIES: [(&str, bool, f64); 1] = [("graph_1", true, 50.0)];

async fn send(manager: &ManagerProxy<'_>) {
    let entries = ENTRIES.map(|(id, valid, value)| (id.to_owned(), valid, value));
    manager.set_graph_sensor_values(&entries).await.unwrap();
}

/// A Graph sensor has no value until the app sends a new one. The value from before the suspend
/// is 1.5 s old in the first tick after it, 2.5 s in the second and 3.5 s in the third.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_value_of_the_app_from_before_a_suspend_never_counts_again() {
    let mut rig = Rig::start_with_clock().await;
    let manager = graph_sensor_on_pwm1(&rig).await;
    rig.step(2).await;
    send(&manager).await;
    rig.step(1).await;
    assert!(!lost_input(&mut rig));

    suspend_and_tick(&mut rig, Duration::from_millis(500)).await;
    for n in 1..=3 {
        if n > 1 {
            rig.step(1).await;
        }
        assert!(lost_input(&mut rig), "tick {n} after the suspend");
        assert_eq!(pwm1_raw(&rig), "255");
    }

    send(&manager).await;
    rig.step(1).await;
    assert!(!lost_input(&mut rig));
}

/// The two clocks are read one after the other, so a change of the suspended time below 1 s is
/// noise.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn suspended_time_noise_below_one_second_is_no_pause() {
    let mut rig = Rig::start_with_clock().await;
    let manager = graph_sensor_on_pwm1(&rig).await;
    rig.step(2).await;
    send(&manager).await;
    rig.step(1).await;
    rig.suspended
        .as_ref()
        .unwrap()
        .add(Duration::from_millis(900));
    rig.step(1).await;
    assert!(!lost_input(&mut rig));
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
