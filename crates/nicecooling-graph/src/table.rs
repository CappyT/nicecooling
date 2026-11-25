//! The table node: a list of pairs of an input and an output, read with the interpolation
//! function that the Curve shares.

use nicecooling_core::interpolate::{Interpolation, interpolate};

use crate::model::{DEFAULT_PAIRS, MAX_PAIRS, MIN_PAIRS, Node};

/// A table with its pairs sorted by input.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    mode: Interpolation,
    pairs: Vec<(f64, f64)>,
}

impl Table {
    /// A table from pairs in any order. The number of pairs is 2 to 64, and no two pairs have the
    /// same input. Anything else gives `None`.
    pub fn new(mode: Interpolation, mut pairs: Vec<(f64, f64)>) -> Option<Self> {
        if !(MIN_PAIRS..=MAX_PAIRS).contains(&pairs.len())
            || pairs.iter().any(|p| !p.0.is_finite() || !p.1.is_finite())
        {
            return None;
        }
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
        if pairs.windows(2).any(|w| w[0].0 == w[1].0) {
            return None;
        }
        Some(Self { mode, pairs })
    }

    /// The table of a table node, with the stored pairs and mode or their defaults.
    pub fn from_node(node: &Node) -> Option<Self> {
        let mode = match node.text("mode").as_str() {
            "step" => Interpolation::Step,
            _ => Interpolation::Linear,
        };
        Self::new(mode, node.pairs())
    }

    pub fn pairs(&self) -> &[(f64, f64)] {
        &self.pairs
    }

    /// The output for `a`. An input that is not a number gives no value.
    pub fn output(&self, a: f64) -> Option<f64> {
        interpolate(&self.pairs, self.mode, a)
    }
}

impl Default for Table {
    fn default() -> Self {
        Self {
            mode: Interpolation::Linear,
            pairs: DEFAULT_PAIRS.to_vec(),
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::model::Kind;

    fn table(mode: Interpolation) -> Table {
        Table::new(mode, vec![(20.0, 30.0), (40.0, 80.0)]).unwrap()
    }

    #[test]
    fn table_linear_between_pairs() {
        assert_eq!(table(Interpolation::Linear).output(30.0), Some(55.0));
        let three = Table::new(
            Interpolation::Linear,
            vec![(0.0, 0.0), (10.0, 100.0), (20.0, 0.0)],
        )
        .unwrap();
        assert_eq!(three.output(5.0), Some(50.0));
        assert_eq!(three.output(15.0), Some(50.0));
    }

    #[test]
    fn table_step_lower_pair() {
        assert_eq!(table(Interpolation::Step).output(39.9), Some(30.0));
        assert_eq!(table(Interpolation::Step).output(20.0), Some(30.0));
        assert_eq!(table(Interpolation::Step).output(40.0), Some(80.0));
    }

    #[test]
    fn table_clamps_both_ends() {
        for mode in [Interpolation::Linear, Interpolation::Step] {
            assert_eq!(table(mode).output(90.0), Some(80.0));
            assert_eq!(table(mode).output(-5.0), Some(30.0));
        }
    }

    #[test]
    fn table_nan_input_gives_no_value() {
        assert_eq!(table(Interpolation::Linear).output(f64::NAN), None);
        assert_eq!(table(Interpolation::Step).output(f64::NAN), None);
    }

    #[test]
    fn table_pair_count_2_to_64() {
        let pairs = |n: usize| (0..n).map(|i| (i as f64, i as f64)).collect::<Vec<_>>();
        assert!(Table::new(Interpolation::Linear, pairs(1)).is_none());
        assert!(Table::new(Interpolation::Linear, pairs(2)).is_some());
        assert!(Table::new(Interpolation::Linear, pairs(64)).is_some());
        assert!(Table::new(Interpolation::Linear, pairs(65)).is_none());
        // The pairs are sorted by input, and two pairs with one input are refused.
        let sorted = Table::new(Interpolation::Linear, vec![(40.0, 80.0), (20.0, 30.0)]).unwrap();
        assert_eq!(sorted.pairs(), [(20.0, 30.0), (40.0, 80.0)]);
        assert!(Table::new(Interpolation::Linear, vec![(20.0, 30.0), (20.0, 80.0)]).is_none());
        assert!(Table::new(Interpolation::Linear, vec![(f64::NAN, 1.0), (2.0, 3.0)]).is_none());
    }

    #[test]
    fn table_default_pairs() {
        let node = Node::new(1, Kind::Table);
        let table = Table::from_node(&node).unwrap();
        assert_eq!(table.pairs(), [(-100.0, -100.0), (100.0, 100.0)]);
        assert_eq!(table, Table::default());
        assert_eq!(table.output(50.0), Some(50.0));
        // The mode comes from the node.
        let step = Node::new(1, Kind::Table)
            .with_param("mode", "step")
            .with_param("pairs", json!([[20, 30], [40, 80]]));
        assert_eq!(Table::from_node(&step).unwrap().output(39.9), Some(30.0));
    }
}
