mod support;

use std::time::Duration;

use nicecooling::command::Command;
use nicecooling::i18n::{EN, IT, Locale};
use nicecooling::model::{AppState, Cause, DaemonEvent, Link, Response};
use nicecooling::notify::{Level, Notifier, NotifyPrefs, Plan, plan};
use nicecooling::shell::{BannerAction, ShellModel};
use nicecooling::tray::{TrayEntry, TrayView};
use support::bus::PrivateBus;
use support::fake_daemon::{FakeDaemon, Options};
use support::fake_notification_server::FakeNotificationServer;
use support::fixtures::{LEAKSHIELD, NOW_US, alarm, online};
use support::harness::Harness;
use support::{SESSION_LOCK, use_as_session_bus};
use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};

fn raised(entry: nicecooling::model::AlarmEntry, state: &mut AppState) -> DaemonEvent {
    let event = DaemonEvent::AlarmRaised(entry);
    state.apply(&event);
    event
}

fn single(plans: Vec<Plan>) -> nicecooling::notify::NotificationSpec {
    match plans.into_iter().next() {
        Some(Plan::Show(spec)) => spec,
        other => panic!("expected one notification, got {other:?}"),
    }
}

/// Show a notification from a plain thread: `notify-rust` runs its own executor.
async fn show(notifier: Notifier, spec: nicecooling::notify::NotificationSpec) -> Notifier {
    tokio::task::spawn_blocking(move || {
        let mut notifier = notifier;
        notifier.show(&spec).expect("show");
        notifier
    })
    .await
    .unwrap()
}

#[test]
fn notification_text_follows_the_table() {
    let mut state = online();
    let prefs = NotifyPrefs::default();
    let cases = [
        // (event, source, response, in_s, title, body, buttons)
        (
            "leakshield.leak-alarm",
            LEAKSHIELD,
            Response::Poweroff,
            30,
            "Large leak Alarm",
            "Leakshield, Main loop. The PC powers off in 30 s.",
            vec!["Cancel poweroff", "Open nicecooling"],
        ),
        (
            "pump-stop",
            "loop_1",
            Response::Poweroff,
            30,
            "Pump stopped",
            "Main loop: a Pump channel reads 0 rpm. Every Channel is at 100 %. The PC powers off in 30 s.",
            vec!["Cancel poweroff", "Open nicecooling"],
        ),
        (
            "leakshield.fill-level-low-alarm",
            LEAKSHIELD,
            Response::Notify,
            0,
            "Fill level Alarm",
            "Leakshield, Main loop.",
            vec!["Open nicecooling"],
        ),
        (
            "leakshield-unreachable",
            LEAKSHIELD,
            Response::Notify,
            0,
            "Leakshield unreachable",
            "Leakshield, Main loop: no data from the Leakshield for 10 s. Check its USB cable.",
            vec!["Open nicecooling"],
        ),
        (
            "leakshield.leak-warning",
            LEAKSHIELD,
            Response::Notify,
            0,
            "Leak Warning",
            "Leakshield, Main loop.",
            vec!["Open nicecooling"],
        ),
        (
            "lost-input",
            "nct6798_pwm6",
            Response::Notify,
            0,
            "Input lost",
            "nct6798_pwm6 runs at 100 % until its Input returns.",
            vec!["Open Channels"],
        ),
    ];
    for (event, source, response, in_s, title, body, buttons) in cases {
        let entry = alarm("i1", event, source, response, in_s);
        let event = raised(entry, &mut state);
        let spec = single(plan(&event, &state, NOW_US, EN, prefs));
        assert_eq!(spec.title, title);
        assert_eq!(spec.body, body, "{title}");
        assert_eq!(
            spec.buttons
                .iter()
                .map(|b| b.label.as_str())
                .collect::<Vec<_>>(),
            buttons,
            "{title}"
        );
    }

    let down = single(plan(
        &DaemonEvent::Link(Link::Down),
        &state,
        NOW_US,
        EN,
        prefs,
    ));
    assert_eq!(down.title, "nicecooling protection is off");
    assert!(
        down.body
            .starts_with("The daemon is down. The Board runs its Hardware curves.")
    );
    assert_eq!(down.level, Level::Critical);

    let cancelled = single(plan(
        &DaemonEvent::PoweroffCancelled {
            instance: "i1".into(),
        },
        &state,
        NOW_US,
        EN,
        prefs,
    ));
    assert_eq!(
        (cancelled.title.as_str(), cancelled.body.as_str()),
        ("Poweroff cancelled", "The Alarm is still active.")
    );
    assert_eq!(cancelled.key, "i1", "it replaces the Alarm notification");

    let external = single(plan(
        &DaemonEvent::ExternalChange {
            leakshield: LEAKSHIELD.into(),
            fields: vec!["display_brightness".into()],
        },
        &state,
        NOW_US,
        EN,
        prefs,
    ));
    assert_eq!(
        external.title,
        "Leakshield setting changed outside nicecooling"
    );
    assert_eq!(external.buttons[0].label, "Open Leakshield");
    assert_eq!(external.level, Level::Normal);

    let switched = DaemonEvent::ProfileActivated {
        profile: "profile_2".into(),
        cause: Cause::Rule,
        state: "applied".into(),
    };
    assert!(
        plan(&switched, &state, NOW_US, EN, prefs).is_empty(),
        "off by default"
    );
    let on = NotifyPrefs {
        profile_switched_by_rule: true,
        ..prefs
    };
    let spec = single(plan(&switched, &state, NOW_US, EN, on));
    assert_eq!(
        (spec.title.as_str(), spec.level, spec.buttons.len()),
        ("Profile: Quiet", Level::Low, 0)
    );

    let off = NotifyPrefs {
        warnings: false,
        ..prefs
    };
    let warning = DaemonEvent::AlarmRaised(alarm(
        "w2",
        "leakshield.leak-warning",
        LEAKSHIELD,
        Response::Notify,
        0,
    ));
    assert!(plan(&warning, &state, NOW_US, EN, off).is_empty());
    let none = DaemonEvent::AlarmRaised(alarm("a9", "pump-stop", "loop_1", Response::None, 0));
    assert!(
        plan(&none, &state, NOW_US, EN, prefs).is_empty(),
        "response none shows nothing"
    );
}

#[test]
fn tray_and_notifications_labels() {
    let mut state = online();
    state.snapshot.cause = Cause::Manual;
    let entry = alarm(
        "i1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        Response::Poweroff,
        21,
    );
    let event = raised(entry, &mut state);
    let expected: [(Locale, &str, &str, &str, &str, &str); 2] = [
        (
            EN,
            "Cancel poweroff (0:21)",
            "Profile: Quiet",
            "Revoke manual activation",
            "Open nicecooling",
            "Quit",
        ),
        (
            IT,
            "Annulla lo spegnimento (0:21)",
            "Profilo: Quiet",
            "Revoca l'attivazione manuale",
            "Apri nicecooling",
            "Esci",
        ),
    ];
    for (locale, cancel, profile, revoke, open, quit) in expected {
        let view = TrayView::build(&state, NOW_US, locale);
        let labels: Vec<_> = view
            .entries
            .iter()
            .filter_map(|entry| match entry {
                TrayEntry::CancelPoweroff { label, .. }
                | TrayEntry::Open { label }
                | TrayEntry::Quit { label } => Some(label.clone()),
                TrayEntry::Profiles {
                    label,
                    revoke: Some(r),
                    ..
                } => Some(format!("{label}|{r}")),
                _ => None,
            })
            .collect();
        assert_eq!(
            labels,
            [
                cancel.to_owned(),
                format!("{profile}|{revoke}"),
                open.to_owned(),
                quit.to_owned()
            ]
        );
        // The notification buttons use the same words as the tray and the window.
        let spec = single(plan(&event, &state, NOW_US, locale, NotifyPrefs::default()));
        assert_eq!(spec.buttons[0].label, cancel.split(" (").next().unwrap());
        assert_eq!(spec.buttons[1].label, open);
    }
    let italian = single(plan(&event, &state, NOW_US, IT, NotifyPrefs::default()));
    assert_eq!(italian.title, "Allarme di perdita grande");
    assert_eq!(
        italian.body,
        "Leakshield, Main loop. Il PC si spegne tra 21 s."
    );
    let en = nicecooling_labels_en();
    for key in [
        "tray.hint.text",
        "tray.hint.link",
        "notification.action.open_channels",
    ] {
        assert!(en.contains_key(key), "{key}");
    }
    assert_eq!(
        en["tray.hint.text"],
        "The tray icon is not visible. On GNOME, turn on the AppIndicator extension."
    );
    assert_eq!(en["tray.hint.link"], "How to turn it on");
}

fn nicecooling_labels_en() -> std::collections::BTreeMap<String, String> {
    support::labels::load("en")
}

async fn wait_for_command(
    commands: &mut UnboundedReceiver<Command>,
    server: &FakeNotificationServer,
    id: u32,
    action: &str,
) -> Command {
    // `notify-rust` subscribes to the action signal in its own thread. The test sends the
    // click again until the subscription exists.
    let attempts = async {
        loop {
            server.invoke(id, action).await;
            if let Ok(Some(command)) =
                tokio::time::timeout(Duration::from_millis(200), commands.recv()).await
            {
                return command;
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(20), attempts)
        .await
        .expect("the action reached the app")
}

#[tokio::test(flavor = "multi_thread")]
async fn alarm_notification_has_critical_urgency() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("notify-urgency");
    use_as_session_bus(&bus);
    let server = FakeNotificationServer::start(bus.connect().await, &["actions", "body"]).await;
    let (tx, _commands) = unbounded_channel();
    let mut state = online();
    let prefs = NotifyPrefs::default();

    let mut notifier = Notifier::new(tx);
    for (event, source, response, in_s) in [
        ("leakshield.leak-alarm", LEAKSHIELD, Response::Poweroff, 30),
        (
            "leakshield.fill-level-low-alarm",
            LEAKSHIELD,
            Response::Notify,
            0,
        ),
        ("pump-stop", "loop_1", Response::Poweroff, 30),
        ("leakshield-unreachable", LEAKSHIELD, Response::Notify, 0),
    ] {
        let event = raised(alarm("i1", event, source, response, in_s), &mut state);
        let spec = single(plan(&event, &state, NOW_US, EN, prefs));
        assert_eq!(spec.level, Level::Critical, "{}", spec.title);
        notifier = show(notifier, spec).await;
    }
    let down = single(plan(
        &DaemonEvent::Link(Link::Down),
        &state,
        NOW_US,
        EN,
        prefs,
    ));
    notifier = show(notifier, down).await;
    let warning = raised(
        alarm(
            "w1",
            "leakshield.leak-warning",
            LEAKSHIELD,
            Response::Notify,
            0,
        ),
        &mut state,
    );
    let spec = single(plan(&warning, &state, NOW_US, EN, prefs));
    notifier = show(notifier, spec).await;
    drop(notifier);

    let received = server.received();
    assert_eq!(received.len(), 6);
    // Critical is urgency 2 and stays on screen (timeout 0). A Warning is normal (1).
    for note in &received[..5] {
        assert_eq!(note.urgency, Some(2), "{}", note.summary);
        assert_eq!(note.expire_timeout, 0, "{}", note.summary);
    }
    assert_eq!(received[5].urgency, Some(1));
    assert_eq!(received[0].summary, "Large leak Alarm");
    assert_eq!(
        received[0].actions,
        [
            "cancel_poweroff",
            "Cancel poweroff",
            "open",
            "Open nicecooling"
        ]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cancel_button_calls_cancel_poweroff() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("notify-cancel");
    use_as_session_bus(&bus);
    let server = FakeNotificationServer::start(bus.connect().await, &["actions", "body"]).await;
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness.until("online", |state| state.is_online()).await;
    daemon
        .raise_alarm(
            "a1",
            "leakshield.leak-alarm",
            "alarm",
            LEAKSHIELD,
            "poweroff",
            3_000_000_000_000,
        )
        .await;
    let event = harness
        .next_event("the Alarm", |event| {
            matches!(event, DaemonEvent::AlarmRaised(_))
        })
        .await;

    let (tx, mut commands) = unbounded_channel();
    let spec = single(plan(
        &event,
        &harness.state,
        NOW_US,
        EN,
        NotifyPrefs::default(),
    ));
    let notifier = show(Notifier::new(tx), spec).await;
    let id = server.received()[0].id;

    // A click on "Cancel poweroff" reaches the daemon without the window.
    let command = wait_for_command(&mut commands, &server, id, "cancel_poweroff").await;
    assert_eq!(
        command,
        Command::CancelPoweroff {
            instance: "a1".into()
        }
    );
    assert!(command.run_on_daemon(&harness.client).await.expect("call"));
    assert_eq!(daemon.calls(), ["CancelPoweroff(a1) interactive=true"]);

    // The notification then changes to "Poweroff cancelled": the same id, no cancel button.
    let cancelled = harness
        .next_event("cancel", |event| {
            matches!(event, DaemonEvent::PoweroffCancelled { .. })
        })
        .await;
    let spec = single(plan(
        &cancelled,
        &harness.state,
        NOW_US,
        EN,
        NotifyPrefs::default(),
    ));
    let _notifier = show(notifier, spec).await;
    let received = server.received();
    assert_eq!(received[1].replaces_id, id);
    assert_eq!(received[1].summary, "Poweroff cancelled");
    assert_eq!(received[1].actions, ["open", "Open nicecooling"]);

    // "Open nicecooling" gives a window command.
    let open = wait_for_command(&mut commands, &server, id, "open").await;
    assert_eq!(open, Command::OpenWindow(None));
    assert!(!open.run_on_daemon(&harness.client).await.expect("no call"));
}

#[tokio::test(flavor = "multi_thread")]
async fn no_actions_capability_cancel_in_tray_and_window() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("notify-no-actions");
    use_as_session_bus(&bus);
    let server = FakeNotificationServer::start(bus.connect().await, &["body"]).await;
    let (tx, _commands) = unbounded_channel();
    let mut state = online();
    let event = raised(
        alarm(
            "a1",
            "leakshield.leak-alarm",
            LEAKSHIELD,
            Response::Poweroff,
            30,
        ),
        &mut state,
    );
    let spec = single(plan(&event, &state, NOW_US, EN, NotifyPrefs::default()));
    assert_eq!(spec.buttons.len(), 2, "the plan has buttons");
    let _notifier = show(Notifier::new(tx), spec).await;

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert!(
        received[0].actions.is_empty(),
        "the server lacks `actions`: no buttons"
    );
    assert_eq!(received[0].urgency, Some(2));

    // Cancel poweroff stays in the tray menu and in the window.
    let tray = TrayView::build(&state, NOW_US, EN);
    assert!(
        matches!(&tray.entries[0], TrayEntry::CancelPoweroff { instance, .. } if instance == "a1")
    );
    let shell = ShellModel::build(&state, NOW_US, EN);
    assert_eq!(
        shell.banners[0].button.as_ref().map(|b| &b.1),
        Some(&BannerAction::CancelPoweroff {
            instance: "a1".into()
        })
    );
}

#[test]
fn a_snapshot_keeps_the_alarm_notifications_of_the_active_instances_only() {
    let mut state = online();
    let event = DaemonEvent::Snapshot(Box::new({
        let mut snapshot = state.snapshot.clone();
        snapshot.alarms = vec![alarm(
            "alarm_200_1",
            "pump-stop",
            "loop_1",
            Response::Poweroff,
            30,
        )];
        snapshot
    }));
    state.apply(&event);
    assert_eq!(
        plan(&event, &state, NOW_US, EN, NotifyPrefs::default()),
        vec![Plan::CloseStaleAlarms {
            active: vec!["alarm_200_1".into()]
        }]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_snapshot_closes_the_notification_of_an_instance_of_an_earlier_daemon() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("notify-stale");
    use_as_session_bus(&bus);
    let server = FakeNotificationServer::start(bus.connect().await, &["actions", "body"]).await;
    let (tx, _commands) = unbounded_channel();
    let mut state = online();
    let prefs = NotifyPrefs::default();

    let mut notifier = Notifier::new(tx);
    for instance in ["alarm_100_1", "alarm_100_2"] {
        let event = raised(
            alarm(instance, "pump-stop", "loop_1", Response::Poweroff, 30),
            &mut state,
        );
        notifier = show(notifier, single(plan(&event, &state, NOW_US, EN, prefs))).await;
    }
    let shown = server.received();
    assert_eq!(shown.len(), 2);
    // The daemon restarted: only the new instance is active, and the app missed the clear of
    // the other ones.
    let mut snapshot = state.snapshot.clone();
    snapshot.alarms = vec![alarm(
        "alarm_200_1",
        "pump-stop",
        "loop_1",
        Response::Poweroff,
        30,
    )];
    let event = DaemonEvent::Snapshot(Box::new(snapshot));
    state.apply(&event);
    let plans = plan(&event, &state, NOW_US, EN, prefs);
    tokio::task::spawn_blocking(move || {
        let mut notifier = notifier;
        for step in plans {
            notifier.apply(step).expect("apply");
        }
        notifier
    })
    .await
    .unwrap();
    let mut closed = server.closed();
    closed.sort_unstable();
    assert_eq!(closed, [shown[0].id, shown[1].id]);
}

fn sound_spec(key: &str, sound: Option<&str>) -> nicecooling::notify::NotificationSpec {
    nicecooling::notify::NotificationSpec {
        key: key.into(),
        title: "Hot water".into(),
        body: String::new(),
        level: Level::Normal,
        buttons: Vec::new(),
        sound: sound.map(str::to_owned),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn spec_with_sound_sends_the_hint() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("notify-sound");
    use_as_session_bus(&bus);
    let server = FakeNotificationServer::start(bus.connect().await, &["body", "sound"]).await;
    let (tx, _commands) = unbounded_channel();

    let notifier = show(
        Notifier::new(tx),
        sound_spec("graph-sound", Some("dialog-warning")),
    )
    .await;
    drop(notifier);

    // The sound is the `sound-name` hint of a notification that carries the Graph name.
    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].summary, "Hot water");
    assert_eq!(received[0].sound_name.as_deref(), Some("dialog-warning"));
}

#[tokio::test(flavor = "multi_thread")]
async fn fake_server_records_the_sound_hint() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("notify-sound-record");
    use_as_session_bus(&bus);
    let server = FakeNotificationServer::start(bus.connect().await, &["body"]).await;
    let (tx, _commands) = unbounded_channel();

    let mut notifier = Notifier::new(tx);
    notifier = show(notifier, sound_spec("plain", None)).await;
    notifier = show(notifier, sound_spec("loud", Some("alarm-clock-elapsed"))).await;
    drop(notifier);

    let received = server.received();
    assert_eq!(received.len(), 2);
    assert_eq!(received[0].sound_name, None, "no sound, no hint");
    assert_eq!(
        received[1].sound_name.as_deref(),
        Some("alarm-clock-elapsed")
    );
}

/// A notification whose title and body hold the markup characters, as a Graph name and a text of
/// the user can.
fn markup_spec() -> nicecooling::notify::NotificationSpec {
    nicecooling::notify::NotificationSpec {
        key: "graph-markup".to_owned(),
        title: "Hot <water>".to_owned(),
        body: "<b>CPU</b> > 80 & rising".to_owned(),
        level: Level::Normal,
        buttons: Vec::new(),
        sound: None,
    }
}

/// A server with the capability `body-markup` reads the body as markup. The app escapes `&`, `<`
/// and `>`, so the server shows the text as the user wrote it. The title is plain text always.
#[tokio::test(flavor = "multi_thread")]
async fn body_is_escaped_for_a_server_with_body_markup() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("notify-markup");
    use_as_session_bus(&bus);
    let server =
        FakeNotificationServer::start(bus.connect().await, &["actions", "body", "body-markup"])
            .await;
    let (tx, _commands) = unbounded_channel();
    drop(show(Notifier::new(tx), markup_spec()).await);

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(
        received[0].body,
        "&lt;b&gt;CPU&lt;/b&gt; &gt; 80 &amp; rising"
    );
    assert_eq!(received[0].summary, "Hot <water>");
}

/// A server without `body-markup` shows the body as it comes: the app sends it as it is.
#[tokio::test(flavor = "multi_thread")]
async fn body_is_sent_as_it_is_to_a_server_without_body_markup() {
    let _guard = SESSION_LOCK.lock().await;
    let bus = PrivateBus::start("notify-no-markup");
    use_as_session_bus(&bus);
    let server = FakeNotificationServer::start(bus.connect().await, &["actions", "body"]).await;
    let (tx, _commands) = unbounded_channel();
    drop(show(Notifier::new(tx), markup_spec()).await);

    let received = server.received();
    assert_eq!(received.len(), 1);
    assert_eq!(received[0].body, "<b>CPU</b> > 80 & rising");
}
