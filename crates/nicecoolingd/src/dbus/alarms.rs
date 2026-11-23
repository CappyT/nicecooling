//! The interface `Alarms` on the root object. Tasks 6.x fill the methods in.

// The signal `AlarmRaised` of the contract has 7 arguments.
#![allow(clippy::too_many_arguments)]

use std::collections::HashMap;
use std::sync::Arc;

use nicecooling_api::errors::Result;
use zbus::object_server::SignalEmitter;
use zbus::{OwnedVariant, interface};

use super::{Hub, not_supported_stub};

pub struct Alarms {
    #[allow(dead_code)]
    hub: Arc<Hub>,
}

impl Alarms {
    pub fn new(hub: Arc<Hub>) -> Self {
        Self { hub }
    }
}

fn stub<T>(method: &str) -> Result<T> {
    Err(not_supported_stub("Alarms", method))
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

#[allow(unused_variables)]
#[interface(name = "io.github.cappyt.NiceCooling1.Alarms")]
impl Alarms {
    #[zbus(property)]
    fn active(&self) -> Vec<Active> {
        Vec::new()
    }

    #[zbus(out_args("responses"))]
    async fn get_responses(&self) -> Result<Vec<(String, String, u32)>> {
        stub("GetResponses")
    }

    async fn set_response(&self, event: &str, response: &str, delay_s: u32) -> Result<()> {
        stub("SetResponse")
    }

    async fn cancel_poweroff(&self, instance: &str) -> Result<()> {
        stub("CancelPoweroff")
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
