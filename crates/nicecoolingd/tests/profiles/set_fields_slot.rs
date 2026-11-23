//! `SetFields` stores its read-back in the slot that the device runs, not in the slot of the
//! active Profile (design D14).

use std::collections::HashMap;

use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use zbus::Variant;

use crate::activate::switch;
use crate::slot_file;
use crate::support::{LEAKSHIELD, Rig, path};

const WRITABLE: &str = "display_page_time";

async fn leakshield(rig: &Rig) -> LeakshieldProxy<'static> {
    LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

fn set(value: f64) -> HashMap<&'static str, Variant<'static>> {
    [(WRITABLE, Variant::F64(value))].into()
}

fn meta(rig: &Rig, slot: u8) -> serde_json::Value {
    let file = slot_file(rig, slot).with_file_name("meta.json");
    serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()
}

async fn stored(proxy: &LeakshieldProxy<'_>, slot: u8) -> f64 {
    let fields = proxy.get_stored_fields(slot).await.unwrap();
    f64::try_from(fields[WRITABLE].try_clone().unwrap()).unwrap()
}

/// The switch to profile_2 fails: the device keeps slot 1 (`ActiveSlot` 1) and the active Profile
/// has slot 2. A stored edit of slot 2 waits for its activation. A later `SetFields` changes the
/// device, which runs slot 1: it must not replace the stored edit of slot 2 or clear its mark.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_fields_after_failed_switch_keeps_the_stored_edit() {
    let rig = Rig::start_with(crate::with_profile_3).await;
    let proxy = leakshield(&rig).await;
    rig.sim.fail_feature_writes(true);
    assert_eq!(switch(&rig, "profile_2").await, "leakshield-failed");
    rig.sim.fail_feature_writes(false);
    assert_eq!(proxy.active_slot().await.unwrap(), 1);
    proxy.set_stored_fields(2, set(25.0)).await.unwrap();
    assert_eq!(meta(&rig, 2)["edited"], true);

    proxy.set_fields(set(40.0)).await.unwrap();

    assert_eq!(stored(&proxy, 2).await, 25.0, "the edit of slot 2 survives");
    assert_eq!(meta(&rig, 2)["edited"], true, "its mark stays");
    // The read-back goes into slot 1, the slot that the device runs.
    assert_eq!(stored(&proxy, 1).await, 40.0);
}

/// The device runs slot 4 while the active Profile has slot 3: the write goes into slot 4.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_fields_stores_into_the_slot_the_device_runs() {
    let rig = Rig::start_on_slot(4, crate::with_profile_3).await;
    let proxy = leakshield(&rig).await;
    // The active Profile is profile_3, with slot 3.
    let before = stored(&proxy, 3).await;

    proxy.set_fields(set(40.0)).await.unwrap();

    assert_eq!(stored(&proxy, 4).await, 40.0);
    assert_eq!(stored(&proxy, 3).await, before);
}
