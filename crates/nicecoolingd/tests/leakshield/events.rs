use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_protocol::report1::EventEntry;
use nicecoolingd::leakshield::clock::DEVICE_EPOCH_UNIX;
use nicecoolingd::leakshield::events::decode_events;

use crate::Rig as DeviceRig;
use crate::support::{LEAKSHIELD, Rig, path};

const NO_VALUE: i16 = 0x7FFF;

fn empty() -> EventEntry {
    EventEntry {
        event_code: 255,
        ..EventEntry::default()
    }
}

/// Three events out of order, and 12 empty entries.
fn put_events(sim: &nicecooling_protocol::sim::SimDevice) {
    sim.edit_report1(|r| {
        r.event_list = [empty(); 15];
        r.event_list[0] = EventEntry {
            time: 2000,
            entry_id: 3,
            event_code: 105,
            pressure: NO_VALUE,
            value_1: 77,
            value_2: NO_VALUE,
            mode: 1,
            ..empty()
        };
        r.event_list[1] = EventEntry {
            time: 500,
            entry_id: 1,
            event_code: 0,
            pressure: NO_VALUE,
            value_1: NO_VALUE,
            value_2: NO_VALUE,
            ..empty()
        };
        r.event_list[2] = EventEntry {
            time: 1000,
            entry_id: 2,
            event_code: 1,
            mode: 1,
            procedure_state: 4,
            pressure: 3005,
            pressure_leak_rate: -2,
            volume_leak_rate: 3,
            value_1: NO_VALUE,
            value_2: NO_VALUE,
        };
    });
}

#[test]
fn decode_events_of_simulated_device() {
    let rig = DeviceRig::new("events");
    put_events(&rig.sim);

    let events = decode_events(&rig.io.read_report1().unwrap());

    // Oldest first. The empty entries (code 255) are left out.
    let ids: Vec<&str> = events.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "leakshield.system-start",
            "leakshield.mode-shield",
            "leakshield.pump-runtime-alarm"
        ]
    );
    let time = |seconds: u64| (seconds + DEVICE_EPOCH_UNIX as u64) * 1_000_000;
    assert_eq!(
        events.iter().map(|e| e.time_us).collect::<Vec<_>>(),
        [time(500), time(1000), time(2000)]
    );
    // A value that the device marks as "no value" is not in the details.
    let keys = |i: usize| events[i].details.iter().map(|d| d.0).collect::<Vec<_>>();
    assert!(keys(0).contains(&"entry_id"));
    assert!(!keys(0).contains(&"pressure") && !keys(0).contains(&"value_1"));
    let mode = &events[1].details;
    assert!(mode.contains(&("pressure", 300.5)));
    assert!(mode.contains(&("pressure_leak_rate", -2.0)));
    assert!(mode.contains(&("procedure_state", 4.0)));
    assert!(!mode.iter().any(|(key, _)| *key == "value_1"));
}

#[test]
fn an_unknown_code_keeps_its_number() {
    let rig = DeviceRig::new("events-unknown");
    rig.sim.edit_report1(|r| {
        r.event_list = [empty(); 15];
        r.event_list[0].event_code = 99;
    });
    let events = decode_events(&rig.io.read_report1().unwrap());
    assert_eq!(events[0].id, "leakshield.event-99");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_events_returns_time_identifier_details() {
    let rig = Rig::start().await;
    put_events(&rig.sim);
    let proxy = LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();

    let events = proxy.get_events().await.unwrap();

    assert_eq!(events.len(), 3);
    let (time_us, id, details) = &events[1];
    assert_eq!(*time_us, (1000 + DEVICE_EPOCH_UNIX as u64) * 1_000_000);
    assert_eq!(id, "leakshield.mode-shield");
    let number = |key: &str| f64::try_from(details[key].try_clone().unwrap()).unwrap();
    assert_eq!(number("entry_id"), 2.0);
    assert_eq!(number("pressure"), 300.5);
    assert_eq!(number("mode"), 1.0);
    assert!(!details.contains_key("value_1"));
}
