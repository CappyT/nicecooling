use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use serde::de::DeserializeOwned;

use super::atomic::{RealFs, create_dir_all_with, write_atomic};

/// Where a loaded value came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Primary,
    LastKnownGood,
    /// No file was usable. The value is the default.
    Defaults,
}

#[derive(Debug)]
pub struct Loaded<T> {
    pub value: T,
    pub source: Source,
    /// The daemon raises the `state-fallback` Warning.
    pub fallback: bool,
}

/// What `save` must do for a file after a fallback load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Guard {
    /// The file has a higher `schema_version`. It is never written.
    ReadOnly,
    /// The file is unreadable. The next write moves it and its copy aside.
    KeepBroken,
}

enum Read<T> {
    Missing,
    TooNew,
    Corrupt,
    Ok(T),
}

/// The files of `/var/lib/nicecooling/` (`docs/storage.md`). JSON files carry a
/// `schema_version` and have a last-known-good copy `<file>.lkg`.
pub struct StateStore {
    dir: PathBuf,
    supported: u64,
    guards: Mutex<BTreeMap<String, Guard>>,
}

impl StateStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            supported: 1,
            guards: Mutex::default(),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.dir.join(rel)
    }

    fn read<T: DeserializeOwned>(&self, path: &Path) -> Read<T> {
        let bytes = match fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Read::Missing,
            Err(_) => return Read::Corrupt,
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return Read::Corrupt;
        };
        match value.get("schema_version").and_then(|v| v.as_u64()) {
            Some(v) if v > self.supported => Read::TooNew,
            Some(_) => serde_json::from_value(value).map_or(Read::Corrupt, Read::Ok),
            None => Read::Corrupt,
        }
    }

    fn guard(&self, rel: &str, guard: Guard) {
        self.guards.lock().unwrap().insert(rel.to_owned(), guard);
    }

    /// Loads a JSON file: the file, else its last-known-good copy, else the default.
    /// After a load of the file itself, the copy is refreshed when it differs.
    pub fn load<T: DeserializeOwned + Default>(&self, rel: &str) -> Loaded<T> {
        let path = self.path(rel);
        let lkg = lkg_path(&path);
        let primary = self.read::<T>(&path);
        if let Read::Ok(value) = primary {
            if fs::read(&path).ok() != fs::read(&lkg).ok()
                && let Ok(bytes) = fs::read(&path)
            {
                let _ = write_atomic(&lkg, &bytes);
            }
            return Loaded {
                value,
                source: Source::Primary,
                fallback: false,
            };
        }
        let copy = self.read::<T>(&lkg);
        match (primary, copy) {
            (Read::Missing, Read::Missing) => Loaded {
                value: T::default(),
                source: Source::Defaults,
                fallback: false,
            },
            (primary, Read::Ok(value)) => {
                self.guard(
                    rel,
                    if matches!(primary, Read::TooNew) {
                        Guard::ReadOnly
                    } else {
                        Guard::KeepBroken
                    },
                );
                Loaded {
                    value,
                    source: Source::LastKnownGood,
                    fallback: true,
                }
            }
            (primary, _) => {
                self.guard(
                    rel,
                    if matches!(primary, Read::TooNew) {
                        Guard::ReadOnly
                    } else {
                        Guard::KeepBroken
                    },
                );
                Loaded {
                    value: T::default(),
                    source: Source::Defaults,
                    fallback: true,
                }
            }
        }
    }

    /// Writes a JSON file atomically. Returns false when the file has a higher
    /// `schema_version` and the daemon must not write it.
    pub fn save<T: Serialize>(&self, rel: &str, value: &T) -> io::Result<bool> {
        let path = self.path(rel);
        let guard = self.guards.lock().unwrap().get(rel).copied();
        match guard {
            Some(Guard::ReadOnly) => return Ok(false),
            Some(Guard::KeepBroken) => {
                for file in [path.clone(), lkg_path(&path)] {
                    if file.exists() {
                        let mut broken = file.clone().into_os_string();
                        broken.push(".broken");
                        fs::rename(&file, broken)?;
                    }
                }
                self.guards.lock().unwrap().remove(rel);
            }
            None => {}
        }
        if let Some(parent) = path.parent() {
            create_dir_all_with(&RealFs, parent)?;
        }
        write_atomic(
            &path,
            &serde_json::to_vec_pretty(value).map_err(io::Error::other)?,
        )?;
        Ok(true)
    }
}

pub fn lkg_path(path: &Path) -> PathBuf {
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".lkg");
    path.with_file_name(name)
}
