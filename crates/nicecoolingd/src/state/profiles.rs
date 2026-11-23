//! The set of Profiles (`docs/storage.md`, `profiles/<id>/profile.json`). The Leakshield settings
//! are not here: a Profile only names a slot, and the files of the slot stay when it goes.

use std::collections::BTreeMap;

use nicecooling_core::profile::Slot;

use super::ProfileFile;

pub use nicecooling_api::ids::DEFAULT_PROFILE;

#[derive(Debug, PartialEq, Eq)]
pub enum ProfileError {
    NotFound,
    /// The Default Profile cannot be deleted, and its slot cannot change.
    Default,
    BadSlot,
}

/// Every Profile of the daemon. The Default Profile is always in it. The number of Profiles is
/// not limited.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileSet {
    map: BTreeMap<String, ProfileFile>,
}

fn default_profile() -> ProfileFile {
    ProfileFile::default()
}

impl ProfileSet {
    /// The Default Profile with slot 1, and three Profiles with the slots 2, 3 and 4.
    pub fn first_start() -> Self {
        let mut set = Self::new(BTreeMap::new());
        for slot in Slot::all().filter(|s| *s != Slot::DEFAULT) {
            let n = u32::from(slot.get());
            let id = format!("profile_{n}");
            set.map.insert(
                id.clone(),
                ProfileFile {
                    id,
                    name: format!("Profile {n}"),
                    slot: n,
                    ..ProfileFile::default()
                },
            );
        }
        set
    }

    /// Adds an empty Default Profile when `map` has none.
    pub fn new(mut map: BTreeMap<String, ProfileFile>) -> Self {
        map.entry(DEFAULT_PROFILE.to_owned())
            .or_insert_with(default_profile);
        Self { map }
    }

    pub fn get(&self, id: &str) -> Option<&ProfileFile> {
        self.map.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut ProfileFile> {
        self.map.get_mut(id)
    }

    pub fn contains(&self, id: &str) -> bool {
        self.map.contains_key(id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.map.keys().map(String::as_str)
    }

    pub fn iter(&self) -> impl Iterator<Item = &ProfileFile> {
        self.map.values()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut ProfileFile> {
        self.map.values_mut()
    }

    pub fn default_profile(&self) -> &ProfileFile {
        &self.map[DEFAULT_PROFILE]
    }

    /// A copy of `source` under a new id: same Curves, same Pump floors, same slot.
    pub fn create(&mut self, name: &str, source: &ProfileFile) -> String {
        let n = nicecooling_api::ids::next_number("profile_", self.map.keys().map(String::as_str));
        let id = nicecooling_api::ids::numbered_profile(n);
        let mut profile = source.clone();
        profile.id = id.clone();
        profile.name = if name.is_empty() {
            id.clone()
        } else {
            name.to_owned()
        };
        self.map.insert(id.clone(), profile);
        id
    }

    pub fn delete(&mut self, id: &str) -> Result<ProfileFile, ProfileError> {
        if id == DEFAULT_PROFILE {
            return Err(ProfileError::Default);
        }
        self.map.remove(id).ok_or(ProfileError::NotFound)
    }

    pub fn set_slot(&mut self, id: &str, slot: u32) -> Result<(), ProfileError> {
        if id == DEFAULT_PROFILE {
            return Err(ProfileError::Default);
        }
        let slot = Slot::new(slot).ok_or(ProfileError::BadSlot)?;
        self.map.get_mut(id).ok_or(ProfileError::NotFound)?.slot = u32::from(slot.get());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(set: &ProfileSet) -> Vec<(String, u32)> {
        set.iter().map(|p| (p.id.clone(), p.slot)).collect()
    }

    #[test]
    fn default_profile_slot_is_1() {
        let set = ProfileSet::new(BTreeMap::new());
        assert_eq!(set.default_profile().id, DEFAULT_PROFILE);
        assert_eq!(set.default_profile().slot, 1);
    }

    #[test]
    fn first_start_creates_profiles_with_slots_1_to_4() {
        let set = ProfileSet::first_start();
        assert_eq!(
            slots(&set),
            [
                ("profile_2".to_owned(), 2),
                ("profile_3".to_owned(), 3),
                ("profile_4".to_owned(), 4),
                (DEFAULT_PROFILE.to_owned(), 1),
            ]
        );
    }

    #[test]
    fn more_than_four_profiles_allowed() {
        let mut set = ProfileSet::first_start();
        let source = set.get("profile_3").unwrap().clone();
        for n in 0..20 {
            set.create(&format!("Extra {n}"), &source);
        }
        assert_eq!(set.iter().count(), 24);
        assert!(set.contains("profile_24"));
    }

    #[test]
    fn new_profile_takes_slot_of_source() {
        let mut set = ProfileSet::first_start();
        let source = set.get("profile_3").unwrap().clone();
        let id = set.create("Copy", &source);
        assert_eq!(id, "profile_5");
        assert_eq!(set.get(&id).unwrap().slot, 3);
        assert_eq!(set.get(&id).unwrap().name, "Copy");
    }

    #[test]
    fn set_slot_changes_slot_only() {
        let mut set = ProfileSet::first_start();
        set.get_mut("profile_3").unwrap().name = "Gaming".into();
        let before = set.get("profile_3").unwrap().clone();
        set.set_slot("profile_3", 4).unwrap();
        let after = set.get("profile_3").unwrap();
        assert_eq!(after.slot, 4);
        assert_eq!(
            ProfileFile {
                slot: before.slot,
                ..after.clone()
            },
            before
        );
    }

    #[test]
    fn set_slot_rejects_0_and_5() {
        let mut set = ProfileSet::first_start();
        assert_eq!(set.set_slot("profile_3", 0), Err(ProfileError::BadSlot));
        assert_eq!(set.set_slot("profile_3", 5), Err(ProfileError::BadSlot));
        assert_eq!(set.get("profile_3").unwrap().slot, 3);
        assert_eq!(set.set_slot("profile_9", 2), Err(ProfileError::NotFound));
    }

    #[test]
    fn set_slot_rejects_default_profile() {
        let mut set = ProfileSet::first_start();
        assert_eq!(set.set_slot(DEFAULT_PROFILE, 2), Err(ProfileError::Default));
        assert_eq!(set.default_profile().slot, 1);
    }

    #[test]
    fn default_profile_cannot_be_deleted() {
        let mut set = ProfileSet::first_start();
        assert_eq!(set.delete(DEFAULT_PROFILE), Err(ProfileError::Default));
        assert_eq!(set.delete("profile_9"), Err(ProfileError::NotFound));
        assert!(set.delete("profile_2").is_ok());
        assert!(!set.contains("profile_2"));
    }
}
