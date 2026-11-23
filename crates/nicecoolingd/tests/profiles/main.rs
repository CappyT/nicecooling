//! Profiles, Leakshield slots, activation and Rule groups, on the simulated hardware layer and
//! the simulated Leakshield. No test touches a real device, `/proc`, `/sys` or logind.

mod activate;
mod rules;
mod slots;
#[path = "../support/mod.rs"]
mod support;

use nicecooling_api::ids::Kind;
use nicecooling_api::profile::ProfileProxy;
use nicecooling_api::profiles::ProfilesProxy;
use support::{Rig, path};

pub const ERR: &str = "io.github.cappyt.NiceCooling1.Error";

pub async fn profiles(rig: &Rig) -> ProfilesProxy<'static> {
    ProfilesProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

pub async fn profile(rig: &Rig, id: &str) -> ProfileProxy<'static> {
    ProfileProxy::builder(&rig.client)
        .path(path(Kind::Profile, id))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

pub fn slot_file(rig: &Rig, n: u8) -> std::path::PathBuf {
    rig.state
        .join("leakshield")
        .join(support::LEAKSHIELD)
        .join(format!("slot-{n}"))
        .join("report3.bin")
}

/// A state directory whose active Profile is `profile_3`, with slot 3.
pub fn with_profile_3(_: &nicecooling_hwmon::fake::FakeSysfs, state: &std::path::Path) {
    std::fs::write(
        state.join("state.json"),
        r#"{"schema_version": 1, "profiles": {"default": "profile_default",
            "active": "profile_3", "cause": "manual", "immediate_switching": false,
            "rule_delay_s": 30, "dwell_s": 300}}"#,
    )
    .unwrap();
    for (id, slot) in [("profile_2", 2), ("profile_3", 3), ("profile_4", 4)] {
        std::fs::create_dir_all(state.join("profiles").join(id)).unwrap();
        std::fs::write(
            state.join("profiles").join(id).join("profile.json"),
            format!(
                r#"{{"schema_version": 1, "id": "{id}", "name": "Profile {slot}", "slot": {slot}}}"#
            ),
        )
        .unwrap();
    }
}

/// The ids of the Profile objects on the bus.
pub async fn profile_objects(rig: &Rig) -> Vec<String> {
    let manager = zbus::fdo::ObjectManagerProxy::builder(&rig.client)
        .destination(nicecooling_api::BUS_NAME)
        .path(nicecooling_api::ROOT_PATH)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let prefix = format!("{}/profile/", nicecooling_api::ROOT_PATH);
    let mut ids: Vec<String> = manager
        .get_managed_objects()
        .await
        .unwrap()
        .keys()
        .filter_map(|p| p.as_str().strip_prefix(&prefix).map(str::to_owned))
        .collect();
    ids.sort();
    ids
}
