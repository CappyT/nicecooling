use std::sync::Arc;

use nicecooling_api::errors::Result;
use nicecooling_api::ids::Kind;
use zbus::message::Header;
use zbus::object_server::{ObjectServer, SignalEmitter};
use zbus::{Connection, OwnedObjectPath, interface};

use super::{Hub, object};

pub struct DerivedSensor {
    hub: Arc<Hub>,
    id: String,
}

impl DerivedSensor {
    pub fn new(hub: Arc<Hub>, id: String) -> Self {
        Self { hub, id }
    }
}

pub fn source_ids(paths: &[OwnedObjectPath]) -> Result<Vec<String>> {
    paths
        .iter()
        .map(|p| {
            nicecooling_api::ids::id_from_path(Kind::Sensor, p.as_str())
                .map(str::to_owned)
                .ok_or_else(|| {
                    nicecooling_api::Error::invalid(format!("{p} is not a sensor object path"))
                })
        })
        .collect()
}

#[interface(name = "io.github.cappyt.NiceCooling1.DerivedSensor")]
impl DerivedSensor {
    #[zbus(property, name = "Type")]
    fn derived_type(&self) -> String {
        self.hub
            .derived(&self.id)
            .map_or_else(String::new, |d| d.0.to_owned())
    }

    #[zbus(property)]
    fn sources(&self) -> Vec<OwnedObjectPath> {
        self.hub
            .derived(&self.id)
            .map(|d| d.1)
            .unwrap_or_default()
            .iter()
            .map(|id| object(Kind::Sensor, id))
            .collect()
    }

    async fn set_definition(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        r#type: &str,
        sources: Vec<OwnedObjectPath>,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "DerivedSensor", "SetDefinition", None)
            .await?;
        self.hub
            .set_derived_definition(&self.id, r#type, &source_ids(&sources)?)?;
        self.type_changed(&emitter).await?;
        self.sources_changed(&emitter).await?;
        Ok(())
    }

    async fn delete(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "DerivedSensor", "Delete", None)
            .await?;
        self.hub.delete_derived(&self.id)?;
        // The objects go after the reply: this call runs on the interface that goes away.
        let (server, id) = (server.clone(), self.id.clone());
        self.hub.spawn(async move {
            let _ = super::unregister_sensor(&server, &id).await;
        });
        Ok(())
    }
}
