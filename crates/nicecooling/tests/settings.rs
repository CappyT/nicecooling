mod support;

use std::path::PathBuf;
use std::sync::atomic::Ordering;

use nicecooling::app_settings::{
    AppSettings, HINT_APPINDICATOR, HINT_IMMEDIATE_SWITCHING, SettingsStore,
};
use nicecooling::autostart;
use nicecooling::i18n::{EN, IT, Language};
use nicecooling::model::{AppState, BoardInfo, Link};
use nicecooling::screens::settings::{
    APP_VERSION, Change, ConnectionKind, STATE_DIRECTORY, SettingsModel, SwitchKey, apply,
};
use nicecooling::screens::{Feedback, ScreenInput};
use support::fixtures::{NOW_US, full};
use support::labels;

/// A new empty directory for one test.
struct Dir(PathBuf);

impl Dir {
    fn new(name: &str) -> Self {
        static COUNTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let id = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "nicecooling-settings-{name}-{}-{id}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("test directory");
        Self(path)
    }
}

impl Drop for Dir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn model_of(state: &AppState, store: &SettingsStore) -> SettingsModel {
    let feedback = Feedback::default();
    SettingsModel::build(
        &ScreenInput {
            state,
            now_us: NOW_US,
            locale: EN,
            down_since_us: None,
            feedback: &feedback,
        },
        store,
    )
}

fn with_board() -> AppState {
    let mut state = full();
    state.snapshot.board = Some(BoardInfo {
        vendor: "ASUSTeK COMPUTER INC.".into(),
        model: "ROG CROSSHAIR VIII DARK HERO".into(),
        board_file: "asus_rog_crosshair_viii_dark_hero".into(),
    });
    state.snapshot.daemon_version = APP_VERSION.into();
    state
}

#[test]
fn settings_states() {
    let store = SettingsStore::default();

    // Connected: the rows of the Daemon card, the About card and the switches.
    let state = with_board();
    let model = model_of(&state, &store);
    assert_eq!(model.language.shown, "System");
    assert_eq!(
        model
            .language
            .options
            .iter()
            .map(|(_, text)| text.as_str())
            .collect::<Vec<_>>(),
        ["System", "English", "Italiano"]
    );
    assert_eq!(model.daemon.connection.kind, ConnectionKind::Connected);
    assert_eq!(model.daemon.connection.value, "Connected");
    assert!(model.daemon.connection.try_again.is_none());
    assert!(model.daemon.mismatch.is_none() && model.daemon.board_note.is_none());
    let rows: Vec<(&str, &str)> = model
        .daemon
        .rows
        .iter()
        .map(|r| (r.label.as_str(), r.value.as_str()))
        .collect();
    assert_eq!(
        rows[2],
        (
            "Board",
            "ROG CROSSHAIR VIII DARK HERO, asus_rog_crosshair_viii_dark_hero"
        )
    );
    assert_eq!(model.about.rows[1].value, APP_VERSION);
    assert_eq!(
        model.about.note,
        "nicecooling is not a product of Aqua Computer. LEAKSHIELD and aquasuite are names of Aqua Computer GmbH & Co. KG."
    );
    assert_eq!(
        model.about.firmware,
        "nicecooling does not update the firmware of any device."
    );
    assert_eq!(model.start.label, "Start at login");
    assert_eq!(
        model.start.text.as_deref(),
        Some("The app starts without a window and shows its tray icon.")
    );
    assert!(model.tray.on && model.tray.text.is_none());
    assert_eq!(
        model.notifications.iter().map(|r| r.on).collect::<Vec<_>>(),
        [true, false, true],
        "Warnings on, Profile switched by a rule off, External change on"
    );
    assert_eq!(
        model.notifications_note,
        "Alarm notifications are always on."
    );

    // Loading: grey bars for the daemon rows.
    let mut loading = with_board();
    loading.link = Link::Connecting;
    assert!(model_of(&loading, &store).daemon.loading);

    // Offline: the text with the command. The app settings still work.
    let mut offline = with_board();
    offline.link = Link::Offline;
    let model = model_of(&offline, &store);
    assert_eq!(model.daemon.connection.kind, ConnectionKind::Offline);
    assert_eq!(model.daemon.connection.value, "Offline");
    assert_eq!(
        model.daemon.connection.text.as_deref(),
        Some("Start the daemon: systemctl start nicecoolingd")
    );
    assert_eq!(
        model.daemon.connection.try_again.as_deref(),
        Some("Try again")
    );
    assert!(model.language.options.len() == 3 && model.notifications.len() == 3);

    // Down: the text and the two commands.
    let mut down = with_board();
    down.link = Link::Down;
    let model = model_of(&down, &store);
    assert_eq!(model.daemon.connection.kind, ConnectionKind::Down);
    assert_eq!(
        model.daemon.connection.text.as_deref(),
        Some(
            "systemd stopped the restarts of the daemon. nicecooling protection is off. The Board runs its Hardware curves from the BIOS snapshot."
        )
    );
    assert_eq!(
        model.daemon.connection.commands,
        [
            "systemctl status nicecoolingd",
            "systemctl restart nicecoolingd"
        ]
    );

    // A daemon of another version.
    let mut mismatch = with_board();
    mismatch.snapshot.daemon_version = "1.1.0".into();
    assert_eq!(
        model_of(&mismatch, &store).daemon.mismatch,
        Some(format!(
            "The daemon is version 1.1.0 and the app is version {APP_VERSION}. Update both to the same version."
        ))
    );

    // No Board file.
    let mut bare = with_board();
    bare.snapshot.board.as_mut().unwrap().board_file.clear();
    assert_eq!(
        model_of(&bare, &store).daemon.board_note.as_deref(),
        Some(
            "Board: ROG CROSSHAIR VIII DARK HERO. No Board file: Channels and sensors have the names of the kernel."
        )
    );

    // The tray hint shows when no host takes the icon, and "Do not show this again" hides it.
    let mut hidden_tray = SettingsStore::default();
    hidden_tray.tray_visible.store(false, Ordering::Relaxed);
    let model = model_of(&state, &hidden_tray);
    assert_eq!(
        model.tray_hint.as_ref().map(|h| h.text.as_str()),
        Some("The tray icon is not visible. On GNOME, turn on the AppIndicator extension.")
    );
    assert_eq!(model.tray_hint.unwrap().link, "How to turn it on");
    apply(&mut hidden_tray, Change::DismissTrayHint);
    assert!(model_of(&state, &hidden_tray).tray_hint.is_none());
    assert!(hidden_tray.values.hint_dismissed(HINT_APPINDICATOR));

    // The tray off: the text under the switch, and no hint.
    let mut no_tray = SettingsStore::default();
    apply(&mut no_tray, Change::Tray(false));
    let model = model_of(&state, &no_tray);
    assert_eq!(
        model.tray.text.as_deref(),
        Some("Without the tray icon, open the window from the app list.")
    );
    assert!(!model.tray.on);
}

#[test]
fn settings_file_roundtrip() {
    let dir = Dir::new("roundtrip");
    let path = dir.0.join("nicecooling/settings.toml");
    let mut settings = AppSettings {
        language: "it".into(),
        tray: false,
        ..AppSettings::default()
    };
    settings.notifications.warnings = false;
    settings.notifications.profile_switched_by_rule = true;
    settings.notifications.external_change = false;
    settings.dismiss_hint(HINT_IMMEDIATE_SWITCHING);
    settings.sensors.hidden = vec!["asusec_fan1".into(), "nct6798_temp1".into()];
    settings.save(&path).expect("the file is written");

    // The format of docs/storage.md.
    let text = std::fs::read_to_string(&path).unwrap();
    let parsed: toml::Table = text.parse().unwrap();
    assert_eq!(parsed["schema_version"].as_integer(), Some(1));
    assert_eq!(parsed["language"].as_str(), Some("it"));
    assert_eq!(parsed["tray"].as_bool(), Some(false));
    assert_eq!(parsed["notifications"]["warnings"].as_bool(), Some(false));
    assert_eq!(
        parsed["hints"]["dismissed"][0].as_str(),
        Some("immediate_switching_warning")
    );
    assert_eq!(parsed["sensors"]["hidden"].as_array().unwrap().len(), 2);
    assert!(
        !path.with_extension("toml.tmp").exists(),
        "the rename leaves no temporary file"
    );

    assert_eq!(AppSettings::load(&path), settings);
    assert_eq!(AppSettings::load(&path).language(), Language::Italian);
    assert!(!AppSettings::load(&path).notify_prefs().warnings);

    // A file that an older app wrote has no `[sensors]`: the other keys still load.
    std::fs::write(
        &path,
        "schema_version = 1\nlanguage = \"en\"\n[notifications]\nwarnings = false\n",
    )
    .unwrap();
    let older = AppSettings::load(&path);
    assert_eq!(older.language(), Language::English);
    assert!(!older.notifications.warnings);
    assert!(
        older.notifications.external_change,
        "a missing key has its default"
    );
    assert!(older.sensors.hidden.is_empty());
}

#[test]
fn hidden_sensors_persist_in_settings_file() {
    let dir = Dir::new("hidden");
    let path = dir.0.join("settings.toml");
    let mut store = SettingsStore::new(AppSettings::default(), Some(path.clone()), None);
    let hidden = std::collections::BTreeSet::from(["asusec_fan1".to_owned()]);
    store.update(|values| values.set_hidden_sensors(&hidden));
    assert!(store.error.is_none());
    assert_eq!(AppSettings::load(&path).hidden_sensors(), hidden);
    store.update(|values| values.set_hidden_sensors(&Default::default()));
    assert!(AppSettings::load(&path).hidden_sensors().is_empty());
}

#[test]
fn missing_settings_file_uses_defaults() {
    let dir = Dir::new("missing");
    let missing = dir.0.join("nicecooling/settings.toml");
    let defaults = AppSettings::load(&missing);
    assert_eq!(defaults, AppSettings::default());
    // The defaults of docs/storage.md.
    assert_eq!(defaults.schema_version, 1);
    assert_eq!(defaults.language, "system");
    assert!(defaults.tray);
    assert!(defaults.notifications.warnings);
    assert!(!defaults.notifications.profile_switched_by_rule);
    assert!(defaults.notifications.external_change);
    assert!(defaults.hints.dismissed.is_empty() && defaults.sensors.hidden.is_empty());
    assert!(!missing.exists(), "reading does not create the file");

    // A file that is not TOML gives the defaults too.
    std::fs::create_dir_all(missing.parent().unwrap()).unwrap();
    std::fs::write(&missing, "this is [not toml").unwrap();
    assert_eq!(AppSettings::load(&missing), AppSettings::default());

    // A store without a path keeps the values in memory and writes no file.
    let mut store = SettingsStore::default();
    apply(&mut store, Change::Language(Language::Italian));
    assert_eq!(store.values.language(), Language::Italian);
    assert!(store.error.is_none());
}

#[test]
fn start_at_login_off_writes_hidden_override() {
    let dir = Dir::new("autostart-off");
    let autostart_dir = dir.0.join("autostart");
    // No override file: the switch is on.
    assert!(autostart::is_enabled(&autostart_dir));
    autostart::set_enabled(&autostart_dir, false).expect("the override is written");
    let file = autostart_dir.join("io.github.cappyt.NiceCooling.desktop");
    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(
        text,
        "[Desktop Entry]\nType=Application\nName=NiceCooling\nHidden=true\n"
    );
    assert!(!autostart::is_enabled(&autostart_dir));

    // The same through the store of the Settings screen.
    let dir = Dir::new("autostart-store");
    let mut store = SettingsStore::new(AppSettings::default(), None, Some(dir.0.clone()));
    assert!(model_of(&full(), &store).start.on);
    apply(&mut store, Change::StartAtLogin(false));
    assert!(!model_of(&full(), &store).start.on);
    assert!(dir.0.join("io.github.cappyt.NiceCooling.desktop").exists());
    // The switch is not in `settings.toml`.
    assert!(!toml::to_string(&store.values).unwrap().contains("login"));
}

#[test]
fn start_at_login_on_removes_override() {
    let dir = Dir::new("autostart-on");
    let autostart_dir = dir.0.join("autostart");
    autostart::set_enabled(&autostart_dir, false).unwrap();
    let file = autostart_dir.join("io.github.cappyt.NiceCooling.desktop");
    assert!(file.exists());
    autostart::set_enabled(&autostart_dir, true).expect("the override goes");
    assert!(!file.exists());
    assert!(autostart::is_enabled(&autostart_dir));
    // On again with no file is not an error.
    autostart::set_enabled(&autostart_dir, true).expect("nothing to remove");

    // An override without `Hidden=true` leaves the switch on.
    std::fs::write(
        &file,
        "[Desktop Entry]\nType=Application\nName=NiceCooling\n",
    )
    .unwrap();
    assert!(autostart::is_enabled(&autostart_dir));
}

#[test]
fn state_directory_shown_read_only() {
    let store = SettingsStore::default();
    let model = model_of(&with_board(), &store);
    let row = model
        .daemon
        .rows
        .iter()
        .find(|row| row.label == "State directory")
        .expect("the row");
    assert_eq!(row.value, "/var/lib/nicecooling/");
    assert_eq!(row.value, STATE_DIRECTORY);
    assert!(row.read_only && !row.from_daemon);
    // Every switch of the screen is one of the five keys: none writes the directory.
    let keys = [
        SwitchKey::StartAtLogin,
        SwitchKey::Tray,
        SwitchKey::Warnings,
        SwitchKey::ProfileSwitchedByRule,
        SwitchKey::ExternalChange,
    ];
    for key in keys {
        assert!(!matches!(key.change(true), Change::Language(_)));
    }
    assert_eq!(model.daemon.rows.len(), 4);
    // The directory shows while the daemon is away, too.
    let mut offline = with_board();
    offline.link = Link::Offline;
    assert!(
        model_of(&offline, &store)
            .daemon
            .rows
            .iter()
            .any(|r| r.value == STATE_DIRECTORY)
    );
}

#[test]
fn switches_apply_at_once() {
    let mut store = SettingsStore::default();
    let prefs = store.notify_prefs.clone();
    assert!(prefs.lock().unwrap().warnings);
    apply(&mut store, Change::Warnings(false));
    apply(&mut store, Change::ProfileSwitchedByRule(true));
    apply(&mut store, Change::ExternalChange(false));
    let now = *prefs.lock().unwrap();
    assert!(!now.warnings && now.profile_switched_by_rule && !now.external_change);
    apply(&mut store, Change::Language(Language::English));
    assert_eq!(store.values.language, "en");
}

#[test]
fn settings_labels() {
    let en = labels::load("en");
    let it = labels::load("it");
    // The copy tables of docs/ui/screens/settings.md.
    let table = [
        ("settings.language.label", "Language", "Lingua"),
        (
            "settings.start.login",
            "Start at login",
            "Avvia all'accesso",
        ),
        (
            "settings.start.tray",
            "Tray icon",
            "Icona nell'area di notifica",
        ),
        ("settings.notifications.warnings", "Warnings", "Avvisi"),
        (
            "settings.notifications.profile",
            "Profile switched by a rule",
            "Profilo cambiato da una regola",
        ),
        (
            "settings.notifications.external",
            "External change on a Leakshield",
            "Modifica esterna su un Leakshield",
        ),
        (
            "settings.notifications.note",
            "Alarm notifications are always on.",
            "Le notifiche degli Allarmi sono sempre attive.",
        ),
        ("settings.daemon.connection", "Connection", "Connessione"),
        (
            "settings.daemon.version",
            "Daemon version",
            "Versione del demone",
        ),
        (
            "settings.daemon.app_version",
            "App version",
            "Versione dell'app",
        ),
        (
            "settings.daemon.state_directory",
            "State directory",
            "Cartella di stato",
        ),
        ("settings.daemon.try_again", "Try again", "Riprova"),
        (
            "settings.start.login_text",
            "The app starts without a window and shows its tray icon.",
            "L'app parte senza finestra e mostra la sua icona nell'area di notifica.",
        ),
        (
            "settings.start.tray_off",
            "Without the tray icon, open the window from the app list.",
            "Senza l'icona, apri la finestra dall'elenco delle app.",
        ),
        (
            "settings.hint.tray",
            "The tray icon is not visible. On GNOME, turn on the AppIndicator extension.",
            "L'icona nell'area di notifica non è visibile. Su GNOME, attiva l'estensione AppIndicator.",
        ),
        (
            "settings.hint.tray_link",
            "How to turn it on",
            "Come attivarla",
        ),
        (
            "settings.about.note",
            "nicecooling is not a product of Aqua Computer. LEAKSHIELD and aquasuite are names of Aqua Computer GmbH & Co. KG.",
            "nicecooling non è un prodotto di Aqua Computer. LEAKSHIELD e aquasuite sono nomi di Aqua Computer GmbH & Co. KG.",
        ),
        (
            "settings.about.firmware",
            "nicecooling does not update the firmware of any device.",
            "nicecooling non aggiorna il firmware di nessun dispositivo.",
        ),
        (
            "settings.daemon.offline_text",
            "Start the daemon: systemctl start nicecoolingd",
            "Avvia il demone: systemctl start nicecoolingd",
        ),
    ];
    for (key, english, italian) in table {
        assert_eq!(en[key], english, "English text of {key}");
        assert_eq!(it[key], italian, "Italian text of {key}");
    }
    for key in en.keys().filter(|key| key.starts_with("settings.")) {
        assert!(it.contains_key(key), "Italian text of {key}");
    }
    // The model uses the language of its input.
    let state = with_board();
    let feedback = Feedback::default();
    let italian = SettingsModel::build(
        &ScreenInput {
            state: &state,
            now_us: NOW_US,
            locale: IT,
            down_since_us: None,
            feedback: &feedback,
        },
        &SettingsStore::default(),
    );
    assert_eq!(italian.language.label, "Lingua");
    assert_eq!(italian.daemon.connection.value, "Connesso");
}
