//! The app settings file `$XDG_CONFIG_HOME/nicecooling/settings.toml` (`docs/storage.md`).
//! The app is the only writer. A missing or unreadable file gives the defaults.

use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::Language;
use crate::mqtt::settings::{MqttSettings, lenient};
use crate::notify::NotifyPrefs;

pub const SCHEMA_VERSION: u32 = 1;

/// Hint ids that "Do not show this again" stores.
pub const HINT_APPINDICATOR: &str = "appindicator";
pub const HINT_IMMEDIATE_SWITCHING: &str = "immediate_switching_warning";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Notifications {
    pub warnings: bool,
    pub profile_switched_by_rule: bool,
    pub external_change: bool,
}

impl Default for Notifications {
    fn default() -> Self {
        let prefs = NotifyPrefs::default();
        Self {
            warnings: prefs.warnings,
            profile_switched_by_rule: prefs.profile_switched_by_rule,
            external_change: prefs.external_change,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hints {
    pub dismissed: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct HiddenSensors {
    /// The sensors that the Sensors screen hides.
    pub hidden: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub schema_version: u32,
    /// `system`, `en` or `it`.
    pub language: String,
    pub tray: bool,
    pub notifications: Notifications,
    pub hints: Hints,
    pub sensors: HiddenSensors,
    /// A bad table gives the default table and leaves the other settings.
    #[serde(deserialize_with = "lenient")]
    pub mqtt: MqttSettings,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            language: "system".to_owned(),
            tray: true,
            notifications: Notifications::default(),
            hints: Hints::default(),
            sensors: HiddenSensors::default(),
            mqtt: MqttSettings::default(),
        }
    }
}

impl AppSettings {
    /// Read the file. Any failure gives the defaults.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Write the file atomically (`write_atomic`).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let text = toml::to_string(self).map_err(std::io::Error::other)?;
        write_atomic(path, text.as_bytes(), 0o666)
    }

    pub fn language(&self) -> Language {
        Language::from_setting(&self.language)
    }

    pub fn notify_prefs(&self) -> NotifyPrefs {
        NotifyPrefs {
            warnings: self.notifications.warnings,
            profile_switched_by_rule: self.notifications.profile_switched_by_rule,
            external_change: self.notifications.external_change,
        }
    }

    pub fn hint_dismissed(&self, id: &str) -> bool {
        self.hints.dismissed.iter().any(|hint| hint == id)
    }

    pub fn dismiss_hint(&mut self, id: &str) {
        if !self.hint_dismissed(id) {
            self.hints.dismissed.push(id.to_owned());
        }
    }

    pub fn hidden_sensors(&self) -> std::collections::BTreeSet<String> {
        self.sensors.hidden.iter().cloned().collect()
    }

    pub fn set_hidden_sensors(&mut self, hidden: &std::collections::BTreeSet<String>) {
        self.sensors.hidden = hidden.iter().cloned().collect();
    }
}

/// Write a file atomically: temporary file `<name>.tmp`, `fsync`, rename, `fsync` of the
/// directory. The temporary file is new, so `mode` (before the umask) holds at no moment
/// wider. A settings file passes 0o666 and a file with a secret passes 0o600.
pub fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut name = path.file_name().unwrap_or_default().to_os_string();
    name.push(".tmp");
    let temporary = path.with_file_name(name);
    // A temporary file that an earlier crash left keeps its old mode, so it goes first.
    let _ = std::fs::remove_file(&temporary);
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    std::fs::rename(&temporary, path)?;
    std::fs::File::open(dir)?.sync_all()
}

pub fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(std::env::temp_dir)
}

pub fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(std::env::temp_dir)
}

/// `$XDG_CONFIG_HOME/nicecooling/settings.toml`.
pub fn settings_path() -> PathBuf {
    config_home().join("nicecooling/settings.toml")
}

/// The user autostart directory: `$XDG_CONFIG_HOME/autostart`.
pub fn autostart_dir() -> PathBuf {
    config_home().join("autostart")
}

/// The settings file, the autostart override and the values that the service threads share.
/// A store without paths (a test, or a failed start) keeps the values in memory only.
#[derive(Clone, Debug)]
pub struct SettingsStore {
    pub values: AppSettings,
    pub path: Option<PathBuf>,
    pub autostart_dir: Option<PathBuf>,
    /// The notification thread reads these at each event.
    pub notify_prefs: std::sync::Arc<std::sync::Mutex<NotifyPrefs>>,
    /// False when no StatusNotifier host takes the tray icon.
    pub tray_visible: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// The last write that failed.
    pub error: Option<String>,
}

impl Default for SettingsStore {
    fn default() -> Self {
        Self::new(AppSettings::default(), None, None)
    }
}

impl SettingsStore {
    pub fn new(values: AppSettings, path: Option<PathBuf>, autostart_dir: Option<PathBuf>) -> Self {
        let prefs = values.notify_prefs();
        Self {
            values,
            path,
            autostart_dir,
            notify_prefs: std::sync::Arc::new(std::sync::Mutex::new(prefs)),
            tray_visible: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            error: None,
        }
    }

    /// The store of the user: the real settings file and autostart directory.
    pub fn open() -> Self {
        let path = settings_path();
        Self::new(AppSettings::load(&path), Some(path), Some(autostart_dir()))
    }

    /// Change a value and write the file at once.
    pub fn update(&mut self, change: impl FnOnce(&mut AppSettings)) {
        change(&mut self.values);
        if let Ok(mut prefs) = self.notify_prefs.lock() {
            *prefs = self.values.notify_prefs();
        }
        self.error = self
            .path
            .as_ref()
            .and_then(|path| self.values.save(path).err())
            .map(|error| error.to_string());
    }

    /// The "Start at login" switch. It follows the override file when the store has a directory.
    pub fn start_at_login(&self) -> bool {
        self.autostart_dir
            .as_ref()
            .is_none_or(|dir| crate::autostart::is_enabled(dir))
    }

    pub fn set_start_at_login(&mut self, enabled: bool) {
        if let Some(dir) = &self.autostart_dir {
            self.error = crate::autostart::set_enabled(dir, enabled)
                .err()
                .map(|error| error.to_string());
        }
    }

    pub fn tray_hint_needed(&self) -> bool {
        self.values.tray
            && !self.tray_visible.load(std::sync::atomic::Ordering::Relaxed)
            && !self.values.hint_dismissed(HINT_APPINDICATOR)
    }
}
