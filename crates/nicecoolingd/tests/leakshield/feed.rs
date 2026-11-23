use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nicecooling_core::loops::Loop;
use nicecooling_core::tick::{LoopSetup, Stage, TickConfig, TickContext, ValueSource};
use nicecooling_protocol::feed::{FEED_SIZE, FeedMessage, NO_VALUE};
use nicecoolingd::leakshield::Attached;
use nicecoolingd::leakshield::feed::{
    CoveredLookup, FeedLink, FeedStage, LinkedSink, SoftwareLookup, feed_channel,
    feed_channel_checked, feed_for_loop,
};
use nicecoolingd::value_table::ValueTable;

use crate::{LEAKSHIELD_ID, Rig};

fn loop_with(leakshield: Option<&str>) -> LoopSetup {
    LoopSetup {
        config: Loop {
            id: "loop_1".to_owned(),
            name: "Loop".to_owned(),
            pump_channels: vec!["nct6798_pwm6".to_owned()],
            flow_sensor: Some("nct6798_fan3".to_owned()),
            calibration_table: None,
            water_temperature: None,
            leakshield: leakshield.map(str::to_owned),
            pump_floor: 20.0,
        },
        table: None,
    }
}

fn context(setup: LoopSetup) -> TickContext {
    let mut ctx = TickContext::new(TickConfig {
        loops: vec![setup],
        ..TickConfig::default()
    });
    ctx.values.insert("nct6798_fan6".to_owned(), 2000.0);
    ctx.values.insert("nct6798_fan3".to_owned(), 629.0);
    // The calibrated flow of the Loop, from stage 2.
    ctx.values.insert("loop_1_flow".to_owned(), 115.0);
    ctx
}

#[test]
fn no_feed_without_leakshield() {
    let rig = Rig::new("no-feed");
    let (mut stage, mut link) = feed_channel();
    let devices = BTreeMap::from([(LEAKSHIELD_ID.to_owned(), rig.io.clone())]);
    let mut results = Vec::new();

    // A Loop without a Leakshield, and a tick without any Loop.
    stage.run(&mut context(loop_with(None)));
    stage.run(&mut TickContext::new(TickConfig::default()));
    link.flush(&devices, &mut |id, r| {
        results.push((id.to_owned(), r.is_ok()))
    });

    assert!(results.is_empty());
    assert!(rig.sim.bulk_messages().is_empty());
    assert!(rig.sim.requests().is_empty());
}

#[test]
fn feed_carries_pump_speed_and_flow_of_the_loop() {
    let rig = Rig::new("feed-values");
    let (mut stage, mut link) = feed_channel();
    let devices = BTreeMap::from([(LEAKSHIELD_ID.to_owned(), rig.io.clone())]);
    let mut results = Vec::new();

    stage.run(&mut context(loop_with(Some(LEAKSHIELD_ID))));
    link.flush(&devices, &mut |id, r| {
        results.push((id.to_owned(), r.is_ok()))
    });
    // The message goes out once, not again at the next flush.
    link.flush(&devices, &mut |id, r| {
        results.push((id.to_owned(), r.is_ok()))
    });

    assert_eq!(results, [(LEAKSHIELD_ID.to_owned(), true)]);
    let sent = rig.sim.bulk_messages();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].len(), FEED_SIZE);
    let message = FeedMessage::decode(&sent[0]).unwrap();
    assert_eq!((message.value(0), message.unit(0)), (2000, 3));
    assert_eq!((message.value(1), message.unit(1)), (1150, 12));
    assert_eq!(message.value(2), NO_VALUE);
}

#[test]
fn missing_values_send_no_value() {
    let rig = Rig::new("feed-empty");
    let mut ctx = context(loop_with(Some(LEAKSHIELD_ID)));
    ctx.values.clear();
    let (mut stage, mut link) = feed_channel();
    stage.run(&mut ctx);
    let devices = BTreeMap::from([(LEAKSHIELD_ID.to_owned(), rig.io.clone())]);
    link.flush(&devices, &mut |_, r| assert!(r.is_ok()));
    let message = FeedMessage::decode(&rig.sim.bulk_messages()[0]).unwrap();
    assert_eq!(message, FeedMessage::default());
}

#[test]
fn feed_value_above_the_i16_range_is_not_the_no_value_code() {
    let mut values = BTreeMap::new();
    values.insert("nct6798_fan6".to_owned(), 40_000.0);
    // 3300.0 L/h is 33000 in tenths: above the i16 range as well.
    values.insert("loop_1_flow".to_owned(), 3_300.0);

    let message = feed_for_loop(&loop_with(Some(LEAKSHIELD_ID)), &values);

    assert_eq!((message.value(0), message.unit(0)), (32766, 3));
    assert_eq!((message.value(1), message.unit(1)), (32766, 12));
    // A reading of exactly 32767 does not become the no-value code either.
    values.insert("nct6798_fan6".to_owned(), 32_767.0);
    let message = feed_for_loop(&loop_with(Some(LEAKSHIELD_ID)), &values);
    assert_eq!(message.value(0), 32766);
}

#[test]
fn feed_is_not_sent_and_a_warning_stays_while_the_firmware_is_not_covered() {
    let rig = Rig::new("feed-uncovered");
    let covered = Arc::new(AtomicBool::new(false));
    let lookup: CoveredLookup = {
        let covered = Arc::clone(&covered);
        Arc::new(move |_| Some(covered.load(Ordering::SeqCst)))
    };
    let software: SoftwareLookup = Arc::new(|_| None);
    let (mut stage, mut link) = feed_channel_checked(software, lookup);
    let devices = BTreeMap::from([(LEAKSHIELD_ID.to_owned(), rig.io.clone())]);
    let mut ctx = context(loop_with(Some(LEAKSHIELD_ID)));

    stage.run(&mut ctx);
    link.flush(&devices, &mut |_, _| panic!("a Feed message went out"));
    assert!(rig.sim.bulk_messages().is_empty());
    assert!(
        ctx.warnings
            .is_active("leakshield-unknown-firmware", LEAKSHIELD_ID)
    );

    // The firmware is covered now: the Feed starts and the Warning clears.
    covered.store(true, Ordering::SeqCst);
    stage.run(&mut ctx);
    link.flush(&devices, &mut |_, r| assert!(r.is_ok()));
    assert_eq!(rig.sim.bulk_messages().len(), 1);
    assert!(
        !ctx.warnings
            .is_active("leakshield-unknown-firmware", LEAKSHIELD_ID)
    );
}

#[tokio::test]
async fn the_registry_covers_the_firmware_of_the_device_it_was_made_for() {
    let rig = Rig::new("feed-covered");
    assert!(rig.attached().feed_covered());

    rig.sim.edit_report1(|r| r.firmware_version = 1019);
    let other = rig.attached();
    assert!(!other.feed_covered());
}

fn linked_to(device: &Arc<Attached>) -> LinkedSink {
    let device = Arc::clone(device);
    Arc::new(move |ids| device.set_feed_linked(ids.contains(&device.id())))
}

/// One tick and one send of the Feed of `device`, and its result for the device.
fn tick_and_send(
    stage: &mut FeedStage,
    link: &mut FeedLink,
    ctx: &mut TickContext,
    rig: &Rig,
    device: &Attached,
) {
    stage.run(ctx);
    let devices = BTreeMap::from([(LEAKSHIELD_ID.to_owned(), rig.io.clone())]);
    link.flush(&devices, &mut |_, result| {
        device.feed_result(&result, |_| {})
    });
}

#[tokio::test(flavor = "multi_thread")]
async fn feed_state_ok_failing_ok() {
    let rig = Rig::new("feed-state-cycle");
    let device = rig.attached();
    let (stage, mut link) = feed_channel();
    let mut stage = stage.with_linked(linked_to(&device));
    let mut ctx = context(loop_with(Some(LEAKSHIELD_ID)));

    tick_and_send(&mut stage, &mut link, &mut ctx, &rig, &device);
    assert_eq!(device.info().feed_state, "ok");
    rig.sim.fail_bulk(true);
    tick_and_send(&mut stage, &mut link, &mut ctx, &rig, &device);
    assert_eq!(device.info().feed_state, "failing");
    rig.sim.fail_bulk(false);
    tick_and_send(&mut stage, &mut link, &mut ctx, &rig, &device);
    assert_eq!(device.info().feed_state, "ok");
}

#[tokio::test(flavor = "multi_thread")]
async fn feed_state_waiting_before_first_send() {
    let rig = Rig::new("feed-state-waiting");
    let device = rig.attached();
    assert_eq!(device.info().feed_state, "off");
    let (stage, _link) = feed_channel();
    let mut stage = stage.with_linked(linked_to(&device));

    stage.run(&mut context(loop_with(Some(LEAKSHIELD_ID))));

    let info = device.info();
    assert_eq!(info.feed_state, "waiting");
    assert_eq!(info.last_feed, (0, 0.0, 0.0));
}

#[tokio::test(flavor = "multi_thread")]
async fn feed_state_off_without_loop() {
    let rig = Rig::new("feed-state-no-loop");
    let device = rig.attached();
    let (stage, mut link) = feed_channel();
    let mut stage = stage.with_linked(linked_to(&device));
    let mut ctx = context(loop_with(Some(LEAKSHIELD_ID)));
    tick_and_send(&mut stage, &mut link, &mut ctx, &rig, &device);
    assert_eq!(device.info().feed_state, "ok");

    // A Loop without a Leakshield, then no Loop at all.
    stage.run(&mut context(loop_with(None)));
    assert_eq!(device.info().feed_state, "off");
    tick_and_send(&mut stage, &mut link, &mut ctx, &rig, &device);
    stage.run(&mut TickContext::new(TickConfig::default()));
    assert_eq!(device.info().feed_state, "off");

    // The Feed goes on again and waits for its first result.
    stage.run(&mut ctx);
    assert_eq!(device.info().feed_state, "waiting");
}

#[tokio::test(flavor = "multi_thread")]
async fn feed_state_off_for_unknown_firmware() {
    let rig = Rig::new("feed-state-firmware");
    rig.sim.edit_report1(|r| r.firmware_version = 1019);
    let device = rig.attached();
    let (stage, _link) = feed_channel();
    let mut stage = stage.with_linked(linked_to(&device));

    stage.run(&mut context(loop_with(Some(LEAKSHIELD_ID))));

    assert_eq!(device.info().feed_state, "off");
}

#[tokio::test(flavor = "multi_thread")]
async fn feed_state_off_when_disconnected() {
    let rig = Rig::new("feed-state-disconnected");
    let device = rig.attached();
    let (stage, mut link) = feed_channel();
    let mut stage = stage.with_linked(linked_to(&device));
    let mut ctx = context(loop_with(Some(LEAKSHIELD_ID)));
    tick_and_send(&mut stage, &mut link, &mut ctx, &rig, &device);
    assert_eq!(device.info().feed_state, "ok");

    device.detach();

    assert_eq!(device.info().feed_state, "off");
}

#[tokio::test(flavor = "multi_thread")]
async fn feed_off_resets_failing_time() {
    let rig = Rig::new("feed-off-reset");
    let device = rig.attached();
    let table = ValueTable::default();
    let _task = device.spawn_feed_reset(table.clone());
    let (stage, _link) = feed_channel();
    let mut stage = stage.with_linked(linked_to(&device));
    stage.run(&mut context(loop_with(Some(LEAKSHIELD_ID))));
    table.publish_feed(LEAKSHIELD_ID, false, Duration::from_secs(5));
    assert!(
        table.leakshields()[LEAKSHIELD_ID]
            .feed_failing_since
            .is_some()
    );

    // The user removes the Leakshield from its Loop while the sends fail.
    stage.run(&mut TickContext::new(TickConfig::default()));

    for _ in 0..100 {
        if table.leakshields()[LEAKSHIELD_ID]
            .feed_failing_since
            .is_none()
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the failing time stayed");
}

#[tokio::test(flavor = "multi_thread")]
async fn last_feed_holds_time_and_values() {
    let rig = Rig::new("feed-last");
    let device = rig.attached();
    let (stage, mut link) = feed_channel();
    let mut stage = stage.with_linked(linked_to(&device));
    let mut ctx = context(loop_with(Some(LEAKSHIELD_ID)));
    let before = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_micros() as u64;

    tick_and_send(&mut stage, &mut link, &mut ctx, &rig, &device);

    let (time_us, rpm, lph) = device.info().last_feed;
    assert!(
        time_us >= before && time_us <= before + 5_000_000,
        "{time_us}"
    );
    assert_eq!((rpm, lph), (2000.0, 115.0));
    // A failed send keeps the last good values.
    rig.sim.fail_bulk(true);
    tick_and_send(&mut stage, &mut link, &mut ctx, &rig, &device);
    assert_eq!(device.info().last_feed, (time_us, 2000.0, 115.0));
}
