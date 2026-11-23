use std::time::Duration;

use super::{Stage, TickConfig, TickContext};
use crate::profile::Cause;
use crate::rules::group::{EdgeTracker, Group, GroupState, Inputs, RuleGroupDef};
use crate::rules::time::DayTime;
use crate::rules::winner::{GroupView, Selector, Timing};

/// The daemon side of the Rule groups: where the groups, the timing and the manual activations
/// come from, and what an activation does.
pub trait RuleHost: Send {
    /// The changes since the last tick. The first call carries the whole state.
    fn poll(&mut self) -> HostInput;

    /// The active Profile changes. Returns the configuration of the new Profile.
    fn activate(&mut self, profile: &str, cause: Cause) -> Option<TickConfig>;

    /// A group started or stopped holding, or the list of groups changed.
    fn publish(&mut self, states: &[GroupState]);

    /// The end of the running Profile dwell changed: its wall time in microseconds since the Unix
    /// epoch, 0 when no dwell runs.
    fn publish_dwell(&mut self, _until_us: u64) {}
}

/// The active Profile as the daemon state has it at the start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveSync {
    pub default_profile: String,
    pub active: String,
    pub cause: Cause,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ManualCommand {
    /// The user activated this Profile by hand. The host activated it already.
    Activated(String),
    ReturnToAutomatic,
}

pub struct HostInput {
    pub sync: Option<ActiveSync>,
    /// The new list of groups: all of them, in order.
    pub groups: Option<Vec<RuleGroupDef>>,
    pub timing: Option<Timing>,
    /// The manual commands since the last tick, in arrival order. The last one wins.
    pub manual: Vec<ManualCommand>,
    pub local: DayTime,
    /// The wall clock in microseconds since the Unix epoch.
    pub wall_us: u64,
}

/// Stage 4: evaluate the Rule groups and activate the Profile of the winner.
#[derive(Default)]
pub struct RulesStage {
    host: Option<Box<dyn RuleHost>>,
    groups: Vec<Group>,
    timing: Timing,
    selector: Option<Selector>,
    tracker: EdgeTracker,
    /// The end of the dwell that the host knows, on the tick clock.
    dwell_end: Option<Duration>,
}

impl RulesStage {
    pub fn new(host: Box<dyn RuleHost>) -> Self {
        Self {
            host: Some(host),
            ..Self::default()
        }
    }
}

/// A group that did not change keeps its state. In an edited group, each rule that did not
/// change keeps its hold time and its interval period. A new or changed rule starts again, and
/// so does the Rule delay of a new or edited group.
fn replace_groups(
    groups: &mut Vec<Group>,
    selector: Option<&mut Selector>,
    defs: Vec<RuleGroupDef>,
    now: Duration,
) {
    let mut old = std::mem::take(groups);
    let mut selector = selector;
    for def in defs {
        let previous = old
            .iter()
            .position(|g| g.def().id == def.id)
            .map(|i| old.swap_remove(i));
        groups.push(match previous {
            Some(group) if group.def() == &def => group,
            previous => {
                if let Some(selector) = selector.as_deref_mut() {
                    selector.timers_mut().reset(&def.id);
                }
                match previous {
                    Some(group) => Group::edited(group, def, now),
                    None => Group::new(def, now),
                }
            }
        });
    }
}

impl Stage for RulesStage {
    fn name(&self) -> &'static str {
        "rules"
    }

    fn run(&mut self, ctx: &mut TickContext) {
        let Some(host) = self.host.as_mut() else {
            return;
        };
        let input = host.poll();
        if let Some(sync) = input.sync {
            self.selector = Some(Selector::new(
                &sync.default_profile,
                &sync.active,
                sync.cause,
                ctx.now,
            ));
        }
        if let Some(timing) = input.timing {
            self.timing = timing;
        }
        if let Some(defs) = input.groups {
            replace_groups(&mut self.groups, self.selector.as_mut(), defs, ctx.now);
        }
        let Some(selector) = self.selector.as_mut() else {
            return;
        };
        for command in input.manual {
            match command {
                ManualCommand::Activated(profile) => selector.manual(&profile, ctx.now),
                ManualCommand::ReturnToAutomatic => selector.return_to_automatic(),
            }
        }

        let inputs = Inputs {
            now: ctx.now,
            values: &ctx.values,
            processes: &ctx.processes,
            local: input.local,
        };
        let views: Vec<GroupView> = self
            .groups
            .iter_mut()
            .map(|group| GroupView {
                id: group.def().id.clone(),
                profile: group.def().profile.clone(),
                holds: group.holds(&inputs),
            })
            .collect();
        let edges: Vec<(&str, bool)> = views.iter().map(|v| (v.id.as_str(), v.holds)).collect();
        if let Some(states) = self.tracker.update(&edges, input.wall_us) {
            host.publish(states);
        }
        if let Some(change) = selector.update(ctx.now, &views, &self.timing)
            && let Some(config) = host.activate(&change.profile, change.cause)
        {
            ctx.config = config;
        }
        // The wall time is fixed when the end of the dwell changes, so a tick that runs late does
        // not move it.
        let end = selector.dwell_end(ctx.now, &self.timing);
        if end != self.dwell_end {
            self.dwell_end = end;
            host.publish_dwell(end.map_or(0, |end| {
                input.wall_us + u64::try_from((end - ctx.now).as_micros()).unwrap_or(0)
            }));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::rules::group::RuleDef;
    use crate::rules::time::TimeRule;

    #[derive(Default)]
    struct Log {
        activations: Vec<(String, Cause)>,
        published: usize,
    }

    struct FakeHost {
        first: bool,
        groups: Vec<RuleGroupDef>,
        log: Arc<Mutex<Log>>,
    }

    impl RuleHost for FakeHost {
        fn poll(&mut self) -> HostInput {
            let first = std::mem::take(&mut self.first);
            HostInput {
                sync: first.then(|| ActiveSync {
                    default_profile: "profile_default".into(),
                    active: "profile_default".into(),
                    cause: Cause::Default,
                }),
                groups: first.then(|| self.groups.clone()),
                timing: first.then_some(Timing {
                    immediate: true,
                    ..Timing::default()
                }),
                manual: Vec::new(),
                local: DayTime {
                    weekday: 1,
                    minute: 0,
                },
                wall_us: 7,
            }
        }

        fn activate(&mut self, profile: &str, cause: Cause) -> Option<TickConfig> {
            self.log
                .lock()
                .unwrap()
                .activations
                .push((profile.to_owned(), cause));
            Some(TickConfig::default())
        }

        fn publish(&mut self, _: &[GroupState]) {
            self.log.lock().unwrap().published += 1;
        }
    }

    /// A host that hands out the changes that a test queues, one for each tick.
    struct ScriptHost {
        inputs: std::collections::VecDeque<HostInput>,
        activations: Arc<Mutex<Vec<(String, Cause)>>>,
    }

    impl RuleHost for ScriptHost {
        fn poll(&mut self) -> HostInput {
            self.inputs.pop_front().unwrap_or_else(idle)
        }

        fn activate(&mut self, profile: &str, cause: Cause) -> Option<TickConfig> {
            self.activations
                .lock()
                .unwrap()
                .push((profile.to_owned(), cause));
            Some(TickConfig::default())
        }

        fn publish(&mut self, _: &[GroupState]) {}
    }

    fn idle() -> HostInput {
        HostInput {
            sync: None,
            groups: None,
            timing: None,
            manual: Vec::new(),
            local: DayTime {
                weekday: 1,
                minute: 0,
            },
            wall_us: 0,
        }
    }

    fn start() -> HostInput {
        HostInput {
            sync: Some(ActiveSync {
                default_profile: "profile_default".into(),
                active: "profile_default".into(),
                cause: Cause::Default,
            }),
            timing: Some(Timing::default()),
            ..idle()
        }
    }

    fn weekly(id: &str, profile: &str, start_minute: u16, end: u16) -> RuleGroupDef {
        RuleGroupDef {
            id: id.into(),
            profile: profile.into(),
            enabled: true,
            rules: vec![RuleDef::Time(TimeRule {
                days: (1..=7).collect(),
                start: start_minute,
                end,
            })],
        }
    }

    /// Runs `ticks` ticks from `from` and returns the activations that they caused.
    fn run(
        stage: &mut RulesStage,
        log: &Mutex<Vec<(String, Cause)>>,
        from: u64,
        ticks: u64,
    ) -> Vec<(String, Cause)> {
        let before = log.lock().unwrap().len();
        let mut ctx = TickContext::default();
        for t in from..from + ticks {
            ctx.now = Duration::from_secs(t);
            stage.run(&mut ctx);
        }
        log.lock().unwrap()[before..].to_vec()
    }

    type Activations = Arc<Mutex<Vec<(String, Cause)>>>;

    fn scripted(inputs: Vec<HostInput>) -> (RulesStage, Activations) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let stage = RulesStage::new(Box::new(ScriptHost {
            inputs: inputs.into(),
            activations: log.clone(),
        }));
        (stage, log)
    }

    /// The next tick reads this list of groups, as after `SetRuleGroups`.
    fn edit(stage: &mut RulesStage, log: &Activations, def: RuleGroupDef) {
        stage.host = Some(Box::new(ScriptHost {
            inputs: vec![HostInput {
                groups: Some(vec![def]),
                ..idle()
            }]
            .into(),
            activations: log.clone(),
        }));
    }

    #[test]
    fn an_edit_of_the_winning_group_keeps_it_active_while_it_holds() {
        let (mut stage, log) = scripted(vec![HostInput {
            groups: Some(vec![weekly("group_1", "profile_2", 0, 1440)]),
            ..start()
        }]);
        assert_eq!(run(&mut stage, &log, 0, 31).len(), 1);

        // The dwell is over. The edited group still holds: no switch, no new Rule delay.
        edit(&mut stage, &log, weekly("group_1", "profile_2", 0, 1439));

        assert_eq!(run(&mut stage, &log, 400, 100), []);
    }

    #[test]
    fn an_edit_that_stops_the_winning_group_restarts_its_rule_delay() {
        let (mut stage, log) = scripted(vec![HostInput {
            groups: Some(vec![weekly("group_1", "profile_2", 0, 1440)]),
            ..start()
        }]);
        assert_eq!(run(&mut stage, &log, 0, 31).len(), 1);

        // The edit makes the group stop holding (an empty range). The dwell is over.
        edit(&mut stage, &log, weekly("group_1", "profile_2", 600, 600));
        assert_eq!(
            run(&mut stage, &log, 400, 5),
            [("profile_default".to_owned(), Cause::Default)]
        );

        // A second edit makes it hold again: it needs the whole Rule delay.
        edit(&mut stage, &log, weekly("group_1", "profile_2", 0, 1440));
        assert_eq!(run(&mut stage, &log, 405, 29), []);
        assert_eq!(
            run(&mut stage, &log, 434, 2),
            [("profile_2".to_owned(), Cause::Rule)]
        );
    }

    /// The user adds a rule to the winning group. Its sensor rule did not change and the sensor
    /// still meets it, so the group still holds: the hold time of the sensor rule does not start
    /// again.
    #[test]
    fn an_edit_of_a_winning_group_with_a_hold_time_keeps_it_active() {
        use crate::rules::sensor::{Comparison, SensorRule};
        let sensor = RuleDef::Sensor(SensorRule {
            sensor: "sensor_1".into(),
            comparison: Comparison::Gt,
            value: 40.0,
            value_max: None,
            hold_s: 60,
        });
        let group = |rules: Vec<RuleDef>| RuleGroupDef {
            id: "group_1".into(),
            profile: "profile_2".into(),
            enabled: true,
            rules,
        };
        let (mut stage, log) = scripted(vec![HostInput {
            groups: Some(vec![group(vec![sensor.clone()])]),
            ..start()
        }]);
        let mut ctx = TickContext::default();
        ctx.values.insert("sensor_1".into(), 50.0);
        let mut tick = |stage: &mut RulesStage, from: u64, ticks: u64| {
            for t in from..from + ticks {
                ctx.now = Duration::from_secs(t);
                stage.run(&mut ctx);
            }
        };
        // 60 s of hold time and 30 s of Rule delay.
        tick(&mut stage, 0, 100);
        assert_eq!(
            log.lock().unwrap().as_slice(),
            [("profile_2".to_owned(), Cause::Rule)]
        );

        // The dwell is over. The new time rule holds for the whole week.
        edit(
            &mut stage,
            &log,
            group(vec![sensor, weekly("x", "x", 0, 1440).rules.remove(0)]),
        );
        tick(&mut stage, 400, 200);

        assert_eq!(log.lock().unwrap()[1..].to_vec(), []);
    }

    /// An edit keeps the interval phase of a rule that did not change, wherever the rule sits in
    /// the new list. A changed rule starts its first period at the edit.
    #[test]
    fn an_edit_keeps_the_interval_phase_of_a_rule_that_did_not_change() {
        use crate::rules::group::Inputs;
        use crate::rules::interval::IntervalRule;
        use crate::rules::process::ProcessList;
        let interval = |delay_s| {
            RuleDef::Interval(IntervalRule {
                interval_s: 100,
                delay_s,
                duration_s: 20,
            })
        };
        let group = |rules: Vec<RuleDef>| RuleGroupDef {
            id: "group_1".into(),
            profile: "profile_2".into(),
            enabled: true,
            rules,
        };
        let always = || weekly("x", "x", 0, 1440).rules.remove(0);
        let (mut stage, log) = scripted(vec![HostInput {
            groups: Some(vec![group(vec![interval(50)])]),
            ..start()
        }]);
        run(&mut stage, &log, 0, 1);
        let holds = |stage: &mut RulesStage, t: u64| {
            let (values, processes) = (Default::default(), ProcessList::default());
            stage.groups[0].holds(&Inputs {
                now: Duration::from_secs(t),
                values: &values,
                processes: &processes,
                local: DayTime {
                    weekday: 1,
                    minute: 0,
                },
            })
        };

        // The edit at 130 s adds a rule before the interval rule. The periods still start at 0 s.
        edit(&mut stage, &log, group(vec![always(), interval(50)]));
        run(&mut stage, &log, 130, 1);
        assert!(holds(&mut stage, 155), "position 55 of the window 50 to 70");
        assert!(!holds(&mut stage, 175));

        // The edit at 230 s changes the interval rule: its first period starts at 230 s.
        edit(&mut stage, &log, group(vec![always(), interval(60)]));
        run(&mut stage, &log, 230, 1);
        assert!(
            !holds(&mut stage, 260),
            "position 30 of the window 60 to 80, the old phase has 60"
        );
        assert!(holds(&mut stage, 295), "position 65 of the window 60 to 80");
    }

    #[test]
    fn one_activation_for_100_ticks() {
        let log = Arc::new(Mutex::new(Log::default()));
        // A time rule for the whole week holds all the time.
        let group = RuleGroupDef {
            id: "group_1".into(),
            profile: "profile_2".into(),
            enabled: true,
            rules: vec![RuleDef::Time(TimeRule {
                days: (1..=7).collect(),
                start: 0,
                end: 1440,
            })],
        };
        let mut stage = RulesStage::new(Box::new(FakeHost {
            first: true,
            groups: vec![group],
            log: log.clone(),
        }));
        let mut ctx = TickContext::default();
        for t in 0..100 {
            ctx.now = Duration::from_secs(t);
            stage.run(&mut ctx);
        }
        let log = log.lock().unwrap();
        assert_eq!(log.activations, [("profile_2".to_owned(), Cause::Rule)]);
        // The state of the group is published once, when it starts to hold.
        assert_eq!(log.published, 1);
    }
}
