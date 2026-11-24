//! The report 1 source of one Leakshield: it reads report 1 every period, updates the state of
//! the device and publishes the device sensors to the value table.

use std::sync::Arc;
use std::time::Duration;

use nicecooling_api::ids::leakshield_sensor_id;
use nicecooling_core::clock::Clock;
use nicecooling_protocol::feed::{UNIT_LITERS_PER_HOUR, UNIT_SPEED_RPM};
use nicecooling_protocol::report1::Report1;
use tokio::sync::oneshot;

use super::hwmon::Source;
use crate::dbus::SensorMeta;
use crate::leakshield::Attached;
use crate::leakshield::attached::now_unix;
use crate::value_table::ValueTable;

/// Report 1 value for "no value".
const NO_VALUE: i16 = 0x7FFF;

/// A sensor of a Leakshield: field id, default name, kind and unit of `docs/api.md`.
pub const SENSORS: [(&str, &str, &str, &str); 15] = [
    ("pressure", "Pressure", "pressure", "mbar"),
    ("pressure_setpoint", "Pressure target", "pressure", "mbar"),
    ("pressure_low", "Pressure minimum", "pressure", "mbar"),
    ("pressure_high", "Pressure maximum", "pressure", "mbar"),
    ("fill_level", "Fill level", "level", "%"),
    ("volume_change", "Volume change", "other", "ml/h"),
    ("pressure_change", "Pressure change", "other", "mbar/min"),
    ("leak_indicator", "Leak indicator", "other", "ml/h"),
    ("daily_pump_time", "Daily pump time", "other", "s"),
    ("pump_head_pressure", "Pump pressure", "pressure", "mbar"),
    ("air_volume", "Air volume", "level", "ml"),
    ("water_volume", "Water volume", "level", "ml"),
    ("supply_voltage", "Supply voltage", "voltage", "V"),
    ("feed_rpm", "Pump speed", "fan", "rpm"),
    ("flow", "Flow", "flow", "l/h"),
];

/// The report 1 sensor slots of the Feed values: slot 4 holds the pump speed, slot 5 the flow.
const SLOT_PUMP_SPEED: usize = 4;
const SLOT_FLOW: usize = 5;

/// The API sensors of a Leakshield.
pub fn sensor_metas(leakshield: &str) -> Vec<SensorMeta> {
    SENSORS
        .into_iter()
        .map(|(field, name, kind, unit)| SensorMeta {
            id: leakshield_sensor_id(leakshield, field),
            name: name.to_owned(),
            kind,
            unit,
            driver: String::new(),
        })
        .collect()
}

/// The values of the sensors in report 1, in the units of `SENSORS`. A value of 32767 is no
/// value. The Feed values come from the device sensors 4 and 5, and only while the unit code of
/// the slot is the unit that the Feed sends.
pub fn sensor_values(report1: &Report1) -> Vec<(&'static str, f64)> {
    let slot = |index: usize, unit_code: u8, scale: f64| {
        let slot = &report1.sensors[index];
        (slot.unit_code == unit_code).then_some((slot.value, scale))
    };
    let feed_rpm = slot(SLOT_PUMP_SPEED, UNIT_SPEED_RPM, 1.0);
    let flow = slot(SLOT_FLOW, UNIT_LITERS_PER_HOUR, 0.1);
    let raw = [
        ("pressure", Some((report1.pressure, 0.1))),
        ("pressure_setpoint", Some((report1.pressure_setpoint, 0.1))),
        ("pressure_low", Some((report1.pressure_low, 0.1))),
        ("pressure_high", Some((report1.pressure_high, 0.1))),
        ("fill_level", Some((report1.fill_level, 0.01))),
        ("volume_change", Some((report1.volume_leak_rate, 1.0))),
        ("pressure_change", Some((report1.pressure_leak_rate, 0.1))),
        (
            "leak_indicator",
            Some((report1.volume_leak_rate_average, 1.0)),
        ),
        ("daily_pump_time", Some((report1.daily_pump_duration, 1.0))),
        (
            "pump_head_pressure",
            Some((report1.pump_head_pressure, 0.1)),
        ),
        ("air_volume", Some((report1.air_volume, 1.0))),
        ("water_volume", Some((report1.water_volume, 1.0))),
        ("supply_voltage", Some((report1.supply_voltage, 0.01))),
        ("feed_rpm", feed_rpm),
        ("flow", flow),
    ];
    raw.into_iter()
        .filter_map(|(field, value)| Some((field, value?)))
        .filter(|&(_, (raw, _))| raw != NO_VALUE)
        .map(|(field, (raw, scale))| (field, f64::from(raw) * scale))
        .collect()
}

/// Reads report 1 every `period`. A read that blocks stops this task only: the values go stale
/// and the Lost Input rule covers them (design D6).
pub fn spawn_source(
    device: Arc<Attached>,
    table: ValueTable,
    clock: Arc<dyn Clock>,
    period: Duration,
) -> Source {
    let (primed_tx, primed) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut primed_tx = Some(primed_tx);
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            let reader = device.clone();
            let Ok(report1) = tokio::task::spawn_blocking(move || reader.poll(now_unix())).await
            else {
                return;
            };
            if let Some(report1) = report1 {
                let now = clock.now();
                table.publish_report1(
                    &device.id(),
                    report1.alarm_bits,
                    report1.alarm_bits_latched,
                    now,
                );
                for (field, value) in sensor_values(&report1) {
                    table.publish(&leakshield_sensor_id(&device.id(), field), value, now);
                }
            }
            if let Some(tx) = primed_tx.take() {
                let _ = tx.send(());
            }
        }
    });
    Source { task, primed }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FW1019: &[u8] =
        include_bytes!("../../../../tests/fixtures/leakshield/fw1019/report1.bin");
    const FW1022: &[u8] =
        include_bytes!("../../../../tests/fixtures/leakshield/fw1022/report1.bin");

    fn values(bytes: &[u8]) -> std::collections::BTreeMap<&'static str, f64> {
        sensor_values(&Report1::decode(bytes).unwrap())
            .into_iter()
            .collect()
    }

    fn near(actual: Option<&f64>, expected: f64) {
        let actual = *actual.expect("the sensor has a value");
        assert!((actual - expected).abs() < 1e-9, "{actual} != {expected}");
    }

    #[test]
    fn fw1022_dump_gives_the_runtime_values() {
        let v = values(FW1022);
        near(v.get("pump_head_pressure"), 101.2);
        near(v.get("supply_voltage"), 4.69);
        near(v.get("air_volume"), 108.0);
        near(v.get("water_volume"), 162.0);
        near(v.get("daily_pump_time"), 0.0);
        // Feed sensor 4 holds 2500 rpm and sensor 5 holds 1300 in 0.1 l/h.
        near(v.get("feed_rpm"), 2500.0);
        near(v.get("flow"), 130.0);
        // The dump holds "no value" in these.
        for name in ["volume_change", "pressure_change", "leak_indicator"] {
            assert!(!v.contains_key(name), "{name}");
        }
    }

    #[test]
    fn fw1019_dump_has_no_value_for_the_pump_and_the_feed() {
        let v = values(FW1019);
        near(v.get("supply_voltage"), 4.87);
        near(v.get("air_volume"), 108.0);
        for name in ["pump_head_pressure", "feed_rpm", "flow", "volume_change"] {
            assert!(!v.contains_key(name), "{name}");
        }
    }

    #[test]
    fn leak_values_use_their_scales() {
        let mut report1 = Report1::decode(FW1022).unwrap();
        report1.volume_leak_rate = 2;
        report1.pressure_leak_rate = -4;
        report1.volume_leak_rate_average = 7;
        let v: std::collections::BTreeMap<_, _> = sensor_values(&report1).into_iter().collect();
        near(v.get("volume_change"), 2.0);
        near(v.get("pressure_change"), -0.4);
        near(v.get("leak_indicator"), 7.0);
    }

    #[test]
    fn a_feed_slot_in_another_unit_has_no_value() {
        let mut report1 = Report1::decode(FW1022).unwrap();
        report1.sensors[SLOT_PUMP_SPEED].unit_code = 4;
        report1.sensors[SLOT_FLOW].unit_code = 13;
        let v: std::collections::BTreeMap<_, _> = sensor_values(&report1).into_iter().collect();
        assert!(!v.contains_key("feed_rpm") && !v.contains_key("flow"));
    }

    #[test]
    fn every_sensor_has_a_meta_row() {
        let metas = sensor_metas("leakshield_1_2");
        assert_eq!(metas.len(), SENSORS.len());
        let names: std::collections::BTreeSet<_> = SENSORS.iter().map(|s| s.0).collect();
        assert_eq!(names.len(), SENSORS.len());
        assert_eq!(metas[13].id, "leakshield_1_2_feed_rpm");
    }
}
