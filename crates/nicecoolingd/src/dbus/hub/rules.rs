//! The Rule group part of the hub: the groups, the timing and the manual activation, and the
//! host that the rules stage of the tick reads (`nicecooling_core::tick::stage_rules`).

use std::sync::Arc;
use std::time::Duration;

use nicecooling_api::Error;
use nicecooling_api::errors::Result;
use nicecooling_api::ids;
use nicecooling_core::profile::Cause;
use nicecooling_core::rules::group::{GroupState, RuleDef, RuleGroupDef};
use nicecooling_core::rules::winner::Timing;
use nicecooling_core::tick::TickConfig;
use nicecooling_core::tick::stage_rules::{ActiveSync, HostInput, ManualCommand, RuleHost};

use super::{Hub, ProfileEvent, lock};
use crate::rules::local;
use crate::state::profiles::DEFAULT_PROFILE;

/// What the rules stage has not read yet.
#[derive(Debug)]
pub(super) struct RuleQueue {
    sync: bool,
    groups: bool,
    timing: bool,
    manual: Option<ManualCommand>,
}

impl Default for RuleQueue {
    /// The first tick reads everything.
    fn default() -> Self {
        Self {
            sync: true,
            groups: true,
            timing: true,
            manual: None,
        }
    }
}

fn cause_of(text: &str) -> Cause {
    match text {
        "manual" => Cause::Manual,
        "rule" => Cause::Rule,
        _ => Cause::Default,
    }
}

impl Hub {
    pub fn rule_groups(&self) -> Vec<RuleGroupDef> {
        self.inner().state.rule_groups.clone()
    }

    /// (group id, holds, since in microseconds or 0), in list order.
    pub fn rule_group_states(&self) -> Vec<(String, bool, u64)> {
        self.inner().rule_states.clone()
    }

    /// Replaces the whole list. An empty id creates a group. Returns the ids in list order.
    pub fn set_rule_groups(&self, mut groups: Vec<RuleGroupDef>) -> Result<Vec<String>> {
        {
            let inner = self.inner();
            let mut used: Vec<String> = inner
                .state
                .rule_groups
                .iter()
                .map(|g| g.id.clone())
                .chain(
                    groups
                        .iter()
                        .filter(|g| !g.id.is_empty())
                        .map(|g| g.id.clone()),
                )
                .collect();
            for n in 0..groups.len() {
                if groups[n].id.is_empty() {
                    let number = ids::next_number("group_", used.iter().map(String::as_str));
                    groups[n].id = ids::rule_group_id(number);
                    used.push(groups[n].id.clone());
                } else if !ids::is_identifier(&groups[n].id) {
                    return Err(Error::invalid(format!(
                        "{:?} is not a group id",
                        groups[n].id
                    )));
                }
                if groups[..n].iter().any(|other| other.id == groups[n].id) {
                    return Err(Error::invalid(format!("{} is listed twice", groups[n].id)));
                }
            }
            for group in &groups {
                if !inner.profiles.contains(&group.profile) {
                    return Err(Error::not_found(format!("no Profile {}", group.profile)));
                }
                for rule in &group.rules {
                    rule.validate().map_err(|e| {
                        Error::invalid(format!("{}, rule {}: {e}", group.id, rule.kind()))
                    })?;
                    if let RuleDef::Sensor(rule) = rule {
                        inner.require_sensor(&rule.sensor)?;
                    }
                }
            }
        }
        let ids: Vec<String> = groups.iter().map(|g| g.id.clone()).collect();
        self.change(|i| {
            i.state.rule_groups = groups;
            i.rule_queue.groups = true;
        });
        Ok(ids)
    }

    /// The Rule delay and the Profile dwell, in seconds. They apply to every group.
    pub fn set_rule_timing(&self, rule_delay_s: u32, dwell_s: u32) {
        self.change(|i| {
            i.state.profiles.rule_delay_s = rule_delay_s;
            i.state.profiles.dwell_s = dwell_s;
            i.rule_queue.timing = true;
        });
    }

    pub fn set_immediate_switching(&self, enabled: bool) {
        self.change(|i| {
            i.state.profiles.immediate_switching = enabled;
            i.rule_queue.timing = true;
        });
    }

    /// A manual activation wins at once, also during the Profile dwell, until
    /// `return_to_automatic`.
    pub fn activate_manually(self: &Arc<Self>, id: &str) -> Result<()> {
        self.activate_profile(id, Cause::Manual)?;
        self.change(|i| i.rule_queue.manual = Some(ManualCommand::Activated(id.to_owned())));
        Ok(())
    }

    /// The Rule groups decide again at the next tick.
    pub fn return_to_automatic(&self) {
        self.change(|i| i.rule_queue.manual = Some(ManualCommand::ReturnToAutomatic));
    }

    // The tick side.

    fn rule_poll(&self) -> HostInput {
        let (wall_us, local) = local::now();
        let mut inner = self.inner();
        let queue = std::mem::take(&mut inner.rule_queue);
        let state = &inner.state;
        let timing = Timing {
            rule_delay: Duration::from_secs(u64::from(state.profiles.rule_delay_s)),
            dwell: Duration::from_secs(u64::from(state.profiles.dwell_s)),
            immediate: state.profiles.immediate_switching,
        };
        HostInput {
            sync: queue.sync.then(|| ActiveSync {
                default_profile: DEFAULT_PROFILE.to_owned(),
                active: state.profiles.active.clone(),
                cause: cause_of(&state.profiles.cause),
            }),
            groups: queue.groups.then(|| state.rule_groups.clone()),
            timing: queue.timing.then_some(timing),
            manual: queue.manual,
            local,
            wall_us,
        }
    }

    fn rule_publish(&self, states: &[GroupState]) {
        lock(&self.inner).rule_states = states
            .iter()
            .map(|s| (s.id.clone(), s.holds, s.since_us))
            .collect();
        let _ = self.profile_events.send(ProfileEvent::RuleGroups);
    }
}

/// The host of the rules stage: it reads the state of the hub and applies the activations.
pub struct HubRules(Arc<Hub>);

impl HubRules {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self(hub)
    }
}

impl RuleHost for HubRules {
    fn poll(&mut self) -> HostInput {
        self.0.rule_poll()
    }

    fn activate(&mut self, profile: &str, cause: Cause) -> Option<TickConfig> {
        match self.0.activate_profile(profile, cause) {
            Ok(config) => Some(config),
            Err(e) => {
                eprintln!("nicecoolingd: the rule cannot activate {profile}: {e}");
                None
            }
        }
    }

    fn publish(&mut self, states: &[GroupState]) {
        self.0.rule_publish(states);
    }
}
