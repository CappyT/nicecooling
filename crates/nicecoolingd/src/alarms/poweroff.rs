//! The active Alarms and Warnings, their alarm response and the poweroff countdown (design D13).
//! The manager is a plain state machine on the monotonic clock of the tick: it returns effects
//! and never waits. The hub runs the effects: signals and the logind call.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use nicecooling_core::alarms::Severity;

use super::response::{Response, resolve};
use crate::state::AlarmResponse;

/// A logind call that fails is made again after this time, while the Alarm is active.
pub const RETRY_EVERY: Duration = Duration::from_secs(5);

/// An active Warning or Alarm as the tick sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Event {
    pub event: String,
    pub source: String,
    pub severity: Severity,
}

/// One row of the property `Active`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub instance: String,
    pub event: String,
    pub severity: &'static str,
    pub source: String,
    pub response: &'static str,
    pub raised_us: u64,
    /// 0 when no poweroff is pending.
    pub poweroff_at_us: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Raised(Row),
    Cleared(String),
    PoweroffCancelled(String),
    PoweroffFailed {
        instance: String,
        error: String,
    },
    /// Call logind. The caller reports the result with `poweroff_result`.
    CallPoweroff(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Countdown {
    /// No poweroff for this instance, or the user cancelled it.
    Off,
    Pending {
        deadline: Duration,
    },
    /// The call to logind is under way.
    Calling,
    Retry {
        at: Duration,
    },
    /// logind accepted the call.
    Done,
}

#[derive(Clone, Debug)]
struct Instance {
    row: Row,
    countdown: Countdown,
}

impl Instance {
    fn pending(&self) -> bool {
        matches!(
            self.countdown,
            Countdown::Pending { .. } | Countdown::Calling | Countdown::Retry { .. }
        )
    }
}

#[derive(Debug, Default)]
pub struct AlarmManager {
    instances: Vec<Instance>,
    next: u64,
    now: Duration,
}

fn severity_name(severity: Severity) -> &'static str {
    severity.as_str()
}

impl AlarmManager {
    /// Feeds one tick: the Warnings and Alarms that are active now. `now` is the tick time and
    /// `wall_us` the wall clock, which only labels the rows.
    pub fn update(
        &mut self,
        now: Duration,
        wall_us: u64,
        active: &[Event],
        responses: &BTreeMap<String, AlarmResponse>,
    ) -> Vec<Effect> {
        self.now = now;
        let mut effects = Vec::new();
        let wanted: BTreeSet<(&str, &str)> = active
            .iter()
            .map(|e| (e.event.as_str(), e.source.as_str()))
            .collect();
        self.instances.retain(|i| {
            let keep = wanted.contains(&(i.row.event.as_str(), i.row.source.as_str()));
            if !keep {
                effects.push(Effect::Cleared(i.row.instance.clone()));
            }
            keep
        });
        for event in active {
            if self
                .instances
                .iter()
                .any(|i| i.row.event == event.event && i.row.source == event.source)
            {
                continue;
            }
            self.next += 1;
            let (response, delay_s) = resolve(responses, &event.event);
            let (countdown, poweroff_at_us) = if response == Response::Poweroff {
                (
                    Countdown::Pending {
                        deadline: now + Duration::from_secs(delay_s.into()),
                    },
                    wall_us + u64::from(delay_s) * 1_000_000,
                )
            } else {
                (Countdown::Off, 0)
            };
            let row = Row {
                instance: format!("alarm_{}", self.next),
                event: event.event.clone(),
                severity: severity_name(event.severity),
                source: event.source.clone(),
                response: response.as_str(),
                raised_us: wall_us,
                poweroff_at_us,
            };
            effects.push(Effect::Raised(row.clone()));
            self.instances.push(Instance { row, countdown });
        }
        for instance in &mut self.instances {
            let due = match instance.countdown {
                Countdown::Pending { deadline } => now >= deadline,
                Countdown::Retry { at } => now >= at,
                _ => false,
            };
            if due {
                instance.countdown = Countdown::Calling;
                effects.push(Effect::CallPoweroff(instance.row.instance.clone()));
            }
        }
        effects
    }

    /// The result of a logind call. An instance that is gone or was cancelled ignores it.
    pub fn poweroff_result(&mut self, instance: &str, result: Result<(), String>) -> Vec<Effect> {
        let now = self.now;
        let Some(found) = self
            .instances
            .iter_mut()
            .find(|i| i.row.instance == instance && i.countdown == Countdown::Calling)
        else {
            return Vec::new();
        };
        match result {
            Ok(()) => {
                found.countdown = Countdown::Done;
                Vec::new()
            }
            Err(error) => {
                found.countdown = Countdown::Retry {
                    at: now + RETRY_EVERY,
                };
                vec![Effect::PoweroffFailed {
                    instance: instance.to_owned(),
                    error,
                }]
            }
        }
    }

    /// Cancels the pending poweroff of an instance. The Alarm stays active, and it never powers
    /// off again as this instance. `None` when the instance has no pending poweroff.
    pub fn cancel(&mut self, instance: &str) -> Option<Effect> {
        let found = self
            .instances
            .iter_mut()
            .find(|i| i.row.instance == instance && i.pending())?;
        found.countdown = Countdown::Off;
        found.row.poweroff_at_us = 0;
        Some(Effect::PoweroffCancelled(instance.to_owned()))
    }

    pub fn rows(&self) -> Vec<Row> {
        self.instances.iter().map(|i| i.row.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    fn event(name: &str, severity: Severity) -> Event {
        Event {
            event: name.into(),
            source: "loop_1".into(),
            severity,
        }
    }

    fn pump_stop() -> Vec<Event> {
        vec![event("pump-stop", Severity::Alarm)]
    }

    fn calls(effects: &[Effect]) -> usize {
        effects
            .iter()
            .filter(|e| matches!(e, Effect::CallPoweroff(_)))
            .count()
    }

    /// Runs one tick a second from `from` to `to`, and returns the effects of all of them.
    fn run(
        manager: &mut AlarmManager,
        seconds: std::ops::Range<u32>,
        active: &[Event],
        table: &BTreeMap<String, AlarmResponse>,
    ) -> Vec<Effect> {
        seconds
            .flat_map(|s| manager.update(SECOND * s, 1_000_000 * u64::from(s), active, table))
            .collect()
    }

    #[test]
    fn manager_powers_off_when_the_default_delay_ends() {
        let mut manager = AlarmManager::default();
        let table = BTreeMap::new();
        let effects = run(&mut manager, 0..30, &pump_stop(), &table);
        assert_eq!(calls(&effects), 0);
        let Effect::Raised(row) = &effects[0] else {
            panic!("first effect is the Raised effect")
        };
        assert_eq!((row.response, row.poweroff_at_us), ("poweroff", 30_000_000));
        let effects = manager.update(SECOND * 30, 30_000_000, &pump_stop(), &table);
        assert_eq!(effects, [Effect::CallPoweroff("alarm_1".into())]);
        // The call is under way: no second call.
        assert_eq!(
            calls(&manager.update(SECOND * 31, 0, &pump_stop(), &table)),
            0
        );
    }

    #[test]
    fn manager_notify_and_none_never_call() {
        for response in ["notify", "none"] {
            let mut manager = AlarmManager::default();
            let table = BTreeMap::from([(
                "pump-stop".to_owned(),
                AlarmResponse {
                    response: response.into(),
                    delay_s: 1,
                },
            )]);
            let effects = run(&mut manager, 0..100, &pump_stop(), &table);
            assert_eq!(calls(&effects), 0, "{response}");
            assert_eq!(manager.rows()[0].poweroff_at_us, 0);
        }
    }

    #[test]
    fn manager_retries_a_failed_call_every_5s() {
        let mut manager = AlarmManager::default();
        let table = BTreeMap::from([(
            "pump-stop".to_owned(),
            AlarmResponse {
                response: "poweroff".into(),
                delay_s: 2,
            },
        )]);
        let effects = run(&mut manager, 0..3, &pump_stop(), &table);
        assert_eq!(calls(&effects), 1);
        let failed = manager.poweroff_result("alarm_1", Err("blocked".into()));
        assert_eq!(
            failed,
            [Effect::PoweroffFailed {
                instance: "alarm_1".into(),
                error: "blocked".into()
            }]
        );
        // Failed at t = 2: the next call is at t = 7, then at t = 12.
        assert_eq!(calls(&run(&mut manager, 3..7, &pump_stop(), &table)), 0);
        assert_eq!(
            calls(&manager.update(SECOND * 7, 0, &pump_stop(), &table)),
            1
        );
        manager.poweroff_result("alarm_1", Err("blocked".into()));
        assert_eq!(calls(&run(&mut manager, 8..12, &pump_stop(), &table)), 0);
        assert_eq!(
            calls(&manager.update(SECOND * 12, 0, &pump_stop(), &table)),
            1
        );
        // A call that logind accepts ends the retries.
        assert!(manager.poweroff_result("alarm_1", Ok(())).is_empty());
        assert_eq!(calls(&run(&mut manager, 13..60, &pump_stop(), &table)), 0);
    }

    #[test]
    fn manager_cancel_stops_the_countdown_and_the_retries() {
        let mut manager = AlarmManager::default();
        let table = BTreeMap::new();
        run(&mut manager, 0..10, &pump_stop(), &table);
        assert_eq!(
            manager.cancel("alarm_1"),
            Some(Effect::PoweroffCancelled("alarm_1".into()))
        );
        assert_eq!(manager.cancel("alarm_1"), None);
        assert_eq!(manager.rows()[0].poweroff_at_us, 0);
        assert_eq!(calls(&run(&mut manager, 10..100, &pump_stop(), &table)), 0);
        // The Alarm clears and rises again: a new instance with a full countdown.
        let cleared = manager.update(SECOND * 100, 0, &[], &table);
        assert_eq!(cleared, [Effect::Cleared("alarm_1".into())]);
        let raised = manager.update(SECOND * 101, 0, &pump_stop(), &table);
        assert!(matches!(&raised[0], Effect::Raised(row) if row.instance == "alarm_2"));
        assert_eq!(calls(&run(&mut manager, 102..131, &pump_stop(), &table)), 0);
        assert_eq!(
            calls(&manager.update(SECOND * 131, 0, &pump_stop(), &table)),
            1
        );
    }

    #[test]
    fn manager_cancel_of_an_unknown_or_calm_instance_gives_none() {
        let mut manager = AlarmManager::default();
        assert_eq!(manager.cancel("alarm_9"), None);
        let table = BTreeMap::new();
        manager.update(
            Duration::ZERO,
            0,
            &[event("lost-input", Severity::Warning)],
            &table,
        );
        assert_eq!(manager.cancel("alarm_1"), None);
    }

    #[test]
    fn manager_a_cleared_alarm_ends_its_countdown() {
        let mut manager = AlarmManager::default();
        let table = BTreeMap::new();
        run(&mut manager, 0..20, &pump_stop(), &table);
        let effects = run(&mut manager, 20..100, &[], &table);
        assert_eq!(effects, [Effect::Cleared("alarm_1".into())]);
    }

    #[test]
    fn manager_a_new_manager_restarts_the_countdown() {
        let table = BTreeMap::new();
        let mut first = AlarmManager::default();
        run(&mut first, 0..29, &pump_stop(), &table);
        // The daemon restarts at t = 29. The Alarm is still active.
        let mut second = AlarmManager::default();
        assert_eq!(
            calls(&second.update(SECOND * 29, 29_000_000, &pump_stop(), &table)),
            0
        );
        assert_eq!(calls(&run(&mut second, 30..59, &pump_stop(), &table)), 0);
        assert_eq!(
            calls(&second.update(SECOND * 59, 0, &pump_stop(), &table)),
            1
        );
    }
}
