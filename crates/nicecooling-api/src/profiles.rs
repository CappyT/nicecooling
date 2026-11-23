//! The typed proxy of the interface `io.github.cappyt.NiceCooling1.Profiles`.

use std::collections::HashMap;

use zbus::{OwnedObjectPath, OwnedVariant as OwnedValue, Variant as Value};

/// A rule: (kind, parameters).
pub type RuleOut = (String, HashMap<String, OwnedValue>);
/// A rule to send: (kind, parameters).
pub type RuleIn<'a> = (&'a str, HashMap<&'a str, Value<'a>>);
/// A Rule group to send: (group id, target Profile, enabled, rules).
pub type GroupIn<'a> = (&'a str, &'a OwnedObjectPath, bool, Vec<RuleIn<'a>>);

#[zbus::proxy(
    interface = "io.github.cappyt.NiceCooling1.Profiles",
    default_service = "io.github.cappyt.NiceCooling1",
    default_path = "/io/github/cappyt/NiceCooling1"
)]
pub trait Profiles {
    #[zbus(property)]
    fn active(&self) -> zbus::Result<OwnedObjectPath>;
    #[zbus(property)]
    fn activation_cause(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn active_state(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn default(&self) -> zbus::Result<OwnedObjectPath>;
    #[zbus(property)]
    fn immediate_switching(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn rule_delay_seconds(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn dwell_seconds(&self) -> zbus::Result<u32>;
    /// (group id, holds, since us or 0)
    #[zbus(property)]
    fn rule_group_state(&self) -> zbus::Result<Vec<(String, bool, u64)>>;

    fn create_profile(
        &self,
        name: &str,
        copy_from: &OwnedObjectPath,
    ) -> zbus::Result<OwnedObjectPath>;
    fn activate_profile(&self, profile: &OwnedObjectPath) -> zbus::Result<()>;
    fn return_to_automatic(&self) -> zbus::Result<()>;
    fn set_immediate_switching(&self, enabled: bool) -> zbus::Result<()>;
    fn set_rule_timing(&self, rule_delay_s: u32, dwell_s: u32) -> zbus::Result<()>;
    fn get_rule_groups(&self) -> zbus::Result<Vec<(String, OwnedObjectPath, bool, Vec<RuleOut>)>>;
    fn set_rule_groups(&self, groups: Vec<GroupIn<'_>>) -> zbus::Result<Vec<String>>;

    #[zbus(signal)]
    fn profile_activated(
        &self,
        profile: OwnedObjectPath,
        cause: String,
        state: String,
    ) -> zbus::Result<()>;
}
