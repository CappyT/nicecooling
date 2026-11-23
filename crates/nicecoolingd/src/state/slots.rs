//! The Leakshield settings of the four slots, per Leakshield (`docs/storage.md`):
//! `leakshield/<leakshield id>/slot-<n>/{report3.bin, meta.json, assets/}`. A Profile has no
//! Leakshield data of its own: Profiles with the same slot share these files, and a Profile
//! delete never removes them.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use nicecooling_core::profile::Slot;
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report3::Report3;
use serde::{Deserialize, Serialize};

use super::StateStore;
use super::atomic::{RealFs, create_dir_all_with, write_atomic};

/// What a stored report 3 comes from. The daemon writes it only to a device that matches.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SlotMeta {
    pub schema_version: u32,
    pub firmware: u16,
    pub structure: u16,
    /// The serial of the Leakshield that produced the report, as `<first>-<second>`.
    pub serial: String,
    /// `SetStoredFields` changed the report. The next activation writes it with a read-back, and
    /// a good read-back clears the mark.
    pub edited: bool,
}

impl SlotMeta {
    pub fn new(firmware: u16, structure: u16, serial: &str) -> Self {
        Self {
            schema_version: 1,
            firmware,
            structure,
            serial: serial.to_owned(),
            edited: false,
        }
    }
}

/// The stored settings of one slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StoredSlot {
    pub report3: Report3,
    pub meta: SlotMeta,
}

#[derive(Clone)]
pub struct SlotStore {
    store: Arc<StateStore>,
}

fn is_asset_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Sets the `device_profile` byte of `report` to the number of `slot`. The CRC follows.
pub fn with_slot_number(registry: &Registry, report: &Report3, slot: Slot) -> io::Result<Report3> {
    let field = registry
        .field("device_profile")
        .ok_or_else(|| io::Error::other("the registry has no device_profile field"))?;
    let mut report = report.clone();
    report.set_bytes(field.offset, &[slot.get()]);
    Ok(report)
}

impl SlotStore {
    pub fn new(store: Arc<StateStore>) -> Self {
        Self { store }
    }

    fn rel(leakshield: &str, slot: Slot) -> String {
        format!("leakshield/{leakshield}/slot-{slot}")
    }

    fn dir(&self, leakshield: &str, slot: Slot) -> PathBuf {
        self.store.path(&Self::rel(leakshield, slot))
    }

    /// True when the slot holds a report 3.
    pub fn exists(&self, leakshield: &str, slot: Slot) -> bool {
        self.dir(leakshield, slot).join("report3.bin").exists()
    }

    /// Stores `report` as slot `slot`: the `device_profile` byte takes the slot number and the CRC
    /// is valid. The file `report3.bin` holds the whole report with its report ID byte.
    pub fn store(
        &self,
        registry: &Registry,
        leakshield: &str,
        slot: Slot,
        report: &Report3,
        meta: &SlotMeta,
    ) -> io::Result<()> {
        let report = with_slot_number(registry, report, slot)?;
        let dir = self.dir(leakshield, slot);
        create_dir_all_with(&RealFs, &dir)?;
        write_atomic(&dir.join("report3.bin"), &report.encode())?;
        self.store
            .save(&format!("{}/meta.json", Self::rel(leakshield, slot)), meta)?;
        Ok(())
    }

    /// Sets or clears the `edited` mark of a slot.
    pub fn set_edited(&self, leakshield: &str, slot: Slot, edited: bool) -> io::Result<()> {
        let file = format!("{}/meta.json", Self::rel(leakshield, slot));
        let mut meta = self.store.load::<SlotMeta>(&file).value;
        if meta.edited != edited {
            meta.edited = edited;
            self.store.save(&file, &meta)?;
        }
        Ok(())
    }

    /// Creates the files of the slots that have none, each with a copy of `report` and its own
    /// number. Returns true when it created a slot. The first read of a Leakshield calls it.
    pub fn init(
        &self,
        registry: &Registry,
        leakshield: &str,
        report: &Report3,
        meta: &SlotMeta,
    ) -> io::Result<bool> {
        let mut created = false;
        for slot in Slot::all().filter(|s| !self.exists(leakshield, *s)) {
            self.store(registry, leakshield, slot, report, meta)?;
            created = true;
        }
        Ok(created)
    }

    /// `None` when the slot has no files. An invalid report is an error.
    pub fn load(&self, leakshield: &str, slot: Slot) -> io::Result<Option<StoredSlot>> {
        let bytes = match fs::read(self.dir(leakshield, slot).join("report3.bin")) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e),
        };
        let report3 = Report3::decode(&bytes).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("slot {slot} of {leakshield}: {e}"),
            )
        })?;
        let meta = self
            .store
            .load::<SlotMeta>(&format!("{}/meta.json", Self::rel(leakshield, slot)))
            .value;
        Ok(Some(StoredSlot { report3, meta }))
    }

    /// Stores a display asset of the slot.
    pub fn write_asset(
        &self,
        leakshield: &str,
        slot: Slot,
        asset: &str,
        bytes: &[u8],
    ) -> io::Result<()> {
        if !is_asset_id(asset) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "an asset id contains only [a-z0-9_]",
            ));
        }
        let dir = self.dir(leakshield, slot).join("assets");
        create_dir_all_with(&RealFs, &dir)?;
        write_atomic(&dir.join(format!("{asset}.bin")), bytes)
    }

    pub fn read_asset(&self, leakshield: &str, slot: Slot, asset: &str) -> io::Result<Vec<u8>> {
        if !is_asset_id(asset) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "an asset id contains only [a-z0-9_]",
            ));
        }
        fs::read(
            self.dir(leakshield, slot)
                .join("assets")
                .join(format!("{asset}.bin")),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("nicecooling-slots-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn device_report() -> Report3 {
        Report3::decode(include_bytes!(
            "../../../../tests/fixtures/leakshield/fw1022/report3.bin"
        ))
        .unwrap()
    }

    fn slots(dir: &Path) -> SlotStore {
        SlotStore::new(Arc::new(StateStore::new(dir)))
    }

    #[test]
    fn stored_report3_carries_slot_number_with_valid_crc() {
        let dir = temp("number");
        let registry = Registry::leakshield();
        let store = slots(&dir);
        let meta = SlotMeta::new(1022, 100, "20417-31072");
        store
            .store(
                &registry,
                "leakshield_20417_31072",
                Slot::new(3).unwrap(),
                &device_report(),
                &meta,
            )
            .unwrap();
        let file = dir.join("leakshield/leakshield_20417_31072/slot-3/report3.bin");
        let bytes = fs::read(file).unwrap();
        // A valid CRC and the report ID byte come back through the codec.
        let report = Report3::decode(&bytes).unwrap();
        let offset = registry.field("device_profile").unwrap().offset;
        assert_eq!(report.bytes_at(offset, 1), [3]);
        // Nothing else changed.
        let original = device_report();
        let changed = report.differing_offsets(&original, &[]);
        assert!(changed.iter().all(|o| *o == offset), "{changed:?}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn roundtrip_with_asset() {
        let dir = temp("asset");
        let registry = Registry::leakshield();
        let store = slots(&dir);
        let meta = SlotMeta::new(1022, 100, "20417-31072");
        let slot = Slot::new(2).unwrap();
        let id = "leakshield_20417_31072";
        store
            .store(&registry, id, slot, &device_report(), &meta)
            .unwrap();
        store
            .write_asset(id, slot, "logo", &[1, 2, 3, 250])
            .unwrap();

        // A new store on the same directory reads the same slot.
        let loaded = slots(&dir).load(id, slot).unwrap().unwrap();
        assert_eq!(loaded.meta, meta);
        assert_eq!(
            loaded.report3,
            with_slot_number(&registry, &device_report(), slot).unwrap()
        );
        assert_eq!(
            slots(&dir).read_asset(id, slot, "logo").unwrap(),
            [1, 2, 3, 250]
        );
        // An asset id is not a path.
        assert!(store.write_asset(id, slot, "../x", &[0]).is_err());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn init_creates_four_slots_and_keeps_existing_ones() {
        let dir = temp("init");
        let registry = Registry::leakshield();
        let store = slots(&dir);
        let meta = SlotMeta::new(1022, 100, "20417-31072");
        let id = "leakshield_20417_31072";
        assert!(store.init(&registry, id, &device_report(), &meta).unwrap());
        for slot in Slot::all() {
            let stored = store.load(id, slot).unwrap().unwrap();
            let offset = registry.field("device_profile").unwrap().offset;
            assert_eq!(stored.report3.bytes_at(offset, 1), [slot.get()]);
        }
        assert!(!store.init(&registry, id, &device_report(), &meta).unwrap());
        fs::remove_dir_all(dir).unwrap();
    }
}
