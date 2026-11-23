mod support;

use nicecooling::command::Command;
use nicecooling::i18n::{EN, IT};
use nicecooling::model::{
    AlarmDetails, AlarmRecord, AppState, DaemonEvent, Link, Outcome, Response, ResponseRule,
    Severity,
};
use nicecooling::screens::alarms::{
    AlarmsModel, AlarmsState, AlarmsUi, CountdownDialog, DialogButton, HistoryFilter,
    default_response,
};
use nicecooling::screens::format::clock;
use nicecooling::screens::{Feedback, ScreenInput, WriteOutcome};
use nicecooling::shell::Page;
use support::bus::PrivateBus;
use support::fake_daemon::{FakeDaemon, LEAKSHIELD, Options};
use support::fixtures::{NOW_US, alarm, full};
use support::harness::Harness;
use support::labels;

fn input<'a>(state: &'a AppState, feedback: &'a Feedback, now_us: u64) -> ScreenInput<'a> {
    ScreenInput {
        state,
        now_us,
        locale: EN,
        down_since_us: None,
        feedback,
    }
}

fn model_at(state: &AppState, now_us: u64) -> AlarmsModel {
    AlarmsModel::build(
        &input(state, &Feedback::default(), now_us),
        &AlarmsUi::default(),
    )
}

fn rule(event: &str, response: Response, delay_s: u32) -> ResponseRule {
    ResponseRule {
        event: event.into(),
        source: String::new(),
        response,
        delay_s,
        overridden: false,
    }
}

/// The defaults of `docs/ui/screens/alarms.md`.
fn with_responses() -> AppState {
    let mut state = full();
    state.snapshot.responses = vec![
        rule("leakshield.leak-warning", Response::Notify, 30),
        rule("leakshield.leak-alarm", Response::Poweroff, 30),
        rule("pump-stop", Response::Poweroff, 30),
        rule("leakshield-unreachable", Response::Notify, 30),
        rule("lost-input", Response::Notify, 30),
    ];
    state
}

#[test]
fn alarms_states() {
    // Loading.
    let mut loading = with_responses();
    loading.link = Link::Connecting;
    assert_eq!(model_at(&loading, NOW_US).state, AlarmsState::Loading);

    // Nothing active, nothing in the history.
    let quiet = model_at(&with_responses(), NOW_US);
    assert_eq!(quiet.state, AlarmsState::Ready);
    assert_eq!(quiet.active.empty.as_deref(), Some("No active Alarms"));
    assert_eq!(
        quiet.history.empty.as_deref(),
        Some("No Alarm and no Warning yet.")
    );
    assert!(quiet.responses.enabled);
    assert!(quiet.responses.disabled_text.is_none());

    // A large leak Alarm with a countdown.
    let mut state = with_responses();
    let mut leak = alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        Response::Poweroff,
        30,
    );
    leak.details = AlarmDetails {
        value: Some(212.0),
        limit: Some(250.0),
        unit: "mbar".into(),
        sensor: Some(format!("{LEAKSHIELD}_pressure")),
        ..Default::default()
    };
    state.snapshot.alarms = vec![leak];
    let at = NOW_US + 9_000_000;
    let shown = model_at(&state, at);
    let row = &shown.active.rows[0];
    assert!(shown.active.empty.is_none());
    assert_eq!(row.event, "Large leak Alarm");
    assert_eq!(row.severity, Severity::Alarm);
    assert_eq!(row.source, "Leakshield, Main loop");
    assert_eq!(row.value, "212 mbar, limit 250 mbar");
    assert_eq!(row.response, "Poweroff after 30 s");
    assert_eq!(row.countdown.as_deref(), Some("Poweroff in 0:21"));
    assert_eq!(row.cancel_label, "Cancel poweroff");
    assert_eq!(
        row.cancel,
        Some(Command::CancelPoweroff {
            instance: "a1".into()
        })
    );
    assert_eq!(row.open, Some(Page::Leakshield(LEAKSHIELD.into())));

    // The poweroff was cancelled: the row says so and has no button.
    let mut cancelled = state.clone();
    cancelled.snapshot.alarms[0].poweroff_at_us = 0;
    cancelled.cancelled.insert("a1".into(), NOW_US + 5_000_000);
    let row = &model_at(&cancelled, at).active.rows[0];
    assert_eq!(
        row.cancelled.as_deref(),
        Some(
            format!(
                "Poweroff cancelled {}. The Alarm is still active.",
                clock(NOW_US + 5_000_000)
            )
            .as_str()
        )
    );
    assert!(row.cancel.is_none() && row.countdown.is_none());

    // A Warning has the triangle and no countdown.
    let mut warning = with_responses();
    warning.snapshot.alarms = vec![alarm(
        "w1",
        "leakshield.leak-warning",
        LEAKSHIELD,
        Response::Notify,
        0,
    )];
    let row = &model_at(&warning, NOW_US).active.rows[0];
    assert_eq!(row.severity, Severity::Warning);
    assert_eq!(row.response, "Notify");
    assert!(row.countdown.is_none());

    // The pump-stop Alarm names the Pump channel and shows rpm and Duty.
    let mut pump = with_responses();
    pump.snapshot.alarms = vec![alarm("p1", "pump-stop", "loop_1", Response::Poweroff, 30)];
    pump.sensors.as_mut().unwrap().sensors[2].1 = 0.0;
    let row = &model_at(&pump, NOW_US).active.rows[0];
    assert_eq!(row.event, "Pump stopped");
    assert_eq!(row.source, "Main loop: Waterpump");
    assert_eq!(row.value, "0 rpm at Duty 62 %");
    assert_eq!(
        row.caption.as_deref(),
        Some("Every Channel is at 100 %. The Alarm closes when the pump runs for 10 s.")
    );

    // Leakshield unreachable: the age of the data.
    let mut gone = with_responses();
    gone.snapshot.alarms = vec![alarm(
        "u1",
        "leakshield-unreachable",
        LEAKSHIELD,
        Response::Notify,
        0,
    )];
    let row = &model_at(&gone, NOW_US + 14_000_000).active.rows[0];
    assert_eq!(row.source, "Main loop: Leakshield");
    assert_eq!(row.value, "No data for 14 s");

    // The daemon is down: the app adds a row, and the response table is off.
    let mut down = with_responses();
    down.link = Link::Down;
    let shown = model_at(&down, NOW_US);
    let row = &shown.active.rows[0];
    assert!(row.synthetic);
    assert_eq!(row.event, "Daemon down");
    assert_eq!(row.source, "nicecooling");
    assert_eq!(row.value, "nicecooling protection is off.");
    assert_eq!(row.severity, Severity::Alarm);
    assert!(!shown.responses.enabled);
    assert_eq!(
        shown.responses.disabled_text.as_deref(),
        Some("The daemon runs the alarm responses. While it does not run, no alarm response runs.")
    );
    let mut offline = with_responses();
    offline.link = Link::Offline;
    assert!(!model_at(&offline, NOW_US).responses.enabled);
    assert!(model_at(&offline, NOW_US).active.rows.is_empty());

    // The defaults: "Large leak" and "Pump stopped" power off, the others notify.
    assert_eq!(
        default_response("leakshield.leak-alarm"),
        Response::Poweroff
    );
    assert_eq!(default_response("pump-stop"), Response::Poweroff);
    assert_eq!(default_response("leakshield-unreachable"), Response::Notify);
    let table = model_at(&with_responses(), NOW_US).responses;
    assert_eq!(table.groups[0].title, "Leakshield");
    assert_eq!(table.groups[1].title, "nicecooling");
    assert!(
        table
            .groups
            .iter()
            .flat_map(|g| &g.rows)
            .all(|r| !r.differs && r.reset.is_none())
    );

    // A row that differs has "Reset", and a delay of 0 s has a warning.
    let mut changed = with_responses();
    changed.snapshot.responses[1] = rule("leakshield.leak-alarm", Response::Poweroff, 0);
    changed.snapshot.responses[3] = rule("leakshield-unreachable", Response::None, 30);
    let table = model_at(&changed, NOW_US).responses;
    let leak_row = &table.groups[0].rows[1];
    assert!(leak_row.differs);
    assert_eq!(
        leak_row.delay_warning.as_deref(),
        Some("With 0 s you cannot cancel the poweroff.")
    );
    assert_eq!(
        leak_row.reset,
        Some(Command::ResetResponse {
            event: "leakshield.leak-alarm".into(),
            source: String::new(),
        })
    );
    let unreachable = table.groups[1]
        .rows
        .iter()
        .find(|row| row.event == "leakshield-unreachable")
        .unwrap();
    assert!(unreachable.differs && !unreachable.delay_enabled);
    assert_eq!(unreachable.options.len(), 3);
}

fn record(
    instance: &str,
    event: &str,
    severity: Severity,
    outcome: Outcome,
    raised_us: u64,
    cleared_us: Option<u64>,
) -> AlarmRecord {
    AlarmRecord {
        instance: instance.into(),
        event: event.into(),
        severity,
        source: LEAKSHIELD.into(),
        response: Response::Notify,
        raised_us,
        cleared_us,
        outcome,
        closed_by_restart: false,
        details: AlarmDetails::default(),
    }
}

#[test]
fn history_table_from_daemon() {
    let mut state = with_responses();
    state.snapshot.alarm_history = vec![
        record(
            "a2",
            "leakshield.leak-alarm",
            Severity::Alarm,
            Outcome::PoweroffCancelled,
            NOW_US + 60_000_000,
            Some(NOW_US + 90_000_000),
        ),
        record(
            "w1",
            "leakshield.leak-warning",
            Severity::Warning,
            Outcome::Notified,
            NOW_US,
            None,
        ),
    ];
    let feedback = Feedback::default();
    let all = AlarmsModel::build(&input(&state, &feedback, NOW_US), &AlarmsUi::default());
    assert!(all.history.empty.is_none());
    // The order of the daemon is kept: newest raise first.
    assert_eq!(all.history.rows[0].event, "Large leak Alarm");
    assert_eq!(all.history.rows[0].duration, "30 s");
    assert_eq!(all.history.rows[1].event, "Leak Warning");
    assert_eq!(all.history.rows[1].duration, "active");
    assert_eq!(all.history.rows[1].source, "Leakshield, Main loop");

    let mut ui = AlarmsUi::default();
    ui.set_filter(HistoryFilter::Warnings);
    let warnings = AlarmsModel::build(&input(&state, &feedback, NOW_US), &ui).history;
    assert_eq!(warnings.rows.len(), 1);
    assert_eq!(warnings.rows[0].event, "Leak Warning");
    assert_eq!(warnings.filter_label, "Warnings");
    ui.set_filter(HistoryFilter::Alarms);
    assert_eq!(
        AlarmsModel::build(&input(&state, &feedback, NOW_US), &ui)
            .history
            .rows
            .len(),
        1
    );

    // The signals raise and clear do not keep rows in the app: only the daemon does.
    let mut live = with_responses();
    live.apply(&DaemonEvent::AlarmRaised(alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        Response::Poweroff,
        30,
    )));
    assert!(live.snapshot.alarm_history.is_empty());
    live.apply(&DaemonEvent::History(state.snapshot.alarm_history.clone()));
    assert_eq!(live.snapshot.alarm_history.len(), 2);
}

#[test]
fn history_outcome_texts() {
    let texts = |locale| {
        let mut state = with_responses();
        state.snapshot.alarm_history = [
            Outcome::None,
            Outcome::Notified,
            Outcome::PoweroffPending,
            Outcome::PoweroffCancelled,
            Outcome::PoweroffStopped,
            Outcome::PoweroffRequested,
            Outcome::PoweroffFailed,
        ]
        .into_iter()
        .enumerate()
        .map(|(n, outcome)| {
            record(
                &format!("i{n}"),
                "leakshield.leak-alarm",
                Severity::Alarm,
                outcome,
                NOW_US + n as u64,
                None,
            )
        })
        .collect();
        let feedback = Feedback::default();
        AlarmsModel::build(
            &ScreenInput {
                locale,
                ..input(&state, &feedback, NOW_US)
            },
            &AlarmsUi::default(),
        )
        .history
        .rows
        .into_iter()
        .map(|row| row.response)
        .collect::<Vec<_>>()
    };
    assert_eq!(
        texts(EN),
        [
            "None",
            "Notified",
            "Poweroff pending",
            "Poweroff cancelled by the user",
            "Poweroff stopped: the Alarm closed",
            "PC powered off",
            "Poweroff failed",
        ]
    );
    assert_eq!(
        texts(IT),
        [
            "Nessuna",
            "Notificato",
            "Spegnimento in attesa",
            "Spegnimento annullato dall'utente",
            "Spegnimento fermato: l'allarme si \u{e8} chiuso",
            "PC spento",
            "Spegnimento non riuscito",
        ]
    );
}

#[test]
fn history_closed_by_restart_text() {
    let mut state = with_responses();
    let mut row = record(
        "w1",
        "leakshield.leak-warning",
        Severity::Warning,
        Outcome::Notified,
        NOW_US,
        Some(NOW_US + 5_000_000),
    );
    row.closed_by_restart = true;
    state.snapshot.alarm_history = vec![row];
    let feedback = Feedback::default();
    let shown = AlarmsModel::build(&input(&state, &feedback, NOW_US), &AlarmsUi::default());
    assert_eq!(shown.history.rows[0].response, "Closed by a daemon restart");
    let italian = AlarmsModel::build(
        &ScreenInput {
            locale: IT,
            ..input(&state, &feedback, NOW_US)
        },
        &AlarmsUi::default(),
    );
    assert_eq!(
        italian.history.rows[0].response,
        "Chiuso da un riavvio del daemon"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn history_rows_read_closed_by_restart_from_the_daemon() {
    let bus = PrivateBus::start("alarms-history-restart");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    daemon.shared.history.lock().expect("history").push((
        "alarm_1_1".into(),
        "lost-input".into(),
        "warning".into(),
        "nct6798_pwm6".into(),
        "notify".into(),
        1_000_000,
        2_000_000,
        "notified".into(),
        std::collections::HashMap::from([(
            "closed_by_restart".to_owned(),
            zbus::OwnedVariant::try_from(zbus::Variant::Bool(true)).expect("value"),
        )]),
    ));
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the history is read", |state| {
            !state.snapshot.alarm_history.is_empty()
        })
        .await;
    let row = &harness.state.snapshot.alarm_history[0];
    assert!(row.closed_by_restart);
    assert_eq!(row.cleared_us, Some(2_000_000));
}

#[tokio::test(flavor = "multi_thread")]
async fn response_table_calls_set_response() {
    let bus = PrivateBus::start("alarms-responses");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the responses are known", |state| {
            !state.snapshot.responses.is_empty()
        })
        .await;
    let feedback = Feedback::default();
    let table = AlarmsModel::build(
        &input(&harness.state, &feedback, NOW_US),
        &AlarmsUi::default(),
    )
    .responses;
    assert_eq!(table.groups.len(), 3);
    let pump = table.groups[2]
        .rows
        .iter()
        .find(|row| row.event == "pump-stop")
        .expect("the pump-stop row");
    assert_eq!(pump.response, Response::Poweroff);
    assert_eq!(pump.delay_s, 30);
    assert_eq!(
        pump.options
            .iter()
            .map(|o| o.label.as_str())
            .collect::<Vec<_>>(),
        ["None", "Notify", "Poweroff after a delay"]
    );

    // Notify only: the row changes at once.
    Command::SetAlarmResponse {
        event: "pump-stop".into(),
        source: String::new(),
        response: Response::Notify,
        delay_s: 30,
    }
    .run_on_daemon(&harness.client)
    .await
    .expect("response");
    // A delay of 0 s.
    Command::SetAlarmResponse {
        event: "leakshield.leak-alarm".into(),
        source: String::new(),
        response: Response::Poweroff,
        delay_s: 0,
    }
    .run_on_daemon(&harness.client)
    .await
    .expect("delay");
    assert_eq!(
        daemon.calls(),
        [
            "SetAlarmResponse(pump-stop,notify,30) interactive=true",
            "SetAlarmResponse(leakshield.leak-alarm,poweroff,0) interactive=true",
        ]
    );
    // The daemon does not send a signal for a response: the app reads again on the next change.
    let responses = harness.client.responses().await.expect("responses");
    let state = {
        let mut state = harness.state.clone();
        state.snapshot.responses = responses;
        state
    };
    let table =
        AlarmsModel::build(&input(&state, &feedback, NOW_US), &AlarmsUi::default()).responses;
    let pump = table.groups[2]
        .rows
        .iter()
        .find(|r| r.event == "pump-stop")
        .unwrap();
    assert_eq!(pump.response, Response::Notify);
    assert!(
        pump.differs && pump.reset.is_some(),
        "the row differs from its default"
    );
    let leak = table.groups[0]
        .rows
        .iter()
        .find(|r| r.event == "leakshield.leak-alarm")
        .unwrap();
    assert_eq!(leak.delay_s, 0);
    assert!(leak.delay_warning.is_some());

    // "Applied" shows next to the row after a write.
    let mut done = Feedback::default();
    done.set(
        "alarm-response:pump-stop".into(),
        WriteOutcome::Applied { at_us: NOW_US },
    );
    let table = AlarmsModel::build(&input(&state, &done, NOW_US), &AlarmsUi::default()).responses;
    let pump = table.groups[2]
        .rows
        .iter()
        .find(|r| r.event == "pump-stop")
        .unwrap();
    assert!(
        pump.write_line
            .as_ref()
            .unwrap()
            .text
            .starts_with("Applied")
    );
    assert_eq!(
        Command::SetAlarmResponse {
            event: "pump-stop".into(),
            source: String::new(),
            response: Response::None,
            delay_s: 30
        }
        .feedback_key()
        .as_deref(),
        Some("alarm-response:pump-stop")
    );

    // The daemon refuses an unknown event.
    let refused = Command::SetAlarmResponse {
        event: "mystery".into(),
        source: String::new(),
        response: Response::Notify,
        delay_s: 30,
    }
    .run_on_daemon(&harness.client)
    .await;
    assert!(refused.is_err());
}

#[tokio::test(flavor = "multi_thread")]
async fn alarms_reach_the_history_from_the_daemon() {
    let bus = PrivateBus::start("alarms-history");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the daemon is known", |state| {
            !state.snapshot.profiles.is_empty()
        })
        .await;
    assert!(harness.state.snapshot.alarm_history.is_empty());
    daemon
        .raise_alarm("a1", "pump-stop", "alarm", "loop_1", "notify", 0)
        .await;
    harness
        .until("the Alarm is in the history", |state| {
            state.snapshot.alarm_history.len() == 1
        })
        .await;
    let row = &harness.state.snapshot.alarm_history[0];
    assert_eq!((row.event.as_str(), row.cleared_us), ("pump-stop", None));
    assert_eq!(row.outcome, Outcome::Notified);
    daemon.clear_alarm("a1").await;
    harness
        .until("the Alarm closes", |state| {
            state.snapshot.alarm_history[0].cleared_us.is_some()
        })
        .await;
}

#[test]
fn countdown_dialog_default_focus_on_cancel() {
    let mut state = with_responses();
    let mut leak = alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        Response::Poweroff,
        30,
    );
    leak.details = AlarmDetails {
        value: Some(212.0),
        limit: Some(250.0),
        unit: "mbar".into(),
        sensor: None,
        ..Default::default()
    };
    state.snapshot.alarms = vec![leak];
    let feedback = Feedback::default();
    let at = NOW_US + 9_000_000;
    let dialog = CountdownDialog::build(&input(&state, &feedback, at)).expect("a countdown runs");
    assert_eq!(dialog.default_focus, DialogButton::Cancel);
    assert_eq!(dialog.title, "Large leak Alarm");
    assert_eq!(dialog.text, "The PC powers off when the countdown ends.");
    assert_eq!(dialog.seconds, 21);
    assert_eq!(dialog.seconds_label, "seconds left of 30");
    assert!((dialog.fraction - 0.7).abs() < 0.001);
    assert_eq!(
        (dialog.cancel.as_str(), dialog.hide.as_str()),
        ("Cancel poweroff", "Hide")
    );
    assert_eq!(dialog.hint, "The countdown continues in the banner.");
    assert_eq!(
        dialog.rows[0],
        ("Source".to_owned(), "Leakshield, Main loop".to_owned())
    );
    assert_eq!(dialog.rows[2].1, "212 mbar, limit 250 mbar");
    assert_eq!(dialog.rows[3].1, "Poweroff after 30 s");
    assert!(!dialog.ended && dialog.more.is_none());
    assert_eq!(
        dialog.cancel_command(),
        Command::CancelPoweroff {
            instance: "a1".into()
        }
    );

    // Another Alarm with a shorter countdown: the dialog shows it and counts both.
    state
        .snapshot
        .alarms
        .push(alarm("a2", "pump-stop", "loop_1", Response::Poweroff, 12));
    let dialog = CountdownDialog::build(&input(&state, &feedback, at)).unwrap();
    assert_eq!(dialog.instance, "a2");
    assert_eq!(dialog.title, "Pump stopped");
    assert_eq!(dialog.more.as_deref(), Some("2 Alarms"));

    // The countdown ended: the dialog says so and has no buttons.
    let ended = CountdownDialog::build(&input(&state, &feedback, NOW_US + 40_000_000)).unwrap();
    assert!(ended.ended);
    assert_eq!(ended.text, "The PC powers off now.");

    // No countdown, no dialog. A cancelled poweroff closes it.
    let mut idle = with_responses();
    idle.snapshot.alarms = vec![alarm(
        "w1",
        "leakshield.leak-warning",
        LEAKSHIELD,
        Response::Notify,
        0,
    )];
    assert!(CountdownDialog::build(&input(&idle, &feedback, at)).is_none());
    state.apply(&DaemonEvent::PoweroffCancelled {
        instance: "a1".into(),
    });
    state.apply(&DaemonEvent::PoweroffCancelled {
        instance: "a2".into(),
    });
    assert!(CountdownDialog::build(&input(&state, &feedback, at)).is_none());
    let _ = IT;
}

#[test]
fn alarms_labels() {
    let en = labels::load("en");
    let it = labels::load("it");
    // The copy tables of docs/ui/screens/alarms.md.
    let table = [
        ("alarms.active.title", "Active", "Attivi"),
        (
            "alarms.active.empty",
            "No active Alarms",
            "Nessun Allarme attivo",
        ),
        (
            "alarms.responses.title",
            "Alarm responses",
            "Risposte agli allarmi",
        ),
        ("alarms.history.title", "History", "Cronologia"),
        (
            "alarms.history.empty",
            "No Alarm and no Warning yet.",
            "Ancora nessun Allarme e nessun Avviso.",
        ),
        ("alarms.option.none", "None", "Nessuna"),
        ("alarms.option.notify", "Notify", "Notifica"),
        (
            "alarms.option.poweroff",
            "Poweroff after a delay",
            "Spegni dopo un ritardo",
        ),
        (
            "alarms.cancel_poweroff",
            "Cancel poweroff",
            "Annulla lo spegnimento",
        ),
        ("alarms.history.notified", "Notified", "Notificato"),
        (
            "alarms.history.cancelled",
            "Poweroff cancelled by the user",
            "Spegnimento annullato dall'utente",
        ),
        (
            "alarms.history.poweroff_pending",
            "Poweroff pending",
            "Spegnimento in attesa",
        ),
        (
            "alarms.history.poweroff_requested",
            "PC powered off",
            "PC spento",
        ),
        (
            "alarms.history.poweroff_stopped",
            "Poweroff stopped: the Alarm closed",
            "Spegnimento fermato: l'allarme si \u{e8} chiuso",
        ),
        (
            "alarms.history.poweroff_failed",
            "Poweroff failed",
            "Spegnimento non riuscito",
        ),
        (
            "alarms.history.closed_by_restart",
            "Closed by a daemon restart",
            "Chiuso da un riavvio del daemon",
        ),
        ("alarms.dialog.hide", "Hide", "Nascondi"),
        ("alarms.daemon_down.title", "Daemon down", "Demone fermo"),
        (
            "alarms.dialog.text",
            "The PC powers off when the countdown ends.",
            "Il PC si spegne quando il conto alla rovescia finisce.",
        ),
        (
            "alarms.dialog.hint",
            "The countdown continues in the banner.",
            "Il conto alla rovescia continua nel banner.",
        ),
        (
            "alarms.delay_zero",
            "With 0 s you cannot cancel the poweroff.",
            "Con 0 s non puoi annullare lo spegnimento.",
        ),
        (
            "alarms.responses.offline",
            "The daemon runs the alarm responses. While it does not run, no alarm response runs.",
            "Il demone esegue le risposte agli allarmi. Finché non è in esecuzione, nessuna risposta agli allarmi è attiva.",
        ),
        (
            "alarms.caption.pump_stop",
            "Every Channel is at 100 %. The Alarm closes when the pump runs for 10 s.",
            "Ogni Canale è al 100 %. L'Allarme si chiude quando la pompa gira per 10 s.",
        ),
    ];
    for (key, english, italian) in table {
        assert_eq!(en[key], english, "English text of {key}");
        assert_eq!(it[key], italian, "Italian text of {key}");
    }
    for key in en.keys().filter(|key| key.starts_with("alarms.")) {
        assert!(it.contains_key(key), "Italian text of {key}");
    }
    // Every event of the response table has a title in both languages.
    for event in [
        "leakshield.leak-warning",
        "leakshield.leak-alarm",
        "pump-stop",
        "leakshield-unreachable",
        "lost-input",
    ] {
        assert_ne!(EN.event_title(event), IT.event_title(event), "{event}");
        assert!(!EN.event_title(event).starts_with("Event "));
    }
}

#[test]
fn a_cancel_that_came_too_late_says_the_poweroff_started() {
    let mut state = with_responses();
    state.snapshot.alarms = vec![alarm(
        "alarm_1_1",
        "pump-stop",
        "loop_1",
        Response::Poweroff,
        30,
    )];
    let cancel = Command::CancelPoweroff {
        instance: "alarm_1_1".into(),
    };
    let key = cancel
        .feedback_key()
        .expect("the cancel has a feedback key");
    let mut feedback = Feedback::default();
    feedback.set(
        key,
        WriteOutcome::Refused {
            error: "PoweroffStarted".into(),
        },
    );
    for (locale, text) in [
        (EN, "The PC already powers off. The cancel came too late."),
        (
            IT,
            "Il PC si sta già spegnendo. L'annullamento è arrivato troppo tardi.",
        ),
    ] {
        let mut input = input(&state, &feedback, NOW_US);
        input.locale = locale;
        let model = AlarmsModel::build(&input, &AlarmsUi::default());
        assert_eq!(model.active.rows[0].cancelled.as_deref(), Some(text));
    }
}

fn leakshield_rule(
    event: &str,
    response: Response,
    delay_s: u32,
    overridden: bool,
) -> ResponseRule {
    ResponseRule {
        source: LEAKSHIELD.into(),
        overridden,
        ..rule(event, response, delay_s)
    }
}

/// The defaults, then one resolved row per Leakshield event, as `GetResponses` returns them.
fn with_leakshield_rows() -> AppState {
    let mut state = with_responses();
    state.snapshot.responses.extend([
        leakshield_rule("leakshield.leak-warning", Response::Notify, 30, false),
        leakshield_rule("leakshield.leak-alarm", Response::Poweroff, 30, false),
    ]);
    state
}

#[test]
fn response_rows_per_leakshield() {
    let state = with_leakshield_rows();
    let table = model_at(&state, NOW_US).responses;
    let titles: Vec<&str> = table.groups.iter().map(|g| g.title.as_str()).collect();
    assert_eq!(
        titles,
        ["Leakshield", "Leakshield 20417-31072", "nicecooling"]
    );
    // The defaults keep source "", the rows of the device carry its id.
    assert!(table.groups[0].rows.iter().all(|r| r.source.is_empty()));
    let device = &table.groups[1].rows;
    assert_eq!(device.len(), 2);
    assert!(device.iter().all(|r| r.source == LEAKSHIELD));
    assert_eq!(
        device[0].key,
        format!("leakshield.leak-warning@{LEAKSHIELD}")
    );

    // An override row of the device is changed with its source.
    let mut state = with_leakshield_rows();
    state.snapshot.responses[6] =
        leakshield_rule("leakshield.leak-alarm", Response::Notify, 30, true);
    let table = model_at(&state, NOW_US).responses;
    let row = &table.groups[1].rows[1];
    assert_eq!(row.response, Response::Notify);
    assert_eq!(
        Command::SetAlarmResponse {
            event: row.event.clone(),
            source: row.source.clone(),
            response: Response::Poweroff,
            delay_s: 30,
        }
        .feedback_key()
        .as_deref(),
        Some("alarm-response:leakshield.leak-alarm@leakshield_20417_31072")
    );

    // The delay of an alarm that rises uses the row of its Leakshield.
    let mut leak = alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        Response::Poweroff,
        30,
    );
    leak.poweroff_at_us = NOW_US + 30_000_000;
    state.snapshot.responses[6] =
        leakshield_rule("leakshield.leak-alarm", Response::Poweroff, 45, true);
    state.snapshot.alarms = vec![leak];
    assert_eq!(
        model_at(&state, NOW_US).active.rows[0].response,
        "Poweroff after 45 s"
    );
}

#[test]
fn reset_link_on_override_and_changed_default() {
    // Nothing differs: no link.
    let table = model_at(&with_leakshield_rows(), NOW_US).responses;
    assert!(
        table
            .groups
            .iter()
            .flat_map(|g| &g.rows)
            .all(|r| r.reset.is_none())
    );

    // A Leakshield row with an override: "Reset" removes the override.
    let mut state = with_leakshield_rows();
    state.snapshot.responses[6] =
        leakshield_rule("leakshield.leak-alarm", Response::Notify, 30, true);
    let table = model_at(&state, NOW_US).responses;
    assert_eq!(
        table.groups[1].rows[1].reset,
        Some(Command::ResetResponse {
            event: "leakshield.leak-alarm".into(),
            source: LEAKSHIELD.into(),
        })
    );
    assert!(table.groups[1].rows[0].reset.is_none());

    // A default row that differs from the built-in default: "Reset" with source "".
    let mut state = with_leakshield_rows();
    state.snapshot.responses[0] = rule("leakshield.leak-warning", Response::None, 30);
    let table = model_at(&state, NOW_US).responses;
    assert_eq!(
        table.groups[0].rows[0].reset,
        Some(Command::ResetResponse {
            event: "leakshield.leak-warning".into(),
            source: String::new(),
        })
    );
}

#[test]
fn overlap_icon_from_proposals() {
    let mut state = with_leakshield_rows();
    let table = model_at(&state, NOW_US).responses;
    assert!(
        table
            .groups
            .iter()
            .flat_map(|g| &g.rows)
            .all(|r| r.overlap.is_none())
    );

    let mut data = support::fixtures::leakshield_data();
    data.proposals = vec![nicecooling::model::ResponseProposal {
        alarm: "leak-alarm".into(),
        current: "signal-long".into(),
        proposed: "none".into(),
        overlap: "double-poweroff".into(),
    }];
    state
        .snapshot
        .leakshield_data
        .insert(LEAKSHIELD.into(), data);
    let table = model_at(&state, NOW_US).responses;
    let text = "The Leakshield also reacts to this Alarm. See its Alarms tab.";
    let with_icon: Vec<(&str, &str)> = table
        .groups
        .iter()
        .flat_map(|g| &g.rows)
        .filter(|r| r.overlap.is_some())
        .map(|r| (r.event.as_str(), r.source.as_str()))
        .collect();
    assert_eq!(
        with_icon,
        [
            ("leakshield.leak-alarm", ""),
            ("leakshield.leak-alarm", LEAKSHIELD)
        ]
    );
    assert_eq!(table.groups[1].rows[1].overlap.as_deref(), Some(text));

    // A proposal without an overlap shows no icon.
    state
        .snapshot
        .leakshield_data
        .get_mut(LEAKSHIELD)
        .unwrap()
        .proposals[0]
        .overlap = String::new();
    let table = model_at(&state, NOW_US).responses;
    assert!(
        table
            .groups
            .iter()
            .flat_map(|g| &g.rows)
            .all(|r| r.overlap.is_none())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn responses_of_a_leakshield_go_through_the_daemon() {
    let bus = PrivateBus::start("alarms-per-leakshield");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the responses are known", |state| {
            state
                .snapshot
                .responses
                .iter()
                .any(|r| r.source == LEAKSHIELD)
        })
        .await;
    Command::SetAlarmResponse {
        event: "leakshield.leak-alarm".into(),
        source: LEAKSHIELD.into(),
        response: Response::Notify,
        delay_s: 30,
    }
    .run_on_daemon(&harness.client)
    .await
    .expect("override");
    let rows = harness.client.responses().await.expect("responses");
    let row = rows
        .iter()
        .find(|r| r.event == "leakshield.leak-alarm" && r.source == LEAKSHIELD)
        .unwrap();
    assert!(row.overridden && row.response == Response::Notify);
    let default = rows
        .iter()
        .find(|r| r.event == "leakshield.leak-alarm" && r.source.is_empty())
        .unwrap();
    assert_eq!(default.response, Response::Poweroff);
    Command::ResetResponse {
        event: "leakshield.leak-alarm".into(),
        source: LEAKSHIELD.into(),
    }
    .run_on_daemon(&harness.client)
    .await
    .expect("reset");
    let rows = harness.client.responses().await.expect("responses");
    assert!(rows.iter().all(|r| !r.overridden));
    assert_eq!(
        daemon.calls(),
        [
            format!(
                "SetAlarmResponse(leakshield.leak-alarm@{LEAKSHIELD},notify,30) interactive=true"
            ),
            format!("ResetResponse(leakshield.leak-alarm,{LEAKSHIELD}) interactive=true"),
        ]
    );
}
