//! The D-Bus API `io.github.cappyt.NiceCooling1` (`docs/api.md`). One file for each interface of
//! the XML contract. `Profiles`, `Profile` and some methods of `Leakshield` answer
//! `org.freedesktop.DBus.Error.NotSupported` until the tasks that own them fill them in.

use std::sync::Arc;

use nicecooling_api::Error;
use nicecooling_api::ids::{self, BOARD_PATH, Kind, ROOT_PATH};
use tokio::task::JoinHandle;
use zbus::object_server::ObjectServer;
use zbus::{Connection, OwnedObjectPath, fdo};

use crate::alarms::logind::ZbusLogin1;
use crate::authz::{Authorizer, Authz};

pub mod alarms;
pub mod board;
pub mod channel;
pub mod derived_sensor;
pub mod hub;
pub mod leakshield;
pub mod loops;
pub mod manager;
pub mod profile;
pub mod profiles;
pub mod sensor;
pub mod stage;

pub use hub::{BoardInfo, ChannelMeta, Hub, HubInit, SensorMeta};
pub use leakshield::LeakshieldInfo;

/// The reply of a method that a later task fills in.
pub fn not_supported_stub(interface: &str, method: &str) -> Error {
    Error::not_supported(format!("{interface}.{method} is not implemented yet"))
}

/// The API view of an hwmon sensor.
pub fn sensor_meta(sensor: &nicecooling_hwmon::discovery::Sensor) -> SensorMeta {
    use nicecooling_hwmon::discovery::SensorKind;
    let (kind, unit) = match sensor.kind {
        SensorKind::Temperature => ("temperature", "degC"),
        SensorKind::Fan => ("fan", "rpm"),
        SensorKind::Voltage => ("voltage", "V"),
        SensorKind::Current => ("current", "A"),
        SensorKind::Power => ("power", "W"),
        SensorKind::Humidity => ("other", "%"),
    };
    SensorMeta {
        id: sensor.id.clone(),
        name: sensor.name.clone(),
        kind,
        unit,
        driver: sensor.device.hwmon_name.clone(),
    }
}

pub fn object(kind: Kind, id: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(ids::object_path(kind, id))
        .expect("an identifier is a valid path element")
}

/// The path `/`: no object.
pub fn none() -> OwnedObjectPath {
    OwnedObjectPath::try_from("/").expect("the root path is valid")
}

pub fn optional_object(kind: Kind, id: Option<&str>) -> OwnedObjectPath {
    id.map_or_else(none, |id| object(kind, id))
}

fn root_path() -> OwnedObjectPath {
    OwnedObjectPath::try_from(ROOT_PATH).expect("the root path is valid")
}

/// Registers the objects of a sensor id: `Sensor`, and `DerivedSensor` for a Derived sensor.
pub(crate) async fn register_sensor(
    server: &ObjectServer,
    hub: &Arc<Hub>,
    id: &str,
) -> zbus::Result<()> {
    let path = object(Kind::Sensor, id);
    server
        .at(
            path.clone(),
            sensor::Sensor::new(hub.clone(), id.to_owned()),
        )
        .await?;
    if hub.derived(id).is_some() {
        server
            .at(
                path,
                derived_sensor::DerivedSensor::new(hub.clone(), id.to_owned()),
            )
            .await?;
    }
    Ok(())
}

/// Removes the objects of a sensor id. A calibrated flow sensor has no `DerivedSensor`
/// interface, and a concurrent call may have removed an interface already.
pub(crate) async fn unregister_sensor(server: &ObjectServer, id: &str) -> zbus::Result<()> {
    let path = object(Kind::Sensor, id);
    remove_if_present::<derived_sensor::DerivedSensor>(server, path.clone()).await?;
    remove_if_present::<sensor::Sensor>(server, path).await
}

async fn remove_if_present<I: zbus::object_server::Interface>(
    server: &ObjectServer,
    path: OwnedObjectPath,
) -> zbus::Result<()> {
    match server.remove::<I, _>(path).await {
        Ok(_) | Err(zbus::Error::InterfaceNotFound) => Ok(()),
        Err(e) => Err(e),
    }
}

pub(crate) async fn register_loop(
    server: &ObjectServer,
    hub: &Arc<Hub>,
    id: &str,
) -> zbus::Result<()> {
    server
        .at(
            object(Kind::Loop, id),
            loops::Loop::new(hub.clone(), id.to_owned()),
        )
        .await?;
    Ok(())
}

/// Keeps the calibrated flow sensor of a Loop in step with its configuration.
pub(crate) async fn sync_flow_sensor(
    server: &ObjectServer,
    hub: &Arc<Hub>,
    before: Option<String>,
    after: Option<String>,
) -> zbus::Result<()> {
    if before == after {
        return Ok(());
    }
    if let Some(id) = before {
        unregister_sensor(server, &id).await?;
    }
    if let Some(id) = after {
        register_sensor(server, hub, &id).await?;
    }
    Ok(())
}

/// Tells the clients that the `IsPump` property of Channels may have changed.
pub(crate) async fn notify_pump_flags(server: &ObjectServer, channels: &[String]) {
    for id in channels {
        if let Ok(iface) = server
            .interface::<_, channel::Channel>(object(Kind::Channel, id))
            .await
        {
            let _ = iface
                .get()
                .await
                .is_pump_changed(iface.signal_emitter())
                .await;
        }
    }
}

pub struct Served {
    task: JoinHandle<()>,
    alarm_task: Option<JoinHandle<()>>,
    profile_task: Option<JoinHandle<()>>,
}

impl Drop for Served {
    fn drop(&mut self) {
        self.task.abort();
        for task in [&self.alarm_task, &self.profile_task].into_iter().flatten() {
            task.abort();
        }
    }
}

/// Registers every object, starts the `SensorsUpdated` emitter and takes the bus name.
pub async fn serve(
    hub: &Arc<Hub>,
    connection: &Connection,
    authorizer: Arc<dyn Authorizer>,
) -> zbus::Result<Served> {
    hub.set_authz(Authz::new(authorizer));
    hub.set_connection(connection.clone());
    hub.set_login1(Arc::new(ZbusLogin1::new(connection.clone())));
    let server = connection.object_server();
    server.at(ROOT_PATH, fdo::ObjectManager).await?;
    server
        .at(root_path(), manager::Manager::new(hub.clone()))
        .await?;
    server
        .at(root_path(), profiles::Profiles::new(hub.clone()))
        .await?;
    server
        .at(root_path(), alarms::Alarms::new(hub.clone()))
        .await?;
    server
        .at(BOARD_PATH, board::Board::new(hub.clone()))
        .await?;
    for id in hub
        .hwmon_sensor_ids()
        .into_iter()
        .chain(hub.derived_ids())
        .chain(hub.flow_sensor_ids())
    {
        register_sensor(server, hub, &id).await?;
    }
    for id in hub.channel_ids() {
        server
            .at(
                object(Kind::Channel, &id),
                channel::Channel::new(hub.clone(), id.clone()),
            )
            .await?;
    }
    for id in hub.loop_ids() {
        register_loop(server, hub, &id).await?;
    }
    for device in hub.leakshields() {
        leakshield::register(connection, hub, &device).await?;
    }
    for id in hub.profile_ids() {
        server
            .at(
                object(Kind::Profile, &id),
                profile::Profile::new(hub.clone(), id.clone()),
            )
            .await?;
    }

    let reports = hub.take_report_receiver();
    let task = tokio::spawn(manager::emit_reports(
        connection.clone(),
        hub.clone(),
        reports,
    ));
    let alarm_task = hub.take_alarm_receiver().map(|effects| {
        tokio::spawn(alarms::run_effects(
            connection.clone(),
            hub.clone(),
            effects,
        ))
    });
    let profile_task = hub.take_profile_receiver().map(|events| {
        tokio::spawn(profiles::run_events(
            connection.clone(),
            hub.clone(),
            events,
        ))
    });
    connection.request_name(ids::BUS_NAME).await?;
    Ok(Served {
        task,
        alarm_task,
        profile_task,
    })
}
