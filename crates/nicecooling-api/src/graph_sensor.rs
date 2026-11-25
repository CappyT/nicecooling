//! The typed proxy of the interface `io.github.cappyt.NiceCooling1.GraphSensor`.

#[zbus::proxy(
    interface = "io.github.cappyt.NiceCooling1.GraphSensor",
    default_service = "io.github.cappyt.NiceCooling1"
)]
pub trait GraphSensor {
    /// Changes the default name and the unit. The daemon then removes the object and adds it
    /// again: read the properties of the new object.
    fn set_definition(&self, name: &str, unit: &str) -> zbus::Result<()>;
    fn delete(&self) -> zbus::Result<()>;
}
