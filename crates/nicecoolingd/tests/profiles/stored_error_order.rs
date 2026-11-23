//! The order of the errors of `SetStoredFields` (design D18): slot range, missing slot, uncovered
//! firmware, active slot, `device_profile`, then the rules of each field.

use std::collections::HashMap;

use nicecooling_api::ids::Kind;
use nicecooling_api::leakshield::LeakshieldProxy;
use zbus::Variant;

use crate::support::{LEAKSHIELD, Rig, error_name, path};
use crate::{ERR, slot_file};

const WRITABLE: &str = "display_page_time";

async fn leakshield(rig: &Rig) -> LeakshieldProxy<'static> {
    LeakshieldProxy::builder(&rig.client)
        .path(path(Kind::Leakshield, LEAKSHIELD))
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
        .unwrap()
}

fn error_of<T: std::fmt::Debug>(result: zbus::Result<T>) -> String {
    error_name(&result.expect_err("the call must fail"))
}

fn unknown_field() -> HashMap<&'static str, Variant<'static>> {
    [("no_such_field", Variant::F64(1.0))].into()
}

/// Design D18 orders the errors: slot range, missing slot, uncovered firmware, active slot,
/// `device_profile`, then the per field rules. The field ids are decoded before the job starts.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn slot_out_of_range_wins_over_field_error() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;

    let error = error_of(proxy.set_stored_fields(9, unknown_field()).await);

    assert_eq!(error, format!("{ERR}.InvalidArgument"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn missing_slot_wins_over_field_error() {
    let rig = Rig::start().await;
    std::fs::remove_dir_all(slot_file(&rig, 3).parent().unwrap()).unwrap();
    let proxy = leakshield(&rig).await;

    let error = error_of(
        proxy
            .set_stored_fields(3, [(WRITABLE, Variant::from("not-a-number"))].into())
            .await,
    );

    assert_eq!(error, format!("{ERR}.NotFound"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn active_slot_wins_over_field_error() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    assert_eq!(proxy.active_slot().await.unwrap(), 1);

    let error = error_of(proxy.set_stored_fields(1, unknown_field()).await);

    assert_eq!(error, format!("{ERR}.InvalidArgument"));
}

/// An uncovered firmware wins over the active slot and over every field rule.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn uncovered_firmware_wins_over_field_error() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let file = slot_file(&rig, 3).with_file_name("meta.json");
    let mut meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    meta["firmware"] = 1019.into();
    std::fs::write(file, serde_json::to_vec(&meta).unwrap()).unwrap();

    let error = error_of(proxy.set_stored_fields(3, unknown_field()).await);

    assert_eq!(error, "org.freedesktop.DBus.Error.NotSupported");
}

/// `device_profile` wins over a field that does not exist, and a field rule comes after both.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn device_profile_wins_over_other_field_errors() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;

    let error = error_of(
        proxy
            .set_stored_fields(
                3,
                [
                    ("device_profile", Variant::I64(4)),
                    ("no_such_field", Variant::F64(1.0)),
                ]
                .into(),
            )
            .await,
    );
    assert_eq!(error, format!("{ERR}.InvalidArgument"));

    // Without `device_profile`, an unknown field is `NotFound` and a bad value is
    // `InvalidArgument`.
    assert_eq!(
        error_of(proxy.set_stored_fields(3, unknown_field()).await),
        format!("{ERR}.NotFound")
    );
    assert_eq!(
        error_of(
            proxy
                .set_stored_fields(3, [(WRITABLE, Variant::from("not-a-number"))].into())
                .await
        ),
        format!("{ERR}.InvalidArgument")
    );
}

/// A value that is not a number, a name or a list is an error of its field, after the slot checks.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn undecodable_value_comes_after_the_slot_checks() {
    let rig = Rig::start().await;
    let proxy = leakshield(&rig).await;
    let bad: HashMap<&str, Variant<'static>> = [(WRITABLE, Variant::Bool(true))].into();

    assert_eq!(
        error_of(proxy.set_stored_fields(9, bad.clone()).await),
        format!("{ERR}.InvalidArgument")
    );
    std::fs::remove_dir_all(slot_file(&rig, 3).parent().unwrap()).unwrap();
    assert_eq!(
        error_of(proxy.set_stored_fields(3, bad.clone()).await),
        format!("{ERR}.NotFound")
    );
    assert_eq!(
        error_of(proxy.set_stored_fields(2, bad).await),
        format!("{ERR}.InvalidArgument")
    );
}
