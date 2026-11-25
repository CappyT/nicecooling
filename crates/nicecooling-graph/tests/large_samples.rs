//! The window nodes with very large finite samples. A window mean of finite samples whose true
//! mean is finite gives that finite mean. A result that is not finite gives no value. It never
//! gives a wrong number and never panics.

use std::time::Duration;

use nicecooling_core::rules::process::ProcessList;
use nicecooling_graph::eval::{EndValue, Evaluator, Sources};
use nicecooling_graph::model::{Connection, Graph, Kind, Node};
use nicecooling_graph::window::{Filter, Stat};

const BIG: f64 = 1e308;

fn at(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

/// One sample per second for each value. The output of each step.
fn feed(stat: Stat, span_s: u32, values: &[f64]) -> Vec<Option<f64>> {
    let mut filter = Filter::default();
    values
        .iter()
        .enumerate()
        .map(|(second, value)| filter.step(stat, Some(*value), span_s, at(second as u64)))
        .collect()
}

/// Whether `got` is within a few digits of `want`, in units of the largest sample `scale`.
fn near(got: f64, want: f64, scale: f64) -> bool {
    (got - want).abs() <= 1e-9 * scale + 1e-5
}

#[test]
fn average_of_200_samples_of_1e308_is_1e308() {
    let out = feed(Stat::Average, 1800, &[BIG; 200]);
    assert!(out.iter().all(|o| near(o.unwrap(), BIG, BIG)), "{out:?}");
}

#[test]
fn average_of_mixed_signs_is_the_finite_mean() {
    // 100 samples of +1e308 and then 100 of -1e308: every partial sum from the 90th on overflows.
    let mut values = vec![BIG; 100];
    values.extend([-BIG; 100]);
    let out = feed(Stat::Average, 1800, &values);
    assert!(near(out[199].unwrap(), 0.0, BIG), "{:?}", out[199]);
    for (i, got) in out.iter().enumerate() {
        let n = (i + 1) as f64;
        let want = if i < 100 { BIG } else { BIG / n * (200.0 - n) };
        assert!(
            near(got.unwrap(), want, BIG),
            "step {i}: {got:?} want {want}"
        );
    }
    // Alternating signs: the mean after 199 samples is 1e308 / 199.
    let alternating: Vec<f64> = (0..199)
        .map(|i| if i % 2 == 0 { BIG } else { -BIG })
        .collect();
    let out = feed(Stat::Average, 1800, &alternating);
    assert!(near(out[198].unwrap(), BIG / 199.0, BIG), "{:?}", out[198]);
    assert!(near(out[197].unwrap(), 0.0, BIG), "{:?}", out[197]);
}

#[test]
fn peak_and_minimum_of_large_samples() {
    let values = [BIG, -BIG, BIG, 5.0];
    assert_eq!(feed(Stat::Peak, 10, &values)[3], Some(BIG));
    assert_eq!(feed(Stat::Minimum, 10, &values)[3], Some(-BIG));
}

#[test]
fn trend_of_large_samples() {
    // A ramp of 1e306 per sample over 100 samples. The true slope is 1e306.
    let ramp: Vec<f64> = (0..100).map(|i| f64::from(i) * 1e306).collect();
    let out = feed(Stat::Trend, 1800, &ramp);
    assert!(near(out[99].unwrap(), 1e306, 1e306), "{:?}", out[99]);
    // All samples equal: slope 0.
    let out = feed(Stat::Trend, 1800, &[BIG; 200]);
    assert_eq!(out[199], Some(0.0));
    // Three samples from the lowest to the highest finite number: the slope is 1e308.
    let out = feed(Stat::Trend, 10, &[-BIG, 0.0, BIG]);
    assert!(near(out[2].unwrap(), BIG, BIG), "{:?}", out[2]);
    // A falling step of the full range: the slope is finite and negative.
    let out = feed(Stat::Trend, 10, &[BIG, BIG, -BIG, -BIG]);
    assert!(near(out[3].unwrap(), -0.8 * BIG, BIG), "{:?}", out[3]);
}

#[test]
fn low_pass_of_large_samples() {
    let stat = Stat::LowPass { damping: 50.0 };
    let out = feed(stat, 1800, &[BIG; 200]);
    assert!(out.iter().all(|o| near(o.unwrap(), BIG, BIG)), "{out:?}");
    // A jump from +1e308 to -1e308: the distance between the two is not finite, the damped
    // output is. It moves toward the new mean and stays between the two.
    let mut values = vec![BIG; 5];
    values.extend([-BIG; 20]);
    let out = feed(stat, 4, &values);
    for (i, got) in out.iter().enumerate() {
        let got = got.unwrap_or_else(|| panic!("step {i}: no value"));
        assert!(got.is_finite() && got.abs() <= BIG, "step {i}: {got}");
    }
    assert!(near(out[24].unwrap(), -BIG, 1e6 * BIG), "{:?}", out[24]);
}

#[test]
fn window_nodes_with_random_large_samples() {
    let mut rng = fastrand::Rng::with_seed(11);
    for round in 0..200 {
        let count = 3 + rng.usize(0..60);
        let scale = [BIG, 1e300, 1e154, 1.0][rng.usize(0..4)];
        let values: Vec<f64> = (0..count)
            .map(|_| (rng.f64() * 2.0 - 1.0) * scale)
            .collect();
        let max = values.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        let n = count as f64;
        let mean = values.iter().map(|v| v / n).sum::<f64>();
        let ordinals: Vec<f64> = (0..count).map(|i| i as f64).collect();
        let mean_x = (n - 1.0) / 2.0;
        let slope = ordinals
            .iter()
            .zip(&values)
            .map(|(x, y)| (x - mean_x) * (y / max.max(1e-300)))
            .sum::<f64>()
            / ordinals.iter().map(|x| (x - mean_x).powi(2)).sum::<f64>()
            * max;
        let average = *feed(Stat::Average, 1800, &values).last().unwrap();
        let trend = *feed(Stat::Trend, 1800, &values).last().unwrap();
        let low = feed(Stat::LowPass { damping: 25.0 }, 1800, &values);
        let average = average.unwrap_or_else(|| panic!("round {round}: no mean"));
        assert!(near(average, mean, max), "round {round}: {average} {mean}");
        let trend = trend.unwrap_or_else(|| panic!("round {round}: no slope"));
        assert!(near(trend, slope, max), "round {round}: {trend} {slope}");
        for got in low {
            let got = got.unwrap_or_else(|| panic!("round {round}: no low pass"));
            assert!(got.is_finite() && got.abs() <= max, "round {round}: {got}");
        }
    }
}

struct Offline(ProcessList);

impl Sources for Offline {
    fn sensor(&self, _: &str) -> Option<f64> {
        None
    }
    fn alarm_active(&self, _: &str, _: &str) -> Option<bool> {
        None
    }
    fn processes(&self) -> &ProcessList {
        &self.0
    }
}

/// A constant of 1e308 into a 1800 s window node into the sensor end node, for 200 ticks.
#[test]
fn graph_with_a_window_of_200_samples_of_1e308() {
    for kind in [
        Kind::WindowAverage,
        Kind::LowPass,
        Kind::Peak,
        Kind::WindowMinimum,
    ] {
        let mut graph = Graph::new("graph_1", "Big");
        graph.nodes = vec![
            Node::new(1, Kind::Constant).with_param("value", BIG),
            Node::new(2, kind).with_param("span_s", 1800),
            Node::new(3, Kind::SensorEnd),
        ];
        graph.connections = vec![Connection::new(1, 2, "a"), Connection::new(2, 3, "a")];
        let mut evaluator = Evaluator::with_seed(1);
        let sources = Offline(ProcessList::default());
        for tick in 0..200 {
            let pass = evaluator.evaluate(std::slice::from_ref(&graph), &sources, at(tick));
            let EndValue::Sensor(Some(got)) = pass.graphs[0].end else {
                panic!("{kind} {tick}: {:?}", pass.graphs[0].end);
            };
            assert!(near(got, BIG, BIG), "{kind} {tick}: {got}");
        }
    }
}

/// A random node whose range is wider than the largest finite number still gives a number in
/// its range in every tick.
#[test]
fn random_node_over_the_whole_range() {
    for (min, max) in [(-BIG, BIG), (-f64::MAX, f64::MAX), (-5.0, BIG), (-BIG, 5.0)] {
        let mut graph = Graph::new("graph_1", "Wide");
        graph.nodes = vec![
            Node::new(1, Kind::Random)
                .with_param("min", min)
                .with_param("max", max),
            Node::new(2, Kind::SensorEnd),
        ];
        graph.connections = vec![Connection::new(1, 2, "a")];
        let mut evaluator = Evaluator::with_seed(3);
        let sources = Offline(ProcessList::default());
        let mut seen = Vec::new();
        for tick in 0..1000 {
            let pass = evaluator.evaluate(std::slice::from_ref(&graph), &sources, at(tick));
            let EndValue::Sensor(Some(value)) = pass.graphs[0].end else {
                panic!("{min} {max} tick {tick}: {:?}", pass.graphs[0].end);
            };
            assert!((min..=max).contains(&value), "{min} {max}: {value}");
            seen.push(value.to_bits());
        }
        seen.sort_unstable();
        seen.dedup();
        assert!(
            seen.len() > 900,
            "{min} {max}: {} distinct numbers",
            seen.len()
        );
    }
}
