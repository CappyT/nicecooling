//! A Feed result that arrives after the Feed went off does not count.

use std::sync::Arc;
use std::time::Duration;

use nicecooling_core::alarms::unreachable::Unreachable;
use nicecooling_core::loops::Loop;
use nicecooling_core::tick::{LoopSetup, Stage, TickConfig, TickContext, ValueSource};
use nicecooling_protocol::device::DeviceError;
use nicecoolingd::leakshield::feed::{FeedSent, feed_channel};
use nicecoolingd::value_table::ValueTable;

use crate::{LEAKSHIELD_ID, Rig};

fn linked_context() -> TickContext {
    let mut ctx = TickContext::new(TickConfig {
        loops: vec![LoopSetup {
            config: Loop {
                id: "loop_1".to_owned(),
                name: "Loop".to_owned(),
                pump_channels: vec!["nct6798_pwm6".to_owned()],
                flow_sensor: Some("nct6798_fan3".to_owned()),
                calibration_table: None,
                water_temperature: None,
                leakshield: Some(LEAKSHIELD_ID.to_owned()),
                pump_floor: 20.0,
            },
            table: None,
        }],
        ..TickConfig::default()
    });
    ctx.values.insert("nct6798_fan6".to_owned(), 2000.0);
    ctx.values.insert("nct6798_fan3".to_owned(), 629.0);
    ctx.values.insert("loop_1_flow".to_owned(), 115.0);
    ctx
}

/// A failing device times out each bulk write after 1 s (`TRANSFER_TIMEOUT`), so a send is in
/// flight at almost every tick. The user unlinks the Leakshield during such a send. The reset
/// runs at the change to `off`; the late failure of the send then sets the failing time again,
/// and no later change to `off` resets it.
#[tokio::test(flavor = "multi_thread")]
async fn late_failed_send_after_unlink_keeps_no_failing_time() {
    let rig = Rig::new("review11-feed-race");
    let device = rig.attached();
    let table = ValueTable::default();
    let _task = device.spawn_feed_reset(table.clone());
    let (stage, _link) = feed_channel();
    let linked = Arc::clone(&device);
    let mut stage = stage.with_linked(Arc::new(move |ids| {
        linked.set_feed_linked(ids.contains(&linked.id()))
    }));

    // Tick N: the Feed is on and its send goes out.
    stage.run(&mut linked_context());
    // Tick N+1: no Loop links the Leakshield any more. The send of tick N is still in flight.
    stage.run(&mut TickContext::new(TickConfig::default()));
    assert_eq!(device.info().feed_state, "off");
    tokio::time::sleep(Duration::from_millis(200)).await;

    // The send of tick N times out now. The daemon reports it as `daemon.rs` does.
    let result: Result<FeedSent, DeviceError> = Err(DeviceError::Io(std::io::Error::other("t/o")));
    device.feed_result(&result, |ok| {
        table.publish_feed(LEAKSHIELD_ID, ok, Duration::from_secs(5))
    });
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Report 1 arrives every second: the Leakshield is reachable and its Feed is off.
    let mut rule = Unreachable::default();
    let mut active = false;
    for s in 5..=40 {
        table.publish_report1(LEAKSHIELD_ID, 0, 0, Duration::from_secs(s));
        active = rule.update(Duration::from_secs(s), &table.leakshields()[LEAKSHIELD_ID]);
    }
    assert_eq!(
        table.leakshields()[LEAKSHIELD_ID].feed_failing_since,
        None,
        "the failing time stays after the Feed went off"
    );
    assert!(
        !active,
        "leakshield-unreachable is active for a reachable device with the Feed off"
    );
}
