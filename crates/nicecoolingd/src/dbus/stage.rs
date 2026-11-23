//! The last tick stage before the watchdog ping. It hands the new configuration of a write to
//! the tick, restores released Channels, and publishes the values for the properties and the
//! `SensorsUpdated` signal.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use nicecooling_core::alarms::Severity;
use nicecooling_core::tick::{Stage, TickContext};
use nicecooling_hwmon::sysfs::read_attr;

use super::hub::{ChannelLive, Hub, Live, TickReport};
use crate::alarms::poweroff::Event;

/// `SensorsUpdated` flags of a sensor.
const NO_VALUE: u8 = 1;
const OUT_OF_RANGE: u8 = 2;
/// `SensorsUpdated` flags of a Channel.
const OVERRIDE_ACTIVE: u8 = 1;
const ON_HARDWARE_CURVE: u8 = 2;

pub struct ApiStage {
    hub: Arc<Hub>,
}

impl ApiStage {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self { hub }
    }
}

fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64)
}

impl Stage for ApiStage {
    fn name(&self) -> &'static str {
        "api"
    }

    fn run(&mut self, ctx: &mut TickContext) {
        // A release commits its change of control, so the new configuration comes after it.
        for (id, reply) in self.hub.take_releases() {
            if !reply.is_closed() {
                let _ = reply.send(self.hub.release_now(&id));
            }
        }
        if let Some(config) = self.hub.take_pending_config() {
            ctx.config = config;
        }

        let mut live = Live {
            values: ctx.values.clone(),
            out_of_range: ctx.out_of_range.clone(),
            ..Live::default()
        };
        let sensors = self
            .hub
            .sensor_ids()
            .into_iter()
            .map(|id| {
                let flags = match ctx.values.get(&id) {
                    None => NO_VALUE,
                    Some(_) if ctx.out_of_range.contains(&id) => OUT_OF_RANGE,
                    Some(_) => 0,
                };
                (
                    id.clone(),
                    ctx.values.get(&id).copied().unwrap_or(0.0),
                    flags,
                )
            })
            .collect();
        let mut channels = Vec::new();
        for meta in self.hub.channel_metas() {
            let tick = ctx.channels.get(&meta.id);
            let raw: Option<u8> = read_attr(&meta.dir, &format!("pwm{}", meta.number))
                .ok()
                .and_then(|text| text.parse().ok());
            let duty = raw.map_or(0.0, |raw| f64::from(raw) * 100.0 / 255.0);
            let overriding = tick.is_some_and(|t| t.override_active);
            let on_hardware = tick.is_none_or(|t| t.duty.is_none());
            let mut flags = 0;
            if overriding {
                flags |= OVERRIDE_ACTIVE;
            }
            if on_hardware {
                flags |= ON_HARDWARE_CURVE;
            }
            channels.push((meta.id.clone(), duty, flags));
            live.channels.insert(
                meta.id.clone(),
                ChannelLive {
                    duty,
                    raw: raw.unwrap_or(0),
                    override_state: if overriding && ctx.alarms.any_active("pump-stop") {
                        "pump-stop"
                    } else if ctx.warnings.is_active("lost-input", &meta.id) {
                        "lost-input"
                    } else {
                        "none"
                    },
                },
            );
            if ctx.warnings.is_active("manual-at-snapshot", &meta.id) {
                live.manual_at_snapshot.insert(meta.id);
            }
        }
        let active: Vec<Event> = ctx
            .warnings
            .iter()
            .map(|(event, source)| (event, source, Severity::Warning))
            .chain(ctx.alarms.iter())
            .map(|(event, source, severity)| Event {
                event: event.to_owned(),
                source: source.to_owned(),
                severity,
            })
            .collect();
        self.hub.alarm_tick(ctx.now, now_us(), &active);
        self.hub.publish(
            live,
            TickReport {
                time_us: now_us(),
                sensors,
                channels,
            },
        );
    }
}
