//! The factory reset and the two counter resets.

use nicecooling_api::Error;
use nicecooling_api::authz_map::Action;
use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report2::Report2;
use nicecoolingd::leakshield::backup::BackupSet;

use crate::support::{LEAKSHIELD, Rig, path};
use crate::{LEAKSHIELD_ID, Rig as DeviceRig};

const FACTORY_DEFAULTS: u32 = 3;
const RESET_REFILL_HISTORY: u32 = 13;
const RESET_SERVICE_COUNTER: u32 = 25000;
/// A byte of report 3 that no test field covers, which the factory reset must bring back.
const CHANGED_OFFSET: usize = 300;

fn running_as_root() -> bool {
    std::fs::read_to_string("/proc/self/status")
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix("Uid:"))
        .is_some_and(|l| l.split_whitespace().next() == Some("0"))
}

fn without_commands(ids: &[&str]) -> Registry {
    let mut registry = Registry::leakshield();
    registry.command.retain(|c| !ids.contains(&c.id.as_str()));
    registry
}

#[tokio::test]
async fn backup_before_reset() {
    let rig = DeviceRig::new("reset-backup");
    let device = rig.attached();
    rig.sim.change_report3(CHANGED_OFFSET, &[0x55]);
    assert!(device.list_backups().unwrap().is_empty());

    device.factory_reset().await.unwrap();

    // The backup holds the state from before the reset.
    let backups = device.list_backups().unwrap();
    assert_eq!(backups.len(), 1);
    let BackupSet { report3, .. } = device.get_backup(&backups[0].id).unwrap();
    assert_eq!(report3[CHANGED_OFFSET], 0x55);
}

#[tokio::test]
async fn reset_sends_factory_defaults_command() {
    let rig = DeviceRig::new("reset-command");
    let device = rig.attached();

    device.factory_reset().await.unwrap();

    assert_eq!(rig.sim.commands(), [Report2::new(FACTORY_DEFAULTS, 0)]);
}

#[tokio::test]
async fn read_back_after_reset() {
    let rig = DeviceRig::new("reset-read-back");
    let device = rig.attached();
    let defaults = rig.sim.report3();
    rig.sim.change_report3(CHANGED_OFFSET, &[0x55]);
    assert_ne!(rig.sim.report3(), defaults);

    device.factory_reset().await.unwrap();

    assert_eq!(rig.sim.report3(), defaults);
    // The daemon holds the settings that it read back, so that it does not report an external
    // change.
    let last_known = rig
        .dir
        .join("leakshield")
        .join(LEAKSHIELD_ID)
        .join("last-known-report3.bin");
    assert_eq!(std::fs::read(last_known).unwrap(), defaults);
    assert!(device.get_fields().is_ok());
}

#[tokio::test]
async fn reset_refuses_a_read_only_device() {
    let rig = DeviceRig::new("reset-read-only");
    let device = rig.attached();
    rig.sim.edit_report1(|r| r.firmware_version = 1019);

    let error = device.factory_reset().await.unwrap_err();

    assert!(matches!(error, Error::ReadOnly(reason) if reason == "unknown-firmware"));
    assert!(rig.sim.commands().is_empty());
}

#[tokio::test]
async fn reset_refill_history_sends_command_13() {
    let rig = DeviceRig::new("reset-refill");
    let device = rig.attached();

    device.reset_refill_history().unwrap();

    assert_eq!(rig.sim.commands(), [Report2::new(RESET_REFILL_HISTORY, 0)]);
}

#[tokio::test]
async fn reset_service_counter_sends_25000_with_serial_xor() {
    let rig = DeviceRig::new("reset-service");
    let device = rig.attached();

    device.reset_service_counter().unwrap();

    // The serial 20417-31072 is the u32 of report 1 bytes 3-6.
    let serial = (20417u32 << 16) | 31072;
    assert_eq!(
        rig.sim.commands(),
        [Report2::new(RESET_SERVICE_COUNTER, serial ^ 0xFFFF_FFFF)]
    );
}

#[tokio::test]
async fn reset_not_supported_when_registry_lacks_command() {
    let rig = DeviceRig::with_registry(
        "reset-no-command",
        without_commands(&[
            "reset_refill_history",
            "reset_service_counter",
            "factory_defaults",
        ]),
    );
    let device = rig.attached();
    let not_supported = |error: Error| matches!(error, Error::ZBus(zbus::Error::FDO(e)) if matches!(*e, zbus::fdo::Error::NotSupported(_)));

    assert!(not_supported(device.reset_refill_history().unwrap_err()));
    assert!(not_supported(device.reset_service_counter().unwrap_err()));
    assert!(not_supported(device.factory_reset().await.unwrap_err()));

    // Nothing went to the device, not even a backup.
    assert!(rig.sim.commands().is_empty());
    assert!(device.list_backups().unwrap().is_empty());
}

async fn proxy(rig: &Rig) -> LeakshieldProxy<'static> {
    LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reset_commands_ask_write() {
    if running_as_root() {
        return;
    }
    let rig = Rig::start().await;
    let proxy = proxy(&rig).await;

    proxy.reset_refill_history().await.unwrap();
    proxy.reset_service_counter().await.unwrap();
    assert_eq!(
        rig.authorizer.calls(),
        [(Action::Write, false), (Action::Write, false)]
    );
    let codes: Vec<u32> = rig.sim.commands().iter().map(|c| c.command).collect();
    assert_eq!(codes, [RESET_REFILL_HISTORY, RESET_SERVICE_COUNTER]);

    // The factory reset asks `critical`.
    rig.authorizer.clear();
    proxy.factory_reset().await.unwrap();
    assert_eq!(rig.authorizer.calls(), [(Action::Critical, false)]);
    assert_eq!(rig.sim.commands().last().unwrap().command, FACTORY_DEFAULTS);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_pump_runtime_method() {
    let rig = Rig::start().await;
    let introspect = zbus::fdo::IntrospectableProxy::builder(&rig.client)
        .destination("io.github.cappyt.NiceCooling1")
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .build()
        .await
        .unwrap();
    let xml = introspect.introspect().await.unwrap();

    let document = roxmltree::Document::parse_with_options(
        &xml,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .unwrap();
    let interface = document
        .descendants()
        .find(|n| n.attribute("name") == Some("io.github.cappyt.NiceCooling1.Leakshield"))
        .expect("the Leakshield interface");
    let methods: Vec<&str> = interface
        .children()
        .filter(|n| n.has_tag_name("method"))
        .filter_map(|n| n.attribute("name"))
        .collect();
    assert!(methods.contains(&"ResetRefillHistory"));
    for method in methods {
        let lower = method.to_ascii_lowercase();
        assert!(
            !lower.contains("runtime") && !lower.contains("pump"),
            "{method} would clear the pump runtime"
        );
    }
}
