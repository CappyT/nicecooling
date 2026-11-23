//! The record of a firmware update that is not finished: `leakshield/<id>/firmware-update.json`
//! (`docs/storage.md`, design D8). It has no last-known-good copy. A file that is damaged or
//! has a higher schema version counts as absent.

use std::fs;
use std::io;
use std::path::PathBuf;

use nicecooling_protocol::firmware::bootloader::Language;
use serde::{Deserialize, Serialize};

use super::checks::RecordIdentity;
use crate::state::atomic::{RealFs, create_dir_all_with, write_atomic};

pub const FILE: &str = "firmware-update.json";
pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ImageLanguage {
    En,
    De,
}

impl From<Language> for ImageLanguage {
    fn from(language: Language) -> Self {
        match language {
            Language::English => Self::En,
            Language::German => Self::De,
        }
    }
}

/// The image that the update flashes. The record holds no image bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordImage {
    pub id: i64,
    pub version: u16,
    pub language: ImageLanguage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateRecord {
    pub serial: Option<String>,
    pub usb_path: String,
    pub hardware: Option<u16>,
    pub bootloader: Option<u16>,
    pub firmware_before: Option<u16>,
    pub backup: Option<String>,
    pub started_us: u64,
    pub image: RecordImage,
}

impl From<&UpdateRecord> for RecordIdentity {
    fn from(record: &UpdateRecord) -> Self {
        Self {
            hardware: record.hardware,
            bootloader: record.bootloader,
            firmware_before: record.firmware_before,
        }
    }
}

#[derive(Serialize)]
struct OnDisk<'a> {
    schema_version: u32,
    #[serde(flatten)]
    record: &'a UpdateRecord,
}

#[derive(Debug, Clone)]
pub struct RecordStore {
    dir: PathBuf,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id != "." && id != ".." && !id.contains('/')
}

impl RecordStore {
    /// `dir` is the state directory.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    fn leakshield_dir(&self, id: &str) -> PathBuf {
        self.dir.join("leakshield").join(id)
    }

    pub fn path(&self, id: &str) -> PathBuf {
        self.leakshield_dir(id).join(FILE)
    }

    pub fn write(&self, id: &str, record: &UpdateRecord) -> io::Result<()> {
        if !valid_id(id) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "bad Leakshield id",
            ));
        }
        let bytes = serde_json::to_vec_pretty(&OnDisk {
            schema_version: SCHEMA_VERSION,
            record,
        })
        .map_err(io::Error::other)?;
        create_dir_all_with(&RealFs, &self.leakshield_dir(id))?;
        write_atomic(&self.path(id), &bytes)
    }

    /// The record of one Leakshield, or `None` when there is none, or when it is damaged or newer
    /// than this daemon knows.
    pub fn read(&self, id: &str) -> Option<UpdateRecord> {
        if !valid_id(id) {
            return None;
        }
        let path = self.path(id);
        let bytes = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
            Err(e) => {
                eprintln!(
                    "nicecoolingd: the update record {} is unreadable: {e}",
                    path.display()
                );
                return None;
            }
        };
        let parsed = serde_json::from_slice::<serde_json::Value>(&bytes)
            .map_err(|e| e.to_string())
            .and_then(|value| {
                match value
                    .get("schema_version")
                    .and_then(serde_json::Value::as_u64)
                {
                    Some(v) if v <= u64::from(SCHEMA_VERSION) => {}
                    Some(v) => return Err(format!("schema version {v} is newer than this daemon")),
                    None => return Err("no schema_version".to_owned()),
                }
                serde_json::from_value::<UpdateRecord>(value).map_err(|e| e.to_string())
            });
        match parsed {
            Ok(record) => Some(record),
            Err(why) => {
                eprintln!(
                    "nicecoolingd: the update record {} is ignored: {why}",
                    path.display()
                );
                None
            }
        }
    }

    /// Removes the record of one Leakshield. A missing record is not an error.
    pub fn remove(&self, id: &str) -> io::Result<()> {
        if !valid_id(id) {
            return Ok(());
        }
        match fs::remove_file(self.path(id)) {
            Ok(()) => fs::File::open(self.leakshield_dir(id))?.sync_all(),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        }
    }

    /// Every readable record, by Leakshield id.
    pub fn all(&self) -> Vec<(String, UpdateRecord)> {
        let Ok(entries) = fs::read_dir(self.dir.join("leakshield")) else {
            return Vec::new();
        };
        let mut found: Vec<(String, UpdateRecord)> = entries
            .filter_map(Result::ok)
            .filter_map(|e| e.file_name().into_string().ok())
            .filter_map(|id| self.read(&id).map(|record| (id, record)))
            .collect();
        found.sort_by(|a, b| a.0.cmp(&b.0));
        found
    }

    pub fn find_by_usb_path(&self, usb_path: &str) -> Option<(String, UpdateRecord)> {
        self.all().into_iter().find(|(_, r)| r.usb_path == usb_path)
    }

    pub fn find_by_serial(&self, serial: &str) -> Option<(String, UpdateRecord)> {
        self.all()
            .into_iter()
            .find(|(_, r)| r.serial.as_deref() == Some(serial))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TempDir(PathBuf);

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn temp(name: &str) -> TempDir {
        let dir =
            std::env::temp_dir().join(format!("nicecooling-record-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn record(serial: Option<&str>, usb_path: &str) -> UpdateRecord {
        UpdateRecord {
            serial: serial.map(str::to_owned),
            usb_path: usb_path.to_owned(),
            hardware: Some(1000),
            bootloader: Some(100),
            firmware_before: Some(1022),
            backup: Some("20261123T100000Z-0123456789ab".to_owned()),
            started_us: 1_700_000_000_000_000,
            image: RecordImage {
                id: 27,
                version: 1022,
                language: ImageLanguage::De,
            },
        }
    }

    #[test]
    fn record_round_trip() {
        let dir = temp("round");
        let store = RecordStore::new(&dir.0);
        let written = record(Some("20417-31072"), "1-3");
        store.write("leakshield_1", &written).unwrap();
        assert_eq!(store.read("leakshield_1"), Some(written));
        let nulls = UpdateRecord {
            serial: None,
            hardware: None,
            bootloader: None,
            firmware_before: None,
            backup: None,
            ..record(None, "1-3")
        };
        store.write("leakshield_1", &nulls).unwrap();
        assert_eq!(store.read("leakshield_1"), Some(nulls));
        assert_eq!(store.read("missing"), None);
    }

    #[test]
    fn record_remove() {
        let dir = temp("remove");
        let store = RecordStore::new(&dir.0);
        store.write("leakshield_1", &record(None, "1-3")).unwrap();
        store.remove("leakshield_1").unwrap();
        assert_eq!(store.read("leakshield_1"), None);
        assert!(!store.path("leakshield_1").exists());
        store.remove("leakshield_1").unwrap();
    }

    #[test]
    fn record_has_no_lkg_copy() {
        let dir = temp("lkg");
        let store = RecordStore::new(&dir.0);
        store.write("leakshield_1", &record(None, "1-3")).unwrap();
        store.write("leakshield_1", &record(None, "1-4")).unwrap();
        let names: Vec<String> = fs::read_dir(dir.0.join("leakshield/leakshield_1"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, [FILE]);
    }

    #[test]
    fn record_holds_the_chosen_image() {
        let dir = temp("image");
        let store = RecordStore::new(&dir.0);
        store
            .write("leakshield_1", &record(Some("1-2"), "1-3"))
            .unwrap();
        let json: serde_json::Value =
            serde_json::from_slice(&fs::read(store.path("leakshield_1")).unwrap()).unwrap();
        assert_eq!(json["schema_version"], 1);
        assert_eq!(
            json["image"],
            serde_json::json!({"id": 27, "version": 1022, "language": "de"})
        );
        assert_eq!(json["usb_path"], "1-3");
        assert_eq!(json["firmware_before"], 1022);
        let image = store.read("leakshield_1").unwrap().image;
        assert_eq!(image.language, ImageLanguage::from(Language::German));
    }

    #[test]
    fn record_found_by_usb_path() {
        let dir = temp("path");
        let store = RecordStore::new(&dir.0);
        store
            .write("leakshield_1", &record(Some("a-b"), "1-3"))
            .unwrap();
        store
            .write("leakshield_2", &record(Some("c-d"), "1-4"))
            .unwrap();
        assert_eq!(store.find_by_usb_path("1-4").unwrap().0, "leakshield_2");
        assert_eq!(store.find_by_usb_path("1-9"), None);
    }

    #[test]
    fn record_found_by_serial() {
        let dir = temp("serial");
        let store = RecordStore::new(&dir.0);
        store
            .write("leakshield_1", &record(Some("a-b"), "1-3"))
            .unwrap();
        store.write("leakshield_2", &record(None, "1-4")).unwrap();
        assert_eq!(store.find_by_serial("a-b").unwrap().0, "leakshield_1");
        assert_eq!(store.find_by_serial("c-d"), None);
    }

    #[test]
    fn damaged_record_counts_as_absent() {
        let dir = temp("damaged");
        let store = RecordStore::new(&dir.0);
        store
            .write("leakshield_1", &record(Some("a-b"), "1-3"))
            .unwrap();
        for garbage in [&b"{ not json"[..], b"", b"{\"schema_version\":1}", b"[1]"] {
            fs::write(store.path("leakshield_1"), garbage).unwrap();
            assert_eq!(store.read("leakshield_1"), None);
            assert_eq!(store.find_by_usb_path("1-3"), None);
            assert!(store.all().is_empty());
        }
    }

    #[test]
    fn higher_schema_version_counts_as_absent() {
        let dir = temp("newer");
        let store = RecordStore::new(&dir.0);
        store
            .write("leakshield_1", &record(Some("a-b"), "1-3"))
            .unwrap();
        let path = store.path("leakshield_1");
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        json["schema_version"] = 2.into();
        let newer = serde_json::to_vec(&json).unwrap();
        fs::write(&path, &newer).unwrap();
        assert_eq!(store.read("leakshield_1"), None);
        assert_eq!(store.find_by_serial("a-b"), None);
        assert_eq!(
            fs::read(&path).unwrap(),
            newer,
            "the daemon does not touch the file"
        );
    }

    #[test]
    fn bad_ids_reach_no_other_directory() {
        let dir = temp("ids");
        let store = RecordStore::new(&dir.0);
        for id in ["", ".", "..", "a/b"] {
            assert!(store.write(id, &record(None, "1-3")).is_err(), "{id:?}");
            assert_eq!(store.read(id), None);
        }
        assert!(!dir.0.join("leakshield").exists());
    }

    #[test]
    fn identity_of_a_record_for_the_checks() {
        let identity = RecordIdentity::from(&record(None, "1-3"));
        assert_eq!(
            (
                identity.hardware,
                identity.bootloader,
                identity.firmware_before
            ),
            (Some(1000), Some(100), Some(1022))
        );
    }
}
