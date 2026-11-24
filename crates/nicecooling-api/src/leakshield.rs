//! The typed proxy of the interface `io.github.cappyt.NiceCooling1.Leakshield`.

use std::collections::HashMap;

use zbus::{OwnedObjectPath, OwnedVariant as OwnedValue, Variant as Value};

#[zbus::proxy(
    interface = "io.github.cappyt.NiceCooling1.Leakshield",
    default_service = "io.github.cappyt.NiceCooling1"
)]
pub trait Leakshield {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn serial(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn firmware(&self) -> zbus::Result<u16>;
    #[zbus(property)]
    fn structure(&self) -> zbus::Result<u16>;
    #[zbus(property)]
    fn connected(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn writable(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn read_only_reason(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn mode(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn running_action(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn clock_offset(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn software_sensor_slots(&self) -> zbus::Result<u16>;
    /// off | waiting | ok | failing
    #[zbus(property)]
    fn feed_state(&self) -> zbus::Result<String>;
    /// (time of the last good send in microseconds, pump speed in rpm, flow in L/h)
    #[zbus(property)]
    fn last_feed(&self) -> zbus::Result<(u64, f64, f64)>;
    #[zbus(property)]
    fn active_slot(&self) -> zbus::Result<u8>;
    #[zbus(property)]
    fn failed_write_backup(&self) -> zbus::Result<String>;

    /// (field id, risk class safety|low, value, writable, reason)
    fn get_fields(&self) -> zbus::Result<Vec<(String, String, OwnedValue, bool, String)>>;
    fn set_fields(&self, fields: HashMap<&str, Value<'_>>) -> zbus::Result<()>;
    /// The stored settings of a slot, as field id to value.
    fn get_stored_fields(&self, slot: u8) -> zbus::Result<HashMap<String, OwnedValue>>;
    fn set_stored_fields(&self, slot: u8, fields: HashMap<&str, Value<'_>>) -> zbus::Result<()>;
    fn set_mode(&self, mode: &str) -> zbus::Result<()>;
    fn start_action(&self, action: &str) -> zbus::Result<()>;
    fn cancel_action(&self) -> zbus::Result<()>;
    fn sync_clock(&self) -> zbus::Result<()>;
    fn reset_alarms(&self) -> zbus::Result<()>;
    fn reset_refill_history(&self) -> zbus::Result<()>;
    fn reset_event_log(&self) -> zbus::Result<()>;
    fn toggle_refill_pause(&self) -> zbus::Result<()>;
    fn reset_service_counter(&self) -> zbus::Result<()>;
    /// Counter name to value (`docs/api.md`, Leakshield counters).
    fn get_counters(&self) -> zbus::Result<HashMap<String, u64>>;
    /// (device time us, duration s, pump revolutions, volume ml), oldest first.
    fn get_refill_history(&self) -> zbus::Result<Vec<(u64, u32, u32, f64)>>;
    /// (device time us, event identifier, details)
    fn get_events(&self) -> zbus::Result<Vec<(u64, String, HashMap<String, OwnedValue>)>>;
    fn get_software_sensors(&self) -> zbus::Result<Vec<(u16, OwnedObjectPath)>>;
    fn set_software_sensor(&self, slot: u16, sensor: &OwnedObjectPath) -> zbus::Result<()>;
    /// (backup id, time us, sha256 hex, firmware, reason)
    fn list_backups(&self) -> zbus::Result<Vec<(String, u64, String, u16, String)>>;
    fn create_backup(&self) -> zbus::Result<String>;
    /// (firmware, report 3, report 5, report 8)
    fn get_backup(&self, backup: &str) -> zbus::Result<(u16, Vec<u8>, Vec<u8>, Vec<u8>)>;
    fn import_backup(
        &self,
        firmware: u16,
        report3: &[u8],
        report5: &[u8],
        report8: &[u8],
    ) -> zbus::Result<String>;
    fn restore_backup(&self, backup: &str) -> zbus::Result<()>;
    fn factory_reset(&self) -> zbus::Result<()>;
    /// (device alarm id, current Device response, proposed Device response, overlap warning id)
    fn get_device_response_proposals(&self) -> zbus::Result<Vec<(String, String, String, String)>>;

    #[zbus(signal)]
    fn external_change(&self, fields: Vec<String>) -> zbus::Result<()>;
    #[zbus(signal)]
    fn write_failed(&self, error: String, backup: String) -> zbus::Result<()>;
}
