//! `secrets.toml`: the MQTT and SMTP passwords (`docs/storage.md`). The file has the mode 0600.
//! The app never shows a stored password, never logs one and never exports one.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::app_settings::write_atomic;

pub const SCHEMA_VERSION: u32 = 1;

/// A password. Its `Debug` form holds no text.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Password(String);

impl Password {
    pub fn new(text: &str) -> Self {
        Self(text.to_owned())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_set(&self) -> bool {
        !self.0.is_empty()
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(if self.is_set() {
            "Password(set)"
        } else {
            "Password(unset)"
        })
    }
}

/// What a password field of the settings gives back.
#[derive(Clone, PartialEq, Eq)]
pub enum PasswordEdit {
    /// The user did not touch the field: the stored password stays.
    Untouched,
    /// The user typed in the field. An empty text clears the password.
    Set(String),
}

impl fmt::Debug for PasswordEdit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Untouched => "PasswordEdit::Untouched",
            Self::Set(_) => "PasswordEdit::Set(..)",
        })
    }
}

impl Password {
    fn edit(&mut self, edit: PasswordEdit) {
        if let PasswordEdit::Set(text) = edit {
            self.0 = text;
        }
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Section {
    password: String,
}

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct File {
    schema_version: u32,
    mqtt: Section,
    smtp: Section,
}

impl Default for File {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            mqtt: Section::default(),
            smtp: Section::default(),
        }
    }
}

/// The passwords and the file that holds them.
#[derive(Clone, Debug)]
pub struct Secrets {
    path: PathBuf,
    pub mqtt: Password,
    pub smtp: Password,
    /// Why the file cannot be used. The app writes no new content over such a file.
    pub error: Option<String>,
}

impl Secrets {
    /// Read the file. A missing file gives no passwords. A file that does not parse, or that has
    /// a higher schema version, gives no passwords and an error, and stays as it is.
    pub fn open(path: &Path) -> Self {
        let mut secrets = Self {
            path: path.to_owned(),
            mqtt: Password::default(),
            smtp: Password::default(),
            error: None,
        };
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return secrets,
            Err(error) => {
                secrets.error = Some(error.to_string());
                return secrets;
            }
        };
        match toml::from_str::<File>(&text) {
            Ok(file) if file.schema_version > SCHEMA_VERSION => {
                secrets.error = Some(format!(
                    "the file has the schema version {}, and this version knows {SCHEMA_VERSION}",
                    file.schema_version
                ));
            }
            Ok(file) => {
                secrets.mqtt = Password(file.mqtt.password);
                secrets.smtp = Password(file.smtp.password);
            }
            // The message of a TOML error can quote the line, and the line holds a password.
            Err(_) => secrets.error = Some("the file is not valid TOML".to_owned()),
        }
        secrets
    }

    pub fn edit_mqtt(&mut self, edit: PasswordEdit) {
        self.mqtt.edit(edit);
    }

    pub fn edit_smtp(&mut self, edit: PasswordEdit) {
        self.smtp.edit(edit);
    }

    /// Write the file with the mode 0600.
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(error) = &self.error {
            return Err(std::io::Error::other(error.clone()));
        }
        let file = File {
            schema_version: SCHEMA_VERSION,
            mqtt: Section {
                password: self.mqtt.0.clone(),
            },
            smtp: Section {
                password: self.smtp.0.clone(),
            },
        };
        let text = toml::to_string(&file).map_err(std::io::Error::other)?;
        write_atomic(&self.path, text.as_bytes(), 0o600)
    }
}
