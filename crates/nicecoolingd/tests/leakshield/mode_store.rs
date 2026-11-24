//! The Mode belongs to the Profile (design D14): after `SetMode` the device stores the Mode in
//! `default_mode` of report 3, and the daemon copies that byte into the stored report 3 of the
//! slot that the device runs. The simulated device stores it at a later read of report 3.

use std::sync::Arc;
use std::time::Duration;

use nicecooling_core::profile::Slot;
use nicecooling_protocol::report3::Report3;
use nicecooling_protocol::sim::Request;
use nicecoolingd::leakshield::DeviceEvent;
use nicecoolingd::leakshield::external::{ExternalDetector, POLL_PERIOD};
use nicecoolingd::profiles::activate::apply_slot;
use nicecoolingd::state::{SlotStore, StateStore};
use tokio::time::sleep;

use crate::{LEAKSHIELD_ID, Rig};

/// Report 3 offset of `default_mode`.
const DEFAULT_MODE: usize = 724;
const DISPLAY_PAGE_TIME: usize = 63;

fn slots(rig: &Rig) -> SlotStore {
    SlotStore::new(Arc::new(StateStore::new(&rig.dir)))
}

fn slot(n: u32) -> Slot {
    Slot::new(n).unwrap()
}

fn stored(slots: &SlotStore, n: u32) -> Report3 {
    slots.load(LEAKSHIELD_ID, slot(n)).unwrap().unwrap().report3
}

fn never() -> bool {
    false
}

fn report3_reads(rig: &Rig) -> usize {
    rig.sim
        .requests()
        .iter()
        .filter(|r| matches!(r, Request::GetFeature { id: 3, .. }))
        .count()
}

/// A device that runs slot 1, with RELEASE as `default_mode` (the reference device) in each slot.
fn rig(name: &str) -> Rig {
    let rig = Rig::new(name);
    rig.sim.set_device_slot(1);
    rig.sim.edit_report1(|r| r.mode = 3);
    assert_eq!(rig.sim.report3()[DEFAULT_MODE], 3);
    rig
}

#[tokio::test(start_paused = true)]
async fn the_slot_of_the_device_holds_the_new_mode_and_nothing_else() {
    let rig = rig("mode-store");
    let (device, slots) = (rig.attached(), slots(&rig));
    let before: Vec<Report3> = (1..=4).map(|n| stored(&slots, n)).collect();
    // The device holds a setting that the slot does not: only the Mode goes into the slot.
    rig.sim.change_report3(DISPLAY_PAGE_TIME, &[0x02, 0x58]);

    device.set_mode("shield", &slots).unwrap();
    sleep(Duration::from_secs(10)).await;

    assert_eq!(rig.sim.report3()[DEFAULT_MODE], 1);
    let after = stored(&slots, 1);
    assert_eq!(after.differing_offsets(&before[0], &[]), [DEFAULT_MODE]);
    assert_eq!(after.bytes_at(DEFAULT_MODE, 1), [1]);
    for n in 2..=4 {
        assert_eq!(stored(&slots, n), before[n as usize - 1], "slot {n}");
    }
    // The mark of an edit stays as it was.
    let meta = slots.load(LEAKSHIELD_ID, slot(1)).unwrap().unwrap().meta;
    assert!(!meta.edited);
}

#[tokio::test(start_paused = true)]
async fn a_later_switch_to_the_slot_keeps_the_mode() {
    let rig = rig("mode-store-switch");
    let (device, slots) = (rig.attached(), slots(&rig));
    device.set_mode("shield", &slots).unwrap();
    sleep(Duration::from_secs(10)).await;

    // Another slot, with the Mode that it holds, and back.
    apply_slot(&device, &slots, slot(2)).await.unwrap();
    assert_eq!(rig.sim.report3()[DEFAULT_MODE], 3);
    sleep(Duration::from_secs(3)).await;
    apply_slot(&device, &slots, slot(1)).await.unwrap();

    assert_eq!(rig.sim.report3()[747], 1);
    assert_eq!(rig.sim.report3()[DEFAULT_MODE], 1);
}

#[tokio::test(start_paused = true)]
async fn the_daemons_own_mode_is_no_external_change() {
    let rig = rig("mode-store-external");
    let (device, slots) = (rig.attached(), slots(&rig));
    let mut events = device.take_events().unwrap();
    device.spawn_external_detection(POLL_PERIOD);
    // The first poll becomes the last known report 3.
    sleep(Duration::from_secs(1)).await;

    device.set_mode("monitor", &slots).unwrap();
    sleep(Duration::from_secs(60)).await;

    assert_eq!(rig.sim.report3()[DEFAULT_MODE], 2);
    assert_eq!(events.try_recv().ok(), None::<DeviceEvent>);
}

#[tokio::test(start_paused = true)]
async fn a_poll_that_sees_the_mode_first_is_no_external_change() {
    let rig = rig("mode-store-race");
    let (device, slots) = (rig.attached(), slots(&rig));
    let mut events = device.take_events().unwrap();
    // The device stores the Mode at the next read of report 3. The detector polls at 0 s and
    // 10 s: its poll at 10 s reads report 3 before the first poll of the Mode at 10.5 s.
    rig.sim.set_mode_store_reads(1);
    device.spawn_external_detection(POLL_PERIOD);
    sleep(Duration::from_millis(9500)).await;

    device.set_mode("shield", &slots).unwrap();
    sleep(Duration::from_millis(700)).await;
    assert_eq!(rig.sim.report3()[DEFAULT_MODE], 1);
    assert_eq!(events.try_recv().ok(), None::<DeviceEvent>);
    sleep(Duration::from_secs(30)).await;

    assert_eq!(events.try_recv().ok(), None::<DeviceEvent>);
    assert_eq!(stored(&slots, 1).bytes_at(DEFAULT_MODE, 1), [1]);
}

#[test]
fn an_external_change_of_the_mode_is_still_reported() {
    let rig = rig("mode-store-external-change");
    let detector = ExternalDetector::new(
        rig.io.clone(),
        Arc::clone(&rig.registry),
        Arc::clone(&rig.last_known),
    );
    assert_eq!(detector.poll().unwrap(), None);

    // The user changes the Mode with the device button: the daemon set nothing.
    rig.sim.change_report3(DEFAULT_MODE, &[2]);

    assert_eq!(
        detector.poll().unwrap(),
        Some(vec!["default_mode".to_owned()])
    );
}

#[tokio::test(start_paused = true)]
async fn a_device_that_does_not_store_the_mode_leaves_the_slots_alone() {
    let rig = rig("mode-store-timeout");
    let (device, slots) = (rig.attached(), slots(&rig));
    let before: Vec<Report3> = (1..=4).map(|n| stored(&slots, n)).collect();
    rig.sim.set_mode_store_reads(u32::MAX);
    rig.sim.clear_requests();

    device.set_mode("shield", &slots).unwrap();
    sleep(Duration::from_secs(120)).await;

    // One read of report 3 each second, for 60 s.
    assert_eq!(report3_reads(&rig), 60);
    for n in 1..=4 {
        assert_eq!(stored(&slots, n), before[n as usize - 1], "slot {n}");
    }
}

#[tokio::test(start_paused = true)]
async fn an_action_stores_nothing() {
    let rig = rig("mode-store-action");
    let (device, slots) = (rig.attached(), slots(&rig));
    let before: Vec<Report3> = (1..=4).map(|n| stored(&slots, n)).collect();
    rig.sim.clear_requests();

    device.start_action("measure-fill-level", never).unwrap();
    sleep(Duration::from_secs(120)).await;

    assert_eq!(report3_reads(&rig), 0);
    assert_eq!(rig.sim.report3()[DEFAULT_MODE], 3);
    for n in 1..=4 {
        assert_eq!(stored(&slots, n), before[n as usize - 1], "slot {n}");
    }
}
