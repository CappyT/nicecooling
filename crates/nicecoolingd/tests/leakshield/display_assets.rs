//! The display assets of report 8: names, icons and the logo. The daemon reads them and writes
//! them with a backup, a 250 ms delay and a read-back check.

use std::sync::Arc;
use std::time::{Duration, Instant};

use nicecooling_api::Error;
use nicecooling_api::authz_map::Action;
use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_protocol::bitmap::Bitmap;
use nicecooling_protocol::report2::Report2;
use nicecooling_protocol::report8::{REPORT8_SIZE, Report8};
use nicecooling_protocol::sim::Request;
use nicecoolingd::leakshield::assets::AssetEdit;
use nicecoolingd::leakshield::attached::Info;
use nicecoolingd::leakshield::save_settings::SAVE_SETTINGS_DELAY;
use nicecoolingd::leakshield::worker::{Job, WorkerParts, WriteWorker};
use nicecoolingd::leakshield::write::WriteError;

use crate::Rig as DeviceRig;
use crate::support::{LEAKSHIELD, Rig, error_name, path};

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";
const SAVE_SETTINGS: u32 = 2;
const NAMES: usize = 1;
const LOGO: usize = 961;
const ICONS: usize = 1985;

fn running_as_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .is_some_and(|l| l.split_whitespace().next() == Some("0"))
}

fn feature_writes(rig: &DeviceRig) -> usize {
    rig.sim
        .requests()
        .iter()
        .filter(|r| matches!(r, Request::SetFeature { .. }))
        .count()
}

/// The offsets where two report 8 images differ, without the CRC.
fn changed(before: &[u8], after: &[u8]) -> Vec<usize> {
    (1..REPORT8_SIZE - 2)
        .filter(|&i| before[i] != after[i])
        .collect()
}

#[tokio::test]
async fn the_assets_of_the_fixture() {
    let rig = DeviceRig::new("assets-read");
    let device = rig.attached();

    let assets = device.get_display_assets().unwrap();

    assert_eq!(assets.names.len(), 40);
    assert_eq!(assets.names[0], "Pressure");
    assert_eq!(assets.names[20], "Name 1");
    assert_eq!(assets.icons.len(), 16);
    assert!(assets.icons.iter().all(|i| i.size() == (23, 23)));
    assert_eq!(assets.logo.size(), (128, 64));
    // A read sends nothing.
    assert_eq!(feature_writes(&rig), 0);
}

#[tokio::test]
async fn a_name_write_changes_that_name_and_the_crc_only() {
    let rig = DeviceRig::new("assets-name");
    let device = rig.attached();
    let before = rig.sim.report8();

    let started = Instant::now();
    device.set_display_name(21, "Pump \u{20ac}").await.unwrap();

    // The delay of 250 ms after the write is part of the call.
    assert!(started.elapsed() >= Duration::from_millis(250));
    let after = rig.sim.report8();
    let at = NAMES + 24 * 21;
    assert_eq!(
        changed(&before, &after),
        (at..at + 24)
            .filter(|&i| before[i] != after[i])
            .collect::<Vec<_>>()
    );
    assert_eq!(&after[at..at + 8], b"Pump \x80\0\0");
    let decoded = Report8::decode(&after).unwrap();
    assert_eq!(decoded.name(21).as_deref(), Some("Pump \u{20ac}"));
    assert_eq!(decoded.name(20).as_deref(), Some("Name 1"));
    // The report 3 stays as it was.
    assert_eq!(feature_writes(&rig), 1);
    // The backup holds report 8 from before the write.
    let backups = device.list_backups().unwrap();
    assert_eq!(backups.len(), 1);
    assert_eq!(device.get_backup(&backups[0].id).unwrap().report8, before);
}

#[tokio::test]
async fn an_icon_and_the_logo_are_written_in_the_column_layout() {
    let rig = DeviceRig::new("assets-bitmaps");
    let device = rig.attached();
    let before = rig.sim.report8();

    let mut icon = Bitmap::new(23, 23);
    icon.set(0, 0, true);
    icon.set(22, 22, true);
    device.set_display_icon(5, icon.clone()).await.unwrap();
    let after_icon = rig.sim.report8();
    let at = ICONS + 69 * 5;
    let touched = changed(&before, &after_icon);
    assert!(
        touched.iter().all(|i| (at..at + 69).contains(i)),
        "{touched:?}"
    );
    assert_eq!(after_icon[at], 1);
    assert_eq!(after_icon[at + 22 * 3 + 2], 1 << 6);
    assert_eq!(Report8::decode(&after_icon).unwrap().icon(5).unwrap(), icon);

    let mut logo = Bitmap::new(128, 64);
    logo.set(127, 63, true);
    device.set_display_logo(logo.clone()).await.unwrap();
    let after_logo = rig.sim.report8();
    let touched = changed(&after_icon, &after_logo);
    assert!(touched.iter().all(|i| (LOGO..LOGO + 1024).contains(i)));
    assert_eq!(after_logo[LOGO + 127 * 8 + 7], 0x80);
    // The icon of the first write is still there.
    assert_eq!(Report8::decode(&after_logo).unwrap().icon(5).unwrap(), icon);
    assert_eq!(device.get_display_assets().unwrap().logo, logo);
}

#[tokio::test]
async fn a_read_back_that_differs_is_verify_failed_with_the_backup() {
    let rig = DeviceRig::new("assets-verify");
    let device = rig.attached();
    let before = rig.sim.report8();
    rig.sim.tamper_next_report8(NAMES + 24 * 22, b'X');

    let error = device.set_display_name(22, "Loop").await.unwrap_err();

    let Error::VerifyFailed(backup) = error else {
        panic!("{error:?}")
    };
    assert_eq!(device.get_backup(&backup).unwrap().report8, before);
}

#[tokio::test]
async fn a_value_that_does_not_fit_sends_nothing() {
    let rig = DeviceRig::new("assets-invalid");
    let device = rig.attached();
    let long = "a".repeat(24);

    for error in [
        device.set_display_name(20, &long).await.unwrap_err(),
        device.set_display_name(20, "\u{65e5}").await.unwrap_err(),
        device.set_display_name(40, "x").await.unwrap_err(),
        device
            .set_display_icon(0, Bitmap::new(22, 23))
            .await
            .unwrap_err(),
        device
            .set_display_icon(16, Bitmap::new(23, 23))
            .await
            .unwrap_err(),
        device
            .set_display_logo(Bitmap::new(128, 63))
            .await
            .unwrap_err(),
    ] {
        assert!(matches!(error, Error::InvalidArgument(_)), "{error:?}");
    }
    assert_eq!(feature_writes(&rig), 0);
    assert!(device.list_backups().unwrap().is_empty());
}

#[tokio::test]
async fn a_read_only_device_gets_no_write() {
    let rig = DeviceRig::new("assets-read-only");
    rig.sim.edit_report1(|r| r.firmware_version = 1099);
    let device = rig.attached();

    let error = device.set_display_name(20, "Pump").await.unwrap_err();

    assert!(matches!(error, Error::ReadOnly(_)), "{error:?}");
    assert_eq!(feature_writes(&rig), 0);
    // A read still works.
    assert_eq!(device.get_display_assets().unwrap().names[0], "Pressure");
}

#[tokio::test(start_paused = true)]
async fn the_save_command_follows_a_report_8_write() {
    let rig = DeviceRig::new("assets-save");
    let (events, _rx) = tokio::sync::mpsc::unbounded_channel();
    let worker = WriteWorker::spawn(WorkerParts {
        io: rig.io.clone(),
        registry: Arc::clone(&rig.registry),
        backups: rig.backups(),
        last_known: Arc::clone(&rig.last_known),
        events,
        save_delay: SAVE_SETTINGS_DELAY,
        info: Arc::new(tokio::sync::watch::channel(Info::default()).0),
    });

    worker
        .submit(Job::Asset {
            report1: rig.io.read_report1().unwrap(),
            edit: AssetEdit::Name {
                index: 20,
                text: "Pump".to_owned(),
            },
        })
        .await
        .unwrap();
    assert!(rig.sim.commands().is_empty());
    tokio::time::sleep(SAVE_SETTINGS_DELAY + Duration::from_millis(100)).await;

    assert_eq!(rig.sim.commands(), [Report2::new(SAVE_SETTINGS, 0)]);
}

#[tokio::test(start_paused = true)]
async fn the_worker_refuses_a_second_write_while_one_runs() {
    let rig = DeviceRig::new("assets-busy");
    let (events, _rx) = tokio::sync::mpsc::unbounded_channel();
    let worker = WriteWorker::spawn(WorkerParts {
        io: rig.io.clone(),
        registry: Arc::clone(&rig.registry),
        backups: rig.backups(),
        last_known: Arc::clone(&rig.last_known),
        events,
        save_delay: SAVE_SETTINGS_DELAY,
        info: Arc::new(tokio::sync::watch::channel(Info::default()).0),
    });
    let report1 = rig.io.read_report1().unwrap();
    let job = |text: &str| Job::Asset {
        report1: report1.clone(),
        edit: AssetEdit::Name {
            index: 20,
            text: text.to_owned(),
        },
    };

    let (first, second) = tokio::join!(worker.submit(job("one")), worker.submit(job("two")));

    assert!(first.is_ok());
    assert!(matches!(second, Err(WriteError::Busy)), "{second:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_assets_over_the_bus() {
    if running_as_root() {
        return; // uid 0 needs no polkit check
    }
    let rig = Rig::start().await;
    let proxy = LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();

    // A read asks polkit nothing.
    let (names, icons, logo) = proxy.get_display_assets().await.unwrap();
    assert_eq!(names.len(), 40);
    assert_eq!(names[0], "Pressure");
    assert_eq!(icons.len(), 16);
    assert_eq!((icons[0].0, icons[0].1, icons[0].2.len()), (23, 23, 3 * 23));
    assert_eq!((logo.0, logo.1, logo.2.len()), (128, 64, 16 * 64));
    assert!(rig.authorizer.calls().is_empty());

    // The writes ask `write`.
    proxy.set_display_name(20, "Pump").await.unwrap();
    proxy
        .set_display_icon(0, 23, 23, &Bitmap::new(23, 23).to_rows())
        .await
        .unwrap();
    proxy
        .set_display_logo(128, 64, &Bitmap::new(128, 64).to_rows())
        .await
        .unwrap();
    assert_eq!(
        rig.authorizer.calls(),
        [
            (Action::Write, false),
            (Action::Write, false),
            (Action::Write, false)
        ]
    );
    let after = proxy.get_display_assets().await.unwrap();
    assert_eq!(after.0[20], "Pump");
    assert!(after.2.2.iter().all(|&b| b == 0));

    // A bitmap with a wrong byte count or size is refused.
    let error = proxy
        .set_display_icon(0, 23, 23, &[0; 68])
        .await
        .unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.InvalidArgument"));
    let error = proxy
        .set_display_logo(64, 64, &Bitmap::new(64, 64).to_rows())
        .await
        .unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.InvalidArgument"));
}
