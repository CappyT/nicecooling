//! The editing rules of the Curve editor (`docs/ui/widgets.md`). The plot and the number
//! inputs edit the same list of points through these functions.

use nicecooling_core::interpolate::{Interpolation, interpolate};

pub const MIN_POINTS: usize = 2;
pub const MAX_POINTS: usize = 8;
/// A point snaps to 1 unit on x and 1 % on y.
const SNAP: f64 = 1.0;

fn snap(value: f64) -> f64 {
    (value / SNAP).round() * SNAP
}

/// The range of the plot: x is the Input in its unit, y is Duty from 0 to 100 %.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlotScale {
    pub x_min: f64,
    pub x_max: f64,
}

impl PlotScale {
    /// A range that holds the points and the live Input with a margin, in steps of 5 units.
    pub fn for_points(points: &[(f64, f64)], live_input: Option<f64>) -> Self {
        let xs = points.iter().map(|p| p.0).chain(live_input);
        let low = xs.clone().fold(f64::INFINITY, f64::min);
        let high = xs.fold(f64::NEG_INFINITY, f64::max);
        if !low.is_finite() {
            return Self {
                x_min: 20.0,
                x_max: 80.0,
            };
        }
        let mut x_min = (((low - 10.0) / 5.0).floor() * 5.0).max(0.0);
        let mut x_max = ((high + 10.0) / 5.0).ceil() * 5.0;
        if x_max - x_min < 40.0 {
            x_max = x_min + 40.0;
        }
        if x_min > low {
            x_min = low;
        }
        Self { x_min, x_max }
    }

    pub fn span(&self) -> f64 {
        self.x_max - self.x_min
    }
}

/// Move one point. It cannot pass its neighbors on x, and it cannot go under the Pump floor.
pub fn move_point(
    points: &mut [(f64, f64)],
    index: usize,
    x: f64,
    y: f64,
    floor: Option<f64>,
) -> bool {
    if index >= points.len() {
        return false;
    }
    let low = if index == 0 {
        f64::NEG_INFINITY
    } else {
        points[index - 1].0 + SNAP
    };
    let high = if index + 1 == points.len() {
        f64::INFINITY
    } else {
        points[index + 1].0 - SNAP
    };
    let x = snap(x).clamp(low, high.max(low));
    let y = snap(y).clamp(floor.unwrap_or(0.0).ceil(), 100.0);
    let changed = points[index] != (x, y);
    points[index] = (x, y);
    changed
}

/// Add a point on the line at `x` (a double click). Returns its index.
pub fn add_point_at(points: &mut Vec<(f64, f64)>, x: f64, floor: Option<f64>) -> Option<usize> {
    if points.len() >= MAX_POINTS {
        return None;
    }
    let x = snap(x);
    if points.iter().any(|p| p.0 == x) {
        return None;
    }
    let line = interpolate(points, Interpolation::Linear, x).unwrap_or(0.0);
    let y = snap(line).clamp(floor.unwrap_or(0.0).ceil(), 100.0);
    let index = points.partition_point(|p| p.0 < x);
    points.insert(index, (x, y));
    Some(index)
}

/// Add a point after the selected one (Insert, "Add point"): half way to the next point, or
/// 5 units past the last one.
pub fn add_point_after(
    points: &mut Vec<(f64, f64)>,
    selected: usize,
    floor: Option<f64>,
) -> Option<usize> {
    if points.len() >= MAX_POINTS || selected >= points.len() {
        return None;
    }
    let here = points[selected].0;
    let x = match points.get(selected + 1) {
        Some(next) if next.0 - here >= 2.0 => snap((here + next.0) / 2.0),
        Some(_) => return None,
        None => here + 5.0,
    };
    add_point_at(points, x, floor)
}

/// Remove a point. A Curve keeps 2 points at least.
pub fn remove_point(points: &mut Vec<(f64, f64)>, index: usize) -> bool {
    if points.len() <= MIN_POINTS || index >= points.len() {
        return false;
    }
    points.remove(index);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn curve() -> Vec<(f64, f64)> {
        vec![(30.0, 40.0), (45.0, 70.0), (60.0, 100.0)]
    }

    #[test]
    fn a_point_snaps_and_stops_at_its_neighbors() {
        let mut points = curve();
        move_point(&mut points, 1, 100.0, 70.4, None);
        assert_eq!(points[1], (59.0, 70.0));
        move_point(&mut points, 1, 0.0, 70.0, None);
        assert_eq!(points[1], (31.0, 70.0));
    }

    #[test]
    fn a_point_cannot_go_under_the_pump_floor() {
        let mut points = curve();
        move_point(&mut points, 0, 30.0, 10.0, Some(40.0));
        assert_eq!(points[0], (30.0, 40.0));
        move_point(&mut points, 2, 60.0, 150.0, Some(40.0));
        assert_eq!(points[2], (60.0, 100.0));
    }

    #[test]
    fn add_and_remove_keep_the_list_sorted_between_2_and_8_points() {
        let mut points = curve();
        assert_eq!(add_point_at(&mut points, 50.0, None), Some(2));
        assert_eq!(points[2], (50.0, 80.0));
        assert_eq!(
            add_point_at(&mut points, 50.0, None),
            None,
            "no second point at x 50"
        );
        assert_eq!(add_point_after(&mut points, 3, None), Some(4));
        assert_eq!(points[4].0, 65.0);
        while points.len() < MAX_POINTS {
            let _ = add_point_after(&mut points, 0, None);
        }
        assert_eq!(
            add_point_at(&mut points, 31.5, None),
            None,
            "at most 8 points"
        );
        while remove_point(&mut points, 1) {}
        assert_eq!(points.len(), MIN_POINTS);
    }

    #[test]
    fn the_line_is_horizontal_outside_the_points() {
        let points = curve();
        let at = |x| interpolate(&points, Interpolation::Linear, x);
        assert_eq!(at(10.0), Some(40.0));
        assert_eq!(at(99.0), Some(100.0));
        assert_eq!(at(37.5), Some(55.0));
    }

    #[test]
    fn the_scale_holds_the_points_and_the_live_input() {
        let scale = PlotScale::for_points(&curve(), Some(31.4));
        assert_eq!((scale.x_min, scale.x_max), (20.0, 70.0));
        let wide = PlotScale::for_points(&curve(), Some(85.0));
        assert_eq!(wide.x_max, 95.0);
    }
}
