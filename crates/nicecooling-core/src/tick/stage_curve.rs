use std::collections::{BTreeMap, VecDeque};
use std::time::Duration;

use super::{Control, Stage, TickContext, finite_or_full};

/// Stage 5: compute each Curve from its smoothed Input.
/// A fixed Duty is its own target. A Channel on its Hardware curve has none.
/// A Curve also reports the mean of its window as `input`. Lost Input, a pause of the tick and a new
/// Input sensor clear the window, so a mean never holds stale samples or samples of two sensors.
#[derive(Default)]
pub struct CurveStage {
    windows: BTreeMap<String, Window>,
}

/// The samples of one Channel and the sensor that gave them.
#[derive(Default)]
struct Window {
    input: String,
    samples: VecDeque<(Duration, f64)>,
}

impl Stage for CurveStage {
    fn name(&self) -> &'static str {
        "curve"
    }

    fn run(&mut self, ctx: &mut TickContext) {
        let now = ctx.now;
        if ctx.after_pause {
            self.windows.clear();
        }
        for setup in &ctx.config.channels {
            let tick = ctx.channels.entry(setup.id.clone()).or_default();
            match &setup.control {
                Control::Hardware => {
                    self.windows.remove(&setup.id);
                }
                Control::Fixed(duty) => tick.target = Some(finite_or_full(*duty)),
                Control::Curve(curve) => {
                    let window = self.windows.entry(setup.id.clone()).or_default();
                    if window.input != curve.input {
                        window.input.clone_from(&curve.input);
                        window.samples.clear();
                    }
                    let Some(value) = ctx.values.get(&curve.input) else {
                        window.samples.clear();
                        tick.lost_input = true;
                        continue;
                    };
                    let samples = &mut window.samples;
                    samples.push_back((now, *value));
                    let width = Duration::from_secs_f64(curve.smoothing_s.max(0.0));
                    // Keep the newest sample, so a width of 0 reads the current value.
                    while samples.len() > 1 && now.saturating_sub(samples[0].0) >= width {
                        samples.pop_front();
                    }
                    let mean = samples.iter().map(|s| s.1).sum::<f64>() / samples.len() as f64;
                    tick.input = Some(mean);
                    tick.target = Some(finite_or_full(curve.duty_at(mean)));
                }
            }
        }
    }
}
