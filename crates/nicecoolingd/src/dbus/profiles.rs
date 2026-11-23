//! The interface `Profiles` on the root object. Tasks 7.x fill the methods in.

use std::collections::HashMap;
use std::sync::Arc;

use nicecooling_api::errors::Result;
use nicecooling_api::ids::Kind;
use zbus::message::Header;
use zbus::object_server::{ObjectServer, SignalEmitter};
use zbus::{Connection, OwnedObjectPath, OwnedVariant, interface};

use super::{Hub, not_supported_stub, object, profile};

pub struct Profiles {
    hub: Arc<Hub>,
}

impl Profiles {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self { hub }
    }
}

fn stub<T>(method: &str) -> Result<T> {
    Err(not_supported_stub("Profiles", method))
}

type Rule = (String, HashMap<String, OwnedVariant>);
type Group = (String, OwnedObjectPath, bool, Vec<Rule>);

#[allow(unused_variables)]
#[interface(name = "io.github.cappyt.NiceCooling1.Profiles")]
impl Profiles {
    #[zbus(property)]
    fn active(&self) -> OwnedObjectPath {
        object(Kind::Profile, &self.hub.profiles_state().active)
    }

    #[zbus(property)]
    fn activation_cause(&self) -> String {
        self.hub.profiles_state().cause
    }

    #[zbus(property)]
    fn active_state(&self) -> String {
        "applied".into()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn default(&self) -> OwnedObjectPath {
        object(Kind::Profile, &self.hub.profiles_state().default)
    }

    #[zbus(property)]
    fn immediate_switching(&self) -> bool {
        self.hub.profiles_state().immediate_switching
    }

    #[zbus(property)]
    fn rule_delay_seconds(&self) -> u32 {
        self.hub.profiles_state().rule_delay_s
    }

    #[zbus(property)]
    fn dwell_seconds(&self) -> u32 {
        self.hub.profiles_state().dwell_s
    }

    #[zbus(out_args("profile"))]
    async fn create_profile(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
        name: &str,
        copy_from: OwnedObjectPath,
    ) -> Result<OwnedObjectPath> {
        self.hub
            .authorize(connection, &header, "Profiles", "CreateProfile", None)
            .await?;
        let id = self.hub.create_profile(name, copy_from.as_str())?;
        let path = object(Kind::Profile, &id);
        server
            .at(path.clone(), profile::Profile::new(self.hub.clone(), id))
            .await
            .map_err(nicecooling_api::Error::ZBus)?;
        Ok(path)
    }

    async fn activate_profile(&self, profile: OwnedObjectPath) -> Result<()> {
        stub("ActivateProfile")
    }

    async fn return_to_automatic(&self) -> Result<()> {
        stub("ReturnToAutomatic")
    }

    async fn set_immediate_switching(&self, enabled: bool) -> Result<()> {
        stub("SetImmediateSwitching")
    }

    async fn set_rule_timing(&self, rule_delay_s: u32, dwell_s: u32) -> Result<()> {
        stub("SetRuleTiming")
    }

    #[zbus(property)]
    fn rule_group_state(&self) -> Vec<(String, bool, u64)> {
        Vec::new()
    }

    #[zbus(out_args("groups"))]
    async fn get_rule_groups(&self) -> Result<Vec<Group>> {
        stub("GetRuleGroups")
    }

    #[zbus(out_args("ids"))]
    async fn set_rule_groups(&self, groups: Vec<Group>) -> Result<Vec<String>> {
        stub("SetRuleGroups")
    }

    #[zbus(signal)]
    async fn profile_activated(
        emitter: &SignalEmitter<'_>,
        profile: OwnedObjectPath,
        cause: String,
        state: String,
    ) -> zbus::Result<()>;
}
