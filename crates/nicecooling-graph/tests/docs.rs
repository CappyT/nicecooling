//! Checks `docs/graphs.md` against the model: one table row for each node kind, with its pins and
//! parameters, and the section "Downgrade".

use std::path::PathBuf;

use nicecooling_graph::model::{Kind, ResponseKind, param_specs};

fn document() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/graphs.md");
    std::fs::read_to_string(path).expect("docs/graphs.md can be read")
}

/// The lines of the section that starts with `heading`, up to the next `## ` heading.
fn section<'a>(text: &'a str, heading: &str) -> Vec<&'a str> {
    let mut lines = text.lines();
    let found = lines.by_ref().any(|line| line == heading);
    assert!(found, "docs/graphs.md has no section {heading:?}");
    lines.take_while(|line| !line.starts_with("## ")).collect()
}

/// The trimmed cells of one table row.
fn cells(line: &str) -> Vec<&str> {
    line.trim()
        .trim_matches('|')
        .split('|')
        .map(str::trim)
        .collect()
}

#[test]
fn docs_table_has_one_row_for_each_kind() {
    let text = document();
    let rows: Vec<Vec<&str>> = section(&text, "## Node kinds")
        .into_iter()
        .filter(|line| line.starts_with("| `"))
        .map(cells)
        .collect();
    assert_eq!(rows.len(), Kind::ALL.len(), "one row for each kind");
    for row in &rows {
        assert_eq!(row.len(), 4, "the cells of the row {row:?}");
    }
    for kind in Kind::ALL {
        let name = format!("`{}`", kind.as_str());
        let matching: Vec<&Vec<&str>> = rows.iter().filter(|row| row[0] == name).collect();
        assert_eq!(matching.len(), 1, "the rows of the kind {kind}");
        let row = matching[0];
        for pin in kind.inputs() {
            assert!(
                row[1].contains(&format!("`{}`", pin.name)),
                "the pin {} of {kind} is in the table",
                pin.name
            );
        }
        // The common parameters of a response end node are in its row. The parameters of each
        // response kind have their own table.
        let names: Vec<&str> = if *kind == Kind::ResponseEnd {
            vec!["kind", "hold_s", "repeat_s", "min_interval_s"]
        } else {
            param_specs(*kind, ResponseKind::Notification)
                .iter()
                .map(|spec| spec.name)
                .collect()
        };
        for name in names {
            assert!(
                row[2].contains(&format!("`{name}`")),
                "the parameter {name} of {kind} is in the table"
            );
        }
    }
}

#[test]
fn docs_has_the_downgrade_section() {
    let text = document();
    let lines = section(&text, "## Downgrade");
    let steps = lines
        .iter()
        .filter(|line| {
            line.split_once(". ").is_some_and(|(number, _)| {
                !number.is_empty() && number.chars().all(|c| c.is_ascii_digit())
            })
        })
        .count();
    assert!(steps >= 4, "the section has {steps} numbered steps");
    let body = lines.join("\n");
    for needle in [
        "graph_sensors",
        "graph_sensor_next",
        "lost-input",
        "new ids",
    ] {
        assert!(body.contains(needle), "the section names {needle:?}");
    }
}
