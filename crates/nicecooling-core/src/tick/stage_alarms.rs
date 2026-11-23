use std::collections::BTreeMap;

use super::{STALE_AFTER, Stage, TickContext};
use crate::alarms::leakshield::{self, GROUPS};
use crate::alarms::pump_stop::{PumpReading, PumpStop};
use crate::alarms::unreachable::Unreachable;
use crate::alarms::{LeakshieldStatus, Severity};
use crate::loops::tach_sensor;

/// The event id of the pump stop Alarm.
pub const PUMP_STOP: &str = "pump-stop";
/// The event id of the Alarm "Leakshield unreachable".
pub const UNREACHABLE: &str = "leakshield-unreachable";

/// Stage 3: evaluate the Alarms. It decodes the Warnings and Alarms of each Leakshield and runs
/// the pump stop rule of each Loop.
#[derive(Default)]
pub struct AlarmsStage {
    pump_stop: BTreeMap<String, PumpStop>,
    unreachable: BTreeMap<String, Unreachable>,
}

impl Stage for AlarmsStage {
    fn name(&self) -> &'static str {
        "alarms"
    }

    fn run(&mut self, ctx: &mut TickContext) {
        let TickContext {
            now,
            config,
            values,
            channels,
            alarms,
            leakshields,
            ..
        } = ctx;

        // A Leakshield without a fresh report 1 keeps the events it had: its state is unknown.
        for (id, status) in leakshields.iter() {
            if !status
                .report1_at
                .is_some_and(|at| now.saturating_sub(at) <= STALE_AFTER)
            {
                continue;
            }
            for group in GROUPS {
                alarms.clear(group.id, id);
            }
            for (event, severity, bits) in leakshield::decode(status.alarm_bits) {
                alarms.raise_bits(event, id, severity, bits);
            }
        }

        // The Leakshields of the sources, and those that a Loop names and no source has seen.
        let mut known: BTreeMap<&str, LeakshieldStatus> = leakshields
            .iter()
            .map(|(id, s)| (id.as_str(), s.clone()))
            .collect();
        for setup in &config.loops {
            if let Some(id) = &setup.config.leakshield {
                known.entry(id).or_default();
            }
        }
        for (id, status) in known {
            let rule = self.unreachable.entry(id.to_owned()).or_default();
            if rule.update(*now, &status) {
                alarms.raise(UNREACHABLE, id, Severity::Alarm);
            } else {
                alarms.clear(UNREACHABLE, id);
            }
        }

        let loops: Vec<&str> = config.loops.iter().map(|l| l.config.id.as_str()).collect();
        self.pump_stop.retain(|id, _| loops.contains(&id.as_str()));
        alarms.retain(|event, source| event != PUMP_STOP || loops.contains(&source));
        for setup in &config.loops {
            let pumps: Vec<PumpReading> = setup
                .config
                .pump_channels
                .iter()
                .map(|channel| PumpReading {
                    rpm: tach_sensor(channel).and_then(|id| values.get(&id).copied()),
                    // The Duty of the last tick, or the `pwmN` value of a Hardware curve.
                    duty_positive: channels
                        .get(channel)
                        .and_then(|t| t.last_duty)
                        .or_else(|| values.get(channel).copied())
                        .is_some_and(|duty| duty > 0.0),
                })
                .collect();
            let rule = self.pump_stop.entry(setup.config.id.clone()).or_default();
            if rule.update(*now, &pumps) {
                alarms.raise(PUMP_STOP, &setup.config.id, Severity::Alarm);
            } else {
                alarms.clear(PUMP_STOP, &setup.config.id);
            }
        }
    }
}
