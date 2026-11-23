use std::sync::Arc;

use nicecooling_api::errors::Result;
use nicecooling_api::ids::Kind;
use zbus::message::Header;
use zbus::object_server::SignalEmitter;
use zbus::{Connection, OwnedObjectPath, fdo, interface};

use super::hub::ChannelView;
use super::{Hub, none, object};

/// `Curve`: Input sensor, points, smoothing and step limit.
type CurveProperty = (OwnedObjectPath, Vec<(f64, f64)>, f64, f64);

pub struct Channel {
    hub: Arc<Hub>,
    id: String,
}

impl Channel {
    pub fn new(hub: Arc<Hub>, id: String) -> Self {
        Self { hub, id }
    }

    /// A gone Channel is an unknown object.
    fn view(&self) -> fdo::Result<ChannelView> {
        self.hub
            .channel(&self.id)
            .ok_or_else(|| fdo::Error::UnknownObject(format!("no Channel {}", self.id)))
    }
}

#[interface(name = "io.github.cappyt.NiceCooling1.Channel")]
impl Channel {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> String {
        self.id.clone()
    }

    #[zbus(property)]
    fn name(&self) -> fdo::Result<String> {
        Ok(self.view()?.name)
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn default_name(&self) -> fdo::Result<String> {
        Ok(self.view()?.default_name)
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn duty(&self) -> fdo::Result<f64> {
        Ok(self.view()?.duty)
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn raw(&self) -> fdo::Result<u8> {
        Ok(self.view()?.raw)
    }

    #[zbus(property)]
    fn control(&self) -> fdo::Result<String> {
        Ok(self.view()?.control.to_owned())
    }

    #[zbus(property, name = "Override")]
    fn override_state(&self) -> fdo::Result<String> {
        Ok(self.view()?.override_state.to_owned())
    }

    #[zbus(property)]
    fn is_pump(&self) -> fdo::Result<bool> {
        Ok(self.view()?.is_pump)
    }

    #[zbus(property)]
    fn manual_at_snapshot(&self) -> fdo::Result<bool> {
        Ok(self.view()?.manual_at_snapshot)
    }

    #[zbus(property)]
    fn curve(&self) -> fdo::Result<CurveProperty> {
        let v = self.view()?;
        let input = if v.curve_input.is_empty() {
            none()
        } else {
            object(Kind::Sensor, &v.curve_input)
        };
        Ok((input, v.curve_points, v.smoothing_s, v.step_limit))
    }

    #[zbus(property)]
    fn fixed_duty(&self) -> fdo::Result<f64> {
        Ok(self.view()?.fixed_duty)
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn bios_snapshot(&self) -> fdo::Result<(u64, Vec<(String, String)>)> {
        Ok(self.view()?.bios_snapshot)
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn fan_sensor(&self) -> fdo::Result<OwnedObjectPath> {
        Ok(self
            .view()?
            .fan_sensor
            .map_or_else(none, |id| object(Kind::Sensor, &id)))
    }

    async fn set_name(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        name: &str,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Channel", "SetName", None)
            .await?;
        self.hub.set_channel_name(&self.id, name)?;
        self.name_changed(&emitter).await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn set_curve(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        input: OwnedObjectPath,
        points: Vec<(f64, f64)>,
        smoothing_s: f64,
        step_limit_pct_per_s: f64,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Channel", "SetCurve", None)
            .await?;
        self.hub.set_curve(
            &self.id,
            input.as_str(),
            points,
            smoothing_s,
            step_limit_pct_per_s,
        )?;
        self.control_changed(&emitter).await?;
        self.curve_changed(&emitter).await?;
        self.fixed_duty_changed(&emitter).await?;
        Ok(())
    }

    async fn set_fixed_duty(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
        duty: f64,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Channel", "SetFixedDuty", None)
            .await?;
        self.hub.set_fixed_duty(&self.id, duty)?;
        self.control_changed(&emitter).await?;
        self.curve_changed(&emitter).await?;
        self.fixed_duty_changed(&emitter).await?;
        Ok(())
    }

    async fn release_to_hardware(
        &self,
        #[zbus(connection)] connection: &Connection,
        #[zbus(header)] header: Header<'_>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> Result<()> {
        self.hub
            .authorize(connection, &header, "Channel", "ReleaseToHardware", None)
            .await?;
        self.hub.release_channel(&self.id).await?;
        self.control_changed(&emitter).await?;
        self.curve_changed(&emitter).await?;
        self.fixed_duty_changed(&emitter).await?;
        Ok(())
    }
}
