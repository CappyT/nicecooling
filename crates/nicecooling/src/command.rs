//! What a tray entry, a notification button, a banner button or a screen asks the app to do.

use crate::daemon_client::DaemonClient;
use crate::model::{DaemonEvent, Derived, FieldValue, Response, RuleGroup};
use crate::shell::Page;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    CancelPoweroff {
        instance: String,
    },
    ActivateProfile {
        id: String,
    },
    ReturnToAutomatic,
    /// Show the window, on a page or on the last one.
    OpenWindow(Option<Page>),
    Quit,
    /// An empty name restores the default name.
    SetSensorName {
        sensor: String,
        name: String,
    },
    CreateDerivedSensor {
        name: String,
        function: String,
        sources: Vec<String>,
    },
    SetDerivedDefinition {
        sensor: String,
        function: String,
        sources: Vec<String>,
    },
    DeleteDerivedSensor {
        sensor: String,
    },
    SetChannelName {
        channel: String,
        name: String,
    },
    SetCurve {
        channel: String,
        input: String,
        points: Vec<(f64, f64)>,
        smoothing_s: f64,
        step_limit: f64,
    },
    SetFixedDuty {
        channel: String,
        duty: f64,
    },
    ReleaseToHardware {
        channel: String,
    },
    CreateLoop {
        name: String,
    },
    SetLoopName {
        loop_id: String,
        name: String,
    },
    SetPumpChannels {
        loop_id: String,
        channels: Vec<String>,
    },
    /// An empty sensor clears the flow sensor.
    SetFlowSensor {
        loop_id: String,
        sensor: String,
        table: String,
    },
    SetWaterTemperature {
        loop_id: String,
        sensor: String,
    },
    SetLoopLeakshield {
        loop_id: String,
        leakshield: String,
    },
    SetPumpFloor {
        loop_id: String,
        duty: f64,
    },
    DeleteLoop {
        loop_id: String,
    },
    /// The First run flow: create a Loop and link its parts. An empty text is "none".
    SetupLoop {
        name: String,
        pump_channels: Vec<String>,
        flow_sensor: String,
        table: String,
        water: String,
        leakshield: String,
        floor: f64,
    },
    /// An empty `copy_from` copies the active Profile.
    CreateProfile {
        name: String,
        copy_from: String,
    },
    SetProfileName {
        profile: String,
        name: String,
    },
    SetProfileSlot {
        profile: String,
        slot: u8,
    },
    /// `groups` is the list without the groups that point to the Profile: the daemon refuses
    /// the delete while a group uses it. `None` deletes only the Profile.
    DeleteProfile {
        profile: String,
        groups: Option<Vec<RuleGroup>>,
    },
    SetImmediateSwitching {
        enabled: bool,
    },
    SetRuleTiming {
        rule_delay_s: u32,
        dwell_s: u32,
    },
    /// The whole ordered list. An empty group id creates a group.
    SetRuleGroups {
        groups: Vec<RuleGroup>,
    },
    /// `source` is `""` for the default of the event, or a Leakshield id for its override.
    ResetResponse {
        event: String,
        source: String,
    },
    /// `source` is `""` for the default of the event, or a Leakshield id for its override.
    SetAlarmResponse {
        event: String,
        source: String,
        response: Response,
        delay_s: u32,
    },
    /// Change fields of a stored slot. The daemon sends nothing to the device.
    SetStoredFields {
        leakshield: String,
        slot: u8,
        fields: Vec<(String, FieldValue)>,
    },
    /// Restore the Leakshield backup after a failed write. Needs the polkit action `critical`.
    RestoreBackup {
        leakshield: String,
        backup: String,
    },
    /// One write path run for all fields. `control` names the control that shows the result.
    SetLeakshieldFields {
        leakshield: String,
        control: String,
        fields: Vec<(String, FieldValue)>,
    },
    /// `shield`, `monitor` or `release`. `release` needs the polkit action `critical`.
    ChangeLeakshieldMode {
        leakshield: String,
        mode: String,
    },
    /// `test`, `measure-fill-level` or `deaerate`. A Test writes its pressure first.
    StartLeakshieldAction {
        leakshield: String,
        action: String,
        test_pressure: Option<f64>,
    },
    CancelLeakshieldAction {
        leakshield: String,
    },
    SyncLeakshieldClock {
        leakshield: String,
    },
    /// Clears the latched alarm bits of the device (`ResetAlarms`).
    ResetLeakshieldAlarms {
        leakshield: String,
    },
    ResetRefillHistory {
        leakshield: String,
    },
    ResetServiceCounter {
        leakshield: String,
    },
    /// An empty sensor clears the slot.
    SetSoftwareSensor {
        leakshield: String,
        slot: u16,
        sensor: String,
    },
    CreateBackup {
        leakshield: String,
    },
    /// Save the three reports of a backup in a text file.
    ExportBackup {
        leakshield: String,
        backup: String,
        path: std::path::PathBuf,
    },
    /// Add the backup of a file to the list of the Leakshield. It does not write the device.
    ImportBackup {
        leakshield: String,
        path: std::path::PathBuf,
    },
    /// Needs the polkit action `critical`.
    FactoryReset {
        leakshield: String,
    },
    /// Save the Calibration table of a Loop. A preset is read-only: the points go into a new
    /// user table, and the Loop uses it from then on.
    SaveCalibration {
        loop_id: String,
        sensor: String,
        /// The table that the Loop uses now. Empty when it has none.
        table: String,
        preset: bool,
        name: String,
        points: Vec<(f64, f64)>,
    },
}

impl Command {
    /// Run the commands that call the daemon. A window command returns `Ok(false)`: the
    /// window code handles it.
    pub async fn run_on_daemon(&self, client: &DaemonClient) -> zbus::Result<bool> {
        match self {
            Self::CancelPoweroff { instance } => client.cancel_poweroff(instance).await?,
            Self::ActivateProfile { id } => client.activate_profile(id).await?,
            Self::ReturnToAutomatic => client.return_to_automatic().await?,
            Self::OpenWindow(_) | Self::Quit => return Ok(false),
            Self::SetSensorName { sensor, name } => client.set_sensor_name(sensor, name).await?,
            Self::CreateDerivedSensor {
                name,
                function,
                sources,
            } => {
                client
                    .create_derived_sensor(name, function, sources)
                    .await?;
            }
            Self::SetDerivedDefinition {
                sensor,
                function,
                sources,
            } => {
                client
                    .set_derived_definition(sensor, function, sources)
                    .await?
            }
            Self::DeleteDerivedSensor { sensor } => client.delete_derived_sensor(sensor).await?,
            Self::SetChannelName { channel, name } => {
                client.set_channel_name(channel, name).await?
            }
            Self::SetCurve {
                channel,
                input,
                points,
                smoothing_s,
                step_limit,
            } => {
                client
                    .set_curve(channel, input, points, *smoothing_s, *step_limit)
                    .await?
            }
            Self::SetFixedDuty { channel, duty } => client.set_fixed_duty(channel, *duty).await?,
            Self::ReleaseToHardware { channel } => client.release_to_hardware(channel).await?,
            Self::CreateLoop { name } => {
                client.create_loop(name).await?;
            }
            Self::SetLoopName { loop_id, name } => client.set_loop_name(loop_id, name).await?,
            Self::SetPumpChannels { loop_id, channels } => {
                client.set_pump_channels(loop_id, channels).await?
            }
            Self::SetFlowSensor {
                loop_id,
                sensor,
                table,
            } => client.set_flow_sensor(loop_id, sensor, table).await?,
            Self::SetWaterTemperature { loop_id, sensor } => {
                client.set_water_temperature(loop_id, sensor).await?
            }
            Self::SetLoopLeakshield {
                loop_id,
                leakshield,
            } => client.set_loop_leakshield(loop_id, leakshield).await?,
            Self::SetPumpFloor { loop_id, duty } => client.set_pump_floor(loop_id, *duty).await?,
            Self::DeleteLoop { loop_id } => client.delete_loop(loop_id).await?,
            Self::SetupLoop {
                name,
                pump_channels,
                flow_sensor,
                table,
                water,
                leakshield,
                floor,
            } => {
                client
                    .setup_loop(
                        name,
                        pump_channels,
                        flow_sensor,
                        table,
                        water,
                        leakshield,
                        *floor,
                    )
                    .await?;
            }
            Self::CreateProfile { name, copy_from } => {
                client.create_profile(name, copy_from).await?;
            }
            Self::SetProfileName { profile, name } => {
                client.set_profile_name(profile, name).await?
            }
            Self::SetProfileSlot { profile, slot } => {
                client.set_profile_slot(profile, *slot).await?
            }
            Self::DeleteProfile { profile, groups } => {
                if let Some(groups) = groups {
                    client.set_rule_groups(groups).await?;
                }
                client.delete_profile(profile).await?
            }
            Self::SetImmediateSwitching { enabled } => {
                client.set_immediate_switching(*enabled).await?
            }
            Self::SetRuleTiming {
                rule_delay_s,
                dwell_s,
            } => client.set_rule_timing(*rule_delay_s, *dwell_s).await?,
            Self::SetRuleGroups { groups } => {
                client.set_rule_groups(groups).await?;
            }
            Self::ResetResponse { event, source } => {
                client.reset_alarm_response(event, source).await?
            }
            Self::SetAlarmResponse {
                event,
                source,
                response,
                delay_s,
            } => {
                client
                    .set_alarm_response(event, source, *response, *delay_s)
                    .await?
            }
            Self::SetStoredFields {
                leakshield,
                slot,
                fields,
            } => client.set_stored_fields(leakshield, *slot, fields).await?,
            Self::RestoreBackup { leakshield, backup } => {
                client.restore_backup(leakshield, backup).await?
            }
            Self::SetLeakshieldFields {
                leakshield, fields, ..
            } => client.set_leakshield_fields(leakshield, fields).await?,
            Self::ChangeLeakshieldMode { leakshield, mode } => {
                client.set_leakshield_mode(leakshield, mode).await?
            }
            Self::StartLeakshieldAction {
                leakshield,
                action,
                test_pressure,
            } => {
                if let Some(pressure) = test_pressure {
                    client
                        .set_leakshield_fields(
                            leakshield,
                            &[("test_pressure".to_owned(), FieldValue::Number(*pressure))],
                        )
                        .await?;
                }
                client.start_leakshield_action(leakshield, action).await?
            }
            Self::CancelLeakshieldAction { leakshield } => {
                client.leakshield_call(leakshield, "CancelAction").await?
            }
            Self::SyncLeakshieldClock { leakshield } => {
                client.leakshield_call(leakshield, "SyncClock").await?
            }
            Self::ResetLeakshieldAlarms { leakshield } => {
                client.leakshield_call(leakshield, "ResetAlarms").await?
            }
            Self::ResetRefillHistory { leakshield } => {
                client
                    .leakshield_call(leakshield, "ResetRefillHistory")
                    .await?
            }
            Self::ResetServiceCounter { leakshield } => {
                client
                    .leakshield_call(leakshield, "ResetServiceCounter")
                    .await?
            }
            Self::SetSoftwareSensor {
                leakshield,
                slot,
                sensor,
            } => {
                client
                    .set_software_sensor(leakshield, *slot, sensor)
                    .await?
            }
            Self::CreateBackup { leakshield } => {
                client.leakshield_call(leakshield, "CreateBackup").await?
            }
            Self::ExportBackup {
                leakshield,
                backup,
                path,
            } => {
                let reports = client.get_backup(leakshield, backup).await?;
                std::fs::write(
                    path,
                    crate::screens::leakshield::system::backup_file(&reports),
                )
                .map_err(|error| zbus::Error::Failure(error.to_string()))?;
            }
            Self::ImportBackup { leakshield, path } => {
                let text = std::fs::read_to_string(path)
                    .map_err(|error| zbus::Error::Failure(error.to_string()))?;
                let reports = crate::screens::leakshield::system::parse_backup_file(&text)
                    .ok_or_else(|| zbus::Error::Failure("not a backup file".into()))?;
                client.import_backup(leakshield, &reports).await?;
            }
            Self::FactoryReset { leakshield } => {
                client.leakshield_call(leakshield, "FactoryReset").await?
            }
            Self::SaveCalibration {
                loop_id,
                sensor,
                table,
                preset,
                name,
                points,
            } => {
                let existing = if *preset { "" } else { table.as_str() };
                let saved = client.set_calibration_table(existing, name, points).await?;
                if saved != *table {
                    client.set_flow_sensor(loop_id, sensor, &saved).await?;
                }
            }
        }
        Ok(true)
    }

    /// The Leakshield that this command writes. The app reads the device again after it.
    pub fn leakshield(&self) -> Option<&str> {
        match self {
            Self::RestoreBackup { leakshield, .. }
            | Self::SetStoredFields { leakshield, .. }
            | Self::SetLeakshieldFields { leakshield, .. }
            | Self::ChangeLeakshieldMode { leakshield, .. }
            | Self::StartLeakshieldAction { leakshield, .. }
            | Self::CancelLeakshieldAction { leakshield }
            | Self::SyncLeakshieldClock { leakshield }
            | Self::ResetLeakshieldAlarms { leakshield }
            | Self::ResetRefillHistory { leakshield }
            | Self::ResetServiceCounter { leakshield }
            | Self::SetSoftwareSensor { leakshield, .. }
            | Self::CreateBackup { leakshield }
            | Self::ExportBackup { leakshield, .. }
            | Self::ImportBackup { leakshield, .. }
            | Self::FactoryReset { leakshield } => Some(leakshield),
            _ => None,
        }
    }

    /// What the screens need to read again after this command ran, when the daemon sends no
    /// signal for the change.
    pub async fn refresh(&self, client: &DaemonClient) -> Option<DaemonEvent> {
        match self {
            Self::SetAlarmResponse { .. } | Self::ResetResponse { .. } => {
                client.responses().await.ok().map(DaemonEvent::Responses)
            }
            // `RuleGroupState` changes with the ids, the order and the states of the groups, not
            // with their rules or targets.
            Self::SetRuleGroups { .. } => client.rule_groups().await.ok().map(|groups| {
                DaemonEvent::Derived(Box::new(Derived {
                    rule_groups: Some(groups),
                    ..Derived::default()
                }))
            }),
            Self::SaveCalibration { .. } => client.calibration_tables().await.ok().map(|tables| {
                DaemonEvent::Derived(Box::new(Derived {
                    calibration_tables: Some(tables),
                    ..Derived::default()
                }))
            }),
            _ => None,
        }
    }

    /// The control that shows the result of this command. `None` for a command without
    /// write feedback.
    pub fn feedback_key(&self) -> Option<String> {
        Some(match self {
            Self::SetSensorName { sensor, .. } => format!("sensor-name:{sensor}"),
            Self::CreateDerivedSensor { .. } => "derived:new".to_owned(),
            Self::SetDerivedDefinition { sensor, .. } | Self::DeleteDerivedSensor { sensor } => {
                format!("derived:{sensor}")
            }
            Self::SetChannelName { channel, .. } => format!("channel-name:{channel}"),
            Self::SetCurve { channel, .. }
            | Self::SetFixedDuty { channel, .. }
            | Self::ReleaseToHardware { channel } => format!("channel:{channel}"),
            Self::CreateLoop { .. } => "loop:new".to_owned(),
            Self::SetLoopName { loop_id, .. } => format!("loop-name:{loop_id}"),
            Self::SetPumpChannels { loop_id, .. } => format!("loop-pumps:{loop_id}"),
            Self::SetFlowSensor { loop_id, .. } => format!("loop-flow:{loop_id}"),
            Self::SetWaterTemperature { loop_id, .. } => format!("loop-water:{loop_id}"),
            Self::SetLoopLeakshield { loop_id, .. } => format!("loop-leakshield:{loop_id}"),
            Self::SetPumpFloor { loop_id, .. } => format!("loop-floor:{loop_id}"),
            Self::DeleteLoop { loop_id } => format!("loop:{loop_id}"),
            Self::SaveCalibration { loop_id, .. } => format!("table:{loop_id}"),
            Self::SetupLoop { .. } => "loop:new".to_owned(),
            Self::CreateProfile { .. } => "profile:new".to_owned(),
            Self::SetProfileName { profile, .. } => format!("profile-name:{profile}"),
            Self::SetProfileSlot { profile, .. } => format!("profile-slot:{profile}"),
            Self::DeleteProfile { profile, .. } => format!("profile:{profile}"),
            Self::ActivateProfile { .. } | Self::ReturnToAutomatic => {
                "profile-activation".to_owned()
            }
            Self::SetImmediateSwitching { .. } => "profile-immediate".to_owned(),
            Self::SetRuleTiming { .. } => "profile-timing".to_owned(),
            Self::SetRuleGroups { .. } => "rule-groups".to_owned(),
            Self::SetAlarmResponse { event, source, .. }
            | Self::ResetResponse { event, source } => response_feedback_key(event, source),
            Self::RestoreBackup { leakshield, .. } => format!("restore:{leakshield}"),
            Self::SetStoredFields {
                leakshield, slot, ..
            } => format!("stored:{leakshield}:{slot}"),
            Self::SetLeakshieldFields {
                leakshield,
                control,
                ..
            } => format!("ls:{leakshield}:{control}"),
            Self::ChangeLeakshieldMode { leakshield, .. } => format!("ls:{leakshield}:mode"),
            Self::StartLeakshieldAction { leakshield, .. }
            | Self::CancelLeakshieldAction { leakshield } => format!("ls:{leakshield}:action"),
            Self::SyncLeakshieldClock { leakshield } => format!("ls:{leakshield}:clock"),
            Self::ResetLeakshieldAlarms { leakshield } => format!("ls:{leakshield}:alarm-reset"),
            Self::ResetRefillHistory { leakshield } => format!("ls:{leakshield}:refill-history"),
            Self::ResetServiceCounter { leakshield } => format!("ls:{leakshield}:service-counter"),
            Self::SetSoftwareSensor {
                leakshield, slot, ..
            } => format!("ls:{leakshield}:software-sensor:{slot}"),
            Self::CreateBackup { leakshield }
            | Self::ExportBackup { leakshield, .. }
            | Self::ImportBackup { leakshield, .. } => format!("ls:{leakshield}:backups"),
            Self::FactoryReset { leakshield } => format!("ls:{leakshield}:factory-reset"),
            Self::CancelPoweroff { instance } => format!("poweroff:{instance}"),
            Self::OpenWindow(_) | Self::Quit => return None,
        })
    }
}

/// The feedback key of a response row: `alarm-response:<event>` or `alarm-response:<event>@<id>`.
pub fn response_feedback_key(event: &str, source: &str) -> String {
    if source.is_empty() {
        format!("alarm-response:{event}")
    } else {
        format!("alarm-response:{event}@{source}")
    }
}
