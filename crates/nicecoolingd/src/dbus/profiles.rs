//! The interface `Profiles` on the root object.

use std::sync::Arc;

use nicecooling_api::Error;
use nicecooling_api::errors::Result;
use nicecooling_api::ids::{self, Kind};
use nicecooling_core::rules::group::RuleGroupDef;
use tokio::sync::mpsc;
use zbus::message::Header;
use zbus::object_server::{ObjectServer, SignalEmitter};
use zbus::{Connection, OwnedObjectPath, interface};

use super::hub::ProfileEvent;
use super::rule_params::{WireRule, decode_rule, encode_rule};
use super::{Hub, object, profile};

pub struct Profiles {
    hub: Arc<Hub>,
}

impl Profiles {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self { hub }
    }

    async fn authorize(
        &self,
        connection: &Connection,
        header: &Header<'_>,
        method: &str,
    ) -> Result<()> {
        self.hub
            .authorize(connection, header, "Profiles", method, None)
            .await
    }
}

type Group = (String, OwnedObjectPath, bool, Vec<WireRule>);

fn profile_id(path: &OwnedObjectPath) -> Result<String> {
    ids::id_from_path(Kind::Profile, path.as_str())
        .map(str::to_owned)
        .ok_or_else(|| Error::invalid(format!("{path} is not a Profile object path")))
}

fn decode_group((id, target, enabled, rules): Group) -> Result<RuleGroupDef> {
    Ok(RuleGroupDef {
        id,
        profile: profile_id(&target)?,
        enabled,
        rules: rules
            .into_iter()
            .map(|(kind, params)| decode_rule(&kind, params))
            .collect::<Result<_>>()?,
    })
}

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
        self.hub.active_state().to_owned()
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
        self.authorize(connection, &header, "CreateProfile").await?;
        let id = self.hub.create_profile(name, copy_from.as_str())?;
        let path = object(Kind::Profile, &id);
        server
            .at(path.clone(), profile::Profile::new(self.hub.clone(), id))
            .await
            .map_err(Error::ZBus)?;
        Ok(path)
    }

    async fn activate_profile(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        profile: OwnedObjectPath,
    ) -> Result<()> {
        self.authorize(connection, &header, "ActivateProfile")
            .await?;
        self.hub.activate_manually(&profile_id(&profile)?)
    }

    async fn return_to_automatic(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<()> {
        self.authorize(connection, &header, "ReturnToAutomatic")
            .await?;
        self.hub.return_to_automatic();
        Ok(())
    }

    async fn set_immediate_switching(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        enabled: bool,
    ) -> Result<()> {
        self.authorize(connection, &header, "SetImmediateSwitching")
            .await?;
        self.hub.set_immediate_switching(enabled);
        let _ = self.immediate_switching_changed(&emitter).await;
        Ok(())
    }

    async fn set_rule_timing(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        rule_delay_s: u32,
        dwell_s: u32,
    ) -> Result<()> {
        self.authorize(connection, &header, "SetRuleTiming").await?;
        self.hub.set_rule_timing(rule_delay_s, dwell_s);
        let _ = self.rule_delay_seconds_changed(&emitter).await;
        let _ = self.dwell_seconds_changed(&emitter).await;
        Ok(())
    }

    #[zbus(property)]
    fn rule_group_state(&self) -> Vec<(String, bool, u64)> {
        self.hub.rule_group_states()
    }

    #[zbus(out_args("groups"))]
    async fn get_rule_groups(&self) -> Result<Vec<Group>> {
        Ok(self
            .hub
            .rule_groups()
            .into_iter()
            .map(|group| {
                (
                    group.id,
                    object(Kind::Profile, &group.profile),
                    group.enabled,
                    group.rules.iter().map(encode_rule).collect(),
                )
            })
            .collect())
    }

    #[zbus(out_args("ids"))]
    async fn set_rule_groups(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        groups: Vec<Group>,
    ) -> Result<Vec<String>> {
        self.authorize(connection, &header, "SetRuleGroups").await?;
        let groups = groups
            .into_iter()
            .map(decode_group)
            .collect::<Result<Vec<_>>>()?;
        self.hub.set_rule_groups(groups)
    }

    #[zbus(signal)]
    async fn profile_activated(
        emitter: &SignalEmitter<'_>,
        profile: OwnedObjectPath,
        cause: String,
        state: String,
    ) -> zbus::Result<()>;
}

/// Emits the signals and the property changes of the activations and of the Rule groups. One
/// task, in order.
pub async fn run_events(
    connection: Connection,
    hub: Arc<Hub>,
    mut events: mpsc::UnboundedReceiver<ProfileEvent>,
) {
    let server = connection.object_server();
    while let Some(event) = events.recv().await {
        let Ok(iface) = server.interface::<_, Profiles>(ids::ROOT_PATH).await else {
            continue;
        };
        let emitter = iface.signal_emitter();
        match event {
            ProfileEvent::Switched => {
                let iface = iface.get().await;
                let _ = iface.active_changed(emitter).await;
                let _ = iface.activation_cause_changed(emitter).await;
                let _ = iface.active_state_changed(emitter).await;
            }
            ProfileEvent::RuleGroups => {
                let _ = iface.get().await.rule_group_state_changed(emitter).await;
            }
            ProfileEvent::Activated {
                profile,
                cause,
                state,
            } => {
                let _ = iface.get().await.active_state_changed(emitter).await;
                let _ = Profiles::profile_activated(
                    emitter,
                    object(Kind::Profile, &profile),
                    cause.to_owned(),
                    state.to_owned(),
                )
                .await;
            }
        }
    }
    drop(hub);
}
