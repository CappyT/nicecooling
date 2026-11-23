//! The typed proxy of the interface `io.github.cappyt.NiceCooling1.Loop`.

use zbus::OwnedObjectPath;

#[zbus::proxy(
    interface = "io.github.cappyt.NiceCooling1.Loop",
    default_service = "io.github.cappyt.NiceCooling1"
)]
pub trait Loop {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn pump_channels(&self) -> zbus::Result<Vec<OwnedObjectPath>>;
    #[zbus(property)]
    fn flow_sensor(&self) -> zbus::Result<OwnedObjectPath>;
    #[zbus(property)]
    fn calibration_table(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn flow(&self) -> zbus::Result<OwnedObjectPath>;
    #[zbus(property)]
    fn water_temperature(&self) -> zbus::Result<OwnedObjectPath>;
    #[zbus(property)]
    fn leakshield(&self) -> zbus::Result<OwnedObjectPath>;
    #[zbus(property)]
    fn pump_floor(&self) -> zbus::Result<f64>;

    fn set_name(&self, name: &str) -> zbus::Result<()>;
    fn set_pump_channels(&self, channels: &[OwnedObjectPath]) -> zbus::Result<()>;
    fn set_flow_sensor(
        &self,
        sensor: &OwnedObjectPath,
        calibration_table: &str,
    ) -> zbus::Result<()>;
    fn set_water_temperature(&self, sensor: &OwnedObjectPath) -> zbus::Result<()>;
    fn set_leakshield(&self, leakshield: &OwnedObjectPath) -> zbus::Result<()>;
    fn set_pump_floor(&self, duty: f64) -> zbus::Result<()>;
    fn delete(&self) -> zbus::Result<()>;
}
