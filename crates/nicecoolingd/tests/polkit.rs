//! `PolkitAuthorizer` against a mock of `org.freedesktop.PolicyKit1` on the private bus.

mod support;

use std::sync::atomic::Ordering;

use nicecooling_api::channel::ChannelProxy;
use nicecooling_api::ids::Kind;
use support::{Rig, error_name, path};

fn running_as_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .is_some_and(|l| l.split_whitespace().next() == Some("0"))
}

async fn pwm1(rig: &Rig) -> ChannelProxy<'static> {
    ChannelProxy::builder(&rig.client)
        .path(path(Kind::Channel, "nct6798_pwm1"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn polkit_denial_returns_access_denied() {
    if running_as_root() {
        return; // uid 0 needs no check
    }
    let mut rig = Rig::start_with_polkit().await;
    rig.tick_in_background();
    let pwm1 = pwm1(&rig).await;
    pwm1.set_fixed_duty(50.0).await.unwrap();
    let polkit = rig.polkit.as_ref().unwrap();
    polkit.deny.store(true, Ordering::SeqCst);
    let e = pwm1.set_fixed_duty(60.0).await.unwrap_err();
    assert_eq!(error_name(&e), "org.freedesktop.DBus.Error.AccessDenied");
    assert_eq!(pwm1.fixed_duty().await.unwrap(), 50.0);
    // A read method asks polkit nothing.
    let calls = polkit.calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2);
    assert!(
        calls
            .iter()
            .all(|c| c.0 == "io.github.cappyt.NiceCooling1.write")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interactive_flag_passed_only_when_set() {
    if running_as_root() {
        return;
    }
    let mut rig = Rig::start_with_polkit().await;
    rig.tick_in_background();
    let pwm1 = pwm1(&rig).await;
    let proxy = zbus::Proxy::new(
        &rig.client,
        nicecooling_api::BUS_NAME,
        path(Kind::Channel, "nct6798_pwm1"),
        "io.github.cappyt.NiceCooling1.Channel",
    )
    .await
    .unwrap();
    pwm1.set_fixed_duty(10.0).await.unwrap();
    proxy
        .call_with_flags::<_, _, ()>(
            "SetFixedDuty",
            zbus::proxy::MethodFlags::ALLOW_INTERACTIVE_AUTH,
            &(20.0f64,),
        )
        .await
        .unwrap();
    // The polkit flag ALLOW_USER_INTERACTION is 1.
    let flags: Vec<u32> = rig
        .polkit
        .as_ref()
        .unwrap()
        .calls
        .lock()
        .unwrap()
        .iter()
        .map(|c| c.1)
        .collect();
    assert_eq!(flags, [0, 1]);
}
