mod support;

use nicecooling::i18n::{EN, IT};
use nicecooling::model::{AlarmEntry, AppState, Cause, Link, ProfileInfo, Response, Severity};
use nicecooling::shell::{BannerAction, BannerKind, BannerStyle, NavBadge, Page, ShellModel};
use support::bus::PrivateBus;
use support::fake_daemon::{FakeDaemon, LEAKSHIELD, Options};
use support::fake_systemd::FakeSystemd;
use support::harness::Harness;
use support::labels;

fn online_state() -> AppState {
    let mut state = AppState {
        link: Link::Online,
        ..Default::default()
    };
    state.snapshot.profiles = vec![ProfileInfo {
        id: "profile_2".into(),
        name: "Quiet".into(),
    }];
    state.snapshot.active_profile = state.snapshot.profiles.first().cloned();
    state
}

fn entry(instance: &str, event: &str, severity: Severity, poweroff_at_us: u64) -> AlarmEntry {
    AlarmEntry {
        instance: instance.into(),
        event: event.into(),
        severity,
        source: LEAKSHIELD.into(),
        response: if poweroff_at_us == 0 {
            Response::Notify
        } else {
            Response::Poweroff
        },
        raised_us: 1,
        poweroff_at_us,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn daemon_offline_banner_and_write_controls_disabled() {
    let bus = PrivateBus::start("shell-offline");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the daemon is online", |state| state.is_online())
        .await;

    let online = ShellModel::build(&harness.state, 0, EN);
    assert!(online.writes.enabled);
    assert!(online.banners.is_empty());
    assert_eq!(online.status.daemon, "Daemon connected");

    daemon.stop().await;
    harness
        .until("the daemon is offline", |state| state.link == Link::Offline)
        .await;

    for (locale, text, tooltip) in [
        (
            EN,
            "The daemon is offline. The values are not live, and you cannot change settings.",
            "The daemon is offline.",
        ),
        (
            IT,
            "Il demone è offline. I valori non sono in tempo reale e non puoi cambiare le impostazioni.",
            "Il demone è offline.",
        ),
    ] {
        let model = ShellModel::build(&harness.state, 0, locale);
        let banner = &model.banners[0];
        assert_eq!(banner.kind, BannerKind::DaemonOffline);
        assert_eq!(banner.style, BannerStyle::Offline);
        assert_eq!(banner.title, text);
        assert_eq!(
            banner.button.as_ref().map(|b| &b.1),
            Some(&BannerAction::TryAgain)
        );
        assert!(!model.writes.enabled, "write controls are disabled");
        assert_eq!(model.writes.tooltip.as_deref(), Some(tooltip));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn start_limit_hit_shows_daemon_down_board_on_bios_curves() {
    let bus = PrivateBus::start("shell-down");
    let _systemd = FakeSystemd::start(bus.connect().await, "failed", "start-limit-hit").await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the daemon is down", |state| state.link == Link::Down)
        .await;

    let model = ShellModel::build(&harness.state, 0, EN);
    let banner = &model.banners[0];
    assert_eq!(banner.kind, BannerKind::DaemonDown);
    assert_eq!(banner.style, BannerStyle::Alarm, "the solid Alarm field");
    assert_eq!(
        banner.title,
        "The daemon is down. nicecooling protection is off."
    );
    assert_eq!(
        banner.detail.as_deref(),
        Some(
            "No Curve, no Feed, no alarm response. The Board runs its Hardware curves from the BIOS snapshot."
        )
    );
    assert_eq!(
        banner.button.as_ref().map(|b| &b.1),
        Some(&BannerAction::ShowHowToStart)
    );
    assert_eq!(model.status.daemon, "Daemon down");
    assert!(!model.writes.enabled);
    assert!(
        model
            .banners
            .iter()
            .all(|b| b.kind != BannerKind::DaemonOffline)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_unit_without_start_limit_is_only_offline() {
    let bus = PrivateBus::start("shell-failed");
    let systemd = FakeSystemd::start(bus.connect().await, "failed", "exit-code").await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("offline", |state| state.link == Link::Offline)
        .await;
    systemd.set_state("failed", "start-limit-hit").await;
    harness
        .until("down", |state| state.link == Link::Down)
        .await;
}

#[test]
fn poweroff_banner_leads_and_banners_collapse_after_three() {
    let mut state = online_state();
    state.snapshot.loops = vec![nicecooling::model::LoopInfo {
        id: "loop_1".into(),
        name: "Main loop".into(),
        leakshield: Some(LEAKSHIELD.into()),
        ..Default::default()
    }];
    state.snapshot.leakshields = vec![nicecooling::model::LeakshieldInfo {
        id: LEAKSHIELD.into(),
        serial: "20417-31072".into(),
        firmware: 1042,
        connected: true,
        read_only_reason: "unknown-firmware".into(),
    }];
    state.snapshot.immediate_switching = true;
    state.snapshot.alarms = vec![entry(
        "a1",
        "leakshield.leak-alarm",
        Severity::Alarm,
        21_000_000,
    )];

    let model = ShellModel::build(&state, 0, EN);
    let kinds: Vec<_> = model.banners.iter().map(|b| b.kind).collect();
    assert_eq!(
        kinds,
        [
            BannerKind::Poweroff,
            BannerKind::UnknownFirmware,
            BannerKind::ImmediateSwitching
        ],
        "three banners show as they are"
    );
    let poweroff = &model.banners[0];
    assert_eq!(poweroff.title, "Large leak Alarm on Leakshield");
    assert_eq!(
        poweroff.detail.as_deref(),
        Some("Main loop. The PC powers off when the countdown ends.")
    );
    assert_eq!(poweroff.countdown.as_deref(), Some("0:21"));
    assert_eq!(poweroff.style, BannerStyle::Alarm);
    assert_eq!(
        poweroff.button.as_ref().map(|b| (&b.0[..], &b.1)),
        Some((
            "Cancel poweroff",
            &BannerAction::CancelPoweroff {
                instance: "a1".into()
            }
        ))
    );
    assert_eq!(
        model.banners[1].title,
        "Leakshield firmware 1042 is unknown. Every field is read-only."
    );

    // The daemon offline adds a fourth notice: the last two collapse.
    state.link = Link::Offline;
    let model = ShellModel::build(&state, 0, EN);
    assert_eq!(model.banners.len(), 3);
    assert_eq!(model.banners[2].kind, BannerKind::MoreNotices);
    assert_eq!(model.banners[1].kind, BannerKind::DaemonOffline);
    assert_eq!(model.banners[2].title, "2 more notices");
    assert_eq!(model.more_notices.len(), 2);
}

#[test]
fn status_bar_and_sidebar_follow_the_state() {
    let mut state = online_state();
    state.snapshot.cause = Cause::Rule;
    state.snapshot.leakshields = vec![
        nicecooling::model::LeakshieldInfo {
            id: "leakshield_1_11111".into(),
            serial: "1-11111".into(),
            connected: true,
            ..Default::default()
        },
        nicecooling::model::LeakshieldInfo {
            id: "leakshield_2_22222".into(),
            serial: "2-22222".into(),
            connected: false,
            ..Default::default()
        },
    ];
    let model = ShellModel::build(&state, 0, EN);
    assert_eq!(
        model.status.profile.as_deref(),
        Some("Profile: Quiet (rule)")
    );
    assert_eq!(model.status.alarms, "No active Alarms");
    let devices = &model.groups[1];
    assert_eq!(devices.title.as_deref(), Some("Devices"));
    assert_eq!(
        devices.entries[0].label, "Leakshield 11111",
        "two devices show the last 5 digits"
    );
    assert_eq!(devices.entries[1].badge, Some(NavBadge::Offline));

    state.snapshot.cause = Cause::Manual;
    state.snapshot.alarms = vec![
        entry("a1", "pump-stop", Severity::Alarm, 21_000_000),
        entry("w1", "lost-input", Severity::Warning, 0),
    ];
    let model = ShellModel::build(&state, 0, EN);
    assert_eq!(
        model.status.profile.as_deref(),
        Some("Profile: Quiet (manual)")
    );
    assert_eq!(model.status.alarms, "1 Alarm, 1 Warning, poweroff in 0:21");
    assert_eq!(
        model.groups.last().unwrap().entries[1].badge,
        Some(NavBadge::Count(2))
    );
    let italian = ShellModel::build(&state, 0, IT);
    assert_eq!(
        italian.status.alarms,
        "1 Allarme, 1 Avviso, spegnimento tra 0:21"
    );
    assert_eq!(model.pages()[0], Page::Overview);
    assert_eq!(*model.pages().last().unwrap(), Page::Settings);
}

#[test]
fn app_shell_labels() {
    let en = labels::load("en");
    let it = labels::load("it");
    // The copy table of docs/ui/screens/app-shell.md.
    let table = [
        ("nav.overview", "Overview", "Panoramica"),
        ("nav.loops", "Loops", "Loop"),
        ("nav.channels", "Channels", "Canali"),
        ("nav.sensors", "Sensors", "Sensori"),
        ("nav.devices", "Devices", "Dispositivi"),
        ("nav.profiles", "Profiles", "Profili"),
        ("nav.alarms", "Alarms", "Allarmi"),
        ("nav.history", "History", "Cronologia"),
        ("nav.settings", "Settings", "Impostazioni"),
        (
            "status.daemon_connected",
            "Daemon connected",
            "Demone connesso",
        ),
        (
            "status.no_alarms",
            "No active Alarms",
            "Nessun Allarme attivo",
        ),
        (
            "banner.poweroff.button",
            "Cancel poweroff",
            "Annulla lo spegnimento",
        ),
        (
            "tooltip.needs_admin",
            "Needs the administrator password",
            "Richiede la password di amministratore",
        ),
    ];
    for (key, english, italian) in table {
        assert_eq!(en[key], english, "{key}");
        assert_eq!(it[key], italian, "{key}");
    }
    // The other labels of the shell exist in both languages.
    for key in [
        "status.daemon_offline",
        "status.daemon_down",
        "status.connecting",
        "banner.offline.button",
        "banner.down.button",
        "banner.down.detail",
        "dialog.daemon_down.title",
        "menu.shortcuts",
        "menu.about",
        "menu.quit",
        "nav.more_notices",
    ] {
        assert!(en.contains_key(key) && it.contains_key(key), "{key}");
    }
    // The sidebar shows the same words as the table, in both languages.
    let model = ShellModel::build(&online_state(), 0, IT);
    let names: Vec<_> = model.groups[0]
        .entries
        .iter()
        .map(|e| e.label.as_str())
        .collect();
    assert_eq!(names, ["Panoramica", "Loop", "Canali", "Sensori"]);
}
