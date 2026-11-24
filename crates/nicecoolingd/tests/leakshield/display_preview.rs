//! `GetDisplayPreview`: the image that the device shows for a page, read through `show_page` and
//! report 5.

use nicecooling_api::Error;
use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_protocol::bitmap::Bitmap;
use nicecooling_protocol::report2::Report2;
use nicecooling_protocol::sim::SimDevice;

use crate::Rig as DeviceRig;
use crate::support::{LEAKSHIELD, Rig, error_name, path};

const SHOW_PAGE: u32 = 4;
const ERR: &str = "io.github.cappyt.NiceCooling1.Error";

fn show_page_commands(rig: &DeviceRig) -> Vec<Report2> {
    rig.sim
        .commands()
        .into_iter()
        .filter(|c| c.command == SHOW_PAGE)
        .collect()
}

fn expected(page: u8) -> Bitmap {
    Bitmap::from_page_layout(128, 64, &SimDevice::page_image(page)).unwrap()
}

#[tokio::test]
async fn preview_sends_show_page_for_30_seconds_and_returns_the_image() {
    let rig = DeviceRig::new("preview-ok");
    let device = rig.attached();

    let image = device.get_display_preview(3).unwrap();

    // The parameter is `page << 16 | 300` (30 s in steps of 0.1 s).
    assert_eq!(
        show_page_commands(&rig),
        [Report2::new(SHOW_PAGE, (3 << 16) | 300)]
    );
    assert_eq!(image.size(), (128, 64));
    assert_eq!(image, expected(3));
    // The device holds no new setting: show_page does not write a report.
    assert_eq!(
        rig.sim.report3(),
        include_bytes!("../../../../tests/fixtures/leakshield/fw1022/report3.bin")
    );
}

#[tokio::test]
async fn preview_of_another_page_is_an_error() {
    let rig = DeviceRig::new("preview-other");
    let device = rig.attached();
    rig.sim.show_page_instead(Some(7));

    let error = device.get_display_preview(2).unwrap_err();

    assert!(
        matches!(&error, Error::PageNotShown(message) if message.contains('7')),
        "{error:?}"
    );
}

#[tokio::test]
async fn a_page_outside_the_ten_is_invalid_and_sends_nothing() {
    let rig = DeviceRig::new("preview-range");
    let device = rig.attached();

    let error = device.get_display_preview(10).unwrap_err();

    assert!(matches!(error, Error::InvalidArgument(_)), "{error:?}");
    assert!(show_page_commands(&rig).is_empty());
}

#[tokio::test]
async fn a_read_only_device_gets_no_show_page() {
    let rig = DeviceRig::new("preview-read-only");
    rig.sim.edit_report1(|r| r.firmware_version = 1099);
    let device = rig.attached();

    let error = device.get_display_preview(0).unwrap_err();

    assert!(matches!(error, Error::ReadOnly(_)), "{error:?}");
    assert!(show_page_commands(&rig).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn preview_over_the_bus_needs_no_authorization() {
    let rig = Rig::start().await;
    let proxy = LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();

    let (width, height, rows) = proxy.get_display_preview(1).await.unwrap();

    assert_eq!((width, height, rows.len()), (128, 64, 16 * 64));
    assert_eq!(Bitmap::from_rows(128, 64, &rows).unwrap(), expected(1));
    let error = proxy.get_display_preview(12).await.unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.InvalidArgument"));
}
