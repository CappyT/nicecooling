//! Review reproducer: the window adds one sample per evaluation, not one per second. Two
//! evaluations in one second double the samples, and the sample limit of the validator no longer
//! bounds the memory.

use std::time::Duration;

use nicecooling_graph::window::{Filter, Stat};

/// Spec "Window sample limit": a window node holds at most its span in seconds plus 1 samples.
#[test]
fn window_holds_at_most_span_plus_one_samples() {
    let mut f = Filter::default();
    for half_seconds in 0..40u64 {
        f.step(
            Stat::Average,
            Some(1.0),
            3,
            Duration::from_millis(half_seconds * 500),
        );
    }
    assert!(f.window().len() <= 4, "{} samples", f.window().len());
}
