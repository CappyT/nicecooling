//! `GetDeviceResponseProposals` on the simulated device: the daemon proposes and warns, and it
//! writes only what the user sends through `SetFields`.

use nicecooling_api::alarms::AlarmsProxy;
use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_protocol::registry::{Registry, VerificationState};
use nicecooling_protocol::sim::Request;
use nicecoolingd::leakshield::fields::FieldValue;

use crate::Rig as DeviceRig;
use crate::support::{LEAKSHIELD, Rig, error_name, path};

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";
const SMALL_LEAK_FIELD: &str = "leak_rate_alarm_1_response_action";
const SMALL_LEAK_OFFSET: usize = 557;
const LARGE_LEAK_OFFSET: usize = 561;
const SYSTEM_OFFSET: usize = 565;
const NONE: u8 = 0;
const SIGNAL_SHORT: u8 = 1;
const SIGNAL_LONG: u8 = 2;
const KEY_SEQUENCE: u8 = 3;
const SMALL: &str = "leakshield.leak-alarm-small";
const LARGE: &str = "leakshield.leak-alarm";
const SYSTEM: &str = "leakshield.system-alarm";

async fn leakshield(rig: &Rig) -> LeakshieldProxy<'static> {
    LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

fn set_feature_count(requests: &[Request]) -> usize {
    requests
        .iter()
        .filter(|r| matches!(r, Request::SetFeature { .. }))
        .count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn proposals_follow_the_device_and_the_alarm_responses() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    rig.sim.change_report3(SMALL_LEAK_OFFSET, &[KEY_SEQUENCE]);
    rig.sim.change_report3(LARGE_LEAK_OFFSET, &[SIGNAL_LONG]);
    rig.sim.change_report3(SYSTEM_OFFSET, &[NONE]);

    let rows = proxy.get_device_response_proposals().await.unwrap();
    let row = |event: &str| rows.iter().find(|r| r.0 == event).unwrap().clone();
    // The pump idle time Warning has no Device response, so it has no row.
    assert_eq!(rows.len(), 6);
    assert_eq!(
        row(SMALL),
        (
            SMALL.into(),
            "key-sequence".into(),
            "none".into(),
            "key-sequence-on".into()
        )
    );
    // The large leak powers off by default: a signal overlaps with it.
    assert_eq!(
        row(LARGE),
        (
            LARGE.into(),
            "signal-long".into(),
            "none".into(),
            "double-poweroff".into()
        )
    );
    assert_eq!(
        row(SYSTEM),
        (
            SYSTEM.into(),
            "none".into(),
            "signal-long".into(),
            String::new()
        )
    );

    // The user turns the alarm response of the large leak to notify: the overlap goes.
    let alarms = AlarmsProxy::new(&rig.client).await.unwrap();
    alarms.set_response(LARGE, "notify", 30).await.unwrap();
    let rows = proxy.get_device_response_proposals().await.unwrap();
    let large = rows.iter().find(|r| r.0 == LARGE).unwrap();
    assert_eq!((large.2.as_str(), large.3.as_str()), ("signal-long", ""));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_write_without_user_approval() {
    let mut rig = Rig::start_with_clock().await;
    let proxy = leakshield(&rig).await;
    rig.sim.change_report3(SMALL_LEAK_OFFSET, &[KEY_SEQUENCE]);
    rig.sim.change_report3(SYSTEM_OFFSET, &[SIGNAL_SHORT]);
    rig.step(2).await;
    let before = rig.sim.report3();
    rig.sim.clear_requests();

    // The proposals say "none" and "signal-long": the daemon holds them and writes nothing.
    let rows = proxy.get_device_response_proposals().await.unwrap();
    assert!(rows.iter().any(|r| r.0 == SMALL && r.2 == "none"));
    assert!(rows.iter().any(|r| r.0 == SYSTEM && r.2 == "signal-long"));
    rig.step(10).await;
    assert_eq!(set_feature_count(&rig.sim.requests()), 0);
    assert_eq!(rig.sim.report3(), before);
    assert_eq!(rig.sim.report3()[SMALL_LEAK_OFFSET], KEY_SEQUENCE);
}

#[tokio::test(start_paused = true)]
async fn accepted_proposal_uses_write_path() {
    // A registry where the field is verified: the write path takes the proposal like any edit.
    let mut registry = Registry::leakshield();
    registry
        .field
        .iter_mut()
        .find(|f| f.id == SMALL_LEAK_FIELD)
        .unwrap()
        .verification_state = VerificationState::VerifiedWrite;
    let rig = DeviceRig::with_registry("proposal-accepted", registry);
    rig.sim.change_report3(SMALL_LEAK_OFFSET, &[KEY_SEQUENCE]);
    let device = rig.attached();

    let proposals = nicecoolingd::alarms::proposals::rows(&device.get_fields().unwrap(), &[]);
    let proposed = &proposals.iter().find(|r| r.0 == SMALL).unwrap().2;
    let code = ["none", "signal-short", "signal-long", "key-sequence"]
        .iter()
        .position(|name| name == proposed)
        .unwrap();
    assert_eq!(set_feature_count(&rig.sim.requests()), 0);

    device
        .set_fields(vec![(
            SMALL_LEAK_FIELD.to_owned(),
            FieldValue::Number(code as f64),
        )])
        .await
        .unwrap();

    assert_eq!(rig.sim.report3()[SMALL_LEAK_OFFSET], NONE);
    assert!(set_feature_count(&rig.sim.requests()) >= 1);
    // The write path saved the state from before in a backup.
    assert_eq!(rig.backups().list().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn accepted_proposal_on_the_bus_meets_the_verification_rule() {
    // The response fields are Safety fields at `code-defined`: SetFields refuses them, as it
    // refuses every unverified Safety write.
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    rig.sim.change_report3(SMALL_LEAK_OFFSET, &[KEY_SEQUENCE]);
    rig.sim.clear_requests();
    let error = proxy
        .set_fields([(SMALL_LEAK_FIELD, zbus::Variant::F64(0.0))].into())
        .await
        .unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.NotVerified"));
    assert_eq!(set_feature_count(&rig.sim.requests()), 0);
}
