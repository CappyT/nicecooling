//! The example states of `docs/profiles.md` load with the state loader (task 7.7).

use std::collections::BTreeMap;
use std::path::PathBuf;

use nicecoolingd::state::{StateStore, load_state};
use serde_json::Value;

/// The text of each fenced `json` block, in file order.
fn json_blocks(text: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        match (&mut current, line.trim_end()) {
            (None, "```json") => current = Some(String::new()),
            (Some(_), "```") => blocks.extend(current.take()),
            (Some(block), line) => {
                block.push_str(line);
                block.push('\n');
            }
            (None, _) => {}
        }
    }
    blocks
}

/// True when every key and value of `example` is in `loaded`. A key that the loader does not
/// know is lost when it re-serializes, so a typo in an example fails here.
fn is_subset(example: &Value, loaded: &Value) -> bool {
    match (example, loaded) {
        (Value::Object(example), Value::Object(loaded)) => example
            .iter()
            .all(|(key, value)| loaded.get(key).is_some_and(|l| is_subset(value, l))),
        (Value::Array(example), Value::Array(loaded)) => {
            example.len() == loaded.len()
                && example.iter().zip(loaded).all(|(e, l)| is_subset(e, l))
        }
        (Value::Number(example), Value::Number(loaded)) => example.as_f64() == loaded.as_f64(),
        (example, loaded) => example == loaded,
    }
}

#[test]
fn profiles_md_examples_load() {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/profiles.md");
    let blocks = json_blocks(&std::fs::read_to_string(file).unwrap());
    assert!(blocks.len() >= 5, "{} blocks", blocks.len());
    let examples: Vec<Value> = blocks
        .iter()
        .map(|b| serde_json::from_str(b).expect("a block is JSON"))
        .collect();
    // A Profile file has an `id` and a `slot`. The other blocks are `state.json` files.
    let (profiles, states): (Vec<&Value>, Vec<&Value>) = examples
        .iter()
        .partition(|e| e.get("id").is_some() && e.get("slot").is_some());
    assert!(profiles.len() >= 3 && states.len() >= 2);

    let dir = std::env::temp_dir().join(format!("nicecooling-profiles-md-{}", std::process::id()));
    for state in states {
        let _ = std::fs::remove_dir_all(&dir);
        for profile in &profiles {
            let id = profile["id"].as_str().unwrap();
            std::fs::create_dir_all(dir.join("profiles").join(id)).unwrap();
            std::fs::write(
                dir.join("profiles").join(id).join("profile.json"),
                serde_json::to_vec(profile).unwrap(),
            )
            .unwrap();
        }
        std::fs::write(dir.join("state.json"), serde_json::to_vec(state).unwrap()).unwrap();

        let loaded = load_state(&StateStore::new(&dir));

        assert!(!loaded.fallback, "{state}");
        assert!(!loaded.first_start);
        assert!(
            is_subset(state, &serde_json::to_value(&loaded.state).unwrap()),
            "the state loader dropped a key of {state}"
        );
        // Every Profile file of the document loads as it is written.
        let loaded_profiles: BTreeMap<_, _> = loaded
            .profiles
            .iter()
            .map(|p| (p.id.clone(), serde_json::to_value(p).unwrap()))
            .collect();
        for profile in &profiles {
            let id = profile["id"].as_str().unwrap();
            assert!(
                is_subset(profile, &loaded_profiles[id]),
                "the loader dropped a key of {id}"
            );
        }
        // The active Profile of the state is one of the Profiles, and the groups point to some.
        assert!(loaded.profiles.contains(&loaded.state.profiles.active));
        for group in &loaded.state.rule_groups {
            assert!(loaded.profiles.contains(&group.profile), "{}", group.id);
            for rule in &group.rules {
                assert_eq!(rule.validate(), Ok(()), "{}", group.id);
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
