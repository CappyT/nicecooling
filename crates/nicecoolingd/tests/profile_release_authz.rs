//! Saving a Profile with Mode RELEASE needs no admin password (ADR-0007), although setting the
//! Mode RELEASE directly does.

mod support;

use nicecooling_api::authz_map::Action;
use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_api::profile::ProfileProxy;
use nicecooling_api::profiles::ProfilesProxy;
use support::{LEAKSHIELD, Rig, none_path, path};

async fn profile(rig: &Rig, id: &str) -> ProfileProxy<'static> {
    ProfileProxy::builder(&rig.client)
        .path(path(Kind::Profile, id))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn save_profile_with_release_asks_write_only() {
    let rig = Rig::start().await;
    let profiles = ProfilesProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    // The settings of the reference device start in Mode RELEASE after power-up
    // (`default_mode` 3): every slot holds it, so every Profile contains RELEASE.
    let default = profile(&rig, "profile_default").await;
    assert!(default.contains_release().await.unwrap());

    // Every way to save a Profile asks `write`, and never `critical`.
    let created = profiles
        .create_profile("With RELEASE", &none_path())
        .await
        .unwrap();
    let id = created.as_str().rsplit('/').next().unwrap().to_owned();
    let release = profile(&rig, &id).await;
    assert!(release.contains_release().await.unwrap());
    release.set_name("Renamed").await.unwrap();
    release.set_slot(2).await.unwrap();
    profiles.activate_profile(&created).await.unwrap();
    profiles
        .activate_profile(&path(Kind::Profile, "profile_default"))
        .await
        .unwrap();
    release.delete().await.unwrap();
    assert_eq!(rig.authorizer.calls(), [(Action::Write, false); 6]);

    // Setting RELEASE directly asks `critical`.
    rig.authorizer.clear();
    let leakshield = LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    leakshield.set_mode("release").await.unwrap();
    assert_eq!(rig.authorizer.calls(), [(Action::Critical, false)]);
}
