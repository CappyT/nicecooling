//! The Response pause: three poweroff or reboot runs in a row stop the Graph responses.
//!
//! A run counts when it starts, so the pause begins at the third run and not after its
//! countdown. A run that is cancelled, or that stops with the app before the power call, gives its
//! count back with `cancel_run`.

use serde::{Deserialize, Serialize};

use crate::model::ResponseKind;

/// The count of runs at which the pause starts.
pub const PAUSE_AT: u32 = 3;
/// The app run time without a counted run after which the count goes back to 0, in seconds.
pub const QUIET_LIMIT_S: u64 = 60 * 60;

/// The power kinds of a response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerKind {
    Poweroff,
    Reboot,
    Suspend,
}

impl PowerKind {
    pub fn from_response(kind: ResponseKind) -> Option<Self> {
        match kind {
            ResponseKind::Poweroff => Some(Self::Poweroff),
            ResponseKind::Reboot => Some(Self::Reboot),
            ResponseKind::Suspend => Some(Self::Suspend),
            _ => None,
        }
    }

    /// A poweroff and a reboot count for the pause. A suspend does not: the PC comes back with
    /// its session.
    pub const fn counts(self) -> bool {
        matches!(self, Self::Poweroff | Self::Reboot)
    }
}

/// The count, the quiet time and the pause, as `graph-state.json` holds them under `pause`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pause {
    /// The counted runs since the count was 0.
    pub count: u32,
    /// The seconds of app run time since the last counted run.
    pub quiet_s: u64,
    pub paused: bool,
    /// The count and the quiet time from before the last counted run started. Not stored: after
    /// a restart a cancel only gives the count back.
    #[serde(skip)]
    before_run: Option<(u32, u64)>,
}

impl Pause {
    /// A counted run starts, "Run once" included. It returns true while the Response pause is on.
    pub fn start_run(&mut self, kind: PowerKind) -> bool {
        if kind.counts() {
            self.before_run = Some((self.count, self.quiet_s));
            self.count = self.count.saturating_add(1);
            self.quiet_s = 0;
            if self.count >= PAUSE_AT {
                self.paused = true;
            }
        }
        self.paused
    }

    /// A run that was counted when it started does not happen: the user cancelled it, or the app
    /// stopped before the power call. The count and the quiet time go back to their values from
    /// before the run, and so does a pause that this run started.
    pub fn cancel_run(&mut self, kind: PowerKind) {
        if kind.counts() {
            match self.before_run.take() {
                Some((count, quiet_s)) => {
                    self.count = count;
                    self.quiet_s = quiet_s;
                }
                None => self.count = self.count.saturating_sub(1),
            }
            self.paused = self.paused && self.count >= PAUSE_AT;
        }
    }

    /// The app ran for `seconds` more. After 60 minutes without a counted run the count is 0. The
    /// pause does not end by itself: only `turn_on` ends it.
    pub fn advance(&mut self, seconds: u64) {
        self.quiet_s = self.quiet_s.saturating_add(seconds);
        if self.quiet_s >= QUIET_LIMIT_S && !self.paused {
            self.count = 0;
        }
    }

    /// The button "Turn the Graph responses on".
    pub fn turn_on(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn third_reboot_enters_pause() {
        let mut pause = Pause::default();
        assert!(!pause.start_run(PowerKind::Reboot));
        // Less than 60 minutes of app run time between the runs.
        pause.advance(30 * 60);
        assert!(!pause.start_run(PowerKind::Reboot));
        pause.advance(30 * 60);
        assert_eq!(pause.count, 2);
        assert!(pause.start_run(PowerKind::Reboot));
        assert!(pause.paused);
        assert_eq!(pause.count, 3);
        // A mix of poweroff and reboot counts the same.
        let mut mixed = Pause::default();
        mixed.start_run(PowerKind::Poweroff);
        mixed.start_run(PowerKind::Reboot);
        assert!(mixed.start_run(PowerKind::Poweroff));
    }

    #[test]
    fn count_resets_after_60_minutes() {
        let mut pause = Pause::default();
        pause.start_run(PowerKind::Poweroff);
        pause.advance(3599);
        assert_eq!(pause.count, 1);
        pause.advance(1);
        assert_eq!(pause.count, 0);
        // Each counted run starts the 60 minutes again.
        pause.start_run(PowerKind::Poweroff);
        pause.advance(3000);
        pause.start_run(PowerKind::Poweroff);
        pause.advance(3000);
        assert_eq!(pause.count, 2);
        // The pause does not end by itself.
        pause.start_run(PowerKind::Reboot);
        pause.advance(10 * 3600);
        assert!(pause.paused);
        assert_eq!(pause.count, 3);
    }

    #[test]
    fn run_time_adds_up_over_restarts() {
        // The app runs 40 minutes, restarts and runs 20 minutes.
        let mut pause = Pause::default();
        pause.start_run(PowerKind::Poweroff);
        pause.advance(40 * 60);
        let stored = serde_json::to_string(&pause).unwrap();
        let mut restarted: Pause = serde_json::from_str(&stored).unwrap();
        assert_eq!(restarted.count, 1);
        restarted.advance(20 * 60);
        assert_eq!(restarted.count, 0);
    }

    #[test]
    fn cancelled_run_does_not_count() {
        let mut pause = Pause::default();
        pause.start_run(PowerKind::Poweroff);
        assert_eq!(pause.count, 1);
        // A second run starts and the user cancels it.
        pause.start_run(PowerKind::Poweroff);
        pause.cancel_run(PowerKind::Poweroff);
        assert_eq!(pause.count, 1);
        // A "Run once" in the pause that the user cancels leaves the pause on.
        let mut paused = Pause {
            count: 3,
            quiet_s: 0,
            paused: true,
            ..Pause::default()
        };
        paused.start_run(PowerKind::Reboot);
        paused.cancel_run(PowerKind::Reboot);
        assert!(paused.paused);
        assert_eq!(paused.count, 3);
        // The cancel of the third run takes away the pause that this run started.
        let mut third = Pause::default();
        third.start_run(PowerKind::Poweroff);
        third.start_run(PowerKind::Poweroff);
        assert!(third.start_run(PowerKind::Poweroff));
        third.cancel_run(PowerKind::Poweroff);
        assert!(!third.paused);
        assert_eq!(third.count, 2);
        // A cancel with a count of 0 does not underflow.
        let mut empty = Pause::default();
        empty.cancel_run(PowerKind::Poweroff);
        assert_eq!(empty.count, 0);
    }

    #[test]
    fn cancelled_run_restores_count_and_quiet_time() {
        let mut pause = Pause::default();
        pause.start_run(PowerKind::Poweroff);
        pause.advance(50 * 60);
        pause.start_run(PowerKind::Reboot);
        pause.cancel_run(PowerKind::Reboot);
        assert_eq!((pause.count, pause.quiet_s), (1, 50 * 60));
        // The cancel of a run that started the pause gives back the quiet time as well.
        let mut third = Pause {
            count: 2,
            quiet_s: 600,
            ..Pause::default()
        };
        assert!(third.start_run(PowerKind::Poweroff));
        third.cancel_run(PowerKind::Poweroff);
        assert_eq!((third.count, third.quiet_s, third.paused), (2, 600, false));
        // After a restart the snapshot is gone: the cancel gives back the count only.
        let mut restarted: Pause =
            serde_json::from_str(r#"{"count":2,"quiet_s":0,"paused":false}"#).unwrap();
        restarted.cancel_run(PowerKind::Poweroff);
        assert_eq!(restarted.count, 1);
    }

    #[test]
    fn suspend_does_not_count() {
        let mut pause = Pause::default();
        for _ in 0..5 {
            assert!(!pause.start_run(PowerKind::Suspend));
        }
        assert_eq!(pause, Pause::default());
        pause.advance(100);
        pause.cancel_run(PowerKind::Suspend);
        assert_eq!(pause.count, 0);
        assert_eq!(pause.quiet_s, 100);
        assert_eq!(
            PowerKind::from_response(ResponseKind::Suspend),
            Some(PowerKind::Suspend)
        );
        assert_eq!(PowerKind::from_response(ResponseKind::Command), None);
    }

    #[test]
    fn run_once_counts() {
        // The count is 2 and the user presses "Run once" on a reboot response: 3 and the pause.
        let mut pause = Pause {
            count: 2,
            quiet_s: 100,
            paused: false,
            ..Pause::default()
        };
        assert!(pause.start_run(PowerKind::Reboot));
        assert_eq!(pause.count, 3);
        assert!(pause.paused);
        assert_eq!(pause.quiet_s, 0);
        // In the pause a run once still counts.
        pause.start_run(PowerKind::Poweroff);
        assert_eq!(pause.count, 4);
    }

    #[test]
    fn pause_survives_json_roundtrip() {
        let pause = Pause {
            count: 3,
            quiet_s: 120,
            paused: true,
            ..Pause::default()
        };
        let text = serde_json::to_string(&pause).unwrap();
        assert_eq!(text, r#"{"count":3,"quiet_s":120,"paused":true}"#);
        assert_eq!(serde_json::from_str::<Pause>(&text).unwrap(), pause);
        // After the restart the pause is on and the count stays over any run time.
        let mut back: Pause = serde_json::from_str(&text).unwrap();
        back.advance(5 * 3600);
        assert!(back.paused);
        assert_eq!(back.count, 3);
    }

    #[test]
    fn turn_on_clears_pause() {
        let mut pause = Pause::default();
        for _ in 0..3 {
            pause.start_run(PowerKind::Poweroff);
        }
        assert!(pause.paused);
        pause.turn_on();
        assert_eq!(pause, Pause::default());
        // The next run starts a new count.
        assert!(!pause.start_run(PowerKind::Reboot));
        assert_eq!(pause.count, 1);
    }
}
