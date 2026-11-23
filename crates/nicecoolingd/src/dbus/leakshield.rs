//! The interface `Leakshield`. Tasks 5.x fill the methods in. Until then each method replies
//! `org.freedesktop.DBus.Error.NotSupported`, and the properties come from a `LeakshieldInfo`.

use std::collections::HashMap;

use nicecooling_api::errors::Result;
use nicecooling_api::ids;
use zbus::{OwnedObjectPath, OwnedVariant, interface};

use super::not_supported_stub;

/// What the daemon knows of a Leakshield before the device access exists.
#[derive(Clone, Debug)]
pub struct LeakshieldInfo {
    pub serial: String,
    pub firmware: u16,
    pub structure: u16,
    pub connected: bool,
    /// Empty when the Leakshield is writable.
    pub read_only_reason: String,
    pub mode: String,
}

impl LeakshieldInfo {
    pub fn id(&self) -> String {
        ids::leakshield_id(&self.serial)
    }
}

pub struct Leakshield {
    info: LeakshieldInfo,
}

impl Leakshield {
    pub fn new(info: LeakshieldInfo) -> Self {
        Self { info }
    }
}

fn stub<T>(method: &str) -> Result<T> {
    Err(not_supported_stub("Leakshield", method))
}

#[allow(unused_variables)]
#[interface(name = "io.github.cappyt.NiceCooling1.Leakshield")]
impl Leakshield {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> String {
        self.info.id()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn serial(&self) -> String {
        self.info.serial.clone()
    }

    #[zbus(property)]
    fn firmware(&self) -> u16 {
        self.info.firmware
    }

    #[zbus(property)]
    fn structure(&self) -> u16 {
        self.info.structure
    }

    #[zbus(property)]
    fn connected(&self) -> bool {
        self.info.connected
    }

    #[zbus(property)]
    fn writable(&self) -> bool {
        self.info.read_only_reason.is_empty()
    }

    #[zbus(property)]
    fn read_only_reason(&self) -> String {
        self.info.read_only_reason.clone()
    }

    #[zbus(property)]
    fn mode(&self) -> String {
        self.info.mode.clone()
    }

    #[zbus(property)]
    fn running_action(&self) -> String {
        String::new()
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn clock_offset(&self) -> i64 {
        0
    }

    #[zbus(property)]
    fn software_sensor_slots(&self) -> u16 {
        12
    }

    #[zbus(out_args("fields"))]
    async fn get_fields(&self) -> Result<Vec<(String, String, OwnedVariant, bool, String)>> {
        stub("GetFields")
    }

    async fn set_fields(&self, fields: HashMap<String, OwnedVariant>) -> Result<()> {
        stub("SetFields")
    }

    async fn set_mode(&self, mode: &str) -> Result<()> {
        stub("SetMode")
    }

    async fn start_action(&self, action: &str) -> Result<()> {
        stub("StartAction")
    }

    async fn cancel_action(&self) -> Result<()> {
        stub("CancelAction")
    }

    async fn sync_clock(&self) -> Result<()> {
        stub("SyncClock")
    }

    async fn reset_refill_history(&self) -> Result<()> {
        stub("ResetRefillHistory")
    }

    async fn reset_service_counter(&self) -> Result<()> {
        stub("ResetServiceCounter")
    }

    #[zbus(out_args("events"))]
    async fn get_events(&self) -> Result<Vec<(u64, String, HashMap<String, OwnedVariant>)>> {
        stub("GetEvents")
    }

    #[zbus(out_args("slots"))]
    async fn get_software_sensors(&self) -> Result<Vec<(u16, OwnedObjectPath)>> {
        stub("GetSoftwareSensors")
    }

    async fn set_software_sensor(&self, slot: u16, sensor: OwnedObjectPath) -> Result<()> {
        stub("SetSoftwareSensor")
    }

    #[zbus(out_args("backups"))]
    async fn list_backups(&self) -> Result<Vec<(String, u64, String, u16, String)>> {
        stub("ListBackups")
    }

    #[zbus(out_args("backup"))]
    async fn create_backup(&self) -> Result<String> {
        stub("CreateBackup")
    }

    #[zbus(out_args("report3", "report5", "report8"))]
    async fn get_backup(&self, backup: &str) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        stub("GetBackup")
    }

    #[zbus(out_args("backup"))]
    async fn import_backup(
        &self,
        report3: Vec<u8>,
        report5: Vec<u8>,
        report8: Vec<u8>,
    ) -> Result<String> {
        stub("ImportBackup")
    }

    async fn restore_backup(&self, backup: &str) -> Result<()> {
        stub("RestoreBackup")
    }

    async fn factory_reset(&self) -> Result<()> {
        stub("FactoryReset")
    }

    #[zbus(out_args("proposals"))]
    async fn get_device_response_proposals(&self) -> Result<Vec<(String, String, String, String)>> {
        stub("GetDeviceResponseProposals")
    }

    #[zbus(signal)]
    async fn external_change(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        fields: Vec<String>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn write_failed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        error: String,
        backup: String,
    ) -> zbus::Result<()>;
}
