//! The daemon against the XML contract: the introspection, and the interfaces of later tasks.

mod support;

use std::collections::{BTreeMap, BTreeSet};

use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_api::manager::ManagerProxy;
use nicecooling_api::profile::ProfileProxy;
use nicecooling_api::profiles::ProfilesProxy;
use nicecooling_api::{INTERFACE_PREFIX, INTERFACE_XML};
use support::{Rig, error_name, none_path, path};
use zbus::OwnedObjectPath;

const NOT_SUPPORTED: &str = "org.freedesktop.DBus.Error.NotSupported";

/// The members of each interface, as comparable text. Names of in and out arguments, types and
/// the `EmitsChangedSignal` annotation count. Comments, the standard interfaces and the
/// authorization annotation do not: zbus has no way to write that annotation.
fn canonical(xml: &str) -> BTreeMap<String, BTreeSet<String>> {
    let doc = roxmltree::Document::parse_with_options(
        xml,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .unwrap();
    let mut interfaces: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for interface in doc.descendants().filter(|n| n.has_tag_name("interface")) {
        let name = interface.attribute("name").unwrap().to_owned();
        if name.starts_with("org.freedesktop.DBus.") {
            continue;
        }
        let members = interfaces.entry(name).or_default();
        for member in interface.children().filter(|n| n.is_element()) {
            let name = member.attribute("name").unwrap_or_default();
            let args = |signal: bool| -> String {
                member
                    .children()
                    .filter(|a| a.has_tag_name("arg"))
                    .map(|a| {
                        let direction = if signal {
                            ""
                        } else {
                            a.attribute("direction").unwrap_or("in")
                        };
                        format!(
                            "{direction}:{}:{}",
                            a.attribute("name").unwrap_or("?"),
                            a.attribute("type").unwrap()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            };
            let text = match member.tag_name().name() {
                "method" => format!("method {name}({})", args(false)),
                "signal" => format!("signal {name}({})", args(true)),
                "property" => {
                    let emits = member
                        .children()
                        .find(|a| {
                            a.has_tag_name("annotation")
                                && a.attribute("name")
                                    == Some("org.freedesktop.DBus.Property.EmitsChangedSignal")
                        })
                        .and_then(|a| a.attribute("value"))
                        .unwrap_or("true");
                    format!(
                        "property {name} {} {} emits={emits}",
                        member.attribute("type").unwrap(),
                        member.attribute("access").unwrap()
                    )
                }
                other => panic!("unexpected element {other}"),
            };
            members.insert(text);
        }
    }
    interfaces
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn introspection_matches_xml() {
    let mut rig = Rig::start().await;
    rig.tick_in_background();
    // A Derived sensor adds its interface to a Sensor object.
    ManagerProxy::new(&rig.client)
        .await
        .unwrap()
        .create_derived_sensor("Hottest", "maximum", &[path(Kind::Sensor, "k10temp_temp1")])
        .await
        .unwrap();

    let mut merged: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut todo = vec![nicecooling_api::ROOT_PATH.to_owned()];
    while let Some(object) = todo.pop() {
        let proxy = zbus::fdo::IntrospectableProxy::builder(&rig.client)
            .destination(nicecooling_api::BUS_NAME)
            .path(object.as_str())
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await
            .unwrap();
        let xml = proxy.introspect().await.unwrap();
        let doc = roxmltree::Document::parse_with_options(
            &xml,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .unwrap();
        for child in doc
            .root_element()
            .children()
            .filter(|n| n.has_tag_name("node"))
        {
            todo.push(format!("{object}/{}", child.attribute("name").unwrap()));
        }
        for (name, members) in canonical(&xml) {
            merged.entry(name).or_default().extend(members);
        }
    }
    let expected = canonical(INTERFACE_XML);
    assert_eq!(expected.len(), 10, "the contract has 10 interfaces");
    for (name, members) in &expected {
        assert!(name.starts_with(INTERFACE_PREFIX));
        let got = merged
            .get(name)
            .unwrap_or_else(|| panic!("{name} is not on the bus"));
        let missing: Vec<_> = members.difference(got).collect();
        let extra: Vec<_> = got.difference(members).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "{name}\n  missing on the bus: {missing:#?}\n  not in the XML: {extra:#?}"
        );
    }
    assert_eq!(
        merged.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>()
    );
}

/// Calls a method of a stub interface and checks the reply.
macro_rules! stub {
    ($seen:expr, $name:literal, $call:expr) => {{
        $seen.insert($name.to_owned());
        let error = $call.await.expect_err(concat!($name, " must fail"));
        assert_eq!(error_name(&error), NOT_SUPPORTED, "{}", $name);
    }};
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unfilled_interfaces_reply_not_supported() {
    let rig = Rig::start().await;
    let mut seen = BTreeSet::new();
    let none: OwnedObjectPath = none_path();

    let leakshield = LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, support::LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    // The properties work.
    assert_eq!(leakshield.serial().await.unwrap(), "20417-31072");
    assert!(leakshield.writable().await.unwrap());
    assert_eq!(leakshield.software_sensor_slots().await.unwrap(), 12);
    stub!(
        seen,
        "Leakshield.GetDeviceResponseProposals",
        leakshield.get_device_response_proposals()
    );

    let profiles = ProfilesProxy::builder(&rig.client)
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    assert_eq!(
        profiles.active().await.unwrap(),
        path(Kind::Profile, "profile_default")
    );
    assert_eq!(profiles.rule_delay_seconds().await.unwrap(), 30);
    stub!(
        seen,
        "Profiles.CreateProfile",
        profiles.create_profile("p", &none)
    );
    stub!(
        seen,
        "Profiles.ActivateProfile",
        profiles.activate_profile(&none)
    );
    stub!(
        seen,
        "Profiles.ReturnToAutomatic",
        profiles.return_to_automatic()
    );
    stub!(
        seen,
        "Profiles.SetImmediateSwitching",
        profiles.set_immediate_switching(true)
    );
    stub!(
        seen,
        "Profiles.SetRuleTiming",
        profiles.set_rule_timing(1, 1)
    );
    stub!(seen, "Profiles.GetRuleGroups", profiles.get_rule_groups());
    stub!(
        seen,
        "Profiles.SetRuleGroups",
        profiles.set_rule_groups(Vec::new())
    );

    let profile = ProfileProxy::builder(&rig.client)
        .path(path(Kind::Profile, "profile_default"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    assert!(profile.is_default().await.unwrap());
    assert_eq!(profile.slot().await.unwrap(), 1);
    stub!(seen, "Profile.SetName", profile.set_name("n"));
    stub!(seen, "Profile.SetSlot", profile.set_slot(2));
    stub!(seen, "Profile.Delete", profile.delete());
    stub!(seen, "Profile.GetContent", profile.get_content());

    // Every method of the three interfaces is in the list, and nothing else, except the wired ones.
    let doc = roxmltree::Document::parse_with_options(
        INTERFACE_XML,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .unwrap();
    let in_xml: BTreeSet<String> = doc
        .descendants()
        .filter(|n| n.has_tag_name("interface"))
        .filter_map(|i| {
            let name = i
                .attribute("name")?
                .strip_prefix(&format!("{INTERFACE_PREFIX}."))?;
            ["Leakshield", "Profiles", "Profile"]
                .contains(&name)
                .then(|| (name.to_owned(), i))
        })
        .flat_map(|(name, i)| {
            i.children()
                .filter(|m| m.has_tag_name("method"))
                .map(move |m| format!("{name}.{}", m.attribute("name").unwrap()))
        })
        .collect();
    // The methods that the Leakshield tests cover (`tests/leakshield/`) are wired.
    let wired = [
        "GetFields",
        "SetFields",
        "SetMode",
        "StartAction",
        "CancelAction",
        "SyncClock",
        "ListBackups",
        "CreateBackup",
        "GetBackup",
        "ImportBackup",
        "RestoreBackup",
        "ResetRefillHistory",
        "ResetServiceCounter",
        "GetEvents",
        "GetSoftwareSensors",
        "SetSoftwareSensor",
        "FactoryReset",
    ]
    .map(|m| format!("Leakshield.{m}"));
    let in_xml: BTreeSet<String> = in_xml.into_iter().filter(|m| !wired.contains(m)).collect();
    assert_eq!(seen, in_xml);
}
