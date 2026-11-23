use super::{Control, Stage, TickContext};
use crate::curve::DEFAULT_STEP_LIMIT;

/// Stage 6: apply the Duty step limit.
/// The previous Duty is the start. Without one, the Duty goes to its target at once.
pub struct StepStage;

impl Stage for StepStage {
    fn name(&self) -> &'static str {
        "step"
    }

    fn run(&mut self, ctx: &mut TickContext) {
        for setup in &ctx.config.channels {
            let limit = match &setup.control {
                Control::Curve(curve) => curve.step_limit,
                Control::Fixed(_) => DEFAULT_STEP_LIMIT,
                Control::Hardware => continue,
            };
            let Some(tick) = ctx.channels.get_mut(&setup.id) else {
                continue;
            };
            let Some(target) = tick.target else { continue };
            tick.duty = Some(match tick.last_duty {
                Some(last) => last + (target - last).clamp(-limit, limit),
                None => target,
            });
        }
    }
}
