use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_protocol::display::{PAGES, Page, read_pages};
use nicecooling_protocol::report3::Report3;
use nicecooling_protocol::sim::Request;
use nicecoolingd::leakshield::display::page_edits;
use nicecoolingd::leakshield::write::WriteError;
use zbus::Variant;

use crate::Rig as DeviceRig;
use crate::support::{LEAKSHIELD, Rig, error_name, path};

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";

fn page(rig: &DeviceRig, index: usize) -> (Report3, Page) {
    let report3 = rig.io.read_report3().unwrap();
    let page = Page::read(&rig.registry, &report3, index).unwrap();
    (report3, page)
}

fn feature_writes(rig: &DeviceRig) -> usize {
    rig.sim
        .requests()
        .iter()
        .filter(|r| matches!(r, Request::SetFeature { .. }))
        .count()
}

#[test]
fn write_page_fields_through_write_path() {
    let rig = DeviceRig::new("display-write");
    let before = rig.sim.report3();
    let (report3, mut changed) = page(&rig, 4);
    // Template 2 (one value) and the option bits are `plausible-read` fields.
    changed.template = 2;
    changed.options = 1;

    let edits = page_edits(&rig.registry, &report3, 4, &changed).unwrap();
    // Only the two fields that differ get an edit, each with the 10 pages.
    assert_eq!(
        edits.iter().map(|e| e.field.as_str()).collect::<Vec<_>>(),
        ["page_template", "page_options"]
    );
    assert!(edits.iter().all(|e| e.raw.len() == PAGES));
    let written = rig.write(&edits).unwrap();

    // Only the two bytes of page 4 and the CRC changed.
    let after = rig.sim.report3();
    let at = 73 + 46 * 4;
    let diff: Vec<usize> = (0..750).filter(|&i| after[i] != before[i]).collect();
    assert_eq!(diff, [at, at + 1, 748, 749]);
    assert_eq!((after[at], after[at + 1]), (2, 1));
    // The read-back holds the new page, and the other pages are as before.
    let pages = read_pages(&rig.registry, &written.report3).unwrap();
    assert_eq!(pages[4], changed);
    let old = read_pages(&rig.registry, &Report3::decode(&before).unwrap()).unwrap();
    for i in (0..PAGES).filter(|&i| i != 4) {
        assert_eq!(pages[i], old[i], "page {i}");
    }
    // The state before the write is a backup.
    assert_eq!(rig.backups().get(&written.backup).unwrap().report3, before);
}

#[test]
fn page_sources_names_icons_and_values_are_written() {
    let rig = DeviceRig::new("display-pointers");
    let (report3, mut changed) = page(&rig, 1);
    // Source 1 is the sensor 5 (index x 4), text 20 is a page text, icon 3, and a bar range.
    changed.sources[1] = 20;
    changed.names[1] = 20;
    changed.icons[3] = 3;
    changed.values[2] = -500;
    changed.values[3] = 1500;

    let edits = page_edits(&rig.registry, &report3, 1, &changed).unwrap();
    let written = rig.write(&edits).unwrap();

    let pages = read_pages(&rig.registry, &written.report3).unwrap();
    assert_eq!(pages[1], changed);
}

#[test]
fn a_page_source_that_is_not_a_sensor_index_times_4_is_refused() {
    let rig = DeviceRig::new("display-refused");
    let before = rig.sim.report3();
    let (report3, mut changed) = page(&rig, 1);
    changed.sources[1] = 5;

    let edits = page_edits(&rig.registry, &report3, 1, &changed).unwrap();
    let error = rig.write(&edits).unwrap_err();

    assert!(
        matches!(&error, WriteError::OutOfRange { field } if field == "page_sources"),
        "{error:?}"
    );
    assert_eq!(rig.sim.report3(), before);
    assert_eq!(feature_writes(&rig), 0);
}

#[test]
fn page_edits_reject_a_page_that_does_not_exist() {
    let rig = DeviceRig::new("display-index");
    let (report3, page) = page(&rig, 0);
    assert!(page_edits(&rig.registry, &report3, PAGES, &page).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_assets_have_no_field_to_write() {
    let rig = Rig::start().await;
    let proxy = LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let report3 = rig.sim.report3();
    let report8 = rig.sim.report8();
    let set = |id: &'static str, len: usize| {
        proxy.set_fields([(id, Variant::from(vec![0_i64; len]))].into())
    };

    // The assets (names, start image, icons of report 8) have no field: `SetDisplayName`,
    // `SetDisplayIcon` and `SetDisplayLogo` write them.
    for id in ["asset_names", "start_image", "asset_icons"] {
        let error = set(id, 1).await.unwrap_err();
        assert_eq!(error_name(&error), format!("{ERR}.NotFound"), "{id}");
    }

    assert_eq!(rig.sim.report3(), report3);
    assert_eq!(rig.sim.report8(), report8);
    assert!(
        !rig.sim
            .requests()
            .iter()
            .any(|r| matches!(r, Request::SetFeature { .. }))
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn page_fields_through_get_and_set_fields() {
    let rig = Rig::start().await;
    let proxy = LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap();
    let list = |rows: &[(String, String, zbus::OwnedVariant, bool, String)], id: &str| {
        let row = rows.iter().find(|r| r.0 == id).unwrap();
        (row.2.try_clone().unwrap(), row.3)
    };

    // `page_template` is an enum list: `as`, with the name of each page template.
    let (templates, writable) = list(&proxy.get_fields().await.unwrap(), "page_template");
    let mut templates = Vec::<String>::try_from(templates).unwrap();
    assert_eq!(templates.len(), PAGES);
    assert!(writable);
    templates[9] = "time".to_owned();
    proxy
        .set_fields([("page_template", Variant::from(templates.clone()))].into())
        .await
        .unwrap();

    let rows = proxy.get_fields().await.unwrap();
    let (read_back, _) = list(&rows, "page_template");
    assert_eq!(Vec::<String>::try_from(read_back).unwrap(), templates);
    assert_eq!(rig.sim.report3()[73 + 46 * 9], 7);
    // `page_names` has one list of 8 bytes for each page: an integer list.
    let (names, writable) = list(&rows, "page_names");
    assert_eq!(Vec::<i64>::try_from(names).unwrap().len(), PAGES * 8);
    assert!(writable);
}
