use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::time::Duration;

use super::{Stage, TickContext};

/// A read-back is not reliable this long after a write: the driver caches readings for about 1.5 s.
const CACHE_GRACE: Duration = Duration::from_secs(2);
const MISMATCH_WINDOW: Duration = Duration::from_secs(60);
const MISMATCHES_FOR_WARNING: usize = 3;

/// `pwmN` and `pwmN_enable` of a Channel as the chip reports them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Readback {
    pub raw: u8,
    pub enable: u32,
}

impl Readback {
    /// A Channel in manual mode at raw 255 reads `pwmN_enable` 0 on `nct6775`.
    pub fn is_manual(self) -> bool {
        self.enable == 1 || (self.enable == 0 && self.raw == 255)
    }
}

/// The device side of the reconcile stage.
pub trait ChannelIo: Send {
    fn read(&mut self, channel: &str) -> io::Result<Readback>;
    /// Writes `pwmN`. With `set_manual` it first sets `pwmN_enable` to 1.
    fn write(&mut self, channel: &str, raw: u8, set_manual: bool) -> io::Result<()>;
}

/// Duty to raw value: `round(duty * 255 / 100)`, half away from zero, clamped to 0-255.
pub fn percent_to_raw(duty: f64) -> u8 {
    (duty * 255.0 / 100.0).round().clamp(0.0, 255.0) as u8
}

#[derive(Default)]
struct ChannelState {
    /// The raw value of the last write and its time.
    written: Option<(u8, Duration)>,
    mismatches: VecDeque<Duration>,
}

/// Stage 9: reconcile and write each Channel.
/// The read-back is compared with the value the daemon wrote last, so that a Duty
/// that changes every tick is not a mismatch. A read-back inside the cache grace is skipped.
pub struct ReconcileStage {
    io: Box<dyn ChannelIo>,
    state: BTreeMap<String, ChannelState>,
}

impl ReconcileStage {
    pub fn new(io: Box<dyn ChannelIo>) -> Self {
        Self {
            io,
            state: BTreeMap::new(),
        }
    }
}

impl Stage for ReconcileStage {
    fn name(&self) -> &'static str {
        "reconcile"
    }

    fn run(&mut self, ctx: &mut TickContext) {
        let now = ctx.now;
        for (id, tick) in &ctx.channels {
            let Some(duty) = tick.duty else {
                // Back on the Hardware curve: the next takeover writes the mode again.
                self.state.remove(id);
                continue;
            };
            let desired = percent_to_raw(duty);
            let state = self.state.entry(id.clone()).or_default();
            let mut set_manual = state.written.is_none();
            let mut rewrite = false;

            if let Some((expected, at)) = state.written
                && now.saturating_sub(at) >= CACHE_GRACE
            {
                match self.io.read(id) {
                    Ok(back) if !back.is_manual() || back.raw != expected => {
                        state.mismatches.push_back(now);
                        set_manual = !back.is_manual();
                        rewrite = true;
                    }
                    Ok(_) => {}
                    Err(e) => ctx.errors.push(format!("{id}: read-back failed: {e}")),
                }
            }
            while state
                .mismatches
                .front()
                .is_some_and(|t| now.saturating_sub(*t) >= MISMATCH_WINDOW)
            {
                state.mismatches.pop_front();
            }
            if state.mismatches.len() >= MISMATCHES_FOR_WARNING {
                ctx.warnings.raise("another-writer", id);
            } else {
                ctx.warnings.clear("another-writer", id);
            }

            let changed = state.written.is_none_or(|(raw, _)| raw != desired);
            if changed || rewrite {
                match self.io.write(id, desired, set_manual) {
                    Ok(()) => state.written = Some((desired, now)),
                    Err(e) => ctx.errors.push(format!("{id}: write failed: {e}")),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_to_raw_rounding() {
        assert_eq!(percent_to_raw(50.0), 128);
        assert_eq!(percent_to_raw(0.2), 1);
        assert_eq!(percent_to_raw(0.0), 0);
        assert_eq!(percent_to_raw(100.0), 255);
        assert_eq!(percent_to_raw(150.0), 255);
        assert_eq!(percent_to_raw(-5.0), 0);
    }

    #[test]
    fn readback_manual_detection() {
        assert!(
            Readback {
                raw: 255,
                enable: 0
            }
            .is_manual()
        );
        assert!(Readback { raw: 10, enable: 1 }.is_manual());
        assert!(
            !Readback {
                raw: 100,
                enable: 0
            }
            .is_manual()
        );
        assert!(
            !Readback {
                raw: 255,
                enable: 5
            }
            .is_manual()
        );
    }
}
