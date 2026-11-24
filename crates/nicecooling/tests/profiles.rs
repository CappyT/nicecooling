mod support;

use nicecooling::command::Command;
use nicecooling::daemon_client::rule_params;
use nicecooling::i18n::{EN, IT};
use nicecooling::model::{
    AppState, Cause, FieldValue, LeakshieldData, Link, ProfileContent, ProfileInfo, Rule, RuleGroup,
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
    let edit = ui.save_group(&harness.state).expect("a command");
    edit.run_on_daemon(&harness.client).await.expect("groups");
    // The daemon sends no signal for an edit of the rules: the app reads the groups again.
    let refresh = edit.refresh(&harness.client).await.expect("a refresh");
    harness.state.apply(&refresh);
    let group = &harness.state.snapshot.rule_groups[0];
    assert!(group.rules.len() == 3 && !group.enabled);
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

#[test]
fn curves_applied_leakshield_failed_shows_restore_offer() {
    // The model: the Curves are active and the daemon holds the backup of the failed write.
    let mut state = base();
    state.snapshot.leakshields[0].failed_write_backup = "20261123T010000Z-abc123".into();
    let detail = model(&state, &ProfilesUi::default()).detail.unwrap();
    let offer = detail
        .restore
        .into_iter()
        .next()
        .expect("the restore offer");
    assert_eq!(
        offer.text,
        "The Curves are active. The Leakshield write failed."
    );
    assert_eq!(offer.button, "Restore the backup");
    assert_eq!(
        offer.command,
        Some(Command::RestoreBackup {
            leakshield: LEAKSHIELD.into(),
            backup: "20261123T010000Z-abc123".into()
        })
    );
    // A Profile that is not active shows no offer, and a good write shows none.
    assert!(
        model(&state, &select(&state, "profile_2"))
            .detail
            .unwrap()
            .restore
            .is_empty()
    );
    state.snapshot.leakshields[0].failed_write_backup.clear();
    assert!(
        model(&state, &ProfilesUi::default())
            .detail
            .unwrap()
            .restore
            .is_empty()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn restore_offer_from_failed_write_backup() {
    let bus = PrivateBus::start("profiles-restore");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    // The failure happened before the app started: the property holds the backup.
    daemon.set_failed_write_backup("backup_7").await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the failed backup is known", |state| {
            state
                .snapshot
                .leakshields
                .first()
                .is_some_and(|device| device.failed_write_backup == "backup_7")
        })
        .await;
    assert!(harness.state.write_failure.is_none(), "no signal was seen");
    let offer = model(&harness.state, &ProfilesUi::default())
        .detail
        .unwrap()
        .restore
        .into_iter()
        .next()
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
    // A good write clears the property, and the offer goes away.
    daemon.set_failed_write_backup("").await;
    harness
        .until("the offer goes away", |state| {
            state.snapshot.leakshields[0].failed_write_backup.is_empty()
        })
        .await;
    assert!(
        model(&harness.state, &ProfilesUi::default())
            .detail
            .unwrap()
            .restore
            .is_empty()
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
            FieldValue::Text("off".into()),
        );
        fields.insert(
            "display_brightness_idle".to_owned(),
            FieldValue::Text("mid".into()),
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
    // Slot 2 has other thresholds. Slot 3 has the same ones.
    state.snapshot.leakshield_data.insert(
        LEAKSHIELD.into(),
        stored_slots(&[(1, 30.0), (2, 25.0), (3, 30.0)]),
    );
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
            ("Display", "Brightness Off, idle Medium"),
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
        (
            "profiles.notice.slot_failed",
            "Slot %{slot}: not changed. %{reason}",
            "Slot %{slot}: non modificato. %{reason}",
        ),
        (
            "profiles.notice.apply_all",
            "Apply to all slots",
            "Applica a tutti gli slot",
        ),
        (
            "profiles.caption.dwell",
            "Stays for %{time} more (Profile dwell)",
            "Resta per altri %{time} (permanenza profilo)",
        ),
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

/// The device data with the stored thresholds of some slots: `(slot, fill level warning)`.
fn stored_slots(slots: &[(u8, f64)]) -> LeakshieldData {
    let mut data = support::fixtures::leakshield_data();
    for (slot, warning) in slots {
        data.stored.insert(
            *slot,
            [
                (
                    "fill_level_warning_threshold".to_owned(),
                    FieldValue::Number(*warning),
                ),
                (
                    "fill_level_alarm_threshold".to_owned(),
                    FieldValue::Number(15.0),
                ),
                (
                    "leak_rate_warning_threshold".to_owned(),
                    FieldValue::Number(100.0),
                ),
            ]
            .into_iter()
            .collect(),
        );
    }
    data
}

/// A state where Profile "Default Profile" has the slot 1 and the device runs the slot 1.
fn with_slots() -> AppState {
    let mut state = base();
    state.snapshot.leakshields[0].active_slot = 1;
    state.snapshot.leakshields[0].connected = true;
    state.snapshot.leakshield_data.insert(
        LEAKSHIELD.into(),
        device_like(stored_slots(&[(1, 60.0), (2, 50.0), (4, 60.0)])),
    );
    state
}

/// The device runs the same alarm thresholds as the stored slots, except the fill level warning.
fn device_like(mut data: LeakshieldData) -> LeakshieldData {
    for field in &mut data.fields {
        match field.id.as_str() {
            "fill_level_alarm_threshold" => field.value = FieldValue::Number(15.0),
            "leak_rate_warning_threshold" => field.value = FieldValue::Number(100.0),
            _ => {}
        }
    }
    data
}

#[test]
fn dwell_caption_from_dwell_until() {
    let mut state = base();
    state.snapshot.cause = Cause::Rule;
    state.snapshot.active_profile = Some(state.snapshot.profiles[1].clone());
    state.snapshot.rule_groups = vec![group("g1", "profile_2", vec![sensor_rule()])];
    state.snapshot.rule_groups[0].holds = true;
    let ui = select(&state, "profile_2");
    let at = |state: &AppState, locale| {
        let feedback = Feedback::default();
        ProfilesModel::build(
            &ScreenInput {
                locale,
                ..input(state, &feedback, NOW_US)
            },
            &ui,
        )
    };
    // No dwell runs.
    let shown = at(&state, EN);
    assert_eq!(shown.rows[1].caption, "Active by group 1");
    // The dwell ends in 3 minutes.
    state.snapshot.dwell_until_us = NOW_US + 180_000_000;
    let shown = at(&state, EN);
    assert_eq!(
        shown.rows[1].caption,
        "Active by group 1. Stays for 3 min more (Profile dwell)"
    );
    assert_eq!(
        shown.detail.unwrap().caption,
        "Active by group 1. Stays for 3 min more (Profile dwell)"
    );
    assert_eq!(
        at(&state, IT).rows[1].caption,
        "Attivo dal gruppo 1. Resta per altri 3 min (permanenza profilo)"
    );
    // Another Profile does not show it, and a dwell in the past shows nothing.
    assert_eq!(shown.rows[2].caption, "No group. Slot 2");
    state.snapshot.dwell_until_us = NOW_US - 1;
    assert_eq!(at(&state, EN).rows[1].caption, "Active by group 1");
}

#[tokio::test(flavor = "multi_thread")]
async fn dwell_until_property_reaches_the_snapshot() {
    let bus = PrivateBus::start("profiles-dwell");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Profiles are known", |state| {
            !state.snapshot.profiles.is_empty()
        })
        .await;
    assert_eq!(harness.state.snapshot.dwell_until_us, 0);
    daemon.set_dwell_until(1_700_000_300_000_000).await;
    harness
        .until("the dwell shows", |state| {
            state.snapshot.dwell_until_us == 1_700_000_300_000_000
        })
        .await;
}

#[test]
fn diff_dialog_before_activation() {
    let state = with_slots();
    let mut ui = select(&state, "profile_2");
    // Slot 2 holds other thresholds than the device runs: the dialog opens, nothing is sent.
    assert_eq!(ui.request_activation(&state), None);
    let dialog = model(&state, &ui)
        .detail
        .unwrap()
        .diff_dialog
        .expect("the dialog");
    assert_eq!(dialog.title, "Activate the Profile 'Quiet'?");
    assert_eq!(dialog.columns, ["Field", "Leakshield now", "Slot 2"]);
    assert_eq!(dialog.rows, [["Fill level warning threshold", "60", "50"]]);
    assert_eq!(
        (dialog.cancel.as_str(), dialog.confirm.as_str()),
        ("Cancel", "Activate")
    );
    // "Activate" sends the activation and closes the dialog.
    assert_eq!(
        ui.confirm_activation(),
        Some(Command::ActivateProfile {
            id: "profile_2".into()
        })
    );
    assert!(model(&state, &ui).detail.unwrap().diff_dialog.is_none());
    // "Cancel" leaves the Profile as it is: another select closes the dialog too.
    ui.request_activation(&state);
    ui.select("profile_3");
    assert!(ui.diff_dialog.is_none());
}

#[test]
fn no_diff_dialog_for_same_slot() {
    let activate = |id: &str| Some(Command::ActivateProfile { id: id.into() });
    // The Profile has the slot of the device.
    let state = with_slots();
    let mut ui = select(&state, "profile_default");
    assert_eq!(ui.request_activation(&state), activate("profile_default"));
    // Another slot with the same values as the device: nothing to show.
    let mut same = with_slots();
    same.snapshot
        .leakshield_data
        .insert(LEAKSHIELD.into(), device_like(stored_slots(&[(2, 60.0)])));
    let mut ui = select(&same, "profile_2");
    assert_eq!(ui.request_activation(&same), activate("profile_2"));
    // The slot of the device is unknown: the app cannot compare.
    let mut unknown = with_slots();
    unknown.snapshot.leakshields[0].active_slot = 0;
    let mut ui = select(&unknown, "profile_2");
    assert_eq!(ui.request_activation(&unknown), activate("profile_2"));
    // A Profile without stored settings for its slot.
    let mut missing = with_slots();
    missing.snapshot.profiles[2].slot = 3;
    let mut ui = select(&missing, "profile_3");
    assert_eq!(ui.request_activation(&missing), activate("profile_3"));
}

#[test]
fn differs_from_other_slots_uses_stored_fields() {
    let mut state = with_slots();
    let content = {
        let mut fields = std::collections::BTreeMap::new();
        for (id, value) in [
            ("fill_level_warning_threshold", 60.0),
            ("fill_level_alarm_threshold", 15.0),
            ("leak_rate_warning_threshold", 100.0),
        ] {
            fields.insert(id.to_owned(), FieldValue::Number(value));
        }
        ProfileContent {
            leakshield: [(LEAKSHIELD.to_owned(), fields)].into(),
            ..Default::default()
        }
    };
    state
        .snapshot
        .profile_contents
        .insert("profile_default".into(), content);
    // Slots 2 and 4 are stored, and only slot 2 has other values. Slot 3 has nothing stored.
    state.snapshot.leakshield_data.insert(
        LEAKSHIELD.into(),
        stored_slots(&[(1, 60.0), (2, 50.0), (4, 60.0)]),
    );
    let notices = model(&state, &ProfilesUi::default())
        .detail
        .unwrap()
        .notices;
    let notice = notices
        .iter()
        .find(|n| n.text.starts_with("Alarm thresholds differ"))
        .expect("the warning");
    assert_eq!(notice.text, "Alarm thresholds differ from 1 other slots");
    assert_eq!(notice.kind, NoticeKind::Warning);
    let (label, commands) = notice.button.clone().expect("the button");
    assert_eq!(label, "Apply to all slots");
    // Every other slot with stored settings: slots 2 and 4. Slot 3 has none.
    assert_eq!(commands.len(), 2);
    // The other Profiles and their content play no part: no content for slot 2 is needed.
    assert!(!state.snapshot.profile_contents.contains_key("profile_2"));

    // Equal thresholds in every slot: no warning.
    state.snapshot.leakshield_data.insert(
        LEAKSHIELD.into(),
        stored_slots(&[(1, 60.0), (2, 60.0), (4, 60.0)]),
    );
    assert!(
        model(&state, &ProfilesUi::default())
            .detail
            .unwrap()
            .notices
            .iter()
            .all(|n| !n.text.starts_with("Alarm thresholds differ"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_to_all_slots_calls_set_stored_fields() {
    let bus = PrivateBus::start("profiles-apply-all");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the stored slots are known", |state| {
            state
                .snapshot
                .leakshield_data
                .get(LEAKSHIELD)
                .is_some_and(|d| !d.stored.is_empty())
                && state
                    .snapshot
                    .profile_contents
                    .contains_key("profile_default")
        })
        .await;
    // The daemon has stored slots 1, 2 and 4: slot 3 has no stored report 3.
    let data = &harness.state.snapshot.leakshield_data[LEAKSHIELD];
    assert_eq!(data.stored.keys().copied().collect::<Vec<_>>(), [1, 2, 4]);
    let ui = ProfilesUi::default();
    let commands = ui.apply_to_all_slots(&harness.state);
    let slots: Vec<u8> = commands
        .iter()
        .filter_map(|c| match c {
            Command::SetStoredFields { slot, .. } => Some(*slot),
            _ => None,
        })
        .collect();
    assert_eq!(slots, [2, 4]);
    for command in &commands {
        assert_eq!(
            command
                .feedback_key()
                .as_deref()
                .map(|k| k.starts_with("stored:")),
            Some(true)
        );
        command
            .run_on_daemon(&harness.client)
            .await
            .expect("stored");
    }
    assert_eq!(
        daemon.calls(),
        [
            "SetStoredFields(2,fill_level_alarm_threshold,fill_level_warning_threshold,leak_rate_warning_threshold) interactive=true",
            "SetStoredFields(4,fill_level_alarm_threshold,fill_level_warning_threshold,leak_rate_warning_threshold) interactive=true",
        ]
    );
    // The command reads the Leakshield again: the stored slots now match.
    let data = harness.client.leakshield_data(LEAKSHIELD).await;
    assert_eq!(
        data.stored[&2]["fill_level_warning_threshold"],
        FieldValue::Number(29.0)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn apply_to_all_slots_shows_failed_slot() {
    let bus = PrivateBus::start("profiles-apply-failed");
    let daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the stored slots are known", |state| {
            state
                .snapshot
                .leakshield_data
                .get(LEAKSHIELD)
                .is_some_and(|d| !d.stored.is_empty())
                && state.snapshot.profile_contents.contains_key("profile_2")
        })
        .await;
    // Profile "Quiet" has the slot 2. Slot 1 runs on the device: it is written with `SetFields`,
    // slot 4 with `SetStoredFields`. The administrator denies both.
    daemon.deny_writes(true);
    let ui = select(&harness.state, "profile_2");
    let commands = ui.apply_to_all_slots(&harness.state);
    let mut feedback = Feedback::default();
    for command in &commands {
        let key = command.feedback_key().expect("a key");
        match command.run_on_daemon(&harness.client).await {
            Ok(_) => feedback.set(
                key,
                nicecooling::screens::WriteOutcome::Applied { at_us: NOW_US },
            ),
            Err(error) => feedback.set(key, nicecooling::screens::WriteOutcome::from_error(&error)),
        }
    }
    let shown = ProfilesModel::build(&input(&harness.state, &feedback, NOW_US), &ui);
    let failed: Vec<&str> = shown
        .detail
        .as_ref()
        .unwrap()
        .notices
        .iter()
        .filter(|n| n.text.starts_with("Slot "))
        .map(|n| n.text.as_str())
        .collect();
    assert_eq!(
        failed,
        [
            "Slot 1: not changed. Not authorized. Nothing changed.",
            "Slot 4: not changed. Not authorized. Nothing changed."
        ]
    );
    // Italian.
    let italian = ProfilesModel::build(
        &ScreenInput {
            locale: IT,
            ..input(&harness.state, &feedback, NOW_US)
        },
        &ui,
    );
    assert!(
        italian
            .detail
            .unwrap()
            .notices
            .iter()
            .any(|n| n.text.starts_with("Slot 1: non modificato."))
    );
}

#[test]
fn active_profile_shows_rule_group() {
    let mut state = base();
    state.snapshot.cause = Cause::Rule;
    state.snapshot.active_profile = Some(state.snapshot.profiles[1].clone());
    state.snapshot.rule_groups = vec![
        group("g1", "profile_3", vec![process_rule()]),
        group("g2", "profile_2", vec![time_rule()]),
    ];
    state.snapshot.rule_groups[1].holds = true;
    let text = model(&state, &ProfilesUi::default())
        .detail
        .unwrap()
        .rule_group;
    assert_eq!(
        text.as_deref(),
        Some("Group 2: time window 22:00 to 07:00.")
    );
    // Another Profile, a manual activation and the Default Profile show no group.
    assert!(
        model(&state, &select(&state, "profile_3"))
            .detail
            .unwrap()
            .rule_group
            .is_none()
    );
    let mut manual = state.clone();
    manual.snapshot.cause = Cause::Manual;
    assert!(
        model(&manual, &ProfilesUi::default())
            .detail
            .unwrap()
            .rule_group
            .is_none()
    );
}

// Review 11 (adversarial).
const SECOND: &str = "leakshield_30000_1";

fn thresholds(warning: f64) -> std::collections::BTreeMap<String, FieldValue> {
    [
        ("fill_level_warning_threshold", warning),
        ("fill_level_alarm_threshold", 15.0),
        ("leak_rate_warning_threshold", 100.0),
    ]
    .into_iter()
    .map(|(id, value)| (id.to_owned(), FieldValue::Number(value)))
    .collect()
}

/// Two Leakshields. Only the second has a stored slot whose thresholds differ.
fn two_devices() -> AppState {
    let mut state = with_slots();
    let mut second = state.snapshot.leakshields[0].clone();
    second.id = SECOND.into();
    second.serial = "30000".into();
    state.snapshot.leakshields.push(second);
    // The first device: every stored slot equals the Profile.
    state.snapshot.leakshield_data.insert(
        LEAKSHIELD.into(),
        device_like(stored_slots(&[(1, 60.0), (2, 60.0)])),
    );
    state.snapshot.leakshield_data.insert(
        SECOND.into(),
        device_like(stored_slots(&[(1, 60.0), (2, 50.0)])),
    );
    state.snapshot.profile_contents.insert(
        "profile_default".into(),
        ProfileContent {
            leakshield: [
                (LEAKSHIELD.to_owned(), thresholds(60.0)),
                (SECOND.to_owned(), thresholds(60.0)),
            ]
            .into(),
            ..Default::default()
        },
    );
    state
}

#[test]
fn review_apply_to_all_slots_covers_every_leakshield() {
    let state = two_devices();
    let commands = ProfilesUi::default().apply_to_all_slots(&state);
    // Slot 2 of the second Leakshield differs: it must get a SetStoredFields.
    assert!(
        commands.iter().any(|c| matches!(
            c,
            Command::SetStoredFields { leakshield, slot: 2, .. } if leakshield == SECOND
        )),
        "commands: {commands:?}"
    );
}

#[test]
fn review_two_leakshields_name_their_device_in_the_warning() {
    let state = two_devices();
    let notices = model(&state, &ProfilesUi::default())
        .detail
        .unwrap()
        .notices;
    let differ: Vec<&str> = notices
        .iter()
        .filter(|n| n.text.contains("Alarm thresholds differ"))
        .map(|n| n.text.as_str())
        .collect();
    // Only the second Leakshield has a slot with other thresholds.
    assert_eq!(
        differ,
        ["Leakshield 30000: Alarm thresholds differ from 1 other slots"]
    );
    // With one Leakshield the row has no device name.
    let one = with_slots();
    let notices = model(&one, &ProfilesUi::default()).detail.unwrap().notices;
    assert!(notices.iter().all(|n| !n.text.contains("Leakshield")));
}

#[test]
fn apply_to_all_slots_writes_the_active_slot_through_the_device() {
    let mut state = two_devices();
    // The second device runs slot 2.
    state.snapshot.leakshields[1].active_slot = 2;
    let commands = ProfilesUi::default().apply_to_all_slots(&state);
    let second: Vec<&Command> = commands
        .iter()
        .filter(|c| matches!(c,
            Command::SetStoredFields { leakshield, .. } | Command::SetLeakshieldFields { leakshield, .. }
            if leakshield == SECOND))
        .collect();
    assert_eq!(second.len(), 1);
    assert!(
        matches!(second[0], Command::SetLeakshieldFields { fields, .. } if fields.len() == 3),
        "{second:?}"
    );
    // The first device runs slot 1, the slot of the Profile: its other slots are stored writes.
    assert!(commands.iter().any(|c| matches!(
        c,
        Command::SetStoredFields { leakshield, slot: 2, .. } if leakshield == LEAKSHIELD
    )));
}

#[test]
fn one_failed_slot_row_for_each_leakshield() {
    let mut state = two_devices();
    state.snapshot.leakshields[1].active_slot = 2;
    let mut feedback = Feedback::default();
    feedback.set(
        format!("stored:{LEAKSHIELD}:2"),
        nicecooling::screens::WriteOutcome::Refused {
            error: "NotVerified".into(),
        },
    );
    feedback.set(
        format!("ls:{SECOND}:slot-2"),
        nicecooling::screens::WriteOutcome::NotAuthorized,
    );
    let shown = ProfilesModel::build(&input(&state, &feedback, NOW_US), &ProfilesUi::default());
    let rows: Vec<String> = shown
        .detail
        .unwrap()
        .notices
        .into_iter()
        .map(|n| n.text)
        .filter(|text| text.contains(": Slot "))
        .collect();
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert!(rows[0].starts_with("Leakshield 31072: Slot 2: not changed."));
    assert!(rows[1].starts_with("Leakshield 30000: Slot 2: not changed."));
}

#[test]
fn review_restore_offer_for_each_failed_leakshield() {
    let mut state = two_devices();
    state.snapshot.active_profile = Some(state.snapshot.profiles[0].clone());
    state.snapshot.leakshields[0].failed_write_backup = "backup_a".into();
    state.snapshot.leakshields[1].failed_write_backup = "backup_b".into();
    let detail = model(&state, &ProfilesUi::default()).detail.unwrap();
    // Both devices hold a backup of the failed activation: the user reaches both.
    let commands: Vec<_> = detail.restore.iter().map(|o| o.command.clone()).collect();
    assert_eq!(
        commands,
        [
            Some(Command::RestoreBackup {
                leakshield: LEAKSHIELD.into(),
                backup: "backup_a".into()
            }),
            Some(Command::RestoreBackup {
                leakshield: SECOND.into(),
                backup: "backup_b".into()
            }),
        ]
    );
    // With two Leakshields each offer names its device.
    assert!(
        detail.restore[0]
            .text
            .starts_with("Leakshield 31072: The Curves are active.")
    );
    assert!(detail.restore[1].text.starts_with("Leakshield 30000: "));
    // With one failed device and one Leakshield, the text has no device name.
    let mut one = base();
    one.snapshot.leakshields[0].failed_write_backup = "backup_a".into();
    let offer = model(&one, &ProfilesUi::default()).detail.unwrap().restore;
    assert_eq!(offer.len(), 1);
    assert_eq!(
        offer[0].text,
        "The Curves are active. The Leakshield write failed."
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn rule_group_edit_reads_the_groups_again() {
    let bus = PrivateBus::start("profiles-group-edit");
    let _daemon = FakeDaemon::start(bus.connect().await, Options::default()).await;
    let mut harness = Harness::start(&bus).await;
    harness
        .until("the Profiles are known", |state| {
            !state.snapshot.profiles.is_empty()
        })
        .await;
    // A new group changes `RuleGroupState`: the signal brings the group.
    let create = Command::SetRuleGroups {
        groups: vec![group("", "profile_2", vec![sensor_rule()])],
    };
    create.run_on_daemon(&harness.client).await.expect("create");
    harness
        .until("the group shows", |state| {
            state.snapshot.rule_groups.len() == 1
        })
        .await;
    // An edit of the rules keeps the id, the order and the state: the daemon sends no signal.
    let mut edited = harness.state.snapshot.rule_groups.clone();
    edited[0].rules = vec![process_rule(), time_rule()];
    let edit = Command::SetRuleGroups { groups: edited };
    edit.run_on_daemon(&harness.client).await.expect("edit");
    let refresh = edit.refresh(&harness.client).await.expect("a refresh");
    harness.state.apply(&refresh);
    assert_eq!(
        harness.state.snapshot.rule_groups[0].rules,
        [process_rule(), time_rule()]
    );
}
