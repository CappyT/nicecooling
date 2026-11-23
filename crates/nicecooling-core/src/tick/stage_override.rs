use super::stage_alarms::PUMP_STOP;
use super::{Stage, TickContext};

/// Stage 8: apply the overrides. They set 100% after the step limit and the Pump floor.
/// Lost Input sets its own Channel. The pump stop Alarm sets every Channel, also a Channel on its
/// Hardware curve. When the Alarm closes, the stage leaves the Channels alone: the step limit
/// ramps a Curve down from 100%, and the reconcile stage gives the other Channels back to the
/// BIOS state.
#[derive(Default)]
pub struct OverrideStage {
    /// Every Channel of the device. A Channel of the configuration is also covered.
    channels: Vec<String>,
}

impl OverrideStage {
    pub fn new(channels: Vec<String>) -> Self {
        Self { channels }
    }
}

impl Stage for OverrideStage {
    fn name(&self) -> &'static str {
        "override"
    }

    fn run(&mut self, ctx: &mut TickContext) {
        let TickContext {
            channels,
            warnings,
            alarms,
            config,
            ..
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
        if alarms.any_active(PUMP_STOP) {
            let known = self
                .channels
                .iter()
                .chain(config.channels.iter().map(|c| &c.id));
            for id in known {
                let tick = channels.entry(id.clone()).or_default();
                tick.duty = Some(100.0);
                tick.override_active = true;
            }
        }
    }
}
