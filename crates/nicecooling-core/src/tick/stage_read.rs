use super::{STALE_AFTER, Stage, TickContext, ValueSource};

/// Stage 1: read the value table. A stale value counts as no value.
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
        for (id, reading) in self.source.snapshot() {
            if ctx.now.saturating_sub(reading.at) <= STALE_AFTER {
                ctx.values.insert(id, reading.value);
            }
        }
    }
}
