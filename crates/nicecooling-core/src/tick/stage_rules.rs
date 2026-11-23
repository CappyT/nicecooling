use super::{Stage, TickContext};

/// Stage 4: evaluate the Rule groups. A stub: tasks 7.x replace it.
pub struct RulesStage;

impl Stage for RulesStage {
    fn name(&self) -> &'static str {
        "rules"
    }

    fn run(&mut self, _ctx: &mut TickContext) {}
}
