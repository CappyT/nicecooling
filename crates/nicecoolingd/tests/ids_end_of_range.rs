//! A create call refuses at the end of the id range: no id is given twice, and no call panics.

mod support;

use std::fs;

use nicecooling_api::ids::{Kind, object_path};
use nicecooling_core::rules::group::RuleGroupDef;
use support::{PROFILE, Rig, STATE};

const LAST: &str = "4294967295";

async fn rig_with_state(state: String) -> Rig {
    Rig::start_with(move |_, dir| fs::write(dir.join("state.json"), &state).unwrap()).await
}

fn refused(result: Result<impl std::fmt::Debug, impl std::fmt::Display>, what: &str) {
    let text = result.expect_err("the create call took an id").to_string();
    assert!(text.contains(&format!("no {what} id is left")), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_loop_refuses_after_the_largest_loop_number() {
    let rig = rig_with_state(STATE.replace("\"loop_1\"", &format!("\"loop_{LAST}\""))).await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    refused(hub.create_loop("Second"), "Loop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_calibration_table_refuses_a_new_id_after_the_largest_table_number() {
    let table = format!(
        r#""calibration_tables": [{{"id": "table_{LAST}", "name": "Top", "points": [[500, 10], [1000, 20]]}}]"#
    );
    let rig = rig_with_state(STATE.replace(
        "\"schema_version\": 1,",
        &format!("\"schema_version\": 1, {table},"),
    ))
    .await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    refused(
        hub.set_calibration_table("", "New", vec![(500.0, 10.0), (1000.0, 20.0)]),
        "Calibration table",
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rule_group_refuses_a_new_id_after_the_largest_group_number() {
    let group = format!(
        r#""rule_groups": [{{"id": "group_{LAST}", "profile": "profile_default", "enabled": true, "rules": []}}]"#
    );
    let rig = rig_with_state(STATE.replace(
        "\"schema_version\": 1,",
        &format!("\"schema_version\": 1, {group},"),
    ))
    .await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let existing = RuleGroupDef {
        id: format!("group_{LAST}"),
        profile: "profile_default".into(),
        enabled: true,
        rules: Vec::new(),
    };
    let new = RuleGroupDef {
        id: String::new(),
        ..existing.clone()
    };
    refused(hub.set_rule_groups(vec![existing, new]), "Rule group");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_profile_refuses_after_the_largest_profile_number() {
    let rig = Rig::start_with(|_, dir| {
        let profile = dir.join(format!("profiles/profile_{LAST}"));
        fs::create_dir_all(&profile).unwrap();
        fs::write(profile.join("profile.json"), PROFILE).unwrap();
    })
    .await;
    let hub = rig.daemon.as_ref().unwrap().hub.clone();
    let default = object_path(Kind::Profile, "profile_default");
    refused(hub.create_profile("Copy", &default), "Profile");
}
