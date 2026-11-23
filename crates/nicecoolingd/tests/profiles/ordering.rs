//! The order of the Leakshield writes of Profile activations: the newest request wins, a request
//! that fails still cancels the older one, and the write that waits reads its slot when it goes.

use std::collections::HashMap;
use std::time::Duration;

use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_core::profile::Cause;
use nicecooling_protocol::report3::Report3;
use nicecooling_protocol::sim::Request;
use zbus::Variant;

use crate::support::{LEAKSHIELD, Rig, path};
use crate::{automatic, slot_file};

const DISPLAY_PAGE_TIME: usize = 63;

fn feature_writes(rig: &Rig) -> usize {
    rig.sim
        .requests()
        .iter()
        .filter(|r| matches!(r, Request::SetFeature { .. }))
        .count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn close_activations_leave_the_device_on_the_slot_of_the_active_profile() {
    let rig = Rig::start_with(automatic).await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let mut wrong = Vec::new();
    for round in 0..6 {
        hub.activate_profile("profile_2", Cause::Manual).unwrap();
        hub.activate_profile("profile_3", Cause::Manual).unwrap();
        // The second write goes out 1.5 s after the first.
        tokio::time::sleep(Duration::from_millis(1800)).await;
        let device = rig.sim.report3()[747];
        if device != 3 {
            wrong.push((round, device));
        }
        // The simulated device keeps its report 1: back to a known start.
        rig.sim.edit_report1(|r| r.active_profile = 1);
    }
    assert!(
        wrong.is_empty(),
        "the active Profile has slot 3, device slot per round: {wrong:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failed_newest_activation_drops_the_waiting_older_write() {
    let rig = Rig::start_with(automatic).await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    // Slot 3 has no stored report 3: its activation fails before it writes.
    std::fs::remove_file(slot_file(&rig, 3)).unwrap();

    hub.activate_profile("profile_2", Cause::Manual).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(feature_writes(&rig), 1);
    // The device follows the written number, as the real device does.
    rig.sim.edit_report1(|r| r.active_profile = 2);

    // Within 2 s of the first write this request waits for its time.
    hub.activate_profile("profile_4", Cause::Manual).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    hub.activate_profile("profile_3", Cause::Manual).unwrap();
    tokio::time::sleep(Duration::from_millis(2500)).await;

    assert_eq!(hub.active_state(), "leakshield-failed");
    assert_eq!(
        (feature_writes(&rig), rig.sim.report3()[747]),
        (1, 2),
        "the superseded slot 4 request was written after the newer activation"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_user_write_while_a_profile_write_waits_stays_on_the_device_and_in_the_slot() {
    let rig = Rig::start_with(automatic).await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    // Slot 3 has its own value of display_page_time: raw 500 is 50 s.
    let mut slot3 = Report3::decode(&std::fs::read(slot_file(&rig, 3)).unwrap()).unwrap();
    slot3.set_bytes(DISPLAY_PAGE_TIME, &500u16.to_be_bytes());
    std::fs::write(slot_file(&rig, 3), slot3.encode()).unwrap();
    let proxy = LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();

    hub.activate_profile("profile_2", Cause::Manual).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    rig.sim.edit_report1(|r| r.active_profile = 2);
    // Waits until 1.5 s after the first write.
    hub.activate_profile("profile_3", Cause::Manual).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    let fields: HashMap<&str, Variant<'static>> =
        [("display_page_time", Variant::F64(30.0))].into();
    proxy.set_fields(fields).await.unwrap();
    tokio::time::sleep(Duration::from_millis(2500)).await;

    let device = rig.sim.report3();
    let stored = std::fs::read(slot_file(&rig, 3)).unwrap();
    let want = 300u16.to_be_bytes();
    assert_eq!(
        (
            &device[DISPLAY_PAGE_TIME..][..2],
            &stored[DISPLAY_PAGE_TIME..][..2],
            device[747]
        ),
        (&want[..], &want[..], 3),
        "device and slot 3 file must agree and keep the edit"
    );
}

/// `display_brightness_active`, a u8 with the values 0 to 3.
const BRIGHTNESS: usize = 65;

/// A user write while a Profile write waits must keep the other settings of the new slot. The
/// edit lands on the report 3 of the old slot, and the read-back goes into the new slot.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_user_write_while_a_profile_write_waits_keeps_the_other_settings_of_the_slot() {
    let rig = Rig::start_with(automatic).await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    // Slot 3 has its own brightness. Slot 2 has the brightness of the device.
    let old = rig.sim.report3()[BRIGHTNESS];
    let own = (old + 1) % 4;
    let mut slot3 = Report3::decode(&std::fs::read(slot_file(&rig, 3)).unwrap()).unwrap();
    slot3.set_bytes(BRIGHTNESS, &[own]);
    std::fs::write(slot_file(&rig, 3), slot3.encode()).unwrap();
    let proxy = LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();

    hub.activate_profile("profile_2", Cause::Manual).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    rig.sim.edit_report1(|r| r.active_profile = 2);
    // Waits until 1.5 s after the first write.
    hub.activate_profile("profile_3", Cause::Manual).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    // The user edits another field than the brightness.
    let fields: HashMap<&str, Variant<'static>> =
        [("display_page_time", Variant::F64(30.0))].into();
    proxy.set_fields(fields).await.unwrap();
    tokio::time::sleep(Duration::from_millis(2500)).await;

    let device = rig.sim.report3();
    let stored = std::fs::read(slot_file(&rig, 3)).unwrap();
    assert_eq!(
        (device[747], device[BRIGHTNESS], stored[BRIGHTNESS]),
        (3, own, own),
        "Profile 3 is active, but its brightness {own} is replaced by {old} of slot 2"
    );
}

/// A state where `profile_4` names slot 9: `profile.json` is a broken file.
fn profile_4_with_slot_9(fake: &nicecooling_hwmon::fake::FakeSysfs, state: &std::path::Path) {
    automatic(fake, state);
    std::fs::write(
        state.join("profiles/profile_4/profile.json"),
        r#"{"schema_version": 1, "id": "profile_4", "name": "Profile 4", "slot": 9}"#,
    )
    .unwrap();
}

/// A Profile with a slot outside 1 to 4 does not load, so its activation fails before any
/// request. The older request that waits is not the victim: it writes for its own activation.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_profile_with_a_bad_slot_is_not_loaded_and_leaves_the_waiting_write_alone() {
    let rig = Rig::start_with(profile_4_with_slot_9).await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();

    hub.activate_profile("profile_2", Cause::Manual).unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    rig.sim.edit_report1(|r| r.active_profile = 2);
    hub.activate_profile("profile_3", Cause::Manual).unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(hub.activate_profile("profile_4", Cause::Manual).is_err());
    tokio::time::sleep(Duration::from_millis(2500)).await;

    assert_eq!(
        (feature_writes(&rig), rig.sim.report3()[747]),
        (2, 3),
        "the active Profile is profile_3"
    );
    assert_eq!(hub.active_state(), "applied");
    assert!(!hub.profile_ids().contains(&"profile_4".to_owned()));
}
