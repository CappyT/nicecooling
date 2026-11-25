//! English, Italian, German and French text. The daemon sends identifiers, and the app owns every string.

use std::borrow::Cow;

/// The language choice of `settings.toml`: `system`, `en`, `it`, `de` or `fr`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Language {
    #[default]
    System,
    English,
    Italian,
    German,
    French,
}

/// A resolved locale. Every function that makes text takes one, so tests do not share a global.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Locale(&'static str);

pub const EN: Locale = Locale("en");
pub const IT: Locale = Locale("it");
pub const DE: Locale = Locale("de");
pub const FR: Locale = Locale("fr");

/// The codes of the languages that have a file in `locales/`.
pub const CODES: [&str; 4] = ["en", "it", "de", "fr"];

impl Language {
    pub fn from_setting(value: &str) -> Self {
        match value {
            "en" => Self::English,
            "it" => Self::Italian,
            "de" => Self::German,
            "fr" => Self::French,
            _ => Self::System,
        }
    }

    pub fn resolve(self) -> Locale {
        self.resolve_with(|name| std::env::var(name).ok())
    }

    fn resolve_with(self, env: impl Fn(&str) -> Option<String>) -> Locale {
        match self {
            Self::English => EN,
            Self::Italian => IT,
            Self::German => DE,
            Self::French => FR,
            Self::System => ["LC_ALL", "LC_MESSAGES", "LANG"]
                .into_iter()
                .filter_map(env)
                .find(|value| !value.is_empty())
                .map_or(EN, |value| {
                    [(IT, "it"), (DE, "de"), (FR, "fr")]
                        .into_iter()
                        .find(|(_, prefix)| value.starts_with(prefix))
                        .map_or(EN, |(locale, _)| locale)
                }),
        }
    }
}

impl Locale {
    pub fn code(self) -> &'static str {
        self.0
    }

    /// The text of `key`. A missing key gives the key itself, so a gap shows in the window.
    pub fn tr(self, key: &str) -> String {
        match crate::_rust_i18n_try_translate(self.0, key) {
            Some(text) => text.into_owned(),
            None => key.to_owned(),
        }
    }

    /// The text of `key` with each `%{name}` replaced.
    pub fn fmt(self, key: &str, args: &[(&str, &str)]) -> String {
        let mut text = self.tr(key);
        for (name, value) in args {
            text = text.replace(&format!("%{{{name}}}"), value);
        }
        text
    }

    /// The text of `<key>.one` or `<key>.other`, with `%{count}` replaced.
    pub fn plural(self, key: &str, count: usize) -> String {
        let form = if count == 1 { "one" } else { "other" };
        self.fmt(&format!("{key}.{form}"), &[("count", &count.to_string())])
    }

    /// The text of an event identifier of `docs/api.md`: `pump-stop`, `leakshield.leak-warning`.
    pub fn event_title(self, event: &str) -> String {
        let key = format!("event.{}.title", event_key(event));
        match crate::_rust_i18n_try_translate(self.0, &key) {
            Some(text) => text.into_owned(),
            None => self.fmt("event.unknown.title", &[("id", event)]),
        }
    }
}

/// `leakshield.fill-level-low-alarm` becomes `leakshield_fill_level_low_alarm`.
pub fn event_key(event: &str) -> Cow<'_, str> {
    if event.contains(['.', '-']) {
        Cow::Owned(event.replace(['.', '-'], "_"))
    } else {
        Cow::Borrowed(event)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_language_follows_the_environment() {
        let it = |name: &str| (name == "LANG").then(|| "it_IT.UTF-8".to_owned());
        let en = |name: &str| (name == "LANG").then(|| "en_US.UTF-8".to_owned());
        assert_eq!(Language::System.resolve_with(it), IT);
        assert_eq!(Language::System.resolve_with(en), EN);
        assert_eq!(Language::System.resolve_with(|_| None), EN);
        assert_eq!(Language::Italian.resolve_with(en), IT);
    }

    #[test]
    fn system_language_picks_german_and_french() {
        let lang =
            |value: &'static str| move |name: &str| (name == "LANG").then(|| value.to_owned());
        assert_eq!(Language::System.resolve_with(lang("de_DE.UTF-8")), DE);
        assert_eq!(Language::System.resolve_with(lang("de_CH")), DE);
        assert_eq!(Language::System.resolve_with(lang("fr_FR.UTF-8")), FR);
        assert_eq!(Language::System.resolve_with(lang("fr_CA.UTF-8")), FR);
        assert_eq!(Language::System.resolve_with(lang("es_ES.UTF-8")), EN);
        // LC_ALL wins over LANG, and an empty value is skipped.
        let both = |name: &str| match name {
            "LC_ALL" => Some("de_DE.UTF-8".to_owned()),
            "LANG" => Some("fr_FR.UTF-8".to_owned()),
            _ => None,
        };
        assert_eq!(Language::System.resolve_with(both), DE);
        let empty_first = |name: &str| match name {
            "LC_ALL" => Some(String::new()),
            "LANG" => Some("fr_FR.UTF-8".to_owned()),
            _ => None,
        };
        assert_eq!(Language::System.resolve_with(empty_first), FR);
        assert_eq!(Language::German.resolve_with(lang("fr_FR")), DE);
        assert_eq!(Language::French.resolve_with(lang("de_DE")), FR);
    }

    #[test]
    fn the_setting_names_each_language() {
        for (value, language) in [
            ("en", Language::English),
            ("it", Language::Italian),
            ("de", Language::German),
            ("fr", Language::French),
            ("system", Language::System),
            ("xx", Language::System),
        ] {
            assert_eq!(Language::from_setting(value), language, "{value}");
        }
        assert_eq!(
            [EN, IT, DE, FR].map(Locale::code),
            CODES,
            "the codes of the locales"
        );
    }

    #[test]
    fn plural_and_event_text() {
        assert_eq!(EN.plural("status.alarms", 1), "1 Alarm");
        assert_eq!(IT.plural("status.warnings", 2), "2 Avvisi");
        assert_eq!(EN.event_title("pump-stop"), "Pump stopped");
        assert_eq!(EN.event_title("leakshield.leak-alarm"), "Large leak Alarm");
        assert_eq!(EN.event_title("mystery"), "Event mystery");
        for id in ["leakshield-unknown-firmware", "no-bios-snapshot"] {
            for locale in [EN, IT, DE] {
                for start in ["Event ", "Evento ", "Ereignis "] {
                    assert!(!locale.event_title(id).starts_with(start), "{id}");
                }
            }
        }
    }
}
