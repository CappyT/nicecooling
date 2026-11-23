use super::{Stage, TickContext};

/// Stage 3: evaluate the Alarms. A stub: tasks 6.x replace it.
pub struct AlarmsStage;

impl Stage for AlarmsStage {
    fn name(&self) -> &'static str {
        "alarms"
    }

    fn run(&mut self, _ctx: &mut TickContext) {}
}
