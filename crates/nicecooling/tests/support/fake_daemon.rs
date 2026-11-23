//! A zbus server of the interface contract `io.github.cappyt.NiceCooling1`, with a small
//! simulated installation. It implements the properties and methods that the app uses.
//! `Alarms.Active` has the 7 fields that the XML comment lists: 5 strings and 2 times.

use std::sync::{Arc, Mutex};

use nicecooling::daemon_client::{BUS_NAME, ROOT};
use zbus::OwnedObjectPath;
use zbus::message::Flags;
use zbus::object_server::SignalEmitter;
use zbus::{Connection, fdo, interface};

pub const LEAKSHIELD: &str = "leakshield_20417_31072";

type ActiveRow = (String, String, String, String, String, u64, u64);

#[derive(Default)]
pub struct Shared {
    /// `"<Method>(<args>) interactive=<bool>"` for each write call.
    pub calls: Mutex<Vec<String>>,
    pub deny_writes: Mutex<bool>,
}

impl Shared {
    fn record(&self, call: &str, header: &zbus::message::Header<'_>) -> fdo::Result<()> {
        let interactive = header
            .primary()
            .flags()
            .contains(Flags::ALLOW_INTERACTIVE_AUTH);
        self.calls
            .lock()
            .expect("calls")
            .push(format!("{call} interactive={interactive}"));
        if *self.deny_writes.lock().expect("deny") {
            return Err(fdo::Error::AccessDenied("polkit denied the call".into()));
        }
        Ok(())
    }
}

fn path(suffix: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(format!("{ROOT}/{suffix}")).expect("object path")
}

struct Manager;

#[interface(name = "io.github.cappyt.NiceCooling1.Manager")]
impl Manager {
    #[zbus(property)]
    fn version(&self) -> String {
        "0.1.0".into()
    }

    #[zbus(property)]
    fn state_health(&self) -> String {
        "ok".into()
    }

    #[zbus(signal)]
    async fn sensors_updated(
        emitter: &SignalEmitter<'_>,
        time_us: u64,
        sensors: Vec<(String, f64, u8)>,
        channels: Vec<(String, f64, u8)>,
    ) -> zbus::Result<()>;
}

struct Profiles {
    active: String,
    cause: String,
    immediate: bool,
    shared: Arc<Shared>,
}

#[interface(name = "io.github.cappyt.NiceCooling1.Profiles")]
impl Profiles {
    #[zbus(property)]
    fn active(&self) -> OwnedObjectPath {
        path(&format!("profile/{}", self.active))
    }

    #[zbus(property)]
    fn activation_cause(&self) -> String {
        self.cause.clone()
    }

    #[zbus(property)]
    fn active_state(&self) -> String {
        "applied".into()
    }

    #[zbus(property)]
    fn default(&self) -> OwnedObjectPath {
        path("profile/profile_default")
    }

    #[zbus(property)]
    fn immediate_switching(&self) -> bool {
        self.immediate
    }

    async fn activate_profile(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        profile: OwnedObjectPath,
    ) -> fdo::Result<()> {
        let id = profile
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or_default()
            .to_owned();
        self.shared
            .record(&format!("ActivateProfile({id})"), &header)?;
        self.active = id;
        self.cause = "manual".into();
        self.active_changed(&emitter).await?;
        self.activation_cause_changed(&emitter).await?;
        Ok(())
    }

    async fn return_to_automatic(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        self.shared.record("ReturnToAutomatic()", &header)?;
        self.active = "profile_default".into();
        self.cause = "default".into();
        self.active_changed(&emitter).await?;
        self.activation_cause_changed(&emitter).await?;
        Ok(())
    }

    #[zbus(signal)]
    async fn profile_activated(
        emitter: &SignalEmitter<'_>,
        profile: OwnedObjectPath,
        cause: &str,
        state: &str,
    ) -> zbus::Result<()>;
}

struct Alarms {
    active: Vec<ActiveRow>,
    shared: Arc<Shared>,
}

#[interface(name = "io.github.cappyt.NiceCooling1.Alarms")]
impl Alarms {
    #[zbus(property)]
    fn active(&self) -> Vec<ActiveRow> {
        self.active.clone()
    }

    async fn cancel_poweroff(
        &mut self,
        #[zbus(header)] header: zbus::message::Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        instance: &str,
    ) -> fdo::Result<()> {
        self.shared
            .record(&format!("CancelPoweroff({instance})"), &header)?;
        let pending = self
            .active
            .iter_mut()
            .find(|row| row.0 == instance && row.6 != 0)
            .ok_or_else(|| {
                fdo::Error::Failed("io.github.cappyt.NiceCooling1.Error.NoPoweroff".into())
            })?;
        pending.6 = 0;
        self.active_changed(&emitter).await?;
        Self::poweroff_cancelled(&emitter, instance).await?;
        Ok(())
    }

    #[zbus(signal)]
    async fn alarm_raised(
        emitter: &SignalEmitter<'_>,
        instance: &str,
        event: &str,
        severity: &str,
        source: &str,
        response: &str,
        poweroff_at_us: u64,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn alarm_cleared(emitter: &SignalEmitter<'_>, instance: &str) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn poweroff_cancelled(emitter: &SignalEmitter<'_>, instance: &str) -> zbus::Result<()>;
}

struct Named {
    id: String,
    name: String,
}

struct Profile(Named);

#[interface(name = "io.github.cappyt.NiceCooling1.Profile")]
impl Profile {
    #[zbus(property)]
    fn id(&self) -> String {
        self.0.id.clone()
    }

    #[zbus(property)]
    fn name(&self) -> String {
        self.0.name.clone()
    }
}

struct Channel(Named);

#[interface(name = "io.github.cappyt.NiceCooling1.Channel")]
impl Channel {
    #[zbus(property)]
    fn id(&self) -> String {
        self.0.id.clone()
    }

    #[zbus(property)]
    fn name(&self) -> String {
        self.0.name.clone()
    }
}

struct Sensor {
    named: Named,
    kind: &'static str,
    unit: &'static str,
}

#[interface(name = "io.github.cappyt.NiceCooling1.Sensor")]
impl Sensor {
    #[zbus(property)]
    fn id(&self) -> String {
        self.named.id.clone()
    }

    #[zbus(property)]
    fn name(&self) -> String {
        self.named.name.clone()
    }

    #[zbus(property)]
    fn kind(&self) -> String {
        self.kind.into()
    }

    #[zbus(property)]
    fn unit(&self) -> String {
        self.unit.into()
    }
}

struct Loop(Named);

#[interface(name = "io.github.cappyt.NiceCooling1.Loop")]
impl Loop {
    #[zbus(property)]
    fn id(&self) -> String {
        self.0.id.clone()
    }

    #[zbus(property)]
    fn name(&self) -> String {
        self.0.name.clone()
    }

    #[zbus(property)]
    fn leakshield(&self) -> OwnedObjectPath {
        path(&format!("leakshield/{LEAKSHIELD}"))
    }

    #[zbus(property)]
    fn flow(&self) -> OwnedObjectPath {
        path("sensor/loop_1_flow")
    }

    #[zbus(property)]
    fn water_temperature(&self) -> OwnedObjectPath {
        path("sensor/nct6798_temp1")
    }
}

struct Leakshield {
    firmware: u16,
    read_only_reason: String,
}

#[interface(name = "io.github.cappyt.NiceCooling1.Leakshield")]
impl Leakshield {
    #[zbus(property)]
    fn id(&self) -> String {
        LEAKSHIELD.into()
    }

    #[zbus(property)]
    fn serial(&self) -> String {
        "20417-31072".into()
    }

    #[zbus(property)]
    fn firmware(&self) -> u16 {
        self.firmware
    }

    #[zbus(property)]
    fn connected(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn read_only_reason(&self) -> String {
        self.read_only_reason.clone()
    }

    #[zbus(signal)]
    async fn external_change(emitter: &SignalEmitter<'_>, fields: Vec<String>) -> zbus::Result<()>;
}

#[derive(Default)]
pub struct Options {
    /// A Leakshield with the firmware 1042 that the registry does not know.
    pub unknown_firmware: bool,
}

pub struct FakeDaemon {
    conn: Connection,
    pub shared: Arc<Shared>,
}

impl FakeDaemon {
    pub async fn start(conn: Connection, options: Options) -> Self {
        let shared = Arc::new(Shared::default());
        let server = conn.object_server();
        server
            .at(ROOT, fdo::ObjectManager)
            .await
            .expect("object manager");
        server.at(ROOT, Manager).await.expect("manager");
        server
            .at(
                ROOT,
                Profiles {
                    active: "profile_default".into(),
                    cause: "default".into(),
                    immediate: false,
                    shared: shared.clone(),
                },
            )
            .await
            .expect("profiles");
        server
            .at(
                ROOT,
                Alarms {
                    active: Vec::new(),
                    shared: shared.clone(),
                },
            )
            .await
            .expect("alarms");
        for (id, name) in [("profile_default", "Default"), ("profile_2", "Quiet")] {
            server
                .at(
                    path(&format!("profile/{id}")),
                    Profile(Named {
                        id: id.into(),
                        name: name.into(),
                    }),
                )
                .await
                .expect("profile");
        }
        server
            .at(
                path("channel/nct6798_pwm6"),
                Channel(Named {
                    id: "nct6798_pwm6".into(),
                    name: "Waterpump".into(),
                }),
            )
            .await
            .expect("channel");
        for (id, name, kind, unit) in [
            ("nct6798_temp1", "Water temperature", "temperature", "degC"),
            ("loop_1_flow", "Flow", "flow", "l/h"),
            (
                "leakshield_20417_31072_pressure",
                "Pressure",
                "pressure",
                "mbar",
            ),
        ] {
            server
                .at(
                    path(&format!("sensor/{id}")),
                    Sensor {
                        named: Named {
                            id: id.into(),
                            name: name.into(),
                        },
                        kind,
                        unit,
                    },
                )
                .await
                .expect("sensor");
        }
        server
            .at(
                path("loop/loop_1"),
                Loop(Named {
                    id: "loop_1".into(),
                    name: "Main loop".into(),
                }),
            )
            .await
            .expect("loop");
        let (firmware, reason) = if options.unknown_firmware {
            (1042, "unknown-firmware")
        } else {
            (1040, "")
        };
        server
            .at(
                path(&format!("leakshield/{LEAKSHIELD}")),
                Leakshield {
                    firmware,
                    read_only_reason: reason.into(),
                },
            )
            .await
            .expect("leakshield");
        conn.request_name(BUS_NAME).await.expect("daemon name");
        Self { conn, shared }
    }

    pub fn calls(&self) -> Vec<String> {
        self.shared.calls.lock().expect("calls").clone()
    }

    pub fn deny_writes(&self, deny: bool) {
        *self.shared.deny_writes.lock().expect("deny") = deny;
    }

    /// Raise an Alarm or a Warning, as the daemon does: the property changes and the signal goes out.
    pub async fn raise_alarm(
        &self,
        instance: &str,
        event: &str,
        severity: &str,
        source: &str,
        response: &str,
        poweroff_at_us: u64,
    ) {
        let iface = self
            .conn
            .object_server()
            .interface::<_, Alarms>(ROOT)
            .await
            .expect("alarms");
        iface.get_mut().await.active.push((
            instance.into(),
            event.into(),
            severity.into(),
            source.into(),
            response.into(),
            1,
            poweroff_at_us,
        ));
        iface
            .get()
            .await
            .active_changed(iface.signal_emitter())
            .await
            .expect("changed");
        Alarms::alarm_raised(
            iface.signal_emitter(),
            instance,
            event,
            severity,
            source,
            response,
            poweroff_at_us,
        )
        .await
        .expect("signal");
    }

    pub async fn clear_alarm(&self, instance: &str) {
        let iface = self
            .conn
            .object_server()
            .interface::<_, Alarms>(ROOT)
            .await
            .expect("alarms");
        iface.get_mut().await.active.retain(|row| row.0 != instance);
        iface
            .get()
            .await
            .active_changed(iface.signal_emitter())
            .await
            .expect("changed");
        Alarms::alarm_cleared(iface.signal_emitter(), instance)
            .await
            .expect("signal");
    }

    pub async fn emit_sensors(
        &self,
        time_us: u64,
        sensors: Vec<(String, f64, u8)>,
        channels: Vec<(String, f64, u8)>,
    ) {
        let iface = self
            .conn
            .object_server()
            .interface::<_, Manager>(ROOT)
            .await
            .expect("manager");
        Manager::sensors_updated(iface.signal_emitter(), time_us, sensors, channels)
            .await
            .expect("signal");
    }

    pub async fn emit_external_change(&self, fields: Vec<String>) {
        let iface = self
            .conn
            .object_server()
            .interface::<_, Leakshield>(path(&format!("leakshield/{LEAKSHIELD}")))
            .await
            .expect("leakshield");
        Leakshield::external_change(iface.signal_emitter(), fields)
            .await
            .expect("signal");
    }

    /// Leave the bus, as a stopped daemon does.
    pub async fn stop(self) {
        let _ = self.conn.release_name(BUS_NAME).await;
    }
}
