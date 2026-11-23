use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nicecooling_core::clock::Clock;
use nicecooling_core::tick::{ChannelIo, Pipeline, Readback, TickConfig, TickContext};
use nicecooling_hwmon::discovery::Channel;
use nicecooling_hwmon::identity::DeviceIdentity;
use nicecooling_hwmon::sysfs::{read_attr, write_attr};
use tokio::time::{Instant, MissedTickBehavior};

use crate::lease::Lease;
use crate::snapshot::{BiosSnapshot, restore_channel};

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
    devices: BTreeMap<String, DeviceIdentity>,
    lease: Lease,
    boot_id: String,
    snapshot: Option<BiosSnapshot>,
}

impl SysfsChannels {
    pub fn new(channels: &[Channel], lease: Lease, boot_id: String) -> Self {
        Self {
            channels: channels
                .iter()
                .map(|c| (c.id.clone(), (c.dir.clone(), c.number)))
                .collect(),
            devices: channels
                .iter()
                .map(|c| (c.id.clone(), c.device.clone()))
                .collect(),
            lease,
            boot_id,
            snapshot: None,
        }
    }

    /// The snapshot that `release` restores a Channel from.
    pub fn with_snapshot(mut self, snapshot: Option<BiosSnapshot>) -> Self {
        self.snapshot = snapshot;
        self
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
    /// Without a snapshot the Channels are not told apart: the `state-fallback` Warning covers it.
    fn can_take_over(&self, id: &str) -> bool {
        match (&self.snapshot, self.channels.get(id), self.devices.get(id)) {
            (Some(snapshot), Some((_, number)), Some(device)) => {
                snapshot.channel(device, *number).is_some()
            }
            _ => true,
        }
    }

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

    fn release(&mut self, id: &str) -> io::Result<()> {
        let (dir, number) = self.channel(id)?.clone();
        let device = &self.devices[id];
        // A Channel that the snapshot does not hold has no BIOS state to go back to.
        let Some(channel) = self
            .snapshot
            .as_ref()
            .and_then(|s| s.channel(device, number))
        else {
            return Ok(());
        };
        let problems = restore_channel(&dir, channel, &mut |dir, name, value| {
            write_attr(dir, name, value)
        });
        if problems.is_empty() {
            Ok(())
        } else {
            Err(io::Error::other(problems.join("; ")))
        }
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

    #[test]
    fn a_channel_without_a_snapshot_entry_cannot_be_taken_over() {
        let fake = FakeSysfs::reference_board();
        let discovery = discover(&fake.sysfs(), None, &Default::default()).unwrap();
        let mut snapshot =
            crate::snapshot::take(&discovery.channels, Default::default(), 1).unwrap();
        for chip in &mut snapshot.chips {
            chip.channels.retain(|c| c.channel != 3);
        }
        let state = std::env::temp_dir().join(format!("nicecooling-take-{}", std::process::id()));
        let io = SysfsChannels::new(&discovery.channels, Lease::new(&state), "boot".into());
        // No snapshot at all: nothing tells the Channels apart.
        assert!(io.can_take_over("nct6798_pwm3"));
        let io = io.with_snapshot(Some(snapshot));
        assert!(!io.can_take_over("nct6798_pwm3"));
        assert!(io.can_take_over("nct6798_pwm4"));
    }

    #[test]
    fn release_restores_only_that_channel_from_the_snapshot() {
        let fake = FakeSysfs::reference_board();
        let discovery = discover(&fake.sysfs(), None, &Default::default()).unwrap();
        let snapshot = crate::snapshot::take(&discovery.channels, Default::default(), 1).unwrap();
        let state =
            std::env::temp_dir().join(format!("nicecooling-release-{}", std::process::id()));
        std::fs::create_dir_all(&state).unwrap();
        let mut io = SysfsChannels::new(&discovery.channels, Lease::new(&state), "boot".into())
            .with_snapshot(Some(snapshot));
        io.write("nct6798_pwm3", 128, true).unwrap();
        io.write("nct6798_pwm4", 77, true).unwrap();

        io.release("nct6798_pwm3").unwrap();
        assert_eq!(
            io.read("nct6798_pwm3").unwrap(),
            Readback { raw: 96, enable: 5 }
        );
        assert_eq!(
            io.read("nct6798_pwm4").unwrap(),
            Readback { raw: 77, enable: 1 }
        );
        std::fs::remove_dir_all(state).unwrap();
    }
}
