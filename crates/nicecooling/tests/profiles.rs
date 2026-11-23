mod support;

use nicecooling::command::Command;
use nicecooling::daemon_client::rule_params;
use nicecooling::i18n::{EN, IT};
use nicecooling::model::{
    AppState, Cause, FieldValue, Link, ProfileContent, ProfileInfo, Rule, RuleGroup, WriteFailure,
};
use nicecooling::screens::profiles::{
    NoticeKind, ProfilesModel, ProfilesState, ProfilesUi, RULE_KINDS, RuleKind, rule_keys,
    rule_sentence, set_rule_field,
};
use nicecooling::screens::{Feedback, ScreenInput};
use support::bus::PrivateBus;
use support::fake_daemon::{FakeDaemon, LEAKSHIELD, Options};
use support::fixtures::{NOW_US, full};
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

fn model(state: &AppState, ui: &ProfilesUi) -> ProfilesModel {
    ProfilesModel::build(&input(state, &Feedback::default(), NOW_US), ui)
}

fn profile(id: &str, name: &str, slot: u8) -> ProfileInfo {
    ProfileInfo {
        id: id.into(),
        name: name.into(),
        is_default: id == "profile_default",
        slot,
        ..Default::default()
    }
}

fn group(id: &str, target: &str, rules: Vec<Rule>) -> RuleGroup {
    RuleGroup {
        id: id.into(),
        profile: target.into(),
        enabled: true,
        rules,
        holds: false,
        since_us: 0,
    }
}

fn sensor_rule() -> Rule {
    Rule::Sensor {
        sensor: "nct6798_temp1".into(),
        comparison: "gt".into(),
        value: 38.0,
        value_max: 0.0,
        hold_s: 60,
    }
}

fn process_rule() -> Rule {
    Rule::Process {
        names: vec!["steam".into()],
        match_kind: "any".into(),
        match_on: "name".into(),
    }
}

fn time_rule() -> Rule {
    Rule::Time {
        days: vec![1, 2, 3, 4, 5, 6, 7],
        start: 22 * 60,
        end: 7 * 60,
    }
}

fn interval_rule() -> Rule {
    Rule::Interval {
        interval_s: 600,
        delay_s: 60,
        duration_s: 120,
    }
}

/// The installation of the model tests: four Profiles, the default one active.
fn base() -> AppState {
    let mut state = full();
    let snapshot = &mut state.snapshot;
    snapshot.profiles = vec![
        profile("profile_default", "Default Profile", 1),
        profile("profile_2", "Quiet", 2),
        profile("profile_3", "Gaming", 2),
        ProfileInfo {
            contains_release: true,
            ..profile("profile_4", "Bench", 3)
        },
    ];
    snapshot.active_profile = Some(snapshot.profiles[0].clone());
    snapshot.cause = Cause::Default;
    snapshot.rule_delay_s = 30;
    snapshot.dwell_s = 300;
    snapshot.profile_state = "applied".into();
    state
}

fn select(state: &AppState, id: &str) -> ProfilesUi {
    let _ = state;
    let mut ui = ProfilesUi::default();
    ui.select(id);
    ui
}

#[test]
fn profiles_states() {
    let ui = ProfilesUi::default();

    let mut loading = base();
    loading.link = Link::Connecting;
    assert_eq!(model(&loading, &ui).state, ProfilesState::Loading);

    // The Default Profile is active when no group holds. Rows, captions and the tag.
    let state = base();
    let ready = model(&state, &ui);
    assert_eq!(ready.state, ProfilesState::Ready);
    assert_eq!(ready.rows.len(), 4);
    assert_eq!(ready.rows[0].caption, "Active when no group holds");
    assert!(ready.rows[0].active && ready.rows[0].selected);
    assert_eq!(ready.rows[1].caption, "No group. Slot 2");
    assert_eq!(ready.rows[3].caption, "No group. Slot 3. Mode RELEASE");
    assert!(ready.rows[3].release);
    assert_eq!(
        ready.groups.empty.as_deref(),
        Some("No group. The Default Profile is active until you activate another Profile by hand.")
    );
    assert_eq!(
        ready.groups.caption,
        "A group holds when all its rules hold. The first group that holds wins. Drag to change the order."
    );
    let detail = ready.detail.as_ref().unwrap();
    assert_eq!(detail.activate_label, "Activate manually");
    assert!(detail.tag.is_none());

    // A group: its number in the caption, a group without rules, a rule sentence.
    let mut grouped = base();
    grouped.snapshot.rule_groups = vec![
        group("group_1", "profile_2", vec![sensor_rule(), process_rule()]),
        group("group_2", "profile_3", vec![]),
    ];
    let with_groups = model(&grouped, &ui);
    assert_eq!(with_groups.rows[1].caption, "Group 1");
    assert_eq!(
        with_groups.groups.groups[0].rules,
        [
            "Sensor Water temperature more than 38.0 °C for 1 min",
            "Process steam runs"
        ]
    );
    assert_eq!(with_groups.groups.groups[0].target, "Quiet");
    assert_eq!(
        with_groups.groups.groups[1].warning.as_deref(),
        Some("No rule: this group never holds.")
    );

    // The Rule delay runs: the group holds for 12 s of 30 s.
    let mut waiting = grouped.clone();
    waiting.snapshot.rule_groups[0].holds = true;
    waiting.snapshot.rule_groups[0].since_us = NOW_US;
    let waiting_model = ProfilesModel::build(
        &input(&waiting, &Feedback::default(), NOW_US + 12_000_000),
        &ui,
    );
    assert_eq!(
        waiting_model.groups.groups[0].tag.as_deref(),
        Some("Holds, activates in 18 s")
    );

    // A rule activated the Profile: the caption names the group, the tag says "by group".
    let mut by_rule = grouped.clone();
    by_rule.snapshot.rule_groups[0].holds = true;
    by_rule.snapshot.rule_groups[0].since_us = NOW_US;
    by_rule.snapshot.active_profile = Some(by_rule.snapshot.profiles[1].clone());
    by_rule.snapshot.cause = Cause::Rule;
    let activated = model(&by_rule, &ui);
    assert_eq!(activated.rows[1].caption, "Active by group 1");
    assert!(
        activated.groups.groups[0]
            .tag
            .as_deref()
            .unwrap()
            .starts_with("Holds since")
    );
    assert_eq!(
        model(&by_rule, &select(&by_rule, "profile_2"))
            .detail
            .unwrap()
            .tag
            .as_deref(),
        Some("Active by group")
    );

    // A manual activation: the tag, "Revoke", and the information row of the groups card.
    let mut manual = grouped.clone();
    manual.snapshot.active_profile = Some(manual.snapshot.profiles[1].clone());
    manual.snapshot.cause = Cause::Manual;
    let held = model(&manual, &select(&manual, "profile_2"));
    let detail = held.detail.as_ref().unwrap();
    assert_eq!(detail.tag.as_deref(), Some("Active manually"));
    assert_eq!(detail.activate_label, "Revoke manual activation");
    assert!(detail.revoke);
    assert_eq!(
        held.groups.manual.as_deref(),
        Some("Rule groups wait until you revoke the manual activation.")
    );

    // Immediate switching: the timing options are off.
    let mut immediate = base();
    immediate.snapshot.immediate_switching = true;
    let options = model(&immediate, &ui).options;
    assert!(!options.timing_enabled && options.immediate);
    assert_eq!(
        options.timing_caption.as_deref(),
        Some("Not used with Immediate switching")
    );
    assert_eq!(options.rule_delay_s, 30);
    assert_eq!(options.dwell_min, 5);

    // RELEASE: a warning row on the Profile.
    let bench = model(&base(), &select(&base(), "profile_4"));
    let notices = &bench.detail.as_ref().unwrap().notices;
    assert_eq!(notices[0].kind, NoticeKind::Warning);
    assert_eq!(
        notices[0].text,
        "This Profile sets Mode RELEASE: the Loop has no leak protection while it is active."
    );

    // The window of an Interval rule is shorter than the Rule delay.
    let mut short = base();
    let mut ui = ProfilesUi::default();
    ui.add_group(&short);
    ui.add_rule(&short);
    ui.set_rule_kind(&short, RuleKind::Interval);
    set_rule_field(ui.rule_draft().unwrap(), "duration_s", "10");
    ui.save_rule();
    assert_eq!(
        model(&short, &ui).groups.editor.unwrap().warning.as_deref(),
        Some("The window is shorter than the Rule delay. The group never activates its Profile.")
    );
    short.snapshot.immediate_switching = true;
    assert!(model(&short, &ui).groups.editor.unwrap().warning.is_none());

    // The daemon is offline: no write control works.
    let mut offline = base();
    offline.link = Link::Offline;
    let offline_model = model(&offline, &ProfilesUi::default());
    assert!(!offline_model.writes_enabled);
    assert!(!offline_model.detail.unwrap().slot.enabled);
}

#[tokio::test(flavor = "multi_thread")]
async fn create_activate_return_to_automatic() {
    let bus = PrivateBus::start("profiles-activate");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Profiles are known", |state| {
            state.snapshot.profiles.len() == 2
        })
        .await;
    let feedback = Feedback::default();
    let ui = ProfilesUi::default();

    // "New" makes a Profile that has the slot of the active Profile.
    let command = ui.new_profile(&input(&harness.state, &feedback, NOW_US));
    assert_eq!(
        command,
        Command::CreateProfile {
            name: "Profile 3".into(),
            copy_from: String::new()
        }
    );
    command
        .run_on_daemon(&harness.client)
        .await
        .expect("create");
    harness
        .until("the new Profile shows", |state| {
            state.snapshot.profiles.len() == 3
        })
        .await;
    let new = harness
        .state
        .snapshot
        .profiles
        .iter()
        .find(|p| p.name == "Profile 3")
        .expect("the new Profile")
        .clone();
    assert_eq!(new.slot, 1, "the active Profile has slot 1");

    // Activate it by hand, then give the choice back to the rules.
    let mut ui = ProfilesUi::default();
    ui.select(&new.id);
    ui.activate(&harness.state)
        .expect("a command")
        .run_on_daemon(&harness.client)
        .await
        .expect("activate");
    harness
        .until("the manual activation shows", |state| {
            state.snapshot.cause == Cause::Manual
                && state
                    .snapshot
                    .active_profile
                    .as_ref()
                    .map(|p| p.id.as_str())
                    == Some(new.id.as_str())
        })
        .await;
    Command::ReturnToAutomatic
        .run_on_daemon(&harness.client)
        .await
        .expect("return");
    harness
        .until("the rules decide again", |state| {
            state.snapshot.cause == Cause::Default
        })
        .await;
    let calls = daemon.calls();
    assert_eq!(
        calls[0],
        "CreateProfile(Profile 3,profile_default) interactive=true"
    );
    assert_eq!(
        calls[1],
        format!("ActivateProfile({}) interactive=true", new.id)
    );
    assert_eq!(calls[2], "ReturnToAutomatic() interactive=true");

    // The Profile content reaches the model: Curves and the Leakshield settings of the slot.
    let content = &harness.state.snapshot.profile_contents["profile_2"];
    assert_eq!(content.channels["nct6798_pwm6"].points.len(), 3);
    assert_eq!(
        content.channels["nct6798_pwm6"].input.as_deref(),
        Some("nct6798_temp1")
    );
    assert_eq!(content.channels["nct6798_pwm2"].fixed_duty, 30.0);
    assert_eq!(
        content.leakshield[LEAKSHIELD]["default_mode"],
        FieldValue::Text("shield".into())
    );
    let curves = model(&harness.state, &ProfilesUi::default());
    let detail = curves.detail.unwrap();
    assert_eq!(detail.curves[0].summary, "Fixed Duty 30 %");
    assert_eq!(detail.curves[1].summary, "Curve, 3 points");
}

#[tokio::test(flavor = "multi_thread")]
async fn immediate_switching_toggle_calls_api() {
    let bus = PrivateBus::start("profiles-immediate");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Profiles are known", |state| {
            !state.snapshot.profiles.is_empty()
        })
        .await;

    // Turning it on opens the warning. The command waits for "Turn on".
    let mut ui = ProfilesUi::default();
    assert!(ui.toggle_immediate(&harness.state, false).is_none());
    let dialog = model(&harness.state, &ui)
        .immediate_dialog
        .expect("the warning");
    assert_eq!(dialog.title, "Turn on Immediate switching?");
    assert_eq!(dialog.checkbox.as_deref(), Some("Do not show this again"));
    assert_eq!(
        (dialog.cancel.as_str(), dialog.confirm.as_str()),
        ("Cancel", "Turn on")
    );
    assert!(daemon.calls().is_empty(), "Cancel sends nothing");
    let (command, hide) = ui.confirm_immediate().expect("the user confirms");
    assert!(!hide);
    command.run_on_daemon(&harness.client).await.expect("on");
    harness
        .until("Immediate switching is on", |state| {
            state.snapshot.immediate_switching
        })
        .await;
    assert_eq!(
        daemon.calls()[0],
        "SetImmediateSwitching(true) interactive=true"
    );

    // Turning it off needs no warning.
    ui.toggle_immediate(&harness.state, false)
        .expect("a command")
        .run_on_daemon(&harness.client)
        .await
        .expect("off");
    harness
        .until("Immediate switching is off", |state| {
            !state.snapshot.immediate_switching
        })
        .await;

    // A user who hid the warning turns it on at once.
    assert!(ui.toggle_immediate(&harness.state, true).is_some());
    assert!(ui.immediate_dialog.is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn rule_timing_calls_set_rule_timing() {
    let bus = PrivateBus::start("profiles-timing");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the timing is known", |state| {
            state.snapshot.rule_delay_s == 30
        })
        .await;
    assert_eq!(harness.state.snapshot.dwell_s, 300);
    let ui = ProfilesUi::default();
    let command = ui.set_timing(&harness.state, Some(45), None);
    assert_eq!(
        command,
        Command::SetRuleTiming {
            rule_delay_s: 45,
            dwell_s: 300
        }
    );
    command
        .run_on_daemon(&harness.client)
        .await
        .expect("timing");
    harness
        .until("the delay changes", |state| {
            state.snapshot.rule_delay_s == 45
        })
        .await;
    ui.set_timing(&harness.state, None, Some(10))
        .run_on_daemon(&harness.client)
        .await
        .expect("dwell");
    harness
        .until("the daemon holds the timing", |state| {
            state.snapshot.rule_delay_s == 45 && state.snapshot.dwell_s == 600
        })
        .await;
    assert_eq!(daemon.calls()[0], "SetRuleTiming(45,300) interactive=true");
    assert_eq!(daemon.calls()[1], "SetRuleTiming(45,600) interactive=true");
}

#[test]
fn default_profile_has_no_delete_control() {
    let state = base();
    let mut ui = ProfilesUi::default();
    let detail = model(&state, &ui).detail.unwrap();
    assert_eq!(detail.name, "Default Profile");
    assert!(!detail.delete_enabled);
    assert_eq!(
        detail.delete_tooltip.as_deref(),
        Some("The Default Profile cannot be deleted.")
    );
    ui.delete_dialog = true;
    assert!(
        ui.confirm_delete(&state).is_none(),
        "no command for the Default Profile"
    );
    // Rename and Duplicate work: the labels are there and the Profile can be copied.
    let feedback = Feedback::default();
    assert!(ui.duplicate(&input(&state, &feedback, NOW_US)).is_some());

    // Another Profile has the delete control, with the groups that go with it.
    let mut grouped = base();
    grouped.snapshot.rule_groups = vec![
        group("group_1", "profile_3", vec![sensor_rule()]),
        group("group_2", "profile_2", vec![process_rule()]),
    ];
    let mut ui = select(&grouped, "profile_3");
    ui.delete_dialog = true;
    let dialog = model(&grouped, &ui).detail.unwrap().delete_dialog.unwrap();
    assert_eq!(dialog.title, "Delete the Profile 'Gaming'?");
    assert_eq!(
        dialog.text,
        "Delete the Profile 'Gaming'? Group 1 uses it and is deleted too. The Leakshield settings of its slot stay."
    );
    assert_eq!(dialog.confirm, "Delete Profile");
    match ui.confirm_delete(&grouped).expect("a command") {
        Command::DeleteProfile { profile, groups } => {
            assert_eq!(profile, "profile_3");
            let kept = groups.expect("the groups go with it");
            assert_eq!(kept.len(), 1);
            assert_eq!(kept[0].id, "group_2");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn default_profile_slot_select_disabled() {
    let state = base();
    let mut ui = ProfilesUi::default();
    let slot = model(&state, &ui).detail.unwrap().slot;
    assert_eq!(slot.value, 1);
    assert!(!slot.enabled);
    assert_eq!(slot.options, [1, 2, 3, 4]);
    assert!(ui.request_slot(&state, 3).is_none());
    assert!(ui.slot_dialog.is_none());
    let other = model(&state, &select(&state, "profile_2"))
        .detail
        .unwrap()
        .slot;
    assert!(other.enabled);
}

#[tokio::test(flavor = "multi_thread")]
async fn slot_select_calls_set_slot() {
    let bus = PrivateBus::start("profiles-slot");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Profiles are known", |state| {
            state.snapshot.profiles.len() == 2
        })
        .await;

    // Another Profile changes its slot without a dialog.
    let mut ui = ProfilesUi::default();
    ui.select("profile_2");
    let command = ui.request_slot(&harness.state, 3).expect("a command");
    assert_eq!(
        command,
        Command::SetProfileSlot {
            profile: "profile_2".into(),
            slot: 3
        }
    );
    command.run_on_daemon(&harness.client).await.expect("slot");
    harness
        .until("the slot changes", |state| {
            state
                .snapshot
                .profiles
                .iter()
                .any(|p| p.id == "profile_2" && p.slot == 3)
        })
        .await;
    assert_eq!(
        daemon.calls()[0],
        "SetProfileSlot(profile_2,3) interactive=true"
    );
    assert!(
        ui.request_slot(&harness.state, 3).is_none(),
        "the same slot sends nothing"
    );

    // A slot outside 1 to 4 is not offered and not sent.
    assert!(ui.request_slot(&harness.state, 5).is_none());

    // The active Profile asks first.
    let mut active = harness.state.clone();
    active.snapshot.active_profile = active
        .snapshot
        .profiles
        .iter()
        .find(|p| p.id == "profile_2")
        .cloned();
    assert!(ui.request_slot(&active, 4).is_none());
    assert_eq!(ui.slot_dialog, Some(4));
    let dialog = model(&active, &ui).detail.unwrap().slot_dialog.unwrap();
    assert_eq!(dialog.title, "Change to slot 4?");
    assert_eq!(
        dialog.text,
        "Slot 4 has other Leakshield settings. They are written to the Leakshield now."
    );
    assert_eq!(
        (dialog.cancel.as_str(), dialog.confirm.as_str()),
        ("Cancel", "Change slot")
    );
    let command = ui.confirm_slot(&active).expect("the user confirms");
    command
        .run_on_daemon(&harness.client)
        .await
        .expect("slot 4");
    harness
        .until("the slot is 4", |state| {
            state
                .snapshot
                .profiles
                .iter()
                .any(|p| p.id == "profile_2" && p.slot == 4)
        })
        .await;

    // The Default Profile keeps slot 1: the daemon refuses a change.
    let refused = Command::SetProfileSlot {
        profile: "profile_default".into(),
        slot: 2,
    }
    .run_on_daemon(&harness.client)
    .await;
    assert!(refused.is_err());
}

#[test]
fn shared_slot_notice_lists_profiles() {
    let state = base();
    let quiet = model(&state, &select(&state, "profile_2")).detail.unwrap();
    assert_eq!(quiet.slot.shared.as_deref(), Some("Shared with Gaming"));
    assert!(quiet.notices.iter().any(|n| n.kind == NoticeKind::Info
        && n.text == "Slot 2 is shared with Gaming. A Leakshield change here changes both."));
    // The Default Profile and a Profile alone on its slot show no notice.
    let default = model(&state, &ProfilesUi::default()).detail.unwrap();
    assert!(default.slot.shared.is_none());
    assert!(default.notices.is_empty());

    // Three Profiles on one slot: the notice lists the two others.
    let mut crowd = base();
    crowd.snapshot.profiles[3].slot = 2;
    let detail = model(&crowd, &select(&crowd, "profile_2")).detail.unwrap();
    assert_eq!(
        detail.slot.shared.as_deref(),
        Some("Shared with Gaming, Bench")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn duplicate_keeps_slot() {
    let bus = PrivateBus::start("profiles-duplicate");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Profiles are known", |state| {
            state.snapshot.profiles.len() == 2
        })
        .await;
    let mut ui = ProfilesUi::default();
    ui.select("profile_2");
    let feedback = Feedback::default();
    let command = ui
        .duplicate(&input(&harness.state, &feedback, NOW_US))
        .expect("a command");
    assert_eq!(
        command,
        Command::CreateProfile {
            name: "Quiet copy".into(),
            copy_from: "profile_2".into()
        }
    );
    command.run_on_daemon(&harness.client).await.expect("copy");
    harness
        .until("the copy shows", |state| {
            state
                .snapshot
                .profiles
                .iter()
                .any(|p| p.name == "Quiet copy")
        })
        .await;
    let copy = harness
        .state
        .snapshot
        .profiles
        .iter()
        .find(|p| p.name == "Quiet copy")
        .unwrap()
        .clone();
    assert_eq!(copy.slot, 2, "the copy keeps the slot of Quiet");
    assert_eq!(
        daemon.calls()[0],
        "CreateProfile(Quiet copy,profile_2) interactive=true"
    );

    // The rename popover of the copy opens at once.
    ui.sync(&harness.state);
    assert_eq!(ui.rename, Some((copy.id.clone(), "Quiet copy".into())));
    assert_eq!(ui.selected.as_deref(), Some(copy.id.as_str()));
}

#[tokio::test(flavor = "multi_thread")]
async fn new_profile_has_no_limit() {
    let bus = PrivateBus::start("profiles-limit");
    let _daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Profiles are known", |state| {
            state.snapshot.profiles.len() == 2
        })
        .await;
    let feedback = Feedback::default();
    let ui = ProfilesUi::default();
    for expected in 3..=12 {
        ui.new_profile(&input(&harness.state, &feedback, NOW_US))
            .run_on_daemon(&harness.client)
            .await
            .expect("create");
        harness
            .until("the Profile shows", |state| {
                state.snapshot.profiles.len() == expected
            })
            .await;
    }
    let ready = model(&harness.state, &ui);
    assert_eq!(ready.rows.len(), 12);
    assert!(ready.writes_enabled, "\"New\" works at 12 Profiles");
    let mut names: Vec<_> = ready.rows.iter().map(|r| r.name.clone()).collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 12, "every name is free");
}

#[tokio::test(flavor = "multi_thread")]
async fn rule_group_editor_calls_set_rule_groups() {
    let bus = PrivateBus::start("profiles-groups");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Profiles are known", |state| {
            state.snapshot.profiles.len() == 2
        })
        .await;
    assert!(harness.state.snapshot.rule_groups.is_empty());

    let mut ui = ProfilesUi::default();
    ui.add_group(&harness.state);
    ui.set_group_target("profile_2");
    // A Sensor rule, set through the fields of the editor.
    ui.add_rule(&harness.state);
    let draft = ui.rule_draft().unwrap();
    set_rule_field(draft, "value", "40");
    set_rule_field(draft, "hold_s", "30");
    ui.save_rule();
    // A Process rule.
    ui.add_rule(&harness.state);
    ui.set_rule_kind(&harness.state, RuleKind::Process);
    set_rule_field(ui.rule_draft().unwrap(), "names", "steam, lutris");
    ui.save_rule();
    // A Time window.
    ui.add_rule(&harness.state);
    ui.set_rule_kind(&harness.state, RuleKind::Time);
    set_rule_field(ui.rule_draft().unwrap(), "start", "21:30");
    ui.save_rule();
    // An Interval.
    ui.add_rule(&harness.state);
    ui.set_rule_kind(&harness.state, RuleKind::Interval);
    ui.save_rule();

    // A Process rule needs a name: the empty draft stays open.
    ui.add_rule(&harness.state);
    ui.set_rule_kind(&harness.state, RuleKind::Process);
    ui.save_rule();
    let editor = model(&harness.state, &ui).groups.editor.unwrap();
    assert_eq!(editor.rules.len(), 4);
    assert!(
        !editor.rule_editor.as_ref().unwrap().complete,
        "no name yet"
    );
    ui.cancel_rule();

    let command = ui.save_group(&harness.state).expect("a command");
    command
        .run_on_daemon(&harness.client)
        .await
        .expect("groups");
    harness
        .until("the group shows", |state| {
            state.snapshot.rule_groups.len() == 1
        })
        .await;
    let stored = &harness.state.snapshot.rule_groups[0];
    assert_eq!(stored.id, "group_1");
    assert_eq!(stored.profile, "profile_2");
    assert!(stored.enabled);
    assert_eq!(
        stored.rules,
        [
            Rule::Sensor {
                sensor: "nct6798_temp1".into(),
                comparison: "gt".into(),
                value: 40.0,
                value_max: 0.0,
                hold_s: 30,
            },
            Rule::Process {
                names: vec!["steam".into(), "lutris".into()],
                match_kind: "any".into(),
                match_on: "name".into(),
            },
            Rule::Time {
                days: vec![1, 2, 3, 4, 5, 6, 7],
                start: 21 * 60 + 30,
                end: 7 * 60,
            },
            interval_rule(),
        ],
        "one group, one rule of each kind, read back from the daemon"
    );
    assert_eq!(
        daemon.calls()[0],
        "SetRuleGroups(>profile_2>true>[sensor:comparison+hold_s+sensor+value,process:match+match_on+names,time:days+end+start,interval:delay_s+duration_s+interval_s]) interactive=true"
    );

    // The sentences of the list.
    let shown = model(&harness.state, &ProfilesUi::default());
    assert_eq!(
        shown.groups.groups[0].rules,
        [
            "Sensor Water temperature more than 40.0 °C for 30 s",
            "Process steam, lutris runs",
            "Time window 21:30 to 07:00",
            "Every 10 min, from 1 min after the start, for 2 min",
        ]
    );

    // Edit the group: disable it and delete a rule. Delete the group at the end.
    let mut ui = ProfilesUi::default();
    ui.open_group(&harness.state, 0);
    ui.set_group_enabled(false);
    ui.remove_rule(3);
    ui.save_group(&harness.state)
        .expect("a command")
        .run_on_daemon(&harness.client)
        .await
        .expect("groups");
    harness
        .until("the edit shows", |state| {
            state.snapshot.rule_groups[0].rules.len() == 3 && !state.snapshot.rule_groups[0].enabled
        })
        .await;
    ui.open_group(&harness.state, 0);
    ui.delete_group(&harness.state)
        .expect("a command")
        .run_on_daemon(&harness.client)
        .await
        .expect("groups");
    harness
        .until("the group is gone", |state| {
            state.snapshot.rule_groups.is_empty()
        })
        .await;
}

#[test]
fn rule_kind_fields_follow_api_keys() {
    let state = base();
    // docs/api.md, section Rule groups: the keys of each kind.
    let keys = |rule: &Rule| -> Vec<String> {
        let mut keys: Vec<String> = rule_params(rule).1.keys().cloned().collect();
        keys.sort();
        keys
    };
    let sorted = |list: Vec<&str>| -> Vec<String> {
        let mut list: Vec<String> = list.into_iter().map(str::to_owned).collect();
        list.sort();
        list
    };
    let in_range = Rule::Sensor {
        sensor: "nct6798_temp1".into(),
        comparison: "in_range".into(),
        value: 30.0,
        value_max: 40.0,
        hold_s: 0,
    };
    let cases: [(Rule, &str, Vec<&str>); 5] = [
        (
            sensor_rule(),
            "sensor",
            vec!["sensor", "comparison", "value", "hold_s"],
        ),
        (
            in_range,
            "sensor",
            vec!["sensor", "comparison", "value", "value_max", "hold_s"],
        ),
        (
            process_rule(),
            "process",
            vec!["names", "match", "match_on"],
        ),
        (time_rule(), "time", vec!["days", "start", "end"]),
        (
            interval_rule(),
            "interval",
            vec!["interval_s", "delay_s", "duration_s"],
        ),
    ];
    for (rule, kind, expected) in cases {
        assert_eq!(rule_params(&rule).0, kind);
        assert_eq!(
            keys(&rule),
            sorted(expected.clone()),
            "the wire keys of {kind}"
        );
        assert_eq!(
            sorted(rule_keys(&rule)),
            sorted(expected),
            "the keys that the editor shows for {kind}"
        );
    }

    // The editor starts from the defaults of docs/ui/screens/profiles.md.
    assert_eq!(RuleKind::Interval.fresh(&state), interval_rule());
    assert_eq!(RuleKind::Time.fresh(&state), time_rule());

    // The sentences of the table of the screen doc.
    assert_eq!(
        rule_sentence(EN, &state, &time_rule()),
        "Time window 22:00 to 07:00"
    );
    let weekend = Rule::Time {
        days: vec![6, 7],
        start: 0,
        end: 60,
    };
    assert_eq!(
        rule_sentence(EN, &state, &weekend),
        "Time window 00:00 to 01:00, Sat Sun"
    );
    assert_eq!(
        rule_sentence(IT, &state, &process_rule()),
        "Il processo steam è in esecuzione"
    );
    let all = Rule::Process {
        names: vec!["a".into(), "b".into()],
        match_kind: "none".into(),
        match_on: "cmdline".into(),
    };
    assert_eq!(rule_sentence(EN, &state, &all), "None of a, b runs");

    // A wrong text keeps the old value.
    let mut rule = sensor_rule();
    set_rule_field(&mut rule, "value", "abc");
    assert_eq!(rule, sensor_rule());
    set_rule_field(&mut rule, "value", "41,5");
    assert!(matches!(rule, Rule::Sensor { value, .. } if value == 41.5));
}

#[tokio::test(flavor = "multi_thread")]
async fn group_drag_reorders_and_calls_set_rule_groups() {
    let bus = PrivateBus::start("profiles-order");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Profiles are known", |state| {
            state.snapshot.profiles.len() == 2
        })
        .await;
    Command::SetRuleGroups {
        groups: vec![
            group("", "profile_2", vec![sensor_rule()]),
            group("", "profile_default", vec![process_rule()]),
            group("", "profile_2", vec![time_rule()]),
        ],
    }
    .run_on_daemon(&harness.client)
    .await
    .expect("three groups");
    harness
        .until("three groups", |state| {
            state.snapshot.rule_groups.len() == 3
        })
        .await;
    let order = |state: &AppState| -> Vec<String> {
        state
            .snapshot
            .rule_groups
            .iter()
            .map(|g| g.id.clone())
            .collect()
    };
    assert_eq!(order(&harness.state), ["group_1", "group_2", "group_3"]);

    // Drag the first group to the end. Alt+Down does the same one step at a time.
    let mut ui = ProfilesUi::default();
    let command = ui.move_group(&harness.state, 0, 2).expect("a command");
    command.run_on_daemon(&harness.client).await.expect("order");
    harness
        .until("the order changes", |state| {
            state.snapshot.rule_groups[0].id == "group_2"
        })
        .await;
    assert_eq!(order(&harness.state), ["group_2", "group_3", "group_1"]);
    let last = daemon.calls().last().cloned().unwrap();
    assert!(
        last.starts_with("SetRuleGroups(group_2>profile_default>true>[process:match+match_on+names];group_3>profile_2>true>[time:days+end+start];group_1>profile_2>true>[sensor:"),
        "{last}"
    );
    ui.move_group(&harness.state, 1, 0)
        .expect("a command")
        .run_on_daemon(&harness.client)
        .await
        .expect("order");
    harness
        .until("the order changes again", |state| {
            state.snapshot.rule_groups[0].id == "group_3"
        })
        .await;

    // A move outside the list, or onto itself, sends nothing.
    assert!(ui.move_group(&harness.state, 0, 3).is_none());
    assert!(ui.move_group(&harness.state, 1, 1).is_none());
    assert!(ui.move_group(&harness.state, 5, 0).is_none());
}

#[test]
fn no_hotkey_rule_kind_offered() {
    let ids: Vec<&str> = RULE_KINDS.iter().map(|kind| kind.id()).collect();
    assert_eq!(ids, ["sensor", "process", "time", "interval"]);
    let state = base();
    let mut ui = ProfilesUi::default();
    ui.add_group(&state);
    ui.add_rule(&state);
    let editor = model(&state, &ui).groups.editor.unwrap();
    let kinds: Vec<&str> = editor.kinds.iter().map(|kind| kind.id()).collect();
    assert_eq!(kinds, ids);
    assert!(!ids.contains(&"hotkey"));
    // The localized names of the kinds do not mention a hotkey or a shortcut either.
    for locale in [labels::load("en"), labels::load("it")] {
        for kind in ids.iter() {
            let text = locale[&format!("profiles.kind.{kind}")].to_lowercase();
            assert!(!text.contains("hotkey") && !text.contains("scorciatoia"));
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn curves_applied_leakshield_failed_shows_restore_offer() {
    // The model: the Curves are active and the write to the Leakshield failed.
    let mut state = base();
    state.snapshot.profile_state = "leakshield-failed".into();
    state.write_failure = Some(WriteFailure {
        leakshield: LEAKSHIELD.into(),
        error: "VerifyFailed".into(),
        backup: "20261123T010000Z-abc123".into(),
    });
    let detail = model(&state, &ProfilesUi::default()).detail.unwrap();
    let offer = detail.restore.expect("the restore offer");
    assert_eq!(
        offer.text,
        "The Curves are active. The Leakshield write failed."
    );
    assert_eq!(offer.button, "Restore the backup");
    assert!(offer.locked, "the restore needs the administrator password");
    assert_eq!(
        offer.command,
        Some(Command::RestoreBackup {
            leakshield: LEAKSHIELD.into(),
            backup: "20261123T010000Z-abc123".into()
        })
    );
    // A Profile that is not active shows no offer, and an applied state shows none.
    assert!(
        model(&state, &select(&state, "profile_2"))
            .detail
            .unwrap()
            .restore
            .is_none()
    );
    state.snapshot.profile_state = "applied".into();
    assert!(
        model(&state, &ProfilesUi::default())
            .detail
            .unwrap()
            .restore
            .is_none()
    );

    // The daemon: `WriteFailed` carries the backup, and the button runs `RestoreBackup`.
    let bus = PrivateBus::start("profiles-restore");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Profiles are known", |state| {
            !state.snapshot.profiles.is_empty()
        })
        .await;
    daemon.emit_write_failed("VerifyFailed", "backup_7").await;
    harness
        .until("the failure is known", |state| {
            state.write_failure.is_some()
        })
        .await;
    let failure = harness.state.write_failure.clone().unwrap();
    assert_eq!(failure.backup, "backup_7");
    assert_eq!(failure.leakshield, LEAKSHIELD);
    harness.state.snapshot.profile_state = "leakshield-failed".into();
    let offer = model(&harness.state, &ProfilesUi::default())
        .detail
        .unwrap()
        .restore
        .expect("the offer");
    offer
        .command
        .expect("a command")
        .run_on_daemon(&harness.client)
        .await
        .expect("restore");
    assert_eq!(
        daemon.calls()[0],
        format!("RestoreBackup({LEAKSHIELD},backup_7) interactive=true")
    );
}

#[test]
fn leakshield_rows_and_differences() {
    let mut state = base();
    let content = |warning: f64| {
        let mut fields = std::collections::BTreeMap::new();
        fields.insert("default_mode".to_owned(), FieldValue::Text("shield".into()));
        fields.insert(
            "display_brightness_active".to_owned(),
            FieldValue::Number(3.0),
        );
        fields.insert(
            "display_brightness_idle".to_owned(),
            FieldValue::Number(1.0),
        );
        fields.insert("led_strip_brightness".to_owned(), FieldValue::Number(60.0));
        fields.insert(
            "fill_level_warning_threshold".to_owned(),
            FieldValue::Number(warning),
        );
        fields.insert(
            "fill_level_alarm_threshold".to_owned(),
            FieldValue::Number(15.0),
        );
        fields.insert(
            "leak_rate_warning_threshold".to_owned(),
            FieldValue::Number(100.0),
        );
        ProfileContent {
            leakshield: [(LEAKSHIELD.to_owned(), fields)].into(),
            ..Default::default()
        }
    };
    state
        .snapshot
        .profile_contents
        .insert("profile_default".into(), content(30.0));
    state
        .snapshot
        .profile_contents
        .insert("profile_2".into(), content(25.0));
    state
        .snapshot
        .profile_contents
        .insert("profile_4".into(), content(30.0));
    let detail = model(&state, &ProfilesUi::default()).detail.unwrap();
    let rows: Vec<(&str, &str)> = detail
        .leakshield_rows
        .iter()
        .map(|(a, b)| (a.as_str(), b.as_str()))
        .collect();
    assert_eq!(
        rows,
        [
            ("Mode", "SHIELD"),
            ("Display", "Brightness 3, idle 1"),
            ("Lighting", "Brightness 60 %"),
            (
                "Alarm thresholds",
                "Fill level 30 % and 15 %, leak 100 ml/h"
            ),
        ]
    );
    // Slot 2 differs from slot 1: the Default Profile warns once for that slot.
    assert!(
        detail
            .notices
            .iter()
            .any(|n| n.text == "Alarm thresholds differ from 1 other slots")
    );
    // An external change on the display shows an information row.
    state.external_change = Some(vec!["display_brightness_active".into()]);
    let detail = model(&state, &ProfilesUi::default()).detail.unwrap();
    assert!(detail.notices.iter().any(|n| n.text
        == "Display brightness changed outside nicecooling. Activating a Profile with another slot overwrites it."));
}

#[test]
fn profiles_labels() {
    let en = labels::load("en");
    let it = labels::load("it");
    // The copy table of docs/ui/screens/profiles.md.
    let table = [
        ("profiles.new", "New", "Nuovo"),
        ("profiles.groups.add", "Add group", "Aggiungi gruppo"),
        ("profiles.activate", "Activate manually", "Attiva a mano"),
        (
            "profiles.revoke",
            "Revoke manual activation",
            "Revoca l'attivazione manuale",
        ),
        (
            "profiles.options.rule_delay",
            "Rule delay",
            "Ritardo regola",
        ),
        (
            "profiles.options.dwell",
            "Profile dwell",
            "Permanenza profilo",
        ),
        (
            "profiles.options.immediate",
            "Immediate switching",
            "Cambio immediato",
        ),
        (
            "profiles.editor.activate",
            "Activate Profile",
            "Attiva il Profilo",
        ),
        ("profiles.editor.enabled", "Enabled", "Attivo"),
        ("profiles.editor.add_rule", "Add rule", "Aggiungi regola"),
        ("profiles.editor.remove", "Remove", "Rimuovi"),
        ("profiles.editor.delete", "Delete group", "Elimina gruppo"),
        ("profiles.editor.save", "Save group", "Salva gruppo"),
        ("profiles.editor.condition", "Condition", "Condizione"),
        ("profiles.field.comparison", "Is", "È"),
        ("profiles.field.value", "Value", "Valore"),
        ("profiles.field.value_max", "Up to", "Fino a"),
        ("profiles.slot.label", "Slot", "Slot"),
        (
            "profiles.delete.confirm",
            "Delete Profile",
            "Elimina il Profilo",
        ),
        ("profiles.slot_dialog.confirm", "Change slot", "Cambia slot"),
        (
            "profiles.restore.button",
            "Restore the backup",
            "Ripristina il backup",
        ),
        (
            "profiles.delete.default_tooltip",
            "The Default Profile cannot be deleted.",
            "Il Profilo predefinito non si può eliminare.",
        ),
        (
            "profiles.options.help_delay",
            "Rule delay: a group must hold for this time before it activates a Profile.",
            "Ritardo regola: un gruppo deve restare valido per questo tempo prima di attivare un Profilo.",
        ),
        (
            "profiles.notice.release",
            "This Profile sets Mode RELEASE: the Loop has no leak protection while it is active.",
            "Questo Profilo imposta la Modalità RELEASE: il Loop non ha protezione dalle perdite mentre è attivo.",
        ),
        (
            "profiles.group.no_rule",
            "No rule: this group never holds.",
            "Nessuna regola: questo gruppo non è mai valido.",
        ),
    ];
    for (key, english, italian) in table {
        assert_eq!(en[key], english, "English text of {key}");
        assert_eq!(it[key], italian, "Italian text of {key}");
    }
    for key in en.keys().filter(|key| key.starts_with("profiles.")) {
        assert!(it.contains_key(key), "Italian text of {key}");
    }
    // The Italian text of a rule sentence differs from the English one.
    let state = base();
    assert_ne!(
        rule_sentence(EN, &state, &sensor_rule()),
        rule_sentence(IT, &state, &sensor_rule())
    );
}
