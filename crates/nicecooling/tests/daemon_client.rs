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
    assert_eq!(snapshot.channels["nct6798_pwm6"].name, "Waterpump");
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
            vec![("nct6798_pwm6".into(), 62.0, 45.5, 70.0, 4)],
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
    assert_eq!(
        sample.channels,
        [("nct6798_pwm6".to_owned(), 62.0, 45.5, 70.0, 4)]
    );
    assert_eq!(sample.channel_readout("nct6798_pwm6"), Some((45.5, 70.0)));

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
fn decode_alarms_reads_the_row_of_the_xml_with_its_details() {
    use std::collections::HashMap;
    use zbus::{OwnedVariant, Variant};
    // The XML signature is `a(ssssstta{sv})`: 5 strings, 2 times and the details.
    let mut details: HashMap<String, OwnedVariant> = HashMap::new();
    details.insert(
        "value".into(),
        OwnedVariant::try_from(Variant::from(212.0_f64)).expect("variant"),
    );
    details.insert(
        "limit".into(),
        OwnedVariant::try_from(Variant::from(250.0_f64)).expect("variant"),
    );
    details.insert(
        "unit".into(),
        OwnedVariant::try_from(Variant::from("mbar")).expect("variant"),
    );
    let sensor = zbus::ObjectPath::try_from(format!("{ROOT}/sensor/leak_pressure")).expect("path");
    details.insert(
        "sensor".into(),
        OwnedVariant::try_from(Variant::from(sensor)).expect("variant"),
    );
    let row = (
        "a1",
        "pump-stop",
        "alarm",
        "loop_1",
        "poweroff",
        5_u64,
        9_u64,
        details,
    );
    let value = OwnedVariant::try_from(Variant::new(vec![row])).expect("variant");
    let entries = decode_alarms(&value);
    assert_eq!(entries.len(), 1);
    assert_eq!(
        (
            entries[0].instance.as_str(),
            entries[0].raised_us,
            entries[0].poweroff_at_us
        ),
        ("a1", 5, 9)
    );
    let details = &entries[0].details;
    assert_eq!(details.value, Some(212.0));
    assert_eq!(details.limit, Some(250.0));
    assert_eq!(details.unit, "mbar");
    assert_eq!(details.sensor.as_deref(), Some("leak_pressure"));
}

/// A connection that sees every message of the bus.
async fn monitor(bus: &PrivateBus) -> (zbus::Connection, zbus::MessageStream) {
    let conn = bus.connect().await;
    zbus::fdo::MonitoringProxy::new(&conn)
        .await
        .expect("monitoring proxy")
        .become_monitor(&[], 0)
        .await
        .expect("become monitor");
    let stream = zbus::MessageStream::from(&conn);
    (conn, stream)
}

/// The method calls to the daemon that the monitor saw until it is quiet for 300 ms.
async fn calls_to_daemon(stream: &mut zbus::MessageStream) -> Vec<String> {
    use futures_util::StreamExt;
    let mut calls = Vec::new();
    while let Ok(Some(Ok(message))) =
        tokio::time::timeout(std::time::Duration::from_millis(300), stream.next()).await
    {
        let header = message.header();
        if header.message_type() == zbus::message::Type::MethodCall
            && header.destination().map(|d| d.as_str())
                == Some(nicecooling::daemon_client::BUS_NAME)
        {
            calls.push(format!(
                "{}.{}",
                header.interface().map(|i| i.as_str()).unwrap_or_default(),
                header.member().map(|m| m.as_str()).unwrap_or_default()
            ));
        }
    }
    calls
}

#[tokio::test(flavor = "multi_thread")]
async fn last_feed_change_triggers_no_other_call() {
    let (bus, daemon, mut harness) = online().await;
    let (_monitor, mut stream) = monitor(&bus).await;
    // The Feed changes about once a second. Two changes: the second one proves that the client
    // handled the first one completely.
    daemon
        .set_last_feed(1_700_000_001_000_000, 2900.0, 120.0)
        .await;
    daemon
        .set_last_feed(1_700_000_002_000_000, 2910.0, 121.0)
        .await;
    harness
        .until("the Feed values reach the state", |state| {
            state.snapshot.leakshields[0].last_feed.time_us == 1_700_000_002_000_000
        })
        .await;
    let feed = harness.state.snapshot.leakshields[0].last_feed;
    assert_eq!((feed.feed_pump, feed.flow_lph), (2910.0, 121.0));
    let calls = calls_to_daemon(&mut stream).await;
    assert!(calls.is_empty(), "the client made calls: {calls:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_property_change_updates_only_its_property() {
    let (bus, daemon, mut harness) = online().await;
    let before = harness.state.snapshot.clone();
    let (_monitor, mut stream) = monitor(&bus).await;
    daemon.set_dwell_until(1_700_000_300_000_000).await;
    harness
        .until("the dwell shows", |state| {
            state.snapshot.dwell_until_us == 1_700_000_300_000_000
        })
        .await;
    let calls = calls_to_daemon(&mut stream).await;
    assert!(calls.is_empty(), "the client made calls: {calls:?}");
    // Nothing else of the snapshot changed.
    let mut expected = before;
    expected.dwell_until_us = 1_700_000_300_000_000;
    assert_eq!(harness.state.snapshot, expected);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_leakshield_property_reads_the_device_data_again() {
    let (bus, daemon, mut harness) = online().await;
    let (_monitor, mut stream) = monitor(&bus).await;
    daemon.set_active_slot(2).await;
    harness
        .until("the slot shows", |state| {
            state.snapshot.leakshields[0].active_slot == 2
        })
        .await;
    // `ActiveSlot` is a Leakshield property that is not `LastFeed` or `FeedState`: the stored
    // slots and the fields are read again.
    let calls = calls_to_daemon(&mut stream).await;
    assert!(
        calls
            .iter()
            .any(|call| call.ends_with("Leakshield.GetFields")),
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .all(|call| !call.ends_with("Manager.GetCalibrationTables")),
        "{calls:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn interfaces_added_read_everything_again() {
    let (bus, daemon, mut harness) = online().await;
    let (_monitor, mut stream) = monitor(&bus).await;
    harness
        .client
        .create_profile("Gaming", "")
        .await
        .expect("create a Profile");
    harness
        .until("the new Profile shows", |state| {
            state.snapshot.profiles.iter().any(|p| p.name == "Gaming")
        })
        .await;
    let calls = calls_to_daemon(&mut stream).await;
    assert!(
        calls
            .iter()
            .any(|call| call.ends_with("Manager.GetCalibrationTables")),
        "a full read: {calls:?}"
    );
    drop(daemon);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_device_read_does_not_block_the_signals() {
    let (_bus, daemon, mut harness) = online().await;
    // The read of the device data waits at `GetFields`.
    let gate = daemon.hold_device_reads().await;
    daemon.set_active_slot(2).await;
    harness
        .until("the slot shows", |state| {
            state.snapshot.leakshields[0].active_slot == 2
        })
        .await;
    // A `SensorsUpdated` that arrives while the read waits is applied at once.
    daemon
        .emit_sensors(
            4_000_000,
            vec![("nct6798_temp1".into(), 33.3, 0)],
            Vec::new(),
        )
        .await;
    harness
        .until("the sample is applied during the read", |state| {
            state
                .sensors
                .as_ref()
                .is_some_and(|sample| sample.time_us == 4_000_000)
        })
        .await;
    // Two more changes during the read give one more read, not two.
    daemon.set_failed_write_backup("backup_1").await;
    daemon.set_failed_write_backup("backup_2").await;
    harness
        .until("the backup shows", |state| {
            state.snapshot.leakshields[0].failed_write_backup == "backup_2"
        })
        .await;
    gate.add_permits(10);
    harness
        .until("the read ends", |state| {
            state
                .snapshot
                .leakshield_data
                .get(LEAKSHIELD)
                .is_some_and(|data| !data.fields.is_empty())
        })
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn only_some_leakshield_properties_read_the_device() {
    let (bus, daemon, mut harness) = online().await;
    let (_monitor, mut stream) = monitor(&bus).await;
    // `Mode` is not one of the properties that change the device data.
    daemon.set_mode("monitor").await;
    harness
        .until("the Mode shows", |state| {
            state.snapshot.leakshields[0].mode == "monitor"
        })
        .await;
    let calls = calls_to_daemon(&mut stream).await;
    assert!(calls.is_empty(), "the client made calls: {calls:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_disconnected_leakshield_loses_its_device_data() {
    let (_bus, daemon, mut harness) = online().await;
    assert!(
        !harness.state.snapshot.leakshield_data[LEAKSHIELD]
            .fields
            .is_empty()
    );
    daemon.set_connected(false).await;
    harness
        .until("the device data go", |state| {
            state
                .snapshot
                .leakshield_data
                .get(LEAKSHIELD)
                .is_none_or(|data| data.fields.is_empty())
        })
        .await;
    assert!(!harness.state.snapshot.leakshields[0].connected);
    assert_eq!(harness.state.snapshot.leakshields[0].field_counts, None);
    // It comes back: the data are read again.
    daemon.set_connected(true).await;
    harness
        .until("the device data come back", |state| {
            state
                .snapshot
                .leakshield_data
                .get(LEAKSHIELD)
                .is_some_and(|data| !data.fields.is_empty())
        })
        .await;
}
