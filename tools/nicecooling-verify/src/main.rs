//! The Verification tool (task 5.12, `docs/hardware-checks/verification-writes.md`). It runs the
//! Verification writes of one or more Safety fields of the reference Leakshield with one replug and
//! prints the change that the registry needs. The daemon must not run: the tool refuses while it
//! owns the bus name.

use std::collections::BTreeSet;
use std::fmt;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nicecooling_api::ids::BUS_NAME;
use nicecooling_protocol::device::DeviceError;
use nicecooling_protocol::registry::{Field, Registry, RiskClass, VerificationState};
use nicecooling_protocol::report1::Report1;
use nicecooling_protocol::report3::Report3;
use nicecoolingd::leakshield::DeviceIo;
use nicecoolingd::leakshield::backup::{Backups, Reason};
use nicecoolingd::leakshield::save_settings::{self, SAVE_SETTINGS_DELAY};
use nicecoolingd::leakshield::write::{
    FieldEdit, WriteEnv, WriteError, take_backup, write_fields_under, write_verified,
};
use nicecoolingd::sources::hotplug::UsbDevice;
use nicecoolingd::sources::usb::{NusbBus, UsbBus};

#[cfg(test)]
mod tests;

const USAGE: &str = "usage: nicecooling-verify --field <field id> --value <raw value> \
[--field <field id> --value <raw value> ...] --confirm-serial <serial> [--state-dir <dir>]";

/// The code does not define the values of these fields, so no test value is safe (owner decision,
/// gate 2.14). They stay read-only.
/// `device_profile` is not here for a lack of a value: a write of N stores report 3 into slot N of
/// the device, so a test value overwrites a stored Profile (owner decision).
const NO_SAFE_TEST_VALUE: [&str; 3] = [
    "signal_output_config",
    "alarm_start_delay",
    "device_profile",
];

/// How long the tool waits for the device to come back after the replug.
const REOPEN_TIMEOUT: Duration = Duration::from_secs(30);
const REOPEN_POLL: Duration = Duration::from_millis(250);

const REGISTRY_FILE: &str = "crates/nicecooling-protocol/registry/leakshield.toml";

/// One field under test and its test value.
#[derive(Debug, Clone)]
pub struct Pair {
    pub field: String,
    /// The raw value to write. For a field with repeats it goes to copy 0 only.
    pub value: i64,
}

#[derive(Debug, Clone)]
pub struct Options {
    /// The fields of the group, in the order of the command line.
    pub pairs: Vec<Pair>,
    pub confirm_serial: Option<String>,
    /// Where the backups go: the state directory of the daemon, so that the daemon can restore them.
    pub state_dir: PathBuf,
    /// The time between the report 3 write and the save-settings command.
    pub save_delay: Duration,
    pub reopen_timeout: Duration,
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Options, String> {
    let (mut pairs, mut confirm_serial) = (Vec::new(), None);
    // A `--value` belongs to the `--field` right before it.
    let mut field: Option<String> = None;
    let mut state_dir = PathBuf::from("/var/lib/nicecooling");
    let mut args = args;
    while let Some(arg) = args.next() {
        let mut next = |what: &str| args.next().ok_or(format!("{arg} needs {what}"));
        match arg.as_str() {
            "--field" => {
                if let Some(open) = &field {
                    return Err(format!(
                        "--field {open} needs its --value before the next --field"
                    ));
                }
                field = Some(next("a field id")?);
            }
            "--value" => {
                let text = next("a raw value")?;
                let value = text
                    .parse::<i64>()
                    .map_err(|_| format!("{text:?} is not an integer"))?;
                let field = field
                    .take()
                    .ok_or(format!("--value {text} needs a --field before it"))?;
                pairs.push(Pair { field, value });
            }
            "--confirm-serial" => confirm_serial = Some(next("a serial")?),
            "--state-dir" => state_dir = PathBuf::from(next("a path")?),
            other => return Err(format!("unknown argument {other}")),
        }
    }
    if let Some(open) = field {
        return Err(format!("--field {open} needs its --value"));
    }
    if pairs.is_empty() {
        return Err("--field and --value are required".to_owned());
    }
    Ok(Options {
        pairs,
        confirm_serial,
        state_dir,
        save_delay: SAVE_SETTINGS_DELAY,
        reopen_timeout: REOPEN_TIMEOUT,
    })
}

/// The tool did not run the check, or it stopped before it had a result.
#[derive(Debug)]
pub enum Error {
    MissingConfirmSerial,
    NameOwned,
    SystemBus(zbus::Error),
    DuplicateField(String),
    UnknownField(String),
    NotSafetyField(String),
    NoSafeTestValue(String),
    ValueOutOfRange {
        field: String,
        value: i64,
    },
    ValueUnchanged {
        field: String,
        value: i64,
    },
    NoDevice(String),
    SerialMismatch {
        confirmed: String,
        found: Vec<String>,
    },
    /// A step failed before the first write, so the device holds its original settings.
    Setup(String),
    /// A step failed after the first write without a verdict on the field. `restored` tells
    /// whether the device holds the backup again.
    Aborted {
        cause: String,
        backup: String,
        restored: bool,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingConfirmSerial => {
                write!(
                    f,
                    "--confirm-serial <serial> is required: the tool writes a real device"
                )
            }
            Self::NameOwned => write!(
                f,
                "{BUS_NAME} is owned on the system bus: stop nicecoolingd first"
            ),
            Self::SystemBus(e) => write!(
                f,
                "cannot ask the system bus, so the tool cannot tell that the daemon is stopped: {e}"
            ),
            Self::DuplicateField(id) => {
                write!(f, "{id} is in the group twice: each field needs one pair")
            }
            Self::UnknownField(id) => write!(f, "the registry has no field {id}"),
            Self::NotSafetyField(id) => {
                write!(
                    f,
                    "{id} is not a Safety field: it needs no Verification write"
                )
            }
            Self::NoSafeTestValue(id) => write!(
                f,
                "{id} has no safe test value: it has no Verification write and stays read-only"
            ),
            Self::ValueOutOfRange { field, value } => {
                write!(f, "{value} is outside the registry range of {field}")
            }
            Self::ValueUnchanged { field, value } => write!(
                f,
                "{field} holds {value} already: the test value must differ from the current value"
            ),
            Self::NoDevice(why) => write!(f, "no Leakshield to verify: {why}"),
            Self::SerialMismatch { confirmed, found } => write!(
                f,
                "--confirm-serial {confirmed} matches no Leakshield on the bus (found {})",
                found.join(", ")
            ),
            Self::Setup(why) => write!(f, "{why}. The device was not written."),
            Self::Aborted {
                cause,
                backup,
                restored: true,
            } => write!(
                f,
                "stopped without a result: {cause}. The device holds the backup {backup} again. \
                 No registry change."
            ),
            Self::Aborted {
                cause,
                backup,
                restored: false,
            } => write!(
                f,
                "stopped without a result: {cause}. THE RESTORE FAILED: the device may hold the \
                 test value. Restore the backup {backup} (RestoreBackup of the daemon, state \
                 directory of the backups) before you use the device. No registry change."
            ),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    Failed(String),
    /// The run stopped before it tested the field to the end.
    NotRun,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pass => write!(f, "Pass"),
            Self::Failed(why) => write!(f, "Failed({why})"),
            Self::NotRun => write!(f, "NotRun"),
        }
    }
}

/// How the operator replugs the USB cable. The tests replug the simulated device.
pub trait Replug {
    fn replug(&mut self, say: &mut dyn Write) -> io::Result<()>;
}

struct Terminal;

impl Replug for Terminal {
    fn replug(&mut self, say: &mut dyn Write) -> io::Result<()> {
        writeln!(
            say,
            "ACTION: unplug the USB cable of the Leakshield, wait 5 s, plug it in again, then \
             press Enter."
        )?;
        say.flush()?;
        let mut line = String::new();
        // End of input is not a replug: the persistence check would read the same session.
        if io::stdin().lock().read_line(&mut line)? == 0 {
            return Err(io::ErrorKind::UnexpectedEof.into());
        }
        Ok(())
    }
}

/// Stops the run: `Failed` is a result on every field of the group, `Error` is a step that gave
/// none.
enum Stop {
    Failed(String),
    Error(String),
}

/// A field of the group with its test value.
#[derive(Clone)]
struct Plan {
    field: Field,
    value: i64,
    /// The state of the field in the compiled-in registry.
    from: VerificationState,
}

impl Plan {
    fn first_offset(&self) -> usize {
        self.field.element_offsets()[0]
    }

    /// The bytes that the write changes.
    fn bytes(&self) -> std::ops::Range<usize> {
        let first = self.first_offset();
        first..first + self.field.value_type.scalar.size()
    }

    fn holds(&self, report: &Report3) -> bool {
        self.field.read_raw(report, self.first_offset()) == self.value
    }
}

fn state_name(state: VerificationState) -> &'static str {
    match state {
        VerificationState::CodeDefined => "code-defined",
        VerificationState::PlausibleRead => "plausible-read",
        VerificationState::VerifiedWrite => "verified-write",
        VerificationState::Failed => "failed",
    }
}

fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_micros() as u64)
}

/// The daemon holds the device and writes the same report 3: the tool must not run next to it.
async fn check_name_free(connection: &zbus::Connection) -> Result<(), Error> {
    let name = zbus::names::BusName::try_from(BUS_NAME).map_err(Error::SystemBus)?;
    let proxy = zbus::fdo::DBusProxy::new(connection)
        .await
        .map_err(Error::SystemBus)?;
    let owned = proxy
        .name_has_owner(name)
        .await
        .map_err(|e| Error::SystemBus(e.into()))?;
    if owned { Err(Error::NameOwned) } else { Ok(()) }
}

enum Probe {
    Found(DeviceIo, Box<Report1>),
    Mismatch(Vec<String>),
    NotYet(String),
}

fn probe(bus: &dyn UsbBus, serial: &str) -> Probe {
    let devices = match bus.list() {
        Ok(devices) => devices,
        Err(e) => return Probe::NotYet(format!("cannot list the USB bus: {e}")),
    };
    let candidates: Vec<UsbDevice> = devices
        .into_iter()
        .filter(|d| d.is_leakshield() && !d.in_bootloader())
        .collect();
    if candidates.is_empty() {
        return Probe::NotYet("no Leakshield on the USB bus".to_owned());
    }
    let (mut found, mut pending) = (Vec::new(), None);
    for device in &candidates {
        match bus.open(device) {
            Ok(io) => match io.read_report1() {
                Ok(report1) if report1.serial_text() == serial => {
                    return Probe::Found(io, Box::new(report1));
                }
                Ok(report1) => found.push(report1.serial_text()),
                Err(e) => pending = Some(format!("{}: {e}", device.path)),
            },
            Err(e) => pending = Some(format!("{}: {e}", device.path)),
        }
    }
    match pending {
        Some(why) => Probe::NotYet(why),
        None => Probe::Mismatch(found),
    }
}

/// Opens the Leakshield with `serial`. A device with another serial ends the search at once.
fn open_by_serial(
    bus: &dyn UsbBus,
    serial: &str,
    wait: Duration,
) -> Result<(DeviceIo, Report1), Error> {
    let deadline = Instant::now() + wait;
    loop {
        match probe(bus, serial) {
            Probe::Found(io, report1) => return Ok((io, *report1)),
            Probe::Mismatch(found) => {
                return Err(Error::SerialMismatch {
                    confirmed: serial.to_owned(),
                    found,
                });
            }
            Probe::NotYet(why) if Instant::now() >= deadline => return Err(Error::NoDevice(why)),
            Probe::NotYet(_) => std::thread::sleep(REOPEN_POLL),
        }
    }
}

struct Check<'a> {
    bus: &'a dyn UsbBus,
    opts: &'a Options,
    serial: &'a str,
    /// The compiled-in registry with the tested fields forced writable. Only the tool holds it.
    registry: Registry,
    backups: Backups,
    backup: String,
    /// Report 3 of the backup.
    original: Report3,
    plans: Vec<Plan>,
    io: DeviceIo,
    report1: Report1,
    /// True once the device may hold a test value.
    touched: bool,
    say: &'a mut dyn Write,
    replug: &'a mut dyn Replug,
}

impl Check<'_> {
    fn line(&mut self, text: impl fmt::Display) {
        // A closed output leaves nothing to report to.
        let _ = writeln!(self.say, "{text}");
    }

    fn env(&self) -> WriteEnv<'_> {
        WriteEnv {
            io: &self.io,
            registry: &self.registry,
            backups: &self.backups,
        }
    }

    /// Offsets that differ from the backup and that the writes of the first `written` fields do not
    /// explain.
    fn unexpected(&self, report: &Report3, written: usize) -> Vec<usize> {
        let explained: BTreeSet<usize> =
            self.plans[..written].iter().flat_map(Plan::bytes).collect();
        report
            .differing_offsets(&self.original, &self.registry.report3.volatile_mask)
            .into_iter()
            .filter(|offset| !explained.contains(offset))
            .collect()
    }

    fn read_report3(&self) -> Result<Report3, Stop> {
        match self.io.read_report3() {
            Ok(report) => Ok(report),
            // A report that does not decode has a bad CRC or size: the device holds something else.
            Err(DeviceError::BadReport(e)) => {
                Err(Stop::Failed(format!("report 3 does not decode: {e}")))
            }
            Err(e) => Err(Stop::Error(format!("read of report 3: {e}"))),
        }
    }

    /// Sends the save-settings command after the delay of the daemon.
    fn save_settings(&mut self) -> Result<(), String> {
        std::thread::sleep(self.opts.save_delay);
        let command = save_settings::command(&self.registry)
            .ok_or("the registry has no save_settings command")?;
        self.io
            .send_command(command)
            .map_err(|e| format!("save-settings command: {e}"))
    }

    /// The verdicts of a write that failed: only field `index` has a result.
    fn failed_at(&self, index: usize, why: String) -> Vec<Verdict> {
        let mut verdicts = vec![Verdict::NotRun; self.plans.len()];
        verdicts[index] = Verdict::Failed(why);
        verdicts
    }

    /// The device after the write of field `index`: only the bytes of the fields written so far
    /// differ from the backup, and the field holds its test value.
    fn check_step(&self, report: &Report3, index: usize) -> Result<(), String> {
        let extra = self.unexpected(report, index + 1);
        if !extra.is_empty() {
            return Err(format!(
                "read-back after the write: bytes changed that the write does not explain, at \
                 offsets {extra:?}"
            ));
        }
        if !self.plans[index].holds(report) {
            return Err(
                "read-back after the write: the field does not hold the test value".to_owned(),
            );
        }
        Ok(())
    }

    /// The verdict of each field once all writes are done. A byte that no field explains fails the
    /// whole group. A field that does not hold its value fails alone.
    fn judge(&self, report: &Report3, when: &str) -> Vec<Verdict> {
        let extra = self.unexpected(report, self.plans.len());
        if !extra.is_empty() {
            let why = format!(
                "{when}: bytes changed that the write does not explain, at offsets {extra:?}"
            );
            return vec![Verdict::Failed(why); self.plans.len()];
        }
        self.plans
            .iter()
            .map(|plan| {
                if plan.holds(report) {
                    Verdict::Pass
                } else {
                    Verdict::Failed(format!("{when}: the field does not hold the test value"))
                }
            })
            .collect()
    }

    /// Writes each field, saves, reads back, replugs, reads back. The verdicts follow the order of
    /// the fields.
    fn exercise(&mut self) -> Result<Vec<Verdict>, Stop> {
        // The report that the previous step read back: the base of the next write.
        let mut base = self.original.clone();
        for index in 0..self.plans.len() {
            let plan = self.plans[index].clone();
            let mut raw: Vec<i64> = plan
                .field
                .element_offsets()
                .iter()
                .map(|&o| plan.field.read_raw(&base, o))
                .collect();
            let before = raw[0];
            raw[0] = plan.value;
            let edit = FieldEdit {
                field: plan.field.id.clone(),
                raw,
            };
            self.line(format!(
                "write: {} {before} -> {} at offset {}",
                plan.field.id,
                plan.value,
                plan.first_offset()
            ));
            let written =
                write_fields_under(&self.env(), &self.report1, &base, &[edit], &self.backup);
            // These errors come before the first report 3 write: the device holds the previous bytes.
            self.touched |= !matches!(
                written,
                Err(WriteError::Refused(_)
                    | WriteError::OutOfRange { .. }
                    | WriteError::DeviceChanged
                    | WriteError::Backup(_)
                    | WriteError::NotSupported)
            );
            let written = match written {
                Ok(written) => written,
                Err(WriteError::VerifyFailed { offsets, .. }) => {
                    let why = format!(
                        "the read-back differs from the written report at offsets {offsets:?}"
                    );
                    return Ok(self.failed_at(index, why));
                }
                Err(e) => return Err(Stop::Error(format!("write: {e}"))),
            };
            if let Err(why) = self.check_step(&written.report3, index) {
                return Ok(self.failed_at(index, why));
            }
            base = written.report3;
        }

        self.line("save-settings: waiting for the delay, then sending the command");
        self.save_settings().map_err(Stop::Error)?;
        let report = self.read_report3()?;
        let verdicts = self.judge(&report, "read-back after save-settings");
        if verdicts.iter().any(|v| matches!(v, Verdict::Failed(_))) {
            // The replug does not run: no field has a persistence result.
            return Ok(verdicts
                .into_iter()
                .map(|v| {
                    if v == Verdict::Pass {
                        Verdict::NotRun
                    } else {
                        v
                    }
                })
                .collect());
        }
        self.line("read-back: only the expected bytes changed");

        let power_ups = self.report1.power_up_count;
        self.replug
            .replug(self.say)
            .map_err(|e| Stop::Error(format!("replug prompt: {e}")))?;
        let no_restart = || {
            Stop::Error(
                "power_up_count did not change: the device did not restart, so the persistence \
                 check would prove nothing"
                    .to_owned(),
            )
        };
        // A device that did not restart still answers on the old handle. Its bulk interface stays
        // claimed there, so a reopen would fail with EBUSY.
        if self
            .io
            .read_report1()
            .is_ok_and(|report1| report1.power_up_count == power_ups)
        {
            return Err(no_restart());
        }
        let (io, report1) = open_by_serial(self.bus, self.serial, self.opts.reopen_timeout)
            .map_err(|e| Stop::Error(e.to_string()))?;
        self.io = io;
        self.report1 = report1;
        if self.report1.power_up_count == power_ups {
            return Err(no_restart());
        }
        let report = self.read_report3()?;
        let verdicts = self.judge(
            &report,
            "read-back after the replug (the value did not persist)",
        );
        if verdicts.iter().all(|v| *v == Verdict::Pass) {
            self.line("persistence: the test values survived the replug");
        }
        Ok(verdicts)
    }

    /// Writes back the report 3 that the tool read before its first write, saves it and checks
    /// that the device holds those bytes. It takes no backup, so a full or read-only state
    /// directory and the backup retention cannot stop it. A device that left the bus during the run
    /// is opened again once.
    fn restore(&mut self) -> Result<(), String> {
        self.line(format!("restore: the report 3 of backup {}", self.backup));
        if let Err(first) = write_verified(&self.env(), &self.original, &self.backup) {
            self.line(format!("restore: {first}; opening the device again"));
            let (io, report1) = open_by_serial(self.bus, self.serial, self.opts.reopen_timeout)
                .map_err(|e| format!("restore: {first}; reopen: {e}"))?;
            self.io = io;
            self.report1 = report1;
            write_verified(&self.env(), &self.original, &self.backup)
                .map_err(|e| format!("restore: {e}"))?;
        }
        self.save_settings()?;
        let report = self
            .io
            .read_report3()
            .map_err(|e| format!("read of report 3 after the restore: {e}"))?;
        let mask = &self.registry.report3.volatile_mask;
        let offsets = report.differing_offsets(&self.original, mask);
        if offsets.is_empty() {
            self.line("restore: the device holds the backup bytes");
            Ok(())
        } else {
            Err(format!(
                "the device differs from the backup after the restore at offsets {offsets:?}"
            ))
        }
    }
}

fn verify(
    bus: &dyn UsbBus,
    opts: &Options,
    serial: &str,
    replug: &mut dyn Replug,
    say: &mut dyn Write,
) -> Result<Vec<(String, Verdict)>, Error> {
    let compiled = Registry::leakshield();
    let mut plans: Vec<Plan> = Vec::new();
    for pair in &opts.pairs {
        if plans.iter().any(|plan| plan.field.id == pair.field) {
            return Err(Error::DuplicateField(pair.field.clone()));
        }
        let field = compiled
            .field(&pair.field)
            .ok_or_else(|| Error::UnknownField(pair.field.clone()))?
            .clone();
        if field.risk_class != RiskClass::Safety {
            return Err(Error::NotSafetyField(field.id));
        }
        if NO_SAFE_TEST_VALUE.contains(&field.id.as_str()) {
            return Err(Error::NoSafeTestValue(field.id));
        }
        if !field.raw_in_range(pair.value) {
            return Err(Error::ValueOutOfRange {
                field: field.id,
                value: pair.value,
            });
        }
        plans.push(Plan {
            from: field.verification_state,
            value: pair.value,
            field,
        });
    }
    let (io, report1) = open_by_serial(bus, serial, Duration::ZERO)?;
    let _ = writeln!(
        say,
        "device: serial {}, firmware {}, power_up_count {}",
        report1.serial_text(),
        report1.firmware_version,
        report1.power_up_count
    );

    // A Verification write writes a field that the registry does not allow yet. Only this copy
    // allows it, and only for the fields under test.
    let mut registry = compiled;
    for plan in &plans {
        if let Some(entry) = registry.field.iter_mut().find(|f| f.id == plan.field.id) {
            entry.verification_state = VerificationState::VerifiedWrite;
        }
    }
    let backups = Backups::new(&opts.state_dir, serial);
    let env = WriteEnv {
        io: &io,
        registry: &registry,
        backups: &backups,
    };
    let backup = take_backup(&env, &report1, Reason::BeforeWrite, now_us())
        .map_err(|e| Error::Setup(format!("backup: {e}")))?;
    let _ = writeln!(say, "backup: {backup}");
    let original = backups
        .get(&backup)
        .map_err(|e| e.to_string())
        .and_then(|set| Report3::decode(&set.report3).map_err(|e| e.to_string()))
        .map_err(|e| Error::Setup(format!("the backup does not read back: {e}")))?;
    for plan in &plans {
        if plan.field.read_raw(&original, plan.first_offset()) == plan.value {
            return Err(Error::ValueUnchanged {
                field: plan.field.id.clone(),
                value: plan.value,
            });
        }
    }

    let mut check = Check {
        bus,
        opts,
        serial,
        registry,
        backups,
        backup: backup.clone(),
        original,
        plans,
        io,
        report1,
        touched: false,
        say,
        replug,
    };
    let stopped = check.exercise();
    if let (Err(Stop::Error(why)), false) = (&stopped, check.touched) {
        return Err(Error::Setup(why.clone()));
    }
    let restored = check.restore();
    if let Err(why) = &restored {
        check.line(format!("RESTORE FAILED: {why}"));
    }
    let verdicts = match stopped {
        Ok(verdicts) => verdicts,
        Err(Stop::Failed(why)) => vec![Verdict::Failed(why); check.plans.len()],
        Err(Stop::Error(cause)) => {
            return Err(Error::Aborted {
                cause,
                backup,
                restored: restored.is_ok(),
            });
        }
    };
    // A device that does not hold the backup again gives no result: the fields get no registry
    // change, and the loud error asks for the restore.
    if let Err(why) = restored {
        let lines: Vec<String> = check
            .plans
            .iter()
            .zip(&verdicts)
            .map(|(plan, verdict)| {
                format!("result: {} {verdict} (no registry change)", plan.field.id)
            })
            .collect();
        for line in lines {
            check.line(line);
        }
        return Err(Error::Aborted {
            cause: why,
            backup,
            restored: false,
        });
    }

    let results: Vec<(String, Verdict)> = check
        .plans
        .iter()
        .map(|plan| plan.field.id.clone())
        .zip(verdicts)
        .collect();
    for (field, verdict) in &results {
        check.line(format!("result: {field} {verdict}"));
    }
    for (plan, (_, verdict)) in check.plans.clone().iter().zip(&results) {
        let to = match verdict {
            Verdict::Pass => VerificationState::VerifiedWrite,
            Verdict::Failed(_) => VerificationState::Failed,
            Verdict::NotRun => continue,
        };
        check.line(format!(
            "registry change in {REGISTRY_FILE}, field {}:",
            plan.field.id
        ));
        check.line(format!(
            "  verification_state = \"{}\" -> \"{}\"",
            state_name(plan.from),
            state_name(to)
        ));
    }
    Ok(results)
}

/// The refusals come first: the serial, then the bus name. Nothing touches the device before both.
pub async fn run(
    connection: &zbus::Connection,
    bus: &dyn UsbBus,
    opts: &Options,
    replug: &mut dyn Replug,
    say: &mut dyn Write,
) -> Result<Vec<(String, Verdict)>, Error> {
    let serial = opts
        .confirm_serial
        .as_deref()
        .ok_or(Error::MissingConfirmSerial)?;
    check_name_free(connection).await?;
    verify(bus, opts, serial, replug, say)
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let opts = match parse_args(std::env::args().skip(1)) {
        Ok(opts) => opts,
        Err(why) => {
            eprintln!("nicecooling-verify: {why}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    let connection = match zbus::Connection::system().await {
        Ok(connection) => connection,
        Err(e) => {
            eprintln!("nicecooling-verify: {}", Error::SystemBus(e));
            return ExitCode::from(2);
        }
    };
    let bus = NusbBus::new("/sys".into(), "/dev".into());
    match run(&connection, &*bus, &opts, &mut Terminal, &mut io::stdout()).await {
        Ok(results) if results.iter().all(|(_, verdict)| *verdict == Verdict::Pass) => {
            ExitCode::SUCCESS
        }
        Ok(_) => ExitCode::from(1),
        Err(e) => {
            eprintln!("nicecooling-verify: {e}");
            ExitCode::from(2)
        }
    }
}
