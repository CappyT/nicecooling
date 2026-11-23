//! The fields of report 3 as `GetFields` and `SetFields` show them: engineering values (the raw
//! value times the scale of the registry), and a list for a field with several elements.

use nicecooling_protocol::access::{Access, ReadOnlyReason};
use nicecooling_protocol::registry::{Field, Registry, RiskClass};
use nicecooling_protocol::report1::Report1;
use nicecooling_protocol::report3::Report3;

use super::write::FieldEdit;

#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    Number(f64),
    List(Vec<f64>),
}

/// One row of `GetFields`.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldRow {
    pub id: String,
    /// `safety` or `low`.
    pub risk: &'static str,
    pub value: FieldValue,
    pub writable: bool,
    /// Empty for a writable field.
    pub reason: String,
}

fn scale_of(field: &Field) -> f64 {
    field.scale.unwrap_or(1.0)
}

/// The rows of every registry field, in registry order. A field that the device may not take
/// has the reason of the device (`unknown-firmware`, `unknown-structure`) or `not-verified`.
pub fn rows(registry: &Registry, report1: &Report1, report3: &Report3) -> Vec<FieldRow> {
    let access = Access::new(registry, report1);
    let device_reason: Option<ReadOnlyReason> = access.device_reason();
    registry
        .field
        .iter()
        .map(|field| {
            let values: Vec<f64> = field
                .element_offsets()
                .into_iter()
                .map(|o| field.read_raw(report3, o) as f64 * scale_of(field))
                .collect();
            let value = match values.as_slice() {
                [one] => FieldValue::Number(*one),
                _ => FieldValue::List(values),
            };
            let writable = access.check_write(&field.id).is_ok();
            let reason = match (writable, device_reason) {
                (true, _) => "",
                (false, Some(reason)) => reason.as_str(),
                (false, None) => "not-verified",
            };
            FieldRow {
                id: field.id.clone(),
                risk: match field.risk_class {
                    RiskClass::Safety => "safety",
                    RiskClass::Low => "low",
                },
                value,
                writable,
                reason: reason.to_owned(),
            }
        })
        .collect()
}

/// The value of every registry field in `report3`, in registry order.
pub fn values(registry: &Registry, report3: &Report3) -> Vec<(String, FieldValue)> {
    registry
        .field
        .iter()
        .map(|field| {
            let values: Vec<f64> = field
                .element_offsets()
                .into_iter()
                .map(|o| field.read_raw(report3, o) as f64 * scale_of(field))
                .collect();
            let value = match values.as_slice() {
                [one] => FieldValue::Number(*one),
                _ => FieldValue::List(values),
            };
            (field.id.clone(), value)
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum EditError {
    NoSuchField(String),
    /// The value is not a finite number that fits the raw type.
    BadValue(String),
}

/// Raw values of the edits. The write path checks the access and the range.
pub fn edits(
    registry: &Registry,
    values: Vec<(String, FieldValue)>,
) -> Result<Vec<FieldEdit>, EditError> {
    values
        .into_iter()
        .map(|(id, value)| {
            let field = registry
                .field(&id)
                .ok_or_else(|| EditError::NoSuchField(id.clone()))?;
            let numbers = match value {
                FieldValue::Number(n) => vec![n],
                FieldValue::List(list) => list,
            };
            let raw = numbers
                .into_iter()
                .map(|n| {
                    let raw = (n / scale_of(field)).round();
                    (raw.is_finite() && raw.abs() < 1e9)
                        .then_some(raw as i64)
                        .ok_or_else(|| EditError::BadValue(id.clone()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(FieldEdit { field: id, raw })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_applies_both_ways() {
        let registry = Registry::leakshield();
        // `display_page_time` has the scale 0.1 s.
        let edits = edits(
            &registry,
            vec![("display_page_time".into(), FieldValue::Number(30.0))],
        )
        .unwrap();
        assert_eq!(edits, [FieldEdit::scalar("display_page_time", 300)]);
    }

    #[test]
    fn bad_values_are_named() {
        let registry = Registry::leakshield();
        assert_eq!(
            edits(&registry, vec![("nope".into(), FieldValue::Number(1.0))]),
            Err(EditError::NoSuchField("nope".into()))
        );
        assert_eq!(
            edits(
                &registry,
                vec![("display_page_time".into(), FieldValue::Number(f64::NAN))]
            ),
            Err(EditError::BadValue("display_page_time".into()))
        );
    }
}
