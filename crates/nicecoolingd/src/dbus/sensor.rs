use std::sync::Arc;

use nicecooling_api::errors::Result;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::{Connection, interface};

use super::Hub;

pub struct Sensor {
    hub: Arc<Hub>,
    id: String,
}

impl Sensor {
    pub fn new(hub: Arc<Hub>, id: String) -> Self {
        Self { hub, id }
    }

    fn view(&self) -> super::hub::SensorView {
        self.hub
            .sensor(&self.id)
            .unwrap_or_else(|| super::hub::SensorView {
                id: self.id.clone(),
                name: String::new(),
                default_name: String::new(),
                kind: "other".into(),
                unit: String::new(),
                origin: "hwmon",
                driver: String::new(),
                value: None,
            })
    }
}

#[interface(name = "io.github.cappyt.NiceCooling1.Sensor")]
impl Sensor {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> String {
        self.id.clone()
    }

    #[zbus(property)]
    fn name(&self) -> String {
        self.view().name
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn default_name(&self) -> String {
        self.view().default_name
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn kind(&self) -> String {
        self.view().kind
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn unit(&self) -> String {
        self.view().unit
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn origin(&self) -> String {
        self.view().origin.to_owned()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn driver(&self) -> String {
        self.view().driver
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn value(&self) -> f64 {
        self.view().value.unwrap_or(0.0)
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn has_value(&self) -> bool {
        self.view().value.is_some()
    }

    async fn set_name(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        name: &str,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Sensor", "SetName", None)
            .await?;
        self.hub.set_sensor_name(&self.id, name)?;
        self.name_changed(&emitter).await?;
        Ok(())
    }
}
