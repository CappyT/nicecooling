//! The typed proxy of the interface `io.github.cappyt.NiceCooling1.Alarms`.

use std::collections::HashMap;

use zbus::OwnedVariant as OwnedValue;

/// (instance id, event id, severity, source id, response, raised us, poweroff at us, details)
pub type ActiveAlarm = (
    String,
    String,
    String,
    String,
    String,
    u64,
    u64,
    HashMap<String, OwnedValue>,
);

/// (instance id, event id, severity, source id, response, raised us, cleared us, outcome, details)
pub type HistoryRow = (
    String,
    String,
    String,
    String,
    String,
    u64,
    u64,
    String,
    HashMap<String, OwnedValue>,
);

#[zbus::proxy(
    interface = "io.github.cappyt.NiceCooling1.Alarms",
    default_service = "io.github.cappyt.NiceCooling1",
    default_path = "/io/github/cappyt/NiceCooling1"
)]
pub trait Alarms {
    #[zbus(property)]
    fn active(&self) -> zbus::Result<Vec<ActiveAlarm>>;

    /// (event id, response, delay s)
    fn get_responses(&self) -> zbus::Result<Vec<(String, String, u32)>>;
    fn set_response(&self, event: &str, response: &str, delay_s: u32) -> zbus::Result<()>;
    fn get_history(&self, since_us: u64, limit: u32) -> zbus::Result<Vec<HistoryRow>>;
    fn cancel_poweroff(&self, instance: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    fn alarm_raised(
        &self,
        instance: String,
        event: String,
        severity: String,
        source: String,
        response: String,
        poweroff_at_us: u64,
        details: HashMap<String, OwnedValue>,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn alarm_cleared(&self, instance: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn poweroff_cancelled(&self, instance: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn poweroff_failed(&self, instance: String, error: String) -> zbus::Result<()>;
}
