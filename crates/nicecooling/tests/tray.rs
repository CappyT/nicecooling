mod support;

use nicecooling::command::Command;
use nicecooling::i18n::{EN, IT};
use nicecooling::model::{Cause, Link, Response};
use nicecooling::tray::{Tray, TrayEntry, TrayIcon, TrayView};
use support::bus::PrivateBus;
use support::fake_tray_host::FakeTrayHost;
use support::fixtures::{LEAKSHIELD, NOW_US, alarm, online};
use support::{SESSION_LOCK, use_as_session_bus};
use tokio::sync::mpsc::unbounded_channel;

#[test]
fn tray_view_follows_the_state() {
    let mut state = online();
    let view = TrayView::build(&state, NOW_US, EN);
    assert_eq!(view.icon, TrayIcon::Normal);
    assert!(!view.needs_attention);
    assert_eq!(view.tooltip, "nicecooling: No active Alarms");
    assert_eq!(
        view.entries[0],
        TrayEntry::Info {
            label: "Main loop: 31.4 °C, 115 L/h, 298 mbar".into()
        }
    );

    state.snapshot.alarms = vec![alarm(
        "w1",
        "lost-input",
        "nct6798_pwm6",
        Response::Notify,
        0,
    )];
    let view = TrayView::build(&state, NOW_US, EN);
    assert_eq!(view.icon, TrayIcon::Warning);
    assert!(!view.needs_attention);

    state.snapshot.alarms = vec![alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        Response::Poweroff,
        21,
    )];
    let view = TrayView::build(&state, NOW_US, EN);
    assert_eq!(view.icon, TrayIcon::Alarm);
    assert!(view.needs_attention, "the status is NeedsAttention");
    assert_eq!(view.tooltip, "nicecooling: 1 Alarm, poweroff in 0:21");
    assert_eq!(
        view.entries[0],
        TrayEntry::CancelPoweroff {
            label: "Cancel poweroff (0:21)".into(),
            instance: "a1".into()
        }
    );
    assert_eq!(
        TrayView::build(&state, NOW_US, IT).entries[0],
        TrayEntry::CancelPoweroff {
            label: "Annulla lo spegnimento (0:21)".into(),
            instance: "a1".into()
        }
    );

    // A Loop without a Leakshield has no pressure. A missing value is "--".
    state.snapshot.loops[0].leakshield = None;
    state.sensors = None;
    let view = TrayView::build(&state, NOW_US, EN);
    assert!(view.entries.contains(&TrayEntry::Info {
        label: "Main loop: --, --".into()
    }));

    state.link = Link::Offline;
    let view = TrayView::build(&state, NOW_US, EN);
    assert_eq!(view.icon, TrayIcon::Offline);
    assert_eq!(
        view.entries[0],
        TrayEntry::Info {
            label: "Daemon offline".into()
        }
    );
    assert!(
        matches!(&view.entries[1], TrayEntry::Profiles { enabled: false, items, .. } if items.is_empty())
    );

    state.link = Link::Down;
    let view = TrayView::build(&state, NOW_US, EN);
    assert_eq!(view.icon, TrayIcon::Alarm);
    assert!(view.needs_attention);
    assert_eq!(
        view.entries[0],
        TrayEntry::Info {
            label: "Daemon down: nicecooling protection is off".into()
        }
    );
    assert_eq!(view.tooltip, "nicecooling: daemon down, protection is off");
}

#[tokio::test(flavor = "multi_thread")]
async fn tray_menu_has_cancel_poweroff() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("tray");
    use_as_session_bus(&bus);
    let host = FakeTrayHost::start(bus.connect().await).await;

    let mut state = online();
    state.snapshot.cause = Cause::Manual;
    state.snapshot.alarms = vec![alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        Response::Poweroff,
        21,
    )];
    let (tx, mut commands) = unbounded_channel();
    let view = TrayView::build(&state, NOW_US, EN);
    let tray = tokio::task::spawn_blocking(move || Tray::spawn(tx, view).expect("tray"))
        .await
        .unwrap();

    let name = host.item_name().await;
    assert!(name.starts_with("org.kde.StatusNotifierItem-"), "{name}");
    assert_eq!(
        host.string_property(&name, "Status").await,
        "NeedsAttention"
    );
    assert_eq!(
        host.string_property(&name, "IconName").await,
        "nicecooling-alarm-symbolic"
    );
    assert_eq!(
        host.tooltip(&name).await,
        "nicecooling: 1 Alarm, poweroff in 0:21"
    );

    let menu = host.layout(&name).await;
    assert_eq!(
        menu.children[0].label, "Cancel poweroff (0:21)",
        "the countdown entry is first"
    );
    assert!(menu.children[0].enabled);
    let info = menu
        .find("Main loop: 31.4 °C, 115 L/h, 298 mbar")
        .expect("loop line");
    assert!(!info.enabled, "a Loop line is not clickable");
    host.click(&name, menu.children[0].id).await;
    assert_eq!(
        commands.recv().await,
        Some(Command::CancelPoweroff {
            instance: "a1".into()
        })
    );

    // The profile submenu marks the active Profile and offers the revoke entry.
    let profile = menu.find("Profile: Quiet").expect("profile submenu");
    assert_eq!(
        profile.labels(),
        ["Default", "Quiet", "", "Revoke manual activation"]
    );
    host.click(&name, profile.children[0].id).await;
    assert_eq!(
        commands.recv().await,
        Some(Command::ActivateProfile {
            id: "profile_default".into()
        })
    );
    host.click(&name, profile.find("Revoke manual activation").unwrap().id)
        .await;
    assert_eq!(commands.recv().await, Some(Command::ReturnToAutomatic));

    host.click(&name, menu.find("Open nicecooling").unwrap().id)
        .await;
    assert_eq!(commands.recv().await, Some(Command::OpenWindow(None)));
    host.activate(&name).await;
    assert_eq!(commands.recv().await, Some(Command::OpenWindow(None)));
    host.click(&name, menu.find("Quit").unwrap().id).await;
    assert_eq!(commands.recv().await, Some(Command::Quit));

    // The poweroff is cancelled: the entry goes away and the icon returns to normal.
    state.snapshot.alarms.clear();
    let view = TrayView::build(&state, NOW_US, EN);
    let tray = tokio::task::spawn_blocking(move || {
        tray.update(view);
        tray
    })
    .await
    .unwrap();
    let menu = host.layout(&name).await;
    assert!(
        menu.children
            .iter()
            .all(|c| !c.label.starts_with("Cancel poweroff"))
    );
    assert_eq!(host.string_property(&name, "Status").await, "Active");
    tokio::task::spawn_blocking(move || tray.shutdown())
        .await
        .unwrap();
}

/// At login the app starts before the tray host of the desktop: the item waits and registers
/// when the host comes.
#[tokio::test(flavor = "multi_thread")]
async fn the_item_waits_for_a_host_that_starts_later() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("tray-late-host");
    use_as_session_bus(&bus);
    let (tx, _commands) = unbounded_channel();
    let view = TrayView::build(&online(), NOW_US, EN);
    let tray = tokio::task::spawn_blocking(move || Tray::spawn(tx, view).expect("tray"))
        .await
        .unwrap();
    assert!(
        !tray.host_online(),
        "no StatusNotifierWatcher on the bus yet"
    );

    let host = FakeTrayHost::start(bus.connect().await).await;
    let name = host.item_name().await;
    assert!(name.starts_with("org.kde.StatusNotifierItem-"), "{name}");
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(5);
    while !tray.host_online() && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(tray.host_online());
    tokio::task::spawn_blocking(move || tray.shutdown())
        .await
        .unwrap();
}

/// At login the tray host comes some seconds after the app: the window stays closed while the
/// app waits, and it opens once when no host came.
#[test]
fn the_window_opens_only_when_no_tray_host_came() {
    use nicecooling::services::{TRAY_HOST_GRACE, opens_window_without_host};
    use std::time::Duration;
    let before = TRAY_HOST_GRACE - Duration::from_secs(1);
    assert!(!opens_window_without_host(false, before, false));
    assert!(opens_window_without_host(false, TRAY_HOST_GRACE, false));
    assert!(
        !opens_window_without_host(false, TRAY_HOST_GRACE, true),
        "once only"
    );
    assert!(!opens_window_without_host(true, TRAY_HOST_GRACE, false));
}
