use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use nicecooling_core::loops::Loop;
use nicecooling_core::tick::{LoopSetup, Stage, TickConfig, TickContext};
use nicecooling_protocol::feed::{FEED_SIZE, FeedMessage, NO_VALUE};
use nicecoolingd::leakshield::feed::{
    CoveredLookup, SoftwareLookup, feed_channel, feed_channel_checked, feed_for_loop,
};

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
