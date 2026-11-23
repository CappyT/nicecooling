use std::collections::{BTreeMap, VecDeque};
use std::io;
use std::time::Duration;

use super::{Stage, TickContext};

/// The driver caches readings for about 1.5 s. A value written this recently can still read back
/// as an older one.
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
    /// False when the device has no BIOS state to give the Channel back to. The stage then leaves
    /// the Channel on its Hardware curve.
    fn can_take_over(&self, _channel: &str) -> bool {
        true
    }
    /// Writes `pwmN`. With `set_manual` it first sets `pwmN_enable` to 1.
    fn write(&mut self, channel: &str, raw: u8, set_manual: bool) -> io::Result<()>;
    /// Gives the Channel back to its BIOS state. The stage calls it once for each Channel that
    /// was under control and is not any more. An `Err` makes the stage call it again next tick.
    fn release(&mut self, channel: &str) -> io::Result<()>;
}

/// Duty to raw value: `round(duty * 255 / 100)`, half away from zero, clamped to 0-255.
/// A Duty that is not finite is full speed.
pub fn percent_to_raw(duty: f64) -> u8 {
    if !duty.is_finite() {
        return 255;
    }
    (duty * 255.0 / 100.0).round().clamp(0.0, 255.0) as u8
}

#[derive(Default)]
struct ChannelState {
    /// The raw value of the last write.
    last: Option<u8>,
    /// The writes of the last `CACHE_GRACE`.
    recent: VecDeque<(u8, Duration)>,
    /// The time of the last write that set manual mode or corrected a mismatch. A read-back
    /// right after it can still show the state before the write.
    settle_from: Option<Duration>,
    mismatches: VecDeque<Duration>,
}

/// Stage 9: reconcile and write each Channel.
/// Every tick reads the Channel back and checks that it is in manual mode at a raw value that the
/// daemon wrote in the last `CACHE_GRACE` (or the last one it wrote). A Duty that changes every tick
/// is not a mismatch, and a mode reset during a ramp is corrected on the next tick.
/// A Channel that is not under control any more goes back to its BIOS state. A Channel that the
/// device cannot take over stays on its Hardware curve.
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

    fn release_left_channels(&mut self, ctx: &mut TickContext) {
        let left: Vec<String> = self
            .state
            .keys()
            .filter(|id| ctx.channels.get(*id).is_none_or(|t| t.duty.is_none()))
            .cloned()
            .collect();
        for id in left {
            match self.io.release(&id) {
                Ok(()) => {
                    self.state.remove(&id);
                    ctx.warnings.clear("another-writer", &id);
                }
                Err(e) => ctx.errors.push(format!("{id}: release failed: {e}")),
            }
        }
    }
}

impl Stage for ReconcileStage {
    fn name(&self) -> &'static str {
        "reconcile"
    }

    fn run(&mut self, ctx: &mut TickContext) {
        let now = ctx.now;
        self.release_left_channels(ctx);
        for (id, tick) in &mut ctx.channels {
            if tick.duty.is_some() && !self.io.can_take_over(id) {
                tick.duty = None;
                tick.override_active = false;
            }
            let Some(duty) = tick.duty else { continue };
            let desired = percent_to_raw(duty);
            let state = self.state.entry(id.clone()).or_default();
            state
                .recent
                .retain(|(_, at)| now.saturating_sub(*at) <= CACHE_GRACE);
            let mut set_manual = state.last.is_none();
            let mut rewrite = false;

            if state.last.is_some() {
                match self.io.read(id) {
                    Ok(back) => {
                        let settling = state
                            .settle_from
                            .is_some_and(|at| now.saturating_sub(at) < CACHE_GRACE);
                        let known = state.last == Some(back.raw)
                            || state.recent.iter().any(|(raw, _)| *raw == back.raw);
                        if !settling && (!back.is_manual() || !known) {
                            state.mismatches.push_back(now);
                            set_manual = !back.is_manual();
                            rewrite = true;
                        }
                    }
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

            if state.last != Some(desired) || rewrite {
                match self.io.write(id, desired, set_manual) {
                    Ok(()) => {
                        state.last = Some(desired);
                        state.recent.push_back((desired, now));
                        if set_manual || rewrite {
                            state.settle_from = Some(now);
                        }
                    }
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
    fn non_finite_duty_is_full_speed() {
        for duty in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            assert_eq!(percent_to_raw(duty), 255, "{duty}");
        }
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
