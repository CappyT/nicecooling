//! The activation of a Profile through the API (task 7.2).

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use nicecooling_api::channel::ChannelProxy;
use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_protocol::sim::Request;
use tokio::time::timeout;
use zbus::Variant;

use crate::support::{LEAKSHIELD, Rig, path};
use crate::{profile, profiles, slot_file, with_profile_3};

/// Activates a Profile by hand and waits for the end of the Leakshield part. Returns the state.
pub async fn switch(rig: &Rig, id: &str) -> String {
    let profiles = profiles(rig).await;
    let mut signals = profiles.receive_profile_activated().await.unwrap();
    profiles
        .activate_profile(&path(Kind::Profile, id))
        .await
        .unwrap();
    let signal = timeout(Duration::from_secs(5), signals.next())
        .await
        .expect("a ProfileActivated signal")
        .unwrap();
    let args = signal.args().unwrap();
    assert_eq!(args.profile, path(Kind::Profile, id));
    assert_eq!(args.cause, "manual");
    args.state.clone()
}

fn feature_writes(rig: &Rig) -> usize {
    rig.sim
        .requests()
        .iter()
        .filter(|r| matches!(r, Request::SetFeature { .. }))
        .count()
}

async fn channel(rig: &Rig, id: &str) -> ChannelProxy<'static> {
    ChannelProxy::builder(&rig.client)
        .path(path(Kind::Channel, id))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

fn read(rig: &Rig, file: &str) -> String {
    std::fs::read_to_string(rig.state.join(file)).unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn daemon_start_writes_nothing_to_leakshield() {
    // The active Profile has slot 3 and the device reports 1: a start would write if it applied.
    let mut rig = Rig::start_with(with_profile_3).await;
    rig.ticks(3).await;
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(feature_writes(&rig), 0);
    assert!(rig.sim.commands().is_empty());
    assert_eq!(rig.sim.report3()[747], 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_profiles_same_slot_write_only_curves() {
    let rig = Rig::start_with(with_profile_3).await;
    let copy = profiles(&rig)
        .await
        .create_profile("Same slot", &path(Kind::Profile, "profile_3"))
        .await
        .unwrap();
    let copy = copy.as_str().rsplit('/').next().unwrap().to_owned();
    let pwm1 = channel(&rig, "nct6798_pwm1").await;

    // The device has number 1: the first switch writes slot 3.
    assert_eq!(switch(&rig, &copy).await, "applied");
    assert_eq!(feature_writes(&rig), 1);
    assert_eq!(rig.sim.report3()[747], 3);
    rig.sim.edit_report1(|r| r.active_profile = 3);
    pwm1.set_fixed_duty(50.0).await.unwrap();
    assert_eq!(pwm1.control().await.unwrap(), "fixed");

    // The other Profile with slot 3 has its own Curves, and no Leakshield write follows.
    assert_eq!(switch(&rig, "profile_3").await, "applied");
    assert_eq!(pwm1.control().await.unwrap(), "hardware");
    assert_eq!(switch(&rig, &copy).await, "applied");
    assert_eq!(pwm1.control().await.unwrap(), "fixed");
    assert_eq!(feature_writes(&rig), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_slot_on_active_profile_applies_activation() {
    let rig = Rig::start_with(with_profile_3).await;
    rig.sim.edit_report1(|r| r.active_profile = 3);
    let target = profile(&rig, "profile_3").await;
    let profiles = profiles(&rig).await;
    let mut signals = profiles.receive_profile_activated().await.unwrap();

    target.set_slot(4).await.unwrap();

    let signal = timeout(Duration::from_secs(5), signals.next())
        .await
        .expect("a ProfileActivated signal")
        .unwrap();
    assert_eq!(signal.args().unwrap().state, "applied");
    assert_eq!(
        rig.sim.report3(),
        std::fs::read(slot_file(&rig, 4)).unwrap()
    );
    assert_eq!(rig.sim.report3()[747], 4);

    // A slot change of a Profile that is not active writes nothing.
    rig.sim.clear_requests();
    profile(&rig, "profile_2").await.set_slot(1).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(feature_writes(&rig), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_error_gives_curves_applied_leakshield_failed() {
    let rig = Rig::start_with(|fake, state| {
        with_profile_3(fake, state);
        std::fs::write(
            state.join("profiles/profile_2/profile.json"),
            r#"{"schema_version": 1, "id": "profile_2", "name": "Two", "slot": 2,
                "channels": {"nct6798_pwm1": {"control": "fixed", "input": null, "points": [],
                "smoothing_s": null, "step_limit_pct_per_s": null, "fixed_duty": 50}}}"#,
        )
        .unwrap();
    })
    .await;
    rig.sim.fail_feature_writes(true);
    let profiles = profiles(&rig).await;

    assert_eq!(switch(&rig, "profile_2").await, "leakshield-failed");

    assert_eq!(
        profiles.active().await.unwrap(),
        path(Kind::Profile, "profile_2")
    );
    assert_eq!(profiles.active_state().await.unwrap(), "leakshield-failed");
    assert_eq!(profiles.activation_cause().await.unwrap(), "manual");
    // The Curves of the Profile are applied.
    let pwm1 = channel(&rig, "nct6798_pwm1").await;
    assert_eq!(pwm1.control().await.unwrap(), "fixed");
    assert_eq!(pwm1.fixed_duty().await.unwrap(), 50.0);

    // The next activation that works clears the state.
    rig.sim.fail_feature_writes(false);
    assert_eq!(switch(&rig, "profile_4").await, "applied");
    assert_eq!(profiles.active_state().await.unwrap(), "applied");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn leakshield_change_goes_into_slot_of_active_profile() {
    let rig = Rig::start_with(with_profile_3).await;
    let proxy = LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let slot_2 = std::fs::read(slot_file(&rig, 2)).unwrap();

    let fields: HashMap<&str, Variant<'static>> =
        [("display_page_time", Variant::F64(30.0))].into();
    proxy.set_fields(fields).await.unwrap();

    // Slot 3 holds the verified read-back, with its own number. The other slots stay.
    let stored = std::fs::read(slot_file(&rig, 3)).unwrap();
    assert_eq!(&stored[63..65], 300u16.to_be_bytes());
    assert_eq!(stored[747], 3);
    assert_eq!(std::fs::read(slot_file(&rig, 2)).unwrap(), slot_2);
    assert_eq!(&slot_2[63..65], 100u16.to_be_bytes());
    // A Profile with slot 3 shows the new value.
    let content = profile(&rig, "profile_3")
        .await
        .get_content()
        .await
        .unwrap();
    assert!(format!("{:?}", content["leakshield_fields"]).contains("300"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn curve_change_goes_into_active_profile() {
    let rig = Rig::start_with(with_profile_3).await;
    let pwm1 = channel(&rig, "nct6798_pwm1").await;
    pwm1.set_curve(
        &path(Kind::Sensor, "nct6798_temp1"),
        &[(20.0, 10.0), (40.0, 50.0)],
        0.0,
        100.0,
    )
    .await
    .unwrap();
    rig.daemon.as_ref().unwrap().hub.flush_now();

    assert!(read(&rig, "profiles/profile_3/profile.json").contains("nct6798_temp1"));
    // No other Profile changed.
    assert!(!read(&rig, "profiles/profile_default/profile.json").contains("nct6798_temp1"));
    assert!(!read(&rig, "profiles/profile_2/profile.json").contains("nct6798_temp1"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn curve_change_goes_into_default_when_no_group_wins() {
    let rig = Rig::start_with(with_profile_3).await;
    // No Rule group exists and the user returns to the Default Profile.
    assert_eq!(switch(&rig, "profile_default").await, "applied");
    let pwm2 = channel(&rig, "nct6798_pwm2").await;
    pwm2.set_fixed_duty(60.0).await.unwrap();
    rig.daemon.as_ref().unwrap().hub.flush_now();

    assert!(read(&rig, "profiles/profile_default/profile.json").contains("\"fixed_duty\": 60"));
    assert!(!read(&rig, "profiles/profile_3/profile.json").contains("\"fixed_duty\": 60"));
}

// The end of the Profile dwell (design D18).

fn tick(rig: &mut Rig) {
    rig.clock
        .as_ref()
        .expect("a rig with a simulated clock")
        .advance(Duration::from_secs(1));
    rig.daemon.as_mut().unwrap().runner.tick();
}

fn active(rig: &Rig) -> String {
    rig.daemon.as_ref().unwrap().hub.profiles_state().active
}

fn wall_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64
}

/// A rig whose group for `profile_2` holds all the time, with a Rule delay of 10 s.
async fn rule_rig(dwell_s: u32) -> Rig {
    let mut rig = Rig::start_with_clock_and(crate::automatic).await;
    let proxy = profiles(&rig).await;
    proxy.set_rule_timing(10, dwell_s).await.unwrap();
    let two = path(Kind::Profile, "profile_2");
    proxy
        .set_rule_groups(vec![crate::rules::group(
            "group_1",
            &two,
            true,
            vec![crate::rules::always()],
        )])
        .await
        .unwrap();
    for _ in 0..40 {
        tick(&mut rig);
        if active(&rig) == "profile_2" {
            return rig;
        }
    }
    panic!("the group did not switch the Profile");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dwell_until_set_and_cleared_at_end() {
    let mut rig = rule_rig(60).await;
    let proxy = profiles(&rig).await;
    tick(&mut rig);

    // The switch has set it: the time of the switch plus the dwell of 60 s.
    let until = proxy.dwell_until().await.unwrap();
    let now = wall_us();
    assert!(
        (now + 55_000_000..=now + 61_000_000).contains(&until),
        "{until} against {now}"
    );
    // It does not move while the dwell runs.
    for _ in 0..30 {
        tick(&mut rig);
    }
    assert_eq!(proxy.dwell_until().await.unwrap(), until);

    // Immediate switching removes the dwell, and the end comes back when it is off again.
    proxy.set_immediate_switching(true).await.unwrap();
    tick(&mut rig);
    assert_eq!(proxy.dwell_until().await.unwrap(), 0);
    proxy.set_immediate_switching(false).await.unwrap();
    tick(&mut rig);
    assert_ne!(proxy.dwell_until().await.unwrap(), 0);

    // A longer dwell moves the end to the time of the switch plus the new dwell. The wall clock
    // runs in real time and the tick clock is simulated, so only the order shows here.
    proxy.set_rule_timing(10, 100).await.unwrap();
    tick(&mut rig);
    let longer = proxy.dwell_until().await.unwrap();
    assert!(longer > until, "{longer} against {until}");

    // The group goes, and the Default Profile returns when the dwell ends: the property is 0.
    proxy.set_rule_groups(Vec::new()).await.unwrap();
    for _ in 0..110 {
        tick(&mut rig);
    }
    assert_eq!(active(&rig), "profile_default");
    assert_eq!(proxy.dwell_until().await.unwrap(), 0);

    // A manual activation has no dwell.
    switch_manually(&rig, "profile_3").await;
    tick(&mut rig);
    assert_eq!(proxy.dwell_until().await.unwrap(), 0);
}

async fn switch_manually(rig: &Rig, id: &str) {
    profiles(rig)
        .await
        .activate_profile(&path(Kind::Profile, id))
        .await
        .unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dwell_until_counts_from_daemon_start() {
    let mut rig = rule_rig(300).await;
    // 200 s of the dwell pass before the restart: 100 s remain.
    for _ in 0..200 {
        tick(&mut rig);
    }
    assert_ne!(
        rig.daemon.as_ref().unwrap().hub.dwell_until(),
        0,
        "the dwell runs"
    );

    rig.restart_daemon().await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    hub.set_rule_groups(Vec::new()).unwrap();
    tick(&mut rig);
    let until = hub.dwell_until();
    let now = wall_us();
    assert!(
        (now + 295_000_000..=now + 301_000_000).contains(&until),
        "{until} against {now}"
    );
    // After 250 s the old dwell would be over, and the new one still runs.
    for _ in 0..250 {
        tick(&mut rig);
    }
    assert_eq!(hub.dwell_until(), until);
    for _ in 0..60 {
        tick(&mut rig);
    }
    assert_eq!(hub.dwell_until(), 0);
}
