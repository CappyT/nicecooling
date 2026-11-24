use std::path::Path;

fn entry(name: &str) -> Vec<(String, String)> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packaging")
        .join(name);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
    text.lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

fn value<'a>(pairs: &'a [(String, String)], key: &str) -> Option<&'a str> {
    pairs
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.as_str())
}

#[test]
fn autostart_entry_runs_the_app_in_the_background() {
    let autostart = entry("io.github.cappyt.NiceCooling.autostart.desktop");
    assert_eq!(value(&autostart, "Exec"), Some("nicecooling --background"));
    assert_eq!(value(&autostart, "Type"), Some("Application"));
    // The user override of docs/storage.md has the same name, so the session ignores this one.
    assert_eq!(value(&autostart, "Name"), Some("NiceCooling"));
}

#[test]
fn desktop_entry_opens_the_window() {
    let desktop = entry("io.github.cappyt.NiceCooling.desktop");
    assert_eq!(value(&desktop, "Exec"), Some("nicecooling"));
    assert_eq!(
        value(&desktop, "Icon"),
        Some("io.github.cappyt.NiceCooling")
    );
    // The window has this app id: the desktop then shows the entry name and icon for it.
    assert_eq!(
        value(&desktop, "StartupWMClass"),
        Some(nicecooling::app::APP_ID)
    );
}
