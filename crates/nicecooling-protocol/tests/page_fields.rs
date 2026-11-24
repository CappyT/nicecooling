//! The page fields that point at report 8 or at a sensor, with the meaning that the code of
//! aquasuite gives them: `plausible-read` when the fixtures hold values in range.

use nicecooling_protocol::registry::{Registry, VerificationState};
use nicecooling_protocol::report3::Report3;

const FIXTURES: [&[u8]; 2] = [
    include_bytes!("../../../tests/fixtures/leakshield/fw1022/report3.bin"),
    include_bytes!("../../../tests/fixtures/leakshield/fw1019/report3.bin"),
];
const FIELDS: [&str; 4] = ["page_sources", "page_values", "page_names", "page_icons"];

#[test]
fn the_four_fields_are_plausible_reads_and_the_fixtures_are_in_range() {
    let registry = Registry::leakshield();
    for id in FIELDS {
        let field = registry.field(id).unwrap();
        assert_eq!(
            field.verification_state,
            VerificationState::PlausibleRead,
            "{id}"
        );
        for fixture in FIXTURES {
            let report = Report3::decode(fixture).unwrap();
            assert!(field.in_range_in(&report), "{id}");
        }
    }
}

#[test]
fn ranges_follow_the_meaning() {
    let registry = Registry::leakshield();
    let ok = |id: &str, raw: i64| registry.field(id).unwrap().raw_in_range(raw);
    // A source is the sensor index times 4 for the sensors 0 to 19, or 255.
    assert!(ok("page_sources", 0) && ok("page_sources", 76) && ok("page_sources", 255));
    assert!(!ok("page_sources", 2) && !ok("page_sources", 80) && !ok("page_sources", 254));
    // A text is an index into the 40 names, an icon an index into the 16 icons.
    assert!(ok("page_names", 39) && ok("page_names", 255) && !ok("page_names", 40));
    assert!(ok("page_icons", 15) && ok("page_icons", 255) && !ok("page_icons", 16));
    assert!(
        ok("page_values", -32000)
            && ok("page_values", 32000)
            && !ok("page_values", 33000)
            && !ok("page_values", -33000)
    );
}
