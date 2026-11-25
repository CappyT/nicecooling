//! The Settings screen (`docs/ui/screens/settings.md`). The switches write `settings.toml` and
//! the autostart override at once (`docs/storage.md`).

use std::fmt;

use crate::app_settings::{HINT_APPINDICATOR, SettingsStore};
use crate::graphs::secrets::PasswordEdit;
use crate::i18n::{Language, Locale};
use crate::model::Link;
use crate::mqtt::client::{MqttConfig, MqttHandle, MqttStatus};
use crate::mqtt::settings::{
    DEFAULT_BASE_TOPIC, DEFAULT_INTERVAL_S, MqttSettings, check_base_topic, check_interval,
    hostname,
};
use crate::screens::ScreenInput;
use crate::smtp::settings::SmtpSecurity;

pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const PROJECT_URL: &str = "https://github.com/CappyT/nicecooling";
/// The directory of the daemon state. It has no setting: the unit file sets it.
pub const STATE_DIRECTORY: &str = "/var/lib/nicecooling/";

/// What the user can change on this screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    Language(Language),
    StartAtLogin(bool),
    Tray(bool),
    Warnings(bool),
    ProfileSwitchedByRule(bool),
    ExternalChange(bool),
    /// "Do not show this again" on the AppIndicator hint.
    DismissTrayHint,
}

/// Apply a change to the store. The store writes its file.
pub fn apply(store: &mut SettingsStore, change: Change) {
    match change {
        Change::Language(language) => store.update(|s| {
            s.language = match language {
                Language::System => "system",
                Language::English => "en",
                Language::Italian => "it",
                Language::German => "de",
                Language::French => "fr",
            }
            .to_owned();
        }),
        Change::StartAtLogin(on) => store.set_start_at_login(on),
        Change::Tray(on) => store.update(|s| s.tray = on),
        Change::Warnings(on) => store.update(|s| s.notifications.warnings = on),
        Change::ProfileSwitchedByRule(on) => {
            store.update(|s| s.notifications.profile_switched_by_rule = on);
        }
        Change::ExternalChange(on) => store.update(|s| s.notifications.external_change = on),
        Change::DismissTrayHint => store.update(|s| s.dismiss_hint(HINT_APPINDICATOR)),
    }
}

/// An input of the group "Network".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NetworkField {
    MqttHost,
    MqttPort,
    MqttUser,
    MqttPassword,
    MqttClientId,
    MqttBaseTopic,
    MqttInterval,
    SmtpHost,
    SmtpPort,
    SmtpUser,
    SmtpPassword,
    SmtpSender,
}

impl NetworkField {
    pub fn is_password(self) -> bool {
        matches!(self, Self::MqttPassword | Self::SmtpPassword)
    }

    /// The field is a setting of the MQTT broker: a change opens a new connection.
    pub fn is_broker(self) -> bool {
        matches!(
            self,
            Self::MqttHost
                | Self::MqttPort
                | Self::MqttUser
                | Self::MqttPassword
                | Self::MqttClientId
                | Self::MqttBaseTopic
                | Self::MqttInterval
        )
    }
}

/// What the user can change in the group "Network". The app holds these settings: they work
/// while the daemon is offline.
#[derive(Clone, PartialEq, Eq)]
pub enum NetworkChange {
    /// The text of an input, as the user confirmed it.
    Field(NetworkField, String),
    MqttTls(bool),
    /// The path of the CA file. Empty: the system certificates only.
    MqttCaFile(String),
    Security(SmtpSecurity),
}

impl fmt::Debug for NetworkChange {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // The text of a password input never reaches a log.
            Self::Field(field, _) if field.is_password() => write!(f, "Field({field:?}, ..)"),
            Self::Field(field, text) => write!(f, "Field({field:?}, {text:?})"),
            Self::MqttTls(on) => write!(f, "MqttTls({on})"),
            Self::MqttCaFile(path) => write!(f, "MqttCaFile({path:?})"),
            Self::Security(security) => write!(f, "Security({security:?})"),
        }
    }
}

/// Why the app refuses the text of an input. The setting stays as it was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetworkError {
    Port,
    Interval,
    BaseTopic,
}

impl NetworkError {
    pub fn text(self, locale: Locale) -> String {
        locale.tr(match self {
            Self::Port => "settings.network.error.port",
            Self::Interval => "settings.network.error.interval",
            Self::BaseTopic => "settings.network.error.base_topic",
        })
    }
}

/// The state of the group "Network" that the user changes.
#[derive(Clone, Debug, Default)]
pub struct NetworkUi {
    /// The input whose last text the app refused, with the reason.
    pub refused: Option<(NetworkField, NetworkError)>,
}

/// A port as the user types it. An empty text gives the default port.
fn parse_port(text: &str) -> Result<Option<u16>, NetworkError> {
    if text.is_empty() {
        return Ok(None);
    }
    match text.parse::<u16>() {
        Ok(port) if port > 0 => Ok(Some(port)),
        _ => Err(NetworkError::Port),
    }
}

/// The config that the settings give to the MQTT client. `None` while the host is empty.
pub fn broker_config(store: &SettingsStore) -> Option<MqttConfig> {
    MqttConfig::new(&store.values.mqtt, &store.secrets.mqtt, &hostname())
}

/// Apply a change of the group "Network": the store writes `settings.toml`, and `secrets.toml`
/// for a password. After a change of a broker setting the MQTT client gets the new config: it
/// closes its connection and opens a new one. A text that the app refuses changes nothing.
pub fn change_network(
    store: &mut SettingsStore,
    mqtt: Option<&MqttHandle>,
    change: NetworkChange,
) -> Result<(), NetworkError> {
    let broker = match &change {
        NetworkChange::Field(field, _) => field.is_broker(),
        NetworkChange::MqttTls(_) | NetworkChange::MqttCaFile(_) => true,
        NetworkChange::Security(_) => false,
    };
    match change {
        NetworkChange::Field(field, text) => {
            let trimmed = text.trim().to_owned();
            match field {
                NetworkField::MqttHost => store.update(|s| s.mqtt.host = trimmed),
                NetworkField::MqttPort => {
                    let port = parse_port(&trimmed)?;
                    store.update(|s| s.mqtt.port = port);
                }
                NetworkField::MqttUser => store.update(|s| s.mqtt.user = trimmed),
                // A password is taken as typed. An empty text clears it.
                NetworkField::MqttPassword => {
                    store.edit_secrets(|secrets| secrets.edit_mqtt(PasswordEdit::Set(text)));
                }
                NetworkField::MqttClientId => store.update(|s| s.mqtt.client_id = trimmed),
                NetworkField::MqttBaseTopic => {
                    let topic = if trimmed.is_empty() {
                        DEFAULT_BASE_TOPIC.to_owned()
                    } else {
                        trimmed
                    };
                    if check_base_topic(&topic).is_err() || topic.split('/').any(str::is_empty) {
                        return Err(NetworkError::BaseTopic);
                    }
                    store.update(|s| s.mqtt.base_topic = topic);
                }
                NetworkField::MqttInterval => {
                    let seconds = if trimmed.is_empty() {
                        DEFAULT_INTERVAL_S
                    } else {
                        trimmed
                            .parse::<u32>()
                            .ok()
                            .and_then(|seconds| check_interval(seconds).ok())
                            .ok_or(NetworkError::Interval)?
                    };
                    store.update(|s| s.mqtt.interval_s = seconds);
                }
                NetworkField::SmtpHost => store.update(|s| s.smtp.host = trimmed),
                NetworkField::SmtpPort => {
                    let port = parse_port(&trimmed)?;
                    store.update(|s| s.smtp.port = port);
                }
                NetworkField::SmtpUser => store.update(|s| s.smtp.user = trimmed),
                NetworkField::SmtpPassword => {
                    store.edit_secrets(|secrets| secrets.edit_smtp(PasswordEdit::Set(text)));
                }
                NetworkField::SmtpSender => store.update(|s| s.smtp.sender = trimmed),
            }
        }
        NetworkChange::MqttTls(on) => store.update(|s| s.mqtt.tls = on),
        NetworkChange::MqttCaFile(path) => store.update(|s| s.mqtt.ca_file = path),
        NetworkChange::Security(security) => store.update(|s| s.smtp.security = security),
    }
    if broker && let Some(mqtt) = mqtt {
        mqtt.configure(broker_config(store));
    }
    Ok(())
}

/// The text that the input of `field` shows for the stored setting. A password input shows none.
pub fn stored_text(store: &SettingsStore, field: NetworkField) -> String {
    let (mqtt, smtp) = (&store.values.mqtt, &store.values.smtp);
    let port = |port: Option<u16>| port.map(|port| port.to_string()).unwrap_or_default();
    match field {
        NetworkField::MqttHost => mqtt.host.clone(),
        NetworkField::MqttPort => port(mqtt.port),
        NetworkField::MqttUser => mqtt.user.clone(),
        NetworkField::MqttPassword | NetworkField::SmtpPassword => String::new(),
        NetworkField::MqttClientId => mqtt.client_id.clone(),
        // The default shows as the dim text of the empty input.
        NetworkField::MqttBaseTopic if mqtt.base_topic == DEFAULT_BASE_TOPIC => String::new(),
        NetworkField::MqttBaseTopic => mqtt.base_topic.clone(),
        NetworkField::MqttInterval => mqtt.interval_s.to_string(),
        NetworkField::SmtpHost => smtp.host.clone(),
        NetworkField::SmtpPort => port(smtp.port),
        NetworkField::SmtpUser => smtp.user.clone(),
        NetworkField::SmtpSender => smtp.sender.clone(),
    }
}

/// The placeholder of a password input while a password is set: eight dots. The input never
/// shows the stored password.
pub const PASSWORD_DOTS: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";

/// A text input of the group "Network".
#[derive(Clone, Debug, PartialEq)]
pub struct NetworkInput {
    pub field: NetworkField,
    pub label: String,
    /// The stored setting. Always empty for a password.
    pub value: String,
    /// The default that shows as dim text in the empty input, or the dots of a password.
    pub placeholder: String,
    pub unit: Option<String>,
    /// A number input: it is narrower.
    pub short: bool,
    pub password: bool,
    /// Why the app refused the last text of this input.
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum NetworkRow {
    Input(NetworkInput),
    Tls {
        label: String,
        on: bool,
    },
    CaFile {
        label: String,
        /// `None` without a file: the row shows `none`.
        path: Option<String>,
        none: String,
        choose: String,
    },
    Security {
        label: String,
        value: SmtpSecurity,
        shown: String,
        options: Vec<(SmtpSecurity, String)>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrokerState {
    Off,
    Connecting,
    Connected,
    Failed,
}

/// The row "Connection" of the MQTT broker.
#[derive(Clone, Debug, PartialEq)]
pub struct BrokerRow {
    pub label: String,
    pub state: BrokerState,
    pub value: String,
    /// The host and the port of a connection, or the last error text.
    pub caption: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NetworkCard {
    pub title: String,
    pub mqtt_title: String,
    pub connection: BrokerRow,
    pub mqtt: Vec<NetworkRow>,
    /// "The password goes over the network in clear text". It blocks nothing.
    pub mqtt_warning: Option<String>,
    pub smtp_title: String,
    pub smtp: Vec<NetworkRow>,
    /// The login without encryption. It blocks nothing.
    pub smtp_warning: Option<String>,
    pub note: String,
}

impl NetworkCard {
    /// The inputs of both halves, in the order of the rows.
    pub fn inputs(&self) -> impl Iterator<Item = &NetworkInput> {
        self.mqtt
            .iter()
            .chain(&self.smtp)
            .filter_map(|row| match row {
                NetworkRow::Input(input) => Some(input),
                _ => None,
            })
    }

    pub fn input(&self, field: NetworkField) -> Option<&NetworkInput> {
        self.inputs().find(|input| input.field == field)
    }
}

fn security_name(locale: Locale, security: SmtpSecurity) -> String {
    match security {
        SmtpSecurity::Starttls => locale.tr("settings.network.security_starttls"),
        SmtpSecurity::Tls => locale.tr("settings.network.security_tls"),
        SmtpSecurity::None => locale.tr("settings.network.security_none"),
    }
}

fn network_card(
    locale: Locale,
    store: &SettingsStore,
    status: &MqttStatus,
    ui: &NetworkUi,
) -> NetworkCard {
    let (mqtt, smtp) = (&store.values.mqtt, &store.values.smtp);
    let secrets = &store.secrets;
    let input = |field: NetworkField, key: &str, placeholder: String| NetworkInput {
        field,
        label: locale.tr(key),
        value: stored_text(store, field),
        placeholder,
        unit: None,
        short: matches!(
            field,
            NetworkField::MqttPort | NetworkField::MqttInterval | NetworkField::SmtpPort
        ),
        password: field.is_password(),
        error: ui
            .refused
            .filter(|(refused, _)| *refused == field)
            .map(|(_, error)| error.text(locale)),
    };
    let dots = |set: bool| {
        if set {
            PASSWORD_DOTS.to_owned()
        } else {
            String::new()
        }
    };
    // The default of the port follows the TLS switch.
    let default_port = MqttSettings {
        port: None,
        ..mqtt.clone()
    }
    .effective_port();
    let label = locale.tr("settings.network.connection");
    let connection = match status {
        MqttStatus::Off => BrokerRow {
            label,
            state: BrokerState::Off,
            value: locale.tr("settings.network.state_off"),
            caption: None,
        },
        MqttStatus::Connecting => BrokerRow {
            label,
            state: BrokerState::Connecting,
            value: locale.tr("settings.network.state_connecting"),
            caption: None,
        },
        MqttStatus::Connected => BrokerRow {
            label,
            state: BrokerState::Connected,
            value: locale.tr("settings.network.state_connected"),
            caption: Some(format!("{}:{}", mqtt.host.trim(), mqtt.effective_port())),
        },
        MqttStatus::Failed(error) => BrokerRow {
            label,
            state: BrokerState::Failed,
            value: locale.tr("settings.network.state_failed"),
            caption: Some(error.clone()),
        },
    };
    NetworkCard {
        title: locale.tr("settings.network.title"),
        mqtt_title: locale.tr("settings.network.mqtt"),
        connection,
        mqtt: vec![
            NetworkRow::Input(input(
                NetworkField::MqttHost,
                "settings.network.host",
                String::new(),
            )),
            NetworkRow::Input(input(
                NetworkField::MqttPort,
                "settings.network.port",
                default_port.to_string(),
            )),
            NetworkRow::Tls {
                label: locale.tr("settings.network.tls"),
                on: mqtt.tls,
            },
            NetworkRow::CaFile {
                label: locale.tr("settings.network.ca_file"),
                path: (!mqtt.ca_file.trim().is_empty()).then(|| mqtt.ca_file.clone()),
                none: locale.tr("settings.network.no_file"),
                choose: locale.tr("settings.network.choose"),
            },
            NetworkRow::Input(input(
                NetworkField::MqttUser,
                "settings.network.user",
                String::new(),
            )),
            NetworkRow::Input(input(
                NetworkField::MqttPassword,
                "settings.network.password",
                dots(secrets.mqtt.is_set()),
            )),
            NetworkRow::Input(input(
                NetworkField::MqttClientId,
                "settings.network.client_id",
                format!("nicecooling-{}", hostname()),
            )),
            NetworkRow::Input(input(
                NetworkField::MqttBaseTopic,
                "settings.network.base_topic",
                DEFAULT_BASE_TOPIC.to_owned(),
            )),
            NetworkRow::Input(NetworkInput {
                unit: Some(locale.tr("settings.network.seconds")),
                ..input(
                    NetworkField::MqttInterval,
                    "settings.network.interval",
                    DEFAULT_INTERVAL_S.to_string(),
                )
            }),
        ],
        mqtt_warning: mqtt
            .warns_plain_password(&secrets.mqtt)
            .then(|| locale.tr("settings.network.warning_mqtt")),
        smtp_title: locale.tr("settings.network.smtp"),
        smtp: vec![
            NetworkRow::Input(input(
                NetworkField::SmtpHost,
                "settings.network.host",
                String::new(),
            )),
            NetworkRow::Security {
                label: locale.tr("settings.network.security"),
                value: smtp.security,
                shown: security_name(locale, smtp.security),
                options: [
                    SmtpSecurity::Starttls,
                    SmtpSecurity::Tls,
                    SmtpSecurity::None,
                ]
                .into_iter()
                .map(|security| (security, security_name(locale, security)))
                .collect(),
            },
            NetworkRow::Input(input(
                NetworkField::SmtpPort,
                "settings.network.port",
                smtp.security.default_port().to_string(),
            )),
            NetworkRow::Input(input(
                NetworkField::SmtpUser,
                "settings.network.user",
                String::new(),
            )),
            NetworkRow::Input(input(
                NetworkField::SmtpPassword,
                "settings.network.password",
                dots(secrets.smtp.is_set()),
            )),
            NetworkRow::Input(input(
                NetworkField::SmtpSender,
                "settings.network.sender",
                String::new(),
            )),
        ],
        smtp_warning: smtp
            .warns_plain_login()
            .then(|| locale.tr("settings.network.warning_smtp")),
        note: locale.tr("settings.network.note"),
    }
}

/// A switch of the screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwitchKey {
    StartAtLogin,
    Tray,
    Warnings,
    ProfileSwitchedByRule,
    ExternalChange,
}

impl SwitchKey {
    pub fn change(self, on: bool) -> Change {
        match self {
            Self::StartAtLogin => Change::StartAtLogin(on),
            Self::Tray => Change::Tray(on),
            Self::Warnings => Change::Warnings(on),
            Self::ProfileSwitchedByRule => Change::ProfileSwitchedByRule(on),
            Self::ExternalChange => Change::ExternalChange(on),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SwitchRow {
    pub key: SwitchKey,
    pub label: String,
    pub on: bool,
    pub text: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LanguageRow {
    pub label: String,
    pub value: Language,
    pub shown: String,
    pub options: Vec<(Language, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Hint {
    pub text: String,
    pub link: String,
    pub dismiss: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionKind {
    Connecting,
    Connected,
    Offline,
    Down,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConnectionRow {
    pub label: String,
    pub kind: ConnectionKind,
    pub value: String,
    pub text: Option<String>,
    /// The commands of the "Daemon down" state.
    pub commands: Vec<String>,
    pub try_again: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct InfoRow {
    pub label: String,
    pub value: String,
    /// The row has no control: every row of Daemon and About.
    pub read_only: bool,
    /// The daemon gives the value: it shows grey bars while the app waits for it.
    pub from_daemon: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DaemonCard {
    pub title: String,
    pub connection: ConnectionRow,
    /// Grey bars in place of the values of the daemon rows.
    pub loading: bool,
    pub rows: Vec<InfoRow>,
    pub mismatch: Option<String>,
    pub board_note: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AboutCard {
    pub title: String,
    pub rows: Vec<InfoRow>,
    pub project_label: String,
    pub project_url: &'static str,
    pub note: String,
    pub firmware: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingsModel {
    pub language_title: String,
    pub language: LanguageRow,
    pub start_title: String,
    pub start: SwitchRow,
    pub tray: SwitchRow,
    pub tray_hint: Option<Hint>,
    pub notifications_title: String,
    pub notifications: Vec<SwitchRow>,
    pub notifications_note: String,
    pub network: NetworkCard,
    pub daemon: DaemonCard,
    pub about: AboutCard,
    /// The last write of the settings file or the autostart override that failed.
    pub error: Option<String>,
}

impl SettingsModel {
    pub fn build(input: &ScreenInput<'_>, store: &SettingsStore) -> Self {
        Self::build_with(input, store, &MqttStatus::Off, &NetworkUi::default())
    }

    /// As `build`, with the state of the MQTT connection and the state of the group "Network".
    pub fn build_with(
        input: &ScreenInput<'_>,
        store: &SettingsStore,
        status: &MqttStatus,
        network: &NetworkUi,
    ) -> Self {
        let locale = input.locale;
        let values = &store.values;
        let language = values.language();
        let language_name = |language| match language {
            Language::System => locale.tr("settings.language.system"),
            Language::English => locale.tr("settings.language.en"),
            Language::Italian => locale.tr("settings.language.it"),
            Language::German => locale.tr("settings.language.de"),
            Language::French => locale.tr("settings.language.fr"),
        };
        let switch = |switch: SwitchKey, key: &str, on: bool, text: Option<&str>| SwitchRow {
            key: switch,
            label: locale.tr(key),
            on,
            text: text.map(|key| locale.tr(key)),
        };
        Self {
            language_title: locale.tr("settings.language.title"),
            language: LanguageRow {
                label: locale.tr("settings.language.label"),
                value: language,
                shown: language_name(language),
                options: [
                    Language::System,
                    Language::English,
                    Language::Italian,
                    Language::German,
                    Language::French,
                ]
                .into_iter()
                .map(|l| (l, language_name(l)))
                .collect(),
            },
            start_title: locale.tr("settings.start.title"),
            start: switch(
                SwitchKey::StartAtLogin,
                "settings.start.login",
                store.start_at_login(),
                Some("settings.start.login_text"),
            ),
            tray: switch(
                SwitchKey::Tray,
                "settings.start.tray",
                values.tray,
                (!values.tray).then_some("settings.start.tray_off"),
            ),
            tray_hint: store.tray_hint_needed().then(|| Hint {
                text: locale.tr("settings.hint.tray"),
                link: locale.tr("settings.hint.tray_link"),
                dismiss: locale.tr("settings.hint.dismiss"),
            }),
            notifications_title: locale.tr("settings.notifications.title"),
            notifications: vec![
                switch(
                    SwitchKey::Warnings,
                    "settings.notifications.warnings",
                    values.notifications.warnings,
                    None,
                ),
                switch(
                    SwitchKey::ProfileSwitchedByRule,
                    "settings.notifications.profile",
                    values.notifications.profile_switched_by_rule,
                    None,
                ),
                switch(
                    SwitchKey::ExternalChange,
                    "settings.notifications.external",
                    values.notifications.external_change,
                    None,
                ),
            ],
            notifications_note: locale.tr("settings.notifications.note"),
            network: network_card(locale, store, status, network),
            daemon: daemon_card(input),
            about: AboutCard {
                title: locale.tr("settings.about.title"),
                rows: vec![
                    InfoRow {
                        label: locale.tr("settings.about.name"),
                        value: "NiceCooling".to_owned(),
                        read_only: true,
                        from_daemon: false,
                    },
                    InfoRow {
                        label: locale.tr("settings.about.version"),
                        value: APP_VERSION.to_owned(),
                        read_only: true,
                        from_daemon: false,
                    },
                    InfoRow {
                        label: locale.tr("settings.about.license"),
                        value: locale.tr("settings.about.license_value"),
                        read_only: true,
                        from_daemon: false,
                    },
                ],
                project_label: locale.tr("settings.about.project"),
                project_url: PROJECT_URL,
                note: locale.tr("settings.about.note"),
                firmware: locale.tr("settings.about.firmware"),
            },
            error: store.error.clone(),
        }
    }
}

fn daemon_card(input: &ScreenInput<'_>) -> DaemonCard {
    let locale: Locale = input.locale;
    let state = input.state;
    let snapshot = &state.snapshot;
    let connecting = state.link == Link::Connecting;
    let connection = match state.link {
        Link::Connecting => ConnectionRow {
            label: locale.tr("settings.daemon.connection"),
            kind: ConnectionKind::Connecting,
            value: locale.tr("status.connecting"),
            text: None,
            commands: Vec::new(),
            try_again: None,
        },
        Link::Online => ConnectionRow {
            label: locale.tr("settings.daemon.connection"),
            kind: ConnectionKind::Connected,
            value: locale.tr("settings.daemon.connected"),
            text: None,
            commands: Vec::new(),
            try_again: None,
        },
        Link::Offline => ConnectionRow {
            label: locale.tr("settings.daemon.connection"),
            kind: ConnectionKind::Offline,
            value: locale.tr("settings.daemon.offline"),
            text: Some(locale.tr("settings.daemon.offline_text")),
            commands: Vec::new(),
            try_again: Some(locale.tr("settings.daemon.try_again")),
        },
        Link::Down => ConnectionRow {
            label: locale.tr("settings.daemon.connection"),
            kind: ConnectionKind::Down,
            value: locale.tr("settings.daemon.down"),
            text: Some(locale.tr("settings.daemon.down_text")),
            commands: vec![
                "systemctl status nicecoolingd".to_owned(),
                "systemctl restart nicecoolingd".to_owned(),
            ],
            try_again: Some(locale.tr("settings.daemon.try_again")),
        },
    };
    let board = snapshot.board.as_ref().map(|board| {
        if board.model.is_empty() {
            board.vendor.clone()
        } else {
            board.model.clone()
        }
    });
    let board_file = snapshot
        .board
        .as_ref()
        .map(|b| b.board_file.clone())
        .filter(|file| !file.is_empty());
    let dash = || "--".to_owned();
    let rows = vec![
        InfoRow {
            label: locale.tr("settings.daemon.version"),
            value: if snapshot.daemon_version.is_empty() {
                dash()
            } else {
                snapshot.daemon_version.clone()
            },
            read_only: true,
            from_daemon: true,
        },
        InfoRow {
            label: locale.tr("settings.daemon.app_version"),
            value: APP_VERSION.to_owned(),
            read_only: true,
            from_daemon: false,
        },
        InfoRow {
            label: locale.tr("settings.daemon.board"),
            value: match (&board, &board_file) {
                (Some(name), Some(file)) => format!("{name}, {file}"),
                (Some(name), None) => name.clone(),
                _ => dash(),
            },
            read_only: true,
            from_daemon: true,
        },
        InfoRow {
            label: locale.tr("settings.daemon.state_directory"),
            value: STATE_DIRECTORY.to_owned(),
            read_only: true,
            from_daemon: false,
        },
    ];
    let mismatch = (!snapshot.daemon_version.is_empty() && snapshot.daemon_version != APP_VERSION)
        .then(|| {
            locale.fmt(
                "settings.daemon.mismatch",
                &[("daemon", &snapshot.daemon_version), ("app", APP_VERSION)],
            )
        });
    DaemonCard {
        title: locale.tr("settings.daemon.title"),
        connection,
        loading: connecting,
        rows,
        mismatch,
        board_note: (board.is_some() && board_file.is_none()).then(|| {
            locale.fmt(
                "settings.daemon.no_board_file",
                &[("board", board.as_deref().unwrap_or_default())],
            )
        }),
    }
}
