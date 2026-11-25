//! The first tick after a pause of the tick: the PC resumed from suspend, or a stopped process
//! went on. The tick reads the sensors again before it judges them. The test rig jumps its
//! simulated clock by 6 hours, which is a pause of any length for the value table.

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use nicecooling_api::channel::ChannelProxy;
use nicecooling_api::ids::Kind;
use nicecooling_api::manager::ManagerProxy;
use support::{Rig, path};

const PAUSE: Duration = Duration::from_secs(6 * 3600);
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
    fs::read_to_string(rig.fake.attr_path("nct6798", "pwm1"))
        .unwrap()
        .trim()
        .to_owned()
}

/// The pause: the clock jumps, then the daemon runs one tick on its own timer.
async fn pause_and_tick(rig: &mut Rig) {
    rig.clock.as_ref().unwrap().advance(PAUSE);
    rig.daemon.as_mut().unwrap().run(Some(1)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sensor_that_reads_in_the_first_tick_after_a_pause_is_not_lost() {
    let mut rig = Rig::start_with_clock().await;
    rig.step(2).await;
    assert!(!lost_input(&mut rig));

    pause_and_tick(&mut rig).await;
    assert!(!lost_input(&mut rig));
    // 61.875 degrees on the Curve (40, 20) to (80, 100) is 63.75%.
    assert_eq!(pwm1_raw(&rig), "163");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_sensor_that_does_not_read_after_a_pause_is_lost() {
    let mut rig = Rig::start_with_clock().await;
    rig.step(2).await;
    assert!(!lost_input(&mut rig));

    // The device stops answering with a number during the pause. The fake file is read-only, as
    // the kernel makes it.
    let temp = rig.fake.attr_path("k10temp", "temp1_input");
    fs::set_permissions(&temp, fs::Permissions::from_mode(0o644)).unwrap();
    fs::write(&temp, "not a number\n").unwrap();
    pause_and_tick(&mut rig).await;
    assert!(lost_input(&mut rig));
    assert_eq!(pwm1_raw(&rig), "255");

    // The sensor reads again: the Warning closes.
    fs::write(&temp, "61875\n").unwrap();
    rig.step(2).await;
    assert!(!lost_input(&mut rig));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_graph_sensor_stays_without_a_value_after_a_pause() {
    let mut rig = Rig::start_with_clock().await;
    let manager = ManagerProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    manager.create_graph_sensor("Water", "degC").await.unwrap();
    let pwm1 = ChannelProxy::builder(&rig.client)
        .path(path(Kind::Channel, PWM1))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    pwm1.set_curve(
        &path(Kind::Sensor, "graph_1"),
        &[(40.0, 20.0), (80.0, 100.0)],
        0.0,
        100.0,
    )
    .await
    .unwrap();
    rig.step(2).await;
    let entries = [("graph_1".to_owned(), true, 50.0)];
    manager.set_graph_sensor_values(&entries).await.unwrap();
    rig.step(1).await;
    assert!(!lost_input(&mut rig));

    pause_and_tick(&mut rig).await;
    assert!(lost_input(&mut rig));
    assert_eq!(pwm1_raw(&rig), "255");

    // The app sends a new value: the Curve takes it in the next tick.
    manager.set_graph_sensor_values(&entries).await.unwrap();
    rig.step(1).await;
    assert!(!lost_input(&mut rig));
}
