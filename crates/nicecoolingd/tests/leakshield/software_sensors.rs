use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use nicecooling_api::ids::{Kind, object_path};
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_core::loops::Loop;
use nicecooling_core::tick::{LoopSetup, Stage, TickConfig, TickContext};
use nicecooling_protocol::feed::{FEED_SLOTS, FeedMessage, NO_VALUE};
use nicecoolingd::daemon;
use nicecoolingd::leakshield::feed::feed_channel_with;
use nicecoolingd::leakshield::software_sensors::{
    Assignment, FIRST_FEED_SLOT, SLOT_COUNT, SoftwareSlots,
};

use nicecoolingd::tick_runner::TokioClock;

use crate::support::{LEAKSHIELD, NoPing, Rig, error_name, none_path, path};
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn software_sensor_assignment_survives_a_daemon_restart() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    for (slot, id) in [
        (3, "k10temp_temp1"),
        (12, "nct6798_temp1"),
        (5, "nct6798_fan1"),
    ] {
        proxy
            .set_software_sensor(slot, &path(Kind::Sensor, id))
            .await
            .unwrap();
    }
    // Clearing a slot persists too.
    proxy.set_software_sensor(5, &none_path()).await.unwrap();
    // The daemon flushes its state before it exits.
    rig.daemon.as_ref().unwrap().hub.flush_now();

    let restarted = daemon::start(&rig.options, Arc::new(TokioClock::new()), NoPing)
        .await
        .unwrap();

    let devices = restarted.fleet.all();
    let assigned: Vec<(u16, String)> = devices[0]
        .software_slots()
        .list()
        .into_iter()
        .filter_map(|(slot, sensor)| Some((slot, sensor?)))
        .collect();
    assert_eq!(
        assigned,
        [
            (3, "k10temp_temp1".to_owned()),
            (12, "nct6798_temp1".to_owned())
        ]
    );
}

#[test]
fn scale_and_offset_apply_before_the_unit_scale() {
    let slots = Arc::new(SoftwareSlots::default());
    slots.assign(1, Some(temperature("water_temp"))).unwrap();
    slots.assign(2, Some(temperature("air_temp"))).unwrap();
    // Slot 1: 2 x value + 5. Slot 2 keeps the default scale 1 and offset 0.
    slots.set_scale(1, 2.0, 5.0).unwrap();

    let message = feed(&slots, &[("water_temp", 10.0), ("air_temp", 22.0)]);

    // (10 x 2 + 5) degC with 2 decimals.
    assert_eq!(message.value(4), 2500);
    assert_eq!(message.value(5), 2200);
    assert_eq!(
        slots.scales()[..3],
        [(1, 2.0, 5.0), (2, 1.0, 0.0), (3, 1.0, 0.0)]
    );
}

#[test]
fn a_value_that_the_offset_pushes_out_of_range_sends_no_value() {
    let slots = Arc::new(SoftwareSlots::default());
    slots.assign(1, Some(temperature("water_temp"))).unwrap();
    slots.set_scale(1, 1.0, 400.0).unwrap();

    // 430 degC does not fit the slot (2 decimals). A sensor with no value stays without one.
    assert_eq!(feed(&slots, &[("water_temp", 30.0)]).value(4), NO_VALUE);
    assert_eq!(feed(&slots, &[]).value(4), NO_VALUE);
    assert!(slots.set_scale(0, 1.0, 0.0).is_err());
    assert!(slots.set_scale(13, 1.0, 0.0).is_err());
}

#[test]
fn scale_and_offset_stay_when_the_sensor_changes() {
    let slots = Arc::new(SoftwareSlots::default());
    slots.set_scale(3, 0.5, -1.0).unwrap();
    slots.assign(3, Some(temperature("water_temp"))).unwrap();
    slots.assign(3, None).unwrap();
    assert_eq!(slots.scales()[2], (3, 0.5, -1.0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scale_and_offset_over_the_bus_reach_the_feed_message() {
    let mut rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let scales = proxy.get_software_sensor_scales().await.unwrap();
    assert_eq!(scales.len(), 12);
    assert!(
        scales
            .iter()
            .all(|&(_, scale, offset)| (scale, offset) == (1.0, 0.0))
    );

    proxy
        .set_software_sensor(3, &path(Kind::Sensor, "k10temp_temp1"))
        .await
        .unwrap();
    proxy.set_software_sensor_scale(3, 0.0, 50.0).await.unwrap();
    assert_eq!(
        proxy.get_software_sensor_scales().await.unwrap()[2],
        (3, 0.0, 50.0)
    );

    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    hub.set_loop_leakshield("loop_1", &object_path(Kind::Leakshield, LEAKSHIELD))
        .unwrap();
    rig.tick_in_background();
    let value = loop {
        let last = rig.sim.bulk_messages().last().cloned();
        if let Some(message) = last.and_then(|m| FeedMessage::decode(&m).ok())
            && message.value(6) != NO_VALUE
        {
            break message.value(6);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    // Scale 0 and offset 50: the slot carries 50 degC whatever the sensor reads.
    assert_eq!(value, 5000);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scale_and_offset_errors_over_the_bus() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    for (slot, scale, offset) in [
        (0, 1.0, 0.0),
        (13, 1.0, 0.0),
        (1, f64::NAN, 0.0),
        (1, 1.0, f64::INFINITY),
    ] {
        let error = proxy
            .set_software_sensor_scale(slot, scale, offset)
            .await
            .unwrap_err();
        assert_eq!(
            error_name(&error),
            format!("{ERR}.InvalidArgument"),
            "{slot} {scale} {offset}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn scale_and_offset_survive_a_daemon_restart() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    proxy.set_software_sensor_scale(4, 1.8, 32.0).await.unwrap();
    // The default values leave no entry.
    proxy.set_software_sensor_scale(5, 2.0, 0.0).await.unwrap();
    proxy.set_software_sensor_scale(5, 1.0, 0.0).await.unwrap();
    rig.daemon.as_ref().unwrap().hub.flush_now();

    let state = std::fs::read_to_string(rig.state.join("state.json")).unwrap();
    let state: serde_json::Value = serde_json::from_str(&state).unwrap();
    assert_eq!(
        state["software_sensor_scales"][LEAKSHIELD],
        serde_json::json!({"4": {"scale": 1.8, "offset": 32.0}})
    );

    let restarted = daemon::start(&rig.options, Arc::new(TokioClock::new()), NoPing)
        .await
        .unwrap();
    let devices = restarted.fleet.all();
    assert_eq!(devices[0].software_slots().scales()[3], (4, 1.8, 32.0));
    assert_eq!(devices[0].software_slots().scales()[4], (5, 1.0, 0.0));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_source_list_has_only_sensors_whose_unit_the_device_knows() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;

    let sources = proxy.get_software_sensor_sources().await.unwrap();

    let has = |id: &str| sources.iter().any(|p| *p == path(Kind::Sensor, id));
    assert!(has("k10temp_temp1") && has("nct6798_fan1"));
    // The air volume of the Leakshield is in ml: the device has no such unit.
    assert!(!has(&format!("{LEAKSHIELD}_air_volume")));
    // Every source takes an assignment.
    for source in &sources {
        proxy.set_software_sensor(1, source).await.unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn setting_scale_and_offset_asks_write() {
    if std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .is_some_and(|l| l.split_whitespace().next() == Some("0"))
    {
        return;
    }
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;

    proxy.get_software_sensor_scales().await.unwrap();
    proxy.get_software_sensor_sources().await.unwrap();
    assert!(rig.authorizer.calls().is_empty());
    proxy.set_software_sensor_scale(1, 2.0, 1.0).await.unwrap();
    assert_eq!(
        rig.authorizer.calls(),
        [(nicecooling_api::authz_map::Action::Write, false)]
    );
}
