//! The NiceCooling daemon as a library. The binary and the tests share it.

pub mod alarms;
pub mod authz;
pub mod daemon;
pub mod dbus;
pub mod firmware;
pub mod fleet;
pub mod leakshield;
pub mod lease;
pub mod profiles;
pub mod rescan;
pub mod rules;
pub mod sleep;
pub mod snapshot;
pub mod sources;
pub mod state;
pub mod tick_runner;
pub mod value_table;
pub mod watchdog;
