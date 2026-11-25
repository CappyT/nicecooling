//! The client of the daemon contract `io.github.cappyt.NiceCooling1`. The app reaches the
//! daemon only through D-Bus. The connection is a parameter: the binary gives the system bus
//! and a test gives a private bus.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use tokio::sync::broadcast;
use zbus::message::Type;
use zbus::names::BusName;
use zbus::proxy::MethodFlags;
use zbus::{Connection, MatchRule, MessageStream, Proxy};
use zbus::{OwnedObjectPath, OwnedVariant as OwnedValue, Variant as Value};

use crate::bitmap::Bitmap;
use crate::model::{
    AlarmDetails, AlarmEntry, AlarmRecord, BackupEntry, BiosSnapshot, BoardInfo,
    CalibrationTableInfo, Cause, ChannelInfo, ChannelOverride, Control, CurveSpec, DaemonEvent,
    Derived, DerivedDef, DeviceEvent, DisplayAssets, FeedState, FieldEntry, FieldRisk, FieldValue,
    LastFeed, LeakshieldData, LeakshieldInfo, Link, LoopInfo, Outcome, PagePreview, ProfileChannel,
    ProfileContent, ProfileInfo, PropertyChange, RefillEntry, Response, ResponseProposal,
    ResponseRule, Rule, RuleGroup, SensorInfo, SensorSample, Severity, Snapshot, UnitState,
};

/// A bitmap of the daemon contract: width, height and the rows.
type BitmapWire = (u16, u16, Vec<u8>);

/// A row of `GetHistory`: instance, event, severity, source, response, raised, cleared, outcome,
/// details.
type HistoryWire = (
    String,
    String,
    String,
    String,
    String,
    u64,
    u64,
    String,
    HashMap<String, OwnedValue>,
);

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
/// The body of `SensorsUpdated`: the time, `(id, value, flags)` of sensors, and `(id, Duty,
/// smoothed Input, Curve target, flags)` of Channels.
type SensorsUpdated = (
    u64,
    Vec<(String, f64, u8)>,
    Vec<(String, f64, f64, f64, u8)>,
);

/// The read of the device data of one Leakshield that runs in a task.
#[derive(Clone, Copy, Debug, Default)]
struct DeviceRead {
    /// Another change came while the read ran: read again after it.
    again: bool,
    /// The Leakshield went away while the read ran: drop its result.
    cancelled: bool,
}

#[derive(Clone)]
pub struct DaemonClient {
    conn: Connection,
    /// The device reads that run now, by Leakshield id.
    reads: Arc<Mutex<HashMap<String, DeviceRead>>>,
}

impl DaemonClient {
    pub fn new(conn: Connection) -> Self {
        Self {
            conn,
            reads: Arc::default(),
        }
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
                    if let Some(events) = self.translate(&message, tx).await? {
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
    async fn translate(
        &self,
        message: &zbus::Message,
        tx: &broadcast::Sender<DaemonEvent>,
    ) -> zbus::Result<Option<Vec<DaemonEvent>>> {
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
            ("org.freedesktop.DBus.Properties", "PropertiesChanged") => {
                let (interface_name, changed, invalidated): (String, Props, Vec<String>) =
                    body.deserialize()?;
                // The signal carries the new values. A property that it only invalidates has no
                // value here: read everything again.
                if !invalidated.is_empty() {
                    let snapshot = self.snapshot().await?;
                    return Ok(Some(vec![DaemonEvent::Snapshot(Box::new(snapshot))]));
                }
                let path = header
                    .path()
                    .map(|p| p.as_str().to_owned())
                    .unwrap_or_default();
                let change = PropertyChange {
                    path,
                    interface: interface_name,
                    changed,
                };
                let mut events = vec![DaemonEvent::Properties(change.clone())];
                let derived = match change.interface.strip_prefix(&format!("{IFACE}.")) {
                    Some("Leakshield") => self.leakshield_changed(&change, tx),
                    _ => self.read_derived(&change).await,
                };
                if let Some(derived) = derived {
                    events.push(DaemonEvent::Derived(Box::new(derived)));
                }
                return Ok(Some(events));
            }
            ("org.freedesktop.DBus.ObjectManager", "InterfacesAdded" | "InterfacesRemoved") => {
                let snapshot = self.snapshot().await?;
                DaemonEvent::Snapshot(Box::new(snapshot))
            }
            _ => return Ok(None),
        };
        let mut events = vec![event];
        // The outcome of a row changes with these signals: read the history again.
        if interface == format!("{IFACE}.Alarms")
            && matches!(
                member,
                "AlarmRaised" | "AlarmCleared" | "PoweroffCancelled" | "PoweroffFailed"
            )
            && let Ok(rows) = self.alarm_history().await
        {
            events.push(DaemonEvent::History(rows));
        }
        Ok(Some(events))
    }

    /// The calls that a property change needs. `LastFeed` and `FeedState` change about once a
    /// second and need none.
    async fn read_derived(&self, change: &PropertyChange) -> Option<Derived> {
        let kind = change.interface.strip_prefix(&format!("{IFACE}."))?;
        let has = |keys: &[&str]| keys.iter().any(|key| change.changed.contains_key(*key));
        let mut derived = Derived::default();
        match kind {
            "Profile" => {
                derived.profile_contents = Some(self.profile_contents().await);
                derived.rule_groups = self.rule_groups().await.ok();
            }
            "Channel" if has(&["Curve", "Control", "FixedDuty"]) => {
                derived.profile_contents = Some(self.profile_contents().await);
            }
            "Profiles" if has(&["Active", "ActivationCause"]) => {
                derived.profile_contents = Some(self.profile_contents().await);
                derived.rule_groups = self.rule_groups().await.ok();
            }
            "Profiles" if has(&["RuleGroupState"]) => {
                derived.rule_groups = self.rule_groups().await.ok();
            }
            "Loop" if has(&["CalibrationTable", "FlowSensor"]) => {
                derived.calibration_tables = self.calibration_tables().await.ok();
            }
            _ => return None,
        }
        Some(derived)
    }

    /// A Leakshield property changed. The device data (`GetFields`, `GetStoredFields`, the
    /// backups, the proposals) are read again in a task, one read for each Leakshield at a time,
    /// and only for the properties that change them: the loop of the signals does not wait for
    /// the device. A Leakshield that went away loses its data at once.
    fn leakshield_changed(
        &self,
        change: &PropertyChange,
        tx: &broadcast::Sender<DaemonEvent>,
    ) -> Option<Derived> {
        let id = last_element(&change.path);
        let props = &change.changed;
        if get::<bool>(props, "Connected") == Some(false) {
            if let Some(read) = self.reads.lock().expect("reads").get_mut(&id) {
                read.cancelled = true;
            }
            return Some(Derived {
                leakshield: Some((id, LeakshieldData::default())),
                ..Derived::default()
            });
        }
        let reads_data = props.contains_key("ActiveSlot")
            || props.contains_key("FailedWriteBackup")
            || props.contains_key("Writable")
            || props.contains_key("ReadOnlyReason")
            || props.contains_key("Firmware")
            || get::<bool>(props, "Connected") == Some(true);
        if reads_data {
            self.read_device_later(id, tx);
        }
        None
    }

    fn read_device_later(&self, id: String, tx: &broadcast::Sender<DaemonEvent>) {
        {
            let mut reads = self.reads.lock().expect("reads");
            if let Some(read) = reads.get_mut(&id) {
                read.again = true;
                read.cancelled = false;
                return;
            }
            reads.insert(id.clone(), DeviceRead::default());
        }
        let client = self.clone();
        let tx = tx.clone();
        tokio::spawn(async move {
            loop {
                let data = client.leakshield_data(&id).await;
                let mut reads = client.reads.lock().expect("reads");
                let read = reads.get(&id).copied().unwrap_or_default();
                if !read.cancelled {
                    let _ = tx.send(DaemonEvent::Derived(Box::new(Derived {
                        leakshield: Some((id.clone(), data)),
                        ..Derived::default()
                    })));
                }
                if read.again {
                    reads.insert(id.clone(), DeviceRead::default());
                    continue;
                }
                reads.remove(&id);
                break;
            }
        });
    }

    /// `GetContent` of every Profile.
    async fn profile_contents(&self) -> BTreeMap<String, ProfileContent> {
        let mut contents = BTreeMap::new();
        let Ok(profiles) = self.profile_ids().await else {
            return contents;
        };
        for id in profiles {
            if let Ok(content) = self.profile_content(&id).await {
                contents.insert(id, content);
            }
        }
        contents
    }

    /// The Profile ids that the daemon lists.
    async fn profile_ids(&self) -> zbus::Result<Vec<String>> {
        let objects = zbus::fdo::ObjectManagerProxy::builder(&self.conn)
            .destination(BUS_NAME)
            .path(ROOT)
            .build()
            .await?
            .get_managed_objects()
            .await?;
        let prefix = format!("{ROOT}/profile/");
        Ok(objects
            .keys()
            .filter_map(|path| path.as_str().strip_prefix(&prefix).map(str::to_owned))
            .collect())
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
                        let mut info = ChannelInfo {
                            id: id.clone(),
                            ..ChannelInfo::default()
                        };
                        apply_channel(&mut info, &props);
                        snapshot.channels.insert(id, info);
                    }
                    Some("Board") => {
                        snapshot.board = Some(BoardInfo {
                            vendor: get(&props, "Vendor").unwrap_or_default(),
                            model: get(&props, "Model").unwrap_or_default(),
                            board_file: get(&props, "BoardFile").unwrap_or_default(),
                        });
                    }
                    Some("Loop") => {
                        let mut info = LoopInfo::default();
                        apply_loop(&mut info, &props);
                        snapshot.loops.push(info);
                    }
                    Some("Leakshield") => {
                        let mut info = LeakshieldInfo::default();
                        apply_leakshield(&mut info, &props);
                        snapshot.leakshields.push(info);
                    }
                    Some("Profile") => {
                        let mut info = ProfileInfo {
                            slot: 1,
                            ..ProfileInfo::default()
                        };
                        apply_profile(&mut info, &props);
                        snapshot.profiles.push(info);
                    }
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
        apply_profiles(&mut snapshot, &profiles);
        if let Some(active) = alarms.get("Active") {
            snapshot.alarms = decode_alarms(active);
        }
        // A daemon without Calibration tables answers with an error: keep an empty list.
        snapshot.calibration_tables = self.calibration_tables().await.unwrap_or_default();
        snapshot.rule_groups = self.rule_groups().await.unwrap_or_default();
        merge_group_state(&mut snapshot);
        snapshot.responses = self.responses().await.unwrap_or_default();
        snapshot.alarm_history = self.alarm_history().await.unwrap_or_default();
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
        let refills: Vec<(u64, u32, u32, f64)> = proxy
            .call("GetRefillHistory", &())
            .await
            .unwrap_or_default();
        data.refill_history = refills
            .into_iter()
            .map(|(time_us, duration_s, pump_turns, volume_ml)| RefillEntry {
                time_us,
                duration_s,
                pump_turns,
                volume_ml,
            })
            .collect();
        data.refill_history.sort_by_key(|refill| refill.time_us);
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
        for slot in 1..=4u8 {
            if let Ok(fields) = self.stored_fields(id, slot).await {
                data.stored.insert(slot, fields);
            }
        }
        data.software_scales = proxy
            .call("GetSoftwareSensorScales", &())
            .await
            .unwrap_or_default();
        let sources: zbus::Result<Vec<OwnedObjectPath>> =
            proxy.call("GetSoftwareSensorSources", &()).await;
        data.software_sources = sources
            .ok()
            .map(|paths| paths.iter().map(|p| last_element(p.as_str())).collect());
        let counters: zbus::Result<HashMap<String, u64>> = proxy.call("GetCounters", &()).await;
        data.counters = counters.ok().map(|map| map.into_iter().collect());
        let assets: zbus::Result<(Vec<String>, Vec<BitmapWire>, BitmapWire)> =
            proxy.call("GetDisplayAssets", &()).await;
        data.display_assets = assets.ok().map(|(names, icons, logo)| DisplayAssets {
            names,
            icons: icons.into_iter().map(bitmap_of).collect(),
            logo: bitmap_of(logo),
        });
        data
    }

    /// `GetDisplayPreview`: the image that the device shows for a page. The device shows the
    /// page for 30 s after the call.
    pub async fn display_preview(&self, leakshield: &str, page: u8) -> zbus::Result<Bitmap> {
        let wire: BitmapWire = self
            .proxy(&Self::leakshield_path(leakshield), "Leakshield")
            .await?
            .call("GetDisplayPreview", &(page,))
            .await?;
        Ok(bitmap_of(wire))
    }

    /// `display_preview` as the event that the app state stores: the image, or the name of the
    /// error.
    pub async fn display_preview_event(&self, leakshield: &str, page: u8) -> DaemonEvent {
        let preview = match self.display_preview(leakshield, page).await {
            Ok(bitmap) if bitmap.is_valid() => PagePreview::Image(bitmap),
            Ok(_) => PagePreview::Failed("InvalidArgument".to_owned()),
            Err(error) => match crate::screens::WriteOutcome::from_error(&error) {
                // An error of the bus arrives as "<name>: <message>". The state is the last
                // element of the name.
                crate::screens::WriteOutcome::Refused { error } => {
                    let name = error.split(':').next().unwrap_or_default().trim();
                    PagePreview::Failed(name.rsplit('.').next().unwrap_or_default().to_owned())
                }
                _ => PagePreview::Failed("AccessDenied".to_owned()),
            },
        };
        DaemonEvent::DisplayPreview {
            leakshield: leakshield.to_owned(),
            page,
            preview,
        }
    }

    /// `SetDisplayName`: one of the 40 names of report 8.
    pub async fn set_display_name(
        &self,
        leakshield: &str,
        index: u8,
        text: &str,
    ) -> zbus::Result<()> {
        self.write(
            &Self::leakshield_path(leakshield),
            "Leakshield",
            "SetDisplayName",
            &(index, text),
        )
        .await
    }

    /// `SetDisplayIcon`: one of the 16 icons of report 8.
    pub async fn set_display_icon(
        &self,
        leakshield: &str,
        index: u8,
        icon: &Bitmap,
    ) -> zbus::Result<()> {
        self.write(
            &Self::leakshield_path(leakshield),
            "Leakshield",
            "SetDisplayIcon",
            &(index, icon.width, icon.height, &icon.rows),
        )
        .await
    }

    /// `SetDisplayLogo`: the start image of report 8.
    pub async fn set_display_logo(&self, leakshield: &str, logo: &Bitmap) -> zbus::Result<()> {
        self.write(
            &Self::leakshield_path(leakshield),
            "Leakshield",
            "SetDisplayLogo",
            &(logo.width, logo.height, &logo.rows),
        )
        .await
    }

    pub async fn set_software_sensor_scale(
        &self,
        leakshield: &str,
        slot: u16,
        scale: f64,
        offset: f64,
    ) -> zbus::Result<()> {
        self.write(
            &Self::leakshield_path(leakshield),
            "Leakshield",
            "SetSoftwareSensorScale",
            &(slot, scale, offset),
        )
        .await
    }

    /// `GetStoredFields`: the fields of the stored report 3 of a slot.
    pub async fn stored_fields(
        &self,
        leakshield: &str,
        slot: u8,
    ) -> zbus::Result<BTreeMap<String, FieldValue>> {
        let proxy = self
            .proxy(&Self::leakshield_path(leakshield), "Leakshield")
            .await?;
        let fields: Props = proxy.call("GetStoredFields", &(slot,)).await?;
        Ok(fields
            .iter()
            .filter_map(|(id, value)| Some((id.clone(), field_value(value)?)))
            .collect())
    }

    /// `SetStoredFields`: change fields of a stored slot. The device is not written.
    pub async fn set_stored_fields(
        &self,
        leakshield: &str,
        slot: u8,
        fields: &[(String, FieldValue)],
    ) -> zbus::Result<()> {
        let fields: HashMap<String, Value<'static>> = fields
            .iter()
            .map(|(name, value)| (name.clone(), field_wire(value)))
            .collect();
        self.write(
            &Self::leakshield_path(leakshield),
            "Leakshield",
            "SetStoredFields",
            &(slot, fields),
        )
        .await
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

    /// `GetResponses`: the default of each event and the resolved row of each Leakshield.
    pub async fn responses(&self) -> zbus::Result<Vec<ResponseRule>> {
        let proxy = self.proxy(ROOT, "Alarms").await?;
        let rows: Vec<(String, String, String, u32, bool)> =
            proxy.call("GetResponses", &()).await?;
        Ok(rows
            .into_iter()
            .map(
                |(event, source, response, delay_s, overridden)| ResponseRule {
                    event,
                    source,
                    response: Response::parse(&response),
                    delay_s,
                    overridden,
                },
            )
            .collect())
    }

    /// `GetHistory` without a time limit and a row limit: the daemon keeps at most 2000 rows.
    pub async fn alarm_history(&self) -> zbus::Result<Vec<AlarmRecord>> {
        let proxy = self.proxy(ROOT, "Alarms").await?;
        let rows: Vec<HistoryWire> = proxy.call("GetHistory", &(0u64, 0u32)).await?;
        Ok(rows
            .into_iter()
            .map(
                |(
                    instance,
                    event,
                    severity,
                    source,
                    response,
                    raised_us,
                    cleared_us,
                    outcome,
                    details,
                )| {
                    let details: Props = details
                        .iter()
                        .filter_map(|(key, value)| Some((key.clone(), value.try_clone().ok()?)))
                        .collect();
                    AlarmRecord {
                        instance,
                        event,
                        severity: Severity::parse(&severity),
                        source,
                        response: Response::parse(&response),
                        raised_us,
                        cleared_us: (cleared_us != 0).then_some(cleared_us),
                        outcome: Outcome::parse(&outcome),
                        closed_by_restart: get(&details, "closed_by_restart").unwrap_or_default(),
                        details: details_of(&details),
                    }
                },
            )
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

    /// Creates a Graph sensor and returns its id (`graph_<n>`). `unit` is one of the eight units
    /// of the contract.
    pub async fn create_graph_sensor(&self, name: &str, unit: &str) -> zbus::Result<String> {
        let path: OwnedObjectPath = self
            .write_returning(ROOT, "Manager", "CreateGraphSensor", &(name, unit))
            .await?;
        Ok(last_element(path.as_str()))
    }

    /// The daemon removes the object and adds it again: the id stays.
    pub async fn set_graph_sensor_definition(
        &self,
        sensor: &str,
        name: &str,
        unit: &str,
    ) -> zbus::Result<()> {
        self.write(
            &object("sensor", sensor),
            "GraphSensor",
            "SetDefinition",
            &(name, unit),
        )
        .await
    }

    pub async fn delete_graph_sensor(&self, sensor: &str) -> zbus::Result<()> {
        self.write(&object("sensor", sensor), "GraphSensor", "Delete", &())
            .await
    }

    /// One entry per Graph sensor: the id, whether it has a value, and the value. The call has no
    /// interactive flag, so a user who polkit does not allow gets `AccessDenied` and no password
    /// dialog once per second.
    pub async fn set_graph_sensor_values(
        &self,
        values: &[(String, bool, f64)],
    ) -> zbus::Result<()> {
        self.proxy(ROOT, "Manager")
            .await?
            .call::<_, _, ()>("SetGraphSensorValues", &(values,))
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

    /// `SetResponse`: source `""` is the default of the event, a Leakshield id its override.
    pub async fn set_alarm_response(
        &self,
        event: &str,
        source: &str,
        response: Response,
        delay_s: u32,
    ) -> zbus::Result<()> {
        self.write(
            ROOT,
            "Alarms",
            "SetResponse",
            &(event, source, response.id(), delay_s),
        )
        .await
    }

    /// `ResetResponse`: source `""` is the default of the event, a Leakshield id its override.
    pub async fn reset_alarm_response(&self, event: &str, source: &str) -> zbus::Result<()> {
        self.write(ROOT, "Alarms", "ResetResponse", &(event, source))
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
        // An empty table gives the sensor no value: the user builds it on the Loops screen.
        if !flow_sensor.is_empty() && !table.is_empty() {
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
            .map(|(name, value)| (name.clone(), field_wire(value)))
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

fn bitmap_of((width, height, rows): BitmapWire) -> Bitmap {
    Bitmap {
        width,
        height,
        rows,
    }
}

/// The wire value of a field for `SetFields` and `SetStoredFields`.
fn field_wire(value: &FieldValue) -> Value<'static> {
    match value {
        FieldValue::Flag(on) => Value::Bool(*on),
        FieldValue::Number(number) => Value::F64(*number),
        FieldValue::Text(text) => Value::from(text.clone()),
        FieldValue::List(list) => Value::from(list.clone()),
        FieldValue::TextList(names) => Value::from(names.clone()),
    }
}

/// A field value of `GetFields` (`docs/api.md`, Leakshield fields): a name (`s`), a number or an
/// integer of any width, or a list of names or of numbers.
fn field_value(value: &Value<'_>) -> Option<FieldValue> {
    Some(match unwrapped(value) {
        Value::Bool(flag) => FieldValue::Flag(*flag),
        Value::Str(text) => FieldValue::Text(text.to_string()),
        Value::Array(items) if is_text_list(items) => FieldValue::TextList(
            items
                .iter()
                .filter_map(|item| match unwrapped(item) {
                    Value::Str(text) => Some(text.to_string()),
                    _ => None,
                })
                .collect(),
        ),
        Value::Array(items) => {
            FieldValue::List(items.iter().filter_map(|item| number_of(item)).collect())
        }
        other => FieldValue::Number(number_of(other)?),
    })
}

/// An array of strings, also when it is empty.
fn is_text_list(items: &zbus::Array<'_>) -> bool {
    match items.first() {
        Some(item) => matches!(unwrapped(item), Value::Str(_)),
        None => *items.element_signature() == zbus::Signature::Str,
    }
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

/// Set the fields of a Channel that `props` carries. A partial map (a `PropertiesChanged`) sets
/// only its own fields.
fn apply_channel(info: &mut ChannelInfo, props: &Props) {
    if let Some(id) = get::<String>(props, "Id") {
        info.id = id;
    }
    if let Some(name) = get(props, "Name") {
        info.name = name;
    }
    if let Some(name) = get(props, "DefaultName") {
        info.default_name = name;
    }
    if let Some(control) = get::<String>(props, "Control") {
        info.control = Control::parse(&control);
    }
    if let Some(state) = get::<String>(props, "Override") {
        info.override_state = ChannelOverride::parse(&state);
    }
    if let Some(is_pump) = get(props, "IsPump") {
        info.is_pump = is_pump;
    }
    if let Some(manual) = get(props, "ManualAtSnapshot") {
        info.manual_at_snapshot = manual;
    }
    if let Some((input, points, smoothing_s, step_limit)) =
        get::<(OwnedObjectPath, Vec<(f64, f64)>, f64, f64)>(props, "Curve")
    {
        info.curve = CurveSpec {
            input: (input.as_str() != "/").then(|| last_element(input.as_str())),
            points,
            smoothing_s,
            step_limit,
        };
    }
    if let Some(duty) = get(props, "FixedDuty") {
        info.fixed_duty = duty;
    }
    if let Some((time_us, attributes)) = get::<(u64, Vec<(String, String)>)>(props, "BiosSnapshot")
    {
        info.bios_snapshot = BiosSnapshot {
            time_us,
            attributes,
        };
    }
    if props.contains_key("FanSensor") {
        info.fan_sensor = object_id(props, "FanSensor");
    }
}

fn apply_loop(info: &mut LoopInfo, props: &Props) {
    if let Some(id) = get(props, "Id") {
        info.id = id;
    }
    if let Some(name) = get(props, "Name") {
        info.name = name;
    }
    if props.contains_key("Leakshield") {
        info.leakshield = object_id(props, "Leakshield");
    }
    if props.contains_key("Flow") {
        info.flow = object_id(props, "Flow");
    }
    if props.contains_key("WaterTemperature") {
        info.water_temperature = object_id(props, "WaterTemperature");
    }
    if props.contains_key("PumpChannels") {
        info.pump_channels = object_ids(props, "PumpChannels");
    }
    if props.contains_key("FlowSensor") {
        info.flow_sensor = object_id(props, "FlowSensor");
    }
    if let Some(table) = get(props, "CalibrationTable") {
        info.calibration_table = table;
    }
    if let Some(floor) = get(props, "PumpFloor") {
        info.pump_floor = floor;
    }
}

fn apply_leakshield(info: &mut LeakshieldInfo, props: &Props) {
    if let Some(id) = get(props, "Id") {
        info.id = id;
    }
    if let Some(serial) = get(props, "Serial") {
        info.serial = serial;
    }
    if let Some(firmware) = get(props, "Firmware") {
        info.firmware = firmware;
    }
    if let Some(connected) = get(props, "Connected") {
        info.connected = connected;
    }
    if let Some(reason) = get(props, "ReadOnlyReason") {
        info.read_only_reason = reason;
    }
    if let Some(mode) = get(props, "Mode") {
        info.mode = mode;
    }
    if let Some(action) = get(props, "RunningAction") {
        info.running_action = action;
    }
    if let Some(state) = get::<String>(props, "FeedState") {
        info.feed_state = FeedState::parse(&state);
    }
    if let Some((time_us, feed_pump, flow_lph)) = get::<(u64, f64, f64)>(props, "LastFeed") {
        info.last_feed = LastFeed {
            time_us,
            feed_pump,
            flow_lph,
        };
    }
    if let Some(slot) = get(props, "ActiveSlot") {
        info.active_slot = slot;
    }
    if let Some(backup) = get(props, "FailedWriteBackup") {
        info.failed_write_backup = backup;
    }
}

fn apply_profile(info: &mut ProfileInfo, props: &Props) {
    if let Some(id) = get(props, "Id") {
        info.id = id;
    }
    if let Some(name) = get(props, "Name") {
        info.name = name;
    }
    if let Some(is_default) = get(props, "IsDefault") {
        info.is_default = is_default;
    }
    if let Some(release) = get(props, "ContainsRelease") {
        info.contains_release = release;
    }
    if let Some(slot) = get(props, "Slot") {
        info.slot = slot;
    }
}

/// The properties of `Profiles` at the root object. The rows of `RuleGroupState` go into the
/// groups that the snapshot holds.
fn apply_profiles(snapshot: &mut Snapshot, props: &Props) {
    if props.contains_key("Active") {
        let active = object_id(props, "Active");
        snapshot.active_profile =
            active.and_then(|id| snapshot.profiles.iter().find(|p| p.id == id).cloned());
    }
    if let Some(cause) = get::<String>(props, "ActivationCause") {
        snapshot.cause = Cause::parse(&cause);
    }
    if let Some(immediate) = get(props, "ImmediateSwitching") {
        snapshot.immediate_switching = immediate;
    }
    if let Some(state) = get(props, "ActiveState") {
        snapshot.profile_state = state;
    }
    if let Some(delay) = get(props, "RuleDelaySeconds") {
        snapshot.rule_delay_s = delay;
    }
    if let Some(dwell) = get(props, "DwellSeconds") {
        snapshot.dwell_s = dwell;
    }
    if let Some(until) = get(props, "DwellUntil") {
        snapshot.dwell_until_us = until;
    }
    if let Some(state) = get::<Vec<(String, bool, u64)>>(props, "RuleGroupState") {
        snapshot.rule_group_state = state;
        merge_group_state(snapshot);
    }
}

fn merge_group_state(snapshot: &mut Snapshot) {
    for group in &mut snapshot.rule_groups {
        if let Some((_, holds, since_us)) = snapshot
            .rule_group_state
            .iter()
            .find(|row| row.0 == group.id)
        {
            group.holds = *holds;
            group.since_us = *since_us;
        }
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
                .filter_map(|(name, value)| Some((name.clone(), field_value(value)?)))
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

/// Apply the properties of one `PropertiesChanged` to the snapshot. Only the properties of the
/// signal change.
pub fn apply_properties(snapshot: &mut Snapshot, change: &PropertyChange) {
    let Some(kind) = change.interface.strip_prefix(&format!("{IFACE}.")) else {
        return;
    };
    let id = last_element(&change.path);
    let props = &change.changed;
    match kind {
        "Profiles" => apply_profiles(snapshot, props),
        "Alarms" => {
            if let Some(active) = props.get("Active") {
                snapshot.alarms = decode_alarms(active);
            }
        }
        "Channel" => {
            if let Some(info) = snapshot.channels.get_mut(&id) {
                apply_channel(info, props);
            }
        }
        "Loop" => {
            if let Some(info) = snapshot.loops.iter_mut().find(|info| info.id == id) {
                apply_loop(info, props);
            }
        }
        "Leakshield" => {
            if let Some(info) = snapshot.leakshields.iter_mut().find(|info| info.id == id) {
                apply_leakshield(info, props);
            }
        }
        "Profile" => {
            if let Some(info) = snapshot.profiles.iter_mut().find(|info| info.id == id) {
                apply_profile(info, props);
                let updated = info.clone();
                if let Some(active) = snapshot.active_profile.as_mut()
                    && active.id == updated.id
                {
                    *active = updated;
                }
            }
        }
        "DerivedSensor" => {
            if let Some(def) = snapshot
                .sensors
                .get_mut(&id)
                .and_then(|sensor| sensor.derived.as_mut())
            {
                if let Some(function) = get::<String>(props, "Type") {
                    def.function = function;
                }
                if let Some(sources) = get::<Vec<OwnedObjectPath>>(props, "Sources") {
                    def.sources = sources.iter().map(|p| last_element(p.as_str())).collect();
                }
            }
        }
        "Sensor" => {
            if let (Some(info), Some(name)) =
                (snapshot.sensors.get_mut(&id), get::<String>(props, "Name"))
            {
                info.name = name;
            }
        }
        _ => {}
    }
}

/// Apply what the app read again after a property change.
pub fn apply_derived(snapshot: &mut Snapshot, derived: &Derived) {
    if let Some(contents) = &derived.profile_contents {
        snapshot.profile_contents = contents.clone();
    }
    if let Some(groups) = &derived.rule_groups {
        snapshot.rule_groups = groups.clone();
        merge_group_state(snapshot);
    }
    if let Some(tables) = &derived.calibration_tables {
        snapshot.calibration_tables = tables.clone();
    }
    if let Some((id, data)) = &derived.leakshield {
        if let Some(device) = snapshot.leakshields.iter_mut().find(|d| &d.id == id) {
            device.field_counts = (!data.fields.is_empty()).then(|| {
                let read_only = data.fields.iter().filter(|field| !field.writable).count();
                (data.fields.len() as u32, read_only as u32)
            });
        }
        snapshot.leakshield_data.insert(id.clone(), data.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_value_follows_its_wire_type() {
        assert_eq!(
            field_value(&Value::from("low")),
            Some(FieldValue::Text("low".into()))
        );
        assert_eq!(
            field_value(&Value::from(vec!["time".to_owned(), "none".to_owned()])),
            Some(FieldValue::TextList(vec!["time".into(), "none".into()]))
        );
        assert_eq!(
            field_value(&Value::from(Vec::<String>::new())),
            Some(FieldValue::TextList(Vec::new()))
        );
        assert_eq!(field_value(&Value::F64(2.5)), Some(FieldValue::Number(2.5)));
        // Every integer width is a number.
        for value in [
            Value::U8(7),
            Value::I16(7),
            Value::U16(7),
            Value::I32(7),
            Value::U32(7),
            Value::I64(7),
            Value::U64(7),
        ] {
            assert_eq!(field_value(&value), Some(FieldValue::Number(7.0)));
        }
        assert_eq!(
            field_value(&Value::from(vec![1_i64, -1])),
            Some(FieldValue::List(vec![1.0, -1.0]))
        );
        assert_eq!(
            field_value(&Value::from(vec![0.5_f64])),
            Some(FieldValue::List(vec![0.5]))
        );
    }
}
