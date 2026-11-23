use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nicecooling_core::clock::Clock;
use nicecooling_core::tick::{ChannelIo, Pipeline, Readback, TickConfig, TickContext};
use nicecooling_hwmon::discovery::Channel;
use nicecooling_hwmon::sysfs::{read_attr, write_attr};
use tokio::time::{Instant, MissedTickBehavior};

use crate::lease::Lease;

/// The clock of the tick: time since the start, on the Tokio clock.
pub struct TokioClock {
    start: Instant,
}

impl TokioClock {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }
}

impl Default for TokioClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for TokioClock {
    fn now(&self) -> Duration {
        self.start.elapsed()
    }
}

/// Reads and writes the `pwmN` attributes of the Channels. A write takes the lease first.
pub struct SysfsChannels {
    channels: BTreeMap<String, (PathBuf, u32)>,
    lease: Lease,
    boot_id: String,
}

impl SysfsChannels {
    pub fn new(channels: &[Channel], lease: Lease, boot_id: String) -> Self {
        Self {
            channels: channels
                .iter()
                .map(|c| (c.id.clone(), (c.dir.clone(), c.number)))
                .collect(),
            lease,
            boot_id,
        }
    }

    fn channel(&self, id: &str) -> io::Result<&(PathBuf, u32)> {
        self.channels
            .get(id)
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "unknown Channel"))
    }
}

fn parse<T: std::str::FromStr>(text: String) -> io::Result<T> {
    text.parse()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, format!("not a number: {text}")))
}

impl ChannelIo for SysfsChannels {
    fn read(&mut self, id: &str) -> io::Result<Readback> {
        let (dir, n) = self.channel(id)?;
        Ok(Readback {
            raw: parse(read_attr(dir, &format!("pwm{n}"))?)?,
            enable: parse(read_attr(dir, &format!("pwm{n}_enable"))?)?,
        })
    }

    fn write(&mut self, id: &str, raw: u8, set_manual: bool) -> io::Result<()> {
        let (dir, n) = self.channel(id)?.clone();
        let now_us = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_micros() as u64);
        self.lease.create(&self.boot_id, now_us)?;
        if set_manual {
            write_attr(&dir, &format!("pwm{n}_enable"), "1")?;
        }
        write_attr(&dir, &format!("pwm{n}"), &raw.to_string())
    }
}

/// Runs the pipeline every period and reports device errors and new Warnings on stderr.
pub struct TickRunner {
    pipeline: Pipeline,
    ctx: TickContext,
    clock: Arc<dyn Clock>,
    period: Duration,
    reported: BTreeSet<(String, String)>,
}

impl TickRunner {
    pub fn new(
        pipeline: Pipeline,
        config: TickConfig,
        clock: Arc<dyn Clock>,
        period: Duration,
    ) -> Self {
        Self {
            pipeline,
            ctx: TickContext::new(config),
            clock,
            period,
            reported: BTreeSet::new(),
        }
    }

    pub fn context(&mut self) -> &mut TickContext {
        &mut self.ctx
    }

    pub fn tick(&mut self) {
        self.pipeline.run(&mut self.ctx, self.clock.now());
        for error in &self.ctx.errors {
            eprintln!("nicecoolingd: {error}");
        }
        let active: BTreeSet<_> = self
            .ctx
            .warnings
            .iter()
            .map(|(e, s)| (e.to_owned(), s.to_owned()))
            .collect();
        for (event, source) in active.difference(&self.reported) {
            eprintln!("nicecoolingd: warning {event} {source}");
        }
        self.reported = active;
    }

    /// Runs `limit` ticks, or ticks without end. A tick that overruns skips the missed ticks.
    pub async fn run(&mut self, limit: Option<u64>) {
        let mut interval = tokio::time::interval(self.period);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let mut count = 0;
        while limit.is_none_or(|l| count < l) {
            interval.tick().await;
            self.tick();
            count += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nicecooling_hwmon::discovery::discover;
    use nicecooling_hwmon::fake::FakeSysfs;

    #[test]
    fn lease_written_before_first_manual_write() {
        let fake = FakeSysfs::reference_board();
        let discovery = discover(&fake.sysfs(), None, &Default::default()).unwrap();
        let pwm = fake.attr_path("nct6798", "pwm3");
        let enable = fake.attr_path("nct6798", "pwm3_enable");

        // A state directory that does not exist: the lease fails, so nothing is written.
        let missing =
            std::env::temp_dir().join(format!("nicecooling-no-dir-{}/state", std::process::id()));
        let mut io = SysfsChannels::new(&discovery.channels, Lease::new(&missing), "boot".into());
        assert!(io.write("nct6798_pwm3", 128, true).is_err());
        assert_eq!(std::fs::read_to_string(&pwm).unwrap().trim(), "96");
        assert_eq!(std::fs::read_to_string(&enable).unwrap().trim(), "5");

        let state = std::env::temp_dir().join(format!("nicecooling-lease-{}", std::process::id()));
        std::fs::create_dir_all(&state).unwrap();
        let lease = Lease::new(&state);
        let mut io = SysfsChannels::new(&discovery.channels, lease.clone(), "boot".into());
        io.write("nct6798_pwm3", 128, true).unwrap();
        assert!(lease.exists());
        assert_eq!(
            io.read("nct6798_pwm3").unwrap(),
            Readback {
                raw: 128,
                enable: 1
            }
        );
        std::fs::remove_dir_all(state).unwrap();
    }
}
