//! The sleep of the PC on a private bus: a mock logind sends `PrepareForSleep` and hands out the
//! lock of the delay inhibitor.

mod support;

use std::time::Duration;

use nicecooling_protocol::report2::Report2;
use nicecoolingd::sleep;
use support::{Rig, without_clock};

/// `suppress_alarm`, report 2 command 16, with 90000 / 25.
const SUPPRESS_ALARM: Report2 = Report2 {
    command: 16,
    parameter: 3600,
};

async fn until(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..200 {
        if done() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("timed out: {what}");
}

async fn watched() -> (Rig, tokio::task::JoinHandle<()>) {
    let rig = Rig::start().await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let task = sleep::spawn(rig.server.clone(), hub);
    until("the daemon takes the lock", || rig.logind.open_locks() == 1).await;
    (rig, task)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn prepare_for_sleep_suppresses_the_alarms_before_the_lock_closes() {
    let (rig, task) = watched().await;
    assert!(!rig.sim.commands().contains(&SUPPRESS_ALARM));

    rig.logind.prepare_for_sleep(true).await;
    until("the lock closes", || rig.logind.open_locks() == 0).await;

    // The command went out before the sleep was released, and the daemon sent no other.
    assert_eq!(
        without_clock(rig.sim.commands()),
        [Report2::new(
            SUPPRESS_ALARM.command,
            SUPPRESS_ALARM.parameter
        )]
    );

    // The PC wakes up: the daemon takes a new lock for the next sleep.
    rig.logind.prepare_for_sleep(false).await;
    until("the new lock", || rig.logind.open_locks() == 1).await;
    task.abort();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_leakshield_with_unknown_firmware_gets_no_command_and_the_lock_still_closes() {
    let (rig, task) = watched().await;
    rig.sim.edit_report1(|r| r.firmware_version = 1019);
    // The next report 1 tells the daemon that the device is read-only.
    tokio::time::sleep(Duration::from_millis(300)).await;

    rig.logind.prepare_for_sleep(true).await;
    until("the lock closes", || rig.logind.open_locks() == 0).await;

    assert!(without_clock(rig.sim.commands()).is_empty());
    task.abort();
}
