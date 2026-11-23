use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DerivedKind {
    Maximum,
    Mean,
    Difference,
}

/// A sensor that the daemon computes from other sensors (`docs/storage.md`, `derived_sensors`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DerivedSensor {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: DerivedKind,
    pub sources: Vec<String>,
}

/// The value of a Derived sensor. A missing source gives no value.
/// A difference needs two sources: the first minus the second.
pub fn compute(kind: DerivedKind, sources: &[Option<f64>]) -> Option<f64> {
    let values: Option<Vec<f64>> = sources.iter().copied().collect();
    let values = values?;
    match kind {
        DerivedKind::Maximum => values.iter().copied().reduce(f64::max),
        DerivedKind::Mean if values.is_empty() => None,
        DerivedKind::Mean => Some(values.iter().sum::<f64>() / values.len() as f64),
        DerivedKind::Difference => match values[..] {
            [a, b] => Some(a - b),
            _ => None,
        },
    }
}

impl DerivedSensor {
    pub fn value(&self, values: &BTreeMap<String, f64>) -> Option<f64> {
        let sources: Vec<_> = self
            .sources
            .iter()
            .map(|s| values.get(s).copied())
            .collect();
        compute(self.kind, &sources)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maximum() {
        assert_eq!(
            compute(DerivedKind::Maximum, &[Some(40.0), Some(55.0)]),
            Some(55.0)
        );
    }

    #[test]
    fn mean() {
        assert_eq!(
            compute(DerivedKind::Mean, &[Some(40.0), Some(55.0), Some(25.0)]),
            Some(40.0)
        );
    }

    #[test]
    fn difference() {
        assert_eq!(
            compute(DerivedKind::Difference, &[Some(40.0), Some(25.0)]),
            Some(15.0)
        );
        assert_eq!(compute(DerivedKind::Difference, &[Some(40.0)]), None);
    }

    #[test]
    fn missing_source_gives_no_value() {
        for kind in [
            DerivedKind::Maximum,
            DerivedKind::Mean,
            DerivedKind::Difference,
        ] {
            assert_eq!(compute(kind, &[Some(40.0), None]), None);
        }
        let sensor = DerivedSensor {
            id: "derived_1".into(),
            name: "Max".into(),
            kind: DerivedKind::Maximum,
            sources: vec!["a".into(), "b".into()],
        };
        let values = BTreeMap::from([("a".to_owned(), 40.0)]);
        assert_eq!(sensor.value(&values), None);
    }
}
