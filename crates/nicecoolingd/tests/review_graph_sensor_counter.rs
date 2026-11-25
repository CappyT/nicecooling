//! Review reproducer: the id counter of the Graph sensors at the end of its range. A `reading_`
//! test pins a behavior that the spec does not state.

mod support;

use std::fs;
use std::time::Duration;

use nicecooling_api::channel::ChannelProxy;
use nicecooling_api::ids::Kind;
use nicecooling_api::manager::ManagerProxy;
use support::{Rig, STATE, error_name, path};
use tokio::time::timeout;

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";

async fn manager(rig: &Rig) -> ManagerProxy<'static> {
    ManagerProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

/// `STATE` with more keys after `schema_version`.
fn state_with(keys: &str) -> String {
    STATE.replace(
        "\"schema_version\": 1,",
        &format!("\"schema_version\": 1, {keys},"),
    )
}

/// Spec: "The counter SHALL never give an id twice." A counter at `u32::MAX` (a state edited by
/// hand) makes `ids::next_number` add 1 to `u32::MAX`: a panic in a debug build, and a wrap to 0 in
/// a release build, where `take_number` then gives `graph_4294967295` a second time.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn counter_at_u32_max_never_gives_an_id_twice() {
    let rig = Rig::start_with(|_, state| {
        fs::write(
            state.join("state.json"),
            state_with("\"graph_sensor_next\": 4294967295"),
        )
        .unwrap();
    })
    .await;
    let manager = manager(&rig).await;
    let first = manager.create_graph_sensor("A", "degC").await.unwrap();
    assert_eq!(first, path(Kind::Sensor, "graph_4294967295"));
    let second = timeout(
        Duration::from_secs(5),
        manager.create_graph_sensor("B", "degC"),
    )
    .await;
    // After the call, the daemon must still answer on the bus.
    let alive = timeout(Duration::from_secs(5), manager.version()).await;
    eprintln!(
        "second create: {:?}; Version after it: {:?}",
        second.as_ref().map(|r| r.as_ref().map_err(error_name)),
        alive.as_ref().map(|r| r.as_ref().map_err(error_name))
    );
    match second {
        Err(_) => panic!("CreateGraphSensor gave no reply in 5 s: the handler panicked"),
        Ok(Ok(path)) => assert_ne!(path, first, "the same id twice"),
        Ok(Err(e)) => assert_eq!(error_name(&e), format!("{ERR}.InvalidArgument")),
    }
    let mut ids = rig.daemon.as_ref().unwrap().hub.graph_sensor_ids();
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), count, "{ids:?}");
}

/// A stored counter that does not fit the `u32` of `graph_sensor_next`. Pins the current result,
/// which the spec does not state: `state.json` does not parse, and the daemon starts on the
/// defaults with `state-fallback`, without the Loop and the Curves of the file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reading_counter_beyond_u32_fails_the_whole_state_file() {
    let rig = Rig::start_with(|_, state| {
        fs::write(
            state.join("state.json"),
            state_with("\"graph_sensor_next\": 18446744073709551615"),
        )
        .unwrap();
    })
    .await;
    let health = manager(&rig).await.state_health().await.unwrap();
    let loops = rig.daemon.as_ref().unwrap().hub.loop_ids();
    let control = ChannelProxy::builder(&rig.client)
        .path(path(Kind::Channel, "nct6798_pwm1"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
        .control()
        .await
        .unwrap();
    eprintln!("state health {health:?}, loops {loops:?}, control of nct6798_pwm1 {control:?}");
    assert_ne!(health, "ok");
    assert!(loops.is_empty());
    assert_ne!(control, "curve");
}
