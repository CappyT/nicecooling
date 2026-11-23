//! The interface `Loop`.

use std::sync::Arc;

use nicecooling_api::errors::Result;
use nicecooling_api::ids::{self, Kind};
use zbus::message::Header;
use zbus::object_server::{ObjectServer, SignalEmitter};
use zbus::{Connection, OwnedObjectPath, interface};

use super::hub::LoopView;
use super::{Hub, notify_pump_flags, object, optional_object, sync_flow_sensor};

pub struct Loop {
    hub: Arc<Hub>,
    id: String,
}

impl Loop {
    pub fn new(hub: Arc<Hub>, id: String) -> Self {
        Self { hub, id }
    }

    fn view(&self) -> LoopView {
        self.hub
            .loop_view(&self.id)
            .expect("a Loop object has a Loop")
    }

    fn flow_id(&self) -> Option<String> {
        self.hub.loop_view(&self.id).and_then(|l| l.flow)
    }
}

fn channel_ids(paths: &[OwnedObjectPath]) -> Result<Vec<String>> {
    paths
        .iter()
        .map(|p| {
            ids::id_from_path(Kind::Channel, p.as_str())
                .map(str::to_owned)
                .ok_or_else(|| {
                    nicecooling_api::Error::invalid(format!("{p} is not a Channel object path"))
                })
        })
        .collect()
}

#[interface(name = "io.github.cappyt.NiceCooling1.Loop")]
impl Loop {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> String {
        self.id.clone()
    }

    #[zbus(property)]
    fn name(&self) -> String {
        self.view().name
    }

    #[zbus(property)]
    fn pump_channels(&self) -> Vec<OwnedObjectPath> {
        self.view()
            .pump_channels
            .iter()
            .map(|id| object(Kind::Channel, id))
            .collect()
    }

    #[zbus(property)]
    fn flow_sensor(&self) -> OwnedObjectPath {
        optional_object(Kind::Sensor, self.view().flow_sensor.as_deref())
    }

    #[zbus(property)]
    fn calibration_table(&self) -> String {
        self.view().calibration_table.unwrap_or_default()
    }

    #[zbus(property)]
    fn flow(&self) -> OwnedObjectPath {
        optional_object(Kind::Sensor, self.view().flow.as_deref())
    }

    #[zbus(property)]
    fn water_temperature(&self) -> OwnedObjectPath {
        optional_object(Kind::Sensor, self.view().water_temperature.as_deref())
    }

    #[zbus(property)]
    fn leakshield(&self) -> OwnedObjectPath {
        optional_object(Kind::Leakshield, self.view().leakshield.as_deref())
    }

    #[zbus(property)]
    fn pump_floor(&self) -> f64 {
        self.view().pump_floor
    }

    async fn set_name(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        name: &str,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Loop", "SetName", None)
            .await?;
        self.hub.set_loop_name(&self.id, name)?;
        self.name_changed(&emitter).await?;
        Ok(())
    }

    async fn set_pump_channels(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        channels: Vec<OwnedObjectPath>,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Loop", "SetPumpChannels", None)
            .await?;
        let before = self.view().pump_channels;
        let wanted = channel_ids(&channels)?;
        self.hub.set_pump_channels(&self.id, &wanted)?;
        self.pump_channels_changed(&emitter).await?;
        let touched: Vec<String> = before.into_iter().chain(wanted).collect();
        notify_pump_flags(server, &touched).await;
        Ok(())
    }

    async fn set_flow_sensor(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        sensor: OwnedObjectPath,
        calibration_table: &str,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Loop", "SetFlowSensor", None)
            .await?;
        let before = self.flow_id();
        self.hub
            .set_flow_sensor(&self.id, sensor.as_str(), calibration_table)?;
        sync_flow_sensor(server, &self.hub, before, self.flow_id()).await?;
        self.flow_sensor_changed(&emitter).await?;
        self.calibration_table_changed(&emitter).await?;
        self.flow_changed(&emitter).await?;
        Ok(())
    }

    async fn set_water_temperature(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        sensor: OwnedObjectPath,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Loop", "SetWaterTemperature", None)
            .await?;
        self.hub.set_water_temperature(&self.id, sensor.as_str())?;
        self.water_temperature_changed(&emitter).await?;
        Ok(())
    }

    async fn set_leakshield(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        leakshield: OwnedObjectPath,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Loop", "SetLeakshield", None)
            .await?;
        self.hub
            .set_loop_leakshield(&self.id, leakshield.as_str())?;
        self.leakshield_changed(&emitter).await?;
        Ok(())
    }

    async fn set_pump_floor(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        duty: f64,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Loop", "SetPumpFloor", None)
            .await?;
        self.hub.set_pump_floor(&self.id, duty)?;
        self.pump_floor_changed(&emitter).await?;
        Ok(())
    }

    async fn delete(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Loop", "Delete", None)
            .await?;
        let (flow, pumps) = {
            let view = self.view();
            (view.flow, view.pump_channels)
        };
        self.hub.delete_loop(&self.id)?;
        // The objects go after the reply: this call runs on the interface that goes away.
        let (server, hub, id) = (server.clone(), self.hub.clone(), self.id.clone());
        self.hub.spawn(async move {
            let _ = sync_flow_sensor(&server, &hub, flow, None).await;
            let _ = server.remove::<Loop, _>(object(Kind::Loop, &id)).await;
            notify_pump_flags(&server, &pumps).await;
        });
        Ok(())
    }
}
