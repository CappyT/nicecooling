//! Review reproducer: the low pass keeps its damped output from before a gap. After the window
//! empties and fills again, it moves from that old output, not from the output that it gave
//! last.

use std::time::Duration;

use nicecooling_graph::window::{Filter, Stat};

/// Spec "Low pass node": with fewer than 3 samples the node gives their mean. Otherwise it moves
/// its output toward the trimmed mean by the factor 1 - damping / 100.
#[test]
fn low_pass_moves_from_its_last_output_after_a_gap() {
    let stat = Stat::LowPass { damping: 25.0 };
    let mut f = Filter::default();
    let at = Duration::from_secs;
    let mut out = Vec::new();
    // 100 for 4 s: the damped output is 100.
    for t in 0..4 {
        out.push(f.step(stat, Some(100.0), 3, at(t)));
    }
    // No value until every sample of 100 has left the 3 s window.
    for t in 4..9 {
        out.push(f.step(stat, None, 3, at(t)));
    }
    // 10 from second 9. The window holds only samples of 10.
    for t in 9..13 {
        out.push(f.step(stat, Some(10.0), 3, at(t)));
    }
    // Seconds 9 and 10 have fewer than 3 samples: their mean, 10. From second 11 the trimmed mean
    // is 10 and the last output is 10, so the output stays 10.
    assert_eq!(out[9..], [Some(10.0), Some(10.0), Some(10.0), Some(10.0)]);
}
