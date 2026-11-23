//! The D-Bus form of a Leakshield field value (`docs/api.md`, Leakshield fields). `GetFields`,
//! `SetFields` and the Leakshield fields of a Profile all use it.
//!
//! Out: a number is `d`, an integer is `x`, a name is `s`, and a list is `ad`, `ax` or `as`.
//! In: `d` is a number, any integer type is an integer, `s` is a name, and an array follows its
//! element type. `b` is refused.

use zbus::{OwnedVariant, Variant};

use crate::leakshield::fields::FieldValue;

pub fn variant_of(value: &FieldValue) -> OwnedVariant {
    let variant = match value {
        FieldValue::Number(n) => Variant::F64(*n),
        FieldValue::Integer(i) => Variant::I64(*i),
        FieldValue::Name(name) => Variant::from(name.clone()),
        FieldValue::Numbers(list) => Variant::from(list.clone()),
        FieldValue::Integers(list) => Variant::from(list.clone()),
        FieldValue::Names(list) => Variant::from(list.clone()),
    };
    OwnedVariant::try_from(variant).expect("a number or a text has no file descriptor")
}

fn scalar(value: &Variant<'_>) -> Option<FieldValue> {
    Some(match value {
        Variant::F64(v) => FieldValue::Number(*v),
        Variant::U8(v) => FieldValue::Integer(i64::from(*v)),
        Variant::I16(v) => FieldValue::Integer(i64::from(*v)),
        Variant::U16(v) => FieldValue::Integer(i64::from(*v)),
        Variant::I32(v) => FieldValue::Integer(i64::from(*v)),
        Variant::U32(v) => FieldValue::Integer(i64::from(*v)),
        Variant::I64(v) => FieldValue::Integer(*v),
        Variant::U64(v) => FieldValue::Integer(i64::try_from(*v).ok()?),
        Variant::Str(v) => FieldValue::Name(v.to_string()),
        Variant::Variant(inner) => return scalar(inner),
        _ => return None,
    })
}

/// `None` for a type that no field takes: `b`, a structure, a dictionary, or an array of mixed
/// elements.
pub fn field_value(value: &Variant<'_>) -> Option<FieldValue> {
    if let Variant::Variant(inner) = value {
        return field_value(inner);
    }
    let Variant::Array(array) = value else {
        return scalar(value);
    };
    let items: Vec<FieldValue> = array.iter().map(scalar).collect::<Option<_>>()?;
    let list = match items.first() {
        Some(FieldValue::Number(_)) => FieldValue::Numbers(
            items
                .iter()
                .map(|v| match v {
                    FieldValue::Number(n) => Some(*n),
                    _ => None,
                })
                .collect::<Option<_>>()?,
        ),
        Some(FieldValue::Integer(_)) => FieldValue::Integers(
            items
                .iter()
                .map(|v| match v {
                    FieldValue::Integer(i) => Some(*i),
                    _ => None,
                })
                .collect::<Option<_>>()?,
        ),
        Some(FieldValue::Name(_)) => FieldValue::Names(
            items
                .iter()
                .map(|v| match v {
                    FieldValue::Name(n) => Some(n.clone()),
                    _ => None,
                })
                .collect::<Option<_>>()?,
        ),
        // An empty array follows its element type.
        None => match array.element_signature() {
            zbus::Signature::F64 => FieldValue::Numbers(Vec::new()),
            zbus::Signature::Str => FieldValue::Names(Vec::new()),
            zbus::Signature::U8
            | zbus::Signature::I16
            | zbus::Signature::U16
            | zbus::Signature::I32
            | zbus::Signature::U32
            | zbus::Signature::I64
            | zbus::Signature::U64 => FieldValue::Integers(Vec::new()),
            _ => return None,
        },
        Some(_) => return None,
    };
    Some(list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode(value: &OwnedVariant) -> Option<FieldValue> {
        field_value(value)
    }

    #[test]
    fn every_value_comes_back_as_it_went_out() {
        for value in [
            FieldValue::Number(2.5),
            FieldValue::Integer(-3),
            FieldValue::Name("signal-long".into()),
            FieldValue::Numbers(vec![0.5, 1.0]),
            FieldValue::Integers(vec![1, 2, 3]),
            FieldValue::Names(vec!["a".into(), "kp-0".into()]),
        ] {
            assert_eq!(decode(&variant_of(&value)), Some(value));
        }
    }

    #[test]
    fn the_types_are_d_x_s_and_their_arrays() {
        let signature = |v: FieldValue| variant_of(&v).value_signature().to_string();
        assert_eq!(signature(FieldValue::Number(1.0)), "d");
        assert_eq!(signature(FieldValue::Integer(1)), "x");
        assert_eq!(signature(FieldValue::Name("a".into())), "s");
        assert_eq!(signature(FieldValue::Numbers(vec![1.0])), "ad");
        assert_eq!(signature(FieldValue::Integers(vec![1])), "ax");
        assert_eq!(signature(FieldValue::Names(vec!["a".into()])), "as");
    }

    #[test]
    fn any_integer_type_is_an_integer() {
        for variant in [
            Variant::U8(7),
            Variant::I16(7),
            Variant::U16(7),
            Variant::I32(7),
            Variant::U32(7),
            Variant::I64(7),
            Variant::U64(7),
        ] {
            assert_eq!(field_value(&variant), Some(FieldValue::Integer(7)));
        }
        assert_eq!(field_value(&Variant::U64(u64::MAX)), None);
        let array = Variant::from(vec![1_u8, 2]);
        assert_eq!(field_value(&array), Some(FieldValue::Integers(vec![1, 2])));
    }

    #[test]
    fn an_empty_array_follows_its_element_type() {
        assert_eq!(
            field_value(&Variant::from(Vec::<f64>::new())),
            Some(FieldValue::Numbers(vec![]))
        );
        assert_eq!(
            field_value(&Variant::from(Vec::<String>::new())),
            Some(FieldValue::Names(vec![]))
        );
        assert_eq!(
            field_value(&Variant::from(Vec::<u16>::new())),
            Some(FieldValue::Integers(vec![]))
        );
        assert_eq!(field_value(&Variant::from(Vec::<bool>::new())), None);
    }

    #[test]
    fn a_flag_and_a_mixed_array_are_refused() {
        assert_eq!(field_value(&Variant::Bool(true)), None);
        assert_eq!(field_value(&Variant::from(vec![true, false])), None);
        let mixed = Variant::from(vec![Variant::from(1.0_f64), Variant::from(1_i64)]);
        assert_eq!(field_value(&mixed), None);
    }

    #[test]
    fn an_array_of_variants_follows_the_inner_type() {
        let av = Variant::from(vec![Variant::from(1.0_f64), Variant::from(2.0_f64)]);
        assert_eq!(field_value(&av), Some(FieldValue::Numbers(vec![1.0, 2.0])));
    }

    #[test]
    fn a_variant_that_holds_an_array_is_a_list() {
        let inner = Variant::from(vec!["logo".to_owned()]);
        let nested = Variant::Variant(Box::new(inner));
        assert_eq!(
            field_value(&nested),
            Some(FieldValue::Names(vec!["logo".into()]))
        );
    }
}
