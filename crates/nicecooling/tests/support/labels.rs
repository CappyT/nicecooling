//! Helpers that load the locale files and read the labels of a screen.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

pub fn load(locale: &str) -> BTreeMap<String, String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("locales/{locale}.yml"));
    let text = std::fs::read_to_string(&path).expect("locale file");
    let value: Value = serde_saphyr::from_str(&text).expect("valid YAML");
    let mut flat = BTreeMap::new();
    flatten("", &value, &mut flat);
    flat
}

fn flatten(prefix: &str, value: &Value, out: &mut BTreeMap<String, String>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let name = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(&name, child, out);
            }
        }
        Value::String(text) => {
            out.insert(prefix.to_owned(), text.clone());
        }
        other => panic!("locale value of {prefix} is not a string: {other}"),
    }
}

/// The names between `%{` and `}`.
pub fn placeholders(text: &str) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut rest = text;
    while let Some(start) = rest.find("%{") {
        let after = &rest[start + 2..];
        let Some(end) = after.find('}') else { break };
        names.insert(after[..end].to_owned());
        rest = &after[end + 1..];
    }
    names
}

/// Every literal key that `src/` passes to `tr`, `fmt` or `plural`.
pub fn keys_used_in_source() -> BTreeSet<(String, bool)> {
    let mut keys = BTreeSet::new();
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    collect(&dir, &mut keys);
    keys
}

fn collect(dir: &Path, keys: &mut BTreeSet<(String, bool)>) {
    for entry in std::fs::read_dir(dir).expect("source dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            collect(&path, keys);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let text = std::fs::read_to_string(&path).expect("source file");
            for (call, plural) in [(".tr(\"", false), (".fmt(\"", false), (".plural(\"", true)] {
                let mut rest = text.as_str();
                while let Some(start) = rest.find(call) {
                    let after = &rest[start + call.len()..];
                    let end = after.find('"').expect("closing quote");
                    keys.insert((after[..end].to_owned(), plural));
                    rest = &after[end..];
                }
            }
        }
    }
}
