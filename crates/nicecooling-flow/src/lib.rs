//! A node canvas for GPUI: pan, zoom, nodes, pins and connections.
//!
//! The code starts as a copy of `gpui-flow` (MIT) at commit `cfa5787`. `THIRD_PARTY.md` and
//! `docs/spikes/gpui-flow.md` list the source and the changes. The type `NodeId` of the source
//! is `FlowNodeId` here: `tools/re/check-identifiers.py` refuses the first name.

// The copy breaks these lints of the workspace. Each lint is fixed or kept with its reason in a
// later commit. Source: `gpui-flow` at commit `cfa5787`.
#![allow(
    clippy::clone_on_copy,
    clippy::collapsible_if,
    clippy::derivable_impls,
    clippy::needless_bool,
    clippy::needless_range_loop,
    clippy::redundant_locals,
    clippy::too_many_arguments,
    clippy::unusual_byte_groupings
)]

pub mod controls;
pub mod edges;
pub mod graph;
pub mod minimap;
pub mod store;
pub mod types;

pub use controls::Controls;
pub use graph::FlowGraph;
pub use minimap::Minimap;
pub use store::{ConnectOutcome, FlowState, MAX_ZOOM, MIN_ZOOM};
pub use types::*;
