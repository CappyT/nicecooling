use std::time::Duration;

use super::{STALE_AFTER, Stage, TickContext, ValueSource};

/// Stage 1: read the value table. A stale value counts as no value. After a pause of the tick, a
/// value from before the pause counts as no value too, whatever its age, in every later tick.
pub struct ReadStage {
    source: Box<dyn ValueSource>,
}

impl ReadStage {
    pub fn new(source: Box<dyn ValueSource>) -> Self {
        Self { source }
    }
}

impl Stage for ReadStage {
    fn name(&self) -> &'static str {
        "read"
    }

    fn run(&mut self, ctx: &mut TickContext) {
        ctx.leakshields = self.source.leakshields();
        ctx.processes = self.source.processes();
        let since = ctx.pause_end.unwrap_or(Duration::ZERO);
        for (id, reading) in self.source.snapshot() {
            // A value that is not finite counts as no value, as a stale one does.
            if reading.value.is_finite()
                && reading.at >= since
                && ctx.now.saturating_sub(reading.at) <= STALE_AFTER
            {
                ctx.values.insert(id, reading.value);
            }
        }
    }
}
