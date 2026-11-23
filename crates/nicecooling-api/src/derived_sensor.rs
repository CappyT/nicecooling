//! The typed proxy of the interface `io.github.cappyt.NiceCooling1.DerivedSensor`.

use zbus::OwnedObjectPath;

#[zbus::proxy(
    interface = "io.github.cappyt.NiceCooling1.DerivedSensor",
    default_service = "io.github.cappyt.NiceCooling1"
)]
pub trait DerivedSensor {
    #[zbus(property)]
    fn r#type(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn sources(&self) -> zbus::Result<Vec<OwnedObjectPath>>;

    fn set_definition(&self, r#type: &str, sources: &[OwnedObjectPath]) -> zbus::Result<()>;
    fn delete(&self) -> zbus::Result<()>;
}
