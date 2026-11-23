use std::collections::BTreeMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::interval::{IntervalRule, IntervalState};
use super::process::{ProcessList, ProcessRule};
use super::sensor::{SensorRule, SensorState};
use super::time::{DayTime, TimeRule};

/// A rule with its kind. In `state.json` it is `{"kind": ..., "params": {...}}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "params", rename_all = "snake_case")]
pub enum RuleDef {
    Sensor(SensorRule),
    Process(ProcessRule),
    Time(TimeRule),
    Interval(IntervalRule),
}

impl RuleDef {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Sensor(_) => "sensor",
            Self::Process(_) => "process",
            Self::Time(_) => "time",
            Self::Interval(_) => "interval",
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Sensor(rule) => rule.validate(),
            Self::Process(rule) => rule.validate(),
            Self::Time(rule) => rule.validate(),
            Self::Interval(rule) => rule.validate(),
        }
    }
}

/// An ordered list of rules with a target Profile.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuleGroupDef {
    pub id: String,
    pub profile: String,
    pub enabled: bool,
    pub rules: Vec<RuleDef>,
}

/// What the rules read in one tick.
pub struct Inputs<'a> {
    pub now: Duration,
    /// Sensor values that are not stale.
    pub values: &'a BTreeMap<String, f64>,
    pub processes: &'a ProcessList,
    pub local: DayTime,
}

#[derive(Clone, Debug)]
enum RuleState {
    Sensor(SensorState),
    Interval(IntervalState),
    Stateless,
}

/// A group with the state of its rules.
#[derive(Clone, Debug)]
pub struct Group {
    def: RuleGroupDef,
    states: Vec<RuleState>,
}

impl Group {
    /// An interval rule starts its first period at `now`.
    pub fn new(def: RuleGroupDef, now: Duration) -> Self {
        let states = def
            .rules
            .iter()
            .map(|rule| match rule {
                RuleDef::Sensor(_) => RuleState::Sensor(SensorState::default()),
                RuleDef::Interval(_) => RuleState::Interval(IntervalState::new(now)),
                RuleDef::Process(_) | RuleDef::Time(_) => RuleState::Stateless,
            })
            .collect();
        Self { def, states }
    }

    pub fn def(&self) -> &RuleGroupDef {
        &self.def
    }

    /// A group holds when it is enabled, has a rule and all its rules hold. Every rule is
    /// evaluated, so that a hold time runs while another rule does not hold.
    pub fn holds(&mut self, inputs: &Inputs<'_>) -> bool {
        if !self.def.enabled {
            // A rule that comes back starts its hold time again.
            self.reset(inputs.now);
            return false;
        }
        let mut all = !self.def.rules.is_empty();
        for (rule, state) in self.def.rules.iter().zip(&mut self.states) {
            let holds = match (rule, state) {
                (RuleDef::Sensor(rule), RuleState::Sensor(state)) => {
                    let reading = inputs.values.get(&rule.sensor).copied();
                    state.holds(rule, reading, inputs.now)
                }
                (RuleDef::Process(rule), _) => rule.holds(inputs.processes),
                (RuleDef::Time(rule), _) => rule.holds(inputs.local),
                (RuleDef::Interval(rule), RuleState::Interval(state)) => {
                    state.holds(rule, inputs.now)
                }
                _ => false,
            };
            all &= holds;
        }
        all
    }

    fn reset(&mut self, now: Duration) {
        *self = Self::new(self.def.clone(), now);
    }
}

/// The first holding group, in list order. `None` means the Default Profile.
pub fn candidate(holds: &[bool]) -> Option<usize> {
    holds.iter().position(|h| *h)
}

/// Whether a group holds and since when. `since_us` is 0 when it does not hold.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupState {
    pub id: String,
    pub holds: bool,
    pub since_us: u64,
}

/// Reports the states of the groups only when a group starts or stops holding, or the list of
/// groups changes.
#[derive(Debug, Default)]
pub struct EdgeTracker {
    states: Vec<GroupState>,
}

impl EdgeTracker {
    /// `groups` has the id and the state of each group in list order. `wall_us` is the time for
    /// a group that starts to hold. Returns the new states when something changed.
    pub fn update(&mut self, groups: &[(&str, bool)], wall_us: u64) -> Option<&[GroupState]> {
        let same_edges = self.states.len() == groups.len()
            && self
                .states
                .iter()
                .zip(groups)
                .all(|(state, (id, holds))| state.id == *id && state.holds == *holds);
        if same_edges {
            return None;
        }
        let states = groups
            .iter()
            .map(|(id, holds)| {
                let before = self.states.iter().find(|s| s.id == *id);
                let since_us = match (holds, before) {
                    (false, _) => 0,
                    (true, Some(s)) if s.holds => s.since_us,
                    (true, _) => wall_us,
                };
                GroupState {
                    id: (*id).to_owned(),
                    holds: *holds,
                    since_us,
                }
            })
            .collect();
        self.states = states;
        Some(&self.states)
    }

    pub fn states(&self) -> &[GroupState] {
        &self.states
    }
}

#[cfg(test)]
mod tests {
    use super::super::interval::IntervalRule;
    use super::super::process::{MatchMode, MatchOn, ProcessEntry};
    use super::super::sensor::Comparison;
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);

    fn game_list(running: bool) -> ProcessList {
        ProcessList {
            entries: running
                .then(|| ProcessEntry::new("game", "game", ""))
                .into_iter()
                .collect(),
        }
    }

    fn process(name: &str) -> RuleDef {
        RuleDef::Process(ProcessRule {
            names: vec![name.to_owned()],
            match_mode: MatchMode::Any,
            match_on: MatchOn::Name,
        })
    }

    fn weekday(day: u8) -> RuleDef {
        RuleDef::Time(TimeRule {
            days: vec![day],
            start: 0,
            end: 1440,
        })
    }

    fn group(enabled: bool, rules: Vec<RuleDef>) -> Group {
        Group::new(
            RuleGroupDef {
                id: "group_1".into(),
                profile: "profile_2".into(),
                enabled,
                rules,
            },
            Duration::ZERO,
        )
    }

    fn holds(group: &mut Group, game: bool, day: u8) -> bool {
        let values = BTreeMap::new();
        let processes = game_list(game);
        group.holds(&Inputs {
            now: Duration::ZERO,
            values: &values,
            processes: &processes,
            local: DayTime {
                weekday: day,
                minute: 600,
            },
        })
    }

    #[test]
    fn group_holds_when_all_rules_hold() {
        let mut group = group(true, vec![process("game"), weekday(3)]);
        assert!(holds(&mut group, true, 3));
        // A process rule that holds and a time rule that does not hold.
        assert!(!holds(&mut group, true, 4));
        assert!(!holds(&mut group, false, 3));
    }

    #[test]
    fn group_without_rules_never_holds() {
        let mut group = group(true, Vec::new());
        assert!(!holds(&mut group, true, 3));
    }

    #[test]
    fn disabled_group_never_holds() {
        let mut group = group(false, vec![process("game"), weekday(3)]);
        assert!(!holds(&mut group, true, 3));
    }

    #[test]
    fn first_holding_group_wins() {
        assert_eq!(candidate(&[false, true, false, true]), Some(1));
        assert_eq!(candidate(&[true, true]), Some(0));
        assert_eq!(candidate(&[false, false]), None);
        assert_eq!(candidate(&[]), None);
    }

    #[test]
    fn a_hold_time_runs_while_another_rule_does_not_hold() {
        let sensor = RuleDef::Sensor(SensorRule {
            sensor: "t".into(),
            comparison: Comparison::Gt,
            value: 50.0,
            value_max: None,
            hold_s: 3,
        });
        let mut group = group(true, vec![process("game"), sensor]);
        let values = BTreeMap::from([("t".to_owned(), 60.0)]);
        let local = DayTime {
            weekday: 1,
            minute: 0,
        };
        let mut eval = |game: bool, second: u32| {
            let processes = game_list(game);
            group.holds(&Inputs {
                now: SECOND * second,
                values: &values,
                processes: &processes,
                local,
            })
        };
        // The sensor holds from second 0. The game starts at second 4: the group holds at once.
        assert!(!eval(false, 0));
        assert!(!eval(false, 3));
        assert!(eval(true, 4));
    }

    #[test]
    fn interval_rule_starts_with_the_group() {
        let rule = RuleDef::Interval(IntervalRule {
            interval_s: 10,
            delay_s: 5,
            duration_s: 2,
        });
        let mut group = Group::new(
            RuleGroupDef {
                id: "group_1".into(),
                profile: "p".into(),
                enabled: true,
                rules: vec![rule],
            },
            Duration::from_secs(100),
        );
        let (values, processes) = (BTreeMap::new(), ProcessList::default());
        let mut at = |t: u64| {
            group.holds(&Inputs {
                now: Duration::from_secs(t),
                values: &values,
                processes: &processes,
                local: DayTime {
                    weekday: 1,
                    minute: 0,
                },
            })
        };
        assert!(!at(104));
        assert!(at(105));
        assert!(!at(107));
    }

    #[test]
    fn rule_group_state_changes_only_on_edges() {
        let mut tracker = EdgeTracker::default();
        // The first call reports the list.
        assert!(tracker.update(&[("group_1", false)], 10).is_some());
        // The same state on later ticks reports nothing.
        for t in 11..20 {
            assert!(tracker.update(&[("group_1", false)], t).is_none());
        }
        // The group starts to hold: one report with the time.
        let states = tracker.update(&[("group_1", true)], 20).unwrap();
        assert_eq!(
            states,
            [GroupState {
                id: "group_1".into(),
                holds: true,
                since_us: 20
            }]
        );
        for t in 21..30 {
            assert!(tracker.update(&[("group_1", true)], t).is_none());
        }
        assert_eq!(tracker.states()[0].since_us, 20);
        // A second group appears while the first one keeps its time.
        let states = tracker
            .update(&[("group_1", true), ("group_2", true)], 30)
            .unwrap();
        assert_eq!(states[0].since_us, 20);
        assert_eq!(states[1].since_us, 30);
        // The group stops: the time goes back to 0.
        let states = tracker
            .update(&[("group_1", false), ("group_2", true)], 40)
            .unwrap();
        assert_eq!((states[0].holds, states[0].since_us), (false, 0));
    }
}
