//! The fields of report 3 as `GetFields` and `SetFields` show them (`docs/api.md`, Leakshield
//! fields). The registry gives the kind of a field: an enum field shows the name of its code, a
//! scaled field shows engineering values (the raw value times the scale), and any other field
//! shows the raw integer. A field with several elements shows a list.

use nicecooling_protocol::access::{Access, ReadOnlyReason};
use nicecooling_protocol::registry::{Field, Registry, RiskClass};
use nicecooling_protocol::report1::Report1;
use nicecooling_protocol::report3::Report3;

use super::write::FieldEdit;

#[derive(Debug, Clone, PartialEq)]
pub enum FieldValue {
    /// A scaled field.
    Number(f64),
    /// A field without names and without a scale: limits, bit masks, type only.
    Integer(i64),
    /// An enum field.
    Name(String),
    Numbers(Vec<f64>),
    Integers(Vec<i64>),
    Names(Vec<String>),
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

enum Kind {
    Enum,
    Scaled,
    Integer,
}

fn kind_of(field: &Field) -> Kind {
    if field.names.is_some() {
        Kind::Enum
    } else if field.scale.is_some() {
        Kind::Scaled
    } else {
        Kind::Integer
    }
}

fn one_or_many<T>(
    mut items: Vec<T>,
    one: fn(T) -> FieldValue,
    many: fn(Vec<T>) -> FieldValue,
) -> FieldValue {
    if items.len() == 1 {
        one(items.remove(0))
    } else {
        many(items)
    }
}

/// The value of `field` in `report3`. A code that the registry does not list reads as
/// `unknown-<code>`.
fn read_value(field: &Field, report3: &Report3) -> FieldValue {
    let raw = field
        .element_offsets()
        .into_iter()
        .map(|o| field.read_raw(report3, o));
    match kind_of(field) {
        Kind::Enum => one_or_many(
            raw.map(|code| {
                field
                    .name_of(code)
                    .map_or_else(|| format!("unknown-{code}"), str::to_owned)
            })
            .collect(),
            FieldValue::Name,
            FieldValue::Names,
        ),
        Kind::Scaled => one_or_many(
            raw.map(|code| code as f64 * scale_of(field)).collect(),
            FieldValue::Number,
            FieldValue::Numbers,
        ),
        Kind::Integer => one_or_many(raw.collect(), FieldValue::Integer, FieldValue::Integers),
    }
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
                value: read_value(field, report3),
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
        .map(|field| (field.id.clone(), read_value(field, report3)))
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
pub enum EditError {
    NoSuchField(String),
    /// The value does not fit the kind of the field, or is not finite, or does not fit the raw
    /// type. `repeat_edits` also uses it for a repeat index that the field lacks.
    BadValue(String),
}

/// The raw value of a scaled number. `None` when it is not finite or too large.
fn raw_of_number(number: f64, scale: f64) -> Option<i64> {
    let raw = (number / scale).round();
    (raw.is_finite() && raw.abs() < 1e9).then_some(raw as i64)
}

/// The raw value of a number for a field without a scale. `None` for a fraction.
fn raw_of_whole(number: f64) -> Option<i64> {
    (number.is_finite() && number.fract() == 0.0 && number.abs() < 1e9).then_some(number as i64)
}

/// The raw values of `value` for `field`. `None` when the value does not fit the kind of the
/// field. A scalar gives one raw value, a list gives one for each element.
fn raw_values(field: &Field, value: FieldValue) -> Option<Vec<i64>> {
    use FieldValue::*;
    match (kind_of(field), value) {
        (Kind::Enum, Name(name)) => Some(vec![field.code_of(&name)?]),
        (Kind::Enum, Names(names)) => names.iter().map(|n| field.code_of(n)).collect(),
        (Kind::Scaled, value) => {
            let numbers = match value {
                Number(n) => vec![n],
                Numbers(list) => list,
                Integer(i) => vec![i as f64],
                Integers(list) => list.into_iter().map(|i| i as f64).collect(),
                Name(_) | Names(_) => return None,
            };
            let scale = scale_of(field);
            numbers
                .into_iter()
                .map(|n| raw_of_number(n, scale))
                .collect()
        }
        (Kind::Integer, Integer(i)) => Some(vec![i]),
        (Kind::Integer, Integers(list)) => Some(list),
        (Kind::Integer, Number(n)) => Some(vec![raw_of_whole(n)?]),
        (Kind::Integer, Numbers(list)) => list.into_iter().map(raw_of_whole).collect(),
        _ => None,
    }
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
            let raw = raw_values(field, value).ok_or_else(|| EditError::BadValue(id.clone()))?;
            Ok(FieldEdit { field: id, raw })
        })
        .collect()
}

/// Edits that set repeat `index` of each field in `ids` to `raw_of(id)`. A field with several
/// repeats (a page, an LED controller) keeps the other repeats as `report3` holds them. A field
/// whose repeat already has the wanted values gives no edit, so the write path never checks a
/// field that the caller left alone.
pub fn repeat_edits(
    registry: &Registry,
    report3: &Report3,
    index: usize,
    ids: &[&str],
    raw_of: impl Fn(&str) -> Option<Vec<i64>>,
) -> Result<Vec<FieldEdit>, EditError> {
    let mut edits = Vec::new();
    for &id in ids {
        let field = registry
            .field(id)
            .ok_or_else(|| EditError::NoSuchField(id.to_owned()))?;
        if index >= field.count.unwrap_or(1) {
            return Err(EditError::BadValue(id.to_owned()));
        }
        let wanted = raw_of(id).ok_or_else(|| EditError::NoSuchField(id.to_owned()))?;
        let len = field.repeat_len();
        if wanted.len() != len {
            return Err(EditError::BadValue(id.to_owned()));
        }
        let mut raw: Vec<i64> = field
            .element_offsets()
            .into_iter()
            .map(|o| field.read_raw(report3, o))
            .collect();
        if raw[index * len..(index + 1) * len] != wanted[..] {
            raw[index * len..(index + 1) * len].copy_from_slice(&wanted);
            edits.push(FieldEdit {
                field: id.to_owned(),
                raw,
            });
        }
    }
    Ok(edits)
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

    fn raw(id: &str, value: FieldValue) -> Result<Vec<i64>, EditError> {
        let registry = Registry::leakshield();
        edits(&registry, vec![(id.into(), value)]).map(|mut e| e.remove(0).raw)
    }

    fn bad(id: &str) -> Result<Vec<i64>, EditError> {
        Err(EditError::BadValue(id.into()))
    }

    #[test]
    fn an_enum_takes_its_names_only() {
        use FieldValue::*;
        assert_eq!(raw("default_mode", Name("release".into())), Ok(vec![3]));
        assert_eq!(raw("pump_model", Name("custom".into())), Ok(vec![255]));
        assert_eq!(
            raw("page_template", Names(vec!["time".into(), "none".into()])),
            Ok(vec![7, 255])
        );
        for wrong in [
            Name("nope".into()),
            Name("unknown-2".into()),
            Name("unknown-9".into()),
            Name("SHIELD".into()),
            Number(1.0),
            Integer(1),
            Numbers(vec![1.0]),
        ] {
            assert_eq!(raw("default_mode", wrong), bad("default_mode"));
        }
        assert_eq!(
            raw("page_template", Names(vec!["time".into(), "logo-2".into()])),
            bad("page_template")
        );
    }

    #[test]
    fn a_scaled_field_takes_a_number_or_an_integer() {
        use FieldValue::*;
        assert_eq!(raw("pump_max_pressure", Number(36.5)), Ok(vec![365]));
        assert_eq!(raw("pump_max_pressure", Integer(36)), Ok(vec![360]));
        assert_eq!(
            raw("pump_max_pressure", Name("x".into())),
            bad("pump_max_pressure")
        );
        assert_eq!(raw("display_page_time", Numbers(vec![1.0])), Ok(vec![10]));
        assert_eq!(
            raw("display_page_time", Integers(vec![1, 2])),
            Ok(vec![10, 20])
        );
    }

    #[test]
    fn an_integer_field_takes_a_whole_number() {
        use FieldValue::*;
        assert_eq!(raw("pump_max_speed", Integer(4800)), Ok(vec![4800]));
        assert_eq!(raw("pump_max_speed", Number(4800.0)), Ok(vec![4800]));
        assert_eq!(raw("pump_max_speed", Number(4800.5)), bad("pump_max_speed"));
        assert_eq!(
            raw("pump_max_speed", Number(f64::INFINITY)),
            bad("pump_max_speed")
        );
        assert_eq!(
            raw("pump_max_speed", Name("fast".into())),
            bad("pump_max_speed")
        );
        // A bit mask is an integer too.
        assert_eq!(raw("pump_flags", Integer(3)), Ok(vec![3]));
        assert_eq!(raw("pump_flags", Number(1.5)), bad("pump_flags"));
        assert_eq!(raw("device_profile", Integer(2)), Ok(vec![2]));
    }

    #[test]
    fn a_scalar_for_several_elements_and_a_list_for_one_element_pass_through() {
        use FieldValue::*;
        // The edit keeps the length that the caller gave: the write path refuses a wrong one.
        assert_eq!(raw("page_template", Name("logo".into())), Ok(vec![0]));
        assert_eq!(
            raw("default_mode", Names(vec!["shield".into()])),
            Ok(vec![1])
        );
        assert_eq!(
            raw(
                "default_mode",
                Names(vec!["shield".into(), "monitor".into()])
            ),
            Ok(vec![1, 2])
        );
        assert_eq!(raw("pump_flags", Integers(vec![1, 2])), Ok(vec![1, 2]));
    }

    #[test]
    fn a_field_reads_by_its_kind() {
        use nicecooling_protocol::sim::SimDevice;
        let report3 = Report3::decode(&SimDevice::fw1022().report3()).unwrap();
        let mut registry = Registry::leakshield();
        let read =
            |registry: &Registry, id: &str| read_value(registry.field(id).unwrap(), &report3);
        assert!(matches!(
            read(&registry, "default_mode"),
            FieldValue::Name(_)
        ));
        assert!(matches!(read(&registry, "page_template"), FieldValue::Names(n) if n.len() == 10));
        assert!(matches!(
            read(&registry, "display_page_time"),
            FieldValue::Number(_)
        ));
        assert!(matches!(
            read(&registry, "pump_max_speed"),
            FieldValue::Integer(_)
        ));
        assert!(
            matches!(read(&registry, "page_options"), FieldValue::Integers(n) if n.len() == 10)
        );
        // No registry field is scaled and repeated, so make one: two copies of a scaled field.
        let field = registry
            .field
            .iter_mut()
            .find(|f| f.id == "display_page_time")
            .unwrap();
        field.count = Some(2);
        field.stride = Some(2);
        assert!(
            matches!(read(&registry, "display_page_time"), FieldValue::Numbers(n) if n.len() == 2)
        );
    }
}
