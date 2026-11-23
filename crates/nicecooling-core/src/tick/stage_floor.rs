use super::{Stage, TickContext};

/// Stage 7: apply the Pump floor. It clamps a Curve Duty and a fixed Duty alike.
pub struct FloorStage;

impl Stage for FloorStage {
    fn name(&self) -> &'static str {
        "floor"
    }

    fn run(&mut self, ctx: &mut TickContext) {
        for setup in &ctx.config.loops {
            for pump in &setup.config.pump_channels {
                if let Some(duty) = ctx.channels.get_mut(pump).and_then(|t| t.duty.as_mut()) {
                    *duty = duty.max(setup.config.pump_floor);
                }
            }
        }
    }
}
