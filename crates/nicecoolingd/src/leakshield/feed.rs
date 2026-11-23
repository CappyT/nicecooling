//! The Feed message: stage 10 of the tick computes the message of each Loop with a Leakshield,
//! and a task sends it. The tick never waits on the device (design D6).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use nicecooling_core::tick::{LoopSetup, Stage, TickContext};
use nicecooling_protocol::device::DeviceError;
use nicecooling_protocol::feed::{FeedMessage, NO_VALUE, UNIT_LITERS_PER_HOUR, UNIT_SPEED_RPM};
use tokio::sync::watch;

use super::software_sensors::SoftwareSlots;
use super::{DeviceIo, blocking};

pub const SLOT_PUMP_SPEED: usize = 0;
pub const SLOT_FLOW: usize = 1;

/// Warning of a Leakshield whose firmware the registry does not cover. It gets no Feed.
pub const UNCOVERED_WARNING: &str = "leakshield-unknown-firmware";

type Messages = BTreeMap<String, FeedMessage>;

/// The values of a Feed message that the device accepted, in the units of `docs/api.md`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FeedSent {
    pub pump_speed_rpm: f64,
    pub flow_lph: f64,
}

impl FeedSent {
    fn of(message: &FeedMessage) -> Self {
        let value = |slot: usize, scale: f64| {
            let raw = message.value(slot);
            if raw == NO_VALUE {
                0.0
            } else {
                f64::from(raw) / scale
            }
        };
        Self {
            pump_speed_rpm: value(SLOT_PUMP_SPEED, 1.0),
            flow_lph: value(SLOT_FLOW, 10.0),
        }
    }
}

/// Gets the ids of the Leakshields that a Loop links, once in each tick.
pub type LinkedSink = Arc<dyn Fn(&BTreeSet<String>) + Send + Sync>;

type SendResult = Result<FeedSent, DeviceError>;

/// Sensor id of the tachometer of a Pump channel: `nct6798_pwm6` -> `nct6798_fan6`.
pub(crate) fn tach_sensor(channel: &str) -> Option<String> {
    let (chip, number) = channel.rsplit_once("_pwm")?;
    Some(format!("{chip}_fan{number}"))
}

/// A reading in slot units. A value above the range stops at the highest real value: the cast
/// alone would reach 32767, the no-value code.
fn slot_value(raw: f64) -> i16 {
    (raw.round() as i16).min(NO_VALUE - 1)
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
        message.set(SLOT_PUMP_SPEED, slot_value(rpm), UNIT_SPEED_RPM);
    }
    let flow = values
        .get(&setup.config.flow_id())
        .filter(|v| v.is_finite());
    if let Some(lph) = flow {
        message.set(SLOT_FLOW, slot_value(lph * 10.0), UNIT_LITERS_PER_HOUR);
    }
    message
}

/// Finds the Software sensor slots of a Leakshield by its id.
pub type SoftwareLookup = Arc<dyn Fn(&str) -> Option<Arc<SoftwareSlots>> + Send + Sync>;

/// Tells whether the registry covers the firmware of a Leakshield. `None`: the Leakshield is not
/// attached.
pub type CoveredLookup = Arc<dyn Fn(&str) -> Option<bool> + Send + Sync>;

/// Stage 10. It runs for each Loop that has a Leakshield and leaves the other Loops alone.
pub struct FeedStage {
    out: watch::Sender<Messages>,
    software: Option<SoftwareLookup>,
    covered: Option<CoveredLookup>,
    linked: Option<LinkedSink>,
    /// The Leakshields that hold the `UNCOVERED_WARNING` now.
    warned: BTreeSet<String>,
}

/// The receiving end of the stage, held by the sender task.
pub struct FeedLink {
    rx: watch::Receiver<Messages>,
}

/// A Feed stage that carries the pump speed and the flow only.
pub fn feed_channel() -> (FeedStage, FeedLink) {
    channel(None, None)
}

/// A Feed stage that also carries the Software sensors of each Leakshield.
pub fn feed_channel_with(software: SoftwareLookup) -> (FeedStage, FeedLink) {
    channel(Some(software), None)
}

/// A Feed stage with the Software sensors that sends no Feed to a Leakshield whose firmware the
/// registry does not cover.
pub fn feed_channel_checked(
    software: SoftwareLookup,
    covered: CoveredLookup,
) -> (FeedStage, FeedLink) {
    channel(Some(software), Some(covered))
}

fn channel(
    software: Option<SoftwareLookup>,
    covered: Option<CoveredLookup>,
) -> (FeedStage, FeedLink) {
    let (out, rx) = watch::channel(Messages::new());
    (
        FeedStage {
            out,
            software,
            covered,
            linked: None,
            warned: BTreeSet::new(),
        },
        FeedLink { rx },
    )
}

impl FeedStage {
    /// Tells `sink` in each tick which Leakshields a Loop links, so that the others show their Feed
    /// as off.
    pub fn with_linked(mut self, sink: LinkedSink) -> Self {
        self.linked = Some(sink);
        self
    }
}

impl Stage for FeedStage {
    fn name(&self) -> &'static str {
        "feed"
    }

    fn run(&mut self, ctx: &mut TickContext) {
        let mut uncovered = BTreeSet::new();
        if let Some(sink) = &self.linked {
            let linked: BTreeSet<String> = ctx
                .config
                .loops
                .iter()
                .filter_map(|l| l.config.leakshield.clone())
                .collect();
            sink(&linked);
        }
        let messages: Messages = ctx
            .config
            .loops
            .iter()
            .filter_map(|l| {
                let id = l.config.leakshield.clone()?;
                if self
                    .covered
                    .as_ref()
                    .and_then(|covered| covered(&id))
                    .is_some_and(|covered| !covered)
                {
                    uncovered.insert(id);
                    return None;
                }
                let mut message = feed_for_loop(l, &ctx.values);
                if let Some(slots) = self.software.as_ref().and_then(|find| find(&id)) {
                    slots.fill(&mut message, &ctx.values);
                }
                Some((id, message))
            })
            .collect();
        for id in &uncovered {
            ctx.warnings.raise(UNCOVERED_WARNING, id);
        }
        for id in self.warned.difference(&uncovered) {
            ctx.warnings.clear(UNCOVERED_WARNING, id);
        }
        self.warned = uncovered;
        if !messages.is_empty() {
            self.out.send_replace(messages);
        }
    }
}

fn send_all(
    messages: &Messages,
    devices: &BTreeMap<String, DeviceIo>,
    report: &mut impl FnMut(&str, SendResult),
) {
    for (id, message) in messages {
        if let Some(io) = devices.get(id) {
            let result = blocking(|| io.send_feed(message));
            report(id, result.map(|()| FeedSent::of(message)));
        }
    }
}

impl FeedLink {
    /// Sends the messages of the last tick, unless a call sent them already. `report` gets each result.
    pub fn flush(
        &mut self,
        devices: &BTreeMap<String, DeviceIo>,
        report: &mut impl FnMut(&str, SendResult),
    ) {
        if self.rx.has_changed().unwrap_or(false) {
            let messages = self.rx.borrow_and_update().clone();
            send_all(&messages, devices, report);
        }
    }

    /// Sends the Feed message of every tick until the stage is dropped. `devices` gives the
    /// Leakshields that exist at that moment: one can attach later.
    pub async fn run(
        mut self,
        devices: impl Fn() -> BTreeMap<String, DeviceIo>,
        mut report: impl FnMut(&str, SendResult),
    ) {
        while self.rx.changed().await.is_ok() {
            let messages = self.rx.borrow_and_update().clone();
            send_all(&messages, &devices(), &mut report);
        }
    }
}
