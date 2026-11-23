use super::{Stage, TickContext};

/// Stage 8: apply the overrides. They set 100% after the step limit and the Pump floor.
/// Lost Input ships here. The pump stop override belongs to task 6.3.
pub struct OverrideStage;

impl Stage for OverrideStage {
    fn name(&self) -> &'static str {
        "override"
    }

    fn run(&mut self, ctx: &mut TickContext) {
        let TickContext {
            channels, warnings, ..
        } = ctx;
        for (id, tick) in channels.iter_mut() {
            if tick.lost_input {
                tick.duty = Some(100.0);
                tick.override_active = true;
                warnings.raise("lost-input", id);
            } else {
                warnings.clear("lost-input", id);
            }
        }
    }
}
