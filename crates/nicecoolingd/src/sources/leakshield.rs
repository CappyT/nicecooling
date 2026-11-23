//! The report 1 source of one Leakshield: it reads report 1 every period, updates the state of
//! the device and publishes the device sensors to the value table.

use std::sync::Arc;
use std::time::Duration;

use nicecooling_api::ids::leakshield_sensor_id;
use nicecooling_core::clock::Clock;
use nicecooling_protocol::report1::Report1;
use tokio::sync::oneshot;

use super::hwmon::Source;
use crate::dbus::SensorMeta;
use crate::leakshield::Attached;
use crate::leakshield::attached::now_unix;
use crate::value_table::ValueTable;

/// Report 1 value for "no value".
const NO_VALUE: i16 = 0x7FFF;

/// A device sensor: field id, default name, kind and unit of `docs/api.md`.
pub const SENSORS: [(&str, &str, &str, &str); 2] = [
    ("pressure", "Pressure", "pressure", "mbar"),
    ("fill_level", "Fill level", "level", "%"),
];

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

/// The values of the device sensors in report 1: the pressure in mbar (0.1 mbar per count) and
/// the fill level in percent (0.01 % per count). A value of 32767 is no value.
pub fn sensor_values(report1: &Report1) -> Vec<(&'static str, f64)> {
    [
        ("pressure", report1.pressure, 0.1),
        ("fill_level", report1.fill_level, 0.01),
    ]
    .into_iter()
    .filter(|&(_, raw, _)| raw != NO_VALUE)
    .map(|(field, raw, scale)| (field, f64::from(raw) * scale))
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
                table.publish_report1(&device.id(), report1.alarm_bits, now);
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
