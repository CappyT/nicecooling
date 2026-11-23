//! Profiles and the Leakshield slots (task 7.1).

use std::collections::HashMap;
use std::time::Duration;

use nicecooling_api::authz_map::Action;
use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_core::profile::Slot;
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report3::Report3;
use nicecooling_protocol::sim::Request;
use zbus::{OwnedVariant, Variant};

use crate::activate::switch;
use crate::support::{LEAKSHIELD, Rig, error_name, none_path, path};
use crate::{ERR, profile, profile_objects, profiles, slot_file, with_profile_3};

fn slot_dir(rig: &Rig, slot: u8) -> std::path::PathBuf {
    rig.state
        .join("leakshield")
        .join(LEAKSHIELD)
        .join(format!("slot-{slot}"))
}

/// The first read of the Leakshield at the daemon start creates the files of the four slots.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn first_read_copies_report3_into_four_slots() {
    let rig = Rig::start().await;
    let offset = Registry::leakshield()
        .field("device_profile")
        .unwrap()
        .offset;
    let device = Report3::decode(&rig.sim.report3()).unwrap();
    for slot in Slot::all() {
        let bytes = std::fs::read(slot_dir(&rig, slot.get()).join("report3.bin")).unwrap();
        let stored = Report3::decode(&bytes).unwrap();
        assert_eq!(stored.bytes_at(offset, 1), [slot.get()]);
        // The slot holds the settings of the device and nothing else changed.
        let changed = stored.differing_offsets(&device, &[]);
        assert!(changed.iter().all(|o| *o == offset), "{changed:?}");
        let meta = std::fs::read_to_string(slot_dir(&rig, slot.get()).join("meta.json")).unwrap();
        assert!(meta.contains("\"firmware\": 1022"), "{meta}");
        assert!(meta.contains("20417-31072"), "{meta}");
    }
    // The first read writes nothing to the device.
    assert!(rig.sim.commands().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slot_stores_whole_report3() {
    let rig = Rig::start().await;
    for slot in 1..=4 {
        let bytes = std::fs::read(slot_dir(&rig, slot).join("report3.bin")).unwrap();
        // The report ID byte, 747 bytes of settings and the CRC.
        assert_eq!(bytes.len(), 750);
        assert_eq!(bytes[0], 3);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn first_start_creates_four_profiles_on_the_bus() {
    let rig = Rig::start_with(|_, state| {
        std::fs::remove_file(state.join("state.json")).unwrap();
        std::fs::remove_dir_all(state.join("profiles")).unwrap();
    })
    .await;
    assert_eq!(
        profile_objects(&rig).await,
        ["profile_2", "profile_3", "profile_4", "profile_default"]
    );
    for (id, slot) in [
        ("profile_default", 1),
        ("profile_2", 2),
        ("profile_3", 3),
        ("profile_4", 4),
    ] {
        assert_eq!(profile(&rig, id).await.slot().await.unwrap(), slot, "{id}");
        assert!(
            rig.state
                .join("profiles")
                .join(id)
                .join("profile.json")
                .exists(),
            "{id}"
        );
    }
    assert!(
        profile(&rig, "profile_default")
            .await
            .is_default()
            .await
            .unwrap()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn new_profile_from_current_state_takes_active_slot() {
    let rig = Rig::start_with(with_profile_3).await;
    let profiles = profiles(&rig).await;
    assert_eq!(
        profiles.active().await.unwrap(),
        path(Kind::Profile, "profile_3")
    );
    let created = profiles
        .create_profile("From current state", &none_path())
        .await
        .unwrap();
    let created = profile(&rig, created.as_str().rsplit('/').next().unwrap()).await;
    assert_eq!(created.slot().await.unwrap(), 3);
    assert_eq!(created.name().await.unwrap(), "From current state");

    // A copy of another Profile takes the slot of that Profile.
    let copy = profiles
        .create_profile("Copy of 4", &path(Kind::Profile, "profile_4"))
        .await
        .unwrap();
    let copy = profile(&rig, copy.as_str().rsplit('/').next().unwrap()).await;
    assert_eq!(copy.slot().await.unwrap(), 4);
    assert_eq!(
        rig.authorizer.calls(),
        [(Action::Write, false), (Action::Write, false)]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_slot_asks_write() {
    let rig = Rig::start_with(with_profile_3).await;
    let target = profile(&rig, "profile_4").await;
    target.set_slot(2).await.unwrap();
    assert_eq!(target.slot().await.unwrap(), 2);
    assert_eq!(rig.authorizer.calls(), [(Action::Write, false)]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_slot_rejects_0_and_5_and_the_default_profile() {
    let rig = Rig::start_with(with_profile_3).await;
    let target = profile(&rig, "profile_4").await;
    for slot in [0, 5] {
        let error = target.set_slot(slot).await.unwrap_err();
        assert_eq!(error_name(&error), format!("{ERR}.InvalidArgument"));
    }
    assert_eq!(target.slot().await.unwrap(), 4);
    let default = profile(&rig, "profile_default").await;
    let error = default.set_slot(2).await.unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.InvalidArgument"));
    assert_eq!(default.slot().await.unwrap(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_default_profile_returns_default_profile_error() {
    let rig = Rig::start().await;
    let default = profile(&rig, "profile_default").await;
    let error = default.delete().await.unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.DefaultProfile"));
    assert_eq!(profile_objects(&rig).await, ["profile_default"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_profile_keeps_slot_files() {
    let rig = Rig::start_with(with_profile_3).await;
    let before = std::fs::read(slot_dir(&rig, 4).join("report3.bin")).unwrap();
    rig.daemon.as_ref().unwrap().hub.flush_now();
    assert!(rig.state.join("profiles/profile_4/profile.json").exists());

    profile(&rig, "profile_4").await.delete().await.unwrap();
    rig.daemon.as_ref().unwrap().hub.flush_now();

    assert!(!rig.state.join("profiles/profile_4").exists());
    assert_eq!(
        std::fs::read(slot_dir(&rig, 4).join("report3.bin")).unwrap(),
        before
    );
    assert!(slot_dir(&rig, 4).join("meta.json").exists());
    // The Profile object goes after the reply.
    for _ in 0..100 {
        if !profile_objects(&rig)
            .await
            .contains(&"profile_4".to_owned())
        {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("the Profile object stays on the bus");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn shared_slot_shares_report3() {
    let rig = Rig::start_with(with_profile_3).await;
    let copy = profiles(&rig)
        .await
        .create_profile("Second with slot 3", &path(Kind::Profile, "profile_3"))
        .await
        .unwrap();
    let first = profile(&rig, "profile_3").await;
    let second = profile(&rig, copy.as_str().rsplit('/').next().unwrap()).await;
    assert_eq!(second.slot().await.unwrap(), 3);

    let (first, second) = (
        first.get_content().await.unwrap(),
        second.get_content().await.unwrap(),
    );
    let fields = |content: &HashMap<String, OwnedVariant>| {
        format!("{:?}", content.get("leakshield_fields").expect("fields"))
    };
    assert_eq!(fields(&first), fields(&second));
    assert!(fields(&first).contains("display_page_time"));
    // Both Profiles read the same files: the daemon has four slots, not five.
    let dirs = std::fs::read_dir(rig.state.join("leakshield").join(LEAKSHIELD))
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("slot-"))
        .count();
    assert_eq!(dirs, 4);
}

/// The Leakshield fields of a Profile use the types of `GetFields`: a name, a scaled number or
/// an integer, and a list of each.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn profile_content_types_follow_the_registry_kind() {
    let rig = Rig::start_with(with_profile_3).await;
    let content = profile(&rig, "profile_3")
        .await
        .get_content()
        .await
        .unwrap();
    let devices: HashMap<String, HashMap<String, OwnedVariant>> = content["leakshield_fields"]
        .try_clone()
        .unwrap()
        .try_into()
        .unwrap();
    let fields = &devices[LEAKSHIELD];
    let signature = |id: &str| fields[id].value_signature().to_string();
    assert_eq!(signature("default_mode"), "s");
    assert_eq!(signature("page_template"), "as");
    assert_eq!(signature("display_page_time"), "d");
    assert_eq!(signature("pump_max_speed"), "x");
    assert_eq!(signature("page_options"), "ax");
    let mode = String::try_from(fields["default_mode"].try_clone().unwrap()).unwrap();
    assert!(
        ["shield", "monitor", "release"].contains(&mode.as_str()),
        "{mode}"
    );
}

// Stored settings of another slot (design D18).

const WRITABLE: &str = "display_page_time";
const NOT_VERIFIED: &str = "shield_pressure_low";
const DISPLAY_PAGE_TIME: usize = 63;

async fn leakshield(rig: &Rig) -> LeakshieldProxy<'static> {
    LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

fn number(value: &OwnedVariant) -> f64 {
    f64::try_from(value.try_clone().unwrap()).unwrap()
}

fn set(id: &'static str, value: f64) -> HashMap<&'static str, Variant<'static>> {
    [(id, Variant::F64(value))].into()
}

fn meta(rig: &Rig, slot: u8) -> serde_json::Value {
    let file = slot_file(rig, slot).with_file_name("meta.json");
    serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap()
}

fn error_of<T: std::fmt::Debug>(result: zbus::Result<T>) -> String {
    error_name(&result.expect_err("the call must fail"))
}

fn sets_to_the_device(rig: &Rig) -> usize {
    let outputs = rig.sim.output_reports().len() + rig.sim.bulk_messages().len();
    let writes = rig
        .sim
        .requests()
        .iter()
        .filter(|r| matches!(r, Request::SetFeature { .. }))
        .count();
    outputs + writes
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_stored_fields_returns_slot() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;

    let stored = proxy.get_stored_fields(2).await.unwrap();

    // The slot holds the report 3 of the device, with its own number.
    let device = proxy.get_fields().await.unwrap();
    assert_eq!(stored.len(), device.len());
    let row = |id: &str| device.iter().find(|r| r.0 == id).unwrap();
    assert_eq!(number(&stored[WRITABLE]), number(&row(WRITABLE).2));
    assert_eq!(
        i64::try_from(stored["device_profile"].try_clone().unwrap()).unwrap(),
        2
    );
    assert_eq!(rig.authorizer.calls(), []);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_stored_fields_missing_slot_not_found() {
    let rig = Rig::start().await;
    std::fs::remove_dir_all(slot_file(&rig, 3).parent().unwrap()).unwrap();

    let error = error_of(leakshield(&rig).await.get_stored_fields(3).await);

    assert_eq!(error, format!("{ERR}.NotFound"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_stored_fields_writes_other_slot_with_valid_crc() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    assert_eq!(proxy.active_slot().await.unwrap(), 1);
    let before = std::fs::read(slot_file(&rig, 3)).unwrap();

    proxy
        .set_stored_fields(3, set(WRITABLE, 30.0))
        .await
        .unwrap();

    // The file decodes: the CRC is valid. Only the field changed, and the other slots did not.
    let after = std::fs::read(slot_file(&rig, 3)).unwrap();
    let report = Report3::decode(&after).unwrap();
    let old = Report3::decode(&before).unwrap();
    let changed = report.differing_offsets(&old, &[]);
    assert_eq!(changed, [DISPLAY_PAGE_TIME, DISPLAY_PAGE_TIME + 1]);
    assert_eq!(
        number(&proxy.get_stored_fields(3).await.unwrap()[WRITABLE]),
        30.0
    );
    assert_eq!(meta(&rig, 3)["edited"], true);
    assert_eq!(meta(&rig, 2)["edited"], false);
    assert_eq!(rig.authorizer.calls(), [(Action::Write, false)]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_stored_fields_sends_nothing_to_device() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let device = rig.sim.report3();
    let before = sets_to_the_device(&rig);

    proxy
        .set_stored_fields(3, set(WRITABLE, 25.0))
        .await
        .unwrap();
    proxy.get_stored_fields(3).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_eq!(sets_to_the_device(&rig), before);
    assert_eq!(rig.sim.report3(), device);
    assert!(rig.sim.commands().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_stored_fields_works_disconnected() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    rig.sim.disconnect();
    for _ in 0..200 {
        if !proxy.connected().await.unwrap() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    assert!(!proxy.connected().await.unwrap());

    proxy
        .set_stored_fields(4, set(WRITABLE, 25.0))
        .await
        .unwrap();

    assert_eq!(
        number(&proxy.get_stored_fields(4).await.unwrap()[WRITABLE]),
        25.0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_stored_fields_active_slot_invalid_argument() {
    let rig = Rig::start_on_slot(3, |_, _| {}).await;
    let proxy = leakshield(&rig).await;
    assert_eq!(proxy.active_slot().await.unwrap(), 3);
    let before = std::fs::read(slot_file(&rig, 3)).unwrap();

    let error = error_of(proxy.set_stored_fields(3, set(WRITABLE, 25.0)).await);

    assert_eq!(error, format!("{ERR}.InvalidArgument"));
    assert_eq!(std::fs::read(slot_file(&rig, 3)).unwrap(), before);
    // The other slots still take a change.
    proxy
        .set_stored_fields(2, set(WRITABLE, 25.0))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_stored_fields_slot_out_of_range() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    for slot in [0, 5, 255] {
        assert_eq!(
            error_of(proxy.set_stored_fields(slot, set(WRITABLE, 25.0)).await),
            format!("{ERR}.InvalidArgument"),
            "{slot}"
        );
        assert_eq!(
            error_of(proxy.get_stored_fields(slot).await),
            format!("{ERR}.InvalidArgument"),
            "{slot}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_stored_fields_uncovered_firmware_not_supported() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let file = slot_file(&rig, 3).with_file_name("meta.json");
    let mut stored = meta(&rig, 3);
    stored["firmware"] = 1019.into();
    std::fs::write(file, serde_json::to_vec(&stored).unwrap()).unwrap();

    assert_eq!(
        error_of(proxy.set_stored_fields(3, set(WRITABLE, 25.0)).await),
        "org.freedesktop.DBus.Error.NotSupported".to_owned()
    );
    assert_eq!(
        error_of(proxy.get_stored_fields(3).await),
        "org.freedesktop.DBus.Error.NotSupported".to_owned()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_stored_fields_not_verified_field() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let before = std::fs::read(slot_file(&rig, 3)).unwrap();

    let result = proxy.set_stored_fields(3, set(NOT_VERIFIED, 10.0)).await;

    let error = result.expect_err("a field that is not verified");
    assert_eq!(error_name(&error), format!("{ERR}.NotVerified"));
    assert_eq!(std::fs::read(slot_file(&rig, 3)).unwrap(), before);
    assert_eq!(meta(&rig, 3)["edited"], false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_stored_fields_refuses_device_profile() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;

    let error = error_of(
        proxy
            .set_stored_fields(3, [("device_profile", Variant::I64(4))].into())
            .await,
    );

    assert_eq!(error, format!("{ERR}.InvalidArgument"));
}

/// The activation of profile_3 writes at once and profile_4 waits for the spacing. A job that
/// arrives meanwhile runs after the write that waits.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_stored_fields_serialized_with_activation() {
    let rig = Rig::start_with(with_profile_3).await;
    let proxy = leakshield(&rig).await;
    assert_eq!(switch(&rig, "profile_2").await, "applied");
    let profiles = profiles(&rig).await;
    profiles
        .activate_profile(&path(Kind::Profile, "profile_4"))
        .await
        .unwrap();
    // The write of slot 4 waits for 1.5 s.
    assert_eq!(rig.sim.report3()[747], 2);

    proxy
        .set_stored_fields(3, set(WRITABLE, 25.0))
        .await
        .unwrap();

    assert_eq!(rig.sim.report3()[747], 4, "the waiting write went first");
}

async fn edit_and_activate(rig: &Rig, slot: u8) -> String {
    leakshield(rig)
        .await
        .set_stored_fields(slot, set(WRITABLE, 25.0))
        .await
        .unwrap();
    switch(rig, &format!("profile_{slot}")).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn edited_slot_activation_reads_back() {
    let rig = Rig::start_with(with_profile_3).await;
    rig.sim.clear_requests();

    assert_eq!(edit_and_activate(&rig, 2).await, "applied");

    let requests = rig.sim.requests();
    let write = requests
        .iter()
        .position(|r| matches!(r, Request::SetFeature { id: 3, .. }))
        .expect("the report 3 write");
    assert!(
        requests[write + 1..]
            .iter()
            .any(|r| matches!(r, Request::GetFeature { id: 3, .. })),
        "the activation read report 3 back: {requests:?}"
    );
    // The device holds the edited slot.
    assert_eq!(
        rig.sim.report3(),
        std::fs::read(slot_file(&rig, 2)).unwrap()
    );
    assert_eq!(
        number(&leakshield(&rig).await.get_stored_fields(2).await.unwrap()[WRITABLE]),
        25.0
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn edited_mark_cleared_after_good_read_back() {
    let rig = Rig::start_with(with_profile_3).await;
    assert_eq!(edit_and_activate(&rig, 2).await, "applied");

    assert_eq!(meta(&rig, 2)["edited"], false);
    assert_eq!(
        leakshield(&rig).await.failed_write_backup().await.unwrap(),
        ""
    );
    // A later activation of the slot is a plain write again: no read-back.
    rig.sim.edit_report1(|r| r.active_profile = 1);
    assert_eq!(switch(&rig, "profile_3").await, "applied");
    rig.sim.clear_requests();
    assert_eq!(switch(&rig, "profile_2").await, "applied");
    let requests = rig.sim.requests();
    let write = requests
        .iter()
        .position(|r| matches!(r, Request::SetFeature { id: 3, .. }))
        .expect("the report 3 write");
    assert!(
        requests[write + 1..]
            .iter()
            .all(|r| !matches!(r, Request::GetFeature { id: 3, .. })),
        "a slot that is not edited gets no read-back: {requests:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn edited_mark_kept_after_bad_read_back() {
    let rig = Rig::start_with(with_profile_3).await;
    let proxy = leakshield(&rig).await;
    proxy
        .set_stored_fields(2, set(WRITABLE, 25.0))
        .await
        .unwrap();
    rig.sim.tamper_next_set_feature(300, 0x55);

    assert_eq!(switch(&rig, "profile_2").await, "leakshield-failed");

    assert_eq!(meta(&rig, 2)["edited"], true);
    assert_ne!(proxy.failed_write_backup().await.unwrap(), "");
}
