//! The authorization of each method, read from the annotation
//! `io.github.cappyt.NiceCooling1.Authorization` of the XML contract.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::OnceLock;

pub const ANNOTATION: &str = "io.github.cappyt.NiceCooling1.Authorization";

/// A polkit action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Action {
    Write,
    Critical,
    CancelPoweroff,
}

impl Action {
    pub const ALL: [Action; 3] = [Self::Write, Self::Critical, Self::CancelPoweroff];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Write => "io.github.cappyt.NiceCooling1.write",
            Self::Critical => "io.github.cappyt.NiceCooling1.critical",
            Self::CancelPoweroff => "io.github.cappyt.NiceCooling1.cancel-poweroff",
        }
    }
}

/// The value of the annotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Authorization {
    Read,
    Write,
    Critical,
    CancelPoweroff,
    /// `SetMode`: critical for `release`, write for the other Modes.
    WriteOrCritical,
}

impl Authorization {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "read" => Self::Read,
            "write" => Self::Write,
            "critical" => Self::Critical,
            "cancel-poweroff" => Self::CancelPoweroff,
            "write-or-critical" => Self::WriteOrCritical,
            _ => return None,
        })
    }

    /// The polkit action to ask, or `None` when the method needs no check.
    /// `mode` is the first argument of a `write-or-critical` method.
    pub fn action(self, mode: Option<&str>) -> Option<Action> {
        match self {
            Self::Read => None,
            Self::Write => Some(Action::Write),
            Self::Critical => Some(Action::Critical),
            Self::CancelPoweroff => Some(Action::CancelPoweroff),
            Self::WriteOrCritical if mode == Some("release") => Some(Action::Critical),
            Self::WriteOrCritical => Some(Action::Write),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum MapError {
    Xml(String),
    /// The methods without an annotation, as `Interface.Method`.
    MissingAnnotation(Vec<String>),
    UnknownValue {
        method: String,
        value: String,
    },
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Xml(e) => write!(f, "the interface XML does not parse: {e}"),
            Self::MissingAnnotation(methods) => {
                write!(
                    f,
                    "methods without an authorization annotation: {}",
                    methods.join(", ")
                )
            }
            Self::UnknownValue { method, value } => {
                write!(f, "{method} has the unknown authorization value {value}")
            }
        }
    }
}

impl std::error::Error for MapError {}

/// From `(interface, method)` to its authorization. The interface is the full name.
#[derive(Debug)]
pub struct AuthzMap(BTreeMap<(String, String), Authorization>);

impl AuthzMap {
    /// Fails when a method has no annotation or an unknown value.
    pub fn parse(xml: &str) -> Result<Self, MapError> {
        let options = roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        };
        let doc = roxmltree::Document::parse_with_options(xml, options)
            .map_err(|e| MapError::Xml(e.to_string()))?;
        let mut map = BTreeMap::new();
        let mut missing = Vec::new();
        for interface in doc.descendants().filter(|n| n.has_tag_name("interface")) {
            let iface = interface.attribute("name").unwrap_or_default();
            for method in interface.children().filter(|n| n.has_tag_name("method")) {
                let name = method.attribute("name").unwrap_or_default();
                let value = method
                    .children()
                    .find(|n| {
                        n.has_tag_name("annotation") && n.attribute("name") == Some(ANNOTATION)
                    })
                    .and_then(|n| n.attribute("value"));
                let Some(value) = value else {
                    missing.push(format!("{iface}.{name}"));
                    continue;
                };
                let authorization =
                    Authorization::parse(value).ok_or_else(|| MapError::UnknownValue {
                        method: format!("{iface}.{name}"),
                        value: value.to_owned(),
                    })?;
                map.insert((iface.to_owned(), name.to_owned()), authorization);
            }
        }
        if !missing.is_empty() {
            return Err(MapError::MissingAnnotation(missing));
        }
        Ok(Self(map))
    }

    /// The map of the shipped contract.
    pub fn embedded() -> &'static Self {
        static MAP: OnceLock<AuthzMap> = OnceLock::new();
        MAP.get_or_init(|| Self::parse(crate::INTERFACE_XML).expect("the shipped XML is valid"))
    }

    /// `interface` is the full name or the short name (`Channel`).
    pub fn get(&self, interface: &str, method: &str) -> Option<Authorization> {
        let full = if interface.contains('.') {
            interface.to_owned()
        } else {
            format!("{}.{interface}", crate::INTERFACE_PREFIX)
        };
        self.0.get(&(full, method.to_owned())).copied()
    }

    pub fn methods(&self) -> impl Iterator<Item = (&str, &str, Authorization)> {
        self.0
            .iter()
            .map(|((i, m), a)| (i.as_str(), m.as_str(), *a))
    }

    /// The polkit actions that some method of the contract asks.
    pub fn actions(&self) -> BTreeSet<Action> {
        let mut actions: BTreeSet<_> = self.0.values().filter_map(|a| a.action(None)).collect();
        if self
            .0
            .values()
            .any(|a| *a == Authorization::WriteOrCritical)
        {
            actions.insert(Action::Critical);
        }
        actions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> &'static AuthzMap {
        AuthzMap::embedded()
    }

    fn asks(interface: &str, method: &str, mode: Option<&str>) -> Option<Action> {
        map()
            .get(interface, method)
            .unwrap_or_else(|| panic!("{interface}.{method} has no entry"))
            .action(mode)
    }

    #[test]
    fn every_method_has_annotation() {
        // The shipped XML passes.
        assert!(map().methods().count() >= 51);
        // A method without the annotation fails the parse and is named.
        let broken = crate::INTERFACE_XML.replacen(
            r#"<annotation name="io.github.cappyt.NiceCooling1.Authorization" value="write"/>"#,
            "",
            1,
        );
        let Err(MapError::MissingAnnotation(methods)) = AuthzMap::parse(&broken) else {
            panic!("a method without annotation must fail")
        };
        assert_eq!(
            methods,
            ["io.github.cappyt.NiceCooling1.Manager.CreateDerivedSensor"]
        );
        let unknown = crate::INTERFACE_XML.replacen("value=\"write\"", "value=\"admin\"", 1);
        assert!(matches!(
            AuthzMap::parse(&unknown),
            Err(MapError::UnknownValue { .. })
        ));
    }

    #[test]
    fn method_asks_its_annotated_action() {
        assert_eq!(asks("Channel", "SetCurve", None), Some(Action::Write));
        assert_eq!(asks("Sensor", "SetName", None), Some(Action::Write));
        assert_eq!(asks("Manager", "GetCalibrationTables", None), None);
        assert_eq!(asks("Leakshield", "GetFields", None), None);
        assert_eq!(asks("Profile", "GetContent", None), None);
        assert_eq!(
            asks("io.github.cappyt.NiceCooling1.Loop", "Delete", None),
            Some(Action::Write)
        );
        assert_eq!(map().get("Channel", "NoSuchMethod"), None);
    }

    #[test]
    fn set_mode_release_is_critical() {
        assert_eq!(
            asks("Leakshield", "SetMode", Some("release")),
            Some(Action::Critical)
        );
        assert_eq!(
            asks("Leakshield", "SetMode", Some("shield")),
            Some(Action::Write)
        );
        assert_eq!(
            asks("Leakshield", "SetMode", Some("monitor")),
            Some(Action::Write)
        );
        assert_eq!(asks("Leakshield", "SetMode", None), Some(Action::Write));
    }

    #[test]
    fn factory_reset_and_restore_are_critical() {
        assert_eq!(
            asks("Leakshield", "FactoryReset", None),
            Some(Action::Critical)
        );
        assert_eq!(
            asks("Leakshield", "RestoreBackup", None),
            Some(Action::Critical)
        );
        let critical: Vec<_> = map()
            .methods()
            .filter(|(_, _, a)| a.action(None) == Some(Action::Critical))
            .map(|(_, m, _)| m)
            .collect();
        assert_eq!(critical, ["FactoryReset", "RestoreBackup"]);
    }

    #[test]
    fn cancel_poweroff_uses_cancel_action() {
        assert_eq!(
            asks("Alarms", "CancelPoweroff", None),
            Some(Action::CancelPoweroff)
        );
        assert_eq!(
            Action::CancelPoweroff.id(),
            "io.github.cappyt.NiceCooling1.cancel-poweroff"
        );
        let users: Vec<_> = map()
            .methods()
            .filter(|(_, _, a)| a.action(None) == Some(Action::CancelPoweroff))
            .collect();
        assert_eq!(users.len(), 1);
    }

    #[test]
    fn alarm_responses_and_pump_floor_are_write() {
        assert_eq!(asks("Alarms", "SetResponse", None), Some(Action::Write));
        assert_eq!(asks("Alarms", "GetResponses", None), None);
        assert_eq!(asks("Loop", "SetPumpFloor", None), Some(Action::Write));
        assert_eq!(asks("Channel", "SetFixedDuty", None), Some(Action::Write));
    }

    #[test]
    fn the_three_actions_of_the_contract() {
        let ids: Vec<_> = map().actions().into_iter().map(Action::id).collect();
        assert_eq!(
            ids,
            [
                "io.github.cappyt.NiceCooling1.write",
                "io.github.cappyt.NiceCooling1.critical",
                "io.github.cappyt.NiceCooling1.cancel-poweroff"
            ]
        );
    }

    #[test]
    fn policy_actions_equal_xml_annotations() {
        let policy = include_str!("../../../packaging/polkit/io.github.cappyt.NiceCooling1.policy");
        let doc = roxmltree::Document::parse_with_options(
            policy,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .unwrap();
        let in_policy: BTreeSet<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("action"))
            .filter_map(|n| n.attribute("id"))
            .collect();
        let in_xml: BTreeSet<_> = map().actions().into_iter().map(Action::id).collect();
        assert_eq!(in_policy, in_xml);
        assert_eq!(in_policy.len(), 3);
    }
}
