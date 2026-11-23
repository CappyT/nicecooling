//! The system local time for the time rules.

use std::time::{SystemTime, UNIX_EPOCH};

use nicecooling_core::rules::time::DayTime;

/// The local weekday and minute of a Unix time, as the C library has them for the time zone of
/// the system.
pub fn local_time(unix_s: i64) -> DayTime {
    let seconds: libc::time_t = unix_s as libc::time_t;
    // SAFETY: `localtime_r` writes only to the `tm` that we give it. A zeroed `tm` is valid.
    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        if libc::localtime_r(&seconds, &mut tm).is_null() {
            return DayTime {
                weekday: 1,
                minute: 0,
            };
        }
        tm
    };
    DayTime {
        // `tm_wday` counts from Sunday = 0. ISO has Sunday = 7.
        weekday: if tm.tm_wday == 0 { 7 } else { tm.tm_wday as u8 },
        minute: (tm.tm_hour * 60 + tm.tm_min) as u16,
    }
}

pub fn now() -> (u64, DayTime) {
    let elapsed = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (
        elapsed.as_micros() as u64,
        local_time(elapsed.as_secs() as i64),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn weekday_and_minute_are_in_range() {
        for t in [0, 86_400 * 3 + 12_345, 1_800_000_000] {
            let local = local_time(t);
            assert!((1..=7).contains(&local.weekday), "{local:?}");
            assert!(local.minute < 1440, "{local:?}");
        }
        // 2024-01-01 12:00 UTC was a Monday. Every time zone has it on Monday or Tuesday.
        let monday = local_time(1_704_110_400);
        assert!(matches!(monday.weekday, 1 | 2), "{monday:?}");
    }
}
