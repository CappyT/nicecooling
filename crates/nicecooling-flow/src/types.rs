use gpui::{Pixels, SharedString};

/// Unique identifier for a node.
pub type FlowNodeId = SharedString;

/// Unique identifier for an edge.
pub type EdgeId = SharedString;

/// A 2D point in flow (graph) coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FlowPoint {
    pub x: f32,
    pub y: f32,
}

impl FlowPoint {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// The type of a handle — determines connection direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandleType {
    Source,
    Target,
}

/// Where on the node a handle is positioned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HandlePosition {
    Top,
    Right,
    Bottom,
    Left,
}

/// Definition of a handle on a node.
#[derive(Debug, Clone)]
pub struct HandleDef {
    pub id: Option<SharedString>,
    pub handle_type: HandleType,
    pub position: HandlePosition,
    /// Whether this handle can accept/start connections. Default: true.
    pub is_connectable: bool,
    /// Distance in flow units from the top of the node (Left, Right) or from its left edge
    /// (Top, Bottom) to the handle center. None: the middle of the side.
    pub offset: Option<f32>,
}

impl HandleDef {
    pub fn source(position: HandlePosition) -> Self {
        Self {
            id: None,
            handle_type: HandleType::Source,
            position,
            is_connectable: true,
            offset: None,
        }
    }

    pub fn target(position: HandlePosition) -> Self {
        Self {
            id: None,
            handle_type: HandleType::Target,
            position,
            is_connectable: true,
            offset: None,
        }
    }

    pub fn connectable(mut self, connectable: bool) -> Self {
        self.is_connectable = connectable;
        self
    }

    pub fn id(mut self, id: impl Into<SharedString>) -> Self {
        self.id = Some(id.into());
        self
    }

    pub fn offset(mut self, offset: f32) -> Self {
        self.offset = Some(offset);
        self
    }
}

/// Background grid pattern style.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BackgroundPattern {
    Dots,
    Lines,
    Cross,
}

impl Default for BackgroundPattern {
    fn default() -> Self {
        BackgroundPattern::Dots
    }
}

/// The visual type and parameters of an edge.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EdgeType {
    Bezier { curvature: f32 },
    SmoothStep { border_radius: f32, offset: f32 },
    Straight,
}

impl Default for EdgeType {
    fn default() -> Self {
        EdgeType::Bezier { curvature: 0.25 }
    }
}

/// A node in the flow graph.
#[derive(Debug, Clone)]
pub struct FlowNode {
    pub id: FlowNodeId,
    pub position: FlowPoint,
    pub node_type: Option<SharedString>,
    pub label: SharedString,
    pub handles: Vec<HandleDef>,
    pub selected: bool,
    pub dragging: bool,
    pub draggable: bool,
    pub selectable: bool,
    pub connectable: bool,
    pub deletable: bool,
    pub hidden: bool,
    pub z_index: i32,
    pub measured_width: Option<Pixels>,
    pub measured_height: Option<Pixels>,
}

impl FlowNode {
    pub fn new(id: impl Into<SharedString>, x: f32, y: f32) -> Self {
        Self {
            id: id.into(),
            position: FlowPoint::new(x, y),
            node_type: None,
            label: SharedString::default(),
            handles: Vec::new(),
            selected: false,
            dragging: false,
            draggable: true,
            selectable: true,
            connectable: true,
            deletable: true,
            hidden: false,
            z_index: 0,
            measured_width: None,
            measured_height: None,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    pub fn node_type(mut self, node_type: impl Into<SharedString>) -> Self {
        self.node_type = Some(node_type.into());
        self
    }

    pub fn handles(mut self, handles: Vec<HandleDef>) -> Self {
        self.handles = handles;
        self
    }

    /// Set initial estimated size (width, height in pixels).
    /// Used for edge positioning before the node is rendered and measured.
    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.measured_width = Some(gpui::px(width));
        self.measured_height = Some(gpui::px(height));
        self
    }

    pub fn z_index(mut self, z: i32) -> Self {
        self.z_index = z;
        self
    }
}

/// An edge connecting two nodes.
#[derive(Debug, Clone)]
pub struct FlowEdge {
    pub id: EdgeId,
    pub source: FlowNodeId,
    pub target: FlowNodeId,
    pub source_handle: Option<SharedString>,
    pub target_handle: Option<SharedString>,
    pub edge_type: EdgeType,
    pub selected: bool,
    pub selectable: bool,
    pub deletable: bool,
    pub hidden: bool,
    pub animated: bool,
    /// Optional label displayed at the edge midpoint.
    pub label: Option<SharedString>,
    /// Custom edge color as 0xRRGGBB. If None, uses the default.
    pub color: Option<u32>,
    /// Custom stroke width. If None, uses default (2.0).
    pub stroke_width: Option<f32>,
}

impl FlowEdge {
    pub fn new(
        id: impl Into<SharedString>,
        source: impl Into<SharedString>,
        target: impl Into<SharedString>,
    ) -> Self {
        Self {
            id: id.into(),
            source: source.into(),
            target: target.into(),
            source_handle: None,
            target_handle: None,
            edge_type: EdgeType::default(),
            selected: false,
            selectable: true,
            deletable: true,
            hidden: false,
            animated: false,
            label: None,
            color: None,
            stroke_width: None,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn color(mut self, color: u32) -> Self {
        self.color = Some(color);
        self
    }

    pub fn stroke_width(mut self, width: f32) -> Self {
        self.stroke_width = Some(width);
        self
    }

    pub fn source_handle(mut self, handle: impl Into<SharedString>) -> Self {
        self.source_handle = Some(handle.into());
        self
    }

    pub fn target_handle(mut self, handle: impl Into<SharedString>) -> Self {
        self.target_handle = Some(handle.into());
        self
    }

    pub fn edge_type(mut self, edge_type: EdgeType) -> Self {
        self.edge_type = edge_type;
        self
    }

    /// The pins that the edge joins.
    pub fn connection(&self) -> Connection {
        Connection {
            source: self.source.clone(),
            target: self.target.clone(),
            source_handle: self.source_handle.clone(),
            target_handle: self.target_handle.clone(),
        }
    }
}

/// Viewport state: pan offset and zoom level.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub x: f32,
    pub y: f32,
    pub zoom: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            zoom: 1.0,
        }
    }
}

impl Viewport {
    /// Convert a point from flow coordinates to screen coordinates.
    pub fn flow_to_screen(&self, point: FlowPoint) -> (f32, f32) {
        (point.x * self.zoom + self.x, point.y * self.zoom + self.y)
    }

    /// Convert a point from screen coordinates to flow coordinates.
    pub fn screen_to_flow(&self, screen_x: f32, screen_y: f32) -> FlowPoint {
        FlowPoint::new(
            (screen_x - self.x) / self.zoom,
            (screen_y - self.y) / self.zoom,
        )
    }
}

/// State tracked during a node drag operation.
#[derive(Debug, Clone)]
pub struct DragState {
    /// Screen coordinates where the drag started.
    pub origin_mouse: (f32, f32),
    /// Original flow-space positions of all dragged nodes.
    pub node_origins: Vec<(FlowNodeId, FlowPoint)>,
}

/// State tracked during a canvas pan drag.
#[derive(Debug, Clone, Copy)]
pub struct PanDragState {
    /// Screen coordinates where the drag started.
    pub start_mouse: (f32, f32),
    /// Viewport position at drag start.
    pub start_viewport: (f32, f32),
}

/// State tracked while dragging a connection from a handle.
#[derive(Debug, Clone)]
pub struct ConnectionDraft {
    pub from_node: FlowNodeId,
    pub from_handle: Option<SharedString>,
    pub from_type: HandleType,
    pub from_position: HandlePosition,
    /// Screen coordinates of the source handle center.
    pub from_point: (f32, f32),
    /// Current mouse screen coordinates (the dangling end).
    pub to_point: (f32, f32),
    /// If the cursor is near a valid target handle, snap to it.
    pub snap_target: Option<SnapTarget>,
}

/// A handle that the connection draft is snapping to.
#[derive(Debug, Clone)]
pub struct SnapTarget {
    pub node_id: FlowNodeId,
    pub handle_id: Option<SharedString>,
    pub handle_type: HandleType,
    pub position: HandlePosition,
    /// Screen coordinates of the snapped handle center.
    pub point: (f32, f32),
    /// The reason why a connection to this pin is refused. A release here adds nothing.
    pub refusal: Option<SharedString>,
}

impl ConnectionDraft {
    /// The connection between the origin of the draft and a pin: from the output pin to the
    /// input pin, whichever end the drag started on.
    pub fn connection_to(
        &self,
        node_id: &FlowNodeId,
        handle_id: &Option<SharedString>,
    ) -> Connection {
        if self.from_type == HandleType::Source {
            Connection {
                source: self.from_node.clone(),
                target: node_id.clone(),
                source_handle: self.from_handle.clone(),
                target_handle: handle_id.clone(),
            }
        } else {
            Connection {
                source: node_id.clone(),
                target: self.from_node.clone(),
                source_handle: handle_id.clone(),
                target_handle: self.from_handle.clone(),
            }
        }
    }
}

/// The selected item. The canvas keeps one selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Selection {
    Node(FlowNodeId),
    Connection(EdgeId),
}

/// What the canvas tells its user. Each event comes from an input of the user. The canvas sends
/// no event when the app sets the state.
#[derive(Debug, Clone, PartialEq)]
pub enum FlowEvent {
    /// A drag moved a node. One event for each drag, with the position at the press and at the
    /// release. A drag that Escape cancels sends none.
    NodeMoved {
        id: FlowNodeId,
        from: FlowPoint,
        to: FlowPoint,
    },
    /// A drop on a free pin made a connection. The canvas gave it the id.
    ConnectionAdded { id: EdgeId, connection: Connection },
    /// The canvas removed a connection: the user deleted it, or deleted a node that it joined.
    ConnectionRemoved { id: EdgeId, connection: Connection },
    /// The canvas removed a node.
    NodeRemoved { id: FlowNodeId },
    /// The selection changed.
    SelectionChanged { selection: Option<Selection> },
    /// A drop on a pin that a rule refuses. The canvas added nothing.
    ConnectionRefused {
        connection: Connection,
        reason: SharedString,
    },
}

/// A completed connection between two handles.
#[derive(Debug, Clone, PartialEq)]
pub struct Connection {
    pub source: FlowNodeId,
    pub target: FlowNodeId,
    pub source_handle: Option<SharedString>,
    pub target_handle: Option<SharedString>,
}
