//! The Profile part of the hub: the set of Profiles, their slots, and the Leakshield settings
//! of a slot.

use std::collections::BTreeMap;
use std::sync::Arc;

use nicecooling_api::Error;
use nicecooling_api::errors::Result;
use nicecooling_api::ids::{self, Kind};
use nicecooling_core::profile::{Cause, Slot};
use nicecooling_core::tick::TickConfig;

use super::{Hub, check_name};
use crate::profiles::activate::{Failure, apply_slot};
use nicecooling_protocol::report3::Report3;

use crate::leakshield::Attached;
use crate::leakshield::fields::{self, FieldValue};
use crate::state::ProfileFile;
use crate::state::profiles::{DEFAULT_PROFILE, ProfileError};
use crate::state::slots::SlotMeta;

/// What the interface `Profile` shows.
#[derive(Clone, Debug)]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    pub slot: u32,
    pub contains_release: bool,
}

/// What the `Profiles` interface tells the clients.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProfileEvent {
    /// `Active` and `ActivationCause` changed.
    Switched,
    /// The Leakshield part of an activation ended. `state` is `applied` or `leakshield-failed`.
    /// `RuleGroupState` changed.
    RuleGroups,
    Activated {
        profile: String,
        cause: &'static str,
        state: &'static str,
    },
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
    /// For the active Profile it runs the switch for the new slot.
    pub fn set_profile_slot(self: &Arc<Self>, id: &str, slot: u8) -> Result<()> {
        let active = self.edit(|i| {
            i.profiles
                .set_slot(id, u32::from(slot))
                .map_err(|e| match e {
                    ProfileError::Default => {
                        Error::invalid("the slot of the Default Profile cannot change")
                    }
                    other => profile_error(other, id),
                })?;
            Ok(i.state.profiles.active == id)
        })?;
        if active {
            self.reapply_active_slot();
        }
        Ok(())
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
            if let Some(group) = i.state.rule_groups.iter().find(|g| g.profile == id) {
                return Err(Error::in_use(format!(
                    "the Rule group {} targets {id}",
                    group.id
                )));
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

    /// `applied`, or `leakshield-failed` when the Leakshield part of the last activation failed.
    pub fn active_state(&self) -> &'static str {
        self.inner().active_state
    }

    /// Makes `id` the active Profile: the Curves apply with the configuration that this returns,
    /// and the Leakshield part runs in the background (`profiles::activate`). The caller keeps the
    /// rule state, if there is one.
    pub fn activate_profile(self: &Arc<Self>, id: &str, cause: Cause) -> Result<TickConfig> {
        let config = {
            let mut inner = self.inner();
            if !inner.profiles.contains(id) {
                return Err(Error::not_found(format!("no Profile {id}")));
            }
            inner.state.profiles.active = id.to_owned();
            inner.state.profiles.cause = cause.as_str().to_owned();
            inner.active_state = "applied";
            self.queue_config(&inner)
        };
        self.persister.changed();
        let _ = self.profile_events.send(ProfileEvent::Switched);
        self.apply_slot_in_background(id.to_owned(), cause);
        Ok(config)
    }

    /// Writes the slot of the active Profile to every Leakshield that needs it, and reports.
    fn apply_slot_in_background(self: &Arc<Self>, profile: String, cause: Cause) {
        let hub = self.clone();
        self.runtime.spawn(async move {
            let (slot, leakshields) = {
                let inner = hub.inner();
                let slot = inner.profiles.get(&profile).and_then(|p| Slot::new(p.slot));
                (slot, inner.leakshields.clone())
            };
            let mut failed = false;
            if let Some(slot) = slot {
                for device in leakshields {
                    match apply_slot(&device, &hub.slots, slot).await {
                        Ok(_) => {}
                        Err(Failure::Superseded) => return,
                        Err(e) => {
                            eprintln!("nicecoolingd: the Leakshield {}: {e}", device.id());
                            failed = true;
                        }
                    }
                }
            }
            let state = if failed {
                "leakshield-failed"
            } else {
                "applied"
            };
            {
                let mut inner = hub.inner();
                // A newer activation reports for itself.
                if inner.state.profiles.active != profile {
                    return;
                }
                inner.active_state = state;
            }
            let _ = hub.profile_events.send(ProfileEvent::Activated {
                profile,
                cause: cause.as_str(),
                state,
            });
        });
    }

    /// `SetSlot` on the active Profile runs the same switch for the new slot.
    pub fn reapply_active_slot(self: &Arc<Self>) {
        let (profile, cause) = {
            let mut inner = self.inner();
            inner.active_state = "applied";
            (
                inner.state.profiles.active.clone(),
                match inner.state.profiles.cause.as_str() {
                    "manual" => Cause::Manual,
                    "rule" => Cause::Rule,
                    _ => Cause::Default,
                },
            )
        };
        self.apply_slot_in_background(profile, cause);
    }

    /// A Leakshield setting that the user changed in nicecooling goes into the slot of the active
    /// Profile, so into every Profile with that slot (ADR-0006). `report3` is the verified
    /// read-back of the write.
    pub fn store_user_write(&self, device: &Attached, report3: &Report3) {
        let Some(slot) = ({
            let inner = self.inner();
            Slot::new(inner.profile().slot)
        }) else {
            return;
        };
        let info = device.info();
        let meta = SlotMeta::new(info.firmware, info.structure, device.serial());
        if let Err(e) = self
            .slots
            .store(device.registry(), &device.id(), slot, report3, &meta)
        {
            eprintln!(
                "nicecoolingd: slot {slot} of {} is not saved: {e}",
                device.id()
            );
        }
    }
}
