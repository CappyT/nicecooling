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
    /// The bits of the device report that set the event, 0 for an event without bits.
    pub bits: u32,
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
    /// The bits of the device report that set the event now, 0 for an event without bits.
    pub bits: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Raised(Row),
    /// The details of an active instance changed.
    Updated(String),
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

/// Why `cancel` does nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CancelError {
    /// The instance is unknown or has no pending poweroff.
    NoPoweroff,
    /// The call to logind is under way or logind accepted it: it is too late to cancel.
    PoweroffStarted,
}

#[derive(Debug)]
pub struct AlarmManager {
    instances: Vec<Instance>,
    next: u64,
    now: Duration,
    /// The start time of the manager. It is part of each instance id, so an id never repeats
    /// after a restart of the daemon.
    started_us: u64,
}

fn severity_name(severity: Severity) -> &'static str {
    severity.as_str()
}

impl AlarmManager {
    /// `started_us` is the wall clock at the start of the daemon, in microseconds.
    pub fn new(started_us: u64) -> Self {
        Self {
            instances: Vec::new(),
            next: 0,
            now: Duration::ZERO,
            started_us,
        }
    }

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
            if let Some(found) = self
                .instances
                .iter_mut()
                .find(|i| i.row.event == event.event && i.row.source == event.source)
            {
                if found.row.bits != event.bits {
                    found.row.bits = event.bits;
                    effects.push(Effect::Updated(found.row.instance.clone()));
                }
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
                instance: format!("alarm_{}_{}", self.started_us, self.next),
                event: event.event.clone(),
                severity: severity_name(event.severity),
                source: event.source.clone(),
                response: response.as_str(),
                raised_us: wall_us,
                poweroff_at_us,
                bits: event.bits,
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
    /// off again as this instance.
    pub fn cancel(&mut self, instance: &str) -> Result<Effect, CancelError> {
        let found = self
            .instances
            .iter_mut()
            .find(|i| i.row.instance == instance)
            .ok_or(CancelError::NoPoweroff)?;
        match found.countdown {
            Countdown::Calling | Countdown::Done => return Err(CancelError::PoweroffStarted),
            Countdown::Off => return Err(CancelError::NoPoweroff),
            Countdown::Pending { .. } | Countdown::Retry { .. } => {}
        }
        found.countdown = Countdown::Off;
        found.row.poweroff_at_us = 0;
        Ok(Effect::PoweroffCancelled(instance.to_owned()))
    }

    pub fn rows(&self) -> Vec<Row> {
        self.instances.iter().map(|i| i.row.clone()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);
    const STARTED_US: u64 = 1_700_000_000_000_000;

    fn id(n: u64) -> String {
        format!("alarm_{STARTED_US}_{n}")
    }

    fn event(name: &str, severity: Severity) -> Event {
        Event {
            event: name.into(),
            source: "loop_1".into(),
            severity,
            bits: 0,
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
        let mut manager = AlarmManager::new(STARTED_US);
        let table = BTreeMap::new();
        let effects = run(&mut manager, 0..30, &pump_stop(), &table);
        assert_eq!(calls(&effects), 0);
        let Effect::Raised(row) = &effects[0] else {
            panic!("first effect is the Raised effect")
        };
        assert_eq!((row.response, row.poweroff_at_us), ("poweroff", 30_000_000));
        let effects = manager.update(SECOND * 30, 30_000_000, &pump_stop(), &table);
        assert_eq!(effects, [Effect::CallPoweroff(id(1))]);
        // The call is under way: no second call.
        assert_eq!(
            calls(&manager.update(SECOND * 31, 0, &pump_stop(), &table)),
            0
        );
    }

    #[test]
    fn manager_notify_and_none_never_call() {
        for response in ["notify", "none"] {
            let mut manager = AlarmManager::new(STARTED_US);
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
        let mut manager = AlarmManager::new(STARTED_US);
        let table = BTreeMap::from([(
            "pump-stop".to_owned(),
            AlarmResponse {
                response: "poweroff".into(),
                delay_s: 2,
            },
        )]);
        let effects = run(&mut manager, 0..3, &pump_stop(), &table);
        assert_eq!(calls(&effects), 1);
        let failed = manager.poweroff_result(&id(1), Err("blocked".into()));
        assert_eq!(
            failed,
            [Effect::PoweroffFailed {
                instance: id(1),
                error: "blocked".into()
            }]
        );
        // Failed at t = 2: the next call is at t = 7, then at t = 12.
        assert_eq!(calls(&run(&mut manager, 3..7, &pump_stop(), &table)), 0);
        assert_eq!(
            calls(&manager.update(SECOND * 7, 0, &pump_stop(), &table)),
            1
        );
        manager.poweroff_result(&id(1), Err("blocked".into()));
        assert_eq!(calls(&run(&mut manager, 8..12, &pump_stop(), &table)), 0);
        assert_eq!(
            calls(&manager.update(SECOND * 12, 0, &pump_stop(), &table)),
            1
        );
        // A call that logind accepts ends the retries.
        assert!(manager.poweroff_result(&id(1), Ok(())).is_empty());
        assert_eq!(calls(&run(&mut manager, 13..60, &pump_stop(), &table)), 0);
    }

    #[test]
    fn manager_cancel_stops_the_countdown_and_the_retries() {
        let mut manager = AlarmManager::new(STARTED_US);
        let table = BTreeMap::new();
        run(&mut manager, 0..10, &pump_stop(), &table);
        assert_eq!(manager.cancel(&id(1)), Ok(Effect::PoweroffCancelled(id(1))));
        assert_eq!(manager.cancel(&id(1)), Err(CancelError::NoPoweroff));
        assert_eq!(manager.rows()[0].poweroff_at_us, 0);
        assert_eq!(calls(&run(&mut manager, 10..100, &pump_stop(), &table)), 0);
        // The Alarm clears and rises again: a new instance with a full countdown.
        let cleared = manager.update(SECOND * 100, 0, &[], &table);
        assert_eq!(cleared, [Effect::Cleared(id(1))]);
        let raised = manager.update(SECOND * 101, 0, &pump_stop(), &table);
        assert!(matches!(&raised[0], Effect::Raised(row) if row.instance == id(2)));
        assert_eq!(calls(&run(&mut manager, 102..131, &pump_stop(), &table)), 0);
        assert_eq!(
            calls(&manager.update(SECOND * 131, 0, &pump_stop(), &table)),
            1
        );
    }

    #[test]
    fn manager_cancel_of_an_unknown_or_calm_instance_gives_none() {
        let mut manager = AlarmManager::new(STARTED_US);
        assert_eq!(manager.cancel(&id(9)), Err(CancelError::NoPoweroff));
        let table = BTreeMap::new();
        manager.update(
            Duration::ZERO,
            0,
            &[event("lost-input", Severity::Warning)],
            &table,
        );
        assert_eq!(manager.cancel(&id(1)), Err(CancelError::NoPoweroff));
    }

    #[test]
    fn manager_a_cleared_alarm_ends_its_countdown() {
        let mut manager = AlarmManager::new(STARTED_US);
        let table = BTreeMap::new();
        run(&mut manager, 0..20, &pump_stop(), &table);
        let effects = run(&mut manager, 20..100, &[], &table);
        assert_eq!(effects, [Effect::Cleared(id(1))]);
    }

    #[test]
    fn manager_follows_the_bits_of_an_active_event() {
        let mut manager = AlarmManager::new(STARTED_US);
        let table = BTreeMap::new();
        let leak = |bits| {
            vec![Event {
                bits,
                ..event("leakshield.leak-alarm", Severity::Alarm)
            }]
        };
        let raised = manager.update(Duration::ZERO, 0, &leak(1 << 5), &table);
        assert!(matches!(&raised[0], Effect::Raised(row) if row.bits == 1 << 5));
        assert!(manager.update(SECOND, 0, &leak(1 << 5), &table).is_empty());
        // The second bit changes the details of the instance and starts no new one.
        let effects = manager.update(SECOND * 2, 0, &leak((1 << 5) | (1 << 6)), &table);
        assert_eq!(effects, [Effect::Updated(id(1))]);
        assert_eq!(manager.rows()[0].bits, (1 << 5) | (1 << 6));
        assert_eq!(manager.rows().len(), 1);
    }

    #[test]
    fn manager_a_new_manager_restarts_the_countdown() {
        let table = BTreeMap::new();
        let mut first = AlarmManager::new(STARTED_US);
        run(&mut first, 0..29, &pump_stop(), &table);
        // The daemon restarts at t = 29. The Alarm is still active.
        let mut second = AlarmManager::new(STARTED_US);
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

    #[test]
    fn manager_cancel_while_the_call_is_under_way_is_too_late() {
        let mut manager = AlarmManager::new(STARTED_US);
        let table = BTreeMap::new();
        let effects = run(&mut manager, 0..31, &pump_stop(), &table);
        assert_eq!(calls(&effects), 1);
        assert_eq!(manager.cancel(&id(1)), Err(CancelError::PoweroffStarted));
        assert_ne!(manager.rows()[0].poweroff_at_us, 0);
        // The call fails, and the retry wait accepts a cancel again.
        manager.poweroff_result(&id(1), Err("blocked".into()));
        assert_eq!(manager.cancel(&id(1)), Ok(Effect::PoweroffCancelled(id(1))));
    }

    #[test]
    fn manager_cancel_after_logind_accepted_is_too_late() {
        let mut manager = AlarmManager::new(STARTED_US);
        let table = BTreeMap::new();
        run(&mut manager, 0..31, &pump_stop(), &table);
        manager.poweroff_result(&id(1), Ok(()));
        assert_eq!(manager.cancel(&id(1)), Err(CancelError::PoweroffStarted));
    }

    #[test]
    fn manager_instance_ids_differ_between_starts() {
        let table = BTreeMap::new();
        let first = AlarmManager::new(STARTED_US);
        let mut first = first;
        let mut second = AlarmManager::new(STARTED_US + 1);
        first.update(Duration::ZERO, 0, &pump_stop(), &table);
        second.update(Duration::ZERO, 0, &pump_stop(), &table);
        assert_ne!(first.rows()[0].instance, second.rows()[0].instance);
        assert!(first.rows()[0].instance.starts_with("alarm_"));
    }
}
