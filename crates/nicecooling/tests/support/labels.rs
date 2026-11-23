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

/// Every quoted label of the Layout and States sections of a screen file of `docs/ui/screens/`.
pub fn quoted_labels(screen_file: &str) -> Vec<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/ui/screens")
        .join(screen_file);
    let text = std::fs::read_to_string(&path).expect("screen file");
    let mut labels: Vec<String> = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        if let Some(title) = line.strip_prefix("## ") {
            inside = matches!(title.trim(), "Layout" | "States");
            continue;
        }
        if !inside {
            continue;
        }
        let mut parts = line.split('"');
        // The pieces at odd positions sit between a pair of quotes.
        parts.next();
        for (index, piece) in parts.enumerate() {
            if index % 2 == 0 && !piece.trim().is_empty() && !labels.contains(&piece.to_owned()) {
                labels.push(piece.to_owned());
            }
        }
    }
    labels
}

/// The labels of a screen file that no English text of the app contains. A label with data in
/// it ("Active since 21:46:51") is in `examples`: the test checks its fixed part.
pub fn missing_labels(screen_file: &str, examples: &[&str]) -> Vec<String> {
    let en = load("en");
    quoted_labels(screen_file)
        .into_iter()
        .filter(|label| !examples.contains(&label.as_str()))
        .filter(|label| !en.values().any(|text| text.contains(label.as_str())))
        .collect()
}
