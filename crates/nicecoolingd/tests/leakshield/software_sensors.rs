use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use nicecooling_api::ids::{Kind, object_path};
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_core::loops::Loop;
use nicecooling_core::tick::{LoopSetup, Stage, TickConfig, TickContext};
use nicecooling_protocol::feed::{FEED_SLOTS, FeedMessage, NO_VALUE};
use nicecoolingd::leakshield::feed::feed_channel_with;
use nicecoolingd::leakshield::software_sensors::{
    Assignment, FIRST_FEED_SLOT, SLOT_COUNT, SoftwareSlots,
};

use crate::support::{LEAKSHIELD, Rig, error_name, none_path, path};
use crate::{LEAKSHIELD_ID, Rig as DeviceRig};

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";
const TEMPERATURE_UNIT_CODE: u8 = 10;

fn loop_setup() -> LoopSetup {
    LoopSetup {
        config: Loop {
            id: "loop_1".to_owned(),
            name: "Loop".to_owned(),
            pump_channels: Vec::new(),
            flow_sensor: None,
            calibration_table: None,
            water_temperature: None,
            leakshield: Some(LEAKSHIELD_ID.to_owned()),
            pump_floor: 20.0,
        },
        table: None,
    }
}

/// Runs the Feed stage once with `values` and returns the message that reaches the device.
fn feed(slots: &Arc<SoftwareSlots>, values: &[(&str, f64)]) -> FeedMessage {
    let rig = DeviceRig::new("software-feed");
    let lookup = {
        let slots = slots.clone();
        Arc::new(move |id: &str| (id == LEAKSHIELD_ID).then(|| slots.clone()))
    };
    let (mut stage, mut link) = feed_channel_with(lookup);
    let mut ctx = TickContext::new(TickConfig {
        loops: vec![loop_setup()],
        ..TickConfig::default()
    });
    for (id, value) in values {
        ctx.values.insert((*id).to_owned(), *value);
    }
    stage.run(&mut ctx);
    let devices = BTreeMap::from([(LEAKSHIELD_ID.to_owned(), rig.io.clone())]);
    link.flush(&devices, &mut |_, result| assert!(result.is_ok()));
    FeedMessage::decode(&rig.sim.bulk_messages()[0]).unwrap()
}

fn temperature(sensor: &str) -> Assignment {
    Assignment::new(sensor, "degC").unwrap()
}

#[test]
fn slot_count_is_12() {
    assert_eq!(SLOT_COUNT, 12);
    // The 12 Software sensors are the slots 4 to 15 of the Feed message.
    assert_eq!(FIRST_FEED_SLOT, 4);
    assert_eq!(FIRST_FEED_SLOT + SLOT_COUNT, FEED_SLOTS);
    let slots = SoftwareSlots::default();
    let listed = slots.list();
    assert_eq!(listed.len(), 12);
    assert_eq!(listed.first().map(|s| s.0), Some(1));
    assert_eq!(listed.last().map(|s| s.0), Some(12));
    assert!(slots.assign(0, None).is_err());
    assert!(slots.assign(13, None).is_err());
}

#[test]
fn assigned_sensor_value_in_feed_slot() {
    let slots = Arc::new(SoftwareSlots::default());
    slots.assign(1, Some(temperature("water_temp"))).unwrap();
    slots.assign(12, Some(temperature("air_temp"))).unwrap();

    let message = feed(&slots, &[("water_temp", 31.456), ("air_temp", 22.0)]);

    // 2 decimals for the unit degC.
    assert_eq!(
        (message.value(4), message.unit(4)),
        (3146, TEMPERATURE_UNIT_CODE)
    );
    assert_eq!(
        (message.value(15), message.unit(15)),
        (2200, TEMPERATURE_UNIT_CODE)
    );
    for slot in 5..15 {
        assert_eq!(message.value(slot), NO_VALUE, "slot {slot}");
    }
}

#[test]
fn cleared_slot_sends_no_value() {
    let slots = Arc::new(SoftwareSlots::default());
    slots.assign(3, Some(temperature("water_temp"))).unwrap();
    assert_eq!(feed(&slots, &[("water_temp", 30.0)]).value(6), 3000);

    slots.assign(3, None).unwrap();
    let message = feed(&slots, &[("water_temp", 30.0)]);
    assert_eq!((message.value(6), message.unit(6)), (NO_VALUE, 0));

    // A sensor that has no value leaves its slot at "no value" too.
    slots.assign(3, Some(temperature("water_temp"))).unwrap();
    let message = feed(&slots, &[]);
    assert_eq!((message.value(6), message.unit(6)), (NO_VALUE, 0));
}

async fn leakshield(rig: &Rig) -> LeakshieldProxy<'static> {
    LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn software_sensor_over_the_bus_reaches_the_feed_message() {
    let mut rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    assert_eq!(proxy.software_sensor_slots().await.unwrap(), 12);
    let slots = proxy.get_software_sensors().await.unwrap();
    assert_eq!(slots.len(), 12);
    assert!(slots.iter().all(|(_, sensor)| sensor.as_str() == "/"));

    let sensor = path(Kind::Sensor, "k10temp_temp1");
    proxy.set_software_sensor(3, &sensor).await.unwrap();
    assert_eq!(proxy.get_software_sensors().await.unwrap()[2], (3, sensor));

    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    hub.set_loop_leakshield("loop_1", &object_path(Kind::Leakshield, LEAKSHIELD))
        .unwrap();
    rig.tick_in_background();
    let value = loop {
        let last = rig.sim.bulk_messages().last().cloned();
        if let Some(message) = last.and_then(|m| FeedMessage::decode(&m).ok())
            && message.value(6) != NO_VALUE
        {
            break (message.value(6), message.unit(6));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    // 61.875 degC with 2 decimals.
    assert_eq!(value, (6188, TEMPERATURE_UNIT_CODE));

    // Clearing the slot stops the value.
    proxy.set_software_sensor(3, &none_path()).await.unwrap();
    loop {
        let message = FeedMessage::decode(rig.sim.bulk_messages().last().unwrap()).unwrap();
        if message.value(6) == NO_VALUE {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn software_sensor_errors() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let sensor = path(Kind::Sensor, "k10temp_temp1");

    for slot in [0, 13] {
        let error = proxy.set_software_sensor(slot, &sensor).await.unwrap_err();
        assert_eq!(
            error_name(&error),
            format!("{ERR}.InvalidArgument"),
            "{slot}"
        );
    }
    let error = proxy
        .set_software_sensor(1, &path(Kind::Sensor, "no_such_sensor"))
        .await
        .unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.NotFound"));
    let error = proxy
        .set_software_sensor(1, &path(Kind::Channel, "nct6798_pwm1"))
        .await
        .unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.InvalidArgument"));
}
