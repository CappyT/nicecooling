use super::{Stage, TickContext};

/// Stage 10: send the Feed message of each Loop. A stub: task 5.8 replaces it.
pub struct FeedStage;

impl Stage for FeedStage {
    fn name(&self) -> &'static str {
        "feed"
    }

    fn run(&mut self, _ctx: &mut TickContext) {}
}
