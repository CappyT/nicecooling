//! The `Leakshield` interface on the private bus, with the simulated device behind it.

use std::time::Duration;

use futures_util::StreamExt;
use futures_util::future::join_all;
use nicecooling_api::authz_map::Action;
use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_api::loops::LoopProxy;
use nicecooling_api::sensor::SensorProxy;
use tokio::time::timeout;
use zbus::{OwnedVariant, Variant};

use crate::support::{LEAKSHIELD, Rig, error_name, none_path, path};

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";
const WRITABLE: &str = "display_page_time";
const NOT_VERIFIED: &str = "shield_pressure_low";

async fn leakshield(rig: &Rig) -> LeakshieldProxy<'static> {
    LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

fn running_as_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .is_some_and(|l| l.split_whitespace().next() == Some("0"))
}

/// The message of a failed call.
fn message(error: &zbus::Error) -> String {
    match error {
        zbus::Error::MethodError(_, Some(text), _) => text.clone(),
        other => format!("{other:?}"),
    }
}

fn number(value: &OwnedVariant) -> f64 {
    f64::try_from(value.try_clone().unwrap()).unwrap()
}

fn fields(id: &str, value: f64) -> std::collections::HashMap<&str, Variant<'static>> {
    [(id, Variant::F64(value))].into()
}

/// Waits until the daemon holds a last known report 3, so that a change behind its back counts.
async fn wait_for_last_known(rig: &Rig) {
    let file = rig
        .state
        .join("leakshield")
        .join(LEAKSHIELD)
        .join("last-known-report3.bin");
    for _ in 0..200 {
        if file.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no last known report 3");
}

async fn until<T>(mut check: impl AsyncFnMut() -> Option<T>) -> T {
    for _ in 0..200 {
        if let Some(found) = check().await {
            return found;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the condition did not hold in 5 s");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_fields_reports_risk_class_and_writable() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;

    let rows = proxy.get_fields().await.unwrap();
    let row = |id: &str| rows.iter().find(|r| r.0 == id).unwrap();
    let report3 = rig.sim.report3();
    let raw = u16::from_be_bytes([report3[63], report3[64]]);
    let writable_row = row(WRITABLE);
    assert_eq!(writable_row.1, "low");
    assert!(writable_row.3);
    assert_eq!(writable_row.4, "");
    assert!((number(&writable_row.2) - f64::from(raw) * 0.1).abs() < 1e-9);
    let safety = row(NOT_VERIFIED);
    assert_eq!((safety.1.as_str(), safety.3), ("safety", false));
    assert_eq!(safety.4, "not-verified");

    // Firmware 1019 is read-only as a whole.
    rig.sim.edit_report1(|r| r.firmware_version = 1019);
    let rows = proxy.get_fields().await.unwrap();
    assert!(rows.iter().all(|r| !r.3 && r.4 == "unknown-firmware"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_fields_errors() {
    let name = |e: &zbus::Error| error_name(e);

    // NotVerified: the message holds the field id.
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let error = proxy
        .set_fields(fields(NOT_VERIFIED, 100.0))
        .await
        .unwrap_err();
    assert_eq!(name(&error), format!("{ERR}.NotVerified"));
    assert_eq!(message(&error), NOT_VERIFIED);

    // ReadOnly: the message holds the reason.
    rig.sim.edit_report1(|r| r.firmware_version = 1019);
    let error = proxy.set_fields(fields(WRITABLE, 30.0)).await.unwrap_err();
    assert_eq!(name(&error), format!("{ERR}.ReadOnly"));
    assert_eq!(message(&error), "unknown-firmware");

    // Busy: the worker holds one write.
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let results =
        join_all((0..6).map(|i| proxy.set_fields(fields(WRITABLE, 20.0 + f64::from(i))))).await;
    assert!(results.iter().any(Result::is_ok));
    assert!(
        results
            .iter()
            .any(|r| r.as_ref().is_err_and(|e| name(e) == format!("{ERR}.Busy")))
    );

    // DeviceChanged: another tool changed report 3 since the daemon read it last.
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    wait_for_last_known(&rig).await;
    rig.sim.change_report3(300, &[0x55]);
    let error = proxy.set_fields(fields(WRITABLE, 30.0)).await.unwrap_err();
    assert_eq!(name(&error), format!("{ERR}.DeviceChanged"));

    // VerifyFailed: the message holds the backup id.
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    wait_for_last_known(&rig).await;
    rig.sim.tamper_next_set_feature(300, 0x55);
    let error = proxy.set_fields(fields(WRITABLE, 30.0)).await.unwrap_err();
    assert_eq!(name(&error), format!("{ERR}.VerifyFailed"));
    let backups = proxy.list_backups().await.unwrap();
    assert!(backups.iter().any(|b| b.0 == message(&error)));

    // Disconnected.
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    rig.sim.disconnect();
    let error = proxy.set_fields(fields(WRITABLE, 30.0)).await.unwrap_err();
    assert_eq!(name(&error), format!("{ERR}.Disconnected"));

    // A value that is no number, and an unknown field.
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let error = proxy
        .set_fields([(WRITABLE, Variant::from("fast"))].into())
        .await
        .unwrap_err();
    assert_eq!(name(&error), format!("{ERR}.InvalidArgument"));
    let error = proxy.set_fields(fields("nope", 1.0)).await.unwrap_err();
    assert_eq!(name(&error), format!("{ERR}.NotFound"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_fields_writes_the_field_and_reads_it_back() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;

    proxy.set_fields(fields(WRITABLE, 30.0)).await.unwrap();

    assert_eq!(&rig.sim.report3()[63..65], &300u16.to_be_bytes());
    let rows = proxy.get_fields().await.unwrap();
    let row = rows.iter().find(|r| r.0 == WRITABLE).unwrap();
    assert!((number(&row.2) - 30.0).abs() < 1e-9);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn start_action_refill_returns_pump_running() {
    let mut rig = Rig::start().await;
    rig.sim.edit_report1(|r| r.mode = 1);
    rig.tick_in_background();
    let proxy = leakshield(&rig).await;
    // `loop_1` gets the Leakshield and the pump `nct6798_pwm1`, which reads 1200 rpm on `fan1`.
    let loop_1 = LoopProxy::builder(&rig.client)
        .path(path(Kind::Loop, "loop_1"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    loop_1
        .set_pump_channels(&[path(Kind::Channel, "nct6798_pwm1")])
        .await
        .unwrap();
    loop_1
        .set_leakshield(&path(Kind::Leakshield, LEAKSHIELD))
        .await
        .unwrap();
    let fan1 = SensorProxy::builder(&rig.client)
        .path(path(Kind::Sensor, "nct6798_fan1"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    until(async || (fan1.value().await.unwrap() > 0.0).then_some(())).await;

    let error = proxy.start_action("refill").await.unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.PumpRunning"));
    assert!(rig.sim.commands().is_empty());

    // The pump stops: the Refill starts.
    // The fake tree makes `fan1_input` read-only: replace the file.
    let fan1_file = rig.fake.attr_path("nct6798", "fan1_input");
    std::fs::remove_file(&fan1_file).unwrap();
    std::fs::write(&fan1_file, "0\n").unwrap();
    until(async || (fan1.value().await.unwrap() == 0.0).then_some(())).await;
    proxy.start_action("refill").await.unwrap();
    assert_eq!(rig.sim.commands().pop().map(|c| c.parameter), Some(7));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pressure_and_fill_level_sensors_exposed() {
    let mut rig = Rig::start().await;
    rig.tick_in_background();
    let sensor = |id: &str| {
        let client = rig.client.clone();
        let path = path(Kind::Sensor, &format!("{LEAKSHIELD}_{id}"));
        async move {
            SensorProxy::builder(&client)
                .path(path)
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()
                .await
                .unwrap()
        }
    };
    let (pressure, fill) = (sensor("pressure").await, sensor("fill_level").await);

    until(async || pressure.has_value().await.unwrap().then_some(())).await;
    // Report 1 of the fixture: 4 x 0.1 mbar and 7795 x 0.01 %.
    assert!((pressure.value().await.unwrap() - 0.4).abs() < 1e-9);
    assert!((fill.value().await.unwrap() - 77.95).abs() < 1e-9);
    assert_eq!(pressure.unit().await.unwrap(), "mbar");
    assert_eq!(pressure.kind().await.unwrap(), "pressure");
    assert_eq!(pressure.origin().await.unwrap(), "leakshield");
    assert_eq!(fill.unit().await.unwrap(), "%");
    assert_eq!(fill.kind().await.unwrap(), "level");

    // The device reports 12.3 mbar.
    rig.sim.edit_report1(|r| r.pressure = 123);
    until(async || ((pressure.value().await.unwrap() - 12.3).abs() < 1e-9).then_some(())).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn mode_and_running_action_properties() {
    let mut rig = Rig::start().await;
    rig.sim.edit_report1(|r| r.mode = 1);
    rig.tick_in_background();
    let proxy = leakshield(&rig).await;
    // `PropertiesChanged` reaches a proxy that caches properties.
    let cached = LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .build()
        .await
        .unwrap();
    let mut modes = cached.receive_mode_changed().await;

    proxy.set_mode("monitor").await.unwrap();
    assert_eq!(proxy.mode().await.unwrap(), "monitor");
    let changed = timeout(Duration::from_secs(5), modes.next())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(changed.get().await.unwrap(), "monitor");

    proxy.start_action("test").await.unwrap();
    assert_eq!(proxy.running_action().await.unwrap(), "test");
    assert_eq!(proxy.mode().await.unwrap(), "monitor");

    // The Action ends on the device: the poll of report 1 shows it.
    rig.sim.finish_action();
    until(async || {
        proxy
            .running_action()
            .await
            .unwrap()
            .is_empty()
            .then_some(())
    })
    .await;
    assert_eq!(proxy.mode().await.unwrap(), "monitor");

    let error = proxy.set_mode("Monitor").await.unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.InvalidArgument"));
    assert!(proxy.writable().await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_failed_signal() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let mut failures = proxy.receive_write_failed().await.unwrap();
    rig.sim.fail_output(true);

    proxy.set_fields(fields(WRITABLE, 30.0)).await.unwrap();

    // The save-settings command goes out 200 ms later in this rig, and the device refuses it.
    let signal = timeout(Duration::from_secs(5), failures.next())
        .await
        .expect("WriteFailed within 5 s")
        .unwrap();
    let args = signal.args().unwrap();
    assert!(!args.error.is_empty());
    let backups = proxy.list_backups().await.unwrap();
    assert!(backups.iter().any(|b| b.0 == args.backup));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_fields_asks_write() {
    if running_as_root() {
        return;
    }
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    proxy.get_fields().await.unwrap();
    assert!(rig.authorizer.calls().is_empty());

    proxy.set_fields(fields(WRITABLE, 30.0)).await.unwrap();

    assert_eq!(rig.authorizer.calls(), [(Action::Write, false)]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sync_clock_asks_write() {
    if running_as_root() {
        return;
    }
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;

    proxy.sync_clock().await.unwrap();

    assert_eq!(rig.authorizer.calls(), [(Action::Write, false)]);
    assert_eq!(rig.sim.commands().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restore_backup_asks_critical() {
    if running_as_root() {
        return;
    }
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let backup = proxy.create_backup().await.unwrap();
    rig.authorizer.clear();

    proxy.restore_backup(&backup).await.unwrap();

    assert_eq!(rig.authorizer.calls(), [(Action::Critical, false)]);
    // SetMode asks `critical` for release only.
    rig.authorizer.clear();
    proxy.set_mode("monitor").await.unwrap();
    proxy.set_mode("release").await.unwrap();
    assert_eq!(
        rig.authorizer.calls(),
        [(Action::Write, false), (Action::Critical, false)]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn backups_over_the_bus() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;

    let id = proxy.create_backup().await.unwrap();
    let listed = proxy.list_backups().await.unwrap();
    assert!(listed.iter().any(|b| b.0 == id && b.3 == 1022));
    let (firmware, report3, report5, report8) = proxy.get_backup(&id).await.unwrap();
    assert_eq!(firmware, 1022);
    assert_eq!(report3, rig.sim.report3());
    // The same firmware and reports import as the same backup.
    let imported = proxy
        .import_backup(firmware, &report3, &report5, &report8)
        .await
        .unwrap();
    assert_eq!(imported, id);
    assert_eq!(
        proxy
            .list_backups()
            .await
            .unwrap()
            .iter()
            .find(|b| b.0 == imported)
            .map(|b| b.3),
        Some(firmware)
    );
    // A firmware that the registry does not cover is refused and nothing is saved.
    let before = proxy.list_backups().await.unwrap().len();
    let mut other = report5.clone();
    other[10] ^= 1;
    let error = proxy
        .import_backup(1023, &report3, &other, &report8)
        .await
        .unwrap_err();
    assert_eq!(
        error_name(&error),
        "org.freedesktop.DBus.Error.NotSupported"
    );
    assert_eq!(proxy.list_backups().await.unwrap().len(), before);
    // A report 3 with a bad CRC does not.
    let mut bad = report3.clone();
    bad[748] ^= 0xFF;
    let error = proxy
        .import_backup(firmware, &bad, &report5, &report8)
        .await
        .unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.InvalidArgument"));
    let error = proxy.get_backup("no-such-backup").await.unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.NotFound"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wired_methods_do_not_reply_not_supported() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let not_supported = "org.freedesktop.DBus.Error.NotSupported";
    let mut replies: Vec<(&str, Option<String>)> = Vec::new();
    macro_rules! call {
        ($name:literal, $call:expr) => {
            replies.push(($name, $call.await.err().map(|e| error_name(&e))));
        };
    }
    call!("GetFields", proxy.get_fields());
    call!("SetFields", proxy.set_fields(Default::default()));
    call!("SetMode", proxy.set_mode("shield"));
    call!("StartAction", proxy.start_action("test"));
    call!("CancelAction", proxy.cancel_action());
    call!("SyncClock", proxy.sync_clock());
    call!("ListBackups", proxy.list_backups());
    call!("CreateBackup", proxy.create_backup());
    call!("GetBackup", proxy.get_backup("b"));
    call!("ImportBackup", proxy.import_backup(1022, &[], &[], &[]));
    call!("RestoreBackup", proxy.restore_backup("b"));
    call!("ResetRefillHistory", proxy.reset_refill_history());
    call!("ResetServiceCounter", proxy.reset_service_counter());
    call!("GetEvents", proxy.get_events());
    call!("GetSoftwareSensors", proxy.get_software_sensors());
    call!(
        "SetSoftwareSensor",
        proxy.set_software_sensor(1, &none_path())
    );
    call!("FactoryReset", proxy.factory_reset());

    assert_eq!(replies.len(), 17);
    for (name, error) in replies {
        assert_ne!(error.as_deref(), Some(not_supported), "{name}");
    }
}
