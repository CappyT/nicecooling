//! The Feed message: stage 10 of the tick computes the message of each Loop with a Leakshield,
//! and a task sends it. The tick never waits on the device (design D6).

use std::collections::BTreeMap;
use std::sync::Arc;

use nicecooling_core::tick::{LoopSetup, Stage, TickContext};
use nicecooling_protocol::device::DeviceError;
use nicecooling_protocol::feed::{FeedMessage, UNIT_LITERS_PER_HOUR, UNIT_SPEED_RPM};
use tokio::sync::watch;

use super::{DeviceIo, blocking};

pub const SLOT_PUMP_SPEED: usize = 0;
pub const SLOT_FLOW: usize = 1;

type Messages = BTreeMap<String, FeedMessage>;

/// Sensor id of the tachometer of a Pump channel: `nct6798_pwm6` -> `nct6798_fan6`.
pub(crate) fn tach_sensor(channel: &str) -> Option<String> {
    let (chip, number) = channel.rsplit_once("_pwm")?;
    Some(format!("{chip}_fan{number}"))
}

/// Pump speed in rpm and flow in L/h of a Loop. The pump speed is the highest tachometer value of
/// its Pump channels. The flow is the calibrated flow of the Loop (`<loop>_flow`, stage 2), not
/// the raw rpm of its flow sensor.
pub fn feed_for_loop(setup: &LoopSetup, values: &BTreeMap<String, f64>) -> FeedMessage {
    let mut message = FeedMessage::default();
    let speed = setup
        .config
        .pump_channels
        .iter()
        .filter_map(|c| values.get(&tach_sensor(c)?))
        .copied()
        .filter(|v| v.is_finite())
        .fold(None, |best: Option<f64>, v| {
            Some(best.map_or(v, |b| b.max(v)))
        });
    if let Some(rpm) = speed {
        message.set(SLOT_PUMP_SPEED, rpm.round() as i16, UNIT_SPEED_RPM);
    }
    let flow = values
        .get(&setup.config.flow_id())
        .filter(|v| v.is_finite());
    if let Some(lph) = flow {
        message.set(SLOT_FLOW, (lph * 10.0).round() as i16, UNIT_LITERS_PER_HOUR);
    }
    message
}

/// Stage 10. It runs for each Loop that has a Leakshield and leaves the other Loops alone.
pub struct FeedStage {
    out: watch::Sender<Messages>,
}

/// The receiving end of the stage, held by the sender task.
pub struct FeedLink {
    rx: watch::Receiver<Messages>,
}

pub fn feed_channel() -> (FeedStage, FeedLink) {
    let (out, rx) = watch::channel(Messages::new());
    (FeedStage { out }, FeedLink { rx })
}

impl Stage for FeedStage {
    fn name(&self) -> &'static str {
        "feed"
    }

    fn run(&mut self, ctx: &mut TickContext) {
        let messages: Messages = ctx
            .config
            .loops
            .iter()
            .filter_map(|l| {
                let id = l.config.leakshield.clone()?;
                Some((id, feed_for_loop(l, &ctx.values)))
            })
            .collect();
        if !messages.is_empty() {
            self.out.send_replace(messages);
        }
    }
}

fn send_all(
    messages: &Messages,
    devices: &BTreeMap<String, DeviceIo>,
    report: &mut impl FnMut(&str, Result<(), DeviceError>),
) {
    for (id, message) in messages {
        if let Some(io) = devices.get(id) {
            report(id, blocking(|| io.send_feed(message)));
        }
    }
}

impl FeedLink {
    /// Sends the messages of the last tick, unless a call sent them already. `report` gets each result.
    pub fn flush(
        &mut self,
        devices: &BTreeMap<String, DeviceIo>,
        report: &mut impl FnMut(&str, Result<(), DeviceError>),
    ) {
        if self.rx.has_changed().unwrap_or(false) {
            let messages = self.rx.borrow_and_update().clone();
            send_all(&messages, devices, report);
        }
    }

    /// Sends the Feed message of every tick until the stage is dropped.
    pub async fn run(
        mut self,
        devices: Arc<BTreeMap<String, DeviceIo>>,
        mut report: impl FnMut(&str, Result<(), DeviceError>),
    ) {
        while self.rx.changed().await.is_ok() {
            let messages = self.rx.borrow_and_update().clone();
            send_all(&messages, &devices, &mut report);
        }
    }
}
