//! Profiles and the Leakshield slots (task 7.1).

use std::collections::HashMap;

use nicecooling_api::authz_map::Action;
use nicecooling_api::ids::Kind;
use nicecooling_core::profile::Slot;
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report3::Report3;
use zbus::OwnedVariant;

use crate::support::{LEAKSHIELD, Rig, error_name, none_path, path};
use crate::{ERR, profile, profile_objects, profiles, with_profile_3};

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
