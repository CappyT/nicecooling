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
    AlarmDetails, AlarmEntry, BoardInfo, CalibrationTableInfo, Cause, ChannelInfo, ChannelOverride,
    Control, CurveSpec, DaemonEvent, DerivedDef, LeakshieldInfo, Link, LoopInfo, ProfileInfo,
    Response, SensorInfo, SensorSample, Severity, Snapshot, UnitState,
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
                    }),
                    Some("Profile") => snapshot.profiles.push(ProfileInfo {
                        id: get(&props, "Id").unwrap_or_default(),
                        name: get(&props, "Name").unwrap_or_default(),
                    }),
                    _ => {}
                }
            }
        }
        snapshot.loops.sort_by(|a, b| a.id.cmp(&b.id));
        snapshot.leakshields.sort_by(|a, b| a.id.cmp(&b.id));
        snapshot.profiles.sort_by(|a, b| a.id.cmp(&b.id));

        let profiles: Props = profiles;
        let active = object_id(&profiles, "Active");
        snapshot.active_profile =
            active.and_then(|id| snapshot.profiles.iter().find(|p| p.id == id).cloned());
        snapshot.cause =
            Cause::parse(&get::<String>(&profiles, "ActivationCause").unwrap_or_default());
        snapshot.immediate_switching = get(&profiles, "ImmediateSwitching").unwrap_or_default();
        if let Some(active) = alarms.get("Active") {
            snapshot.alarms = decode_alarms(active);
        }
        // A daemon without Calibration tables answers with an error: keep an empty list.
        snapshot.calibration_tables = self.calibration_tables().await.unwrap_or_default();
        Ok(snapshot)
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

/// The `details` dictionary of an Alarm: `value`, `limit`, `unit` and `sensor`.
fn details_of(details: &Props) -> AlarmDetails {
    AlarmDetails {
        value: get(details, "value"),
        limit: get(details, "limit"),
        unit: get(details, "unit").unwrap_or_default(),
        sensor: object_id(details, "sensor"),
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
