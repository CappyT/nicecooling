use std::collections::HashMap;
use std::rc::Rc;

use gpui::{Bounds, EventEmitter, Pixels, SharedString};

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

/// A rule of the app for a connection. It runs after the default rules. `Err` holds the reason
/// of the refusal: the app chooses its text or its code.
pub type ConnectRule = Rc<dyn Fn(&Connection, &FlowState) -> Result<(), SharedString>>;

/// The reason of a refusal by the default rules: a connection from a node to itself.
pub const SAME_NODE: &str = "same node";
/// The reason of a refusal by the default rules: the two pins have a connection already.
pub const ALREADY_CONNECTED: &str = "already connected";

/// The app decides whether the canvas may remove the selection.
pub type DeleteGuard = Rc<dyn Fn(&Selection, &FlowState) -> bool>;

/// What the end of a connection drag did.
#[derive(Debug, Clone, PartialEq)]
pub enum ConnectOutcome {
    /// The canvas added the connection.
    Added(Connection),
    /// A rule refused the connection. The canvas added nothing.
    Refused {
        connection: Connection,
        reason: SharedString,
    },
    /// The drag ended away from a pin, or there was no drag.
    Dropped,
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
    // Config
    pub min_zoom: f32,
    pub max_zoom: f32,
    pub snap_to_grid: bool,
    pub snap_grid: (f32, f32),
    pub connection_radius: f32,
    /// The wheel zooms without a modifier key. Default: false, the wheel pans.
    pub zoom_on_scroll: bool,
    rule: Option<ConnectRule>,
    delete_guard: Option<DeleteGuard>,
    next_edge: u64,
    events: Vec<FlowEvent>,
    /// Window coordinates of the canvas origin, set at each paint. The mouse events have
    /// window coordinates, and the nodes and edges have coordinates relative to the canvas.
    pub origin: (f32, f32),
    /// Size of the canvas in pixels, set at each paint. The zoom keys center on it.
    pub size: (f32, f32),
}

impl EventEmitter<FlowEvent> for FlowState {}

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
            min_zoom: MIN_ZOOM,
            max_zoom: MAX_ZOOM,
            snap_to_grid: false,
            snap_grid: (20.0, 20.0),
            connection_radius: 20.0,
            zoom_on_scroll: false,
            rule: None,
            delete_guard: None,
            next_edge: 0,
            events: Vec::new(),
            origin: (0.0, 0.0),
            size: (0.0, 0.0),
        }
    }

    /// Set the rule of the app for a connection. The canvas asks it during a connection drag
    /// and at the release.
    pub fn with_rule(
        mut self,
        rule: impl Fn(&Connection, &FlowState) -> Result<(), SharedString> + 'static,
    ) -> Self {
        self.rule = Some(Rc::new(rule));
        self
    }

    /// Ask the app before the canvas removes the selection with the Delete key. The guard
    /// returns false to keep it, for example for the end node of a graph. A node with `deletable`
    /// off stays in any case.
    pub fn with_delete_guard(
        mut self,
        guard: impl Fn(&Selection, &FlowState) -> bool + 'static,
    ) -> Self {
        self.delete_guard = Some(Rc::new(guard));
        self
    }

    /// Rebuild the node lookup index.
    pub fn rebuild_lookup(&mut self) {
        self.node_lookup.clear();
        for (i, node) in self.nodes.iter().enumerate() {
            self.node_lookup.insert(node.id.clone(), i);
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

    /// A pointer move at the canvas point `mouse`. It moves the active drag: the connection
    /// draft, the dragged nodes or the view. Returns true when the state changed.
    pub fn pointer_moved(&mut self, mouse: (f32, f32)) -> bool {
        if self.drag_connection(mouse) {
            true
        } else if self.drag_state.is_some() {
            self.drag_nodes(mouse);
            true
        } else {
            self.drag_pan(mouse)
        }
    }

    /// A release of the main button, anywhere. It ends every drag. A node that a drag moved
    /// gives one `NodeMoved` event, with its position at the press and at the release. Returns
    /// true when a drag was active.
    pub fn pointer_released(&mut self) -> bool {
        let mut changed = false;
        if self.connecting.is_some() {
            self.end_connection();
            changed = true;
        }
        if let Some(drag) = self.drag_state.take() {
            for (id, from) in drag.node_origins {
                if let Some(to) = self.get_node(&id).map(|node| node.position) {
                    if to != from {
                        self.events.push(FlowEvent::NodeMoved { id, from, to });
                    }
                }
            }
            for node in &mut self.nodes {
                node.dragging = false;
            }
            changed = true;
        }
        if self.pan_drag.is_some() {
            self.end_pan();
            changed = true;
        }
        changed
    }

    /// Cancel the active drag. A dragged node goes back to where it was, and the view goes back
    /// to where it was before a pan. Returns true when a drag was active.
    pub fn cancel_drag(&mut self) -> bool {
        let mut changed = self.connecting.take().is_some();
        if let Some(drag) = self.drag_state.take() {
            for (id, origin) in drag.node_origins {
                if let Some(node) = self.get_node_mut(&id) {
                    node.position = origin;
                }
            }
            for node in &mut self.nodes {
                node.dragging = false;
            }
            changed = true;
        }
        if let Some(pan) = self.pan_drag.take() {
            self.viewport.x = pan.start_viewport.0;
            self.viewport.y = pan.start_viewport.1;
            changed = true;
        }
        changed
    }

    fn drag_nodes(&mut self, mouse: (f32, f32)) {
        let Some(drag) = &self.drag_state else {
            return;
        };
        let dx = (mouse.0 - drag.origin_mouse.0) / self.viewport.zoom;
        let dy = (mouse.1 - drag.origin_mouse.1) / self.viewport.zoom;
        let origins = drag.node_origins.clone();
        let (gx, gy) = self.snap_grid;
        for (id, origin) in &origins {
            let mut x = origin.x + dx;
            let mut y = origin.y + dy;
            if self.snap_to_grid {
                x = (x / gx).round() * gx;
                y = (y / gy).round() * gy;
            }
            if let Some(node) = self.get_node_mut(id) {
                node.position = FlowPoint::new(x, y);
            }
        }
    }

    /// The selected node or connection. The canvas keeps one selection.
    pub fn selection(&self) -> Option<Selection> {
        self.nodes
            .iter()
            .find(|node| node.selected)
            .map(|node| Selection::Node(node.id.clone()))
            .or_else(|| {
                self.edges
                    .iter()
                    .find(|edge| edge.selected)
                    .map(|edge| Selection::Connection(edge.id.clone()))
            })
    }

    /// Select the next node, in the order of `nodes`, after the selected one: the first node when
    /// none is selected. `backwards` goes the other way. At the end of the list it goes on at the
    /// other end. A hidden node and a node that is not selectable do not count. Returns false when
    /// there is no such node.
    pub fn select_next(&mut self, backwards: bool) -> bool {
        let ids: Vec<FlowNodeId> = self
            .nodes
            .iter()
            .filter(|node| !node.hidden && node.selectable)
            .map(|node| node.id.clone())
            .collect();
        if ids.is_empty() {
            return false;
        }
        let current = match self.selection() {
            Some(Selection::Node(id)) => ids.iter().position(|other| *other == id),
            _ => None,
        };
        let last = ids.len() - 1;
        let next = match (current, backwards) {
            (None, false) => 0,
            (None, true) => last,
            (Some(at), false) => (at + 1) % ids.len(),
            (Some(at), true) => at.checked_sub(1).unwrap_or(last),
        };
        self.select(Some(Selection::Node(ids[next].clone())));
        true
    }

    /// Move the selected node by `steps` grid steps (`snap_grid`), for example `(1, 0)` for one
    /// step to the right. A move sends `NodeMoved`. Returns false when no node moved.
    pub fn nudge_selection(&mut self, steps: (i32, i32)) -> bool {
        let Some(Selection::Node(id)) = self.selection() else {
            return false;
        };
        let (gx, gy) = self.snap_grid;
        let Some(node) = self.get_node_mut(&id).filter(|node| node.draggable) else {
            return false;
        };
        let from = node.position;
        let to = FlowPoint::new(from.x + steps.0 as f32 * gx, from.y + steps.1 as f32 * gy);
        node.position = to;
        self.events.push(FlowEvent::NodeMoved { id, from, to });
        true
    }

    /// Set the zoom to 100%, around the center of the canvas.
    pub fn reset_zoom(&mut self) {
        let center = (self.size.0 / 2.0, self.size.1 / 2.0);
        self.zoom_at(center, 1.0);
    }

    /// Set the selection, as the app wants it. This sends no event.
    pub fn set_selection(&mut self, selection: Option<Selection>) {
        for node in &mut self.nodes {
            node.selected = selection.as_ref() == Some(&Selection::Node(node.id.clone()));
        }
        for edge in &mut self.edges {
            edge.selected = selection.as_ref() == Some(&Selection::Connection(edge.id.clone()));
        }
    }

    /// Set the selection because of an input of the user. A change sends `SelectionChanged`.
    pub fn select(&mut self, selection: Option<Selection>) {
        if self.selection() != selection {
            self.set_selection(selection.clone());
            self.events.push(FlowEvent::SelectionChanged { selection });
        }
    }

    /// Replace the nodes and the connections with the state of the app. This sends no event: the
    /// events tell the app about the inputs of the user, and the app owns the state. A node
    /// without a measured size keeps the size that the canvas measured for the same id. A drag
    /// that is active ends, and the nodes stay where the app put them.
    pub fn set_graph(&mut self, mut nodes: Vec<FlowNode>, edges: Vec<FlowEdge>) {
        for node in &mut nodes {
            if let Some(old) = self.get_node(&node.id) {
                node.measured_width = node.measured_width.or(old.measured_width);
                node.measured_height = node.measured_height.or(old.measured_height);
            }
        }
        self.nodes = nodes;
        self.edges = edges;
        self.rebuild_lookup();
        self.drag_state = None;
        self.connecting = None;
    }

    /// A press on a node: select it, bring it to the front and start a drag. A press during a
    /// connection drag does nothing.
    pub fn press_node(&mut self, id: &FlowNodeId, mouse: (f32, f32)) {
        if self.connecting.is_some() {
            return;
        }
        let Some(node) = self.get_node(id) else {
            return;
        };
        let (selectable, draggable, position) = (node.selectable, node.draggable, node.position);
        if selectable {
            self.select(Some(Selection::Node(id.clone())));
        }
        let top = self
            .nodes
            .iter()
            .map(|node| node.z_index)
            .max()
            .unwrap_or(0);
        if let Some(node) = self.get_node_mut(id) {
            if selectable {
                node.z_index = top + 1;
            }
            if draggable {
                node.dragging = true;
            }
        }
        if draggable {
            self.drag_state = Some(DragState {
                origin_mouse: mouse,
                node_origins: vec![(id.clone(), position)],
            });
        }
    }

    /// A press on the empty canvas: select the connection under the pointer, or clear the
    /// selection and start a pan.
    pub fn press_canvas(&mut self, mouse: (f32, f32)) {
        if self.drag_state.is_some() || self.connecting.is_some() {
            return;
        }
        if let Some(id) = crate::edges::hit_test_edges(self, mouse.0, mouse.1, 5.0) {
            let selectable = self
                .edges
                .iter()
                .any(|edge| edge.id == id && edge.selectable);
            if selectable {
                self.select(Some(Selection::Connection(id)));
            }
            return;
        }
        self.select(None);
        self.start_pan(mouse);
    }

    /// Remove the selected node with its connections, or the selected connection, when the delete
    /// guard of the app agrees. A node or a connection with `deletable` off stays. Each removal
    /// sends an event. Returns true when something went.
    pub fn delete_selection(&mut self) -> bool {
        let selection = self.selection();
        let allowed = selection.as_ref().is_some_and(|selection| {
            self.delete_guard
                .as_ref()
                .is_none_or(|guard| guard(selection, self))
        });
        if !allowed {
            return false;
        }
        let removed = match selection {
            Some(Selection::Node(id)) if self.get_node(&id).is_some_and(|node| node.deletable) => {
                let (gone, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.edges)
                    .into_iter()
                    .partition(|edge| edge.source == id || edge.target == id);
                self.edges = kept;
                for edge in gone {
                    self.events.push(FlowEvent::ConnectionRemoved {
                        connection: edge.connection(),
                        id: edge.id,
                    });
                }
                self.nodes.retain(|node| node.id != id);
                self.rebuild_lookup();
                self.events.push(FlowEvent::NodeRemoved { id });
                true
            }
            Some(Selection::Connection(id))
                if self
                    .edges
                    .iter()
                    .any(|edge| edge.id == id && edge.deletable) =>
            {
                if let Some(at) = self.edges.iter().position(|edge| edge.id == id) {
                    let edge = self.edges.remove(at);
                    self.events.push(FlowEvent::ConnectionRemoved {
                        connection: edge.connection(),
                        id: edge.id,
                    });
                }
                true
            }
            _ => false,
        };
        if removed {
            // The removed item was the selection. `select` would see no change.
            self.set_selection(None);
            self.events
                .push(FlowEvent::SelectionChanged { selection: None });
        }
        removed
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
        let Some(snap) = draft.snap_target.clone() else {
            return ConnectOutcome::Dropped;
        };
        let connection = draft.connection_to(&snap.node_id, &snap.handle_id);
        if let Err(reason) = self.check(&connection) {
            self.events.push(FlowEvent::ConnectionRefused {
                connection: connection.clone(),
                reason: reason.clone(),
            });
            return ConnectOutcome::Refused { connection, reason };
        }
        let edge_id = self.new_edge_id();
        self.add_edge_from_connection(&connection, edge_id.clone());
        self.events.push(FlowEvent::ConnectionAdded {
            id: edge_id,
            connection: connection.clone(),
        });
        ConnectOutcome::Added(connection)
    }

    /// The default rules and the rule of the app, for one connection. `Err` is the reason.
    pub fn check(&self, connection: &Connection) -> Result<(), SharedString> {
        self.default_rules(connection)?;
        self.rule
            .as_ref()
            .map_or(Ok(()), |rule| rule(connection, self))
    }

    /// The events that the canvas has not given to its user yet. The canvas does this after each
    /// input, and the events leave the state.
    pub fn take_events(&mut self) -> Vec<FlowEvent> {
        std::mem::take(&mut self.events)
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
                                    refusal: self
                                        .check(&draft.connection_to(&node.id, &handle.id))
                                        .err(),
                                },
                            ));
                        }
                    }
                }
            }
        }

        best.map(|(_, target)| target)
    }

    fn default_rules(&self, connection: &Connection) -> Result<(), SharedString> {
        if connection.source == connection.target {
            return Err(SAME_NODE.into());
        }
        let duplicate = self.edges.iter().any(|e| {
            e.source == connection.source
                && e.target == connection.target
                && e.source_handle == connection.source_handle
                && e.target_handle == connection.target_handle
        });
        if duplicate {
            return Err(ALREADY_CONNECTED.into());
        }
        Ok(())
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
