//! "Start at login" (`docs/storage.md`, Start at login). The package installs the system
//! autostart entry. The setting is the user override file with the same name.

use std::io::Write as _;
use std::path::{Path, PathBuf};

pub const ENTRY: &str = "io.github.cappyt.NiceCooling.desktop";

const OVERRIDE_OFF: &str = "[Desktop Entry]\nType=Application\nName=NiceCooling\nHidden=true\n";

fn override_path(dir: &Path) -> PathBuf {
    dir.join(ENTRY)
}

/// On unless an override with `Hidden=true` exists in `dir`.
pub fn is_enabled(dir: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(override_path(dir)) else {
        return true;
    };
    !text.lines().any(|line| line.trim() == "Hidden=true")
}

/// Off writes the override with `Hidden=true`. On removes the override file.
pub fn set_enabled(dir: &Path, enabled: bool) -> std::io::Result<()> {
    let path = override_path(dir);
    if enabled {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    std::fs::create_dir_all(dir)?;
    let temporary = path.with_extension("desktop.tmp");
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(OVERRIDE_OFF.as_bytes())?;
    file.sync_all()?;
    std::fs::rename(&temporary, &path)?;
    std::fs::File::open(dir)?.sync_all()
}
