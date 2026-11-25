//! A node canvas for GPUI: pan, zoom, nodes, pins and connections.
//!
//! The code starts as a copy of `gpui-flow` (MIT) at commit `cfa5787`. `THIRD_PARTY.md` and
//! `docs/spikes/gpui-flow.md` list the source and the changes. The type `NodeId` of the source
//! is `FlowNodeId` here: `tools/re/check-identifiers.py` refuses the first name.
//!
//! # The model
//!
//! The app owns the graph. The canvas shows it, and it reports what the user does. It never
//! decides what the graph is.
//!
//! - [`FlowState`] holds the nodes, the connections, the view and the drag in progress. Put it in
//!   an `Entity` and give it to [`FlowGraph`], the element that draws it.
//! - The app sets the state with [`FlowState::set_graph`] and [`FlowState::set_selection`]. These
//!   calls send no event.
//! - The canvas sends a [`FlowEvent`] for each input of the user: [`FlowEvent::NodeMoved`] (one
//!   for each drag, with the start and the end position), [`FlowEvent::ConnectionAdded`],
//!   [`FlowEvent::ConnectionRemoved`], [`FlowEvent::NodeRemoved`],
//!   [`FlowEvent::SelectionChanged`] and [`FlowEvent::ConnectionRefused`]. Subscribe with
//!   `cx.subscribe(&state, ...)`. The app applies the change to its own graph and sets the
//!   state again.
//! - The canvas keeps no undo history and binds no undo key. The app owns undo and redo.
//! - Observe the entity of the state (`cx.observe`) to read the view, for example the zoom.
//!
//! # Connections
//!
//! [`FlowState::with_rule`] sets the rule of the app. The rule returns `Err(reason)` for a
//! connection that it refuses. The canvas runs it while a drag is over a pin: the pin shows the
//! refused look and [`SnapTarget::refusal`] holds the reason. A drop on that pin adds nothing and
//! sends [`FlowEvent::ConnectionRefused`]. The default rules refuse a connection from a node to
//! itself ([`SAME_NODE`]) and a connection that exists ([`ALREADY_CONNECTED`]).
//!
//! # View
//!
//! A drag on the empty canvas pans. The wheel pans, and the wheel with Ctrl zooms. The zoom stays
//! between [`MIN_ZOOM`] (25%) and [`MAX_ZOOM`] (400%).

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
pub use store::{ALREADY_CONNECTED, ConnectOutcome, FlowState, MAX_ZOOM, MIN_ZOOM, SAME_NODE};
pub use types::*;
