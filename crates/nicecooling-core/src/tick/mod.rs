//! The control tick. Every second the daemon runs the stages in order.
//! Each stage reads and writes the `TickContext`. A stage keeps its own state between ticks.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use crate::calibration::CalibrationTable;
use crate::curve::Curve;
use crate::derived::DerivedSensor;
use crate::loops::Loop;
use crate::warnings::Warnings;

pub mod stage_alarms;
pub mod stage_curve;
pub mod stage_derive;
pub mod stage_floor;
pub mod stage_override;
pub mod stage_read;
pub mod stage_reconcile;
pub mod stage_rules;
pub mod stage_step;

#[cfg(test)]
mod tests;

pub use stage_reconcile::{ChannelIo, Readback, percent_to_raw};

/// A Duty that is not finite is full speed: a fan at 100% is the safe failure.
pub(crate) fn finite_or_full(duty: f64) -> f64 {
    if duty.is_finite() {
        duty
    } else {
        crate::curve::FAIL_SAFE_DUTY
    }
}

/// A value older than this counts as no value.
pub const STALE_AFTER: Duration = Duration::from_secs(3);

/// A value of the value table with the time its source published it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub value: f64,
    pub at: Duration,
}

/// The value table as the tick sees it. A call returns at once and never waits on a device.
pub trait ValueSource: Send {
    fn snapshot(&self) -> BTreeMap<String, Reading>;
}

#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    /// The Channel stays on its Hardware curve.
    Hardware,
    Curve(Curve),
    /// A fixed Duty in percent.
    Fixed(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChannelSetup {
    pub id: String,
    pub control: Control,
}

/// A Loop with its resolved Calibration table.
#[derive(Clone, Debug, PartialEq)]
pub struct LoopSetup {
    pub config: Loop,
    pub table: Option<CalibrationTable>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TickConfig {
    pub channels: Vec<ChannelSetup>,
    pub derived: Vec<DerivedSensor>,
    pub loops: Vec<LoopSetup>,
}

/// What the stages know about one Channel in this tick.
#[derive(Clone, Debug, Default)]
pub struct ChannelTick {
    /// The Duty that the Curve or the fixed Duty asks for. Stage 5.
    pub target: Option<f64>,
    /// The Curve Input has no value. Stage 5.
    pub lost_input: bool,
    /// The Duty to write. `None` means the Channel stays on its Hardware curve. Stages 6 to 8.
    pub duty: Option<f64>,
    pub override_active: bool,
    /// The Duty of the previous tick. It carries over.
    pub last_duty: Option<f64>,
}

#[derive(Default)]
pub struct TickContext {
    pub now: Duration,
    pub config: TickConfig,
    /// Sensor values that are not stale, including Derived sensors and calibrated flows.
    pub values: BTreeMap<String, f64>,
    /// Calibrated flows above the last point of their table.
    pub out_of_range: BTreeSet<String>,
    pub channels: BTreeMap<String, ChannelTick>,
    pub warnings: Warnings,
    /// Device errors of this tick, as text.
    pub errors: Vec<String>,
}

impl TickContext {
    pub fn new(config: TickConfig) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    fn begin(&mut self, now: Duration) {
        self.now = now;
        self.values.clear();
        self.out_of_range.clear();
        self.errors.clear();
        for tick in self.channels.values_mut() {
            tick.target = None;
            tick.lost_input = false;
            tick.duty = None;
            tick.override_active = false;
        }
    }

    fn finish(&mut self) {
        for tick in self.channels.values_mut() {
            tick.last_duty = tick.duty;
        }
    }

    pub fn channel(&mut self, id: &str) -> &mut ChannelTick {
        self.channels.entry(id.to_owned()).or_default()
    }
}

pub trait Stage: Send {
    fn name(&self) -> &'static str;
    fn run(&mut self, ctx: &mut TickContext);
}

pub struct Pipeline {
    stages: Vec<Box<dyn Stage>>,
}

impl Pipeline {
    pub fn new(stages: Vec<Box<dyn Stage>>) -> Self {
        Self { stages }
    }

    /// Stages 1 to 9. The daemon appends stage 10, the Feed message (it needs the devices), and
    /// stage 11, the watchdog ping.
    pub fn standard(source: Box<dyn ValueSource>, io: Box<dyn ChannelIo>) -> Self {
        Self::new(vec![
            Box::new(stage_read::ReadStage::new(source)),
            Box::new(stage_derive::DeriveStage),
            Box::new(stage_alarms::AlarmsStage),
            Box::new(stage_rules::RulesStage),
            Box::new(stage_curve::CurveStage::default()),
            Box::new(stage_step::StepStage),
            Box::new(stage_floor::FloorStage),
            Box::new(stage_override::OverrideStage),
            Box::new(stage_reconcile::ReconcileStage::new(io)),
        ])
    }

    pub fn push(&mut self, stage: Box<dyn Stage>) {
        self.stages.push(stage);
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.stages.iter().map(|s| s.name()).collect()
    }

    pub fn run(&mut self, ctx: &mut TickContext, now: Duration) {
        ctx.begin(now);
        for stage in &mut self.stages {
            stage.run(ctx);
        }
        ctx.finish();
    }
}
