//! Source tasks. Each device is read in its own task that publishes to the value table.

pub mod hotplug;
pub mod hwmon;
pub mod leakshield_bulk;
pub mod leakshield_hid;
