use std::sync::Arc;

use nicecooling_api::errors::Result;
use zbus::Connection;
use zbus::interface;
use zbus::message::Header;
use zbus::object_server::ObjectServer;

use super::Hub;

pub struct GraphSensor {
    hub: Arc<Hub>,
    id: String,
}

impl GraphSensor {
    pub fn new(hub: Arc<Hub>, id: String) -> Self {
        Self { hub, id }
    }
}

#[interface(name = "io.github.cappyt.NiceCooling1.GraphSensor")]
impl GraphSensor {
    async fn set_definition(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
        name: &str,
        unit: &str,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "GraphSensor", "SetDefinition", None)
            .await?;
        self.hub.set_graph_definition(&self.id, name, unit)?;
        // The properties of a Sensor object are constant. The object goes and comes back after the
        // reply, so a client reads the new name, unit and Kind. The call runs on the interface
        // that goes away.
        let (server, hub, id) = (server.clone(), self.hub.clone(), self.id.clone());
        self.hub.spawn(async move {
            let _ = super::unregister_sensor(&server, &id).await;
            if let Err(e) = super::register_sensor(&server, &hub, &id).await {
                eprintln!("nicecoolingd: cannot register the sensor {id} again: {e}");
            }
        });
        Ok(())
    }

    async fn delete(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "GraphSensor", "Delete", None)
            .await?;
        self.hub.delete_graph_sensor(&self.id)?;
        let (server, id) = (server.clone(), self.id.clone());
        self.hub.spawn(async move {
            let _ = super::unregister_sensor(&server, &id).await;
        });
        Ok(())
    }
}
