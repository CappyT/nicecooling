use std::collections::HashMap;

use gpui::prelude::FluentBuilder;
use gpui::*;

use crate::edges;
use crate::store::{FlowState, HandleKey};
use crate::types::*;

type NodeRendererFn = Box<dyn Fn(&FlowNode, &mut Window, &mut App) -> AnyElement>;

/// The top-level flow graph component.
///
/// Renders nodes as positioned divs and edges via a canvas paint layer.
pub struct FlowGraph {
    state: Entity<FlowState>,
    focus_handle: FocusHandle,
    node_renderers: HashMap<SharedString, NodeRendererFn>,
    default_renderer: Option<NodeRendererFn>,
    /// Whether to show the default node wrapper chrome (bg, border, shadow, padding).
    show_node_chrome: bool,
    /// Background color for the canvas (default: 0xf8f8f8).
    bg_color: u32,
    /// Dot grid color (default: 0xd4d4d4).
    grid_color: u32,
    /// Background pattern style.
    bg_pattern: BackgroundPattern,
    /// Node wrapper background color.
    node_bg_color: u32,
    /// Node wrapper border color.
    node_border_color: u32,
    /// Color of a selection, a snap target and the connection draft (default: 0x3b82f6).
    accent_color: u32,
    /// Color of a pin that refuses the connection of the drag (default: 0xdc2626).
    danger_color: u32,
    /// Color of an edge without a color of its own (default: 0xb1b1b7).
    edge_color: u32,
    /// Whether we've done the initial measurement pass.
    measured: bool,
}

impl FlowGraph {
    pub fn new(state: Entity<FlowState>, cx: &mut Context<Self>) -> Self {
        // The canvas draws again when the state changes, also when the app sets the state.
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        Self {
            state,
            focus_handle: cx.focus_handle().tab_stop(true),
            node_renderers: HashMap::new(),
            default_renderer: None,
            show_node_chrome: true,
            bg_color: 0xf8f8f8,
            grid_color: 0xd4d4d4,
            bg_pattern: BackgroundPattern::Dots,
            node_bg_color: 0xffffff,
            node_border_color: 0xe2e2e2,
            accent_color: 0x3b82f6,
            danger_color: 0xdc2626,
            edge_color: 0xb1b1b7,
            measured: false,
        }
    }

    /// The focus handle of the canvas. The keys of the canvas work while it has the focus.
    pub fn focus_handle(&self) -> &FocusHandle {
        &self.focus_handle
    }

    /// Register a renderer for a specific node type.
    pub fn node_renderer(
        mut self,
        node_type: impl Into<SharedString>,
        renderer: impl Fn(&FlowNode, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.node_renderers
            .insert(node_type.into(), Box::new(renderer));
        self
    }

    /// Set the default renderer for nodes without a specific type renderer.
    pub fn default_renderer(
        mut self,
        renderer: impl Fn(&FlowNode, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self {
        self.default_renderer = Some(Box::new(renderer));
        self
    }

    /// Hide the default node wrapper chrome (bg, border, shadow, padding).
    pub fn no_node_chrome(mut self) -> Self {
        self.show_node_chrome = false;
        self
    }

    /// Set the canvas background color.
    pub fn bg_color(mut self, color: u32) -> Self {
        self.bg_color = color;
        self
    }

    /// Set the dot grid color.
    pub fn grid_color(mut self, color: u32) -> Self {
        self.grid_color = color;
        self
    }

    /// Set the node wrapper background color.
    pub fn node_bg_color(mut self, color: u32) -> Self {
        self.node_bg_color = color;
        self
    }

    /// Set the node wrapper border color.
    pub fn node_border_color(mut self, color: u32) -> Self {
        self.node_border_color = color;
        self
    }

    /// Set the color of a selection, a snap target and the connection draft.
    pub fn accent_color(mut self, color: u32) -> Self {
        self.accent_color = color;
        self
    }

    /// Set the color of a pin that refuses the connection of the drag.
    pub fn danger_color(mut self, color: u32) -> Self {
        self.danger_color = color;
        self
    }

    /// Set the color of an edge without a color of its own.
    pub fn edge_color(mut self, color: u32) -> Self {
        self.edge_color = color;
        self
    }

    /// Set the background pattern (Dots, Lines, Cross).
    pub fn bg_pattern(mut self, pattern: BackgroundPattern) -> Self {
        self.bg_pattern = pattern;
        self
    }

    /// Render a single node using the appropriate renderer.
    fn render_node(
        &self,
        node: &FlowNode,
        viewport: &Viewport,
        is_connecting: bool,
        snap: Option<&(HandleKey, bool)>,
        window: &mut Window,
        cx: &mut App,
    ) -> AnyElement {
        if node.hidden {
            return div().into_any_element();
        }

        let (screen_x, screen_y) = viewport.flow_to_screen(node.position);

        // Choose renderer
        let content = if let Some(node_type) = &node.node_type {
            if let Some(renderer) = self.node_renderers.get(node_type) {
                renderer(node, window, cx)
            } else {
                self.render_default_node(node, window, cx)
            }
        } else {
            self.render_default_node(node, window, cx)
        };

        let node_id = node.id.clone();
        let state = self.state.clone();
        let selected = node.selected;
        let dragging = node.dragging;
        let show_chrome = self.show_node_chrome;
        let node_bg = self.node_bg_color;
        let node_border = self.node_border_color;
        let accent = self.accent_color;
        let zoom = viewport.zoom;
        let element_id: ElementId = ElementId::Name(node.id.clone());

        // Build handle dot elements (skip if not connecting to reduce overhead)
        let handle_elements = if !node.handles.is_empty() {
            Self::render_handles(
                &node.handles,
                &node.id,
                &state,
                is_connecting,
                snap,
                node_bg,
                node_border,
                self.accent_color,
                self.danger_color,
                viewport.zoom,
            )
        } else {
            Vec::new()
        };

        // Per-node measurement canvas: captures actual wrapper size.
        // Writes dimensions silently — the graph-level re-render is triggered once.
        let measure_state = self.state.clone();
        let measure_node_id = node.id.clone();
        let prev_w = node.measured_width;
        let prev_h = node.measured_height;
        let measure_canvas = canvas(
            |_bounds, _window, _cx| {},
            move |bounds, _: (), _window, cx| {
                // The measured size is in flow units, so it does not change with the zoom.
                let w = bounds.size.width / zoom;
                let h = bounds.size.height / zoom;
                if prev_w != Some(w) || prev_h != Some(h) {
                    measure_state.update(cx, |state, _| {
                        if let Some(node) = state.get_node_mut(&measure_node_id) {
                            node.measured_width = Some(w);
                            node.measured_height = Some(h);
                        }
                    });
                }
            },
        )
        .size_full()
        .absolute();

        let element = div()
            .id(element_id)
            .absolute()
            .left(px(screen_x))
            .top(px(screen_y))
            // Node box styling on the wrapper so handles align to visual edges
            .when(show_chrome, |el: Stateful<Div>| {
                el.bg(gpui::rgb(node_bg))
                    .border_1()
                    .border_color(gpui::rgb(node_border))
                    .rounded_lg()
                    .shadow_sm()
                    .px_4()
                    .py_2()
            })
            .cursor(if dragging {
                CursorStyle::ClosedHand
            } else {
                CursorStyle::OpenHand
            })
            .when(selected, |el: Stateful<Div>| {
                el.border_2().border_color(gpui::rgb(accent))
            })
            .on_mouse_down(MouseButton::Left, {
                let node_id = node_id.clone();
                let state = state.clone();
                let focus = self.focus_handle.clone();
                move |event, window, cx| {
                    // The keys of the canvas work only with the focus.
                    window.focus(&focus, cx);
                    // The press belongs to the node: the canvas must not start a pan.
                    cx.stop_propagation();
                    update_state(&state, cx, |state| {
                        let mouse = state.to_canvas(event.position);
                        state.press_node(&node_id, mouse);
                        true
                    });
                }
            })
            .child(content)
            .child(measure_canvas)
            .children(handle_elements)
            .into_any_element();

        // Every size of the node that is in rems follows the zoom.
        Zoomed {
            child: element,
            rem_size: window.rem_size() * zoom,
        }
        .into_any_element()
    }

    /// Render handle dots for a node.
    fn render_handles(
        handles: &[HandleDef],
        node_id: &FlowNodeId,
        state: &Entity<FlowState>,
        is_connecting: bool,
        snap: Option<&(HandleKey, bool)>,
        default_bg: u32,
        default_border: u32,
        accent: u32,
        danger: u32,
        zoom: f32,
    ) -> Vec<AnyElement> {
        let handle_size = 10.0 * zoom;
        let half = handle_size / 2.0;

        handles
            .iter()
            .enumerate()
            .map(|(i, handle)| {
                let node_id = node_id.clone();
                let handle_id = handle.id.clone();
                let handle_type = handle.handle_type;
                let handle_position = handle.position;
                let state = state.clone();

                let look = pin_look(is_connecting, &node_id, &handle_id, handle_type, snap);
                let (bg_color, border_color, size_mult) = match look {
                    // Actively snapped: large accent dot
                    PinLook::Snapped => (gpui::rgb(accent), gpui::rgb(accent), 1.4),
                    // Snapped, and a rule refuses the connection: large danger dot
                    PinLook::Refused => (gpui::rgb(danger), gpui::rgb(danger), 1.4),
                    // Potential target: accent outline
                    PinLook::Candidate => (gpui::rgb(default_bg), gpui::rgb(accent), 1.0),
                    PinLook::Plain => (gpui::rgb(default_bg), gpui::rgb(default_border), 1.0),
                };
                let dot_size = handle_size * size_mult;

                let dot = div()
                    .w(px(dot_size))
                    .h(px(dot_size))
                    .rounded_full()
                    .bg(bg_color)
                    .border_1()
                    .border_color(border_color)
                    .cursor(CursorStyle::Crosshair)
                    .flex_shrink_0();

                let container = div()
                    .id(ElementId::Integer(i as u64))
                    .absolute()
                    .flex()
                    .items_center()
                    .justify_center()
                    // Handle mouse down → start connection
                    .on_mouse_down(MouseButton::Left, {
                        let state = state.clone();
                        let node_id = node_id.clone();
                        let handle_id = handle_id.clone();
                        move |event, _window, cx| {
                            update_state(&state, cx, |state| {
                                let mouse = state.to_canvas(event.position);
                                state.begin_connection(
                                    &node_id,
                                    &handle_id,
                                    handle_type,
                                    handle_position,
                                    mouse,
                                );
                                true
                            });
                        }
                    });

                let container = match handle.position {
                    HandlePosition::Left => container.left(px(-half)).w(px(handle_size)),
                    HandlePosition::Right => container.right(px(-half)).w(px(handle_size)),
                    HandlePosition::Top => container.top(px(-half)).h(px(handle_size)),
                    HandlePosition::Bottom => container.bottom(px(-half)).h(px(handle_size)),
                };
                let on_vertical_side = matches!(
                    handle.position,
                    HandlePosition::Left | HandlePosition::Right
                );
                // Without an offset the handle takes the whole side, with its dot in the middle.
                let along = handle.offset.map(|offset| px(offset * zoom - half));
                let container = match (on_vertical_side, along) {
                    (true, None) => container.top_0().bottom_0(),
                    (true, Some(top)) => container.top(top).h(px(handle_size)),
                    (false, None) => container.left_0().right_0(),
                    (false, Some(left)) => container.left(left).w(px(handle_size)),
                };

                container.child(dot).into_any_element()
            })
            .collect()
    }

    /// Default node rendering — just the label content (styling is on the wrapper).
    fn render_default_node(
        &self,
        node: &FlowNode,
        _window: &mut Window,
        _cx: &mut App,
    ) -> AnyElement {
        div()
            .min_w(px(80.0))
            .text_sm()
            .text_color(gpui::rgb(0x1a1a1a))
            .child(if node.label.is_empty() {
                node.id.to_string()
            } else {
                node.label.to_string()
            })
            .into_any_element()
    }

    /// Paint the background dot grid.
    fn paint_grid(
        bounds: &Bounds<Pixels>,
        viewport: &Viewport,
        step: f32,
        grid_color: u32,
        pattern: BackgroundPattern,
        window: &mut Window,
    ) {
        let color = gpui::rgb(grid_color);
        let spacing = step * viewport.zoom;

        if spacing < 5.0 {
            return;
        }

        let start_x = viewport.x % spacing;
        let start_y = viewport.y % spacing;
        let bw = bounds.size.width.as_f32();
        let bh = bounds.size.height.as_f32();
        let ox = bounds.origin.x;
        let oy = bounds.origin.y;

        match pattern {
            BackgroundPattern::Dots => {
                let dot_size = px(1.5 * viewport.zoom.min(1.0));
                let mut x = start_x;
                while x < bw {
                    let mut y = start_y;
                    while y < bh {
                        let dot_bounds = Bounds::new(
                            Point::new(ox + px(x) - dot_size / 2.0, oy + px(y) - dot_size / 2.0),
                            Size {
                                width: dot_size,
                                height: dot_size,
                            },
                        );
                        window.paint_quad(gpui::fill(dot_bounds, color));
                        y += spacing;
                    }
                    x += spacing;
                }
            }
            BackgroundPattern::Lines => {
                let line_w = px(0.5);
                let mut x = start_x;
                while x < bw {
                    let line = Bounds::new(
                        Point::new(ox + px(x), oy),
                        Size {
                            width: line_w,
                            height: bounds.size.height,
                        },
                    );
                    window.paint_quad(gpui::fill(line, color));
                    x += spacing;
                }
            }
            BackgroundPattern::Cross => {
                let line_w = px(0.5);
                // Vertical lines
                let mut x = start_x;
                while x < bw {
                    let line = Bounds::new(
                        Point::new(ox + px(x), oy),
                        Size {
                            width: line_w,
                            height: bounds.size.height,
                        },
                    );
                    window.paint_quad(gpui::fill(line, color));
                    x += spacing;
                }
                // Horizontal lines
                let mut y = start_y;
                while y < bh {
                    let line = Bounds::new(
                        Point::new(ox, oy + px(y)),
                        Size {
                            width: bounds.size.width,
                            height: line_w,
                        },
                    );
                    window.paint_quad(gpui::fill(line, color));
                    y += spacing;
                }
            }
        }
    }

    /// Paint a draft connection line from handle to mouse cursor.
    fn paint_connection_draft(
        draft: &ConnectionDraft,
        origin: (f32, f32),
        accent: u32,
        danger: u32,
        window: &mut Window,
    ) {
        let refused = draft
            .snap_target
            .as_ref()
            .is_some_and(|t| t.refusal.is_some());
        let color: Background =
            gpui::rgba((if refused { danger } else { accent } << 8) | 0x80).into();
        let (sx, sy) = (draft.from_point.0 + origin.0, draft.from_point.1 + origin.1);
        let (tx, ty) = (draft.to_point.0 + origin.0, draft.to_point.1 + origin.1);

        let mut builder = PathBuilder::stroke(px(2.0));
        builder.move_to(Point::new(px(sx), px(sy)));

        // Simple bezier toward cursor
        let dx = (tx - sx).abs() * 0.5;
        let (cx1, cy1, cx2, cy2) = match draft.from_position {
            HandlePosition::Right => (sx + dx, sy, tx - dx, ty),
            HandlePosition::Left => (sx - dx, sy, tx + dx, ty),
            HandlePosition::Bottom => (sx, sy + dx, tx, ty - dx),
            HandlePosition::Top => (sx, sy - dx, tx, ty + dx),
        };

        builder.cubic_bezier_to(
            Point::new(px(tx), px(ty)),
            Point::new(px(cx1), px(cy1)),
            Point::new(px(cx2), px(cy2)),
        );

        if let Ok(path) = builder.build() {
            window.paint_path(path, color);
        }
    }
}

impl Render for FlowGraph {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // On second render, edges will have correct node measurements
        // from the first render's measurement canvases.
        if !self.measured {
            self.measured = true;
            cx.notify(); // triggers one immediate re-render
        }

        let window_size = window.viewport_size();
        let win_w = window_size.width.as_f32();
        let win_h = window_size.height.as_f32();
        let cull_margin = 200.0;

        // Read state, extract what we need, then release the borrow
        let (
            viewport,
            grid_step,
            is_panning,
            is_connecting,
            snap,
            connecting_draft,
            visible_nodes,
            edge_label_elements,
        ) = {
            let state = self.state.read(cx);
            let viewport = state.viewport;
            let grid_step = state.snap_grid.0;
            let is_panning = state.pan_drag.is_some();
            let is_connecting = state.connecting.is_some();
            let snap: Option<(HandleKey, bool)> = state
                .connecting
                .as_ref()
                .and_then(|d| d.snap_target.as_ref())
                .map(|t| {
                    (
                        (t.node_id.clone(), t.handle_id.clone()),
                        t.refusal.is_some(),
                    )
                });
            let connecting_draft = state.connecting.clone();

            // Build visible node indices
            let mut visible_indices: Vec<usize> = Vec::new();
            for (i, node) in state.nodes.iter().enumerate() {
                if node.hidden {
                    continue;
                }
                let (sx, sy) = viewport.flow_to_screen(node.position);
                let nw = node.measured_width.map(|p| p.as_f32()).unwrap_or(150.0) * viewport.zoom;
                let nh = node.measured_height.map(|p| p.as_f32()).unwrap_or(50.0) * viewport.zoom;
                if sx + nw < -cull_margin
                    || sx > win_w + cull_margin
                    || sy + nh < -cull_margin
                    || sy > win_h + cull_margin
                {
                    continue;
                }
                visible_indices.push(i);
            }
            visible_indices.sort_by_key(|&i| state.nodes[i].z_index);

            let visible_nodes: Vec<FlowNode> = visible_indices
                .iter()
                .map(|&i| state.nodes[i].clone())
                .collect();

            // Edge labels
            let bg = self.bg_color;
            let mut edge_label_elements: Vec<AnyElement> = Vec::new();
            for edge in &state.edges {
                if let Some(ref label) = edge.label {
                    if let Some((lx, ly)) = edges::compute_edge_label_position(state, edge) {
                        let label_color = edge.color.unwrap_or(self.edge_color);
                        edge_label_elements.push(
                            div()
                                .absolute()
                                .left(px(lx))
                                .top(px(ly))
                                .px_2()
                                .py_0p5()
                                .bg(gpui::rgb(bg))
                                .rounded_sm()
                                .text_xs()
                                .text_color(gpui::rgb(label_color))
                                .child(label.to_string())
                                .into_any_element(),
                        );
                    }
                }
            }

            (
                viewport,
                grid_step,
                is_panning,
                is_connecting,
                snap,
                connecting_draft,
                visible_nodes,
                edge_label_elements,
            )
        }; // state borrow ends here

        // Render only visible nodes
        let mut node_elements: Vec<AnyElement> = Vec::with_capacity(visible_nodes.len());
        for node in &visible_nodes {
            node_elements.push(self.render_node(
                node,
                &viewport,
                is_connecting,
                snap.as_ref(),
                window,
                cx,
            ));
        }

        let focus_ring = self.focus_handle.is_focused(window) && window.last_input_was_keyboard();
        let state_for_canvas = self.state.clone();
        let viewport_for_canvas = viewport;
        let bg_color = self.bg_color;
        let grid_color = self.grid_color;
        let bg_pattern = self.bg_pattern;
        let accent_color = self.accent_color;
        let danger_color = self.danger_color;
        let edge_color = self.edge_color;
        let state_for_scroll = self.state.clone();
        let state_for_mouse_down = self.state.clone();
        let state_for_key = self.state.clone();
        let state_for_pinch = self.state.clone();

        div()
            .id("flow-graph")
            .track_focus(&self.focus_handle)
            .size_full()
            .overflow_hidden()
            .relative()
            .bg(gpui::rgb(bg_color))
            .cursor(if is_panning {
                CursorStyle::ClosedHand
            } else if is_connecting {
                CursorStyle::Crosshair
            } else {
                CursorStyle::Arrow
            })
            // Background grid + edge painting layer
            .child(
                canvas(
                    |_bounds, _window, _cx| {},
                    move |bounds, _: (), window, cx| {
                        Self::paint_grid(
                            &bounds,
                            &viewport_for_canvas,
                            grid_step,
                            grid_color,
                            bg_pattern,
                            window,
                        );
                        let origin = (bounds.origin.x.as_f32(), bounds.origin.y.as_f32());
                        let size = (bounds.size.width.as_f32(), bounds.size.height.as_f32());
                        state_for_canvas.update(cx, |state, _| {
                            state.origin = origin;
                            state.size = size;
                        });
                        // Window listeners: a drag goes on and ends where the pointer is, also
                        // outside the canvas and after a key press, when the canvas is not hovered.
                        window.on_mouse_event({
                            let state = state_for_canvas.clone();
                            move |event: &MouseMoveEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble {
                                    update_state(&state, cx, |state| {
                                        let mouse = state.to_canvas(event.position);
                                        state.pointer_moved(mouse)
                                    });
                                }
                            }
                        });
                        window.on_mouse_event({
                            let state = state_for_canvas.clone();
                            move |event: &MouseUpEvent, phase, _, cx| {
                                if phase == DispatchPhase::Bubble
                                    && event.button == MouseButton::Left
                                {
                                    update_state(&state, cx, |state| state.pointer_released());
                                }
                            }
                        });
                        let state = state_for_canvas.read(cx);
                        edges::paint_edges(state, edge_color, accent_color, window);

                        // Paint draft connection line
                        if let Some(ref draft) = connecting_draft {
                            Self::paint_connection_draft(
                                draft,
                                origin,
                                accent_color,
                                danger_color,
                                window,
                            );
                        }
                    },
                )
                .absolute()
                .size_full(),
            )
            // Node layer
            .children(node_elements)
            // Edge labels
            .children(edge_label_elements)
            // Focus ring: only when the focus came from the keyboard
            .when(focus_ring, |el| {
                el.child(
                    div()
                        .absolute()
                        .size_full()
                        .border_2()
                        .border_color(gpui::rgb(accent_color)),
                )
            })
            // A press on the empty canvas selects a connection, or clears the selection and pans.
            .on_mouse_down(MouseButton::Left, {
                let focus = self.focus_handle.clone();
                move |event, window, cx| {
                    // The keys of the canvas, Escape among them, work only with the focus.
                    window.focus(&focus, cx);
                    update_state(&state_for_mouse_down, cx, |state| {
                        let mouse = state.to_canvas(event.position);
                        state.press_canvas(mouse);
                        true
                    });
                }
            })
            .on_key_down({
                let focus = self.focus_handle.clone();
                move |event: &KeyDownEvent, window, cx| {
                    let key: &str = event.keystroke.key.as_ref();
                    // Keys that change the graph work only when the canvas itself has the
                    // focus, not when an element inside a node has it.
                    let graph_focused = focus.is_focused(window);

                    let modifiers = event.keystroke.modifiers;
                    let plain = !modifiers.control && !modifiers.platform && !modifiers.alt;
                    if key == "escape" {
                        update_state(&state_for_key, cx, |state| state.cancel_drag());
                    } else if graph_focused && plain {
                        let handled = update_state(&state_for_key, cx, |state| match key {
                            "tab" => state.select_next(modifiers.shift),
                            "left" => state.nudge_selection((-1, 0)),
                            "right" => state.nudge_selection((1, 0)),
                            "up" => state.nudge_selection((0, -1)),
                            "down" => state.nudge_selection((0, 1)),
                            "delete" | "backspace" => state.delete_selection(),
                            "=" | "+" => {
                                let (w, h) = state.size;
                                state.zoom_in(w, h);
                                true
                            }
                            "-" | "_" => {
                                let (w, h) = state.size;
                                state.zoom_out(w, h);
                                true
                            }
                            "0" => {
                                state.reset_zoom();
                                true
                            }
                            _ => false,
                        });
                        // The canvas owns Tab, also at the ends of the list.
                        if handled || key == "tab" {
                            cx.stop_propagation();
                        }
                    }
                }
            })
            // Scroll to zoom / pan
            .on_scroll(move |event, _window, cx| {
                let delta = event.delta.pixel_delta(px(20.0));
                update_state(&state_for_scroll, cx, |state| {
                    if state.zoom_on_scroll || event.modifiers.platform || event.modifiers.control {
                        // Zoom towards mouse. One wheel step is 3 lines of 20 px, and it
                        // zooms by the factor of the zoom buttons. Wheel up zooms in.
                        let focus = state.to_canvas(event.position);
                        state.zoom_steps_at(focus, delta.y.as_f32() / 60.0);
                    } else {
                        state.viewport.x += delta.x.as_f32();
                        state.viewport.y += delta.y.as_f32();
                    }
                    true
                });
            })
            // Pinch to zoom (macOS trackpad / Linux Wayland)
            .on_pinch(move |event, _window, cx| {
                update_state(&state_for_pinch, cx, |state| {
                    let focus = state.to_canvas(event.position);
                    state.zoom_at(focus, state.viewport.zoom * (1.0 + event.delta));
                    true
                });
            })
    }
}

/// Change the state, then give the events that the change made to the subscribers. `change`
/// returns true when the state changed: the canvas then draws again. The function gives that
/// value back.
fn update_state(
    state: &Entity<FlowState>,
    cx: &mut App,
    change: impl FnOnce(&mut FlowState) -> bool,
) -> bool {
    state.update(cx, |state, cx| {
        let changed = change(state);
        let events = state.take_events();
        if changed || !events.is_empty() {
            cx.notify();
        }
        for event in events {
            cx.emit(event);
        }
        changed
    })
}

/// How a pin shows during a connection drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PinLook {
    Plain,
    /// An input pin: the drag can end here.
    Candidate,
    /// The draft is on this pin, and the connection is allowed.
    Snapped,
    /// The draft is on this pin, and a rule refuses the connection.
    Refused,
}

pub(crate) fn pin_look(
    is_connecting: bool,
    node_id: &FlowNodeId,
    handle_id: &Option<SharedString>,
    handle_type: HandleType,
    snap: Option<&(HandleKey, bool)>,
) -> PinLook {
    match snap {
        Some(((snap_node, snap_handle), refused))
            if snap_node == node_id && snap_handle == handle_id =>
        {
            if *refused {
                PinLook::Refused
            } else {
                PinLook::Snapped
            }
        }
        _ if is_connecting && handle_type == HandleType::Target => PinLook::Candidate,
        _ => PinLook::Plain,
    }
}

/// Lays out and paints a node with a rem size that follows the zoom.
struct Zoomed {
    child: AnyElement,
    rem_size: Pixels,
}

impl IntoElement for Zoomed {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for Zoomed {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let layout_id = window.with_rem_size(Some(self.rem_size), |window| {
            self.child.request_layout(window, cx)
        });
        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_rem_size(Some(self.rem_size), |window| {
            self.child.prepaint(window, cx);
        });
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut (),
        _prepaint: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_rem_size(Some(self.rem_size), |window| {
            self.child.paint(window, cx);
        });
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: it brings the `test` macro of GPUI, which hides `#[test]`.
    use super::{PinLook, pin_look};
    use crate::{FlowNodeId, HandleType};
    use gpui::SharedString;

    fn pin(node: &str, handle: &str) -> (FlowNodeId, Option<SharedString>) {
        (node.to_string().into(), Some(handle.to_string().into()))
    }

    #[test]
    fn a_refused_snap_target_shows_the_refused_look() {
        let (node, handle) = pin("b", "in");
        let look = |is_connecting, snap: Option<((FlowNodeId, Option<SharedString>), bool)>| {
            pin_look(
                is_connecting,
                &node,
                &handle,
                HandleType::Target,
                snap.as_ref(),
            )
        };
        assert_eq!(look(true, Some((pin("b", "in"), true))), PinLook::Refused);
        assert_eq!(look(true, Some((pin("b", "in"), false))), PinLook::Snapped);
        // A snap on another pin leaves this input pin a candidate.
        assert_eq!(
            look(true, Some((pin("b", "other"), true))),
            PinLook::Candidate
        );
        assert_eq!(look(false, None), PinLook::Plain);
    }
}
