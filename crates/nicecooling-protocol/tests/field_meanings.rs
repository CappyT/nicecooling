//! The meaning column of the report 3 table in `docs/protocol/leakshield.md` for the fields whose
//! meaning is not obvious from the name. The table comes from `tools/re/gen-leakshield-registry.py`.

fn meaning(id: &str) -> String {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/protocol/leakshield.md");
    let text = std::fs::read_to_string(path).expect("the protocol document");
    let row = text
        .lines()
        .find(|line| line.starts_with(&format!("| `{id}` |")))
        .unwrap_or_else(|| panic!("no row for {id}"));
    row.trim_end_matches(" |")
        .rsplit(" | ")
        .next()
        .unwrap()
        .to_owned()
}

#[test]
fn the_meaning_of_the_flag_fields() {
    for (id, phrases) in [
        (
            "page_sources",
            &["sensor index times 4", "80 or more has no sensor"][..],
        ),
        (
            "display_page_time",
            &["30 s and more: no automatic page change"],
        ),
        (
            "display_flags",
            &[
                "Bit 0: rotate the display content by 180 degrees",
                "Bit 1: display stays on in standby (clear: off after 10 minutes in standby)",
            ],
        ),
        (
            "standby_conditions",
            &["Bit 3: disable the alarm actions during standby (set: disabled)"],
        ),
        (
            "daily_pump_time_warning",
            &["Warning limit of the daily pump time"],
        ),
        (
            "led_strip_flags",
            &["Bit 2: LEDs off in standby (clear: LEDs stay on in standby)"],
        ),
    ] {
        let text = meaning(id);
        for phrase in phrases {
            assert!(text.contains(phrase), "{id}: {text:?} lacks {phrase:?}");
        }
    }
}
