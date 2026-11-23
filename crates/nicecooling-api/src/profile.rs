//! The typed proxy of the interface `io.github.cappyt.NiceCooling1.Profile`.

use std::collections::HashMap;

use zbus::OwnedVariant as OwnedValue;

#[zbus::proxy(
    interface = "io.github.cappyt.NiceCooling1.Profile",
    default_service = "io.github.cappyt.NiceCooling1"
)]
pub trait Profile {
    #[zbus(property)]
    fn id(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn is_default(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn contains_release(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn slot(&self) -> zbus::Result<u8>;

    fn set_name(&self, name: &str) -> zbus::Result<()>;
    fn set_slot(&self, slot: u8) -> zbus::Result<()>;
    fn delete(&self) -> zbus::Result<()>;
    fn get_content(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
}
