//! The daemon against the XML contract: the introspection, and the interfaces of later tasks.

mod support;

use std::collections::{BTreeMap, BTreeSet};

use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_api::manager::ManagerProxy;
use nicecooling_api::profile::ProfileProxy;
use nicecooling_api::profiles::ProfilesProxy;
use nicecooling_api::{INTERFACE_PREFIX, INTERFACE_XML};
use support::{Rig, path};

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
    // A Derived sensor and a Graph sensor add their interfaces to a Sensor object.
    let manager = ManagerProxy::new(&rig.client).await.unwrap();
    manager
        .create_derived_sensor("Hottest", "maximum", &[path(Kind::Sensor, "k10temp_temp1")])
        .await
        .unwrap();
    manager.create_graph_sensor("Water", "degC").await.unwrap();

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
    assert_eq!(expected.len(), 11, "the contract has 11 interfaces");
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn leakshield_and_profile_properties_read() {
    let rig = Rig::start().await;
    let leakshield = LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, support::LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    assert_eq!(leakshield.serial().await.unwrap(), "20417-31072");
    assert!(leakshield.writable().await.unwrap());
    assert_eq!(leakshield.software_sensor_slots().await.unwrap(), 12);

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
    assert_eq!(profiles.dwell_seconds().await.unwrap(), 300);

    let profile = ProfileProxy::builder(&rig.client)
        .path(path(Kind::Profile, "profile_default"))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    assert!(profile.is_default().await.unwrap());
    assert_eq!(profile.slot().await.unwrap(), 1);
}
