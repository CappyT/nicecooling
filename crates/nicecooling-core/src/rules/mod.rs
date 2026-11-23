//! Profile rules and Rule groups (`docs/api.md`, section Rule groups). The kinds are a sensor
//! comparison with a hold time, a process list, weekdays with a time window and an interval
//! timer. There is no hotkey rule: Wayland has no standard global shortcut.

pub mod group;
pub mod interval;
pub mod process;
pub mod sensor;
pub mod time;
pub mod winner;
