//! The `[smtp]` table of `settings.toml`. The password is in `secrets.toml`, never here.
//! One bad field gives the default of that field and leaves the other settings as they are.

use serde::{Deserialize, Serialize};

use crate::mqtt::settings::lenient;

pub const STARTTLS_PORT: u16 = 587;
pub const TLS_PORT: u16 = 465;
pub const NONE_PORT: u16 = 25;

/// How the connection to the server is secured.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SmtpSecurity {
    /// Plain connection, then `STARTTLS`. The send fails when the server has no `STARTTLS`.
    #[default]
    Starttls,
    /// TLS from the first byte.
    Tls,
    /// No encryption.
    None,
}

impl SmtpSecurity {
    /// The port that this security uses while the setting `port` is empty.
    pub fn default_port(self) -> u16 {
        match self {
            Self::Starttls => STARTTLS_PORT,
            Self::Tls => TLS_PORT,
            Self::None => NONE_PORT,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SmtpSettings {
    /// Empty: no account is set.
    #[serde(deserialize_with = "lenient")]
    pub host: String,
    /// `None`: the default port of the security.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(deserialize_with = "lenient")]
    pub security: SmtpSecurity,
    /// Empty: no login.
    #[serde(deserialize_with = "lenient")]
    pub user: String,
    /// The `From` address.
    #[serde(deserialize_with = "lenient")]
    pub sender: String,
}

impl SmtpSettings {
    /// An account needs a host and a sender address. The e-mail response has the outcome
    /// `not-configured` without one.
    pub fn is_configured(&self) -> bool {
        !self.host.trim().is_empty() && !self.sender.trim().is_empty()
    }

    pub fn effective_port(&self) -> u16 {
        self.port.unwrap_or_else(|| self.security.default_port())
    }

    /// The sender logs in when a user name is set, in every security.
    pub fn logs_in(&self) -> bool {
        !self.user.is_empty()
    }

    /// The settings screen warns about this. The warning never blocks the settings.
    pub fn warns_plain_login(&self) -> bool {
        self.security == SmtpSecurity::None && self.logs_in()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_settings::AppSettings;
    use crate::graphs::secrets::{PasswordEdit, Secrets};

    fn parse(text: &str) -> AppSettings {
        toml::from_str(text).expect("settings parse")
    }

    /// A directory under the temporary directory, removed at the end of the test.
    struct TempDir(std::path::PathBuf);

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("nicecooling-smtp-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).expect("test directory");
            Self(path)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn smtp_default_ports() {
        assert_eq!(SmtpSecurity::Starttls.default_port(), 587);
        assert_eq!(SmtpSecurity::Tls.default_port(), 465);
        assert_eq!(SmtpSecurity::None.default_port(), 25);
        let default = AppSettings::default().smtp;
        assert_eq!(default.security, SmtpSecurity::Starttls);
        assert_eq!(default.effective_port(), 587);
        let tls = parse("[smtp]\nhost = \"mail.lan\"\nsecurity = \"tls\"\n").smtp;
        assert_eq!(tls.effective_port(), 465);
        let none = parse("[smtp]\nsecurity = \"none\"\n").smtp;
        assert_eq!(none.effective_port(), 25);
        let own = parse("[smtp]\nsecurity = \"tls\"\nport = 2465\n").smtp;
        assert_eq!(own.effective_port(), 2465);
        // A bad value gives the default of its field and keeps the other fields.
        let bad = parse("[smtp]\nhost = \"mail.lan\"\nsecurity = \"ssl\"\nport = \"x\"\n").smtp;
        assert_eq!(bad.host, "mail.lan");
        assert_eq!(bad.security, SmtpSecurity::Starttls);
        assert_eq!(bad.port, None);
        let table_wrong = parse("language = \"it\"\nsmtp = 5\n");
        assert_eq!(table_wrong.language, "it");
        assert_eq!(table_wrong.smtp, SmtpSettings::default());
    }

    #[test]
    fn smtp_untouched_password_is_kept() {
        let dir = TempDir::new("untouched");
        let secrets_path = dir.0.join("secrets.toml");
        let settings_path = dir.0.join("settings.toml");
        let mut secrets = Secrets::open(&secrets_path);
        secrets.edit_smtp(PasswordEdit::Set("hunter2".into()));
        secrets.save().unwrap();

        // The user changes the host and leaves the password field untouched.
        let mut settings = AppSettings::default();
        settings.smtp.host = "mail.lan".to_owned();
        settings.save(&settings_path).unwrap();
        let mut secrets = Secrets::open(&secrets_path);
        secrets.edit_smtp(PasswordEdit::Untouched);
        secrets.save().unwrap();

        assert_eq!(Secrets::open(&secrets_path).smtp.expose(), "hunter2");
        let text = std::fs::read_to_string(&settings_path).unwrap();
        assert!(text.contains("host = \"mail.lan\""), "{text}");
        assert!(
            !text.contains("password") && !text.contains("hunter2"),
            "{text}"
        );
    }

    #[test]
    fn smtp_empty_password_clears() {
        let dir = TempDir::new("clear");
        let path = dir.0.join("secrets.toml");
        let mut secrets = Secrets::open(&path);
        secrets.edit_smtp(PasswordEdit::Set("hunter2".into()));
        secrets.edit_mqtt(PasswordEdit::Set("other".into()));
        secrets.save().unwrap();

        let mut secrets = Secrets::open(&path);
        secrets.edit_smtp(PasswordEdit::Set(String::new()));
        secrets.save().unwrap();
        let secrets = Secrets::open(&path);
        assert!(!secrets.smtp.is_set());
        assert_eq!(secrets.mqtt.expose(), "other");
    }

    #[test]
    fn smtp_warning_for_none_with_a_login() {
        let mut smtp = SmtpSettings {
            security: SmtpSecurity::None,
            user: "bob".to_owned(),
            ..SmtpSettings::default()
        };
        assert!(smtp.logs_in());
        assert!(smtp.warns_plain_login());
        smtp.user.clear();
        assert!(!smtp.logs_in());
        assert!(!smtp.warns_plain_login());
        for security in [SmtpSecurity::Starttls, SmtpSecurity::Tls] {
            let secure = SmtpSettings {
                security,
                user: "bob".to_owned(),
                ..SmtpSettings::default()
            };
            assert!(secure.logs_in());
            assert!(!secure.warns_plain_login());
        }
    }

    #[test]
    fn smtp_account_needs_a_host_and_a_sender() {
        let mut smtp = SmtpSettings::default();
        assert!(!smtp.is_configured());
        smtp.host = "mail.lan".to_owned();
        assert!(!smtp.is_configured());
        smtp.sender = "nicecooling@example.org".to_owned();
        assert!(smtp.is_configured());
        smtp.host = "  ".to_owned();
        assert!(!smtp.is_configured());
    }

    #[test]
    fn smtp_settings_round_trip_through_toml_without_a_password() {
        let mut settings = AppSettings::default();
        settings.smtp.host = "mail.lan".to_owned();
        settings.smtp.security = SmtpSecurity::Tls;
        settings.smtp.user = "bob".to_owned();
        let text = toml::to_string(&settings).expect("serialize");
        assert!(text.contains("security = \"tls\""), "{text}");
        assert!(!text.contains("password"));
        assert_eq!(parse(&text), settings);
    }
}
