//! Rule groups through the API, and the rules stage of the tick (tasks 7.3 and 7.4).

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use nicecooling_api::authz_map::Action;
use nicecooling_api::ids::Kind;
use nicecooling_api::profiles::{GroupIn, RuleIn};
use nicecooling_protocol::sim::Request;
use tokio::time::timeout;
use zbus::{OwnedObjectPath, Variant};

use crate::support::{Rig, error_name, path};
use crate::{ERR, profile, profiles, with_profile_3};

type Params = HashMap<&'static str, Variant<'static>>;

/// A time rule for every day, all day: it holds all the time.
pub fn always() -> RuleIn<'static> {
    (
        "time",
        Params::from([
            ("days", Variant::from(vec![1u8, 2, 3, 4, 5, 6, 7])),
            ("start", Variant::U16(0)),
            ("end", Variant::U16(1440)),
        ]),
    )
}

/// A time rule that never holds: the range is empty.
pub fn never() -> RuleIn<'static> {
    (
        "time",
        Params::from([
            ("days", Variant::from(vec![1u8])),
            ("start", Variant::U16(600)),
            ("end", Variant::U16(600)),
        ]),
    )
}

pub fn group<'a>(
    id: &'a str,
    target: &'a OwnedObjectPath,
    enabled: bool,
    rules: Vec<RuleIn<'a>>,
) -> GroupIn<'a> {
    (id, target, enabled, rules)
}

fn pwm(n: &str) -> OwnedObjectPath {
    path(Kind::Sensor, n)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_rule_groups_returns_ids_in_order() {
    let rig = Rig::start_with(with_profile_3).await;
    let profiles = profiles(&rig).await;
    let (two, three) = (
        path(Kind::Profile, "profile_2"),
        path(Kind::Profile, "profile_3"),
    );

    let ids = profiles
        .set_rule_groups(vec![
            group("", &two, true, vec![always()]),
            group("", &three, false, vec![never()]),
            group("", &two, true, Vec::new()),
        ])
        .await
        .unwrap();
    assert_eq!(ids, ["group_1", "group_2", "group_3"]);

    // The list is replaced: an existing id keeps its place, an empty one creates a group.
    let ids = profiles
        .set_rule_groups(vec![
            group("group_3", &two, true, Vec::new()),
            group("", &three, true, vec![always()]),
            group("group_1", &two, true, vec![always()]),
        ])
        .await
        .unwrap();
    assert_eq!(ids, ["group_3", "group_4", "group_1"]);

    let groups = profiles.get_rule_groups().await.unwrap();
    let listed: Vec<(&str, &str, bool, usize)> = groups
        .iter()
        .map(|(id, target, enabled, rules)| {
            (
                id.as_str(),
                target.as_str().rsplit('/').next().unwrap(),
                *enabled,
                rules.len(),
            )
        })
        .collect();
    assert_eq!(
        listed,
        [
            ("group_3", "profile_2", true, 0),
            ("group_4", "profile_3", true, 1),
            ("group_1", "profile_2", true, 1),
        ]
    );
    let (kind, params) = &groups[1].3[0];
    assert_eq!(kind, "time");
    assert_eq!(
        u16::try_from(params["end"].try_clone().unwrap()).unwrap(),
        1440
    );
    assert_eq!(
        rig.authorizer.calls(),
        [(Action::Write, false), (Action::Write, false)]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_rule_groups_round_trips_every_kind() {
    let rig = Rig::start_with(with_profile_3).await;
    let profiles = profiles(&rig).await;
    let two = path(Kind::Profile, "profile_2");
    let sensor = pwm("k10temp_temp1");
    let rules: Vec<RuleIn> = vec![
        (
            "sensor",
            Params::from([
                ("sensor", Variant::from(sensor.clone())),
                ("comparison", Variant::from("in_range")),
                ("value", Variant::F64(30.0)),
                ("value_max", Variant::F64(40.0)),
                ("hold_s", Variant::U32(60)),
            ]),
        ),
        (
            "process",
            Params::from([
                ("names", Variant::from(vec!["game".to_owned()])),
                ("match", Variant::from("all")),
                ("match_on", Variant::from("cmdline")),
            ]),
        ),
        always(),
        (
            "interval",
            Params::from([
                ("interval_s", Variant::U32(600)),
                ("delay_s", Variant::U32(60)),
                ("duration_s", Variant::U32(120)),
            ]),
        ),
    ];
    profiles
        .set_rule_groups(vec![group("", &two, true, rules)])
        .await
        .unwrap();

    let groups = profiles.get_rule_groups().await.unwrap();
    let rules = &groups[0].3;
    let kinds: Vec<&str> = rules.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(kinds, ["sensor", "process", "time", "interval"]);
    let sensor_params = &rules[0].1;
    assert_eq!(
        OwnedObjectPath::try_from(sensor_params["sensor"].try_clone().unwrap()).unwrap(),
        sensor
    );
    assert_eq!(
        String::try_from(sensor_params["comparison"].try_clone().unwrap()).unwrap(),
        "in_range"
    );
    assert_eq!(
        f64::try_from(sensor_params["value_max"].try_clone().unwrap()).unwrap(),
        40.0
    );
    assert_eq!(
        u32::try_from(sensor_params["hold_s"].try_clone().unwrap()).unwrap(),
        60
    );
    assert_eq!(
        String::try_from(rules[1].1["match_on"].try_clone().unwrap()).unwrap(),
        "cmdline"
    );
    // The groups are in `state.json` with the keys of the storage contract.
    rig.daemon.as_ref().unwrap().hub.flush_now();
    let state = std::fs::read_to_string(rig.state.join("state.json")).unwrap();
    assert!(state.contains("\"kind\": \"interval\""), "{state}");
    assert!(state.contains("\"params\""), "{state}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_rule_groups_rejects_unknown_kind_and_bad_keys() {
    let rig = Rig::start_with(with_profile_3).await;
    let profiles = profiles(&rig).await;
    let two = path(Kind::Profile, "profile_2");
    let time = |edit: &dyn Fn(&mut Params)| -> RuleIn<'static> {
        let (kind, mut params) = always();
        edit(&mut params);
        (kind, params)
    };
    let sensor = |edit: &dyn Fn(&mut Params)| -> RuleIn<'static> {
        let mut params = Params::from([
            ("sensor", Variant::from(pwm("k10temp_temp1"))),
            ("comparison", Variant::from("gt")),
            ("value", Variant::F64(50.0)),
            ("hold_s", Variant::U32(0)),
        ]);
        edit(&mut params);
        ("sensor", params)
    };
    let process = |edit: &dyn Fn(&mut Params)| -> RuleIn<'static> {
        let mut params = Params::from([
            ("names", Variant::from(vec!["game".to_owned()])),
            ("match", Variant::from("any")),
        ]);
        edit(&mut params);
        ("process", params)
    };
    let bad: Vec<(&str, RuleIn<'static>)> = vec![
        ("unknown kind", ("hotkey", Params::new())),
        ("missing days", time(&|p| drop(p.remove("days")))),
        (
            "wrong type",
            time(&|p| drop(p.insert("start", Variant::from("noon")))),
        ),
        (
            "unknown key",
            time(&|p| drop(p.insert("zone", Variant::U32(1)))),
        ),
        (
            "weekday 8",
            time(&|p| drop(p.insert("days", Variant::from(vec![8u8])))),
        ),
        (
            "minute 1441",
            time(&|p| drop(p.insert("end", Variant::U16(1441)))),
        ),
        (
            "unknown comparison",
            sensor(&|p| drop(p.insert("comparison", Variant::from("near")))),
        ),
        (
            "in_range without max",
            sensor(&|p| drop(p.insert("comparison", Variant::from("in_range")))),
        ),
        (
            "max below min",
            sensor(&|p| {
                p.insert("comparison", Variant::from("in_range"));
                p.insert("value_max", Variant::F64(10.0));
            }),
        ),
        (
            "max for gt",
            sensor(&|p| drop(p.insert("value_max", Variant::F64(90.0)))),
        ),
        (
            "value not finite",
            sensor(&|p| drop(p.insert("value", Variant::F64(f64::NAN)))),
        ),
        ("no hold_s", sensor(&|p| drop(p.remove("hold_s")))),
        (
            "process without names",
            process(&|p| drop(p.insert("names", Variant::from(Vec::<String>::new())))),
        ),
        (
            "unknown match",
            process(&|p| drop(p.insert("match", Variant::from("some")))),
        ),
        (
            "unknown match_on",
            process(&|p| drop(p.insert("match_on", Variant::from("pid")))),
        ),
        (
            "interval 0",
            (
                "interval",
                Params::from([
                    ("interval_s", Variant::U32(0)),
                    ("delay_s", Variant::U32(0)),
                    ("duration_s", Variant::U32(1)),
                ]),
            ),
        ),
    ];
    for (name, rule) in bad {
        let error = profiles
            .set_rule_groups(vec![group("", &two, true, vec![rule])])
            .await
            .expect_err(name);
        assert_eq!(
            error_name(&error),
            format!("{ERR}.InvalidArgument"),
            "{name}"
        );
    }
    // A group id that is not an identifier, and the same id twice.
    for ids in [vec!["Bad Id"], vec!["group_1", "group_1"]] {
        let groups = ids
            .iter()
            .map(|id| group(id, &two, true, vec![always()]))
            .collect();
        let error = profiles.set_rule_groups(groups).await.unwrap_err();
        assert_eq!(
            error_name(&error),
            format!("{ERR}.InvalidArgument"),
            "{ids:?}"
        );
    }
    // Nothing of a refused list is kept.
    assert!(profiles.get_rule_groups().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_rule_groups_unknown_profile_returns_not_found() {
    let rig = Rig::start_with(with_profile_3).await;
    let profiles = profiles(&rig).await;
    let missing = path(Kind::Profile, "profile_99");
    let error = profiles
        .set_rule_groups(vec![group("", &missing, true, vec![always()])])
        .await
        .unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.NotFound"));
    // An unknown sensor is not found either.
    let two = path(Kind::Profile, "profile_2");
    let unknown_sensor: RuleIn = (
        "sensor",
        Params::from([
            ("sensor", Variant::from(pwm("no_such_temp1"))),
            ("comparison", Variant::from("gt")),
            ("value", Variant::F64(1.0)),
            ("hold_s", Variant::U32(0)),
        ]),
    );
    let error = profiles
        .set_rule_groups(vec![group("", &two, true, vec![unknown_sensor])])
        .await
        .unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.NotFound"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_rule_groups_asks_write() {
    let rig = Rig::start_with(with_profile_3).await;
    let two = path(Kind::Profile, "profile_2");
    profiles(&rig)
        .await
        .set_rule_groups(vec![group("", &two, true, vec![always()])])
        .await
        .unwrap();
    assert_eq!(rig.authorizer.calls(), [(Action::Write, false)]);
    // Reading the groups needs no check.
    rig.authorizer.clear();
    profiles(&rig).await.get_rule_groups().await.unwrap();
    assert!(rig.authorizer.calls().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delete_profile_targeted_by_group_returns_in_use() {
    let rig = Rig::start_with(with_profile_3).await;
    let profiles = profiles(&rig).await;
    let four = path(Kind::Profile, "profile_4");
    profiles
        .set_rule_groups(vec![group("", &four, true, vec![always()])])
        .await
        .unwrap();

    let error = profile(&rig, "profile_4").await.delete().await.unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.InUse"));

    // The Profile goes when no group points to it.
    profiles.set_rule_groups(Vec::new()).await.unwrap();
    profile(&rig, "profile_4").await.delete().await.unwrap();
    // The active Profile is in use as well.
    let error = profile(&rig, "profile_3").await.delete().await.unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.InUse"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_group_activates_its_profile_once_and_the_default_comes_back() {
    let mut rig = Rig::start_with_clock_and(with_profile_3).await;
    let profiles = profiles(&rig).await;
    let two = path(Kind::Profile, "profile_2");
    let mut signals = profiles.receive_profile_activated().await.unwrap();
    profiles.set_immediate_switching(true).await.unwrap();
    profiles
        .set_rule_groups(vec![group("", &two, true, vec![always()])])
        .await
        .unwrap();
    // The user returns to automatic: the rules decide.
    profiles.return_to_automatic().await.unwrap();

    rig.step(2).await;
    let signal = timeout(Duration::from_secs(5), signals.next())
        .await
        .expect("an activation")
        .unwrap();
    let args = signal.args().unwrap();
    assert_eq!(
        (
            args.profile.as_str().rsplit('/').next(),
            args.cause.as_str()
        ),
        (Some("profile_2"), "rule")
    );
    assert_eq!(profiles.active().await.unwrap(), two);
    assert_eq!(profiles.activation_cause().await.unwrap(), "rule");

    // The group stays the winner for 100 ticks: no second activation.
    for _ in 0..100 {
        rig.daemon.as_mut().unwrap().runner.tick();
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        timeout(Duration::from_millis(300), signals.next())
            .await
            .is_err(),
        "a second activation"
    );

    // The group goes away: the Default Profile is active again.
    profiles.set_rule_groups(Vec::new()).await.unwrap();
    rig.step(2).await;
    assert_eq!(
        profiles.active().await.unwrap(),
        path(Kind::Profile, "profile_default")
    );
    assert_eq!(profiles.activation_cause().await.unwrap(), "default");
}

fn feature_writes(rig: &Rig) -> usize {
    rig.sim
        .requests()
        .iter()
        .filter(|r| matches!(r, Request::SetFeature { .. }))
        .count()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn device_change_while_group_holds_not_overwritten() {
    let mut rig = Rig::start_with_clock_and(with_profile_3).await;
    let profiles = profiles(&rig).await;
    let two = path(Kind::Profile, "profile_2");
    profiles.set_immediate_switching(true).await.unwrap();
    profiles
        .set_rule_groups(vec![group("", &two, true, vec![always()])])
        .await
        .unwrap();
    profiles.return_to_automatic().await.unwrap();
    rig.step(2).await;
    // The group won and wrote slot 2: the device has the number 2 now.
    assert_eq!(rig.sim.report3()[747], 2);
    assert_eq!(feature_writes(&rig), 1);
    rig.sim.edit_report1(|r| r.active_profile = 2);

    // The device button changes a setting while the group keeps holding.
    rig.sim.change_report3(63, &[0x02, 0x58]);
    for _ in 0..50 {
        rig.daemon.as_mut().unwrap().runner.tick();
    }
    tokio::time::sleep(Duration::from_millis(300)).await;

    assert_eq!(feature_writes(&rig), 1);
    assert_eq!(&rig.sim.report3()[63..65], [0x02, 0x58]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rule_group_state_follows_the_groups() {
    let mut rig = Rig::start_with_clock_and(with_profile_3).await;
    let profiles = profiles(&rig).await;
    let two = path(Kind::Profile, "profile_2");
    profiles
        .set_rule_groups(vec![
            group("", &two, true, vec![always()]),
            group("", &two, true, vec![never()]),
        ])
        .await
        .unwrap();
    rig.step(2).await;
    let states = profiles.rule_group_state().await.unwrap();
    assert_eq!(states.len(), 2);
    assert_eq!((states[0].0.as_str(), states[0].1), ("group_1", true));
    assert!(states[0].2 > 0);
    assert_eq!(
        (states[1].0.as_str(), states[1].1, states[1].2),
        ("group_2", false, 0)
    );
    // The time of the start stays while the group holds.
    let since = states[0].2;
    rig.step(3).await;
    assert_eq!(profiles.rule_group_state().await.unwrap()[0].2, since);
}

/// Advances the simulated clock by one second and runs one tick, without a pause.
fn tick(rig: &mut Rig) {
    rig.clock
        .as_ref()
        .expect("a rig with a simulated clock")
        .advance(Duration::from_secs(1));
    rig.daemon.as_mut().unwrap().runner.tick();
}

fn active(rig: &Rig) -> String {
    rig.daemon.as_ref().unwrap().hub.profiles_state().active
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_rule_timing_changes_delay_and_dwell_and_properties() {
    let mut rig = Rig::start_with_clock_and(with_profile_3).await;
    let profiles = profiles(&rig).await;
    // The defaults.
    assert_eq!(profiles.rule_delay_seconds().await.unwrap(), 30);
    assert_eq!(profiles.dwell_seconds().await.unwrap(), 300);

    profiles.set_rule_timing(10, 60).await.unwrap();
    assert_eq!(profiles.rule_delay_seconds().await.unwrap(), 10);
    assert_eq!(profiles.dwell_seconds().await.unwrap(), 60);
    let two = path(Kind::Profile, "profile_2");
    profiles
        .set_rule_groups(vec![group("", &two, true, vec![always()])])
        .await
        .unwrap();
    profiles.return_to_automatic().await.unwrap();

    // The group holds from the first tick and wins after the Rule delay of 10 s.
    let mut switched = None;
    for n in 0..30 {
        tick(&mut rig);
        if active(&rig) == "profile_2" {
            switched = Some(n);
            break;
        }
    }
    // The first tick returns to automatic: it starts the timers of the group.
    assert!(matches!(switched, Some(10 | 11)), "{switched:?}");

    // The group goes. Its Profile stays for the Profile dwell of 60 s, then the Default Profile.
    profiles.set_rule_groups(Vec::new()).await.unwrap();
    let mut back = None;
    for n in 0..200 {
        tick(&mut rig);
        if active(&rig) == "profile_default" {
            back = Some(n);
            break;
        }
    }
    assert!(matches!(back, Some(59..=61)), "{back:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rule_timing_survives_restart() {
    let mut rig = Rig::start_with_clock_and(with_profile_3).await;
    let profiles = profiles(&rig).await;
    profiles.set_rule_timing(7, 99).await.unwrap();
    profiles.set_immediate_switching(true).await.unwrap();

    rig.restart_daemon().await;

    let state = rig.daemon.as_ref().unwrap().hub.profiles_state();
    assert_eq!((state.rule_delay_s, state.dwell_s), (7, 99));
    assert!(state.immediate_switching);
    let file = std::fs::read_to_string(rig.state.join("state.json")).unwrap();
    assert!(file.contains("\"rule_delay_s\": 7"), "{file}");
    assert!(file.contains("\"dwell_s\": 99"), "{file}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_rule_timing_asks_write() {
    let rig = Rig::start_with_clock_and(with_profile_3).await;
    let profiles = profiles(&rig).await;
    profiles.set_rule_timing(5, 5).await.unwrap();
    profiles.set_immediate_switching(true).await.unwrap();
    profiles.return_to_automatic().await.unwrap();
    profiles
        .activate_profile(&path(Kind::Profile, "profile_4"))
        .await
        .unwrap();
    assert_eq!(rig.authorizer.calls(), [(Action::Write, false); 4]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn immediate_switching_applies_a_group_in_the_tick_where_it_holds() {
    let mut rig = Rig::start_with_clock_and(with_profile_3).await;
    let profiles = profiles(&rig).await;
    let two = path(Kind::Profile, "profile_2");
    profiles.set_immediate_switching(true).await.unwrap();
    assert!(profiles.immediate_switching().await.unwrap());
    profiles
        .set_rule_groups(vec![group("", &two, true, vec![always()])])
        .await
        .unwrap();
    profiles.return_to_automatic().await.unwrap();
    tick(&mut rig);
    assert_eq!(active(&rig), "profile_2");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn manual_activation_lasts_until_return_to_automatic() {
    let mut rig = Rig::start_with_clock_and(with_profile_3).await;
    let profiles = profiles(&rig).await;
    let two = path(Kind::Profile, "profile_2");
    profiles.set_immediate_switching(true).await.unwrap();
    profiles
        .set_rule_groups(vec![group("", &two, true, vec![always()])])
        .await
        .unwrap();
    tick(&mut rig);
    tick(&mut rig);
    assert_eq!(active(&rig), "profile_3");

    // The user activates a Profile by hand: it wins at once.
    profiles
        .activate_profile(&path(Kind::Profile, "profile_4"))
        .await
        .unwrap();
    assert_eq!(active(&rig), "profile_4");
    assert_eq!(profiles.activation_cause().await.unwrap(), "manual");
    for _ in 0..20 {
        tick(&mut rig);
    }
    assert_eq!(active(&rig), "profile_4");

    // The rules decide again.
    profiles.return_to_automatic().await.unwrap();
    tick(&mut rig);
    assert_eq!(active(&rig), "profile_2");
    assert_eq!(profiles.activation_cause().await.unwrap(), "rule");
}
