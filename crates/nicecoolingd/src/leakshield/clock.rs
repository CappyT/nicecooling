//! The device clock. The device keeps seconds since 2009-01-01 00:00 UTC and has no battery.

use nicecooling_protocol::device::DeviceError;
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report1::Report1;
use nicecooling_protocol::report2::Report2;

use super::DeviceIo;

/// Seconds from 1970-01-01 to 2009-01-01.
pub const DEVICE_EPOCH_UNIX: i64 = 1_230_768_000;

/// The daemon sets the clock by itself only when the device differs from the PC by more than this.
pub const AUTO_SYNC_THRESHOLD_S: i64 = 60;

/// `None` before the epoch or after the end of the u32 range.
pub fn device_seconds(unix_seconds: i64) -> Option<u32> {
    u32::try_from(unix_seconds - DEVICE_EPOCH_UNIX).ok()
}

/// Device time minus PC time, in seconds.
pub fn offset(report1: &Report1, now_unix: i64) -> i64 {
    i64::from(report1.device_clock) + DEVICE_EPOCH_UNIX - now_unix
}

/// `None` when the registry of the firmware has no `set_clock` command, or the time is out of range.
pub fn command(registry: &Registry, now_unix: i64) -> Option<Report2> {
    let seconds = device_seconds(now_unix)?;
    registry
        .command("set_clock")
        .map(|c| Report2::new(c.code, seconds))
}

pub fn needs_auto_sync(offset_s: i64) -> bool {
    offset_s.abs() > AUTO_SYNC_THRESHOLD_S
}

/// Sends the clock of the PC, whatever the offset. Returns false when the registry has no command.
pub fn sync(io: &DeviceIo, registry: &Registry, now_unix: i64) -> Result<bool, DeviceError> {
    match command(registry, now_unix) {
        Some(command) => io.send_command(command).map(|()| true),
        None => Ok(false),
    }
}

/// The automatic sync: writes only when the device differs from the PC by more than 60 s.
/// Returns true when it wrote.
pub fn auto_sync(
    io: &DeviceIo,
    registry: &Registry,
    report1: &Report1,
    now_unix: i64,
) -> Result<bool, DeviceError> {
    if needs_auto_sync(offset(report1, now_unix)) {
        sync(io, registry, now_unix)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn epoch_is_2009_01_01() {
        // 14245 days of 86400 s from 1970-01-01 to 2009-01-01.
        assert_eq!(DEVICE_EPOCH_UNIX, 14_245 * 86_400);
        assert_eq!(device_seconds(DEVICE_EPOCH_UNIX + 5), Some(5));
        assert_eq!(device_seconds(DEVICE_EPOCH_UNIX - 1), None);
    }

    #[test]
    fn threshold_is_more_than_60_s() {
        assert!(!needs_auto_sync(60));
        assert!(!needs_auto_sync(-60));
        assert!(needs_auto_sync(61));
        assert!(needs_auto_sync(-61));
    }
}
