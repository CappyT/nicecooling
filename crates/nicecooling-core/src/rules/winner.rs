//! The Rule delay timers, the Profile dwell and the choice of the active Profile (design D14).

use std::collections::BTreeMap;
use std::time::Duration;

use crate::profile::Cause;

/// The default Rule delay, in seconds.
pub const DEFAULT_RULE_DELAY_S: u32 = 30;
/// The default Profile dwell, in seconds.
pub const DEFAULT_DWELL_S: u32 = 300;

/// The timing settings. They apply to every group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    pub rule_delay: Duration,
    pub dwell: Duration,
    /// Removes the Rule delay and the Profile dwell.
    pub immediate: bool,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            rule_delay: Duration::from_secs(u64::from(DEFAULT_RULE_DELAY_S)),
            dwell: Duration::from_secs(u64::from(DEFAULT_DWELL_S)),
            immediate: false,
        }
    }
}

/// A group as the selection sees it, in list order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupView {
    pub id: String,
    pub profile: String,
    pub holds: bool,
}

/// One timer for each group.
#[derive(Debug, Default)]
pub struct DelayTimers {
    since: BTreeMap<String, Duration>,
    /// The group that `winner` chose last.
    winning: Option<String>,
}

impl DelayTimers {
    /// The winner is the first group in list order that has held for the Rule delay, or that
    /// holds with Immediate switching. A group that does not hold loses its timer.
    pub fn winner(
        &mut self,
        now: Duration,
        groups: &[GroupView],
        timing: &Timing,
    ) -> Option<usize> {
        self.since
            .retain(|id, _| groups.iter().any(|g| &g.id == id));
        for group in groups {
            if group.holds {
                self.since.entry(group.id.clone()).or_insert(now);
            } else {
                self.since.remove(&group.id);
            }
        }
        // A list with two groups of one id can reach this point. A group without a timer waits.
        let index = groups.iter().position(|g| {
            g.holds
                && (timing.immediate
                    || self
                        .since
                        .get(&g.id)
                        .is_some_and(|since| now.saturating_sub(*since) >= timing.rule_delay))
        });
        self.winning = index.map(|i| groups[i].id.clone());
        index
    }

    /// Forgets a group that changed: its Rule delay starts again. The winning group keeps its
    /// timer. It loses the timer in the tick where it stops holding.
    pub fn reset(&mut self, id: &str) {
        if self.winning.as_deref() != Some(id) {
            self.since.remove(id);
        }
    }
}

/// A switch of the active Profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Change {
    pub profile: String,
    pub cause: Cause,
}

/// Decides which Profile is active. A Profile that a group activated stays for the Profile
/// dwell. A manual activation wins at once and stays until `return_to_automatic`.
#[derive(Debug)]
pub struct Selector {
    default_profile: String,
    active: String,
    cause: Cause,
    /// When the active Profile became active.
    since: Duration,
    manual: bool,
    timers: DelayTimers,
}

impl Selector {
    /// `active` and `cause` come from the state of the daemon. The dwell of a Profile that a
    /// group activated before a restart starts again.
    pub fn new(default_profile: &str, active: &str, cause: Cause, now: Duration) -> Self {
        Self {
            default_profile: default_profile.to_owned(),
            active: active.to_owned(),
            cause,
            since: now,
            manual: cause == Cause::Manual,
            timers: DelayTimers::default(),
        }
    }

    pub fn active(&self) -> &str {
        &self.active
    }

    pub fn cause(&self) -> Cause {
        self.cause
    }

    pub fn timers_mut(&mut self) -> &mut DelayTimers {
        &mut self.timers
    }

    /// The user activated a Profile by hand. It is active at once, also during the dwell.
    pub fn manual(&mut self, profile: &str, now: Duration) {
        self.active = profile.to_owned();
        self.cause = Cause::Manual;
        self.manual = true;
        self.since = now;
    }

    /// The Rule groups decide again at the next `update`.
    pub fn return_to_automatic(&mut self) {
        self.manual = false;
    }

    /// One tick. Returns a change when the active Profile or its cause changes.
    pub fn update(
        &mut self,
        now: Duration,
        groups: &[GroupView],
        timing: &Timing,
    ) -> Option<Change> {
        let winner = self.timers.winner(now, groups, timing);
        if self.manual {
            return None;
        }
        let (profile, cause) = match winner {
            Some(index) => (groups[index].profile.as_str(), Cause::Rule),
            None => (self.default_profile.as_str(), Cause::Default),
        };
        if profile == self.active {
            if cause == self.cause {
                return None;
            }
            self.cause = cause;
            return Some(Change {
                profile: profile.to_owned(),
                cause,
            });
        }
        let in_dwell = self.cause == Cause::Rule
            && !timing.immediate
            && now.saturating_sub(self.since) < timing.dwell;
        if in_dwell {
            return None;
        }
        self.active = profile.to_owned();
        self.cause = cause;
        self.since = now;
        Some(Change {
            profile: profile.to_owned(),
            cause,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    fn view(id: &str, profile: &str, holds: bool) -> GroupView {
        GroupView {
            id: id.into(),
            profile: profile.into(),
            holds,
        }
    }

    fn timing() -> Timing {
        Timing::default()
    }

    /// The first tick of a selector that starts on the Default Profile.
    fn selector() -> Selector {
        Selector::new(
            "profile_default",
            "profile_default",
            Cause::Default,
            secs(0),
        )
    }

    #[test]
    fn rule_timing_defaults_30_and_300() {
        let timing = Timing::default();
        assert_eq!(timing.rule_delay, secs(30));
        assert_eq!(timing.dwell, secs(300));
        assert!(!timing.immediate);
    }

    #[test]
    fn delay_30s() {
        let mut timers = DelayTimers::default();
        let groups = [view("group_1", "profile_2", true)];
        for t in 0..30 {
            assert_eq!(timers.winner(secs(t), &groups, &timing()), None, "t={t}");
        }
        assert_eq!(timers.winner(secs(30), &groups, &timing()), Some(0));
        // A Rule delay of 10 s changes the wait.
        let short = Timing {
            rule_delay: secs(10),
            ..timing()
        };
        let mut timers = DelayTimers::default();
        assert_eq!(timers.winner(secs(0), &groups, &short), None);
        assert_eq!(timers.winner(secs(10), &groups, &short), Some(0));
    }

    /// `state.json` can hold two groups with one id. The winner must not panic the tick.
    #[test]
    fn duplicate_group_ids_do_not_panic_the_winner() {
        let groups = [
            view("group_1", "profile_2", true),
            view("group_1", "profile_3", false),
        ];
        let mut timers = DelayTimers::default();
        for t in 0..40 {
            timers.winner(secs(t), &groups, &timing());
        }
    }

    #[test]
    fn group_stops_early_resets_timer() {
        let mut timers = DelayTimers::default();
        let holds = [view("group_1", "profile_2", true)];
        let stops = [view("group_1", "profile_2", false)];
        assert_eq!(timers.winner(secs(0), &holds, &timing()), None);
        assert_eq!(timers.winner(secs(10), &stops, &timing()), None);
        // 30 s after the first start the group has not held for 30 s in a row.
        assert_eq!(timers.winner(secs(11), &holds, &timing()), None);
        assert_eq!(timers.winner(secs(30), &holds, &timing()), None);
        assert_eq!(timers.winner(secs(41), &holds, &timing()), Some(0));
    }

    #[test]
    fn later_group_ready_first() {
        let mut timers = DelayTimers::default();
        // The third group holds from 0, the first from 20.
        let third_only = [
            view("group_1", "p1", false),
            view("group_2", "p2", false),
            view("group_3", "p3", true),
        ];
        let both = [
            view("group_1", "p1", true),
            view("group_2", "p2", false),
            view("group_3", "p3", true),
        ];
        assert_eq!(timers.winner(secs(0), &third_only, &timing()), None);
        assert_eq!(timers.winner(secs(20), &both, &timing()), None);
        // At 30 s the third group has held for 30 s and the first has held for 10 s.
        assert_eq!(timers.winner(secs(30), &both, &timing()), Some(2));
        // The first group wins when its own 30 s end.
        assert_eq!(timers.winner(secs(49), &both, &timing()), Some(2));
        assert_eq!(timers.winner(secs(50), &both, &timing()), Some(0));
    }

    #[test]
    fn interval_window_shorter_than_delay_never_wins() {
        let mut timers = DelayTimers::default();
        // A window of 20 s in each period of 100 s, against a Rule delay of 30 s.
        for period in 0..5u64 {
            for t in 0..100 {
                let holds = t < 20;
                let groups = [view("group_1", "p1", holds)];
                assert_eq!(
                    timers.winner(secs(period * 100 + t), &groups, &timing()),
                    None
                );
            }
        }
    }

    #[test]
    fn no_group_holds_gives_default_profile() {
        let mut selector = selector();
        for t in 0..100 {
            assert_eq!(selector.update(secs(t), &[], &timing()), None);
            assert_eq!(selector.active(), "profile_default");
        }
        let idle = [view("group_1", "profile_2", false)];
        assert_eq!(selector.update(secs(100), &idle, &timing()), None);
        assert_eq!(selector.cause(), Cause::Default);
    }

    #[test]
    fn dwell_5min() {
        let mut selector = selector();
        let first = [
            view("group_1", "profile_2", true),
            view("group_2", "profile_3", false),
        ];
        let second = [
            view("group_1", "profile_2", false),
            view("group_2", "profile_3", true),
        ];
        // Group 1 wins after the Rule delay and activates profile_2.
        assert_eq!(selector.update(secs(0), &first, &timing()), None);
        let change = selector.update(secs(30), &first, &timing()).unwrap();
        assert_eq!(change.profile, "profile_2");
        assert_eq!(change.cause, Cause::Rule);
        // Group 2 holds for 30 s from second 100, but the dwell runs until second 330.
        assert_eq!(selector.update(secs(100), &second, &timing()), None);
        assert_eq!(selector.update(secs(130), &second, &timing()), None);
        assert_eq!(selector.update(secs(329), &second, &timing()), None);
        assert_eq!(selector.active(), "profile_2");
        // The dwell ends: group 2 becomes active.
        let change = selector.update(secs(330), &second, &timing()).unwrap();
        assert_eq!(change.profile, "profile_3");
    }

    #[test]
    fn dwell_end_activates_default_profile() {
        let mut selector = selector();
        let holds = [view("group_1", "profile_2", true)];
        let stops = [view("group_1", "profile_2", false)];
        assert_eq!(selector.update(secs(0), &holds, &timing()), None);
        selector.update(secs(30), &holds, &timing()).unwrap();
        // The group stops after 2 minutes. Its Profile stays until the dwell ends.
        assert_eq!(selector.update(secs(150), &stops, &timing()), None);
        assert_eq!(selector.update(secs(329), &stops, &timing()), None);
        let change = selector.update(secs(330), &stops, &timing()).unwrap();
        assert_eq!(
            change,
            Change {
                profile: "profile_default".into(),
                cause: Cause::Default
            }
        );
        // A dwell of 1 minute ends sooner.
        let short = Timing {
            dwell: secs(60),
            ..timing()
        };
        let mut selector = Selector::new(
            "profile_default",
            "profile_default",
            Cause::Default,
            secs(0),
        );
        assert_eq!(selector.update(secs(0), &holds, &short), None);
        selector.update(secs(30), &holds, &short).unwrap();
        assert_eq!(selector.update(secs(80), &stops, &short), None);
        assert!(selector.update(secs(90), &stops, &short).is_some());
    }

    #[test]
    fn immediate_switching_same_tick() {
        let immediate = Timing {
            immediate: true,
            ..timing()
        };
        let mut selector = selector();
        let holds = [view("group_1", "profile_2", true)];
        // The group activates its Profile in the tick where it starts to hold.
        let change = selector.update(secs(5), &holds, &immediate).unwrap();
        assert_eq!(change.profile, "profile_2");
        // No dwell: the next group changes the Profile in the tick where it asks.
        let next = [
            view("group_1", "profile_2", false),
            view("group_2", "profile_3", true),
        ];
        let change = selector.update(secs(6), &next, &immediate).unwrap();
        assert_eq!(change.profile, "profile_3");
        let none = [view("group_2", "profile_3", false)];
        let change = selector.update(secs(7), &none, &immediate).unwrap();
        assert_eq!(change.cause, Cause::Default);
    }

    #[test]
    fn interval_window_wins_with_immediate_switching() {
        let immediate = Timing {
            immediate: true,
            ..timing()
        };
        let mut selector = selector();
        // A window of 20 s is shorter than the Rule delay of 30 s.
        let open = [view("group_1", "profile_2", true)];
        let closed = [view("group_1", "profile_2", false)];
        assert!(selector.update(secs(80), &open, &immediate).is_some());
        assert_eq!(selector.active(), "profile_2");
        assert!(selector.update(secs(100), &closed, &immediate).is_some());
        assert_eq!(selector.active(), "profile_default");
        // Without Immediate switching the same window never wins.
        let mut selector = self::selector();
        for t in 80..100 {
            assert_eq!(selector.update(secs(t), &open, &timing()), None);
        }
        assert_eq!(selector.update(secs(100), &closed, &timing()), None);
        assert_eq!(selector.active(), "profile_default");
    }

    #[test]
    fn manual_wins_at_once_during_dwell() {
        let mut selector = selector();
        let holds = [view("group_1", "profile_2", true)];
        assert_eq!(selector.update(secs(0), &holds, &timing()), None);
        selector.update(secs(30), &holds, &timing()).unwrap();
        // The user activates a Profile 2 minutes after the group activation.
        selector.manual("profile_4", secs(150));
        assert_eq!(selector.active(), "profile_4");
        assert_eq!(selector.cause(), Cause::Manual);
    }

    #[test]
    fn manual_lasts_until_return_to_automatic() {
        let mut selector = selector();
        selector.manual("profile_4", secs(0));
        let holds = [view("group_1", "profile_2", true)];
        // A group holds for a long time: the Profile does not switch.
        for t in 0..1000 {
            assert_eq!(selector.update(secs(t), &holds, &timing()), None);
        }
        assert_eq!(selector.active(), "profile_4");
        // The timers kept running, so the ready group takes over at once, with no dwell.
        selector.return_to_automatic();
        let change = selector.update(secs(1000), &holds, &timing()).unwrap();
        assert_eq!(change.profile, "profile_2");
        assert_eq!(change.cause, Cause::Rule);
        // Without a holding group the Default Profile takes over.
        let mut selector = self::selector();
        selector.manual("profile_4", secs(0));
        selector.return_to_automatic();
        let change = selector.update(secs(1), &[], &timing()).unwrap();
        assert_eq!(change.profile, "profile_default");
    }

    #[test]
    fn a_group_for_the_active_profile_changes_only_the_cause() {
        let mut selector = Selector::new("profile_default", "profile_2", Cause::Manual, secs(0));
        let holds = [view("group_1", "profile_2", true)];
        // The timer of the group runs while the manual activation lasts.
        for t in 0..=30 {
            assert_eq!(selector.update(secs(t), &holds, &timing()), None);
        }
        selector.return_to_automatic();
        // The group is ready for the Profile that is active already: only the cause changes.
        let change = selector.update(secs(31), &holds, &timing()).unwrap();
        assert_eq!(
            (change.profile.as_str(), change.cause),
            ("profile_2", Cause::Rule)
        );
    }
}
