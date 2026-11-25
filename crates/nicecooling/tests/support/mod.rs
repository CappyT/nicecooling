#![allow(dead_code)]

pub mod bus;
pub mod fake_daemon;
pub mod fake_leakshield;
pub mod fake_notification_server;
pub mod fake_systemd;
pub mod fake_tray_host;
pub mod fixtures;
pub mod graphs;
pub mod harness;
pub mod labels;
pub mod mqtt_broker;
pub mod smtp_server;

/// Point the session bus lookup of `ksni` and `notify-rust` at a private bus. Both read
/// `DBUS_SESSION_BUS_ADDRESS` when they connect. Hold `SESSION_LOCK` for the whole test.
pub static SESSION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

pub fn use_as_session_bus(bus: &bus::PrivateBus) {
    // SAFETY: the caller holds `SESSION_LOCK`, so no other test of this process reads or writes
    // the environment while the variable changes.
    unsafe { std::env::set_var("DBUS_SESSION_BUS_ADDRESS", &bus.address) };
}
