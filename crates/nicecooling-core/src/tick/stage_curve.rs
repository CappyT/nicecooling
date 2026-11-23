use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

use super::{Control, Stage, TickContext};

/// Stage 5: compute each Curve from its smoothed Input.
/// A fixed Duty is its own target. A Channel on its Hardware curve has none.
#[derive(Default)]
pub struct CurveStage {
    windows: BTreeMap<String, VecDeque<(Duration, f64)>>,
}

impl Stage for CurveStage {
    fn name(&self) -> &'static str {
        "curve"
    }

    fn run(&mut self, ctx: &mut TickContext) {
        let now = ctx.now;
        for setup in &ctx.config.channels {
            let tick = ctx.channels.entry(setup.id.clone()).or_default();
            match &setup.control {
                Control::Hardware => {
                    self.windows.remove(&setup.id);
                }
                Control::Fixed(duty) => tick.target = Some(*duty),
                Control::Curve(curve) => {
                    let window = self.windows.entry(setup.id.clone()).or_default();
                    let Some(value) = ctx.values.get(&curve.input) else {
                        tick.lost_input = true;
                        continue;
                    };
                    window.push_back((now, *value));
                    let width = Duration::from_secs_f64(curve.smoothing_s.max(0.0));
                    // Keep the newest sample, so a width of 0 reads the current value.
                    while window.len() > 1 && now.saturating_sub(window[0].0) >= width {
                        window.pop_front();
                    }
                    let mean = window.iter().map(|s| s.1).sum::<f64>() / window.len() as f64;
                    tick.target = Some(curve.duty_at(mean));
                }
            }
        }
    }
}
