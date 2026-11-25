//! The typed proxy of the interface `io.github.cappyt.NiceCooling1.Sensor`.

#[zbus::proxy(
    interface = "io.github.cappyt.NiceCooling1.Sensor",
    default_service = "io.github.cappyt.NiceCooling1"
)]
pub trait Sensor {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn default_name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn kind(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn unit(&self) -> zbus::Result<String>;
    /// `hwmon`, `leakshield`, `derived`, `calibrated` or `graph`.
    #[zbus(property)]
    fn origin(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn driver(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn value(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn has_value(&self) -> zbus::Result<bool>;

    fn set_name(&self, name: &str) -> zbus::Result<()>;
}
