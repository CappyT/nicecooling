mod support;

use nicecooling::i18n::{EN, IT};
use nicecooling::model::{AppState, ChannelOverride, Control, Link, Response};
use nicecooling::screens::channels::{ChannelsModel, ChannelsState, ChannelsUi};
use nicecooling::screens::{Feedback, ScreenInput, WriteOutcome};
use support::bus::PrivateBus;
use support::fake_daemon::{FakeDaemon, Options};
use support::fixtures::{NOW_US, alarm, full};
use support::harness::Harness;
use support::labels;

fn input<'a>(state: &'a AppState, feedback: &'a Feedback) -> ScreenInput<'a> {
    ScreenInput {
        state,
        now_us: NOW_US,
        locale: EN,
        down_since_us: None,
        feedback,
    }
}

fn model(state: &AppState, ui: &ChannelsUi) -> ChannelsModel {
    ChannelsModel::build(&input(state, &Feedback::default()), ui)
}

#[test]
fn channels_states() {
    let mut ui = ChannelsUi::default();

    let mut state = full();
    state.link = Link::Connecting;
    assert_eq!(model(&state, &ui).state, ChannelsState::Loading);

    let mut state = full();
    state.snapshot.channels.clear();
    let empty = model(&state, &ui);
    assert_eq!(empty.state, ChannelsState::Empty);
    assert_eq!(
        empty.empty_text,
        "No Channel found. nicecooling needs a hwmon driver for the fan controller of this Board."
    );
    assert_eq!(empty.empty_link, "Open Settings");
    assert!(empty.detail.is_none());

    // The list: the control of each Channel on its second line, the tag of a Pump channel.
    let state = full();
    let ready = model(&state, &ui);
    assert_eq!(ready.state, ChannelsState::Ready);
    let fan = &ready.rows[0];
    assert_eq!(fan.name, "Chassis Fan 1");
    assert_eq!(fan.line2, "Hardware curve, not controlled");
    assert_eq!(fan.pump_tag, None);
    assert_eq!(fan.no_signal.as_deref(), Some("no speed signal"));
    let pump = &ready.rows[1];
    assert_eq!(pump.line2, "Curve on Water temperature");
    assert_eq!(pump.pump_tag.as_deref(), Some("Pump channel"));
    assert_eq!(
        (pump.rpm.as_str(), pump.duty.as_str()),
        ("2840 rpm", "62 %")
    );
    assert!(pump.no_signal.is_none());
    // Without a selection the first Channel opens: "Chassis Fan 1" has the Hardware curve.
    let detail = ready.detail.as_ref().unwrap();
    assert_eq!(detail.name, "Chassis Fan 1");
    assert_eq!(
        detail.hardware_text.as_deref(),
        Some("nicecooling does not control this Channel. The Board runs its Hardware curve.")
    );
    assert!(detail.curve.is_none() && detail.fixed.is_none());
    assert!(ready.rows[0].selected);

    // A Curve: the Input, the plot with the Pump floor and the operating point, the readout.
    ui.select("nct6798_pwm6");
    let curve_state = model(&state, &ui);
    let detail = curve_state.detail.as_ref().unwrap();
    assert!(detail.editable);
    let curve = detail.curve.as_ref().unwrap();
    assert_eq!(curve.input.name, "Water temperature");
    assert_eq!(curve.input.value, "31.4 °C");
    assert_eq!(curve.smoothing_s, 5.0);
    assert_eq!(curve.step_limit, 5.0);
    assert_eq!(
        curve.smoothing_help,
        "Input smoothing: the Curve reads the average of the last 5 s, so a short spike does not move the fan."
    );
    assert_eq!(
        curve.step_help,
        "Duty step limit: the Duty changes by 5 % per second at most."
    );
    assert_eq!(curve.plot.y_title, "Duty, %");
    assert_eq!(curve.plot.x_title, "Input, °C");
    assert_eq!(
        curve.plot.floor.as_ref().map(|f| f.1.as_str()),
        Some("Pump floor 40 %: no Curve goes below it")
    );
    let now = curve.plot.operating.as_ref().unwrap();
    assert_eq!(now.label, "Now 31.4 °C, 62 %");
    assert!(curve.plot.handles);
    assert_eq!(curve.readout.input, "Input 31.4 °C");
    assert_eq!(curve.readout.smoothing, "5 s average");
    assert_eq!(curve.readout.curve, "Curve 43 %");
    assert_eq!(curve.readout.duty, "Duty 62 %");
    assert!(curve.can_add && curve.can_remove);
    assert_eq!(curve.points.len(), 3);

    // Fixed Duty: a slider, and the Pump floor note.
    let mut fixed = full();
    fixed
        .snapshot
        .channels
        .get_mut("nct6798_pwm6")
        .unwrap()
        .control = Control::Fixed;
    let fixed_model = model(&fixed, &ui);
    let detail = fixed_model.detail.as_ref().unwrap();
    assert_eq!(detail.fixed.as_ref().unwrap().duty, 50.0);
    assert_eq!(detail.fixed.as_ref().unwrap().floor, Some(40.0));
    assert!(
        detail
            .controls
            .iter()
            .find(|c| c.control == Control::Fixed)
            .unwrap()
            .active
    );
    assert_eq!(
        ui.set_fixed_duty(&input(&fixed, &Feedback::default()), 30.0),
        None,
        "a Fixed Duty below the Pump floor is refused"
    );
    assert_eq!(
        model_with_ui(&fixed, &ui)
            .detail
            .unwrap()
            .fixed
            .unwrap()
            .error
            .as_deref(),
        Some("The Pump floor is 40 %.")
    );
    assert!(
        ui.set_fixed_duty(&input(&fixed, &Feedback::default()), 45.0)
            .is_some()
    );

    // Lost Input: the triangle, 100 %, a Warning strip, and no operating point.
    ui.select("nct6798_pwm6");
    let mut lost = full();
    {
        let pump = lost.snapshot.channels.get_mut("nct6798_pwm6").unwrap();
        pump.override_state = ChannelOverride::LostInput;
    }
    lost.sensors.as_mut().unwrap().sensors[0].2 = 1;
    lost.sensors.as_mut().unwrap().channels[0].1 = 100.0;
    let lost_model = model(&lost, &ui);
    let row = lost_model
        .rows
        .iter()
        .find(|r| r.id == "nct6798_pwm6")
        .unwrap();
    assert_eq!(row.line2, "Curve, Input lost: Water temperature");
    assert!(row.line2_warning);
    assert_eq!(row.duty, "100 %");
    let detail = lost_model.detail.as_ref().unwrap();
    assert_eq!(
        detail.lost_input.as_deref(),
        Some("Input lost: the Channel runs at 100 % until Water temperature returns.")
    );
    assert!(detail.curve.as_ref().unwrap().plot.operating.is_none());

    // The pump-stop Alarm: every row shows 100 %, a red row, the editor is read-only.
    let mut stopped = full();
    stopped.snapshot.alarms = vec![alarm("a1", "pump-stop", "loop_1", Response::Poweroff, 30)];
    let stopped = model(&stopped, &ui);
    assert_eq!(
        stopped.pump_stop.as_deref(),
        Some("Pump stopped. Every Channel is at 100 % until the pump runs for 10 s.")
    );
    assert!(stopped.rows.iter().all(|r| r.duty == "100 %"));
    let detail = stopped.detail.as_ref().unwrap();
    assert!(!detail.editable && !detail.curve.as_ref().unwrap().plot.handles);

    // Daemon offline: the last Curve shows, the handles are hidden.
    let mut offline = full();
    offline.link = Link::Offline;
    offline.sensors = None;
    let offline = model(&offline, &ui);
    let detail = offline.detail.as_ref().unwrap();
    assert!(!detail.editable);
    assert_eq!(
        detail.disabled_reason.as_deref(),
        Some("The daemon is offline.")
    );
    let curve = detail.curve.as_ref().unwrap();
    assert!(!curve.plot.handles);
    assert_eq!(curve.points.len(), 3);
    assert!(curve.plot.operating.is_none());

    // Write failed: the readout row says so, with "Try again".
    let state = full();
    let mut feedback = Feedback::default();
    feedback.set(
        "channel:nct6798_pwm6".into(),
        WriteOutcome::Refused {
            error: "InvalidArgument".into(),
        },
    );
    let failed = ChannelsModel::build(&input(&state, &feedback), &ui);
    let line = failed.detail.unwrap().write_line.unwrap();
    assert_eq!(line.text, "Not applied: the daemon refused the Curve.");
    assert!(line.failure);
    assert_eq!(line.retry.as_deref(), Some("Try again"));
    feedback.set(
        "channel:nct6798_pwm6".into(),
        WriteOutcome::Applied { at_us: NOW_US },
    );
    let applied = ChannelsModel::build(&input(&state, &feedback), &ui)
        .detail
        .unwrap()
        .write_line
        .unwrap();
    assert!(applied.text.starts_with("Applied "));
    assert!(!applied.failure && applied.retry.is_none());
    feedback.set("channel:nct6798_pwm6".into(), WriteOutcome::NotAuthorized);
    let denied = ChannelsModel::build(&input(&state, &feedback), &ui)
        .detail
        .unwrap()
        .write_line
        .unwrap();
    assert_eq!(denied.text, "Not authorized. Nothing changed.");
}

fn model_with_ui(state: &AppState, ui: &ChannelsUi) -> ChannelsModel {
    model(state, ui)
}

#[tokio::test(flavor = "multi_thread")]
async fn curve_edit_sends_set_curve_to_fake_daemon() {
    let bus = PrivateBus::start("channels-curve");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Channels are known", |state| {
            state.snapshot.channels.contains_key("nct6798_pwm6")
        })
        .await;
    assert_eq!(
        harness.state.snapshot.channels["nct6798_pwm6"].curve.points,
        [(30.0, 40.0), (45.0, 70.0), (60.0, 100.0)]
    );

    // A drag moves the point without a call. The call goes out when the drag ends.
    let mut ui = ChannelsUi::default();
    ui.select("nct6798_pwm6");
    ui.drag_point(&harness.state, 1, 45.4, 79.6);
    ui.drag_point(&harness.state, 1, 47.0, 80.2);
    assert!(daemon.calls().is_empty(), "no call during the drag");
    assert_eq!(ui.draft.as_ref().unwrap().points[1], (47.0, 80.0));
    let command = ui.commit().expect("a change");
    assert!(
        command
            .run_on_daemon(&harness.client)
            .await
            .expect("the call works")
    );
    assert_eq!(
        daemon.calls(),
        [
            "SetCurve(nct6798_pwm6,nct6798_temp1,[(30.0, 40.0), (47.0, 80.0), (60.0, 100.0)],5,5) interactive=true"
        ]
    );
    harness
        .until("the daemon holds the new Curve", |state| {
            state.snapshot.channels["nct6798_pwm6"].curve.points[1] == (47.0, 80.0)
        })
        .await;
    ui.sync(&harness.state);
    assert!(
        ui.draft.is_none(),
        "the draft ends when the daemon holds it"
    );
    assert_eq!(
        harness.state.snapshot.channels["nct6798_pwm6"].control,
        Control::Curve
    );

    // A number input changes one point: one call.
    let command = ui
        .set_point(&harness.state, 2, 62.0, 100.0)
        .expect("a change");
    command
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    assert!(daemon.calls()[1].starts_with(
        "SetCurve(nct6798_pwm6,nct6798_temp1,[(30.0, 40.0), (47.0, 80.0), (62.0, 100.0)]"
    ));

    // A point cannot go under the Pump floor, and cannot pass its neighbor.
    let mut ui = ChannelsUi::default();
    ui.select("nct6798_pwm6");
    ui.drag_point(&harness.state, 0, 30.0, 5.0);
    assert_eq!(ui.draft.as_ref().unwrap().points[0], (30.0, 40.0));
    ui.drag_point(&harness.state, 0, 90.0, 40.0);
    assert_eq!(ui.draft.as_ref().unwrap().points[0].0, 46.0);

    // Add and remove a point, and change the Input fields: each change goes to the daemon.
    let mut ui = ChannelsUi::default();
    ui.select("nct6798_pwm6");
    let added = ui
        .add_point(&harness.state, Some(40.0))
        .expect("a new point");
    added
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    assert!(
        daemon.calls()[2].contains("(40.0, 64.0)"),
        "{}",
        daemon.calls()[2]
    );
    let removed = ui.remove_point(&harness.state, 0).expect("a point leaves");
    removed
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    let smoothing = ui.set_smoothing(&harness.state, 8.0).expect("a change");
    smoothing
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    assert!(
        daemon
            .calls()
            .last()
            .unwrap()
            .ends_with(",8,5) interactive=true")
    );
    let step = ui.set_step_limit(&harness.state, 10.0).expect("a change");
    step.run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    assert!(
        daemon
            .calls()
            .last()
            .unwrap()
            .ends_with(",8,10) interactive=true")
    );

    // polkit denies the call: the model says "Not authorized. Nothing changed.".
    daemon.deny_writes(true);
    let command = ui
        .set_input(&harness.state, "k10temp_tctl")
        .expect("a change");
    let error = command
        .run_on_daemon(&harness.client)
        .await
        .expect_err("denied");
    assert_eq!(
        WriteOutcome::from_error(&error),
        WriteOutcome::NotAuthorized
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn release_to_hardware() {
    let bus = PrivateBus::start("channels-release");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Channels are known", |state| {
            state.snapshot.channels.contains_key("nct6798_pwm6")
        })
        .await;

    // From "Curve" to "Hardware curve" a dialog asks first. Cancel sends nothing.
    let mut ui = ChannelsUi::default();
    ui.select("nct6798_pwm6");
    assert_eq!(ui.request_control(&harness.state, Control::Hardware), None);
    let feedback = Feedback::default();
    let dialog = ChannelsModel::build(&input(&harness.state, &feedback), &ui)
        .detail
        .unwrap()
        .release_dialog
        .expect("the dialog is open");
    assert_eq!(dialog.title, "Give this Channel back to the Board?");
    assert_eq!(dialog.text, "nicecooling stops its Curve.");
    assert_eq!(
        (dialog.cancel.as_str(), dialog.confirm.as_str()),
        ("Cancel", "Use the Hardware curve")
    );
    ui.cancel_release();
    assert!(daemon.calls().is_empty());

    assert_eq!(ui.request_control(&harness.state, Control::Hardware), None);
    let command = ui
        .confirm_release(&harness.state)
        .expect("the confirmation sends");
    assert!(!ui.release_dialog);
    command
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    assert_eq!(
        daemon.calls(),
        ["ReleaseToHardware(nct6798_pwm6) interactive=true"]
    );
    harness
        .until("the Channel is on its Hardware curve", |state| {
            state.snapshot.channels["nct6798_pwm6"].control == Control::Hardware
        })
        .await;

    // From "Hardware curve" to "Curve" applies at once, with the Curve that the Channel holds.
    let command = ui
        .request_control(&harness.state, Control::Curve)
        .expect("no dialog on the way out");
    command
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    harness
        .until("the Channel runs its Curve", |state| {
            state.snapshot.channels["nct6798_pwm6"].control == Control::Curve
        })
        .await;

    // To "Fixed Duty".
    let command = ui
        .request_control(&harness.state, Control::Fixed)
        .expect("applies at once");
    command
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    assert!(
        daemon
            .calls()
            .last()
            .unwrap()
            .starts_with("SetFixedDuty(nct6798_pwm6,50")
    );
    harness
        .until("the Channel runs a fixed Duty", |state| {
            state.snapshot.channels["nct6798_pwm6"].control == Control::Fixed
        })
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn rename_a_channel_through_api() {
    let bus = PrivateBus::start("channels-rename");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Channels are known", |state| {
            state.snapshot.channels.contains_key("nct6798_pwm6")
        })
        .await;
    let mut ui = ChannelsUi::default();
    ui.select("nct6798_pwm6");
    ui.open_rename(&harness.state);
    ui.rename.as_mut().unwrap().1 = "Pump".into();
    ui.rename_command()
        .unwrap()
        .run_on_daemon(&harness.client)
        .await
        .expect("the call works");
    assert_eq!(
        daemon.calls(),
        ["SetChannelName(nct6798_pwm6,Pump) interactive=true"]
    );
    harness
        .until("the new name shows", |state| {
            state.snapshot.channels["nct6798_pwm6"].name == "Pump"
        })
        .await;
}

#[test]
fn channels_labels() {
    let en = labels::load("en");
    let it = labels::load("it");
    // The copy table of docs/ui/screens/channels.md.
    let table = [
        ("channels.field.input", "Input", "Input"),
        (
            "channels.field.smoothing",
            "Input smoothing",
            "Smorzamento Input",
        ),
        ("channels.field.smoothing_unit", "s average", "s di media"),
        (
            "channels.field.step",
            "Duty step limit",
            "Limite di variazione Duty",
        ),
        ("channels.field.step_unit", "% per s", "% al s"),
        ("channels.plot.y_title", "Duty, %", "Duty, %"),
        ("channels.control.curve", "Curve", "Curva"),
        ("channels.control.fixed", "Fixed Duty", "Duty fisso"),
        (
            "channels.control.hardware",
            "Hardware curve",
            "Curva hardware",
        ),
        ("channels.pump_tag", "Pump channel", "Canale pompa"),
        ("channels.add_point", "Add point", "Aggiungi punto"),
        ("channels.remove_point", "Remove point", "Rimuovi punto"),
        ("channels.bios.title", "BIOS snapshot", "Snapshot BIOS"),
        (
            "channels.bios.text",
            "When the daemon stops, the Board runs its Hardware curve for this Channel.",
            "Quando il demone si ferma, la Scheda applica la sua Curva hardware a questo Canale.",
        ),
        (
            "channels.release.title",
            "Give this Channel back to the Board?",
            "Restituire questo Canale alla Scheda?",
        ),
        ("channels.release.cancel", "Cancel", "Annulla"),
        (
            "channels.release.confirm",
            "Use the Hardware curve",
            "Usa la Curva hardware",
        ),
        ("channels.try_again", "Try again", "Riprova"),
        (
            "channels.refused_curve",
            "Not applied: the daemon refused the Curve.",
            "Non applicato: il demone ha rifiutato la Curva.",
        ),
        (
            "channels.empty.link",
            "Open Settings",
            "Apri le Impostazioni",
        ),
        (
            "channels.no_signal",
            "no speed signal",
            "nessun segnale di velocità",
        ),
    ];
    for (key, english, italian) in table {
        assert_eq!(en[key], english, "{key}");
        assert_eq!(it[key], italian, "{key}");
    }
    // The model shows the Italian words.
    let state = full();
    let feedback = Feedback::default();
    let mut ui = ChannelsUi::default();
    ui.select("nct6798_pwm6");
    let italian = ChannelsModel::build(
        &ScreenInput {
            locale: IT,
            ..input(&state, &feedback)
        },
        &ui,
    );
    assert_eq!(italian.rows[1].line2, "Curva su Water temperature");
    let curve = italian.detail.unwrap().curve.unwrap();
    assert_eq!(curve.readout.smoothing, "media su 5 s");
    assert_eq!(curve.plot.operating.unwrap().label, "Ora 31,4 °C, 62 %");
}
