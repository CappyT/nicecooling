//! The Profile part of the hub: the set of Profiles, their slots, and the Leakshield settings
//! of a slot.

use std::collections::BTreeMap;

use nicecooling_api::Error;
use nicecooling_api::errors::Result;
use nicecooling_api::ids::{self, Kind};
use nicecooling_core::profile::Slot;

use super::{Hub, check_name};
use crate::leakshield::fields::{self, FieldValue};
use crate::state::ProfileFile;
use crate::state::profiles::{DEFAULT_PROFILE, ProfileError};

/// What the interface `Profile` shows.
#[derive(Clone, Debug)]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    pub slot: u32,
    pub contains_release: bool,
}

/// The Mode code of RELEASE in the `default_mode` field.
const MODE_RELEASE: i64 = 3;

/// A Profile with the decoded Leakshield fields of its slot (`GetContent`).
#[derive(Clone, Debug)]
pub struct ProfileContent {
    pub profile: ProfileFile,
    /// Leakshield id -> field id -> value, from the stored report 3 of the slot.
    pub leakshield_fields: BTreeMap<String, BTreeMap<String, FieldValue>>,
}

fn profile_error(error: ProfileError, id: &str) -> Error {
    match error {
        ProfileError::NotFound => Error::not_found(format!("no Profile {id}")),
        ProfileError::Default => Error::DefaultProfile(format!("{id} is the Default Profile")),
        ProfileError::BadSlot => Error::invalid("a slot is a number from 1 to 4"),
    }
}

impl Hub {
    pub fn profile_ids(&self) -> Vec<String> {
        self.inner().profiles.ids().map(str::to_owned).collect()
    }

    pub fn profile_view(&self, id: &str) -> Option<ProfileView> {
        let (profile, leakshields) = {
            let inner = self.inner();
            (inner.profiles.get(id)?.clone(), inner.leakshields.clone())
        };
        let contains_release = Slot::new(profile.slot).is_some_and(|slot| {
            leakshields.iter().any(|device| {
                let registry = device.registry();
                let Some(field) = registry.field("default_mode") else {
                    return false;
                };
                self.slots
                    .load(&device.id(), slot)
                    .ok()
                    .flatten()
                    .is_some_and(|stored| {
                        field.read_raw(&stored.report3, field.offset) == MODE_RELEASE
                    })
            })
        });
        Some(ProfileView {
            id: profile.id,
            name: profile.name,
            slot: profile.slot,
            contains_release,
        })
    }

    /// `copy_from` is a Profile object path, or `/` for the active Profile (the current state).
    /// The new Profile takes the slot of the Profile it copies.
    pub fn create_profile(&self, name: &str, copy_from: &str) -> Result<String> {
        check_name(name)?;
        let source = if copy_from == "/" {
            None
        } else {
            Some(
                ids::id_from_path(Kind::Profile, copy_from)
                    .ok_or_else(|| Error::invalid(format!("{copy_from} is not a Profile path")))?
                    .to_owned(),
            )
        };
        self.edit(|i| {
            let source = match &source {
                None => i.profile().clone(),
                Some(id) => i
                    .profiles
                    .get(id)
                    .cloned()
                    .ok_or_else(|| Error::not_found(format!("no Profile {id}")))?,
            };
            Ok(i.profiles.create(name, &source))
        })
    }

    pub fn set_profile_name(&self, id: &str, name: &str) -> Result<()> {
        check_name(name)?;
        self.edit(|i| {
            let profile = i
                .profiles
                .get_mut(id)
                .ok_or_else(|| Error::not_found(format!("no Profile {id}")))?;
            profile.name = if name.is_empty() {
                id.to_owned()
            } else {
                name.to_owned()
            };
            Ok(())
        })
    }

    /// Changes the slot of a Profile. The Leakshield settings of both slots stay as they are.
    pub fn set_profile_slot(&self, id: &str, slot: u8) -> Result<()> {
        self.edit(|i| {
            i.profiles
                .set_slot(id, u32::from(slot))
                .map_err(|e| match e {
                    ProfileError::Default => {
                        Error::invalid("the slot of the Default Profile cannot change")
                    }
                    other => profile_error(other, id),
                })
        })
    }

    pub fn delete_profile(&self, id: &str) -> Result<()> {
        self.edit(|i| {
            if id == DEFAULT_PROFILE {
                return Err(profile_error(ProfileError::Default, id));
            }
            if !i.profiles.contains(id) {
                return Err(profile_error(ProfileError::NotFound, id));
            }
            if i.state.profiles.active == id {
                return Err(Error::in_use(format!("{id} is the active Profile")));
            }
            i.profiles.delete(id).map_err(|e| profile_error(e, id))?;
            i.deleted.insert(id.to_owned());
            Ok(())
        })
    }

    pub fn profile_content(&self, id: &str) -> Result<ProfileContent> {
        let (profile, leakshields) = {
            let inner = self.inner();
            let profile = inner
                .profiles
                .get(id)
                .cloned()
                .ok_or_else(|| Error::not_found(format!("no Profile {id}")))?;
            (profile, inner.leakshields.clone())
        };
        let mut leakshield_fields = BTreeMap::new();
        if let Some(slot) = Slot::new(profile.slot) {
            for device in leakshields {
                if let Ok(Some(stored)) = self.slots.load(&device.id(), slot) {
                    let values = fields::values(device.registry(), &stored.report3);
                    leakshield_fields.insert(device.id(), values.into_iter().collect());
                }
            }
        }
        Ok(ProfileContent {
            profile,
            leakshield_fields,
        })
    }
}
