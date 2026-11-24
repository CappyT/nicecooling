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
        self.group_options(&[(field, value)])
    }

    fn group_options(&self, pairs: &[(&str, i64)]) -> Options {
        Options {
            pairs: pairs
                .iter()
                .map(|&(field, value)| Pair {
                    field: field.to_owned(),
                    value,
                })
                .collect(),
            confirm_serial: Some(self.serial()),
            state_dir: self.state.0.clone(),
            save_delay: Duration::ZERO,
        }
    }

    /// The first field of the group has the first verdict.
    async fn run_group(&self, opts: &Options) -> (Result<Vec<(String, Verdict)>, Error>, String) {
        let mut out = Tripwire::default();
        let result = self.run_into(opts, &mut out).await;
        (result, out.text())
    }

    async fn run_into(
        &self,
        opts: &Options,
        out: &mut dyn Write,
    ) -> Result<Vec<(String, Verdict)>, Error> {
        let connection = self.bus.connect().await;
        run(&connection, &self.usb, opts, out).await
    }

    /// A run of one field.
    async fn run(&self, opts: &Options) -> (Result<Verdict, Error>, String) {
        let (result, out) = self.run_group(opts).await;
        let result = result.map(|mut results| {
            assert_eq!(results.len(), 1, "{results:?}");
            results.remove(0).1
        });
        (result, out)
    }

    /// The test value of `field`: one more than the value that the device holds.
    fn test_value(&self, field: &str) -> i64 {
        let registry = Registry::leakshield();
        let field = registry.field(field).unwrap();
        let original = Report3::decode(&self.original).unwrap();
        field.read_raw(&original, field.element_offsets()[0]) + 1
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

/// The output of a run. It runs `hook` once, when the text so far holds `trigger`: a test uses it
/// to change the device between two steps of the tool.
#[derive(Default)]
struct Tripwire {
    bytes: Vec<u8>,
    trigger: &'static str,
    hook: Option<Box<dyn FnOnce()>>,
}

impl Tripwire {
    fn new(trigger: &'static str, hook: impl FnOnce() + 'static) -> Self {
        Self {
            bytes: Vec::new(),
            trigger,
            hook: Some(Box::new(hook)),
        }
    }

    fn text(&self) -> String {
        String::from_utf8(self.bytes.clone()).unwrap()
    }
}

impl Write for Tripwire {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.bytes.extend_from_slice(buf);
        if self.hook.is_some() && self.text().contains(self.trigger) {
            (self.hook.take().unwrap())();
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[tokio::test]
async fn refuses_when_name_owned() {
    let rig = Rig::new();
    let owner = rig.bus.connect().await;
    owner.request_name(BUS_NAME).await.unwrap();
    let (result, _) = rig.run(&rig.options("pump_max_speed", 4801)).await;
    assert!(matches!(result, Err(Error::NameOwned)), "{result:?}");
    assert!(rig.sim.requests().is_empty());
    assert!(rig.usb.opened().is_empty());
}

#[tokio::test]
async fn refuses_without_confirm_serial() {
    let rig = Rig::new();
    let mut opts = rig.options("pump_max_speed", 4801);
    opts.confirm_serial = None;
    let (result, _) = rig.run(&opts).await;
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
    let (result, _) = rig.run(&opts).await;
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
}

#[tokio::test]
async fn happy_path_prints_registry_change() {
    let rig = Rig::new();
    // The device holds the test value, and only that, when the restore starts.
    let mut out = Tripwire::new("restore: the report 3", {
        let sim = rig.sim.clone();
        let original = rig.original.clone();
        move || {
            let held = sim.report3();
            let changed: Vec<usize> = (1..=747).filter(|&o| held[o] != original[o]).collect();
            // 4800 and 4801 differ in the low byte only.
            assert_eq!(changed, [16]);
            assert_eq!(held[15..17], 4801i16.to_be_bytes());
        }
    });
    let result = rig
        .run_into(&rig.options("pump_max_speed", 4801), &mut out)
        .await;
    let out = out.text();
    let results = result.unwrap();
    assert_eq!(verdicts(&results), [Verdict::Pass], "{out}");
    assert!(out.contains("field pump_max_speed:"), "{out}");
    assert!(out.contains("-> \"verified-write\""), "{out}");
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
    let (result, out) = rig.run(&rig.options("pump_max_speed", 4801)).await;
    match &result {
        Ok(Verdict::Failed(why)) => assert!(why.contains("[100]"), "{why}"),
        other => panic!("{other:?}\n{out}"),
    }
    assert_eq!(rig.sim.report3(), rig.original);
    assert!(
        out.contains("restore: the device holds the backup bytes"),
        "{out}"
    );
    assert!(out.contains("-> \"failed\""), "{out}");
}

#[tokio::test]
async fn repeated_field_writes_copy_0_only() {
    let rig = Rig::new();
    let mut out = Tripwire::new("restore: the report 3", {
        let sim = rig.sim.clone();
        let original = rig.original.clone();
        move || {
            let held = sim.report3();
            assert_eq!(held[716], 10);
            assert_eq!(held[718], original[718], "copy 1 stays");
        }
    });
    let result = rig
        .run_into(&rig.options("vacuum_timer_start_hour", 10), &mut out)
        .await;
    let out = out.text();
    assert_eq!(verdicts(&result.unwrap()), [Verdict::Pass], "{out}");
    assert_eq!(rig.sim.report3(), rig.original);
}

#[tokio::test]
async fn refuses_fields_that_need_no_check_or_have_no_safe_value() {
    let rig = Rig::new();
    for (field, value, refused) in [
        ("signal_output_config", 1, "no safe test value"),
        ("alarm_start_delay", 6, "no safe test value"),
        ("display_page_time", 50, "not a Safety field"),
        ("no_such_field", 1, "no field"),
        ("pump_max_speed", -1, "outside the registry range"),
        // The same value as the device holds.
        ("pump_max_speed", 4800, "holds 4800 already"),
    ] {
        let (result, _) = rig.run(&rig.options(field, value)).await;
        let why = result.expect_err(field).to_string();
        assert!(why.contains(refused), "{field} {value}: {why}");
    }
    assert_eq!(rig.report3_writes(), 0);
    assert_eq!(rig.save_settings_count(), 0);
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
    assert_eq!(opts.pairs.len(), 1);
    assert_eq!(
        (opts.pairs[0].field.as_str(), opts.pairs[0].value),
        ("pump_max_speed", -3)
    );
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

#[test]
fn arguments_repeat_in_pairs() {
    let parse = |args: &[&str]| parse_args(args.iter().map(|a| (*a).to_owned()));
    let opts = parse(&[
        "--field", "b", "--value", "2", "--field", "a", "--value", "-1", "--field", "c", "--value",
        "3",
    ])
    .unwrap();
    let pairs: Vec<(&str, i64)> = opts
        .pairs
        .iter()
        .map(|p| (p.field.as_str(), p.value))
        .collect();
    assert_eq!(pairs, [("b", 2), ("a", -1), ("c", 3)]);
    // Each --field needs its own --value.
    let why = parse(&["--field", "a", "--field", "b", "--value", "1"]).unwrap_err();
    assert!(why.contains("--field a needs its --value"), "{why}");
    assert!(parse(&["--field", "a", "--value", "1", "--value", "2"]).is_err());
    // A --value belongs to the --field right before it.
    assert!(parse(&["--value", "1", "--field", "a"]).is_err());
    assert!(parse(&["--field", "a", "--value", "1", "--field", "b"]).is_err());
}

const GROUP: [&str; 3] = [
    "reservoir_air_volume_full",
    "pump_max_speed",
    "pump_start_speed",
];

fn group_pairs(rig: &Rig) -> Vec<(&'static str, i64)> {
    GROUP.iter().map(|&f| (f, rig.test_value(f))).collect()
}

fn verdicts(results: &[(String, Verdict)]) -> Vec<Verdict> {
    results.iter().map(|(_, verdict)| verdict.clone()).collect()
}

fn count(out: &str, text: &str) -> usize {
    out.matches(text).count()
}

#[tokio::test]
async fn group_of_three_passes() {
    let rig = Rig::new();
    let pairs = group_pairs(&rig);
    // The device holds the three test values, and only them, when the restore starts.
    let mut out = Tripwire::new("restore: the report 3", {
        let sim = rig.sim.clone();
        let original = rig.original.clone();
        move || {
            let held = sim.report3();
            let changed: Vec<usize> = (1..=747).filter(|&o| held[o] != original[o]).collect();
            assert!(changed.contains(&8) && changed.contains(&16) && changed.contains(&20));
            assert!(
                changed
                    .iter()
                    .all(|o| (7..9).contains(o) || (15..17).contains(o) || (19..21).contains(o))
            );
        }
    });
    let result = rig.run_into(&rig.group_options(&pairs), &mut out).await;
    let out = out.text();
    let results = result.unwrap();
    let fields: Vec<&str> = results.iter().map(|(f, _)| f.as_str()).collect();
    assert_eq!(fields, GROUP, "{out}");
    assert_eq!(verdicts(&results), vec![Verdict::Pass; 3], "{out}");
    for field in GROUP {
        assert!(out.contains(&format!("result: {field} Pass")), "{out}");
        assert!(out.contains(&format!("field {field}:")), "{out}");
    }
    assert_eq!(count(&out, "-> \"verified-write\""), 3, "{out}");
    // Three writes, one for each field, and the restore. One save-settings after the last write
    // and one after the restore.
    assert_eq!(rig.sim.report3(), rig.original);
    assert_eq!(rig.report3_writes(), 4);
    assert_eq!(rig.save_settings_count(), 2);
}

#[tokio::test]
async fn failure_at_the_second_write_stops_the_group_and_restores() {
    let rig = Rig::new();
    let pairs = group_pairs(&rig);
    // The device stores another byte than the one written by the second write only.
    let mut out = Tripwire::new("write: pump_max_speed", {
        let sim = rig.sim.clone();
        let byte = rig.original[100] ^ 0xFF;
        move || sim.tamper_next_set_feature(100, byte)
    });
    let result = rig.run_into(&rig.group_options(&pairs), &mut out).await;
    let out = out.text();
    let results = result.unwrap();
    match verdicts(&results).as_slice() {
        [Verdict::NotRun, Verdict::Failed(why), Verdict::NotRun] => {
            assert!(why.contains("[100]"), "{why}");
        }
        other => panic!("{other:?}\n{out}"),
    }
    assert!(!out.contains("write: pump_start_speed"), "{out}");
    assert_eq!(rig.sim.report3(), rig.original);
    assert!(
        out.contains("result: reservoir_air_volume_full NotRun"),
        "{out}"
    );
    assert!(out.contains("result: pump_start_speed NotRun"), "{out}");
    // Only the failed field has a registry change.
    assert_eq!(count(&out, "registry change"), 1, "{out}");
    assert!(out.contains("field pump_max_speed:"), "{out}");
    assert!(out.contains("-> \"failed\""), "{out}");
    // Two writes and the restore.
    assert_eq!(rig.report3_writes(), 3);
}

#[tokio::test]
async fn a_byte_that_no_field_explains_fails_the_whole_group() {
    let rig = Rig::new();
    let pairs = group_pairs(&rig);
    // A byte that no write explains changes before the read-back after save-settings.
    let mut out = Tripwire::new("save-settings: waiting", {
        let sim = rig.sim.clone();
        let byte = rig.original[100] ^ 0xFF;
        move || sim.change_report3(100, &[byte])
    });
    let result = rig.run_into(&rig.group_options(&pairs), &mut out).await;
    let out = out.text();
    let results = result.unwrap();
    for verdict in verdicts(&results) {
        match verdict {
            Verdict::Failed(why) => assert!(why.contains("[100]"), "{why}"),
            other => panic!("{other:?}\n{out}"),
        }
    }
    assert_eq!(rig.sim.report3(), rig.original);
    assert_eq!(count(&out, "-> \"failed\""), 3, "{out}");
}

#[tokio::test]
async fn group_refuses_a_duplicate_field_before_the_device() {
    let rig = Rig::new();
    let opts = rig.group_options(&[("pump_max_speed", 4801), ("pump_max_speed", 4802)]);
    let (result, _) = rig.run_group(&opts).await;
    match result {
        Err(Error::DuplicateField(field)) => assert_eq!(field, "pump_max_speed"),
        other => panic!("{other:?}"),
    }
    assert!(rig.sim.requests().is_empty());
    assert!(rig.usb.opened().is_empty());
}

#[tokio::test]
async fn group_validates_every_pair_before_the_first_write() {
    let rig = Rig::new();
    for (second, refused) in [
        (("display_page_time", 50), "not a Safety field"),
        (("alarm_start_delay", 6), "no safe test value"),
        (("no_such_field", 1), "no field"),
        (("pump_start_speed", -1), "outside the registry range"),
        // The same value as the device holds.
        (
            ("pump_start_speed", rig.test_value("pump_start_speed") - 1),
            "holds",
        ),
    ] {
        let opts = rig.group_options(&[("pump_max_speed", 4801), second]);
        let (result, _) = rig.run_group(&opts).await;
        let why = result.expect_err(second.0).to_string();
        assert!(why.contains(refused), "{second:?}: {why}");
    }
    assert_eq!(rig.report3_writes(), 0);
    assert_eq!(rig.save_settings_count(), 0);
}

/// The state dir already holds the first backup and 20 newer ones, and the oldest of them holds the
/// state of the device: a former run restored it. The run backup deduplicates to that old id. The
/// writes of the group take no backup of their own, and the restore needs none from the store.
#[tokio::test]
async fn restore_survives_backup_retention() {
    use nicecoolingd::leakshield::backup::BackupSet;
    let rig = Rig::new();
    let backups = Backups::new(&rig.state.0, &rig.serial());
    let current = BackupSet {
        report3: Report3::decode(&rig.sim.report3()).unwrap().encode(),
        report5: rig.sim.report5(),
        report8: rig.sim.report8(),
    };
    let firmware = rig.sim.report1().firmware_version;
    let other = |i: u8| {
        let mut set = current.clone();
        let mut r3 = Report3::decode(&set.report3).unwrap();
        // XOR with a non-zero key: never the state of the device, and distinct per key.
        r3.set_bytes(200, &[rig.original[200] ^ i]);
        set.report3 = r3.encode();
        set
    };
    backups
        .save(&other(0x40), firmware, Reason::BeforeWrite, 1)
        .unwrap();
    let same = backups
        .save(&current, firmware, Reason::BeforeWrite, 2)
        .unwrap()
        .id;
    for i in 1..=19u8 {
        assert!(
            backups
                .save(&other(i), firmware, Reason::BeforeWrite, 2 + u64::from(i))
                .unwrap()
                .written
        );
    }
    let pairs = &group_pairs(&rig)[..2];
    let (result, out) = rig.run_group(&rig.group_options(pairs)).await;
    assert!(out.contains(&format!("backup: {same}")), "{out}");
    assert_eq!(
        rig.sim.report3(),
        rig.original,
        "device not restored: {result:?}\n{out}"
    );
}

/// A field of the original report holds a value outside its registry range. The restore writes
/// the original bytes back as they are.
#[tokio::test]
async fn restore_of_an_out_of_range_original() {
    let mut rig = Rig::new();
    // fill_level_calibration: i16 at 4, scale 0.01, range -20..20 %.
    rig.sim.change_report3(4, &3000i16.to_be_bytes());
    rig.original = rig.sim.report3();
    let (result, out) = rig.run(&rig.options("pump_max_speed", 4801)).await;
    assert!(
        rig.report3_writes() == 0 || rig.sim.report3() == rig.original,
        "wrote, then left the device unrestored: {result:?}\n{out}"
    );
}

/// The state dir stops taking new backups between two writes (disk full). The restore takes no
/// backup, so it still writes the device back.
#[tokio::test]
async fn restore_without_a_writable_state_dir() {
    use std::os::unix::fs::PermissionsExt;
    let rig = Rig::new();
    let pairs = group_pairs(&rig);
    let leak_dir = rig.state.0.join("backups");
    let mut out = Tripwire::new("save-settings: waiting", {
        let leak_dir = leak_dir.clone();
        move || {
            for entry in std::fs::read_dir(&leak_dir).unwrap() {
                let p = entry.unwrap().path();
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o555)).unwrap();
            }
        }
    });
    let result = rig.run_into(&rig.group_options(&pairs), &mut out).await;
    for entry in std::fs::read_dir(&leak_dir).unwrap() {
        let p = entry.unwrap().path();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    assert_eq!(
        rig.sim.report3(),
        rig.original,
        "device not restored: {result:?}\n{}",
        out.text()
    );
}

/// No two writable Safety fields share a byte: the group check attributes each byte to one field.
#[test]
fn safety_fields_do_not_overlap() {
    let registry = Registry::leakshield();
    let mut owner: std::collections::BTreeMap<usize, &str> = Default::default();
    let mut clashes = Vec::new();
    for field in registry
        .field
        .iter()
        .filter(|f| f.risk_class == RiskClass::Safety)
    {
        let size = field.value_type.scalar.size();
        for offset in field.element_offsets() {
            for byte in offset..offset + size {
                if let Some(prev) = owner.insert(byte, &field.id)
                    && prev != field.id
                {
                    clashes.push((byte, prev.to_owned(), field.id.clone()));
                }
            }
        }
    }
    assert!(clashes.is_empty(), "{clashes:?}");
}

/// A device that does not take the restore gives no result: no registry change, a loud error.
#[tokio::test]
async fn a_failed_restore_aborts_without_a_registry_change() {
    let rig = Rig::new();
    let sim = rig.sim.clone();
    let mut out = Tripwire::new("restore: the report 3", move || {
        sim.fail_feature_writes(true)
    });
    let result = rig
        .run_into(&rig.options("pump_max_speed", 4801), &mut out)
        .await;
    rig.sim.fail_feature_writes(false);
    assert!(
        matches!(
            result,
            Err(Error::Aborted {
                restored: false,
                ..
            })
        ),
        "{result:?}"
    );
    assert!(!out.text().contains("registry change in"), "{}", out.text());
}
