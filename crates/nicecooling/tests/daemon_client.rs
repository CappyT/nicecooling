mod support;

use nicecooling::daemon_client::{ROOT, decode_alarms};
use nicecooling::model::{Cause, DaemonEvent, Response, Severity};
use support::bus::PrivateBus;
use support::fake_daemon::{FakeDaemon, LEAKSHIELD, Options};
use support::harness::Harness;

async fn online() -> (PrivateBus, FakeDaemon, Harness) {
    let bus = PrivateBus::start("client");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the daemon is online", |state| state.is_online())
        .await;
    (bus, daemon, harness)
}

#[tokio::test(flavor = "multi_thread")]
async fn snapshot_reads_the_objects_of_the_daemon() {
    let (_bus, _daemon, harness) = online().await;
    let snapshot = &harness.state.snapshot;
    assert_eq!(
        snapshot.active_profile.as_ref().map(|p| p.name.as_str()),
        Some("Default")
    );
    assert_eq!(snapshot.cause, Cause::Default);
    assert_eq!(snapshot.profiles.len(), 2);
    assert_eq!(snapshot.loops.len(), 1);
    assert_eq!(snapshot.loops[0].name, "Main loop");
    assert_eq!(snapshot.loops[0].leakshield.as_deref(), Some(LEAKSHIELD));
    assert_eq!(snapshot.loops[0].flow.as_deref(), Some("loop_1_flow"));
    assert_eq!(
        snapshot.loops[0].water_temperature.as_deref(),
        Some("nct6798_temp1")
    );
    assert_eq!(snapshot.sensors["loop_1_flow"].unit, "l/h");
    assert_eq!(snapshot.channels["nct6798_pwm6"], "Waterpump");
    assert_eq!(snapshot.leakshields[0].firmware, 1040);
    assert!(snapshot.leakshields[0].connected);
    assert!(snapshot.alarms.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn alarm_signals_reach_the_app_state() {
    let (_bus, daemon, mut harness) = online().await;
    daemon
        .raise_alarm(
            "a1",
            "leakshield.leak-alarm",
            "alarm",
            LEAKSHIELD,
            "poweroff",
            30_000_000,
        )
        .await;
    let event = harness
        .next_event("the Alarm", |event| {
            matches!(event, DaemonEvent::AlarmRaised(_))
        })
        .await;
    let DaemonEvent::AlarmRaised(entry) = event else {
        unreachable!()
    };
    assert_eq!(entry.event, "leakshield.leak-alarm");
    assert_eq!(entry.severity, Severity::Alarm);
    assert_eq!(entry.response, Response::Poweroff);
    assert_eq!(entry.poweroff_at_us, 30_000_000);
    // The property change makes a full read: it holds the raise time.
    harness
        .until("the full read", |state| {
            state
                .snapshot
                .alarms
                .first()
                .is_some_and(|a| a.raised_us == 1)
        })
        .await;
    assert_eq!(harness.state.alarm_count(), 1);

    daemon.clear_alarm("a1").await;
    harness
        .until("the Alarm clears", |state| state.snapshot.alarms.is_empty())
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_poweroff_carries_the_interactive_flag() {
    let (_bus, daemon, mut harness) = online().await;
    daemon
        .raise_alarm("a1", "pump-stop", "alarm", "loop_1", "poweroff", 30_000_000)
        .await;
    harness
        .until("the Alarm", |state| state.next_poweroff().is_some())
        .await;

    harness.client.cancel_poweroff("a1").await.expect("cancel");
    assert_eq!(daemon.calls(), ["CancelPoweroff(a1) interactive=true"]);
    harness
        .next_event("the cancel", |event| {
            matches!(event, DaemonEvent::PoweroffCancelled { .. })
        })
        .await;
    assert!(harness.state.next_poweroff().is_none());
    assert_eq!(harness.state.alarm_count(), 1, "the Alarm stays active");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_write_returns_the_bus_error() {
    let (_bus, daemon, harness) = online().await;
    daemon.deny_writes(true);
    let error = harness
        .client
        .cancel_poweroff("a1")
        .await
        .expect_err("denied");
    assert!(
        matches!(&error, zbus::Error::FDO(inner) if matches!(**inner, zbus::fdo::Error::AccessDenied(_))),
        "{error:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn profile_writes_update_the_state() {
    let (_bus, daemon, mut harness) = online().await;
    harness
        .client
        .activate_profile("profile_2")
        .await
        .expect("activate");
    harness
        .until("manual activation", |state| {
            state.snapshot.cause == Cause::Manual
                && state
                    .snapshot
                    .active_profile
                    .as_ref()
                    .is_some_and(|p| p.name == "Quiet")
        })
        .await;
    harness.client.return_to_automatic().await.expect("return");
    harness
        .until("automatic", |state| state.snapshot.cause == Cause::Default)
        .await;
    assert_eq!(
        daemon.calls(),
        [
            "ActivateProfile(profile_2) interactive=true",
            "ReturnToAutomatic() interactive=true"
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn sensor_and_external_change_signals_decode() {
    let (_bus, daemon, mut harness) = online().await;
    daemon
        .emit_sensors(
            7_000_000,
            vec![
                ("nct6798_temp1".into(), 31.4, 0),
                ("loop_1_flow".into(), 0.0, 1),
            ],
            vec![("nct6798_pwm6".into(), 62.0, 0)],
        )
        .await;
    harness
        .until("a sample", |state| state.sensors.is_some())
        .await;
    let sample = harness.state.sensors.clone().expect("sample");
    assert_eq!(sample.time_us, 7_000_000);
    assert_eq!(sample.sensor_value("nct6798_temp1"), Some(31.4));
    assert_eq!(
        sample.sensor_value("loop_1_flow"),
        None,
        "flag bit 0: no value"
    );
    assert_eq!(sample.channels, [("nct6798_pwm6".to_owned(), 62.0, 0)]);

    daemon
        .emit_external_change(vec!["display_brightness".into()])
        .await;
    let event = harness
        .next_event("external change", |event| {
            matches!(event, DaemonEvent::ExternalChange { .. })
        })
        .await;
    assert_eq!(
        event,
        DaemonEvent::ExternalChange {
            leakshield: LEAKSHIELD.into(),
            fields: vec!["display_brightness".into()]
        }
    );
}

#[test]
fn decode_alarms_reads_the_row_of_the_xml_and_the_row_of_the_comment() {
    use zbus::{OwnedVariant, Variant};
    // The XML signature is `a(sssssst)`: 6 strings and one time. The comment lists 7 fields.
    let xml_row = ("a1", "pump-stop", "alarm", "loop_1", "poweroff", "", 9_u64);
    let value = OwnedVariant::try_from(Variant::new(vec![xml_row])).expect("variant");
    let entries = decode_alarms(&value);
    assert_eq!(entries.len(), 1);
    assert_eq!((entries[0].raised_us, entries[0].poweroff_at_us), (0, 9));
    let comment_row = (
        "a2",
        "pump-stop",
        "alarm",
        "loop_1",
        "poweroff",
        5_u64,
        9_u64,
    );
    let value = OwnedVariant::try_from(Variant::new(vec![comment_row])).expect("variant");
    let entries = decode_alarms(&value);
    assert_eq!(
        (
            entries[0].instance.as_str(),
            entries[0].raised_us,
            entries[0].poweroff_at_us
        ),
        ("a2", 5, 9)
    );
    assert!(ROOT.starts_with('/'));
}
