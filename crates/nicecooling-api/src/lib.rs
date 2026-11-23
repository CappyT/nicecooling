//! The D-Bus contract of nicecoolingd: identifiers, errors, the authorization map and one typed
//! proxy for each interface. `dbus/io.github.cappyt.NiceCooling1.xml` is the source of truth.

pub mod alarms;
pub mod authz_map;
pub mod board;
pub mod channel;
pub mod derived_sensor;
pub mod errors;
pub mod ids;
pub mod leakshield;
pub mod loops;
pub mod manager;
pub mod profile;
pub mod profiles;
pub mod sensor;

pub use errors::Error;
pub use ids::{BUS_NAME, ROOT_PATH};

/// The interface contract, as introspection XML with the authorization annotations.
pub const INTERFACE_XML: &str = include_str!("../dbus/io.github.cappyt.NiceCooling1.xml");

/// The prefix of every interface name of the contract.
pub const INTERFACE_PREFIX: &str = "io.github.cappyt.NiceCooling1";

#[cfg(test)]
mod tests {
    #[test]
    fn no_method_name_contains_verif() {
        let doc = roxmltree::Document::parse_with_options(
            super::INTERFACE_XML,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .unwrap();
        let methods: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("method"))
            .filter_map(|n| n.attribute("name"))
            .collect();
        assert!(methods.len() >= 51, "{} methods", methods.len());
        let offenders: Vec<_> = methods
            .iter()
            .filter(|m| m.to_ascii_lowercase().contains("verif"))
            .collect();
        assert!(offenders.is_empty(), "{offenders:?}");
    }
}
