use super::{Stage, TickContext};

/// Stage 2: compute Derived sensors and calibrated flows.
pub struct DeriveStage;

impl Stage for DeriveStage {
    fn name(&self) -> &'static str {
        "derive"
    }

    fn run(&mut self, ctx: &mut TickContext) {
        let derived: Vec<_> = ctx
            .config
            .derived
            .iter()
            .map(|d| (d.id.clone(), d.value(&ctx.values)))
            .collect();
        for (id, value) in derived {
            if let Some(value) = value.filter(|v| v.is_finite()) {
                ctx.values.insert(id, value);
            }
        }
        let flows: Vec<_> = ctx
            .config
            .loops
            .iter()
            .filter_map(|l| {
                let rpm = ctx.values.get(l.config.flow_sensor.as_ref()?)?;
                Some((l.config.flow_id(), l.table.as_ref()?.flow(*rpm)))
            })
            .collect();
        for (id, flow) in flows {
            if flow.out_of_range {
                ctx.out_of_range.insert(id.clone());
            }
            if flow.lph.is_finite() {
                ctx.values.insert(id, flow.lph);
            }
        }
    }
}
