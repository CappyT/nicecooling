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
    AlarmEntry, Cause, DaemonEvent, LeakshieldInfo, Link, LoopInfo, ProfileInfo, Response,
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
                let (instance, event, severity, source, response, poweroff_at_us): (
                    String,
                    String,
                    String,
                    String,
                    String,
                    u64,
                ) = body.deserialize()?;
                DaemonEvent::AlarmRaised(AlarmEntry {
                    instance,
                    event,
                    severity: Severity::parse(&severity),
                    source,
                    response: Response::parse(&response),
                    raised_us: 0,
                    poweroff_at_us,
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
        for interfaces in objects.values() {
            for (interface, props) in interfaces {
                let props: Props = props
                    .iter()
                    .filter_map(|(key, value)| Some((key.clone(), value.try_clone().ok()?)))
                    .collect();
                match interface.as_str().strip_prefix(&format!("{IFACE}.")) {
                    Some("Sensor") => {
                        let id = get::<String>(&props, "Id").unwrap_or_default();
                        snapshot.sensors.insert(
                            id.clone(),
                            SensorInfo {
                                id,
                                name: get(&props, "Name").unwrap_or_default(),
                                kind: get(&props, "Kind").unwrap_or_default(),
                                unit: get(&props, "Unit").unwrap_or_default(),
                            },
                        );
                    }
                    Some("Channel") => {
                        let id = get::<String>(&props, "Id").unwrap_or_default();
                        snapshot
                            .channels
                            .insert(id, get(&props, "Name").unwrap_or_default());
                    }
                    Some("Loop") => snapshot.loops.push(LoopInfo {
                        id: get(&props, "Id").unwrap_or_default(),
                        name: get(&props, "Name").unwrap_or_default(),
                        leakshield: object_id(&props, "Leakshield"),
                        flow: object_id(&props, "Flow"),
                        water_temperature: object_id(&props, "WaterTemperature"),
                    }),
                    Some("Leakshield") => snapshot.leakshields.push(LeakshieldInfo {
                        id: get(&props, "Id").unwrap_or_default(),
                        serial: get(&props, "Serial").unwrap_or_default(),
                        firmware: get(&props, "Firmware").unwrap_or_default(),
                        connected: get(&props, "Connected").unwrap_or_default(),
                        read_only_reason: get(&props, "ReadOnlyReason").unwrap_or_default(),
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
        Ok(snapshot)
    }

    /// Call a method that needs polkit. The flag lets the daemon ask polkit to show its prompt.
    async fn write(
        &self,
        path: &str,
        interface: &str,
        method: &str,
        body: &(impl serde::Serialize + zbus::zvariant::DynamicType),
    ) -> zbus::Result<()> {
        let proxy = Proxy::new(
            &self.conn,
            BUS_NAME,
            path.to_owned(),
            format!("{IFACE}.{interface}"),
        )
        .await?;
        proxy
            .call_with_flags::<_, _, ()>(method, MethodFlags::ALLOW_INTERACTIVE_AUTH, body)
            .await?;
        Ok(())
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

fn get<T: TryFrom<OwnedValue>>(props: &Props, key: &str) -> Option<T> {
    let value = props.get(key)?.try_clone().ok()?;
    T::try_from(value).ok()
}

/// The identifier in an object path property. `/` means none.
fn object_id(props: &Props, key: &str) -> Option<String> {
    let path: OwnedObjectPath = get(props, key)?;
    (path.as_str() != "/").then(|| last_element(path.as_str()))
}

/// Decode `Alarms.Active`. The documented row has 5 strings and then the raise time and the
/// poweroff time; the decoder reads the last field as the poweroff time, so it also accepts a
/// row with one number only.
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
        let text = |index: usize| match fields.get(index) {
            Some(Value::Str(text)) => text.to_string(),
            _ => String::new(),
        };
        let number = |index: usize| match fields.get(index) {
            Some(Value::U64(number)) => *number,
            _ => 0,
        };
        if fields.len() < 6 {
            continue;
        }
        let last = fields.len() - 1;
        entries.push(AlarmEntry {
            instance: text(0),
            event: text(1),
            severity: Severity::parse(&text(2)),
            source: text(3),
            response: Response::parse(&text(4)),
            raised_us: if last >= 6 { number(last - 1) } else { 0 },
            poweroff_at_us: number(last),
        });
    }
    entries
}
