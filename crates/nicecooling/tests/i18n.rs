mod support;

use support::labels;

#[test]
fn every_key_has_both_languages() {
    let en = labels::load("en");
    let it = labels::load("it");
    let only_en: Vec<_> = en.keys().filter(|key| !it.contains_key(*key)).collect();
    let only_it: Vec<_> = it.keys().filter(|key| !en.contains_key(*key)).collect();
    assert!(only_en.is_empty(), "keys without Italian: {only_en:?}");
    assert!(only_it.is_empty(), "keys without English: {only_it:?}");
    for (key, text) in &en {
        assert!(!text.trim().is_empty(), "empty English text for {key}");
        assert!(!it[key].trim().is_empty(), "empty Italian text for {key}");
        assert_eq!(
            labels::placeholders(text),
            labels::placeholders(&it[key]),
            "placeholders of {key} differ between languages"
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
