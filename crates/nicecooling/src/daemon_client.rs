//! The client of the daemon contract `io.github.cappyt.NiceCooling1`. The app reaches the
//! daemon only through D-Bus. The connection is a parameter: the binary gives the system bus
//! and a test gives a private bus.

use std::collections::HashMap;
use std::time::Duration;

use futures_util::StreamExt;
use tokio::sync::broadcast;
use zbus::message::Type;
use zbus::names::BusName;
use zbus::proxy::MethodFlags;
use zbus::{Connection, MatchRule, MessageStream, Proxy};
use zbus::{OwnedObjectPath, OwnedVariant as OwnedValue, Variant as Value};

use crate::model::{
    AlarmDetails, AlarmEntry, BackupEntry, BoardInfo, CalibrationTableInfo, Cause, ChannelInfo,
    ChannelOverride, Control, CurveSpec, DaemonEvent, DerivedDef, DeviceEvent, FieldEntry,
    FieldRisk, FieldValue, LeakshieldData, LeakshieldInfo, Link, LoopInfo, ProfileChannel,
    ProfileContent, ProfileInfo, Response, ResponseProposal, ResponseRule, Rule, RuleGroup,
    SensorInfo, SensorSample, Severity, Snapshot, UnitState,
};

pub const BUS_NAME: &str = "io.github.cappyt.NiceCooling1";
pub const ROOT: &str = "/io/github/cappyt/NiceCooling1";
const IFACE: &str = "io.github.cappyt.NiceCooling1";
pub const UNIT: &str = "nicecoolingd.service";

const SYSTEMD: &str = "org.freedesktop.systemd1";
const SYSTEMD_PATH: &str = "/org/freedesktop/systemd1";

/// How often the client looks for the daemon on the bus while it is away.
const OFFLINE_POLL: Duration = Duration::from_secs(1);

type Props = HashMap<String, OwnedValue>;
/// One row of `GetCalibrationTables`: id, name, preset flag and the points.
type CalibrationRow = (String, String, bool, Vec<(f64, f64)>);
/// One row of `GetRuleGroups`: group id, target Profile, enabled, and the rules.
type RuleGroupRow = (String, OwnedObjectPath, bool, Vec<(String, Props)>);
/// A Rule group to send, with the parameters as variants.
type RuleGroupOut<'a> = (
    String,
    OwnedObjectPath,
    bool,
    Vec<(String, HashMap<String, Value<'a>>)>,
);
/// The body of `SensorsUpdated`: the time, then `(id, value, flags)` of sensors and Channels.
type SensorsUpdated = (u64, Vec<(String, f64, u8)>, Vec<(String, f64, u8)>);

#[derive(Clone)]
pub struct DaemonClient {
    conn: Connection,
}

impl DaemonClient {
    pub fn new(conn: Connection) -> Self {
        Self { conn }
    }

    /// Follow the daemon and send what happens. It runs until the receivers are gone.
    pub async fn run(self, tx: broadcast::Sender<DaemonEvent>) {
        let mut last_link = None;
        while tx.receiver_count() > 0 {
            if let Some(owner) = self.owner().await {
                // An error ends the session. The daemon may have left the bus: look again.
                let _ = self.session(&tx, &owner).await;
                last_link = Some(Link::Online);
                continue;
            }
            let link = self
                .unit_state()
                .await
                .map_or(Link::Offline, |unit| unit.link());
            if last_link != Some(link) {
                last_link = Some(link);
                let _ = tx.send(DaemonEvent::Link(link));
            }
            tokio::time::sleep(OFFLINE_POLL).await;
        }
    }

    /// The unique name that owns the daemon name, or `None` when the daemon is not on the bus.
    async fn owner(&self) -> Option<String> {
        let dbus = zbus::fdo::DBusProxy::new(&self.conn).await.ok()?;
        let name = BusName::try_from(BUS_NAME).ok()?;
        dbus.get_name_owner(name)
            .await
            .ok()
            .map(|owner| owner.to_string())
    }

    /// `ActiveState` and `Result` of `nicecoolingd.service`, read through systemd.
    pub async fn unit_state(&self) -> zbus::Result<UnitState> {
        let manager = Proxy::new(
            &self.conn,
            SYSTEMD,
            SYSTEMD_PATH,
            "org.freedesktop.systemd1.Manager",
        )
        .await?;
        let path: OwnedObjectPath = manager.call("LoadUnit", &(UNIT,)).await?;
        let unit = Proxy::new(
            &self.conn,
            SYSTEMD,
            path.clone(),
            "org.freedesktop.systemd1.Unit",
        )
        .await?;
        let service = Proxy::new(
            &self.conn,
            SYSTEMD,
            path,
            "org.freedesktop.systemd1.Service",
        )
        .await?;
        Ok(UnitState {
            active_state: unit.get_property("ActiveState").await?,
            result: service.get_property("Result").await?,
        })
    }

    async fn session(&self, tx: &broadcast::Sender<DaemonEvent>, owner: &str) -> zbus::Result<()> {
        let dbus = zbus::fdo::DBusProxy::new(&self.conn).await?;
        let mut owner_changes = dbus
            .receive_name_owner_changed_with_args(&[(0, BUS_NAME)])
            .await?;
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .path_namespace(ROOT)
            .build()?;
        let mut signals = MessageStream::for_match_rule(rule, &self.conn, None).await?;
        // The snapshot comes after the subscriptions, so no signal is lost between them.
        let snapshot = self.snapshot().await?;
        let _ = tx.send(DaemonEvent::Snapshot(Box::new(snapshot)));
        loop {
            tokio::select! {
                _ = owner_changes.next() => return Ok(()),
                message = signals.next() => {
                    let Some(message) = message else { return Ok(()) };
                    let message = message?;
                    if message.header().sender().map(|s| s.as_str()) != Some(owner) {
                        continue;
                    }
                    if let Some(events) = self.translate(&message).await? {
                        for event in events {
                            let _ = tx.send(event);
                        }
                    }
                    if tx.receiver_count() == 0 { return Ok(()); }
                }
            }
        }
    }

    /// The events of one signal. `None` means the signal is of no interest.
    async fn translate(&self, message: &zbus::Message) -> zbus::Result<Option<Vec<DaemonEvent>>> {
        let header = message.header();
        let interface = header.interface().map(|i| i.as_str()).unwrap_or_default();
        let member = header.member().map(|m| m.as_str()).unwrap_or_default();
        let body = message.body();
        let event = match (interface, member) {
            (i, "SensorsUpdated") if i == format!("{IFACE}.Manager") => {
                let (time_us, sensors, channels): SensorsUpdated = body.deserialize()?;
                DaemonEvent::Sensors(SensorSample {
                    time_us,
                    sensors,
                    channels,
                })
            }
            (i, "AlarmRaised") if i == format!("{IFACE}.Alarms") => {
                let (instance, event, severity, source, response, poweroff_at_us, details): (
                    String,
                    String,
                    String,
                    String,
                    String,
                    u64,
                    Props,
                ) = body.deserialize()?;
                DaemonEvent::AlarmRaised(AlarmEntry {
                    instance,
                    event,
                    severity: Severity::parse(&severity),
                    source,
                    response: Response::parse(&response),
                    raised_us: 0,
                    poweroff_at_us,
                    details: details_of(&details),
                })
            }
            (i, "AlarmCleared") if i == format!("{IFACE}.Alarms") => DaemonEvent::AlarmCleared {
                instance: body.deserialize::<(String,)>()?.0,
            },
            (i, "PoweroffCancelled") if i == format!("{IFACE}.Alarms") => {
                DaemonEvent::PoweroffCancelled {
                    instance: body.deserialize::<(String,)>()?.0,
                }
            }
            (i, "PoweroffFailed") if i == format!("{IFACE}.Alarms") => {
                let (instance, error): (String, String) = body.deserialize()?;
                DaemonEvent::PoweroffFailed { instance, error }
            }
            (i, "ProfileActivated") if i == format!("{IFACE}.Profiles") => {
                let (profile, cause, state): (OwnedObjectPath, String, String) =
                    body.deserialize()?;
                DaemonEvent::ProfileActivated {
                    profile: last_element(profile.as_str()),
                    cause: Cause::parse(&cause),
                    state,
                }
            }
            (i, "WriteFailed") if i == format!("{IFACE}.Leakshield") => {
                let (error, backup): (String, String) = body.deserialize()?;
                let path = header
                    .path()
                    .map(|p| p.as_str().to_owned())
                    .unwrap_or_default();
                DaemonEvent::WriteFailed {
                    leakshield: last_element(&path),
                    error,
                    backup,
                }
            }
            (i, "ExternalChange") if i == format!("{IFACE}.Leakshield") => {
                let fields: Vec<String> = body.deserialize::<(Vec<String>,)>()?.0;
                let path = header
                    .path()
                    .map(|p| p.as_str().to_owned())
                    .unwrap_or_default();
                DaemonEvent::ExternalChange {
                    leakshield: last_element(&path),
                    fields,
                }
            }
            ("org.freedesktop.DBus.Properties", "PropertiesChanged")
            | ("org.freedesktop.DBus.ObjectManager", "InterfacesAdded" | "InterfacesRemoved") => {
                let snapshot = self.snapshot().await?;
                DaemonEvent::Snapshot(Box::new(snapshot))
            }
            _ => return Ok(None),
        };
        Ok(Some(vec![event]))
    }

    /// Read the objects of the daemon in one pass.
    pub async fn snapshot(&self) -> zbus::Result<Snapshot> {
        let objects = zbus::fdo::ObjectManagerProxy::builder(&self.conn)
            .destination(BUS_NAME)
            .path(ROOT)
            .build()
            .await?
            .get_managed_objects()
            .await?;
        let properties = zbus::fdo::PropertiesProxy::builder(&self.conn)
            .destination(BUS_NAME)
            .path(ROOT)
            .build()
            .await?;
        let profiles = properties
            .get_all(zbus::names::InterfaceName::try_from(format!(
                "{IFACE}.Profiles"
            ))?)
            .await?;
        let alarms = properties
            .get_all(zbus::names::InterfaceName::try_from(format!(
                "{IFACE}.Alarms"
            ))?)
            .await?;

        let mut snapshot = Snapshot::default();
        for (path, interfaces) in &objects {
            for (interface, props) in interfaces {
                let props: Props = props
                    .iter()
                    .filter_map(|(key, value)| Some((key.clone(), value.try_clone().ok()?)))
                    .collect();
                match interface.as_str().strip_prefix(&format!("{IFACE}.")) {
                    Some("Sensor") => {
                        let id = get::<String>(&props, "Id").unwrap_or_default();
                        snapshot
                            .sensors
                            .entry(id.clone())
                            .or_default()
                            .apply_sensor(
                                id,
                                get(&props, "Name").unwrap_or_default(),
                                get(&props, "DefaultName").unwrap_or_default(),
                                get(&props, "Kind").unwrap_or_default(),
                                get(&props, "Unit").unwrap_or_default(),
                                get(&props, "Origin").unwrap_or_default(),
                                get(&props, "Driver").unwrap_or_default(),
                            );
                    }
                    Some("DerivedSensor") => {
                        // The object path holds the sensor id: the Sensor interface may come later.
                        let id = last_element(path.as_str());
                        let sources: Vec<OwnedObjectPath> =
                            get(&props, "Sources").unwrap_or_default();
                        snapshot.sensors.entry(id).or_default().derived = Some(DerivedDef {
                            function: get(&props, "Type").unwrap_or_default(),
                            sources: sources.iter().map(|p| last_element(p.as_str())).collect(),
                        });
                    }
                    Some("Channel") => {
                        let id = get::<String>(&props, "Id").unwrap_or_default();
                        snapshot
                            .channels
                            .insert(id.clone(), channel_info(id, &props));
                    }
                    Some("Board") => {
                        snapshot.board = Some(BoardInfo {
                            vendor: get(&props, "Vendor").unwrap_or_default(),
                            model: get(&props, "Model").unwrap_or_default(),
                            board_file: get(&props, "BoardFile").unwrap_or_default(),
                        });
                    }
                    Some("Loop") => snapshot.loops.push(LoopInfo {
                        id: get(&props, "Id").unwrap_or_default(),
                        name: get(&props, "Name").unwrap_or_default(),
                        leakshield: object_id(&props, "Leakshield"),
                        flow: object_id(&props, "Flow"),
                        water_temperature: object_id(&props, "WaterTemperature"),
                        pump_channels: object_ids(&props, "PumpChannels"),
                        flow_sensor: object_id(&props, "FlowSensor"),
                        calibration_table: get(&props, "CalibrationTable").unwrap_or_default(),
                        pump_floor: get(&props, "PumpFloor").unwrap_or_default(),
                    }),
                    Some("Leakshield") => snapshot.leakshields.push(LeakshieldInfo {
                        id: get(&props, "Id").unwrap_or_default(),
                        serial: get(&props, "Serial").unwrap_or_default(),
                        firmware: get(&props, "Firmware").unwrap_or_default(),
                        connected: get(&props, "Connected").unwrap_or_default(),
                        read_only_reason: get(&props, "ReadOnlyReason").unwrap_or_default(),
                        mode: get(&props, "Mode").unwrap_or_default(),
                        running_action: get(&props, "RunningAction").unwrap_or_default(),
                        pressure: None,
                        field_counts: None,
                    }),
                    Some("Profile") => snapshot.profiles.push(ProfileInfo {
                        id: get(&props, "Id").unwrap_or_default(),
                        name: get(&props, "Name").unwrap_or_default(),
                        is_default: get(&props, "IsDefault").unwrap_or_default(),
                        contains_release: get(&props, "ContainsRelease").unwrap_or_default(),
                        slot: get(&props, "Slot").unwrap_or(1),
                    }),
                    _ => {}
                }
            }
        }
        snapshot.loops.sort_by(|a, b| a.id.cmp(&b.id));
        snapshot.leakshields.sort_by(|a, b| a.id.cmp(&b.id));
        // The Default Profile first, then by number: `profile_2` before `profile_10`.
        snapshot.profiles.sort_by(|a, b| {
            (!a.is_default, a.id.len(), &a.id).cmp(&(!b.is_default, b.id.len(), &b.id))
        });

        let profiles: Props = profiles;
        let active = object_id(&profiles, "Active");
        snapshot.active_profile =
            active.and_then(|id| snapshot.profiles.iter().find(|p| p.id == id).cloned());
        snapshot.cause =
            Cause::parse(&get::<String>(&profiles, "ActivationCause").unwrap_or_default());
        snapshot.immediate_switching = get(&profiles, "ImmediateSwitching").unwrap_or_default();
        snapshot.profile_state = get(&profiles, "ActiveState").unwrap_or_default();
        snapshot.rule_delay_s = get(&profiles, "RuleDelaySeconds").unwrap_or_default();
        snapshot.dwell_s = get(&profiles, "DwellSeconds").unwrap_or_default();
        let group_state: Vec<(String, bool, u64)> =
            get(&profiles, "RuleGroupState").unwrap_or_default();
        if let Some(active) = alarms.get("Active") {
            snapshot.alarms = decode_alarms(active);
        }
        // A daemon without Calibration tables answers with an error: keep an empty list.
        snapshot.calibration_tables = self.calibration_tables().await.unwrap_or_default();
        snapshot.rule_groups = self.rule_groups().await.unwrap_or_default();
        for group in &mut snapshot.rule_groups {
            if let Some((_, holds, since_us)) = group_state.iter().find(|row| row.0 == group.id) {
                group.holds = *holds;
                group.since_us = *since_us;
            }
        }
        snapshot.responses = self.responses().await.unwrap_or_default();
        for profile in &snapshot.profiles {
            if let Ok(content) = self.profile_content(&profile.id).await {
                snapshot
                    .profile_contents
                    .insert(profile.id.clone(), content);
            }
        }
        snapshot.daemon_version = self.daemon_version().await.unwrap_or_default();
        for device in snapshot.leakshields.iter_mut().filter(|d| d.connected) {
            let data = self.leakshield_data(&device.id).await;
            if !data.fields.is_empty() {
                let read_only = data.fields.iter().filter(|field| !field.writable).count();
                device.field_counts = Some((data.fields.len() as u32, read_only as u32));
            }
            snapshot.leakshield_data.insert(device.id.clone(), data);
        }
        Ok(snapshot)
    }

    /// Read what a Leakshield has beyond its properties. A call that fails (the daemon answers
    /// `NotSupported`, or the device is away) leaves its part empty.
    pub async fn leakshield_data(&self, id: &str) -> LeakshieldData {
        let Ok(proxy) = self.proxy(&object("leakshield", id), "Leakshield").await else {
            return LeakshieldData::default();
        };
        let mut data = LeakshieldData {
            structure: proxy.get_property("Structure").await.unwrap_or_default(),
            writable: proxy.get_property("Writable").await.unwrap_or_default(),
            clock_offset_s: proxy.get_property("ClockOffset").await.unwrap_or_default(),
            software_sensor_slots: proxy
                .get_property("SoftwareSensorSlots")
                .await
                .unwrap_or_default(),
            ..LeakshieldData::default()
        };
        let fields: Vec<(String, String, OwnedValue, bool, String)> =
            proxy.call("GetFields", &()).await.unwrap_or_default();
        data.fields = fields
            .into_iter()
            .filter_map(|(id, risk, value, writable, reason)| {
                Some(FieldEntry {
                    id,
                    risk: FieldRisk::parse(&risk),
                    value: field_value(&value)?,
                    writable,
                    reason,
                })
            })
            .collect();
        let events: Vec<(u64, String, Props)> =
            proxy.call("GetEvents", &()).await.unwrap_or_default();
        data.events = events
            .into_iter()
            .map(|(time_us, id, details)| DeviceEvent {
                time_us,
                id,
                details: details
                    .iter()
                    .filter_map(|(key, value)| Some((key.clone(), text_of(value)?)))
                    .collect(),
            })
            .collect();
        data.events
            .sort_by_key(|event| std::cmp::Reverse(event.time_us));
        let backups: Vec<(String, u64, String, u16, String)> =
            proxy.call("ListBackups", &()).await.unwrap_or_default();
        data.backups = backups
            .into_iter()
            .map(|(id, time_us, sha256, firmware, reason)| BackupEntry {
                id,
                time_us,
                sha256,
                firmware,
                reason,
            })
            .collect();
        data.backups
            .sort_by_key(|backup| std::cmp::Reverse(backup.time_us));
        let slots: Vec<(u16, OwnedObjectPath)> = proxy
            .call("GetSoftwareSensors", &())
            .await
            .unwrap_or_default();
        data.software_sensors = slots
            .into_iter()
            .map(|(slot, sensor)| {
                (
                    slot,
                    (sensor.as_str() != "/").then(|| last_element(sensor.as_str())),
                )
            })
            .collect();
        let proposals: Vec<(String, String, String, String)> = proxy
            .call("GetDeviceResponseProposals", &())
            .await
            .unwrap_or_default();
        data.proposals = proposals
            .into_iter()
            .map(|(alarm, current, proposed, overlap)| ResponseProposal {
                alarm: alarm
                    .strip_prefix("leakshield.")
                    .unwrap_or(&alarm)
                    .to_owned(),
                current,
                proposed,
                overlap,
            })
            .collect();
        data
    }

    async fn daemon_version(&self) -> zbus::Result<String> {
        self.proxy(ROOT, "Manager")
            .await?
            .get_property("Version")
            .await
    }

    /// `GetRuleGroups`, in list order.
    pub async fn rule_groups(&self) -> zbus::Result<Vec<RuleGroup>> {
        let proxy = self.proxy(ROOT, "Profiles").await?;
        let rows: Vec<RuleGroupRow> = proxy.call("GetRuleGroups", &()).await?;
        Ok(rows
            .into_iter()
            .map(|(id, profile, enabled, rules)| RuleGroup {
                id,
                profile: last_element(profile.as_str()),
                enabled,
                rules: rules
                    .iter()
                    .filter_map(|(kind, params)| rule_of(kind, params))
                    .collect(),
                holds: false,
                since_us: 0,
            })
            .collect())
    }

    /// `GetResponses`: the alarm response of each event type.
    pub async fn responses(&self) -> zbus::Result<Vec<ResponseRule>> {
        let proxy = self.proxy(ROOT, "Alarms").await?;
        let rows: Vec<(String, String, u32)> = proxy.call("GetResponses", &()).await?;
        Ok(rows
            .into_iter()
            .map(|(event, response, delay_s)| ResponseRule {
                event,
                response: Response::parse(&response),
                delay_s,
            })
            .collect())
    }

    /// `GetContent` of a Profile.
    pub async fn profile_content(&self, id: &str) -> zbus::Result<ProfileContent> {
        let proxy = self.proxy(&object("profile", id), "Profile").await?;
        let content: Props = proxy.call("GetContent", &()).await?;
        Ok(content_of(&content))
    }

    /// `GetCalibrationTables`: the presets of the package and the user tables.
    pub async fn calibration_tables(&self) -> zbus::Result<Vec<CalibrationTableInfo>> {
        let proxy = self.proxy(ROOT, "Manager").await?;
        let tables: Vec<CalibrationRow> = proxy.call("GetCalibrationTables", &()).await?;
        Ok(tables
            .into_iter()
            .map(|(id, name, preset, points)| CalibrationTableInfo {
                id,
                name,
                preset,
                points,
            })
            .collect())
    }

    async fn proxy(&self, path: &str, interface: &str) -> zbus::Result<Proxy<'static>> {
        Proxy::new(
            &self.conn,
            BUS_NAME,
            path.to_owned(),
            format!("{IFACE}.{interface}"),
        )
        .await
    }

    /// Call a method that needs polkit. The flag lets the daemon ask polkit to show its prompt.
    async fn write(
        &self,
        path: &str,
        interface: &str,
        method: &str,
        body: &(impl serde::Serialize + zbus::zvariant::DynamicType),
    ) -> zbus::Result<()> {
        self.write_returning::<()>(path, interface, method, body)
            .await
    }

    async fn write_returning<R>(
        &self,
        path: &str,
        interface: &str,
        method: &str,
        body: &(impl serde::Serialize + zbus::zvariant::DynamicType),
    ) -> zbus::Result<R>
    where
        R: serde::de::DeserializeOwned + zbus::zvariant::Type,
    {
        let proxy = self.proxy(path, interface).await?;
        let reply = proxy
            .call_with_flags::<_, _, R>(method, MethodFlags::ALLOW_INTERACTIVE_AUTH, body)
            .await?;
        // `call_with_flags` returns `None` only for a `NO_REPLY_EXPECTED` call.
        reply.ok_or_else(|| zbus::Error::Failure("no reply".into()))
    }

    pub async fn set_sensor_name(&self, sensor: &str, name: &str) -> zbus::Result<()> {
        self.write(&object("sensor", sensor), "Sensor", "SetName", &(name,))
            .await
    }

    /// `function` is `maximum`, `mean` or `difference`. Returns the identifier of the new sensor.
    pub async fn create_derived_sensor(
        &self,
        name: &str,
        function: &str,
        sources: &[String],
    ) -> zbus::Result<String> {
        let sources = object_paths("sensor", sources)?;
        let path: OwnedObjectPath = self
            .write_returning(
                ROOT,
                "Manager",
                "CreateDerivedSensor",
                &(name, function, sources),
            )
            .await?;
        Ok(last_element(path.as_str()))
    }

    pub async fn set_derived_definition(
        &self,
        sensor: &str,
        function: &str,
        sources: &[String],
    ) -> zbus::Result<()> {
        let sources = object_paths("sensor", sources)?;
        self.write(
            &object("sensor", sensor),
            "DerivedSensor",
            "SetDefinition",
            &(function, sources),
        )
        .await
    }

    pub async fn delete_derived_sensor(&self, sensor: &str) -> zbus::Result<()> {
        self.write(&object("sensor", sensor), "DerivedSensor", "Delete", &())
            .await
    }

    pub async fn set_channel_name(&self, channel: &str, name: &str) -> zbus::Result<()> {
        self.write(&object("channel", channel), "Channel", "SetName", &(name,))
            .await
    }

    pub async fn set_curve(
        &self,
        channel: &str,
        input: &str,
        points: &[(f64, f64)],
        smoothing_s: f64,
        step_limit: f64,
    ) -> zbus::Result<()> {
        let input = object_path("sensor", input)?;
        self.write(
            &object("channel", channel),
            "Channel",
            "SetCurve",
            &(input, points.to_vec(), smoothing_s, step_limit),
        )
        .await
    }

    pub async fn set_fixed_duty(&self, channel: &str, duty: f64) -> zbus::Result<()> {
        self.write(
            &object("channel", channel),
            "Channel",
            "SetFixedDuty",
            &(duty,),
        )
        .await
    }

    pub async fn release_to_hardware(&self, channel: &str) -> zbus::Result<()> {
        self.write(
            &object("channel", channel),
            "Channel",
            "ReleaseToHardware",
            &(),
        )
        .await
    }

    pub async fn create_loop(&self, name: &str) -> zbus::Result<String> {
        let path: OwnedObjectPath = self
            .write_returning(ROOT, "Manager", "CreateLoop", &(name,))
            .await?;
        Ok(last_element(path.as_str()))
    }

    pub async fn set_loop_name(&self, id: &str, name: &str) -> zbus::Result<()> {
        self.write(&object("loop", id), "Loop", "SetName", &(name,))
            .await
    }

    pub async fn set_pump_channels(&self, id: &str, channels: &[String]) -> zbus::Result<()> {
        let channels = object_paths("channel", channels)?;
        self.write(&object("loop", id), "Loop", "SetPumpChannels", &(channels,))
            .await
    }

    /// An empty `sensor` clears the flow sensor.
    pub async fn set_flow_sensor(&self, id: &str, sensor: &str, table: &str) -> zbus::Result<()> {
        let sensor = object_path("sensor", sensor)?;
        self.write(
            &object("loop", id),
            "Loop",
            "SetFlowSensor",
            &(sensor, table),
        )
        .await
    }

    pub async fn set_water_temperature(&self, id: &str, sensor: &str) -> zbus::Result<()> {
        let sensor = object_path("sensor", sensor)?;
        self.write(
            &object("loop", id),
            "Loop",
            "SetWaterTemperature",
            &(sensor,),
        )
        .await
    }

    pub async fn set_loop_leakshield(&self, id: &str, leakshield: &str) -> zbus::Result<()> {
        let leakshield = object_path("leakshield", leakshield)?;
        self.write(&object("loop", id), "Loop", "SetLeakshield", &(leakshield,))
            .await
    }

    pub async fn set_pump_floor(&self, id: &str, duty: f64) -> zbus::Result<()> {
        self.write(&object("loop", id), "Loop", "SetPumpFloor", &(duty,))
            .await
    }

    pub async fn delete_loop(&self, id: &str) -> zbus::Result<()> {
        self.write(&object("loop", id), "Loop", "Delete", &()).await
    }

    /// An empty `id` creates a table. Returns the id of the table.
    pub async fn set_calibration_table(
        &self,
        id: &str,
        name: &str,
        points: &[(f64, f64)],
    ) -> zbus::Result<String> {
        self.write_returning(
            ROOT,
            "Manager",
            "SetCalibrationTable",
            &(id, name, points.to_vec()),
        )
        .await
    }

    pub async fn delete_calibration_table(&self, id: &str) -> zbus::Result<()> {
        self.write(ROOT, "Manager", "DeleteCalibrationTable", &(id,))
            .await
    }

    pub async fn cancel_poweroff(&self, instance: &str) -> zbus::Result<()> {
        self.write(ROOT, "Alarms", "CancelPoweroff", &(instance,))
            .await
    }

    pub async fn activate_profile(&self, id: &str) -> zbus::Result<()> {
        let path = OwnedObjectPath::try_from(format!("{ROOT}/profile/{id}"))?;
        self.write(ROOT, "Profiles", "ActivateProfile", &(path,))
            .await
    }

    pub async fn return_to_automatic(&self) -> zbus::Result<()> {
        self.write(ROOT, "Profiles", "ReturnToAutomatic", &()).await
    }

    /// `copy_from` is a Profile id, or empty for the active Profile.
    pub async fn create_profile(&self, name: &str, copy_from: &str) -> zbus::Result<String> {
        let source = object_path("profile", copy_from)?;
        let path: OwnedObjectPath = self
            .write_returning(ROOT, "Profiles", "CreateProfile", &(name, source))
            .await?;
        Ok(last_element(path.as_str()))
    }

    pub async fn set_profile_name(&self, id: &str, name: &str) -> zbus::Result<()> {
        self.write(&object("profile", id), "Profile", "SetName", &(name,))
            .await
    }

    pub async fn set_profile_slot(&self, id: &str, slot: u8) -> zbus::Result<()> {
        self.write(&object("profile", id), "Profile", "SetSlot", &(slot,))
            .await
    }

    pub async fn delete_profile(&self, id: &str) -> zbus::Result<()> {
        self.write(&object("profile", id), "Profile", "Delete", &())
            .await
    }

    pub async fn set_immediate_switching(&self, enabled: bool) -> zbus::Result<()> {
        self.write(ROOT, "Profiles", "SetImmediateSwitching", &(enabled,))
            .await
    }

    pub async fn set_rule_timing(&self, rule_delay_s: u32, dwell_s: u32) -> zbus::Result<()> {
        self.write(ROOT, "Profiles", "SetRuleTiming", &(rule_delay_s, dwell_s))
            .await
    }

    /// Replace the list of Rule groups. An empty id creates a group.
    pub async fn set_rule_groups(&self, groups: &[RuleGroup]) -> zbus::Result<Vec<String>> {
        let mut rows: Vec<RuleGroupOut<'_>> = Vec::new();
        for group in groups {
            rows.push((
                group.id.clone(),
                object_path("profile", &group.profile)?,
                group.enabled,
                group.rules.iter().map(rule_params).collect(),
            ));
        }
        self.write_returning(ROOT, "Profiles", "SetRuleGroups", &(rows,))
            .await
    }

    pub async fn set_alarm_response(
        &self,
        event: &str,
        response: Response,
        delay_s: u32,
    ) -> zbus::Result<()> {
        self.write(
            ROOT,
            "Alarms",
            "SetResponse",
            &(event, response.id(), delay_s),
        )
        .await
    }

    /// Create the first Loop and link its parts. Returns the id of the Loop.
    #[allow(clippy::too_many_arguments)]
    pub async fn setup_loop(
        &self,
        name: &str,
        pump_channels: &[String],
        flow_sensor: &str,
        table: &str,
        water: &str,
        leakshield: &str,
        floor: f64,
    ) -> zbus::Result<String> {
        let id = self.create_loop(name).await?;
        self.set_pump_channels(&id, pump_channels).await?;
        if !flow_sensor.is_empty() {
            self.set_flow_sensor(&id, flow_sensor, table).await?;
        }
        if !water.is_empty() {
            self.set_water_temperature(&id, water).await?;
        }
        if !leakshield.is_empty() {
            self.set_loop_leakshield(&id, leakshield).await?;
        }
        self.set_pump_floor(&id, floor).await?;
        Ok(id)
    }

    fn leakshield_path(id: &str) -> String {
        object("leakshield", id)
    }

    /// `SetFields`: one write path run for all fields.
    pub async fn set_leakshield_fields(
        &self,
        leakshield: &str,
        fields: &[(String, FieldValue)],
    ) -> zbus::Result<()> {
        let fields: HashMap<String, Value<'static>> = fields
            .iter()
            .map(|(name, value)| {
                let value = match value {
                    FieldValue::Flag(on) => Value::Bool(*on),
                    FieldValue::Number(number) => Value::F64(*number),
                    FieldValue::Text(text) => Value::from(text.clone()),
                    FieldValue::List(list) => Value::from(list.clone()),
                };
                (name.clone(), value)
            })
            .collect();
        self.write(
            &Self::leakshield_path(leakshield),
            "Leakshield",
            "SetFields",
            &(fields,),
        )
        .await
    }

    /// `SetMode` with `shield`, `monitor` or `release`. `release` needs the action `critical`.
    pub async fn set_leakshield_mode(&self, leakshield: &str, mode: &str) -> zbus::Result<()> {
        self.write(
            &Self::leakshield_path(leakshield),
            "Leakshield",
            "SetMode",
            &(mode,),
        )
        .await
    }

    pub async fn start_leakshield_action(
        &self,
        leakshield: &str,
        action: &str,
    ) -> zbus::Result<()> {
        self.write(
            &Self::leakshield_path(leakshield),
            "Leakshield",
            "StartAction",
            &(action,),
        )
        .await
    }

    /// A call of the Leakshield interface that takes no argument and needs `write`.
    pub async fn leakshield_call(&self, leakshield: &str, method: &str) -> zbus::Result<()> {
        self.write(
            &Self::leakshield_path(leakshield),
            "Leakshield",
            method,
            &(),
        )
        .await
    }

    pub async fn set_software_sensor(
        &self,
        leakshield: &str,
        slot: u16,
        sensor: &str,
    ) -> zbus::Result<()> {
        let sensor = object_path("sensor", sensor)?;
        self.write(
            &Self::leakshield_path(leakshield),
            "Leakshield",
            "SetSoftwareSensor",
            &(slot, sensor),
        )
        .await
    }

    /// The firmware and the three reports of a backup, for the export file.
    pub async fn get_backup(
        &self,
        leakshield: &str,
        backup: &str,
    ) -> zbus::Result<(u16, Vec<u8>, Vec<u8>, Vec<u8>)> {
        self.proxy(&Self::leakshield_path(leakshield), "Leakshield")
            .await?
            .call("GetBackup", &(backup,))
            .await
    }

    pub async fn import_backup(
        &self,
        leakshield: &str,
        reports: &(u16, Vec<u8>, Vec<u8>, Vec<u8>),
    ) -> zbus::Result<String> {
        self.write_returning(
            &Self::leakshield_path(leakshield),
            "Leakshield",
            "ImportBackup",
            &(reports.0, &reports.1, &reports.2, &reports.3),
        )
        .await
    }

    /// The restore needs the polkit action `critical`.
    pub async fn restore_backup(&self, leakshield: &str, backup: &str) -> zbus::Result<()> {
        self.write(
            &object("leakshield", leakshield),
            "Leakshield",
            "RestoreBackup",
            &(backup,),
        )
        .await
    }
}

/// A field value of `GetFields`: a flag, a number, a text or a list of numbers.
fn field_value(value: &OwnedValue) -> Option<FieldValue> {
    Some(match unwrapped(value) {
        Value::Bool(flag) => FieldValue::Flag(*flag),
        Value::Str(text) => FieldValue::Text(text.to_string()),
        Value::Array(items) => {
            FieldValue::List(items.iter().filter_map(|item| number_of(item)).collect())
        }
        other => FieldValue::Number(number_of(other)?),
    })
}

/// A scalar of an `a{sv}` as text, for the details of a device event.
fn text_of(value: &OwnedValue) -> Option<String> {
    Some(match unwrapped(value) {
        Value::Bool(flag) => flag.to_string(),
        Value::Str(text) => text.to_string(),
        other => {
            let number = number_of(other)?;
            if (number - number.round()).abs() < 1e-9 {
                format!("{number:.0}")
            } else {
                format!("{number}")
            }
        }
    })
}

fn last_element(path: &str) -> String {
    path.rsplit('/').next().unwrap_or_default().to_owned()
}

fn object(kind: &str, id: &str) -> String {
    format!("{ROOT}/{kind}/{id}")
}

/// The object path of an identifier. An empty identifier is the path `/`: "none".
fn object_path(kind: &str, id: &str) -> zbus::Result<OwnedObjectPath> {
    let path = if id.is_empty() {
        "/".to_owned()
    } else {
        object(kind, id)
    };
    OwnedObjectPath::try_from(path)
}

fn object_paths(kind: &str, ids: &[String]) -> zbus::Result<Vec<OwnedObjectPath>> {
    ids.iter().map(|id| object_path(kind, id)).collect()
}

fn get<T: TryFrom<OwnedValue>>(props: &Props, key: &str) -> Option<T> {
    let value = props.get(key)?.try_clone().ok()?;
    T::try_from(value).ok()
}

/// The identifier in an object path property. `/` means none.
fn object_id(props: &Props, key: &str) -> Option<String> {
    let path: OwnedObjectPath = get(props, key)?;
    (path.as_str() != "/").then(|| last_element(path.as_str()))
}

fn object_ids(props: &Props, key: &str) -> Vec<String> {
    get::<Vec<OwnedObjectPath>>(props, key)
        .unwrap_or_default()
        .iter()
        .map(|path| last_element(path.as_str()))
        .collect()
}

fn channel_info(id: String, props: &Props) -> ChannelInfo {
    let curve = get::<(OwnedObjectPath, Vec<(f64, f64)>, f64, f64)>(props, "Curve")
        .map(|(input, points, smoothing_s, step_limit)| CurveSpec {
            input: (input.as_str() != "/").then(|| last_element(input.as_str())),
            points,
            smoothing_s,
            step_limit,
        })
        .unwrap_or_default();
    ChannelInfo {
        id,
        name: get(props, "Name").unwrap_or_default(),
        default_name: get(props, "DefaultName").unwrap_or_default(),
        control: Control::parse(&get::<String>(props, "Control").unwrap_or_default()),
        override_state: ChannelOverride::parse(
            &get::<String>(props, "Override").unwrap_or_default(),
        ),
        is_pump: get(props, "IsPump").unwrap_or_default(),
        manual_at_snapshot: get(props, "ManualAtSnapshot").unwrap_or_default(),
        curve,
        fixed_duty: get(props, "FixedDuty").unwrap_or_default(),
    }
}

/// The parameters of a rule on the wire, with the keys of `docs/api.md`.
pub fn rule_params(rule: &Rule) -> (String, HashMap<String, Value<'static>>) {
    let mut params: HashMap<String, Value<'static>> = HashMap::new();
    let mut put = |key: &str, value: Value<'static>| {
        params.insert(key.to_owned(), value);
    };
    let kind = match rule {
        Rule::Sensor {
            sensor,
            comparison,
            value,
            value_max,
            hold_s,
        } => {
            put(
                "sensor",
                Value::from(object_path("sensor", sensor).unwrap_or_else(|_| root_none())),
            );
            put("comparison", Value::from(comparison.clone()));
            put("value", Value::F64(*value));
            if comparison == "in_range" {
                put("value_max", Value::F64(*value_max));
            }
            put("hold_s", Value::U32(*hold_s));
            "sensor"
        }
        Rule::Process {
            names,
            match_kind,
            match_on,
        } => {
            put("names", Value::from(names.clone()));
            put("match", Value::from(match_kind.clone()));
            put("match_on", Value::from(match_on.clone()));
            "process"
        }
        Rule::Time { days, start, end } => {
            put("days", Value::from(days.clone()));
            put("start", Value::U16(*start));
            put("end", Value::U16(*end));
            "time"
        }
        Rule::Interval {
            interval_s,
            delay_s,
            duration_s,
        } => {
            put("interval_s", Value::U32(*interval_s));
            put("delay_s", Value::U32(*delay_s));
            put("duration_s", Value::U32(*duration_s));
            "interval"
        }
    };
    (kind.to_owned(), params)
}

fn root_none() -> OwnedObjectPath {
    OwnedObjectPath::try_from("/").expect("the root path")
}

fn rule_of(kind: &str, params: &Props) -> Option<Rule> {
    match kind {
        "sensor" => Some(Rule::Sensor {
            sensor: object_id(params, "sensor").unwrap_or_default(),
            comparison: get(params, "comparison").unwrap_or_default(),
            value: get(params, "value").unwrap_or_default(),
            value_max: get(params, "value_max").unwrap_or_default(),
            hold_s: get(params, "hold_s").unwrap_or_default(),
        }),
        "process" => Some(Rule::Process {
            names: get(params, "names").unwrap_or_default(),
            match_kind: get(params, "match").unwrap_or_else(|| "any".to_owned()),
            match_on: get(params, "match_on").unwrap_or_else(|| "name".to_owned()),
        }),
        "time" => Some(Rule::Time {
            days: get(params, "days").unwrap_or_default(),
            start: get(params, "start").unwrap_or_default(),
            end: get(params, "end").unwrap_or_default(),
        }),
        "interval" => Some(Rule::Interval {
            interval_s: get(params, "interval_s").unwrap_or_default(),
            delay_s: get(params, "delay_s").unwrap_or_default(),
            duration_s: get(params, "duration_s").unwrap_or_default(),
        }),
        _ => None,
    }
}

fn unwrapped<'a, 'v>(value: &'a Value<'v>) -> &'a Value<'v> {
    let mut value = value;
    while let Value::Variant(inner) = value {
        value = inner;
    }
    value
}

/// The entries of an `a{sv}` that sits in a variant.
fn dict_of<'a, 'v>(value: &'a Value<'v>) -> Option<Vec<(String, &'a Value<'v>)>> {
    let Value::Dict(dict) = unwrapped(value) else {
        return None;
    };
    Some(
        dict.iter()
            .filter_map(|(key, entry)| match unwrapped(key) {
                Value::Str(name) => Some((name.to_string(), unwrapped(entry))),
                _ => None,
            })
            .collect(),
    )
}

fn number_of(value: &Value<'_>) -> Option<f64> {
    Some(match unwrapped(value) {
        Value::F64(v) => *v,
        Value::U8(v) => f64::from(*v),
        Value::I16(v) => f64::from(*v),
        Value::U16(v) => f64::from(*v),
        Value::I32(v) => f64::from(*v),
        Value::U32(v) => f64::from(*v),
        Value::I64(v) => *v as f64,
        Value::U64(v) => *v as f64,
        _ => return None,
    })
}

/// Decode the `a{sv}` of `GetContent`: the keys of the Profile file of `docs/storage.md`.
fn content_of(content: &Props) -> ProfileContent {
    let mut out = ProfileContent::default();
    if let Some(channels) = content.get("channels").and_then(|v| dict_of(v)) {
        for (id, entry) in &channels {
            let Some(entry) = dict_of(entry) else {
                continue;
            };
            let field = |key: &str| entry.iter().find(|(name, _)| name == key).map(|(_, v)| *v);
            let text = |key: &str| match field(key) {
                Some(Value::Str(text)) => Some(text.to_string()),
                _ => None,
            };
            let points = match field("points") {
                Some(Value::Array(rows)) => rows
                    .iter()
                    .filter_map(|row| match unwrapped(row) {
                        Value::Structure(pair) => {
                            let fields = pair.fields();
                            Some((number_of(fields.first()?)?, number_of(fields.get(1)?)?))
                        }
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            };
            out.channels.insert(
                id.clone(),
                ProfileChannel {
                    control: Control::parse(&text("control").unwrap_or_default()),
                    input: text("input").filter(|input| !input.is_empty()),
                    points,
                    fixed_duty: field("fixed_duty").and_then(number_of).unwrap_or_default(),
                },
            );
        }
    }
    if let Some(devices) = content.get("leakshield_fields").and_then(|v| dict_of(v)) {
        for (id, fields) in &devices {
            let Some(fields) = dict_of(fields) else {
                continue;
            };
            let decoded = fields
                .iter()
                .filter_map(|(name, value)| {
                    let value = match value {
                        Value::Bool(flag) => FieldValue::Flag(*flag),
                        Value::Str(text) => FieldValue::Text(text.to_string()),
                        Value::Array(items) => FieldValue::List(
                            items.iter().filter_map(|item| number_of(item)).collect(),
                        ),
                        other => FieldValue::Number(number_of(other)?),
                    };
                    Some((name.clone(), value))
                })
                .collect();
            out.leakshield.insert(id.clone(), decoded);
        }
    }
    out
}

/// The `details` dictionary of an Alarm: `value`, `limit`, `unit`, `sensor` and `latched_bits`.
fn details_of(details: &Props) -> AlarmDetails {
    AlarmDetails {
        value: get(details, "value"),
        limit: get(details, "limit"),
        unit: get(details, "unit").unwrap_or_default(),
        sensor: object_id(details, "sensor"),
        latched_bits: get(details, "latched_bits").unwrap_or(0),
    }
}

/// Decode `Alarms.Active`: `(instance, event, severity, source, response, raised us, poweroff
/// at us, details)` for each row.
pub fn decode_alarms(value: &OwnedValue) -> Vec<AlarmEntry> {
    let Value::Array(rows) = &**value else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    for row in rows.iter() {
        let Value::Structure(fields) = row else {
            continue;
        };
        let fields = fields.fields();
        if fields.len() < 8 {
            continue;
        }
        let text = |index: usize| match &fields[index] {
            Value::Str(text) => text.to_string(),
            _ => String::new(),
        };
        let number = |index: usize| match &fields[index] {
            Value::U64(number) => *number,
            _ => 0,
        };
        let details = match &fields[7] {
            Value::Dict(dict) => dict
                .try_clone()
                .ok()
                .and_then(|dict| Props::try_from(dict).ok())
                .map(|props| details_of(&props))
                .unwrap_or_default(),
            _ => AlarmDetails::default(),
        };
        entries.push(AlarmEntry {
            instance: text(0),
            event: text(1),
            severity: Severity::parse(&text(2)),
            source: text(3),
            response: Response::parse(&text(4)),
            raised_us: number(5),
            poweroff_at_us: number(6),
            details,
        });
    }
    entries
}

impl SensorInfo {
    #[allow(clippy::too_many_arguments)]
    fn apply_sensor(
        &mut self,
        id: String,
        name: String,
        default_name: String,
        kind: String,
        unit: String,
        origin: String,
        driver: String,
    ) {
        self.id = id;
        self.name = name;
        self.default_name = default_name;
        self.kind = kind;
        self.unit = unit;
        self.origin = origin;
        self.driver = driver;
    }
}
