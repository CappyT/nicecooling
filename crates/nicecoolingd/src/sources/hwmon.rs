use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nicecooling_core::clock::Clock;
use nicecooling_hwmon::discovery::{Discovery, SensorKind};
use nicecooling_hwmon::identity::DeviceIdentity;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use crate::value_table::ValueTable;

/// Reads every sensor of one hwmon chip. `read` may block on the device.
pub trait ChipReader: Send + 'static {
    fn read(&mut self) -> Vec<(String, f64)>;
}

/// The `*_input` files of one chip, in the units of `docs/api.md`.
pub struct SysfsChip {
    sensors: Vec<(String, PathBuf, SensorKind)>,
}

impl ChipReader for SysfsChip {
    fn read(&mut self) -> Vec<(String, f64)> {
        self.sensors
            .iter()
            .filter_map(|(id, path, kind)| {
                let raw: f64 = fs::read_to_string(path).ok()?.trim().parse().ok()?;
                Some((id.clone(), raw * kind.scale()))
            })
            .collect()
    }
}

/// One reader for each hwmon chip that has sensors.
pub fn chip_readers(discovery: &Discovery) -> Vec<SysfsChip> {
    let mut chips: BTreeMap<&DeviceIdentity, Vec<_>> = BTreeMap::new();
    for sensor in &discovery.sensors {
        chips.entry(&sensor.device).or_default().push((
            sensor.id.clone(),
            sensor.dir.join(format!("{}_input", sensor.attribute)),
            sensor.kind,
        ));
    }
    chips
        .into_values()
        .map(|sensors| SysfsChip { sensors })
        .collect()
}

pub struct Source {
    pub task: JoinHandle<()>,
    /// Fires after the first publish.
    pub primed: oneshot::Receiver<()>,
}

/// Runs one chip reader every `period` and publishes its values with the time of the read.
/// A read that blocks stops this task only. The tick keeps its period, and the values go stale.
pub fn spawn_source<R: ChipReader>(
    reader: R,
    table: ValueTable,
    clock: Arc<dyn Clock>,
    period: Duration,
) -> Source {
    let (primed_tx, primed) = oneshot::channel();
    let task = tokio::spawn(async move {
        let mut primed_tx = Some(primed_tx);
        let mut interval = tokio::time::interval(period);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut slot = Some(reader);
        loop {
            interval.tick().await;
            let Some(current) = slot.take() else { return };
            let Ok((current, values)) = tokio::task::spawn_blocking(move || {
                let mut reader = current;
                let values = reader.read();
                (reader, values)
            })
            .await
            else {
                return;
            };
            let now = clock.now();
            for (id, value) in values {
                table.publish(&id, value, now);
            }
            slot = Some(current);
            if let Some(tx) = primed_tx.take() {
                let _ = tx.send(());
            }
        }
    });
    Source { task, primed }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tick_runner::TickRunner;
    use nicecooling_core::tick::stage_read::ReadStage;
    use nicecooling_core::tick::{Pipeline, Stage, TickConfig, TickContext};
    use std::sync::Mutex;
    use std::sync::mpsc;

    struct Steady;

    impl ChipReader for Steady {
        fn read(&mut self) -> Vec<(String, f64)> {
            vec![("steady".into(), 1.0)]
        }
    }

    /// Publishes once, then blocks on the device until the test releases it.
    struct Stuck {
        first: bool,
        gate: mpsc::Receiver<()>,
    }

    impl ChipReader for Stuck {
        fn read(&mut self) -> Vec<(String, f64)> {
            if std::mem::take(&mut self.first) {
                return vec![("stuck".into(), 2.0)];
            }
            let _ = self.gate.recv();
            vec![]
        }
    }

    struct Probe(Arc<Mutex<Vec<(Duration, bool, bool)>>>);

    impl Stage for Probe {
        fn name(&self) -> &'static str {
            "probe"
        }

        fn run(&mut self, ctx: &mut TickContext) {
            let seen = (
                ctx.now,
                ctx.values.contains_key("steady"),
                ctx.values.contains_key("stuck"),
            );
            self.0.lock().unwrap().push(seen);
        }
    }

    // Real time: a blocking read stops the Tokio clock from advancing when it is paused.
    #[tokio::test]
    async fn blocked_source_keeps_tick_period() {
        let table = ValueTable::default();
        let clock = Arc::new(crate::tick_runner::TokioClock::new());
        let (release, gate) = mpsc::channel();
        let period = Duration::from_secs(1);
        let steady = spawn_source(Steady, table.clone(), clock.clone(), period);
        let stuck = spawn_source(
            Stuck { first: true, gate },
            table.clone(),
            clock.clone(),
            period,
        );
        steady.primed.await.unwrap();
        stuck.primed.await.unwrap();

        let seen = Arc::new(Mutex::new(Vec::new()));
        let pipeline = Pipeline::new(vec![
            Box::new(ReadStage::new(Box::new(table))),
            Box::new(Probe(seen.clone())),
        ]);
        let mut runner = TickRunner::new(pipeline, TickConfig::default(), clock, period);
        runner.run(Some(6)).await;

        steady.task.abort();
        stuck.task.abort();
        drop(release);

        let seen = seen.lock().unwrap();
        for pair in seen.windows(2) {
            let step = pair[1].0 - pair[0].0;
            assert!(
                step > Duration::from_millis(900) && step < Duration::from_millis(1100),
                "step {step:?}"
            );
        }
        assert!(
            seen.iter().all(|s| s.1),
            "the steady source is always present"
        );
        assert!(seen[1].2 && seen[2].2, "the stuck value is fresh for 3 s");
        assert!(
            !seen[4].2 && !seen[5].2,
            "the stuck value reads as no value after 3 s"
        );
    }
}
