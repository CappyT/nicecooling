//! The tests use the simulated Leakshield, the simulated USB bus of the daemon tests and a private
//! D-Bus daemon. None of them reaches `/dev`, `/sys`, the USB bus or the system bus.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use nicecooling_protocol::sim::{Request, SimDevice};

use super::*;

#[allow(dead_code)]
#[path = "../../../crates/nicecoolingd/tests/support/bus.rs"]
mod bus;
#[allow(dead_code)]
#[path = "../../../crates/nicecoolingd/tests/support/sim_usb.rs"]
mod sim_usb;

use bus::PrivateBus;
use sim_usb::SimUsb;

const PATH: &str = "1-2.3";
const SAVE_SETTINGS: u32 = 2;

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("nicecooling-verify-{}-{id}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Rig {
    usb: SimUsb,
    sim: SimDevice,
    state: TempDir,
    bus: PrivateBus,
    original: Vec<u8>,
}

impl Rig {
    fn new() -> Self {
        let sim = SimDevice::fw1022();
        let usb = SimUsb::default();
        usb.present(sim_usb::device(PATH, 0x0100, 3), &sim);
        Self {
            original: sim.report3(),
            usb,
            sim,
            state: TempDir::new(),
            bus: PrivateBus::start(),
        }
    }

    fn serial(&self) -> String {
        self.sim.report1().serial_text()
    }

    fn options(&self, field: &str, value: i64) -> Options {
        Options {
            field: field.to_owned(),
            value,
            confirm_serial: Some(self.serial()),
            state_dir: self.state.0.clone(),
            save_delay: Duration::ZERO,
            reopen_timeout: Duration::from_secs(1),
        }
    }

    /// A replug that runs `hook` while the device is off the bus.
    fn replug(&self, hook: impl FnMut(&SimDevice) + 'static) -> SimReplug {
        SimReplug {
            usb: self.usb.clone(),
            sim: self.sim.clone(),
            calls: 0,
            hook: Box::new(hook),
            power_cycle: true,
        }
    }

    async fn run(
        &self,
        opts: &Options,
        replug: &mut SimReplug,
    ) -> (Result<Verdict, Error>, String) {
        let connection = self.bus.connect().await;
        let mut out = Vec::new();
        let result = run(&connection, &self.usb, opts, replug, &mut out).await;
        (result, String::from_utf8(out).unwrap())
    }

    fn save_settings_count(&self) -> usize {
        self.sim
            .commands()
            .iter()
            .filter(|c| c.command == SAVE_SETTINGS)
            .count()
    }

    fn report3_writes(&self) -> usize {
        self.sim
            .requests()
            .iter()
            .filter(|r| matches!(r, Request::SetFeature { id: 3, .. }))
            .count()
    }
}

struct SimReplug {
    usb: SimUsb,
    sim: SimDevice,
    calls: usize,
    hook: Box<dyn FnMut(&SimDevice)>,
    /// A real replug powers the device off and on, so `power_up_count` grows.
    power_cycle: bool,
}

impl Replug for SimReplug {
    fn replug(&mut self, _say: &mut dyn Write) -> io::Result<()> {
        self.calls += 1;
        self.usb.unplug(PATH);
        (self.hook)(&self.sim);
        if self.power_cycle {
            self.sim.edit_report1(|r| r.power_up_count += 1);
        }
        self.usb.plug(sim_usb::device(PATH, 0x0100, 3), &self.sim);
        Ok(())
    }
}

fn no_hook(_: &SimDevice) {}

#[tokio::test]
async fn refuses_when_name_owned() {
    let rig = Rig::new();
    let owner = rig.bus.connect().await;
    owner.request_name(BUS_NAME).await.unwrap();
    let mut replug = rig.replug(no_hook);
    let (result, _) = rig
        .run(&rig.options("pump_max_speed", 4801), &mut replug)
        .await;
    assert!(matches!(result, Err(Error::NameOwned)), "{result:?}");
    assert!(rig.sim.requests().is_empty());
    assert!(rig.usb.opened().is_empty());
}

#[tokio::test]
async fn refuses_without_confirm_serial() {
    let rig = Rig::new();
    let mut opts = rig.options("pump_max_speed", 4801);
    opts.confirm_serial = None;
    let mut replug = rig.replug(no_hook);
    let (result, _) = rig.run(&opts, &mut replug).await;
    assert!(
        matches!(result, Err(Error::MissingConfirmSerial)),
        "{result:?}"
    );
    assert!(rig.sim.requests().is_empty());
    assert!(rig.usb.opened().is_empty());
}

#[tokio::test]
async fn refuses_wrong_serial() {
    let rig = Rig::new();
    let mut opts = rig.options("pump_max_speed", 4801);
    opts.confirm_serial = Some("1-2".to_owned());
    let mut replug = rig.replug(no_hook);
    let (result, _) = rig.run(&opts, &mut replug).await;
    match result {
        Err(Error::SerialMismatch { confirmed, found }) => {
            assert_eq!(confirmed, "1-2");
            assert_eq!(found, [rig.serial()]);
        }
        other => panic!("{other:?}"),
    }
    // The device was read for its serial and nothing else.
    assert!(
        rig.sim
            .requests()
            .iter()
            .all(|r| matches!(r, Request::ReadInput))
    );
    assert!(!rig.state.0.join("backups").exists());
    assert_eq!(replug.calls, 0);
}

#[tokio::test]
async fn happy_path_prints_registry_change() {
    let rig = Rig::new();
    // The device holds the test value, and only that, when the operator replugs it.
    let at_replug = {
        let original = rig.original.clone();
        move |sim: &SimDevice| {
            let held = sim.report3();
            let changed: Vec<usize> = (1..=747).filter(|&o| held[o] != original[o]).collect();
            // 4800 and 4801 differ in the low byte only.
            assert_eq!(changed, [16]);
            assert_eq!(held[15..17], 4801i16.to_be_bytes());
        }
    };
    let mut replug = rig.replug(at_replug);
    let (result, out) = rig
        .run(&rig.options("pump_max_speed", 4801), &mut replug)
        .await;
    assert_eq!(result.unwrap(), Verdict::Pass, "{out}");
    assert!(out.contains("field pump_max_speed:"), "{out}");
    assert!(
        out.contains("verification_state = \"code-defined\" -> \"verified-write\""),
        "{out}"
    );
    assert_eq!(replug.calls, 1);
    // The restore brought the backup back and saved it.
    assert_eq!(rig.sim.report3(), rig.original);
    assert_eq!(rig.report3_writes(), 2);
    assert_eq!(rig.save_settings_count(), 2);
    let backups = Backups::new(&rig.state.0, &rig.serial()).list().unwrap();
    assert!(!backups.is_empty());
}

#[tokio::test]
async fn unexpected_byte_change_aborts_and_restores() {
    let rig = Rig::new();
    // The device stores another byte than the one written.
    rig.sim
        .tamper_next_set_feature(100, rig.original[100] ^ 0xFF);
    let mut replug = rig.replug(no_hook);
    let (result, out) = rig
        .run(&rig.options("pump_max_speed", 4801), &mut replug)
        .await;
    match &result {
        Ok(Verdict::Failed(why)) => assert!(why.contains("[100]"), "{why}"),
        other => panic!("{other:?}\n{out}"),
    }
    assert_eq!(replug.calls, 0, "the check ends before the replug");
    assert_eq!(rig.sim.report3(), rig.original);
    assert!(
        out.contains("restore: the device holds the backup bytes"),
        "{out}"
    );
    assert!(
        out.contains("verification_state = \"code-defined\" -> \"failed\""),
        "{out}"
    );
}

#[tokio::test]
async fn persistence_failure_prints_failed_state() {
    let rig = Rig::new();
    // The device lost the test value while it was off.
    let lose_value = {
        let original = rig.original.clone();
        move |sim: &SimDevice| sim.change_report3(15, &original[15..17])
    };
    let mut replug = rig.replug(lose_value);
    let (result, out) = rig
        .run(&rig.options("pump_max_speed", 4801), &mut replug)
        .await;
    match &result {
        Ok(Verdict::Failed(why)) => assert!(why.contains("did not persist"), "{why}"),
        other => panic!("{other:?}\n{out}"),
    }
    assert_eq!(replug.calls, 1);
    assert_eq!(rig.sim.report3(), rig.original);
    assert!(
        out.contains("verification_state = \"code-defined\" -> \"failed\""),
        "{out}"
    );
}

#[tokio::test]
async fn repeated_field_writes_copy_0_only() {
    let rig = Rig::new();
    let original = rig.original.clone();
    let mut replug = rig.replug(move |sim| {
        let held = sim.report3();
        assert_eq!(held[716], 10);
        assert_eq!(held[718], original[718], "copy 1 stays");
    });
    let (result, out) = rig
        .run(&rig.options("vacuum_timer_start_hour", 10), &mut replug)
        .await;
    assert_eq!(result.unwrap(), Verdict::Pass, "{out}");
    assert_eq!(rig.sim.report3(), rig.original);
}

#[tokio::test]
async fn refuses_fields_that_need_no_check_or_have_no_safe_value() {
    let rig = Rig::new();
    let mut replug = rig.replug(no_hook);
    for (field, value, refused) in [
        ("signal_output_config", 1, "no safe test value"),
        ("alarm_start_delay", 6, "no safe test value"),
        ("display_page_time", 50, "not a Safety field"),
        ("no_such_field", 1, "no field"),
        ("pump_max_speed", -1, "outside the registry range"),
        // The same value as the device holds.
        ("pump_max_speed", 4800, "holds 4800 already"),
    ] {
        let (result, _) = rig.run(&rig.options(field, value), &mut replug).await;
        let why = result.expect_err(field).to_string();
        assert!(why.contains(refused), "{field} {value}: {why}");
    }
    assert_eq!(rig.report3_writes(), 0);
    assert_eq!(rig.save_settings_count(), 0);
    assert_eq!(replug.calls, 0);
}

#[test]
fn arguments() {
    let parse = |args: &[&str]| parse_args(args.iter().map(|a| (*a).to_owned()));
    let opts = parse(&[
        "--field",
        "pump_max_speed",
        "--value",
        "-3",
        "--confirm-serial",
        "1-2",
    ])
    .unwrap();
    assert_eq!((opts.field.as_str(), opts.value), ("pump_max_speed", -3));
    assert_eq!(opts.confirm_serial.as_deref(), Some("1-2"));
    assert_eq!(opts.state_dir, PathBuf::from("/var/lib/nicecooling"));
    // The serial is checked by `run`, so the parser lets its absence through.
    assert!(
        parse(&["--field", "a", "--value", "1"])
            .unwrap()
            .confirm_serial
            .is_none()
    );
    assert!(parse(&["--value", "1"]).is_err());
    assert!(parse(&["--field", "a"]).is_err());
    assert!(parse(&["--field", "a", "--value", "x"]).is_err());
    assert!(parse(&["--field", "a", "--value", "1", "--bogus"]).is_err());
}

#[tokio::test]
async fn replug_without_a_power_cycle_aborts_and_restores() {
    let rig = Rig::new();
    let mut replug = rig.replug(|_| {});
    replug.power_cycle = false;
    let (result, out) = rig
        .run(&rig.options("pump_max_speed", 4801), &mut replug)
        .await;
    match &result {
        Err(Error::Aborted {
            cause, restored, ..
        }) => {
            assert!(cause.contains("did not restart"), "{cause}");
            assert!(restored, "{out}");
        }
        other => panic!("{other:?}\n{out}"),
    }
    assert_eq!(rig.sim.report3(), rig.original);
    assert!(!out.contains("verification_state ="), "{out}");
}
