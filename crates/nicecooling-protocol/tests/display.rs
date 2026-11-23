mod common;

use common::fw1022;
use nicecooling_protocol::display::{PAGES, Page, read_pages};
use nicecooling_protocol::registry::Registry;
use nicecooling_protocol::report3::Report3;

fn report3() -> Report3 {
    Report3::decode(fw1022().report3).unwrap()
}

#[test]
fn read_all_10_pages_from_fixture() {
    let pages = read_pages(&Registry::leakshield(), &report3()).unwrap();

    assert_eq!(pages.len(), PAGES);
    // Page 0 shows the Leakshield state and has no value source.
    assert_eq!(pages[0].template, 13);
    assert_eq!(pages[0].options, 3);
    assert_eq!(pages[0].sources, [255; 8]);
    assert_eq!(pages[0].values, [0, 100, 0, 100, 0, 100, 0, 100]);
    // Page 3 shows two values with an icon each.
    assert_eq!(pages[3].template, 3);
    assert_eq!(pages[3].icons, [2, 9, 255, 255]);
    assert_eq!(pages[3].names, [0, 3, 255, 255, 255, 255, 255, 255]);
    assert_eq!(pages[3].sources, [0, 12, 255, 255, 255, 255, 255, 255]);
    assert_eq!(pages[3].precisions, [1, 1, 0, 0, 0, 0, 0, 0]);
    // Page 9 is the last one in the report: template 0 (logo), shown in standby only.
    assert_eq!(pages[9].template, 0);
    assert_eq!(pages[9].options, 1);
    // Each page comes from its own 46 bytes.
    let report = report3();
    for (i, page) in pages.iter().enumerate() {
        assert_eq!(
            page.template,
            report.bytes_at(73 + 46 * i, 1)[0],
            "page {i}"
        );
        assert_eq!(page.names, report.bytes_at(79 + 46 * i, 8), "page {i}");
    }
    // `raw` returns what `read` took for every page field.
    let registry = Registry::leakshield();
    for (i, page) in pages.iter().enumerate() {
        for id in nicecooling_protocol::display::PAGE_FIELDS {
            assert_eq!(
                page.raw(id),
                registry.read_repeat(&report, id, i),
                "{id} of page {i}"
            );
        }
    }
    assert!(Page::read(&registry, &report, PAGES).is_none());
}
