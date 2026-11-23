use std::time::Duration;

use super::{Stage, TickConfig, TickContext};
use crate::profile::Cause;
use crate::rules::group::{EdgeTracker, Group, GroupState, Inputs, RuleGroupDef};
use crate::rules::time::LocalTime;
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
    pub manual: Option<ManualCommand>,
    pub local: LocalTime,
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
}

impl RulesStage {
    pub fn new(host: Box<dyn RuleHost>) -> Self {
        Self {
            host: Some(host),
            ..Self::default()
        }
    }
}

/// A group that did not change keeps its state. A new or changed group starts its Rule delay,
/// its hold times and its interval period again.
fn replace_groups(
    groups: &mut Vec<Group>,
    selector: Option<&mut Selector>,
    defs: Vec<RuleGroupDef>,
    now: Duration,
) {
    let mut old = std::mem::take(groups);
    let mut selector = selector;
    for def in defs {
        let kept = old
            .iter()
            .position(|g| g.def() == &def)
            .map(|i| old.swap_remove(i));
        groups.push(kept.unwrap_or_else(|| {
            if let Some(selector) = selector.as_deref_mut() {
                selector.timers_mut().reset(&def.id);
            }
            Group::new(def, now)
        }));
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
        match input.manual {
            Some(ManualCommand::Activated(profile)) => selector.manual(&profile, ctx.now),
            Some(ManualCommand::ReturnToAutomatic) => selector.return_to_automatic(),
            None => {}
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
                manual: None,
                local: LocalTime {
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
