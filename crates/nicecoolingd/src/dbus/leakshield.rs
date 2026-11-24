//! The interface `Leakshield`. The device operations live in `crate::leakshield`: this file
//! authorizes the caller, converts the arguments and replies, and emits the signals.

use std::collections::HashMap;
use std::sync::Arc;

use nicecooling_api::errors::Result;
use nicecooling_api::ids::{self, Kind};
use tokio::sync::mpsc;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::{Connection, OwnedObjectPath, OwnedVariant, Variant, interface};

use super::field_value::{field_value, variant_of};
use super::hub::Hub;
use super::{object, optional_object};
use crate::alarms::proposals;
use crate::leakshield::DeviceEvent;
use crate::leakshield::attached::{Attached, Info};
use crate::leakshield::backup::BackupSet;
use crate::leakshield::software_sensors::{Assignment, SLOT_COUNT};

/// What the hub keeps of a Leakshield: the attached device.
pub type LeakshieldInfo = Arc<Attached>;

pub struct Leakshield {
    hub: Arc<Hub>,
    device: Arc<Attached>,
}

impl Leakshield {
    pub fn new(hub: Arc<Hub>, device: Arc<Attached>) -> Self {
        Self { hub, device }
    }

    async fn authorize(
        &self,
        connection: &Connection,
        header: &Header<'_>,
        method: &str,
        mode: Option<&str>,
    ) -> Result<()> {
        self.hub
            .authorize(connection, header, "Leakshield", method, mode)
            .await
    }
}

/// A detail of an event stays a `d` (`docs/api.md`, Leakshield events).
fn details_variant(value: f64) -> OwnedVariant {
    OwnedVariant::try_from(Variant::F64(value)).expect("a number has no file descriptor")
}

/// The values of a `SetFields` or `SetStoredFields` argument, sorted by field id.
fn field_values(
    fields: HashMap<String, OwnedVariant>,
) -> Result<Vec<(String, crate::leakshield::fields::FieldValue)>> {
    let mut values = Vec::with_capacity(fields.len());
    for (id, value) in fields {
        let value = field_value(&value).ok_or_else(|| {
            nicecooling_api::Error::invalid(format!(
                "the value of {id} is not a number, a name or a list of them"
            ))
        })?;
        values.push((id, value));
    }
    values.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(values)
}

#[interface(name = "io.github.cappyt.NiceCooling1.Leakshield")]
impl Leakshield {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> String {
        self.device.id()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn serial(&self) -> String {
        self.device.serial().to_owned()
    }

    #[zbus(property)]
    fn firmware(&self) -> u16 {
        self.device.info().firmware
    }

    #[zbus(property)]
    fn structure(&self) -> u16 {
        self.device.info().structure
    }

    #[zbus(property)]
    fn connected(&self) -> bool {
        self.device.info().connected
    }

    #[zbus(property)]
    fn writable(&self) -> bool {
        self.device.info().read_only_reason.is_empty()
    }

    #[zbus(property)]
    fn read_only_reason(&self) -> String {
        self.device.info().read_only_reason
    }

    #[zbus(property)]
    fn mode(&self) -> String {
        self.device.info().mode
    }

    #[zbus(property)]
    fn running_action(&self) -> String {
        self.device.info().running_action
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn clock_offset(&self) -> i64 {
        self.device.clock_offset()
    }

    #[zbus(property)]
    fn software_sensor_slots(&self) -> u16 {
        SLOT_COUNT as u16
    }

    #[zbus(property)]
    fn feed_state(&self) -> String {
        self.device.info().feed_state.to_owned()
    }

    #[zbus(property)]
    fn last_feed(&self) -> (u64, f64, f64) {
        self.device.info().last_feed
    }

    #[zbus(property)]
    fn active_slot(&self) -> u8 {
        self.device.info().active_slot
    }

    #[zbus(property)]
    fn failed_write_backup(&self) -> String {
        self.device.info().failed_write_backup
    }

    #[zbus(out_args("fields"))]
    async fn get_fields(&self) -> Result<Vec<(String, String, OwnedVariant, bool, String)>> {
        Ok(self
            .device
            .get_fields()?
            .into_iter()
            .map(|row| {
                (
                    row.id,
                    row.risk.to_owned(),
                    variant_of(&row.value),
                    row.writable,
                    row.reason,
                )
            })
            .collect())
    }

    async fn set_fields(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        fields: HashMap<String, OwnedVariant>,
    ) -> Result<()> {
        self.authorize(connection, &header, "SetFields", None)
            .await?;
        let values = field_values(fields)?;
        if let Some(report3) = self.device.set_fields(values).await? {
            self.hub.store_user_write(&self.device, &report3);
        }
        Ok(())
    }

    #[zbus(out_args("fields"))]
    async fn get_stored_fields(&self, slot: u8) -> Result<HashMap<String, OwnedVariant>> {
        let stored = self
            .device
            .get_stored_fields(slot, &self.hub.slot_store())
            .await?;
        Ok(stored
            .into_iter()
            .map(|(id, value)| (id, variant_of(&value)))
            .collect())
    }

    async fn set_stored_fields(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        slot: u8,
        fields: HashMap<String, OwnedVariant>,
    ) -> Result<()> {
        self.authorize(connection, &header, "SetStoredFields", None)
            .await?;
        // A value that does not decode is an error of its field, after the slot checks.
        let mut values: Vec<(String, Option<crate::leakshield::fields::FieldValue>)> = fields
            .iter()
            .map(|(id, value)| (id.clone(), field_value(value)))
            .collect();
        values.sort_by(|a, b| a.0.cmp(&b.0));
        self.device
            .set_stored_fields(slot, values, &self.hub.slot_store())
            .await
    }

    async fn set_mode(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        mode: &str,
    ) -> Result<()> {
        self.authorize(connection, &header, "SetMode", Some(mode))
            .await?;
        self.device.set_mode(mode, &self.hub.slot_store())
    }

    async fn start_action(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        action: &str,
    ) -> Result<()> {
        self.authorize(connection, &header, "StartAction", None)
            .await?;
        self.device.start_action(action)
    }

    async fn cancel_action(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<()> {
        self.authorize(connection, &header, "CancelAction", None)
            .await?;
        self.device.cancel_action()
    }

    async fn sync_clock(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<()> {
        self.authorize(connection, &header, "SyncClock", None)
            .await?;
        self.device.sync_clock()
    }

    async fn reset_alarms(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<()> {
        self.authorize(connection, &header, "ResetAlarms", None)
            .await?;
        self.device.reset_alarms()
    }

    async fn reset_refill_history(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<()> {
        self.authorize(connection, &header, "ResetRefillHistory", None)
            .await?;
        self.device.reset_refill_history()
    }

    async fn reset_service_counter(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<()> {
        self.authorize(connection, &header, "ResetServiceCounter", None)
            .await?;
        self.device.reset_service_counter()
    }

    #[zbus(out_args("counters"))]
    async fn get_counters(&self) -> Result<HashMap<String, u64>> {
        Ok(self
            .device
            .get_counters()?
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect())
    }

    #[zbus(out_args("refills"))]
    async fn get_refill_history(&self) -> Vec<(u64, u32, u32, f64)> {
        self.device
            .get_refill_history()
            .into_iter()
            .map(|r| {
                (
                    r.time_us,
                    r.duration_s,
                    r.pump_turns,
                    f64::from(r.volume_tenth_ml) / 10.0,
                )
            })
            .collect()
    }

    #[zbus(out_args("events"))]
    async fn get_events(&self) -> Result<Vec<(u64, String, HashMap<String, OwnedVariant>)>> {
        Ok(self
            .device
            .get_events()?
            .into_iter()
            .map(|event| {
                let details = event
                    .details
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), details_variant(value)))
                    .collect();
                (event.time_us, event.id, details)
            })
            .collect())
    }

    #[zbus(out_args("slots"))]
    async fn get_software_sensors(&self) -> Result<Vec<(u16, OwnedObjectPath)>> {
        Ok(self
            .device
            .software_slots()
            .list()
            .into_iter()
            .map(|(slot, sensor)| (slot, optional_object(Kind::Sensor, sensor.as_deref())))
            .collect())
    }

    async fn set_software_sensor(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        slot: u16,
        sensor: OwnedObjectPath,
    ) -> Result<()> {
        self.authorize(connection, &header, "SetSoftwareSensor", None)
            .await?;
        let assignment = if sensor.as_str() == "/" {
            None
        } else {
            let id = ids::id_from_path(Kind::Sensor, sensor.as_str()).ok_or_else(|| {
                nicecooling_api::Error::invalid(format!("{sensor} is not a sensor"))
            })?;
            let view = self
                .hub
                .sensor(id)
                .ok_or_else(|| nicecooling_api::Error::not_found(format!("no sensor {id}")))?;
            Some(Assignment::new(id, &view.unit).ok_or_else(|| {
                nicecooling_api::Error::invalid(format!(
                    "the unit {} of {id} has no device unit code",
                    view.unit
                ))
            })?)
        };
        let sensor_id = assignment.as_ref().map(|a| a.sensor.clone());
        self.device
            .software_slots()
            .assign(slot, assignment)
            .map_err(|_| {
                nicecooling_api::Error::invalid(format!(
                    "slot {slot} is not between 1 and {SLOT_COUNT}"
                ))
            })?;
        self.hub
            .remember_software_sensor(&self.device.id(), slot, sensor_id.as_deref());
        Ok(())
    }

    #[zbus(out_args("backups"))]
    async fn list_backups(&self) -> Result<Vec<(String, u64, String, u16, String)>> {
        Ok(self
            .device
            .list_backups()?
            .into_iter()
            .map(|b| (b.id, b.created_us, b.sha256, b.firmware, b.reason))
            .collect())
    }

    #[zbus(out_args("backup"))]
    async fn create_backup(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<String> {
        self.authorize(connection, &header, "CreateBackup", None)
            .await?;
        self.device.create_backup()
    }

    #[zbus(out_args("firmware", "report3", "report5", "report8"))]
    async fn get_backup(&self, backup: &str) -> Result<(u16, Vec<u8>, Vec<u8>, Vec<u8>)> {
        let set = self.device.get_backup(backup)?;
        let firmware = self.device.backup_firmware(backup)?;
        Ok((firmware, set.report3, set.report5, set.report8))
    }

    #[zbus(out_args("backup"))]
    async fn import_backup(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        firmware: u16,
        report3: Vec<u8>,
        report5: Vec<u8>,
        report8: Vec<u8>,
    ) -> Result<String> {
        self.authorize(connection, &header, "ImportBackup", None)
            .await?;
        self.device.import_backup(
            firmware,
            &BackupSet {
                report3,
                report5,
                report8,
            },
        )
    }

    async fn restore_backup(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        backup: &str,
    ) -> Result<()> {
        self.authorize(connection, &header, "RestoreBackup", None)
            .await?;
        let report3 = self.device.restore_backup(backup).await?;
        self.hub.store_user_write(&self.device, &report3);
        Ok(())
    }

    async fn factory_reset(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> Result<()> {
        self.authorize(connection, &header, "FactoryReset", None)
            .await?;
        self.device.factory_reset().await
    }

    #[zbus(out_args("proposals"))]
    async fn get_device_response_proposals(&self) -> Result<Vec<(String, String, String, String)>> {
        Ok(proposals::rows(
            &self.device.get_fields()?,
            &self.hub.alarm_responses_for(&self.device.id()),
        ))
    }

    #[zbus(signal)]
    async fn external_change(emitter: &SignalEmitter<'_>, fields: Vec<String>) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn write_failed(
        emitter: &SignalEmitter<'_>,
        error: String,
        backup: String,
    ) -> zbus::Result<()>;
}

/// Puts the object of a device on the bus and starts the task that emits its signals.
pub(crate) async fn register(
    connection: &Connection,
    hub: &Arc<Hub>,
    device: &Arc<Attached>,
) -> zbus::Result<()> {
    let path = object(Kind::Leakshield, &device.id());
    connection
        .object_server()
        .at(path.clone(), Leakshield::new(hub.clone(), device.clone()))
        .await?;
    // The task holds the receivers only, so it ends when the device goes.
    hub.spawn(emit(
        connection.clone(),
        path,
        device.subscribe(),
        device.take_events(),
    ));
    Ok(())
}

async fn next_event(
    events: &mut Option<mpsc::UnboundedReceiver<DeviceEvent>>,
) -> Option<DeviceEvent> {
    match events {
        Some(rx) => rx.recv().await,
        None => std::future::pending().await,
    }
}

async fn emit(
    connection: Connection,
    path: OwnedObjectPath,
    mut info: tokio::sync::watch::Receiver<Info>,
    mut events: Option<mpsc::UnboundedReceiver<DeviceEvent>>,
) {
    let mut last = info.borrow_and_update().clone();
    loop {
        let Ok(iface) = connection
            .object_server()
            .interface::<_, Leakshield>(path.clone())
            .await
        else {
            return;
        };
        let emitter = iface.signal_emitter();
        tokio::select! {
            changed = info.changed() => {
                if changed.is_err() {
                    return;
                }
                let now = info.borrow_and_update().clone();
                let this = iface.get().await;
                if now.firmware != last.firmware {
                    let _ = this.firmware_changed(emitter).await;
                }
                if now.connected != last.connected {
                    let _ = this.connected_changed(emitter).await;
                }
                if now.read_only_reason != last.read_only_reason {
                    let _ = this.writable_changed(emitter).await;
                    let _ = this.read_only_reason_changed(emitter).await;
                }
                if now.mode != last.mode {
                    let _ = this.mode_changed(emitter).await;
                }
                if now.running_action != last.running_action {
                    let _ = this.running_action_changed(emitter).await;
                }
                if now.feed_state != last.feed_state {
                    let _ = this.feed_state_changed(emitter).await;
                }
                if now.last_feed != last.last_feed {
                    let _ = this.last_feed_changed(emitter).await;
                }
                if now.active_slot != last.active_slot {
                    let _ = this.active_slot_changed(emitter).await;
                }
                if now.failed_write_backup != last.failed_write_backup {
                    let _ = this.failed_write_backup_changed(emitter).await;
                }
                last = now;
            }
            event = next_event(&mut events) => {
                match event {
                    Some(DeviceEvent::WriteFailed { error, backup }) => {
                        let _ = Leakshield::write_failed(emitter, error, backup).await;
                    }
                    Some(DeviceEvent::ExternalChange { fields }) => {
                        let _ = Leakshield::external_change(emitter, fields).await;
                    }
                    None => events = None,
                }
            }
        }
    }
}
