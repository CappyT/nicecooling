//! Editor spike: a Graph of 8 nodes and 8 connections on the copied `gpui-flow` canvas, in the
//! light and the dark tokens of `docs/ui/DESIGN.md`. The result is in `docs/spikes/gpui-flow.md`.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_flow::*;

/// The tokens of `docs/ui/DESIGN.md` that the spike uses, as 0xRRGGBB.
#[derive(Clone, Copy)]
struct Tokens {
    view: u32,
    card: u32,
    headerbar: u32,
    fg: u32,
    fg_dim: u32,
    border: u32,
    border_strong: u32,
    accent_bg: u32,
}

impl Tokens {
    fn new(dark: bool) -> Self {
        let pick = |light: u32, dark_value: u32| if dark { dark_value } else { light };
        Self {
            view: pick(0xFFFFFF, 0x1D1D20),
            card: pick(0xFFFFFF, 0x36363A),
            headerbar: pick(0xFFFFFF, 0x2E2E32),
            fg: pick(0x2A2A2E, 0xFFFFFF),
            fg_dim: pick(0x5E5E66, 0xB4B4BC),
            border: pick(0xDCDCE0, 0x47474D),
            border_strong: pick(0xB8B8C0, 0x6A6A72),
            accent_bg: 0x1C71D8,
        }
    }
}

/// One node of the sample Graph. An input pin has the name of its row. The output pin is `out`.
struct NodeSpec {
    id: &'static str,
    kind: &'static str,
    name: &'static str,
    value: f32,
    unit: &'static str,
    inputs: &'static [&'static str],
    output: bool,
    x: f32,
    y: f32,
}

const NODES: [NodeSpec; 8] = [
    NodeSpec {
        id: "water",
        kind: "Sensor",
        name: "Water temperature",
        value: 31.4,
        unit: "\u{b0}C",
        inputs: &[],
        output: true,
        x: 40.0,
        y: 40.0,
    },
    NodeSpec {
        id: "ambient",
        kind: "Sensor",
        name: "Ambient temperature",
        value: 22.8,
        unit: "\u{b0}C",
        inputs: &[],
        output: true,
        x: 40.0,
        y: 200.0,
    },
    NodeSpec {
        id: "flow",
        kind: "Sensor",
        name: "Flow",
        value: 182.0,
        unit: "l/h",
        inputs: &[],
        output: true,
        x: 40.0,
        y: 440.0,
    },
    NodeSpec {
        id: "sub",
        kind: "Subtract",
        name: "Delta T",
        value: 8.6,
        unit: "K",
        inputs: &["A", "B"],
        output: true,
        x: 300.0,
        y: 100.0,
    },
    NodeSpec {
        id: "avg",
        kind: "Average, 60 s",
        name: "Delta T, smooth",
        value: 8.4,
        unit: "K",
        inputs: &["In"],
        output: true,
        x: 560.0,
        y: 100.0,
    },
    NodeSpec {
        id: "less",
        kind: "Less than",
        name: "Flow is low",
        value: 0.0,
        unit: "",
        inputs: &["A", "B"],
        output: true,
        x: 300.0,
        y: 420.0,
    },
    NodeSpec {
        id: "switch",
        kind: "Switch",
        name: "Pick",
        value: 8.4,
        unit: "K",
        inputs: &["Condition", "On", "Off"],
        output: true,
        x: 800.0,
        y: 260.0,
    },
    NodeSpec {
        id: "end",
        kind: "Sensor end node",
        name: "Graph sensor",
        value: 8.4,
        unit: "K",
        inputs: &["In"],
        output: false,
        x: 1050.0,
        y: 280.0,
    },
];

/// Source node, target node, input pin of the target. The input pin `B` of `less` stays free.
const CONNECTIONS: [(&str, &str, &str); 8] = [
    ("water", "sub", "A"),
    ("ambient", "sub", "B"),
    ("sub", "avg", "In"),
    ("flow", "less", "A"),
    ("less", "switch", "Condition"),
    ("water", "switch", "On"),
    ("avg", "switch", "Off"),
    ("switch", "end", "In"),
];

// The sizes of a node are in flow units: 1 unit is 1 px at a zoom of 100%.
const NODE_WIDTH: f32 = 170.0;
/// The copied canvas adds a 1 px border and 8 px of padding around the content of a node.
const CONTENT_INSET: f32 = 9.0;
const SIDE_PADDING: f32 = 17.0;
const CAPTION_HEIGHT: f32 = 16.0;
const NAME_HEIGHT: f32 = 20.0;
const VALUE_HEIGHT: f32 = 22.0;
const PIN_ROW_HEIGHT: f32 = 20.0;
const HEADER_HEIGHT: f32 = 46.0;

/// A length that follows the zoom: the copied canvas scales the rem size of a node.
fn units(value: f32) -> Rems {
    rems(value / 16.0)
}

/// The output pin is at the middle of the value row.
fn output_pin_offset() -> f32 {
    CONTENT_INSET + CAPTION_HEIGHT + NAME_HEIGHT + VALUE_HEIGHT / 2.0
}

/// An input pin is at the middle of its row.
fn input_pin_offset(row: usize) -> f32 {
    CONTENT_INSET
        + CAPTION_HEIGHT
        + NAME_HEIGHT
        + VALUE_HEIGHT
        + PIN_ROW_HEIGHT * (row as f32 + 0.5)
}

fn node_height(spec: &NodeSpec) -> f32 {
    2.0 * CONTENT_INSET
        + CAPTION_HEIGHT
        + NAME_HEIGHT
        + VALUE_HEIGHT
        + PIN_ROW_HEIGHT * spec.inputs.len() as f32
}

fn flow_nodes() -> Vec<FlowNode> {
    NODES
        .iter()
        .map(|spec| {
            let mut handles: Vec<HandleDef> = spec
                .inputs
                .iter()
                .enumerate()
                .map(|(row, name)| {
                    HandleDef::target(HandlePosition::Left)
                        .id(*name)
                        .offset(input_pin_offset(row))
                })
                .collect();
            if spec.output {
                handles.push(
                    HandleDef::source(HandlePosition::Right)
                        .id("out")
                        .offset(output_pin_offset()),
                );
            }
            FlowNode::new(spec.id, spec.x, spec.y)
                .node_type("node")
                .size(NODE_WIDTH, node_height(spec))
                .handles(handles)
        })
        .collect()
}

fn flow_edges() -> Vec<FlowEdge> {
    CONNECTIONS
        .iter()
        .map(|(source, target, pin)| {
            FlowEdge::new(edge_id(source, target, pin), *source, *target)
                .source_handle("out")
                .target_handle(*pin)
        })
        .collect()
}

/// An input pin has one connection at most, so the target and its pin name the connection.
fn edge_id(source: &str, target: &str, pin: &str) -> String {
    format!("{source}>{target}.{pin}")
}

/// The rule of the Graph model: an input pin has at most one connection.
fn input_pin_is_free(connection: &Connection, state: &FlowState) -> bool {
    !state.edges.iter().any(|edge| {
        edge.target == connection.target && edge.target_handle == connection.target_handle
    })
}

fn add_connection(connection: &Connection, state: &mut FlowState) {
    let pin = connection.target_handle.as_deref().unwrap_or("?");
    let id = edge_id(&connection.source, &connection.target, pin);
    state.add_edge_from_connection(connection, id);
}

/// The content of a node: its kind, its name, its value, and one row per input pin.
fn node_content(spec: &NodeSpec, tokens: Tokens, tick: u64) -> AnyElement {
    // With `--live` the value moves once per second, as a value of the evaluator does.
    let value = spec.value + (tick % 5) as f32 * 0.1;
    div()
        .w(units(NODE_WIDTH - 2.0 * SIDE_PADDING))
        .flex()
        .flex_col()
        .child(
            div()
                .h(units(CAPTION_HEIGHT))
                .text_size(units(12.0))
                .line_height(units(CAPTION_HEIGHT))
                .text_color(rgb(tokens.fg_dim))
                .child(spec.kind),
        )
        .child(
            div()
                .h(units(NAME_HEIGHT))
                .text_size(units(14.0))
                .line_height(units(NAME_HEIGHT))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(tokens.fg))
                .truncate()
                .child(spec.name),
        )
        .child(
            div()
                .h(units(VALUE_HEIGHT))
                .flex()
                .justify_end()
                .gap_1()
                .text_size(units(16.0))
                .line_height(units(VALUE_HEIGHT))
                .child(
                    div()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(tokens.fg))
                        .child(format!("{value:.1}")),
                )
                .child(div().text_color(rgb(tokens.fg_dim)).child(spec.unit)),
        )
        .children(spec.inputs.iter().map(|name| {
            div()
                .h(units(PIN_ROW_HEIGHT))
                .text_size(units(12.0))
                .line_height(units(PIN_ROW_HEIGHT))
                .text_color(rgb(tokens.fg_dim))
                .child(*name)
        }))
        .into_any_element()
}

fn build_flow(
    state: &Entity<FlowState>,
    tokens: Tokens,
    tick: Rc<Cell<u64>>,
    cx: &mut App,
) -> Entity<FlowGraph> {
    cx.new(|cx| {
        FlowGraph::new(state.clone(), cx)
            .bg_color(tokens.view)
            .grid_color(tokens.border)
            .node_bg_color(tokens.card)
            .node_border_color(tokens.border_strong)
            .accent_color(tokens.accent_bg)
            .edge_color(tokens.fg_dim)
            .validate_connection(input_pin_is_free)
            .on_connect(add_connection)
            .node_renderer("node", move |node, _window, _cx| {
                match NODES.iter().find(|spec| spec.id == node.id.as_ref()) {
                    Some(spec) => node_content(spec, tokens, tick.get()),
                    None => div().into_any_element(),
                }
            })
    })
}

struct Spike {
    state: Entity<FlowState>,
    flow: Entity<FlowGraph>,
    /// The theme of the command line. Without it the spike follows the system.
    forced_dark: Option<bool>,
    dark: bool,
    tick: Rc<Cell<u64>>,
    logged_zoom: f32,
    _flow_observer: Subscription,
}

impl Spike {
    fn new(
        forced_dark: Option<bool>,
        live: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let state = cx.new(|_| {
            let mut state = FlowState::new(flow_nodes(), flow_edges());
            state.min_zoom = 0.25;
            state.max_zoom = 4.0;
            state.zoom_on_scroll = true;
            state
        });
        let dark = forced_dark.unwrap_or_else(|| system_is_dark(window));
        let tick = Rc::new(Cell::new(0));
        let flow = build_flow(&state, Tokens::new(dark), tick.clone(), cx);
        let flow_observer = cx.observe(&flow, Self::flow_changed);
        cx.observe_window_appearance(window, |this, window, cx| {
            let dark = this.forced_dark.unwrap_or_else(|| system_is_dark(window));
            if dark != this.dark {
                this.dark = dark;
                // The colors of the copied canvas are set when it is built.
                this.flow = build_flow(&this.state, Tokens::new(dark), this.tick.clone(), cx);
                this._flow_observer = cx.observe(&this.flow, Self::flow_changed);
                cx.notify();
            }
        })
        .detach();
        if live {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                    let alive = this.update(cx, |this, cx| {
                        this.tick.set(this.tick.get() + 1);
                        this.flow.update(cx, |_, cx| cx.notify());
                    });
                    if alive.is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
        Self {
            state,
            flow,
            forced_dark,
            dark,
            tick,
            logged_zoom: 1.0,
            _flow_observer: flow_observer,
        }
    }

    fn flow_changed(&mut self, _flow: Entity<FlowGraph>, cx: &mut Context<Self>) {
        let zoom = self.state.read(cx).viewport.zoom;
        if zoom != self.logged_zoom {
            self.logged_zoom = zoom;
            println!("zoom {:.0}%", zoom * 100.0);
        }
        cx.notify();
    }

    /// One line per mouse release, for a check without a person.
    fn log_state(&self, cx: &App) {
        let state = self.state.read(cx);
        let nodes: Vec<String> = state
            .nodes
            .iter()
            .map(|node| {
                format!(
                    "{}=({:.0},{:.0})",
                    node.id, node.position.x, node.position.y
                )
            })
            .collect();
        let connections: Vec<String> = state
            .edges
            .iter()
            .map(|edge| {
                format!(
                    "{}>{}.{}",
                    edge.source,
                    edge.target,
                    edge.target_handle.as_deref().unwrap_or("?")
                )
            })
            .collect();
        println!(
            "release: view=({:.0},{:.0}) zoom={:.0}% connections={} [{}] nodes [{}]",
            state.viewport.x,
            state.viewport.y,
            state.viewport.zoom * 100.0,
            connections.len(),
            connections.join(" "),
            nodes.join(" "),
        );
    }
}

fn system_is_dark(window: &Window) -> bool {
    matches!(
        window.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    )
}

/// The first family of the design that has a bold face of its own, as the app picks it.
fn ui_font(cx: &App) -> Option<SharedString> {
    let text = cx.text_system();
    let installed = text.all_font_names();
    ["Adwaita Sans", "Inter"]
        .into_iter()
        .find(|family| {
            let regular = font(*family);
            installed.iter().any(|name| name == family)
                && text.resolve_font(&regular) != text.resolve_font(&regular.clone().bold())
        })
        .map(SharedString::from)
}

impl Render for Spike {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tokens = Tokens::new(self.dark);
        let state = self.state.read(cx);
        let theme = match (self.dark, self.forced_dark) {
            (true, Some(_)) => "dark, from the command line",
            (false, Some(_)) => "light, from the command line",
            (true, None) => "dark, from the system",
            (false, None) => "light, from the system",
        };
        let status = format!(
            "Theme: {theme}.  Zoom {:.0}%.  {} nodes, {} connections.",
            state.viewport.zoom * 100.0,
            state.nodes.len(),
            state.edges.len(),
        );
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(tokens.view))
            .text_color(rgb(tokens.fg))
            .text_size(px(14.0))
            .when_some(ui_font(cx), |el, family| el.font_family(family))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| this.log_state(cx)),
            )
            .child(
                div()
                    .h(px(HEADER_HEIGHT))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_4()
                    .px_6()
                    .bg(rgb(tokens.headerbar))
                    .border_b_1()
                    .border_color(rgb(tokens.border))
                    .child(div().font_weight(FontWeight::BOLD).child("gpui-flow spike"))
                    .child(div().text_color(rgb(tokens.fg_dim)).child(status)),
            )
            .child(div().flex_1().min_h_0().child(self.flow.clone()))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|arg| arg == flag);
    let forced_dark = if has("--dark") {
        Some(true)
    } else if has("--light") {
        Some(false)
    } else {
        None
    };
    let live = has("--live");
    gpui_platform::application().run(move |cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.0), px(800.0)), cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("gpui-flow spike".into()),
                ..Default::default()
            }),
            app_id: Some("gpui-flow-spike".to_owned()),
            ..Default::default()
        };
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.open_window(options, move |window, cx| {
            cx.new(|cx| Spike::new(forced_dark, live, window, cx))
        })
        .expect("the window of the spike");
        cx.activate(true);
    });
}
