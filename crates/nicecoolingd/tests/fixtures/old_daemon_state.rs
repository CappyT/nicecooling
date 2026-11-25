//! `DaemonState` as `state.json` had it before the Graph sensor: no `graph_sensors` and no
//! `graph_sensor_next`. A copy of the type of the daemon of that release, so a test can load and
//! save a new state file the way an older daemon does after a downgrade. Keep it as it is: it
//! stands for code that is already released.

use std::collections::BTreeMap;

use nicecooling_core::derived::DerivedSensor;
use nicecooling_core::loops::Loop;
use nicecooling_core::rules::group::RuleGroupDef;
use nicecoolingd::state::{AlarmResponse, CalibrationDef, Identity, ProfilesState, SlotScale};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OldDaemonState {
    pub schema_version: u32,
    pub identities: BTreeMap<String, Identity>,
    pub names: BTreeMap<String, String>,
    pub derived_sensors: Vec<DerivedSensor>,
    pub calibration_tables: Vec<CalibrationDef>,
    pub loops: Vec<Loop>,
    pub alarm_responses: BTreeMap<String, AlarmResponse>,
    pub rule_groups: Vec<RuleGroupDef>,
    pub software_sensors: BTreeMap<String, BTreeMap<u16, String>>,
    pub software_sensor_scales: BTreeMap<String, BTreeMap<u16, SlotScale>>,
    pub profiles: ProfilesState,
}

impl Default for OldDaemonState {
    fn default() -> Self {
        Self {
            schema_version: 1,
            identities: BTreeMap::new(),
            names: BTreeMap::new(),
            derived_sensors: Vec::new(),
            calibration_tables: Vec::new(),
            loops: Vec::new(),
            alarm_responses: BTreeMap::new(),
            rule_groups: Vec::new(),
            software_sensors: BTreeMap::new(),
            software_sensor_scales: BTreeMap::new(),
            profiles: ProfilesState::default(),
        }
    }
}
