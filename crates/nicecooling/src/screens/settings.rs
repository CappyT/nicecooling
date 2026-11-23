//! The Settings screen (`docs/ui/screens/settings.md`). The switches write `settings.toml` and
//! the autostart override at once (`docs/storage.md`).

use crate::app_settings::{HINT_APPINDICATOR, SettingsStore};
use crate::i18n::{Language, Locale};
use crate::model::Link;
use crate::screens::ScreenInput;

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
    pub daemon: DaemonCard,
    pub about: AboutCard,
    /// The last write of the settings file or the autostart override that failed.
    pub error: Option<String>,
}

impl SettingsModel {
    pub fn build(input: &ScreenInput<'_>, store: &SettingsStore) -> Self {
        let locale = input.locale;
        let values = &store.values;
        let language = values.language();
        let language_name = |language| match language {
            Language::System => locale.tr("settings.language.system"),
            Language::English => locale.tr("settings.language.en"),
            Language::Italian => locale.tr("settings.language.it"),
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
                options: [Language::System, Language::English, Language::Italian]
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
