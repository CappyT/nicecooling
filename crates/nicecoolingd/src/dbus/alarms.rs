//! The interface `Alarms` on the root object: the active Alarms and Warnings, the alarm responses
//! and the cancel of a pending poweroff.

// The signal `AlarmRaised` of the contract has 7 arguments.
#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;
use std::sync::Arc;

use nicecooling_api::errors::Result;
use nicecooling_api::ids::ROOT_PATH;
use tokio::sync::mpsc;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::{Connection, OwnedVariant, Variant, interface};

use super::Hub;
use crate::alarms::history::Detail;
use crate::alarms::logind::{CALL_TIMEOUT, SD_LOGIND_SKIP_INHIBITORS, call_poweroff};
use crate::alarms::poweroff::Effect;

pub struct Alarms {
    hub: Arc<Hub>,
}

impl Alarms {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self { hub }
    }
}

/// The keys of `docs/api.md`, section Event identifiers: `bits` (`u`) holds the bits of
/// `alarm_bits` that set the event and `latched_bits` (`u`) those of `alarm_bits_latched`.
fn details(bits: u32, latched: u32) -> HashMap<String, OwnedVariant> {
    let mut details = HashMap::new();
    for (key, value) in [("bits", bits), ("latched_bits", latched)] {
        if value != 0 {
            details.insert(
                key.to_owned(),
                OwnedVariant::try_from(Variant::U32(value))
                    .expect("a number has no file descriptor"),
            );
        }
    }
    details
}

type Active = (
    String,
    String,
    String,
    String,
    String,
    u64,
    u64,
    HashMap<String, OwnedVariant>,
);

type History = (
    String,
    String,
    String,
    String,
    String,
    u64,
    u64,
    String,
    HashMap<String, OwnedVariant>,
);

fn detail_variant(detail: Detail) -> OwnedVariant {
    let variant = match detail {
        Detail::Double(value) => Variant::F64(value),
        Detail::Unsigned(value) => Variant::U32(value),
        Detail::Text(value) => Variant::from(value),
        Detail::Object(path) => match zbus::ObjectPath::try_from(path) {
            Ok(path) => Variant::from(path),
            Err(_) => Variant::from(""),
        },
    };
    OwnedVariant::try_from(variant).expect("a variant has no file descriptor")
}

#[interface(name = "io.github.cappyt.NiceCooling1.Alarms")]
impl Alarms {
    #[zbus(property)]
    fn active(&self) -> Vec<Active> {
        self.hub
            .alarm_rows()
            .into_iter()
            .map(|row| {
                (
                    row.instance,
                    row.event,
                    row.severity.to_owned(),
                    row.source,
                    row.response.to_owned(),
                    row.raised_us,
                    row.poweroff_at_us,
                    details(row.bits, row.latched),
                )
            })
            .collect()
    }

    #[zbus(out_args("responses"))]
    async fn get_responses(&self) -> Result<Vec<(String, String, String, u32, bool)>> {
        Ok(self.hub.alarm_responses())
    }

    async fn set_response(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        event: &str,
        source: &str,
        response: &str,
        delay_s: u32,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Alarms", "SetResponse", None)
            .await?;
        self.hub
            .set_alarm_response(event, source, response, delay_s)
    }

    async fn reset_response(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        event: &str,
        source: &str,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Alarms", "ResetResponse", None)
            .await?;
        self.hub.reset_alarm_response(event, source)
    }

    #[zbus(out_args("rows"))]
    async fn get_history(&self, since_us: u64, limit: u32) -> Vec<History> {
        self.hub
            .alarm_history(since_us, limit)
            .into_iter()
            .map(|row| {
                let mut details: HashMap<String, OwnedVariant> = row
                    .details
                    .into_iter()
                    .map(|(key, detail)| (key, detail_variant(detail)))
                    .collect();
                if row.closed_by_restart {
                    details.insert(
                        "closed_by_restart".to_owned(),
                        OwnedVariant::try_from(Variant::Bool(true))
                            .expect("a bool has no file descriptor"),
                    );
                }
                (
                    row.instance,
                    row.event,
                    row.severity,
                    row.source,
                    row.response,
                    row.raised_us,
                    row.cleared_us,
                    row.outcome,
                    details,
                )
            })
            .collect()
    }

    async fn cancel_poweroff(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        instance: &str,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Alarms", "CancelPoweroff", None)
            .await?;
        self.hub.cancel_poweroff(instance)
    }

    #[zbus(signal)]
    async fn alarm_raised(
        emitter: &SignalEmitter<'_>,
        instance: String,
        event: String,
        severity: String,
        source: String,
        response: String,
        poweroff_at_us: u64,
        details: HashMap<String, OwnedVariant>,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn alarm_cleared(emitter: &SignalEmitter<'_>, instance: String) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn poweroff_cancelled(emitter: &SignalEmitter<'_>, instance: String) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn poweroff_failed(
        emitter: &SignalEmitter<'_>,
        instance: String,
        error: String,
    ) -> zbus::Result<()>;
}

/// Runs the effects of the alarm manager: the signals, and the calls to logind. One task runs
/// them in order, and a logind call runs on its own task so that it never delays a signal.
pub async fn run_effects(
    connection: Connection,
    hub: Arc<Hub>,
    mut effects: mpsc::UnboundedReceiver<Effect>,
) {
    let server = connection.object_server();
    while let Some(effect) = effects.recv().await {
        if let Effect::CallPoweroff(instance) = effect {
            let Some(login1) = hub.login1() else {
                hub.poweroff_result(&instance, Err("the daemon has no logind client".into()));
                continue;
            };
            let hub = hub.clone();
            tokio::spawn(async move {
                let result =
                    call_poweroff(login1.as_ref(), SD_LOGIND_SKIP_INHIBITORS, CALL_TIMEOUT).await;
                hub.poweroff_result(&instance, result);
            });
            continue;
        }
        let Ok(iface) = server.interface::<_, Alarms>(ROOT_PATH).await else {
            continue;
        };
        let emitter = iface.signal_emitter();
        let changed = match effect {
            Effect::Raised(row) => {
                let _ = Alarms::alarm_raised(
                    emitter,
                    row.instance,
                    row.event,
                    row.severity.to_owned(),
                    row.source,
                    row.response.to_owned(),
                    row.poweroff_at_us,
                    details(row.bits, row.latched),
                )
                .await;
                true
            }
            Effect::Updated(_) => true,
            Effect::Cleared(instance) => {
                let _ = Alarms::alarm_cleared(emitter, instance).await;
                true
            }
            Effect::PoweroffCancelled(instance) => {
                let _ = Alarms::poweroff_cancelled(emitter, instance).await;
                true
            }
            Effect::PoweroffFailed { instance, error } => {
                let _ = Alarms::poweroff_failed(emitter, instance, error).await;
                false
            }
            Effect::CallPoweroff(_) => false,
        };
        if changed {
            let _ = iface.get().await.active_changed(emitter).await;
        }
    }
}
