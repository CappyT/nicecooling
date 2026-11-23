//! Attach at the start and on hot-plug, on a simulated USB bus (issue 56). The daemon never
//! opens a real device here: the bus, the device nodes and the transports are simulated.

use std::time::Duration;

use nicecooling_api::ids::{Kind, object_path};
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_api::sensor::SensorProxy;
use nicecooling_protocol::sim::{Request, SimDevice};
use nicecoolingd::daemon::{Options, Paths};
use nicecoolingd::leakshield::attached::now_unix;
use nicecoolingd::leakshield::clock;
use nicecoolingd::sources::hotplug::{UsbDevice, UsbPath};

use crate::support::sim_usb::{SimUsb, device};
use crate::support::{LEAKSHIELD, Rig, path};

const PATH: &str = "1-2.3";

/// A device with a valid clock, so that the daemon sends no clock command.
fn sim() -> SimDevice {
    let sim = SimDevice::fw1022();
    let now = clock::device_seconds(now_unix()).unwrap();
    sim.edit_report1(|r| r.device_clock = now);
    sim
}

async fn proxy(rig: &Rig) -> LeakshieldProxy<'static> {
    LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

async fn until(mut check: impl AsyncFnMut() -> bool) {
    for _ in 0..200 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the condition did not hold in 5 s");
}

async fn connected(rig: &Rig) -> bool {
    proxy(rig).await.connected().await.unwrap_or_default()
}

fn attached(rig: &Rig) -> usize {
    rig.daemon.as_ref().unwrap().fleet.all().len()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn attaches_the_enumerated_leakshield_at_start() {
    let usb = SimUsb::default();
    usb.present(device(PATH, 0x0100, 3), &sim());

    let rig = Rig::start_with_usb(usb.clone()).await;

    assert_eq!(attached(&rig), 1);
    assert_eq!(usb.opened(), [UsbPath(PATH.to_owned())]);
    let leakshield = proxy(&rig).await;
    assert!(leakshield.connected().await.unwrap());
    assert_eq!(leakshield.serial().await.unwrap(), "20417-31072");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn attaches_a_leakshield_that_plugs_in_later() {
    let usb = SimUsb::default();
    let mut rig = Rig::start_with_usb(usb.clone()).await;
    let fleet = rig.daemon.as_ref().unwrap().fleet.clone();
    rig.tick_in_background();
    assert_eq!(fleet.all().len(), 0);

    usb.plug(device(PATH, 0x0100, 3), &sim());

    until(async || connected(&rig).await).await;
    assert_eq!(fleet.all().len(), 1);
    // The sensors of the device join the API with it.
    let sensor = SensorProxy::builder(&rig.client)
        .path(path(Kind::Sensor, &format!("{LEAKSHIELD}_pressure")))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    until(async || sensor.has_value().await.unwrap_or(false)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn removal_disconnects_and_replug_at_the_same_path_resumes_the_feed() {
    let usb = SimUsb::default();
    let first = sim();
    usb.present(device(PATH, 0x0100, 3), &first);
    let mut rig = Rig::start_with_usb(usb.clone()).await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    hub.set_loop_leakshield("loop_1", &object_path(Kind::Leakshield, LEAKSHIELD))
        .unwrap();
    rig.tick_in_background();
    until(async || !first.bulk_messages().is_empty()).await;

    usb.unplug(PATH);
    until(async || !connected(&rig).await).await;
    // The transports of the old device are closed: nothing reaches it any more.
    let seen = first.requests().len();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert_eq!(first.requests().len(), seen);

    let second = sim();
    usb.plug(device(PATH, 0x0100, 3), &second);
    until(async || connected(&rig).await).await;
    until(async || !second.bulk_messages().is_empty()).await;
    assert!(second.requests().contains(&Request::ReadInput));
    assert_eq!(usb.opened().len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn replug_at_another_path_is_the_same_leakshield() {
    let usb = SimUsb::default();
    usb.present(device(PATH, 0x0100, 3), &sim());
    let rig = Rig::start_with_usb(usb.clone()).await;

    usb.unplug(PATH);
    until(async || !connected(&rig).await).await;
    usb.plug(device("1-4", 0x0100, 3), &sim());

    until(async || connected(&rig).await).await;
    assert_eq!(attached(&rig), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_device_in_the_bootloader_is_skipped_and_never_opened() {
    let usb = SimUsb::default();
    usb.present(device("1-2", 0x0200, 2), &sim());
    let rig = Rig::start_with_usb(usb.clone()).await;
    assert_eq!(attached(&rig), 0);

    usb.plug(device("1-3", 0x0200, 2), &sim());
    // A later event is handled after the earlier one, so the probe below has been read.
    usb.plug(device(PATH, 0x0100, 3), &sim());
    until(async || attached(&rig) == 1).await;

    assert_eq!(usb.opened(), [UsbPath(PATH.to_owned())]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn other_usb_devices_are_ignored() {
    let usb = SimUsb::default();
    let rig = Rig::start_with_usb(usb.clone()).await;
    let other = UsbDevice {
        vendor: 0x1234,
        product: 0x5678,
        ..device("1-5", 0x0100, 3)
    };
    usb.announce(other);
    usb.plug(device(PATH, 0x0100, 3), &sim());

    until(async || attached(&rig) == 1).await;
    assert_eq!(usb.opened(), [UsbPath(PATH.to_owned())]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn waits_for_the_hidraw_node() {
    let usb = SimUsb::default();
    let rig = Rig::start_with_usb(usb.clone()).await;
    usb.not_ready_for(3);

    usb.plug(device(PATH, 0x0100, 3), &sim());

    until(async || attached(&rig) == 1).await;
    assert_eq!(usb.opened().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_event_for_an_attached_device_does_not_open_it_again() {
    let usb = SimUsb::default();
    usb.present(device(PATH, 0x0100, 3), &sim());
    let rig = Rig::start_with_usb(usb.clone()).await;
    assert_eq!(usb.opened().len(), 1);

    usb.announce(device(PATH, 0x0100, 3));
    usb.plug(device("1-6", 0x0200, 2), &sim());
    tokio::time::sleep(Duration::from_millis(200)).await;

    assert_eq!(usb.opened().len(), 1);
    assert_eq!(attached(&rig), 1);
}

#[test]
fn the_daemon_options_enumerate_no_usb_bus_by_default() {
    let options = Options::new(Paths::default());
    assert!(options.usb.is_none());
}
