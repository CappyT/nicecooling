mod support;

use nicecooling::i18n::CODES;
use support::labels;

/// `en.yml` is the reference. Every other language of `CODES` has its keys and its placeholders.
/// A language without a file fails here.
#[test]
fn every_key_has_every_language() {
    let en = labels::load("en");
    for code in CODES.into_iter().filter(|code| *code != "en") {
        let other = labels::load(code);
        let missing: Vec<_> = en.keys().filter(|key| !other.contains_key(*key)).collect();
        let extra: Vec<_> = other.keys().filter(|key| !en.contains_key(*key)).collect();
        assert!(
            missing.is_empty(),
            "keys without text in {code}: {missing:?}"
        );
        assert!(
            extra.is_empty(),
            "keys of {code} without English: {extra:?}"
        );
        for (key, text) in &en {
            assert!(!text.trim().is_empty(), "empty English text for {key}");
            assert!(!other[key].trim().is_empty(), "empty {code} text for {key}");
            assert_eq!(
                labels::placeholders(text),
                labels::placeholders(&other[key]),
                "placeholders of {key} differ between en and {code}"
            );
        }
    }
}

/// The order of the keys is the order of `en.yml`, so that a diff between two files lines up.
#[test]
fn every_language_keeps_the_order_of_the_keys() {
    let order = |code: &str| {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("locales/{code}.yml"));
        std::fs::read_to_string(path)
            .expect("locale file")
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
            .map(|line| {
                let indent = line.len() - line.trim_start().len();
                let key = line.trim_start().split(':').next().unwrap_or("");
                format!("{indent} {key}")
            })
            .collect::<Vec<_>>()
    };
    let en = order("en");
    for code in CODES.into_iter().filter(|code| *code != "en") {
        assert_eq!(
            order(code),
            en,
            "the structure of {code}.yml differs from en.yml"
        );
    }
}

#[test]
fn every_key_used_in_source_exists() {
    let en = labels::load("en");
    for (key, plural) in labels::keys_used_in_source() {
        let names = if plural {
            vec![format!("{key}.one"), format!("{key}.other")]
        } else {
            vec![key]
        };
        for name in names {
            assert!(en.contains_key(&name), "missing locale key {name}");
        }
    }
}
