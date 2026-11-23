//! The outcome of an Alarm history row when the call to logind fails after the instance cleared.

use std::collections::BTreeMap;
use std::time::Duration;

use nicecooling_core::alarms::Severity;
use nicecoolingd::alarms::history::{FAILED, History, REQUESTED};
use nicecoolingd::alarms::poweroff::{AlarmManager, Effect, Event};
use nicecoolingd::state::AlarmResponse;

fn apply(history: &mut History, manager: &AlarmManager, effects: &[Effect], at_us: u64) {
    let rows = manager.rows();
    for effect in effects {
        history.apply(effect, &rows, at_us);
    }
}

/// The countdown ends and the call to logind starts (`poweroff-requested`). The Alarm clears
/// while logind has not answered (the call may take 10 s). Then the call fails: the PC stays on,
/// so the row must say `poweroff-failed`, not "PC powered off".
#[test]
fn clear_during_call_then_failure_is_failed() {
    let mut manager = AlarmManager::new(1);
    let mut history = History::default();
    let table = BTreeMap::from([(
        "pump-stop".to_owned(),
        AlarmResponse {
            response: "poweroff".into(),
            delay_s: 0,
        },
    )]);
    let active = [Event {
        event: "pump-stop".into(),
        source: "loop_1".into(),
        severity: Severity::Alarm,
        bits: 0,
        latched: 0,
    }];

    let effects = manager.update(Duration::from_secs(1), 1_000_000, &active, &table);
    apply(&mut history, &manager, &effects, 1_000_000);
    assert!(effects.iter().any(|e| matches!(e, Effect::CallPoweroff(_))));
    assert_eq!(history.rows()[0].outcome, REQUESTED);

    let effects = manager.update(Duration::from_secs(2), 2_000_000, &[], &table);
    apply(&mut history, &manager, &effects, 2_000_000);

    let instance = history.rows()[0].instance.clone();
    let effects = manager.poweroff_result(&instance, Err("logind did not answer in 10 s".into()));
    apply(&mut history, &manager, &effects, 12_000_000);

    assert_eq!(history.rows()[0].outcome, FAILED);
    // The clear time stays.
    assert_eq!(history.rows()[0].cleared_us, 2_000_000);
}
