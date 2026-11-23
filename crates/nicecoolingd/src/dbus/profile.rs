//! The interface `Profile`. Tasks 7.x fill the methods in.

use std::collections::HashMap;

use nicecooling_api::errors::Result;
use nicecooling_api::ids::DEFAULT_PROFILE;
use zbus::{OwnedVariant, interface};

use super::not_supported_stub;

pub struct Profile {
    id: String,
    name: String,
    slot: u32,
}

impl Profile {
    pub fn new(id: String, name: String, slot: u32) -> Self {
        Self { id, name, slot }
    }
}

fn stub<T>(method: &str) -> Result<T> {
    Err(not_supported_stub("Profile", method))
}

#[allow(unused_variables)]
#[interface(name = "io.github.cappyt.NiceCooling1.Profile")]
impl Profile {
    #[zbus(property(emits_changed_signal = "const"))]
    fn id(&self) -> String {
        self.id.clone()
    }

    #[zbus(property)]
    fn name(&self) -> String {
        self.name.clone()
    }

    #[zbus(property(emits_changed_signal = "const"))]
    fn is_default(&self) -> bool {
        self.id == DEFAULT_PROFILE
    }

    #[zbus(property)]
    fn contains_release(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn slot(&self) -> u8 {
        u8::try_from(self.slot).unwrap_or(1)
    }

    async fn set_name(&self, name: &str) -> Result<()> {
        stub("SetName")
    }

    async fn set_slot(&self, slot: u8) -> Result<()> {
        stub("SetSlot")
    }

    async fn delete(&self) -> Result<()> {
        stub("Delete")
    }

    #[zbus(out_args("content"))]
    async fn get_content(&self) -> Result<HashMap<String, OwnedVariant>> {
        stub("GetContent")
    }
}
