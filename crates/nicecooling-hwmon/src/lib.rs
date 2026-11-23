//! The sysfs layer: hwmon devices, their identity, Board files and discovery.

pub mod board;
pub mod discovery;
pub mod identity;
pub mod sysfs;

#[cfg(any(test, feature = "fake"))]
pub mod fake;
