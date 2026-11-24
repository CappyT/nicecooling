use std::time::{SystemTime, UNIX_EPOCH};

use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_protocol::report1::RefillEntry;
use nicecoolingd::leakshield::clock::DEVICE_EPOCH_UNIX;

use crate::support::{LEAKSHIELD, Rig, path};

/// The device time of `days_ago` days before now.
fn device_time(days_ago: i64) -> u32 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    u32::try_from(now - days_ago * 86_400 - DEVICE_EPOCH_UNIX).unwrap()
}

fn refill(time: u32, duration: u32, turns: u32) -> RefillEntry {
    RefillEntry {
        time,
        duration,
        pump_turns: turns,
        volume: 1234,
    }
}

async fn proxy(rig: &Rig) -> LeakshieldProxy<'static> {
    LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_refill_history_returns_the_kept_entries() {
    let rig = Rig::start().await;
    let proxy = proxy(&rig).await;
    // The fixture has no refill.
    assert!(proxy.get_refill_history().await.unwrap().is_empty());
    let (older, newer) = (device_time(3), device_time(1));
    rig.sim.edit_report1(|r| {
        r.refill_history = Default::default();
        r.refill_history[0] = refill(newer, 12, 60);
        r.refill_history[1] = refill(older, 9, 40);
        // No time, and no pump revolutions: neither is a refill.
        r.refill_history[2] = refill(0, 30, 10);
        r.refill_history[3] = refill(device_time(2), 30, 0);
    });
    // A read of the device ingests the entries.
    proxy.get_counters().await.unwrap();

    let history = proxy.get_refill_history().await.unwrap();

    let us = |time: u32| (u64::from(time) + DEVICE_EPOCH_UNIX as u64) * 1_000_000;
    assert_eq!(history.len(), 2);
    assert_eq!(history[0], (us(older), 9, 40, 123.4));
    assert_eq!(history[1], (us(newer), 12, 60, 123.4));
    assert!(rig.sim.commands().is_empty());

    // The device forgets its history. The daemon keeps it, in the state directory.
    rig.sim
        .edit_report1(|r| r.refill_history = Default::default());
    proxy.get_counters().await.unwrap();
    assert_eq!(proxy.get_refill_history().await.unwrap().len(), 2);
    let file = rig
        .state
        .join("leakshield")
        .join(LEAKSHIELD)
        .join("refill-history.json");
    assert!(file.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_refill_history_survives_a_restart_and_drops_entries_after_90_days() {
    let mut rig = Rig::start().await;
    let (fresh, old) = (device_time(2), device_time(100));
    rig.sim.edit_report1(|r| {
        r.refill_history = Default::default();
        r.refill_history[0] = refill(fresh, 7, 30);
        r.refill_history[1] = refill(old, 20, 80);
    });
    proxy(&rig).await.get_counters().await.unwrap();
    assert_eq!(
        proxy(&rig).await.get_refill_history().await.unwrap().len(),
        1,
        "an entry of 100 days ago is not kept"
    );

    rig.sim
        .edit_report1(|r| r.refill_history = Default::default());
    rig.restart_daemon().await;

    let history = proxy(&rig).await.get_refill_history().await.unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].1, 7);
}
