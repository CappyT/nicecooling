//! The interface `Manager` on the root object, and the emitter of `SensorsUpdated`.

use std::collections::BTreeMap;
use std::sync::Arc;

use nicecooling_api::errors::Result;
use nicecooling_api::ids::{self, Kind, ROOT_PATH};
use tokio::sync::mpsc;
use zbus::message::Header;
use zbus::object_server::{ObjectServer, SignalEmitter};
use zbus::{Connection, OwnedObjectPath, interface};

use super::hub::TickReport;
use super::{Hub, object, register_loop, register_sensor};

pub struct Manager {
    hub: Arc<Hub>,
}

impl Manager {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self { hub }
    }
}

#[interface(name = "io.github.cappyt.NiceCooling1.Manager")]
impl Manager {
    #[zbus(property(emits_changed_signal = "const"))]
    fn version(&self) -> String {
        self.hub.version().to_owned()
    }

    #[zbus(property)]
    fn state_health(&self) -> String {
        self.hub.state_health()
    }

    #[zbus(out_args("sensor"))]
    async fn create_derived_sensor(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
        name: &str,
        r#type: &str,
        sources: Vec<OwnedObjectPath>,
    ) -> Result<OwnedObjectPath> {
        self.hub
            .authorize(connection, &header, "Manager", "CreateDerivedSensor", None)
            .await?;
        let sources = super::derived_sensor::source_ids(&sources)?;
        let id = self.hub.create_derived(name, r#type, &sources)?;
        register_sensor(server, &self.hub, &id).await?;
        Ok(object(Kind::Sensor, &id))
    }

    #[zbus(out_args("loop"))]
    async fn create_loop(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(object_server)] server: &ObjectServer,
        name: &str,
    ) -> Result<OwnedObjectPath> {
        self.hub
            .authorize(connection, &header, "Manager", "CreateLoop", None)
            .await?;
        let id = self.hub.create_loop(name)?;
        register_loop(server, &self.hub, &id).await?;
        Ok(object(Kind::Loop, &id))
    }

    #[zbus(out_args("tables"))]
    fn get_calibration_tables(&self) -> Vec<super::hub::CalibrationRow> {
        self.hub.calibration_tables()
    }

    #[zbus(out_args("table_id"))]
    async fn set_calibration_table(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        id: &str,
        name: &str,
        points: Vec<(f64, f64)>,
    ) -> Result<String> {
        self.hub
            .authorize(connection, &header, "Manager", "SetCalibrationTable", None)
            .await?;
        self.hub.set_calibration_table(id, name, points)
    }

    async fn delete_calibration_table(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        id: &str,
    ) -> Result<()> {
        self.hub
            .authorize(
                connection,
                &header,
                "Manager",
                "DeleteCalibrationTable",
                None,
            )
            .await?;
        self.hub.delete_calibration_table(id)
    }

    #[zbus(signal)]
    async fn sensors_updated(
        emitter: &SignalEmitter<'_>,
        time_us: u64,
        sensors: Vec<(String, f64, u8)>,
        channels: Vec<(String, f64, u8)>,
    ) -> zbus::Result<()>;
}

/// Sends `SensorsUpdated` for each tick report, and `PropertiesChanged` for the Channel
/// properties that the tick changes.
pub async fn emit_reports(
    connection: Connection,
    hub: Arc<Hub>,
    reports: Option<mpsc::Receiver<TickReport>>,
) {
    let Some(mut reports) = reports else { return };
    let server = connection.object_server();
    let mut last: BTreeMap<String, (String, bool)> = BTreeMap::new();
    while let Some(report) = reports.recv().await {
        if let Ok(iface) = server.interface::<_, Manager>(ROOT_PATH).await {
            let _ = Manager::sensors_updated(
                iface.signal_emitter(),
                report.time_us,
                report.sensors,
                report.channels,
            )
            .await;
        }
        for id in hub.channel_ids() {
            let Some(view) = hub.channel(&id) else {
                continue;
            };
            let state = (view.override_state.to_owned(), view.manual_at_snapshot);
            if last.get(&id) == Some(&state) {
                continue;
            }
            let first = last.insert(id.clone(), state).is_none();
            if first {
                continue;
            }
            if let Ok(iface) = server
                .interface::<_, super::channel::Channel>(ids::object_path(Kind::Channel, &id))
                .await
            {
                let guard = iface.get().await;
                let _ = guard.override_changed(iface.signal_emitter()).await;
                let _ = guard
                    .manual_at_snapshot_changed(iface.signal_emitter())
                    .await;
            }
        }
    }
}
