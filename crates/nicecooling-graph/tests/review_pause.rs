//! Review reproducer: a cancelled poweroff or reboot gives its count back, but it also sets the
//! quiet time to 0. The 60 minutes then count from the cancelled run.

use nicecooling_graph::pause::{Pause, PowerKind};

/// Spec "Response pause": the count goes back to 0 after 60 minutes of app run time with no
/// counted run. Scenario "Cancelled run": a cancel does not change the count.
#[test]
fn cancelled_run_keeps_the_quiet_time() {
    let mut pause = Pause::default();
    pause.start_run(PowerKind::Poweroff);
    pause.advance(50 * 60);
    // A second poweroff starts at minute 50, and the user cancels it.
    pause.start_run(PowerKind::Poweroff);
    pause.cancel_run(PowerKind::Poweroff);
    assert_eq!(pause.count, 1);
    // Minute 60: one hour since the one run that counts.
    pause.advance(10 * 60);
    assert_eq!(pause.count, 0, "{pause:?}");
}
