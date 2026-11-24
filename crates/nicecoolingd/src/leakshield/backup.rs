//! Backups of reports 3, 5 and 8 (`docs/storage.md`): one directory for each distinct content,
//! the first backup of a serial and the 20 newest.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use nicecooling_api::ids::leakshield_id;
use nicecooling_protocol::ReportError;
use nicecooling_protocol::report3::{REPORT3_SIZE, Report3};
use nicecooling_protocol::report5::{REPORT5_SIZE, Report5};
use nicecooling_protocol::report8::{REPORT8_SIZE, Report8};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::state::atomic::{RealFs, create_dir_all_with, write_atomic};

/// Backups kept besides the first one.
pub const KEEP_NEWEST: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    First,
    BeforeWrite,
    Manual,
    Import,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::First => "first",
            Self::BeforeWrite => "before-write",
            Self::Manual => "manual",
            Self::Import => "import",
        }
    }
}

/// The three reports of one backup, each with its report ID byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupSet {
    pub report3: Vec<u8>,
    pub report5: Vec<u8>,
    pub report8: Vec<u8>,
}

impl BackupSet {
    fn sha256(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(&self.report3);
        hash.update(&self.report5);
        hash.update(&self.report8);
        hash.finalize().iter().map(|b| format!("{b:02x}")).collect()
    }

    /// Sizes, report IDs and CRCs, as the codecs check them.
    fn validate(&self) -> Result<(), ImportError> {
        Report3::decode(&self.report3).map_err(|e| ImportError::Invalid {
            report: 3,
            error: e,
        })?;
        Report5::decode(&self.report5).map_err(|e| ImportError::Invalid {
            report: 5,
            error: e,
        })?;
        Report8::decode(&self.report8).map_err(|e| ImportError::Invalid {
            report: 8,
            error: e,
        })?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Meta {
    schema_version: u32,
    created_us: u64,
    leakshield: String,
    serial: String,
    firmware: u16,
    reason: String,
    sha256: String,
}

/// What `ListBackups` shows of a backup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupEntry {
    pub id: String,
    pub created_us: u64,
    pub reason: String,
    pub firmware: u16,
    pub sha256: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Saved {
    pub id: String,
    /// False when a backup with the same hash exists. `id` is then that backup.
    pub written: bool,
}

#[derive(Debug)]
pub enum ImportError {
    Invalid {
        report: u8,
        error: ReportError,
    },
    /// The structure id of the imported report 3 is not the one of the device.
    OtherStructure {
        backup: u16,
        device: u16,
    },
    Io(io::Error),
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid { report, error } => write!(f, "report {report}: {error}"),
            Self::OtherStructure { backup, device } => {
                write!(f, "structure {backup} does not match the device ({device})")
            }
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for ImportError {}

impl From<io::Error> for ImportError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

/// The backups of one Leakshield.
pub struct Backups {
    dir: PathBuf,
    leakshield: String,
    serial: String,
}

impl Backups {
    /// `state_dir` is `/var/lib/nicecooling`. `serial` is the text form, such as `20417-31072`.
    pub fn new(state_dir: &Path, serial: &str) -> Self {
        let leakshield = leakshield_id(serial);
        Self {
            dir: state_dir.join("backups").join(&leakshield),
            leakshield,
            serial: serial.to_owned(),
        }
    }

    /// Saves a backup unless one with the same hash exists, then applies the retention. The first
    /// backup of a serial has the reason `first`.
    pub fn save(
        &self,
        set: &BackupSet,
        firmware: u16,
        reason: Reason,
        now_us: u64,
    ) -> io::Result<Saved> {
        let sizes = [
            (set.report3.len(), REPORT3_SIZE),
            (set.report5.len(), REPORT5_SIZE),
            (set.report8.len(), REPORT8_SIZE),
        ];
        if sizes.iter().any(|(actual, expected)| actual != expected) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "wrong report size",
            ));
        }
        let sha256 = set.sha256();
        let existing = self.list_all()?;
        if let Some((id, _)) = existing.iter().find(|(_, meta)| meta.sha256 == sha256) {
            return Ok(Saved {
                id: id.clone(),
                written: false,
            });
        }
        // The first backup that comes from the device. An import never holds the place.
        let has_first = existing
            .iter()
            .any(|(_, m)| m.reason == Reason::First.as_str());
        let reason = if reason != Reason::Import && !has_first {
            Reason::First
        } else {
            reason
        };
        let id = format!("{}-{}", utc_stamp(now_us), &sha256[..12]);
        let dir = self.dir.join(&id);
        create_dir_all_with(&RealFs, &dir)?;
        write_atomic(&dir.join("report3.bin"), &set.report3)?;
        write_atomic(&dir.join("report5.bin"), &set.report5)?;
        write_atomic(&dir.join("report8.bin"), &set.report8)?;
        let meta = Meta {
            schema_version: 1,
            created_us: now_us,
            leakshield: self.leakshield.clone(),
            serial: self.serial.clone(),
            firmware,
            reason: reason.as_str().to_owned(),
            sha256,
        };
        let json = serde_json::to_vec(&meta).map_err(io::Error::other)?;
        // The meta file comes last: a directory without it is an unfinished backup.
        write_atomic(&dir.join("meta.json"), &json)?;
        self.apply_retention(&id)?;
        Ok(Saved { id, written: true })
    }

    /// Saves a backup of reports that a client sends. They need valid sizes and CRCs and the
    /// structure id of the device.
    pub fn import(
        &self,
        set: &BackupSet,
        device_structure: u16,
        firmware: u16,
        now_us: u64,
    ) -> Result<Saved, ImportError> {
        set.validate()?;
        let backup = Report3::decode(&set.report3)
            .map_err(|e| ImportError::Invalid {
                report: 3,
                error: e,
            })?
            .structure_number();
        if backup != device_structure {
            return Err(ImportError::OtherStructure {
                backup,
                device: device_structure,
            });
        }
        Ok(self.save(set, firmware, Reason::Import, now_us)?)
    }

    /// Oldest first.
    pub fn list(&self) -> io::Result<Vec<BackupEntry>> {
        Ok(self
            .list_all()?
            .into_iter()
            .map(|(id, meta)| BackupEntry {
                id,
                created_us: meta.created_us,
                reason: meta.reason,
                firmware: meta.firmware,
                sha256: meta.sha256,
            })
            .collect())
    }

    pub fn get(&self, id: &str) -> io::Result<BackupSet> {
        let dir = self.backup_dir(id)?;
        Ok(BackupSet {
            report3: fs::read(dir.join("report3.bin"))?,
            report5: fs::read(dir.join("report5.bin"))?,
            // A backup without report 8 gives an empty report: a restore writes report 3 only.
            report8: match fs::read(dir.join("report8.bin")) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => Vec::new(),
                other => other?,
            },
        })
    }

    pub fn firmware(&self, id: &str) -> io::Result<u16> {
        Ok(self.read_meta(&self.backup_dir(id)?)?.firmware)
    }

    /// The id comes from a client, so it must be a name that `save` makes.
    fn backup_dir(&self, id: &str) -> io::Result<PathBuf> {
        let ok = id.len() == 16 + 1 + 12
            && id.bytes().all(|b| {
                b.is_ascii_digit()
                    || b == b'T'
                    || b == b'Z'
                    || b == b'-'
                    || (b'a'..=b'f').contains(&b)
            });
        if !ok {
            return Err(io::Error::new(io::ErrorKind::NotFound, "no such backup"));
        }
        Ok(self.dir.join(id))
    }

    fn read_meta(&self, dir: &Path) -> io::Result<Meta> {
        let meta: Meta = serde_json::from_slice(&fs::read(dir.join("meta.json"))?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        Ok(meta)
    }

    /// Backups with a readable `meta.json`, oldest first.
    fn list_all(&self) -> io::Result<Vec<(String, Meta)>> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut backups = Vec::new();
        for entry in entries {
            let entry = entry?;
            let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if let Ok(meta) = self.read_meta(&entry.path()) {
                backups.push((id, meta));
            }
        }
        backups.sort_by(|a, b| (a.1.created_us, &a.0).cmp(&(b.1.created_us, &b.0)));
        Ok(backups)
    }

    /// Keeps the first backup and the 20 newest. `keep` is the backup that was just saved: a PC
    /// clock that went back makes it look old, and a write result points at it.
    fn apply_retention(&self, keep: &str) -> io::Result<()> {
        let rest: Vec<String> = self
            .list_all()?
            .into_iter()
            .filter(|(_, meta)| meta.reason != Reason::First.as_str())
            .map(|(id, _)| id)
            .collect();
        let old = rest.len().saturating_sub(KEEP_NEWEST);
        for id in rest[..old].iter().filter(|id| *id != keep) {
            fs::remove_dir_all(self.dir.join(id))?;
        }
        Ok(())
    }
}

/// `yyyymmddThhmmssZ` of a time in microseconds since the Unix epoch.
fn utc_stamp(us: u64) -> String {
    let secs = us / 1_000_000;
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    // Civil date from a day count (H. Hinnant, "chrono-compatible low-level date algorithms").
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}Z",
        rest / 3_600,
        rest % 3_600 / 60,
        rest % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stamp_of_known_times() {
        assert_eq!(utc_stamp(0), "19700101T000000Z");
        assert_eq!(utc_stamp(1_700_000_000_000_000), "20231114T221320Z");
        assert_eq!(utc_stamp(1_709_251_199_000_000), "20240229T235959Z");
    }
}
