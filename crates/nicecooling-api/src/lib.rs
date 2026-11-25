//! The D-Bus contract of nicecoolingd: identifiers, errors, the authorization map and one typed
//! proxy for each interface. `dbus/io.github.cappyt.NiceCooling1.xml` is the source of truth.

pub mod alarms;
pub mod authz_map;
pub mod board;
pub mod channel;
pub mod derived_sensor;
pub mod errors;
pub mod graph_sensor;
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
    use std::collections::BTreeSet;

    /// Every method, signal and property of the contract before the Graph sensor. The contract
    /// only grows: a client of an earlier release keeps working.
    const OLD_MEMBERS: &str = "
        Manager.Version
        Manager.StateHealth
        Manager.CreateDerivedSensor
        Manager.CreateLoop
        Manager.GetCalibrationTables
        Manager.SetCalibrationTable
        Manager.DeleteCalibrationTable
        Manager.SensorsUpdated
        Board.Vendor
        Board.Model
        Board.BoardFile
        Sensor.Id
        Sensor.Name
        Sensor.DefaultName
        Sensor.Kind
        Sensor.Unit
        Sensor.Origin
        Sensor.Driver
        Sensor.Value
        Sensor.HasValue
        Sensor.SetName
        DerivedSensor.Type
        DerivedSensor.Sources
        DerivedSensor.SetDefinition
        DerivedSensor.Delete
        Channel.Id
        Channel.Name
        Channel.DefaultName
        Channel.Duty
        Channel.Raw
        Channel.Control
        Channel.Override
        Channel.IsPump
        Channel.ManualAtSnapshot
        Channel.Curve
        Channel.FixedDuty
        Channel.BiosSnapshot
        Channel.FanSensor
        Channel.SetName
        Channel.SetCurve
        Channel.SetFixedDuty
        Channel.ReleaseToHardware
        Loop.Id
        Loop.Name
        Loop.PumpChannels
        Loop.FlowSensor
        Loop.CalibrationTable
        Loop.Flow
        Loop.WaterTemperature
        Loop.Leakshield
        Loop.PumpFloor
        Loop.SetName
        Loop.SetPumpChannels
        Loop.SetFlowSensor
        Loop.SetWaterTemperature
        Loop.SetLeakshield
        Loop.SetPumpFloor
        Loop.Delete
        Leakshield.Id
        Leakshield.Serial
        Leakshield.Firmware
        Leakshield.Structure
        Leakshield.Connected
        Leakshield.Writable
        Leakshield.ReadOnlyReason
        Leakshield.Mode
        Leakshield.RunningAction
        Leakshield.ClockOffset
        Leakshield.SoftwareSensorSlots
        Leakshield.FeedState
        Leakshield.LastFeed
        Leakshield.ActiveSlot
        Leakshield.FailedWriteBackup
        Leakshield.GetFields
        Leakshield.SetFields
        Leakshield.GetStoredFields
        Leakshield.SetStoredFields
        Leakshield.SetMode
        Leakshield.StartAction
        Leakshield.CancelAction
        Leakshield.SyncClock
        Leakshield.ResetAlarms
        Leakshield.ResetRefillHistory
        Leakshield.ResetEventLog
        Leakshield.ToggleRefillPause
        Leakshield.ResetServiceCounter
        Leakshield.GetCounters
        Leakshield.GetRefillHistory
        Leakshield.GetEvents
        Leakshield.GetSoftwareSensors
        Leakshield.SetSoftwareSensor
        Leakshield.GetDisplayPreview
        Leakshield.GetDisplayAssets
        Leakshield.SetDisplayName
        Leakshield.SetDisplayIcon
        Leakshield.SetDisplayLogo
        Leakshield.GetSoftwareSensorScales
        Leakshield.SetSoftwareSensorScale
        Leakshield.GetSoftwareSensorSources
        Leakshield.ListBackups
        Leakshield.CreateBackup
        Leakshield.GetBackup
        Leakshield.ImportBackup
        Leakshield.RestoreBackup
        Leakshield.FactoryReset
        Leakshield.GetDeviceResponseProposals
        Leakshield.ExternalChange
        Leakshield.WriteFailed
        Profiles.Active
        Profiles.ActivationCause
        Profiles.ActiveState
        Profiles.Default
        Profiles.ImmediateSwitching
        Profiles.RuleDelaySeconds
        Profiles.DwellSeconds
        Profiles.DwellUntil
        Profiles.CreateProfile
        Profiles.ActivateProfile
        Profiles.ReturnToAutomatic
        Profiles.SetImmediateSwitching
        Profiles.SetRuleTiming
        Profiles.RuleGroupState
        Profiles.GetRuleGroups
        Profiles.SetRuleGroups
        Profiles.ProfileActivated
        Profile.Id
        Profile.Name
        Profile.IsDefault
        Profile.ContainsRelease
        Profile.Slot
        Profile.SetName
        Profile.SetSlot
        Profile.Delete
        Profile.GetContent
        Alarms.Active
        Alarms.GetResponses
        Alarms.SetResponse
        Alarms.ResetResponse
        Alarms.GetHistory
        Alarms.CancelPoweroff
        Alarms.AlarmRaised
        Alarms.AlarmCleared
        Alarms.PoweroffCancelled
        Alarms.PoweroffFailed
    ";

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

    #[test]
    fn no_member_removed() {
        let doc = roxmltree::Document::parse_with_options(
            super::INTERFACE_XML,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        )
        .unwrap();
        let mut now = BTreeSet::new();
        for interface in doc.descendants().filter(|n| n.has_tag_name("interface")) {
            let name = interface.attribute("name").unwrap();
            let name = name
                .strip_prefix("io.github.cappyt.NiceCooling1.")
                .unwrap_or(name);
            for member in interface.children().filter(|n| {
                n.has_tag_name("method") || n.has_tag_name("signal") || n.has_tag_name("property")
            }) {
                now.insert(format!("{name}.{}", member.attribute("name").unwrap()));
            }
        }
        let old: BTreeSet<_> = OLD_MEMBERS.split_whitespace().map(str::to_owned).collect();
        assert_eq!(old.len(), 144);
        let removed: Vec<_> = old.difference(&now).collect();
        assert!(removed.is_empty(), "removed members: {removed:?}");
    }
}
