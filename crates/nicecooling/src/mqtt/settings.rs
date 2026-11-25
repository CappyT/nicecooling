//! The `[mqtt]` table of `settings.toml`. The password is in `secrets.toml`, never here.
//! One bad field gives the default of that field and leaves the other settings as they are.

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use crate::graphs::secrets::Password;

pub const DEFAULT_PORT: u16 = 1883;
pub const DEFAULT_TLS_PORT: u16 = 8883;
pub const DEFAULT_BASE_TOPIC: &str = "nicecooling";
pub const DEFAULT_INTERVAL_S: u32 = 10;
pub const MIN_INTERVAL_S: u32 = 1;
pub const MAX_INTERVAL_S: u32 = 3600;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MqttSettingError {
    /// The interval is not from 1 s to 1 h.
    IntervalOutOfRange,
    /// The base topic is empty, or it holds a wildcard or a NUL character.
    BadBaseTopic,
}

/// The interval limits of the spec.
pub fn check_interval(seconds: u32) -> Result<u32, MqttSettingError> {
    if (MIN_INTERVAL_S..=MAX_INTERVAL_S).contains(&seconds) {
        Ok(seconds)
    } else {
        Err(MqttSettingError::IntervalOutOfRange)
    }
}

/// A base topic is a publish prefix, so it must not hold `#`, `+` or NUL.
pub fn check_base_topic(topic: &str) -> Result<&str, MqttSettingError> {
    if topic.is_empty() || topic.contains(['#', '+', '\0']) {
        Err(MqttSettingError::BadBaseTopic)
    } else {
        Ok(topic)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct MqttSettings {
    /// MQTT is off while the host is empty.
    #[serde(deserialize_with = "lenient")]
    pub host: String,
    /// `None`: 1883, or 8883 with TLS.
    #[serde(deserialize_with = "lenient", skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(deserialize_with = "lenient_tls")]
    pub tls: bool,
    /// Path of a CA file. Empty: the system certificates only.
    #[serde(deserialize_with = "lenient")]
    pub ca_file: String,
    #[serde(deserialize_with = "lenient")]
    pub user: String,
    /// Empty: `nicecooling-<hostname>`.
    #[serde(deserialize_with = "lenient")]
    pub client_id: String,
    #[serde(deserialize_with = "lenient_base_topic")]
    pub base_topic: String,
    #[serde(deserialize_with = "lenient_interval")]
    pub interval_s: u32,
}

impl Default for MqttSettings {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: None,
            tls: true,
            ca_file: String::new(),
            user: String::new(),
            client_id: String::new(),
            base_topic: DEFAULT_BASE_TOPIC.to_owned(),
            interval_s: DEFAULT_INTERVAL_S,
        }
    }
}

impl MqttSettings {
    /// MQTT is off while the host is empty: no connection and no queue.
    pub fn is_enabled(&self) -> bool {
        !self.host.trim().is_empty()
    }

    pub fn effective_port(&self) -> u16 {
        self.port.unwrap_or(if self.tls {
            DEFAULT_TLS_PORT
        } else {
            DEFAULT_PORT
        })
    }

    pub fn effective_client_id(&self, hostname: &str) -> String {
        if self.client_id.is_empty() {
            format!("nicecooling-{hostname}")
        } else {
            self.client_id.clone()
        }
    }

    pub fn interval(&self) -> Duration {
        Duration::from_secs(u64::from(self.interval_s))
    }

    /// The settings screen warns about this. The warning never blocks the settings.
    pub fn warns_plain_password(&self, password: &Password) -> bool {
        !self.tls && password.is_set()
    }
}

/// The host name for the default client id.
pub fn hostname() -> String {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .map(|name| name.trim().to_owned())
        .ok()
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "localhost".to_owned())
}

/// Read a value of any shape. A value of the wrong type gives the default of the field.
pub(crate) fn lenient<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned + Default,
{
    let value = toml::Value::deserialize(deserializer)?;
    Ok(T::deserialize(value).unwrap_or_default())
}

fn lenient_tls<'de, D: Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    let value = toml::Value::deserialize(deserializer)?;
    Ok(bool::deserialize(value).unwrap_or(MqttSettings::default().tls))
}

fn lenient_base_topic<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    let topic: String = lenient(deserializer)?;
    Ok(if check_base_topic(&topic).is_ok() {
        topic
    } else {
        DEFAULT_BASE_TOPIC.to_owned()
    })
}

fn lenient_interval<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let seconds: u32 = lenient(deserializer)?;
    Ok(check_interval(seconds).unwrap_or(DEFAULT_INTERVAL_S))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_settings::AppSettings;

    fn parse(text: &str) -> AppSettings {
        toml::from_str(text).expect("settings parse")
    }

    #[test]
    fn mqtt_defaults_with_tls_port_8883() {
        let settings = parse("[mqtt]\nhost = \"broker.lan\"\ntls = true\n").mqtt;
        assert_eq!(settings.effective_port(), 8883);
        assert_eq!(settings.effective_client_id("desk"), "nicecooling-desk");
        assert_eq!(settings.base_topic, "nicecooling");
        assert_eq!(settings.interval_s, 10);
        let plain = parse("[mqtt]\nhost = \"broker.lan\"\ntls = false\n").mqtt;
        assert_eq!(plain.effective_port(), 1883);
        let own = parse("[mqtt]\nport = 1884\ntls = true\nclient_id = \"mine\"\n").mqtt;
        assert_eq!(own.effective_port(), 1884);
        assert_eq!(own.effective_client_id("desk"), "mine");
    }

    #[test]
    fn mqtt_interval_range_1s_to_1h() {
        assert_eq!(check_interval(1), Ok(1));
        assert_eq!(check_interval(3600), Ok(3600));
        assert_eq!(check_interval(0), Err(MqttSettingError::IntervalOutOfRange));
        assert_eq!(
            check_interval(7200),
            Err(MqttSettingError::IntervalOutOfRange)
        );
        assert_eq!(parse("[mqtt]\ninterval_s = 7200\n").mqtt.interval_s, 10);
        assert_eq!(parse("[mqtt]\ninterval_s = 0\n").mqtt.interval_s, 10);
        assert_eq!(parse("[mqtt]\ninterval_s = -5\n").mqtt.interval_s, 10);
        assert_eq!(parse("[mqtt]\ninterval_s = 3600\n").mqtt.interval_s, 3600);
        assert_eq!(parse("[mqtt]\n").mqtt.interval(), Duration::from_secs(10));
    }

    #[test]
    fn mqtt_wildcard_base_topic_refused() {
        for topic in ["home/#", "home/+/x", "", "a\u{0}b"] {
            assert_eq!(
                check_base_topic(topic),
                Err(MqttSettingError::BadBaseTopic),
                "{topic:?}"
            );
        }
        assert_eq!(check_base_topic("home/pc"), Ok("home/pc"));
        let loaded = parse("[mqtt]\nbase_topic = \"home/#\"\n").mqtt;
        assert_eq!(loaded.base_topic, "nicecooling");
    }

    #[test]
    fn mqtt_off_while_host_is_empty() {
        assert!(!AppSettings::default().mqtt.is_enabled());
        assert!(!parse("[mqtt]\nhost = \"  \"\n").mqtt.is_enabled());
        assert!(parse("[mqtt]\nhost = \"broker.lan\"\n").mqtt.is_enabled());
    }

    #[test]
    fn mqtt_warning_for_tls_off_with_password() {
        let set = Password::new("secret");
        let unset = Password::default();
        let mut mqtt = MqttSettings {
            tls: false,
            ..MqttSettings::default()
        };
        assert!(mqtt.warns_plain_password(&set));
        assert!(!mqtt.warns_plain_password(&unset));
        mqtt.tls = true;
        assert!(!mqtt.warns_plain_password(&set));
    }

    #[test]
    fn mqtt_bad_field_gives_the_default_and_keeps_the_rest() {
        let settings = parse(
            "language = \"it\"\ntray = false\n[mqtt]\nhost = \"broker.lan\"\nport = \"x\"\n\
             tls = 3\nuser = \"bob\"\ninterval_s = 99999\n",
        );
        assert_eq!(settings.language, "it");
        assert!(!settings.tray);
        assert_eq!(settings.mqtt.host, "broker.lan");
        assert_eq!(settings.mqtt.user, "bob");
        assert_eq!(settings.mqtt.port, None);
        assert!(settings.mqtt.tls);
        assert_eq!(settings.mqtt.interval_s, 10);
        let table_wrong = parse("language = \"it\"\nmqtt = 5\n");
        assert_eq!(table_wrong.language, "it");
        assert_eq!(table_wrong.mqtt, MqttSettings::default());
    }

    #[test]
    fn mqtt_settings_round_trip_through_toml_without_a_password() {
        let mut settings = AppSettings::default();
        settings.mqtt.host = "broker.lan".to_owned();
        settings.mqtt.port = Some(8884);
        let text = toml::to_string(&settings).expect("serialize");
        assert!(!text.contains("password"));
        assert_eq!(parse(&text), settings);
    }
}
