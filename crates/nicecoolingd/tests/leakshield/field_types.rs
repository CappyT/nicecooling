//! The variant types of `GetFields` and `SetFields` follow the registry kind of each field
//! (`docs/api.md`, Leakshield fields): an enum is `s` or `as`, a scaled field is `d` or `ad`, any
//! other field is `x` or `ax`.

use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use nicecooling_protocol::registry::Registry;
use zbus::{OwnedVariant, Variant};

use crate::support::{LEAKSHIELD, Rig, error_name, path};

const ERR: &str = "io.github.cappyt.NiceCooling1.Error";
/// An enum with one element, a low-risk field at `plausible-read`.
const ENUM: &str = "display_brightness_active";
const ENUM_OFFSET: usize = 65;
/// An enum with 10 elements.
const ENUM_LIST: &str = "page_template";
const PAGE_TEMPLATE_OFFSET: usize = 73;
const PAGE_STRIDE: usize = 46;
/// A scaled field with one element.
const SCALED: &str = "display_page_time";
/// An integer field (a bit mask) with one element, and with 10 elements.
const INTEGER: &str = "display_flags";
const INTEGER_OFFSET: usize = 72;
const INTEGER_LIST: &str = "page_options";
/// An integer field that is not writable: only a type.
const INTEGER_READ_ONLY: &str = "pump_max_speed";

async fn leakshield(rig: &Rig) -> LeakshieldProxy<'static> {
    LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

async fn value(proxy: &LeakshieldProxy<'_>, id: &str) -> OwnedVariant {
    let rows = proxy.get_fields().await.unwrap();
    rows.into_iter().find(|r| r.0 == id).unwrap().2
}

fn signature(value: &OwnedVariant) -> String {
    value.value_signature().to_string()
}

fn set(
    id: &'static str,
    value: Variant<'static>,
) -> std::collections::HashMap<&'static str, Variant<'static>> {
    [(id, value)].into()
}

async fn refused(proxy: &LeakshieldProxy<'_>, id: &'static str, value: Variant<'static>) {
    let error = proxy.set_fields(set(id, value)).await.unwrap_err();
    assert_eq!(error_name(&error), format!("{ERR}.InvalidArgument"), "{id}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn get_fields_types_follow_the_registry_kind() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let registry = Registry::leakshield();
    let report3 = rig.sim.report3();

    // An enum shows the name of its code, one element or a list.
    let brightness = value(&proxy, ENUM).await;
    assert_eq!(signature(&brightness), "s");
    let name = registry
        .field(ENUM)
        .unwrap()
        .name_of(i64::from(report3[ENUM_OFFSET]))
        .unwrap();
    assert_eq!(String::try_from(brightness).unwrap(), name);
    let templates = value(&proxy, ENUM_LIST).await;
    assert_eq!(signature(&templates), "as");
    let templates = Vec::<String>::try_from(templates).unwrap();
    let field = registry.field(ENUM_LIST).unwrap();
    let expected: Vec<&str> = (0..10)
        .map(|i| {
            field
                .name_of(i64::from(report3[PAGE_TEMPLATE_OFFSET + i * PAGE_STRIDE]))
                .unwrap()
        })
        .collect();
    assert_eq!(templates, expected);
    // The unit of a device sensor and the pump model are enums too.
    assert_eq!(signature(&value(&proxy, "sensor_unit").await), "as");
    assert_eq!(signature(&value(&proxy, "pump_model").await), "s");

    // A scaled field is `d`: the raw value times the scale.
    let scaled = value(&proxy, SCALED).await;
    assert_eq!(signature(&scaled), "d");
    let raw = u16::from_be_bytes([report3[63], report3[64]]);
    assert!((f64::try_from(scaled).unwrap() - f64::from(raw) * 0.1).abs() < 1e-9);

    // Every other field is `x`: limits, type only, bit masks.
    let flags = value(&proxy, INTEGER).await;
    assert_eq!(signature(&flags), "x");
    assert_eq!(
        i64::try_from(flags).unwrap(),
        i64::from(report3[INTEGER_OFFSET])
    );
    assert_eq!(signature(&value(&proxy, INTEGER_READ_ONLY).await), "x");
    assert_eq!(signature(&value(&proxy, "device_profile").await), "x");
    let options = value(&proxy, INTEGER_LIST).await;
    assert_eq!(signature(&options), "ax");
    assert_eq!(Vec::<i64>::try_from(options).unwrap().len(), 10);

    // No field is a flag or a text, and no scaled field has several elements.
    for row in proxy.get_fields().await.unwrap() {
        let kind = registry.field(&row.0).unwrap();
        let wanted = match (&kind.names, kind.scale, kind.element_offsets().len() > 1) {
            (Some(_), _, false) => "s",
            (Some(_), _, true) => "as",
            (None, Some(_), false) => "d",
            (None, Some(_), true) => "ad",
            (None, None, false) => "x",
            (None, None, true) => "ax",
        };
        assert_eq!(signature(&row.2), wanted, "{}", row.0);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_code_that_is_not_listed_reads_as_unknown_code() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    rig.sim.change_report3(ENUM_OFFSET, &[9]);
    rig.sim.change_report3(PAGE_TEMPLATE_OFFSET, &[99]);

    let brightness = String::try_from(value(&proxy, ENUM).await).unwrap();
    assert_eq!(brightness, "unknown-9");
    let templates = Vec::<String>::try_from(value(&proxy, ENUM_LIST).await).unwrap();
    assert_eq!(templates[0], "unknown-99");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_fields_with_a_name_writes_the_code() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;

    proxy.set_fields(set(ENUM, "low".into())).await.unwrap();
    assert_eq!(rig.sim.report3()[ENUM_OFFSET], 2);
    assert_eq!(String::try_from(value(&proxy, ENUM).await).unwrap(), "low");

    // A list of names, one for each element.
    let mut names = Vec::<String>::try_from(value(&proxy, ENUM_LIST).await).unwrap();
    names[0] = "time".to_owned();
    names[9] = "none".to_owned();
    proxy
        .set_fields(set(ENUM_LIST, Variant::from(names.clone())))
        .await
        .unwrap();
    let report3 = rig.sim.report3();
    assert_eq!(report3[PAGE_TEMPLATE_OFFSET], 7);
    assert_eq!(report3[PAGE_TEMPLATE_OFFSET + 9 * PAGE_STRIDE], 255);
    let read_back = Vec::<String>::try_from(value(&proxy, ENUM_LIST).await).unwrap();
    assert_eq!(read_back, names);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_fields_takes_every_integer_type_for_an_integer_field() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;

    for (variant, raw) in [
        (Variant::U8(1), 1),
        (Variant::I16(3), 3),
        (Variant::U16(5), 5),
        (Variant::I32(7), 7),
        (Variant::U32(9), 9),
        (Variant::I64(11), 11),
        (Variant::U64(13), 13),
        // The app sends `d`: an integral value is fine.
        (Variant::F64(15.0), 15),
    ] {
        proxy.set_fields(set(INTEGER, variant)).await.unwrap();
        assert_eq!(rig.sim.report3()[INTEGER_OFFSET], raw);
        assert_eq!(
            i64::try_from(value(&proxy, INTEGER).await).unwrap(),
            i64::from(raw)
        );
    }
    // A list of integers, in `ax`.
    proxy
        .set_fields(set(INTEGER_LIST, Variant::from(vec![1_i64; 10])))
        .await
        .unwrap();
    assert_eq!(
        Vec::<i64>::try_from(value(&proxy, INTEGER_LIST).await).unwrap(),
        vec![1; 10]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_fields_takes_an_integer_for_a_scaled_field() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;

    proxy
        .set_fields(set(SCALED, Variant::I64(30)))
        .await
        .unwrap();
    assert_eq!(&rig.sim.report3()[63..65], &300u16.to_be_bytes());
    proxy
        .set_fields(set(SCALED, Variant::F64(31.5)))
        .await
        .unwrap();
    assert_eq!(&rig.sim.report3()[63..65], &315u16.to_be_bytes());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_fields_refuses_a_value_of_the_wrong_kind() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let before = rig.sim.report3();

    // An enum takes the names of its registry entry only.
    refused(&proxy, ENUM, "nope".into()).await;
    refused(&proxy, ENUM, "unknown-2".into()).await;
    refused(&proxy, ENUM, "unknown-9".into()).await;
    refused(&proxy, ENUM, Variant::F64(2.0)).await;
    refused(&proxy, ENUM, Variant::I64(2)).await;
    refused(&proxy, ENUM, Variant::Bool(true)).await;
    refused(&proxy, ENUM_LIST, Variant::from(vec![1.0_f64; 10])).await;
    let mut names = vec!["logo".to_owned(); 10];
    names[3] = "unknown-3".to_owned();
    refused(&proxy, ENUM_LIST, Variant::from(names)).await;
    // A fraction for an integer field, and a name for a scaled or an integer field.
    refused(&proxy, INTEGER, Variant::F64(1.5)).await;
    refused(&proxy, INTEGER_LIST, Variant::from(vec![0.5_f64; 10])).await;
    refused(&proxy, INTEGER, "none".into()).await;
    refused(&proxy, SCALED, "fast".into()).await;
    refused(&proxy, SCALED, Variant::Bool(true)).await;
    refused(&proxy, SCALED, Variant::F64(f64::NAN)).await;
    // A mixed array.
    refused(
        &proxy,
        ENUM_LIST,
        Variant::from(vec![Variant::from("logo"), Variant::from(1.0_f64)]),
    )
    .await;

    assert_eq!(rig.sim.report3(), before);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_scalar_for_several_elements_and_a_list_for_one_element_keep_their_errors() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let before = rig.sim.report3();

    // A scalar for a field with 10 elements, and a list of 2 for a field with 1 element: the
    // write path refuses the length, as it did before the field types.
    for (id, value) in [
        (ENUM_LIST, Variant::from("logo")),
        (INTEGER_LIST, Variant::I64(1)),
        (
            ENUM,
            Variant::from(vec!["low".to_owned(), "low".to_owned()]),
        ),
        (INTEGER, Variant::from(vec![1_i64, 1])),
    ] {
        refused(&proxy, id, value).await;
    }
    assert_eq!(rig.sim.report3(), before);

    // A list of one element is a value for a field with one element.
    proxy
        .set_fields(set(ENUM, Variant::from(vec!["off".to_owned()])))
        .await
        .unwrap();
    assert_eq!(rig.sim.report3()[ENUM_OFFSET], 3);
}
