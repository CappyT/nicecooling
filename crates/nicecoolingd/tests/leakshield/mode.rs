use std::sync::Arc;

use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report2::Report2;
use nicecoolingd::leakshield::attached::now_unix;
use nicecoolingd::leakshield::mode::Mode;
use nicecoolingd::state::{SlotStore, StateStore};

use crate::Rig;

fn slots(rig: &Rig) -> SlotStore {
    SlotStore::new(Arc::new(StateStore::new(&rig.dir)))
}

/// `set_mode`: report 2 command 12.
const SET_MODE: u32 = 12;
const NOT_SUPPORTED: &str = "org.freedesktop.DBus.Error.NotSupported";

/// The fixture was dumped while an Action ran. A test starts with the Mode SHIELD instead.
fn quiet(rig: &Rig) {
    rig.sim.edit_report1(|r| r.mode = 1);
}

fn never() -> bool {
    false
}

#[tokio::test]
async fn each_mode() {
    let rig = Rig::new("mode-each");
    quiet(&rig);
    let device = rig.attached();

    for mode in Mode::ALL {
        device.set_mode(mode.as_str(), &slots(&rig)).unwrap();
        assert_eq!(
            rig.sim.commands().pop(),
            Some(Report2::new(SET_MODE, mode.code()))
        );
        assert_eq!(device.info().mode, mode.as_str());
        assert_eq!(device.info().running_action, "");
    }
    // Only the lowercase names are Modes.
    for text in ["Shield", "SHIELD", "", "test"] {
        let error = device.set_mode(text, &slots(&rig)).unwrap_err();
        assert_eq!(
            error.dbus_name(),
            "io.github.cappyt.NiceCooling1.Error.InvalidArgument"
        );
    }
    assert_eq!(rig.sim.commands().len(), 3);
}

#[tokio::test]
async fn each_action() {
    let rig = Rig::new("action-each");
    quiet(&rig);
    let device = rig.attached();

    for (action, code) in [
        ("measure-fill-level", 4),
        ("deaerate", 5),
        ("test", 6),
        ("refill", 7),
    ] {
        device.start_action(action, never).unwrap();
        assert_eq!(
            rig.sim.commands().pop(),
            Some(Report2::new(SET_MODE, code)),
            "{action}"
        );
        assert_eq!(device.info().running_action, action);
        rig.sim.finish_action();
    }
    let error = device.start_action("Test", never).unwrap_err();
    assert_eq!(
        error.dbus_name(),
        "io.github.cappyt.NiceCooling1.Error.InvalidArgument"
    );
    assert_eq!(rig.sim.commands().len(), 4);
}

#[tokio::test]
async fn return_to_mode_after_action() {
    let rig = Rig::new("action-return");
    quiet(&rig);
    let device = rig.attached();
    device.set_mode("monitor", &slots(&rig)).unwrap();

    device.start_action("test", never).unwrap();
    // The Mode stays known while the Action runs.
    assert_eq!(
        (device.info().mode, device.info().running_action),
        ("monitor".to_owned(), "test".to_owned())
    );

    rig.sim.finish_action();
    device.poll(now_unix()).unwrap();
    assert_eq!(
        (device.info().mode, device.info().running_action),
        ("monitor".to_owned(), String::new())
    );
}

#[tokio::test]
async fn running_action_property() {
    let rig = Rig::new("action-running");
    // The fixture was dumped during a deaeration.
    let device = rig.attached();
    assert_eq!(device.info().running_action, "deaerate");
    assert_eq!(device.info().mode, "");

    device.set_mode("shield", &slots(&rig)).unwrap();
    device.cancel_action().unwrap();
    assert_eq!(device.info().running_action, "");
    assert_eq!(device.info().mode, "shield");
}

#[tokio::test]
async fn cancel_action_sends_command_12_code_0() {
    let rig = Rig::new("cancel");
    quiet(&rig);
    let device = rig.attached();
    device.start_action("deaerate", never).unwrap();

    device.cancel_action().unwrap();

    assert_eq!(rig.sim.commands().pop(), Some(Report2::new(12, 0)));
    assert_eq!(device.info().running_action, "");
    assert_eq!(device.info().mode, "shield");
}

#[tokio::test]
async fn cancel_action_not_supported_when_registry_lacks_command() {
    let mut registry = Registry::leakshield();
    registry.command.retain(|c| c.id != "cancel_action");
    let rig = Rig::with_registry("cancel-missing", registry);
    quiet(&rig);
    let device = rig.attached();
    device.start_action("deaerate", never).unwrap();

    let error = device.cancel_action().unwrap_err();

    assert_eq!(error.dbus_name(), NOT_SUPPORTED);
    // The Action continues.
    assert_eq!(rig.sim.commands().len(), 1);
    assert_eq!(device.info().running_action, "deaerate");
}

#[tokio::test]
async fn unknown_firmware_refuses_mode_and_action() {
    let rig = Rig::new("mode-read-only");
    rig.sim.edit_report1(|r| r.firmware_version = 1019);
    let device = rig.attached();

    for error in [
        device.set_mode("monitor", &slots(&rig)).unwrap_err(),
        device.start_action("test", never).unwrap_err(),
        device.cancel_action().unwrap_err(),
        device.sync_clock().unwrap_err(),
    ] {
        assert_eq!(
            error.dbus_name(),
            "io.github.cappyt.NiceCooling1.Error.ReadOnly"
        );
    }
    assert!(rig.sim.output_reports().is_empty());
}
