//! The interface `Profile`.

use std::collections::HashMap;
use std::sync::Arc;

use nicecooling_api::errors::Result;
use nicecooling_api::ids::{DEFAULT_PROFILE, Kind};
use zbus::message::Header;
use zbus::object_server::{ObjectServer, SignalEmitter};
use zbus::{Connection, OwnedVariant, Variant, fdo, interface};

use super::hub::{Hub, ProfileContent, ProfileView};
use super::object;
use crate::leakshield::fields::FieldValue;
use crate::state::{ControlKind, ProfileChannel};

pub struct Profile {
    hub: Arc<Hub>,
    id: String,
}

impl Profile {
    pub fn new(hub: Arc<Hub>, id: String) -> Self {
        Self { hub, id }
    }

    /// For a property: a gone Profile is an unknown object.
    fn view(&self) -> fdo::Result<ProfileView> {
        self.hub
            .profile_view(&self.id)
            .ok_or_else(|| fdo::Error::UnknownObject(format!("no Profile {}", self.id)))
    }
}

fn variant(value: impl Into<Variant<'static>>) -> OwnedVariant {
    OwnedVariant::try_from(value.into()).expect("a plain value has no file descriptor")
}

fn channel_dict(channel: &ProfileChannel) -> HashMap<String, OwnedVariant> {
    let control = match channel.control {
        ControlKind::Hardware => "hardware",
        ControlKind::Curve => "curve",
        ControlKind::Fixed => "fixed",
    };
    let mut dict = HashMap::from([
        ("control".to_owned(), variant(control)),
        (
            "input".to_owned(),
            variant(channel.input.clone().unwrap_or_default()),
        ),
        ("points".to_owned(), variant(channel.points.clone())),
    ]);
    for (key, value) in [
        ("smoothing_s", channel.smoothing_s),
        ("step_limit_pct_per_s", channel.step_limit_pct_per_s),
        ("fixed_duty", channel.fixed_duty),
    ] {
        if let Some(value) = value {
            dict.insert(key.to_owned(), variant(value));
        }
    }
    dict
}

fn field_value(value: &FieldValue) -> OwnedVariant {
    match value {
        FieldValue::Number(n) => variant(*n),
        FieldValue::List(list) => variant(list.clone()),
    }
}

/// `docs/storage.md`: the keys of `profile.json`, plus `leakshield_fields`.
fn content_dict(content: ProfileContent) -> HashMap<String, OwnedVariant> {
    let profile = content.profile;
    let channels: HashMap<String, HashMap<String, OwnedVariant>> = profile
        .channels
        .iter()
        .map(|(id, channel)| (id.clone(), channel_dict(channel)))
        .collect();
    let fields: HashMap<String, HashMap<String, OwnedVariant>> = content
        .leakshield_fields
        .iter()
        .map(|(id, values)| {
            (
                id.clone(),
                values
                    .iter()
                    .map(|(field, value)| (field.clone(), field_value(value)))
                    .collect(),
            )
        })
        .collect();
    HashMap::from([
        ("id".to_owned(), variant(profile.id)),
        ("name".to_owned(), variant(profile.name)),
        ("slot".to_owned(), variant(profile.slot)),
        ("channels".to_owned(), variant(channels)),
        (
            "pump_floors".to_owned(),
            variant(profile.pump_floors.into_iter().collect::<HashMap<_, _>>()),
        ),
        ("leakshield_fields".to_owned(), variant(fields)),
    ])
}

#[interface(name = "io.github.cappyt.NiceCooling1.Profile")]
impl Profile {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> String {
        self.id.clone()
    }

    #[zbus(property)]
    fn name(&self) -> fdo::Result<String> {
        Ok(self.view()?.name)
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn is_default(&self) -> bool {
        self.id == DEFAULT_PROFILE
    }

    #[zbus(property)]
    fn contains_release(&self) -> fdo::Result<bool> {
        Ok(self.view()?.contains_release)
    }

    #[zbus(property)]
    fn slot(&self) -> fdo::Result<u8> {
        Ok(u8::try_from(self.view()?.slot).unwrap_or(1))
    }

    async fn set_name(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        name: &str,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Profile", "SetName", None)
            .await?;
        self.hub.set_profile_name(&self.id, name)?;
        let _ = self.name_changed(&emitter).await;
        Ok(())
    }

    async fn set_slot(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        slot: u8,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Profile", "SetSlot", None)
            .await?;
        self.hub.set_profile_slot(&self.id, slot)?;
        let _ = self.slot_changed(&emitter).await;
        let _ = self.contains_release_changed(&emitter).await;
        Ok(())
    }

    async fn delete(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Profile", "Delete", None)
            .await?;
        self.hub.delete_profile(&self.id)?;
        // The object goes after the reply: this call runs on the interface that goes away.
        let (server, id) = (server.clone(), self.id.clone());
        self.hub.spawn(async move {
            let _ = server
                .remove::<Profile, _>(object(Kind::Profile, &id))
                .await;
        });
        Ok(())
    }

    #[zbus(out_args("content"))]
    async fn get_content(&self) -> Result<HashMap<String, OwnedVariant>> {
        Ok(content_dict(self.hub.profile_content(&self.id)?))
    }
}
