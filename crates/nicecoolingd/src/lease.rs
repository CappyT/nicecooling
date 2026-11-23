use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::state::atomic::write_atomic;

pub const LEASE_FILE: &str = "lease.json";

#[derive(Serialize)]
struct LeaseFile<'a> {
    schema_version: u32,
    created_us: u64,
    boot_id: &'a str,
}

/// The takeover lease: it exists from before the first manual write until a restore
/// whose read-back matches the BIOS snapshot.
#[derive(Clone, Debug)]
pub struct Lease {
    path: PathBuf,
}

impl Lease {
    pub fn new(state_dir: &Path) -> Self {
        Self {
            path: state_dir.join(LEASE_FILE),
        }
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Writes the lease unless it exists. The existing lease keeps its time.
    pub fn create(&self, boot_id: &str, now_us: u64) -> io::Result<()> {
        if self.exists() {
            return Ok(());
        }
        let file = LeaseFile {
            schema_version: 1,
            created_us: now_us,
            boot_id,
        };
        write_atomic(
            &self.path,
            &serde_json::to_vec(&file).map_err(io::Error::other)?,
        )
    }

    pub fn remove(&self) -> io::Result<()> {
        match fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        }
        fs::File::open(self.path.parent().unwrap_or(Path::new(".")))?.sync_all()
    }
}
