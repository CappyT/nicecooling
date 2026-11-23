use std::collections::BTreeMap;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;
use crate::clock::{Clock, SimClock};

const SECOND: Duration = Duration::from_secs(1);

/// A chip that behaves like `nct6775`: manual mode at raw 255 reads `pwmN_enable` 0,
/// and a reading can stay cached for a while after a write.
struct Chip {
    clock: SimClock,
    cache: Duration,
    channels: BTreeMap<String, (Readback, Option<(Readback, Duration)>)>,
    writes: Vec<(String, u8, bool)>,
    releases: Vec<String>,
    fail_release: bool,
    /// Channels that the device refuses to take over.
    blocked: Vec<String>,
}

impl Chip {
    fn set(&mut self, id: &str, raw: u8, enable: u32) {
        self.channels
            .insert(id.to_owned(), (Readback { raw, enable }, None));
    }

    fn actual(&self, id: &str) -> Readback {
        self.channels[id].0
    }
}

#[derive(Clone)]
struct SharedChip(Arc<Mutex<Chip>>);

impl ChannelIo for SharedChip {
    fn can_take_over(&self, id: &str) -> bool {
        !self.0.lock().unwrap().blocked.iter().any(|b| b == id)
    }

    fn read(&mut self, id: &str) -> io::Result<Readback> {
        let chip = self.0.lock().unwrap();
        let (actual, cached) = &chip.channels[id];
        Ok(match cached {
            Some((old, until)) if chip.clock.now() < *until => *old,
            _ => *actual,
        })
    }

    fn write(&mut self, id: &str, raw: u8, set_manual: bool) -> io::Result<()> {
        let mut chip = self.0.lock().unwrap();
        let now = chip.clock.now();
        let cache = chip.cache;
        let entry = chip.channels.entry(id.to_owned()).or_default();
        let old = entry.0;
        if cache > Duration::ZERO {
            entry.1 = Some((old, now + cache));
        }
        entry.0.raw = raw;
        if set_manual {
            entry.0.enable = 1;
        }
        if entry.0.enable == 1 && raw == 255 {
            entry.0.enable = 0;
        }
        chip.writes.push((id.to_owned(), raw, set_manual));
        Ok(())
    }

    fn release(&mut self, id: &str) -> io::Result<()> {
        let mut chip = self.0.lock().unwrap();
        chip.releases.push(id.to_owned());
        if chip.fail_release {
            return Err(io::Error::other("busy"));
        }
        chip.set(id, 200, 5);
        Ok(())
    }
}

impl Default for Readback {
    fn default() -> Self {
        Self { raw: 0, enable: 5 }
    }
}

#[derive(Clone)]
struct SharedValues(Arc<Mutex<BTreeMap<String, Reading>>>);

impl ValueSource for SharedValues {
    fn snapshot(&self) -> BTreeMap<String, Reading> {
        self.0.lock().unwrap().clone()
    }
}

struct Harness {
    pipeline: Pipeline,
    ctx: TickContext,
    clock: SimClock,
    values: SharedValues,
    chip: Arc<Mutex<Chip>>,
}

impl Harness {
    fn new(config: TickConfig, chip_cache: Duration) -> Self {
        let clock = SimClock::default();
        clock.advance(Duration::from_secs(100));
        let chip = Arc::new(Mutex::new(Chip {
            clock: clock.clone(),
            cache: chip_cache,
            channels: BTreeMap::new(),
            writes: Vec::new(),
            releases: Vec::new(),
            fail_release: false,
            blocked: Vec::new(),
        }));
        for setup in &config.channels {
            chip.lock().unwrap().set(&setup.id, 200, 5);
        }
        let values = SharedValues(Arc::default());
        let pipeline =
            Pipeline::standard(Box::new(values.clone()), Box::new(SharedChip(chip.clone())));
        Self {
            pipeline,
            ctx: TickContext::new(config),
            clock,
            values,
            chip,
        }
    }

    fn set(&self, sensor: &str, value: f64) {
        let reading = Reading {
            value,
            at: self.clock.now(),
        };
        self.values
            .0
            .lock()
            .unwrap()
            .insert(sensor.to_owned(), reading);
    }

    fn remove(&self, sensor: &str) {
        self.values.0.lock().unwrap().remove(sensor);
    }

    /// Runs one tick, then advances the clock by one second.
    fn tick(&mut self) {
        self.pipeline.run(&mut self.ctx, self.clock.now());
        self.clock.advance(SECOND);
    }

    fn ticks(&mut self, n: usize) {
        for _ in 0..n {
            self.tick();
        }
    }

    fn duty(&self, id: &str) -> Option<f64> {
        self.ctx.channels[id].duty
    }

    fn writes(&self) -> usize {
        self.chip.lock().unwrap().writes.len()
    }
}

fn curve(input: &str, smoothing_s: f64, step_limit: f64) -> Control {
    let mut curve = Curve::new(input, vec![(0.0, 0.0), (100.0, 100.0)]).unwrap();
    curve.smoothing_s = smoothing_s;
    curve.step_limit = step_limit;
    Control::Curve(curve)
}

fn setup(id: &str, control: Control) -> ChannelSetup {
    ChannelSetup {
        id: id.to_owned(),
        control,
    }
}

fn one_channel(control: Control) -> TickConfig {
    TickConfig {
        channels: vec![setup("ch", control)],
        ..TickConfig::default()
    }
}

struct Recorder(&'static str, Arc<Mutex<Vec<&'static str>>>);

impl Stage for Recorder {
    fn name(&self) -> &'static str {
        self.0
    }

    fn run(&mut self, _ctx: &mut TickContext) {
        self.1.lock().unwrap().push(self.0);
    }
}

#[test]
fn stage_order() {
    let values = SharedValues(Arc::default());
    let chip = Arc::new(Mutex::new(Chip {
        clock: SimClock::default(),
        cache: Duration::ZERO,
        channels: BTreeMap::new(),
        writes: Vec::new(),
        releases: Vec::new(),
        fail_release: false,
        blocked: Vec::new(),
    }));
    let standard = Pipeline::standard(Box::new(values), Box::new(SharedChip(chip)));
    assert_eq!(
        standard.names(),
        [
            "read",
            "derive",
            "alarms",
            "rules",
            "curve",
            "step",
            "floor",
            "override",
            "reconcile"
        ]
    );

    let log = Arc::new(Mutex::new(Vec::new()));
    let mut pipeline = Pipeline::new(vec![
        Box::new(Recorder("a", log.clone())),
        Box::new(Recorder("b", log.clone())),
    ]);
    pipeline.push(Box::new(Recorder("c", log.clone())));
    pipeline.run(&mut TickContext::default(), Duration::ZERO);
    assert_eq!(*log.lock().unwrap(), ["a", "b", "c"]);
}

#[test]
fn smoothing_5s() {
    let mut h = Harness::new(one_channel(curve("t", 5.0, 100.0)), Duration::ZERO);
    for _ in 0..4 {
        h.set("t", 40.0);
        h.tick();
    }
    h.set("t", 70.0);
    h.tick();
    assert_eq!(h.ctx.channels["ch"].target, Some(46.0));
    h.set("t", 40.0);
    h.tick();
    assert!(h.ctx.channels["ch"].target.unwrap() <= 46.0);
}

#[test]
fn per_curve_smoothing_and_step_settings() {
    let config = TickConfig {
        channels: vec![
            setup("fast", curve("t", 1.0, 100.0)),
            setup("slow", curve("t", 10.0, 2.0)),
        ],
        ..TickConfig::default()
    };
    let mut h = Harness::new(config, Duration::ZERO);
    for _ in 0..10 {
        h.set("t", 20.0);
        h.tick();
    }
    assert_eq!((h.duty("fast"), h.duty("slow")), (Some(20.0), Some(20.0)));
    h.set("t", 60.0);
    h.tick();
    assert_eq!(h.duty("fast"), Some(60.0));
    // The mean of the last 10 s is 24, and the step limit is 2.
    assert_eq!(h.duty("slow"), Some(22.0));
}

#[test]
fn step_limit_5_per_s() {
    let mut h = Harness::new(one_channel(curve("t", 0.0, 5.0)), Duration::ZERO);
    h.set("t", 40.0);
    h.tick();
    assert_eq!(h.duty("ch"), Some(40.0));
    h.set("t", 80.0);
    for expected in [45.0, 50.0, 55.0, 60.0, 65.0, 70.0, 75.0, 80.0] {
        h.tick();
        assert_eq!(h.duty("ch"), Some(expected));
        h.set("t", 80.0);
    }
    h.tick();
    assert_eq!(h.duty("ch"), Some(80.0));
}

#[test]
fn lost_input_bypasses_step_limit() {
    let mut h = Harness::new(one_channel(curve("t", 0.0, 5.0)), Duration::ZERO);
    h.set("t", 40.0);
    h.tick();
    assert_eq!(h.duty("ch"), Some(40.0));
    h.remove("t");
    h.tick();
    assert_eq!(h.duty("ch"), Some(100.0));
    assert!(h.ctx.channels["ch"].override_active);
    assert!(h.ctx.warnings.is_active("lost-input", "ch"));
    assert_eq!(h.chip.lock().unwrap().actual("ch").raw, 255);
    h.set("t", 40.0);
    h.tick();
    assert!(!h.ctx.warnings.is_active("lost-input", "ch"));
    assert!(!h.ctx.channels["ch"].override_active);
}

#[test]
fn stale_input_is_lost_input() {
    let mut h = Harness::new(one_channel(curve("t", 0.0, 5.0)), Duration::ZERO);
    h.set("t", 40.0);
    h.tick();
    // The value stays in the table but is 4 s old.
    h.ticks(4);
    assert_eq!(h.duty("ch"), Some(100.0));
    assert!(h.ctx.warnings.is_active("lost-input", "ch"));
}

#[test]
fn ramp_down_from_100_after_override() {
    let mut h = Harness::new(one_channel(curve("t", 0.0, 5.0)), Duration::ZERO);
    h.set("t", 40.0);
    h.tick();
    h.remove("t");
    h.tick();
    assert_eq!(h.duty("ch"), Some(100.0));
    h.set("t", 40.0);
    for expected in [95.0, 90.0, 85.0] {
        h.tick();
        assert_eq!(h.duty("ch"), Some(expected));
        h.set("t", 40.0);
    }
}

#[test]
fn untouched_channel_stays_on_hardware_curve() {
    let mut h = Harness::new(one_channel(Control::Hardware), Duration::ZERO);
    h.ticks(5);
    assert_eq!(h.writes(), 0);
    assert_eq!(h.duty("ch"), None);
    assert_eq!(
        h.chip.lock().unwrap().actual("ch"),
        Readback {
            raw: 200,
            enable: 5
        }
    );
}

#[test]
fn pump_floor_clamps_curve_and_fixed_duty() {
    let pump_loop = Loop {
        id: "loop_1".into(),
        name: "Loop".into(),
        pump_channels: vec!["curved".into(), "fixed".into()],
        flow_sensor: None,
        calibration_table: None,
        water_temperature: None,
        leakshield: None,
        pump_floor: 40.0,
    };
    let config = TickConfig {
        channels: vec![
            setup("curved", curve("t", 0.0, 100.0)),
            setup("fixed", Control::Fixed(20.0)),
        ],
        loops: vec![LoopSetup {
            config: pump_loop,
            table: None,
        }],
        ..TickConfig::default()
    };
    let mut h = Harness::new(config, Duration::ZERO);
    h.set("t", 20.0);
    h.tick();
    assert_eq!(h.duty("curved"), Some(40.0));
    assert_eq!(h.duty("fixed"), Some(40.0));
    assert_eq!(
        h.chip.lock().unwrap().actual("fixed").raw,
        percent_to_raw(40.0)
    );
    h.set("t", 70.0);
    h.tick();
    // Above the floor, the Curve rules again.
    assert_eq!(h.duty("curved"), Some(70.0));
}

#[test]
fn derived_and_flow_values_reach_the_curve() {
    use crate::calibration::CalibrationTable;
    use crate::derived::{DerivedKind, DerivedSensor};

    let pump_loop = Loop {
        id: "loop_1".into(),
        name: "Loop".into(),
        pump_channels: vec![],
        flow_sensor: Some("flow_rpm".into()),
        calibration_table: None,
        water_temperature: None,
        leakshield: None,
        pump_floor: 0.0,
    };
    let table = CalibrationTable::new(vec![(100.0, 10.0), (200.0, 20.0)]).unwrap();
    let config = TickConfig {
        channels: vec![setup("ch", curve("derived_1", 0.0, 100.0))],
        derived: vec![DerivedSensor {
            id: "derived_1".into(),
            name: "Max".into(),
            kind: DerivedKind::Maximum,
            sources: vec!["a".into(), "b".into()],
        }],
        loops: vec![LoopSetup {
            config: pump_loop,
            table: Some(table),
        }],
    };
    let mut h = Harness::new(config, Duration::ZERO);
    h.set("a", 40.0);
    h.set("b", 55.0);
    h.set("flow_rpm", 250.0);
    h.tick();
    assert_eq!(h.duty("ch"), Some(55.0));
    assert_eq!(h.ctx.values["loop_1_flow"], 25.0);
    assert!(h.ctx.out_of_range.contains("loop_1_flow"));
}

#[test]
fn non_finite_input_is_lost_input_and_recovers() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut h = Harness::new(one_channel(curve("t", 0.0, 5.0)), Duration::ZERO);
        h.set("t", 40.0);
        h.tick();
        h.set("t", bad);
        h.tick();
        assert_eq!(h.duty("ch"), Some(100.0), "{bad}");
        assert!(h.ctx.channels["ch"].lost_input, "{bad}");
        assert_eq!(h.chip.lock().unwrap().actual("ch").raw, 255, "{bad}");
        h.set("t", 40.0);
        h.tick();
        assert_eq!(h.duty("ch"), Some(95.0), "{bad}");
    }
}

#[test]
fn non_finite_derived_value_is_lost_input() {
    use crate::derived::{DerivedKind, DerivedSensor};

    let config = TickConfig {
        channels: vec![setup("ch", curve("derived_1", 0.0, 5.0))],
        derived: vec![DerivedSensor {
            id: "derived_1".into(),
            name: "Mean".into(),
            kind: DerivedKind::Mean,
            sources: vec!["a".into(), "b".into()],
        }],
        ..TickConfig::default()
    };
    let mut h = Harness::new(config, Duration::ZERO);
    h.set("a", 1e308);
    h.set("b", 1e308);
    h.tick();
    assert!(!h.ctx.values.contains_key("derived_1"));
    assert_eq!(h.duty("ch"), Some(100.0));
}

#[test]
fn non_finite_fixed_duty_is_full_speed_and_never_stored() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut h = Harness::new(one_channel(Control::Fixed(bad)), Duration::ZERO);
        h.ticks(2);
        assert_eq!(h.duty("ch"), Some(100.0), "{bad}");
        assert_eq!(h.ctx.channels["ch"].last_duty, Some(100.0), "{bad}");
        assert_eq!(h.chip.lock().unwrap().actual("ch").raw, 255, "{bad}");
        // A fixed Duty of 40 takes over: the step limit walks down from 100.
        h.ctx.config = one_channel(Control::Fixed(40.0));
        h.tick();
        assert_eq!(h.duty("ch"), Some(95.0), "{bad}");
    }
}

#[test]
fn step_stage_replaces_a_non_finite_previous_duty() {
    let mut h = Harness::new(one_channel(Control::Fixed(40.0)), Duration::ZERO);
    h.tick();
    h.ctx.channels.get_mut("ch").unwrap().last_duty = Some(f64::NAN);
    h.tick();
    assert_eq!(h.duty("ch"), Some(40.0));
}

#[test]
fn channel_the_device_cannot_take_over_stays_on_hardware() {
    let config = TickConfig {
        channels: vec![
            setup("free", Control::Fixed(40.0)),
            setup("blocked", Control::Fixed(40.0)),
        ],
        ..TickConfig::default()
    };
    let mut h = Harness::new(config, Duration::ZERO);
    h.chip.lock().unwrap().blocked.push("blocked".to_owned());
    h.ticks(3);
    assert_eq!(h.duty("free"), Some(40.0));
    assert_eq!(h.duty("blocked"), None);
    let chip = h.chip.lock().unwrap();
    assert!(chip.writes.iter().all(|(id, _, _)| id == "free"));
    assert_eq!(
        chip.actual("blocked"),
        Readback {
            raw: 200,
            enable: 5
        }
    );
    assert!(chip.releases.is_empty());
}

#[test]
fn single_write_for_constant_duty() {
    let mut h = Harness::new(one_channel(curve("t", 0.0, 5.0)), Duration::ZERO);
    for _ in 0..10 {
        h.set("t", 50.0);
        h.tick();
    }
    let chip = h.chip.lock().unwrap();
    assert_eq!(chip.writes, [("ch".to_owned(), 128, true)]);
}

fn settled_fixed(cache: Duration) -> Harness {
    let mut h = Harness::new(one_channel(Control::Fixed(50.0)), cache);
    h.ticks(4);
    assert_eq!(h.writes(), 1);
    h
}

#[test]
fn corrects_changed_pwm() {
    let mut h = settled_fixed(Duration::ZERO);
    h.chip.lock().unwrap().set("ch", 10, 1);
    h.tick();
    assert_eq!(
        h.chip.lock().unwrap().actual("ch"),
        Readback {
            raw: 128,
            enable: 1
        }
    );
    assert_eq!(h.writes(), 2);
    assert!(!h.ctx.warnings.is_active("another-writer", "ch"));
}

#[test]
fn enable_0_raw_255_is_manual() {
    let mut h = Harness::new(one_channel(Control::Fixed(100.0)), Duration::ZERO);
    h.ticks(6);
    assert_eq!(
        h.chip.lock().unwrap().actual("ch"),
        Readback {
            raw: 255,
            enable: 0
        }
    );
    assert_eq!(h.writes(), 1);
    assert!(h.ctx.warnings.iter().next().is_none());
}

#[test]
fn another_writer_after_3_mismatches() {
    let mut h = settled_fixed(Duration::ZERO);
    for round in 1..=3 {
        h.chip.lock().unwrap().set("ch", 10, 1);
        h.tick();
        assert_eq!(
            h.ctx.warnings.is_active("another-writer", "ch"),
            round == 3,
            "round {round}"
        );
        // The next tick is inside the cache grace of the correction.
        h.tick();
    }
    assert_eq!(h.writes(), 4);
}

#[test]
fn stale_cache_1_5s_no_false_mismatch() {
    let mut h = settled_fixed(Duration::from_millis(1500));
    h.ticks(10);
    assert_eq!(h.writes(), 1);
    assert!(h.ctx.warnings.iter().next().is_none());
}

#[test]
fn resume_corrects_reset_channel() {
    let mut h = settled_fixed(Duration::ZERO);
    // The Board resets the Channel to its automatic mode during a suspend.
    h.chip.lock().unwrap().set("ch", 200, 5);
    h.tick();
    assert_eq!(
        h.chip.lock().unwrap().actual("ch"),
        Readback {
            raw: 128,
            enable: 1
        }
    );
    assert_eq!(
        h.chip.lock().unwrap().writes.last(),
        Some(&("ch".to_owned(), 128, true))
    );
}

/// Starts a ramp: Input 20 gives the first Duty, then Input 90 moves it 5% each tick.
fn ramping(cache: Duration) -> Harness {
    let mut h = Harness::new(one_channel(curve("t", 0.0, 5.0)), cache);
    h.set("t", 20.0);
    h.tick();
    h
}

fn ramp_ticks(h: &mut Harness, n: usize) {
    for _ in 0..n {
        h.set("t", 90.0);
        h.tick();
    }
}

#[test]
fn ramp_with_driver_cache_has_no_false_mismatch() {
    let mut h = ramping(Duration::from_millis(1500));
    ramp_ticks(&mut h, 14);
    assert_eq!(h.duty("ch"), Some(90.0));
    assert_eq!(h.writes(), 15);
    assert!(h.ctx.warnings.iter().next().is_none());
    assert!(h.ctx.errors.is_empty());
}

#[test]
fn mode_reset_during_ramp_is_corrected_next_tick() {
    let mut h = ramping(Duration::from_millis(1500));
    ramp_ticks(&mut h, 5);
    h.chip.lock().unwrap().set("ch", 200, 5);
    ramp_ticks(&mut h, 1);
    let chip = h.chip.lock().unwrap();
    assert_eq!(chip.actual("ch").enable, 1);
    assert_eq!(chip.writes.last().map(|w| w.2), Some(true));
}

#[test]
fn another_writer_is_detected_during_ramp() {
    let mut h = ramping(Duration::from_millis(1500));
    ramp_ticks(&mut h, 3);
    for round in 1..=3 {
        h.chip.lock().unwrap().set("ch", 10, 1);
        ramp_ticks(&mut h, 1);
        assert_eq!(
            h.ctx.warnings.is_active("another-writer", "ch"),
            round == 3,
            "round {round}"
        );
        ramp_ticks(&mut h, 2);
    }
}

#[test]
fn channel_back_on_hardware_control_is_released_once() {
    let mut h = settled_fixed(Duration::ZERO);
    h.ctx.config.channels[0].control = Control::Hardware;
    h.ticks(3);
    let chip = h.chip.lock().unwrap();
    assert_eq!(chip.releases, ["ch"]);
    assert_eq!(
        chip.actual("ch"),
        Readback {
            raw: 200,
            enable: 5
        }
    );
}

#[test]
fn channel_removed_from_config_is_released() {
    let mut h = settled_fixed(Duration::ZERO);
    h.ctx.config.channels.clear();
    h.ticks(2);
    assert_eq!(h.chip.lock().unwrap().releases, ["ch"]);
}

#[test]
fn failed_release_is_retried() {
    let mut h = settled_fixed(Duration::ZERO);
    h.chip.lock().unwrap().fail_release = true;
    h.ctx.config.channels[0].control = Control::Hardware;
    h.tick();
    assert_eq!(h.ctx.errors.len(), 1);
    h.chip.lock().unwrap().fail_release = false;
    h.ticks(2);
    assert_eq!(h.chip.lock().unwrap().releases, ["ch", "ch"]);
    assert!(h.ctx.errors.is_empty());
}
