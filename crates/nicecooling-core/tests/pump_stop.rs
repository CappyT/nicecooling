//! The pump stop rule reads the raw value that the reconcile stage writes: a Curve Duty that
//! rounds to raw 0 is off by choice, not a running pump.

use std::collections::BTreeMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use nicecooling_core::curve::Curve;
use nicecooling_core::loops::Loop;
use nicecooling_core::tick::{
    ChannelIo, ChannelSetup, Control, LoopSetup, Pipeline, Readback, Reading, TickConfig,
    TickContext, ValueSource,
};

#[derive(Clone, Default)]
struct Values(Arc<Mutex<BTreeMap<String, Reading>>>);

impl ValueSource for Values {
    fn snapshot(&self) -> BTreeMap<String, Reading> {
        self.0.lock().unwrap().clone()
    }
}

#[derive(Clone, Default)]
struct Chip(Arc<Mutex<BTreeMap<String, Readback>>>);

impl ChannelIo for Chip {
    fn channel_ids(&self) -> Vec<String> {
        vec!["c_pwm2".into()]
    }
    fn read(&mut self, id: &str) -> io::Result<Readback> {
        Ok(self.0.lock().unwrap()[id])
    }
    fn write(&mut self, id: &str, raw: u8, _set_manual: bool) -> io::Result<()> {
        self.0
            .lock()
            .unwrap()
            .insert(id.into(), Readback { raw, enable: 1 });
        Ok(())
    }
    fn release(&mut self, _id: &str) -> io::Result<()> {
        Ok(())
    }
}

/// Runs 8 ticks with a stopped pump tach and a Curve that asks `temperature`. Returns the raw
/// value written before the pump stop Alarm rose (the last one when it never rose) and whether
/// the Alarm rose. The Alarm then overrides the Channel to full speed.
fn run_stopped_pump(temperature: f64) -> (Option<u8>, bool) {
    let mut curve = Curve::new("t", vec![(30.0, 0.0), (50.0, 100.0)]).unwrap();
    curve.smoothing_s = 0.0;
    curve.step_limit = 100.0;
    let config = TickConfig {
        channels: vec![ChannelSetup {
            id: "c_pwm2".into(),
            control: Control::Curve(curve),
        }],
        loops: vec![LoopSetup {
            config: Loop {
                id: "loop_1".into(),
                name: "Loop".into(),
                pump_channels: vec!["c_pwm2".into()],
                flow_sensor: None,
                calibration_table: None,
                water_temperature: None,
                leakshield: None,
                pump_floor: 0.0,
            },
            table: None,
        }],
        ..TickConfig::default()
    };
    let values = Values::default();
    let chip = Chip::default();
    let mut pipeline = Pipeline::standard(Box::new(values.clone()), Box::new(chip.clone()));
    let mut ctx = TickContext::new(config);
    let mut raw = None;
    let mut raised = false;
    for t in 0..8u64 {
        let now = Duration::from_secs(100 + t);
        let mut v = values.0.lock().unwrap();
        v.insert(
            "t".into(),
            Reading {
                value: temperature,
                at: now,
            },
        );
        v.insert(
            "c_fan2".into(),
            Reading {
                value: 0.0,
                at: now,
            },
        );
        drop(v);
        pipeline.run(&mut ctx, now);
        if ctx.alarms.is_active("pump-stop", "loop_1") {
            raised = true;
            break;
        }
        raw = chip.0.lock().unwrap().get("c_pwm2").map(|r| r.raw);
    }
    (raw, raised)
}

#[test]
fn pump_stop_ignores_a_duty_that_rounds_to_raw_0() {
    // 30.03 C asks 0.15%: raw 0, the same as a Duty of 0% by choice.
    let (raw, raised) = run_stopped_pump(30.03);
    assert_eq!(raw, Some(0));
    assert!(!raised);
}

#[test]
fn pump_stop_counts_a_duty_that_rounds_to_raw_1_as_running() {
    // 30.1 C asks 0.5%: raw 1, a pump that must turn.
    let (raw, raised) = run_stopped_pump(30.1);
    assert_eq!(raw, Some(1));
    assert!(raised);
}
