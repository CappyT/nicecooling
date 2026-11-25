use std::collections::HashMap;
use std::rc::Rc;

use gpui::{Bounds, Pixels, SharedString};

use crate::types::*;

/// Resolved handle position in screen coordinates, stored after layout.
#[derive(Debug, Clone)]
pub struct ResolvedHandle {
    pub node_id: FlowNodeId,
    pub handle_id: Option<SharedString>,
    pub handle_type: HandleType,
    pub position: HandlePosition,
    pub bounds: Bounds<Pixels>,
}

/// Screen-space center of a handle, relative to the canvas origin.
///
/// The measured size of a node and the offset of a handle are in flow units.
pub fn handle_center(
    node: &FlowNode,
    handle_id: &Option<SharedString>,
    position: HandlePosition,
    viewport: &Viewport,
) -> (f32, f32) {
    let (sx, sy) = viewport.flow_to_screen(node.position);
    let zoom = viewport.zoom;
    let w = node.measured_width.map(|p| p.as_f32()).unwrap_or(114.0) * zoom;
    let h = node.measured_height.map(|p| p.as_f32()).unwrap_or(54.0) * zoom;
    let offset = node
        .handles
        .iter()
        .find(|handle| handle.id == *handle_id && handle.position == position)
        .and_then(|handle| handle.offset)
        .map(|offset| offset * zoom);
    match position {
        HandlePosition::Top => (sx + offset.unwrap_or(w / 2.0), sy),
        HandlePosition::Bottom => (sx + offset.unwrap_or(w / 2.0), sy + h),
        HandlePosition::Left => (sx, sy + offset.unwrap_or(h / 2.0)),
        HandlePosition::Right => (sx + w, sy + offset.unwrap_or(h / 2.0)),
    }
}

/// Key for looking up a handle: (node_id, handle_id).
pub type HandleKey = (FlowNodeId, Option<SharedString>);

/// The lowest zoom of a new canvas: 25%.
pub const MIN_ZOOM: f32 = 0.25;
/// The highest zoom of a new canvas: 400%.
pub const MAX_ZOOM: f32 = 4.0;
/// One zoom step of the buttons and the wheel.
const ZOOM_STEP: f32 = 1.2;

/// A rule of the app for a connection. It runs after the default rules.
pub type ConnectRule = Rc<dyn Fn(&Connection, &FlowState) -> bool>;

/// What the end of a connection drag did.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectOutcome {
    /// The canvas added the connection.
    Added(Connection),
    /// A rule refused the connection. The canvas added nothing.
    Refused(Connection),
    /// The drag ended away from a pin, or there was no drag.
    Dropped,
}

/// A snapshot of the graph state for undo/redo.
#[derive(Clone)]
struct HistoryEntry {
    nodes: Vec<FlowNode>,
    edges: Vec<FlowEdge>,
}

/// The central state store for a flow graph. Wrapped in `Entity<FlowState>`.
pub struct FlowState {
    pub nodes: Vec<FlowNode>,
    pub edges: Vec<FlowEdge>,
    node_lookup: HashMap<FlowNodeId, usize>,
    pub viewport: Viewport,
    pub handle_bounds: HashMap<HandleKey, ResolvedHandle>,
    // Interaction state
    pub drag_state: Option<DragState>,
    pub pan_drag: Option<PanDragState>,
    pub connecting: Option<ConnectionDraft>,
    pub selection_box: Option<SelectionBox>,
    // Undo/redo
    undo_stack: Vec<HistoryEntry>,
    redo_stack: Vec<HistoryEntry>,
    // Config
    pub min_zoom: f32,
    pub max_zoom: f32,
    pub snap_to_grid: bool,
    pub snap_grid: (f32, f32),
    pub connection_radius: f32,
    /// The wheel zooms without a modifier key. Default: false, the wheel pans.
    pub zoom_on_scroll: bool,
    rule: Option<ConnectRule>,
    next_edge: u64,
    /// Window coordinates of the canvas origin, set at each paint. The mouse events have
    /// window coordinates, and the nodes and edges have coordinates relative to the canvas.
    pub origin: (f32, f32),
}

impl FlowState {
    pub fn new(nodes: Vec<FlowNode>, edges: Vec<FlowEdge>) -> Self {
        let mut node_lookup = HashMap::new();
        for (i, node) in nodes.iter().enumerate() {
            node_lookup.insert(node.id.clone(), i);
        }

        Self {
            nodes,
            edges,
            node_lookup,
            viewport: Viewport::default(),
            handle_bounds: HashMap::new(),
            drag_state: None,
            pan_drag: None,
            connecting: None,
            selection_box: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            min_zoom: MIN_ZOOM,
            max_zoom: MAX_ZOOM,
            snap_to_grid: false,
            snap_grid: (20.0, 20.0),
            connection_radius: 20.0,
            zoom_on_scroll: false,
            rule: None,
            next_edge: 0,
            origin: (0.0, 0.0),
        }
    }

    /// Set the rule of the app for a connection. The canvas asks it during a connection drag
    /// and at the release.
    pub fn with_rule(mut self, rule: impl Fn(&Connection, &FlowState) -> bool + 'static) -> Self {
        self.rule = Some(Rc::new(rule));
        self
    }

    /// Rebuild the node lookup index.
    pub fn rebuild_lookup(&mut self) {
        self.node_lookup.clear();
        for (i, node) in self.nodes.iter().enumerate() {
            self.node_lookup.insert(node.id.clone(), i);
        }
    }

    /// Push the current state onto the undo stack. Call before destructive operations.
    pub fn push_undo(&mut self) {
        self.undo_stack.push(HistoryEntry {
            nodes: self.nodes.clone(),
            edges: self.edges.clone(),
        });
        // Clear redo stack on new action
        self.redo_stack.clear();
        // Limit history size
        if self.undo_stack.len() > 100 {
            self.undo_stack.remove(0);
        }
    }

    /// Undo the last operation. Returns true if undo was performed.
    pub fn undo(&mut self) -> bool {
        if let Some(entry) = self.undo_stack.pop() {
            self.redo_stack.push(HistoryEntry {
                nodes: self.nodes.clone(),
                edges: self.edges.clone(),
            });
            self.nodes = entry.nodes;
            self.edges = entry.edges;
            self.rebuild_lookup();
            true
        } else {
            false
        }
    }

    /// Redo the last undone operation. Returns true if redo was performed.
    pub fn redo(&mut self) -> bool {
        if let Some(entry) = self.redo_stack.pop() {
            self.undo_stack.push(HistoryEntry {
                nodes: self.nodes.clone(),
                edges: self.edges.clone(),
            });
            self.nodes = entry.nodes;
            self.edges = entry.edges;
            self.rebuild_lookup();
            true
        } else {
            false
        }
    }

    /// Get a node by ID.
    pub fn get_node(&self, id: &FlowNodeId) -> Option<&FlowNode> {
        self.node_lookup.get(id).map(|&i| &self.nodes[i])
    }

    /// Get a mutable node by ID.
    pub fn get_node_mut(&mut self, id: &FlowNodeId) -> Option<&mut FlowNode> {
        self.node_lookup.get(id).map(|&i| &mut self.nodes[i])
    }

    /// Set all nodes, rebuilding the index.
    pub fn set_nodes(&mut self, nodes: Vec<FlowNode>) {
        self.nodes = nodes;
        self.rebuild_lookup();
    }

    /// Set all edges.
    pub fn set_edges(&mut self, edges: Vec<FlowEdge>) {
        self.edges = edges;
    }

    /// Apply a batch of node changes.
    pub fn apply_node_changes(&mut self, changes: &[NodeChange]) {
        let mut needs_rebuild = false;

        for change in changes {
            match change {
                NodeChange::Position {
                    id,
                    position,
                    dragging,
                } => {
                    if let Some(node) = self.get_node_mut(id) {
                        node.position = *position;
                        node.dragging = *dragging;
                    }
                }
                NodeChange::Dimensions { id, width, height } => {
                    if let Some(node) = self.get_node_mut(id) {
                        node.measured_width = Some(*width);
                        node.measured_height = Some(*height);
                    }
                }
                NodeChange::Select { id, selected } => {
                    if let Some(node) = self.get_node_mut(id) {
                        node.selected = *selected;
                    }
                }
                NodeChange::Remove { id } => {
                    if let Some(&idx) = self.node_lookup.get(id) {
                        self.nodes.remove(idx);
                        needs_rebuild = true;
                    }
                }
            }
        }

        if needs_rebuild {
            self.rebuild_lookup();
        }
    }

    /// Apply a batch of edge changes.
    pub fn apply_edge_changes(&mut self, changes: &[EdgeChange]) {
        for change in changes {
            match change {
                EdgeChange::Select { id, selected } => {
                    if let Some(edge) = self.edges.iter_mut().find(|e| e.id == *id) {
                        edge.selected = *selected;
                    }
                }
                EdgeChange::Remove { id } => {
                    self.edges.retain(|e| e.id != *id);
                }
            }
        }
    }

    /// Set the viewport, clamping zoom to min/max.
    pub fn set_viewport(&mut self, viewport: Viewport) {
        self.viewport = Viewport {
            x: viewport.x,
            y: viewport.y,
            zoom: viewport.zoom.clamp(self.min_zoom, self.max_zoom),
        };
    }

    /// Register a resolved handle position (called during layout).
    pub fn set_handle_bounds(&mut self, key: HandleKey, resolved: ResolvedHandle) {
        self.handle_bounds.insert(key, resolved);
    }

    /// A mouse position in window coordinates, relative to the canvas origin.
    pub fn to_canvas(&self, position: gpui::Point<Pixels>) -> (f32, f32) {
        (
            position.x.as_f32() - self.origin.0,
            position.y.as_f32() - self.origin.1,
        )
    }

    /// Find the screen-space center of a handle, relative to the canvas origin.
    ///
    /// Computes the position directly from the node's flow coordinates + viewport,
    /// using measured wrapper dimensions when available.
    pub fn find_handle_center(
        &self,
        node_id: &FlowNodeId,
        handle_id: &Option<SharedString>,
        handle_position: HandlePosition,
    ) -> Option<(f32, f32)> {
        let node = self.get_node(node_id)?;
        Some(handle_center(
            node,
            handle_id,
            handle_position,
            &self.viewport,
        ))
    }

    /// Fit the viewport to show all nodes with padding.
    pub fn fit_view(&mut self, padding: f32, container_width: f32, container_height: f32) {
        if self.nodes.is_empty() {
            return;
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for node in &self.nodes {
            if node.hidden {
                continue;
            }
            let w = node.measured_width.map(|p| p.as_f32()).unwrap_or(150.0);
            let h = node.measured_height.map(|p| p.as_f32()).unwrap_or(40.0);
            min_x = min_x.min(node.position.x);
            min_y = min_y.min(node.position.y);
            max_x = max_x.max(node.position.x + w);
            max_y = max_y.max(node.position.y + h);
        }

        let content_width = max_x - min_x + padding * 2.0;
        let content_height = max_y - min_y + padding * 2.0;

        if content_width <= 0.0 || content_height <= 0.0 {
            return;
        }

        let zoom_x = container_width / content_width;
        let zoom_y = container_height / content_height;
        let zoom = zoom_x.min(zoom_y).clamp(self.min_zoom, self.max_zoom);

        self.viewport.zoom = zoom;
        self.viewport.x = (container_width - content_width * zoom) / 2.0 - (min_x - padding) * zoom;
        self.viewport.y =
            (container_height - content_height * zoom) / 2.0 - (min_y - padding) * zoom;
    }

    /// Set the zoom, clamped to the limits, and keep the canvas point `focus` where it is.
    pub fn zoom_at(&mut self, focus: (f32, f32), zoom: f32) {
        let old_zoom = self.viewport.zoom;
        let new_zoom = zoom.clamp(self.min_zoom, self.max_zoom);
        self.viewport.x = focus.0 - (focus.0 - self.viewport.x) * (new_zoom / old_zoom);
        self.viewport.y = focus.1 - (focus.1 - self.viewport.y) * (new_zoom / old_zoom);
        self.viewport.zoom = new_zoom;
    }

    /// Zoom in by a step, centered on the container center.
    pub fn zoom_in(&mut self, container_width: f32, container_height: f32) {
        let center = (container_width / 2.0, container_height / 2.0);
        self.zoom_at(center, self.viewport.zoom * ZOOM_STEP);
    }

    /// Zoom out by a step, centered on the container center.
    pub fn zoom_out(&mut self, container_width: f32, container_height: f32) {
        let center = (container_width / 2.0, container_height / 2.0);
        self.zoom_at(center, self.viewport.zoom / ZOOM_STEP);
    }

    /// Zoom by `ZOOM_STEP` to the power of `steps`, around the canvas point `focus`.
    pub fn zoom_steps_at(&mut self, focus: (f32, f32), steps: f32) {
        self.zoom_at(focus, self.viewport.zoom * ZOOM_STEP.powf(steps));
    }

    /// Start a pan drag at the canvas point `mouse`.
    pub fn start_pan(&mut self, mouse: (f32, f32)) {
        self.pan_drag = Some(PanDragState {
            start_mouse: mouse,
            start_viewport: (self.viewport.x, self.viewport.y),
        });
    }

    /// Move the view with the pan drag. Returns false when no pan drag is active.
    pub fn drag_pan(&mut self, mouse: (f32, f32)) -> bool {
        let Some(pan) = self.pan_drag else {
            return false;
        };
        self.viewport.x = pan.start_viewport.0 + (mouse.0 - pan.start_mouse.0);
        self.viewport.y = pan.start_viewport.1 + (mouse.1 - pan.start_mouse.1);
        true
    }

    /// End the pan drag.
    pub fn end_pan(&mut self) {
        self.pan_drag = None;
    }

    /// Start a connection drag on a pin.
    pub fn begin_connection(
        &mut self,
        node_id: &FlowNodeId,
        handle_id: &Option<SharedString>,
        handle_type: HandleType,
        position: HandlePosition,
        mouse: (f32, f32),
    ) {
        let from_point = self
            .find_handle_center(node_id, handle_id, position)
            .unwrap_or(mouse);
        self.connecting = Some(ConnectionDraft {
            from_node: node_id.clone(),
            from_handle: handle_id.clone(),
            from_type: handle_type,
            from_position: position,
            from_point,
            to_point: mouse,
            snap_target: None,
        });
        // A connection drag and a node drag never run together.
        self.drag_state = None;
    }

    /// Move the free end of the connection draft. It snaps to the nearest pin in range.
    /// Returns false when no connection drag is active.
    pub fn drag_connection(&mut self, mouse: (f32, f32)) -> bool {
        let Some(mut draft) = self.connecting.take() else {
            return false;
        };
        let snap = self.find_snap_target(&draft, mouse.0, mouse.1);
        draft.to_point = snap.as_ref().map_or(mouse, |target| target.point);
        draft.snap_target = snap;
        self.connecting = Some(draft);
        true
    }

    /// End the connection drag. A snapped pin that the rules accept gets a new connection.
    pub fn end_connection(&mut self) -> ConnectOutcome {
        let Some(draft) = self.connecting.take() else {
            return ConnectOutcome::Dropped;
        };
        let Some(snap) = draft.snap_target else {
            return ConnectOutcome::Dropped;
        };
        let connection = if draft.from_type == HandleType::Source {
            Connection {
                source: draft.from_node,
                target: snap.node_id,
                source_handle: draft.from_handle,
                target_handle: snap.handle_id,
            }
        } else {
            Connection {
                source: snap.node_id,
                target: draft.from_node,
                source_handle: snap.handle_id,
                target_handle: draft.from_handle,
            }
        };
        if !self.accepts(&connection) {
            return ConnectOutcome::Refused(connection);
        }
        self.push_undo();
        let edge_id = self.new_edge_id();
        self.add_edge_from_connection(&connection, edge_id);
        ConnectOutcome::Added(connection)
    }

    /// The default rules and the rule of the app, for one connection.
    pub fn accepts(&self, connection: &Connection) -> bool {
        self.is_valid_connection(connection)
            && self.rule.as_ref().is_none_or(|rule| rule(connection, self))
    }

    fn new_edge_id(&mut self) -> SharedString {
        loop {
            self.next_edge += 1;
            let id: SharedString = format!("e{}", self.next_edge).into();
            if self.edges.iter().all(|edge| edge.id != id) {
                return id;
            }
        }
    }

    /// Center the viewport on a specific flow coordinate.
    pub fn set_center(
        &mut self,
        flow_x: f32,
        flow_y: f32,
        container_width: f32,
        container_height: f32,
    ) {
        self.viewport.x = container_width / 2.0 - flow_x * self.viewport.zoom;
        self.viewport.y = container_height / 2.0 - flow_y * self.viewport.zoom;
    }

    /// Select all nodes and edges.
    pub fn select_all(&mut self) {
        for node in &mut self.nodes {
            if node.selectable {
                node.selected = true;
            }
        }
        for edge in &mut self.edges {
            if edge.selectable {
                edge.selected = true;
            }
        }
    }

    /// Get nodes that have edges pointing TO the given node.
    pub fn get_incomers(&self, node_id: &FlowNodeId) -> Vec<&FlowNode> {
        self.edges
            .iter()
            .filter(|e| e.target == *node_id)
            .filter_map(|e| self.get_node(&e.source))
            .collect()
    }

    /// Get nodes that the given node has edges pointing TO.
    pub fn get_outgoers(&self, node_id: &FlowNodeId) -> Vec<&FlowNode> {
        self.edges
            .iter()
            .filter(|e| e.source == *node_id)
            .filter_map(|e| self.get_node(&e.target))
            .collect()
    }

    /// Get all edges connected to a node (as source or target).
    pub fn get_connected_edges(&self, node_id: &FlowNodeId) -> Vec<&FlowEdge> {
        self.edges
            .iter()
            .filter(|e| e.source == *node_id || e.target == *node_id)
            .collect()
    }

    /// Find the nearest valid handle to snap to during connection dragging.
    pub fn find_snap_target(
        &self,
        draft: &ConnectionDraft,
        mouse_x: f32,
        mouse_y: f32,
    ) -> Option<SnapTarget> {
        let radius = self.connection_radius;
        let mut best: Option<(f32, SnapTarget)> = None;

        for node in &self.nodes {
            if node.hidden {
                continue;
            }
            // Can't connect to the same node
            if node.id == draft.from_node {
                continue;
            }

            for handle in &node.handles {
                // Source→Target or Target→Source only
                if handle.handle_type == draft.from_type {
                    continue;
                }
                // Check per-handle connectable flag
                if !handle.is_connectable {
                    continue;
                }

                if let Some((hx, hy)) =
                    self.find_handle_center(&node.id, &handle.id, handle.position)
                {
                    let dx = mouse_x - hx;
                    let dy = mouse_y - hy;
                    let dist = (dx * dx + dy * dy).sqrt();

                    if dist <= radius {
                        if best.is_none() || dist < best.as_ref().unwrap().0 {
                            best = Some((
                                dist,
                                SnapTarget {
                                    node_id: node.id.clone(),
                                    handle_id: handle.id.clone(),
                                    handle_type: handle.handle_type,
                                    position: handle.position,
                                    point: (hx, hy),
                                },
                            ));
                        }
                    }
                }
            }
        }

        best.map(|(_, target)| target)
    }

    /// Check if a connection is valid (default rules).
    pub fn is_valid_connection(&self, connection: &Connection) -> bool {
        // No self-connections
        if connection.source == connection.target {
            return false;
        }

        // No duplicate edges
        let duplicate = self.edges.iter().any(|e| {
            e.source == connection.source
                && e.target == connection.target
                && e.source_handle == connection.source_handle
                && e.target_handle == connection.target_handle
        });
        if duplicate {
            return false;
        }

        true
    }

    /// Add an edge from a completed connection.
    pub fn add_edge_from_connection(
        &mut self,
        connection: &Connection,
        edge_id: impl Into<SharedString>,
    ) {
        let mut edge = FlowEdge::new(
            edge_id,
            connection.source.clone(),
            connection.target.clone(),
        );
        if let Some(ref sh) = connection.source_handle {
            edge.source_handle = Some(sh.clone());
        }
        if let Some(ref th) = connection.target_handle {
            edge.target_handle = Some(th.clone());
        }
        self.edges.push(edge);
    }
}
