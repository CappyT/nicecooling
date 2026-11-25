//! The window of the Graph editor: the toolbar, the node list, the canvas and its status line.
//! It draws `EditorModel` and gives each input of the user to `EditorUi`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Context, Div, Entity, FocusHandle, FontWeight, Hsla, KeyDownEvent, Pixels,
    Point, Rems, Render, SharedString, Window, div, px, rems,
};
use gpui_component::button::ButtonVariants as _;
use gpui_component::{Disableable as _, Selectable as _, h_flex, v_flex};
use nicecooling_flow::{FlowEvent, FlowGraph, FlowNode, FlowState};
use nicecooling_graph::model::Kind;

use super::canvas::{
    CAPTION_ROW, GUIDE_ROW, NodeFace, PIN_ROW, SUBJECT_ROW, Shape, TITLE_ROW, VALUE_ROW,
    canvas_point, flow_edges, flow_nodes, flow_selection, new_flow_state, opening_view, shape_of,
    view_centre,
};
use super::palette::{ListForm, ListKey, ListRow, NodeList, family_key};
use super::{EditorKey, EditorModel, EditorUi, Selected, editor_key};
use crate::app::now_us;
use crate::graphs::view::GraphView;
use crate::icons::{NcIcon, icon, small_icon};
use crate::screen_view::{InputKey, ScreenView, with_screen_input};
use crate::screens::controls::act;
use crate::screens::ui::Tip as _;
use crate::screens::ui::{caption, label, popover, with_popover};
use crate::tokens::Palette;

/// The widths of the two side columns of the editor.
const LIST_WIDTH: f32 = 168.;
/// The size of the node list popover.
const POPOVER_WIDTH: f32 = 240.;
const POPOVER_MAX_HEIGHT: f32 = 480.;
/// The height of the search field of the node list with its padding.
const SEARCH_HEIGHT: f32 = 46.;

/// What the node renderer of the canvas reads: the faces of the nodes, and the theme.
#[derive(Default)]
struct CanvasLook {
    faces: BTreeMap<u32, NodeFace>,
    dark: bool,
}

/// The open Graph editor: its state, and the entities of its canvas.
pub(crate) struct EditorHost {
    pub(crate) ui: EditorUi,
    flow: Entity<FlowState>,
    canvas: Entity<FlowGraph>,
    look: Rc<RefCell<CanvasLook>>,
    /// What the canvas holds of the draft. A change of the draft sets the canvas again.
    shape: Shape,
    /// The canvas showed the whole Graph once, after its first paint gave its size.
    fitted: bool,
    /// The theme of the colors that the canvas has.
    dark: bool,
    /// The events of the canvas that wait for the end of the input: one input is one undo step.
    events: Vec<FlowEvent>,
    /// The focus of the rows of the node list: Up, Down, Left, Right and Enter work there.
    list_focus: FocusHandle,
    /// The focus of the button "Add node": Escape in the popover gives the focus back to it.
    add_focus: FocusHandle,
}

/// A color of the palette as 0xRRGGBB, for the canvas.
fn rgb_of(color: Hsla) -> u32 {
    let rgb = color.to_rgb();
    let byte = |channel: f32| (channel * 255.0).round() as u32;
    (byte(rgb.r) << 16) | (byte(rgb.g) << 8) | byte(rgb.b)
}

/// A length in units of the canvas. The canvas scales the rem size of a node with its zoom.
fn units(value: f32) -> Rems {
    rems(value / 16.)
}

/// A kind that the user drags from the node list.
struct DraggedKind {
    kind: Kind,
    label: SharedString,
}

/// What follows the pointer during a drag from the node list.
struct KindGhost(SharedString);

impl Render for KindGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .px(px(12.))
            .py(px(6.))
            .rounded(px(9.))
            .bg(gpui::black().opacity(0.7))
            .text_color(gpui::white())
            .child(self.0.clone())
    }
}

/// The canvas element on a state, with the colors of the theme and the node renderer.
fn new_canvas(
    flow: &Entity<FlowState>,
    look: &Rc<RefCell<CanvasLook>>,
    p: &Palette,
    cx: &mut Context<ScreenView>,
) -> Entity<FlowGraph> {
    let (flow, look) = (flow.clone(), look.clone());
    let (view, grid, card, pin, accent) = (
        rgb_of(p.view),
        rgb_of(p.border),
        rgb_of(p.card),
        rgb_of(p.fg_dim),
        rgb_of(p.accent_bg),
    );
    cx.new(|cx| {
        FlowGraph::new(flow, cx)
            .no_node_chrome()
            .bg_color(view)
            .grid_color(grid)
            .node_bg_color(card)
            .node_border_color(pin)
            .edge_color(pin)
            .accent_color(accent)
            // A pin that refuses the connection of a drag has no highlight.
            .danger_color(pin)
            .default_renderer(move |node, _, _| node_element(node, &look.borrow()))
    })
}

/// One row of a node: its height in units, and its content.
fn node_row(height: f32) -> Div {
    h_flex()
        .w_full()
        .h(units(height))
        .flex_none()
        .gap(units(4.))
        .items_center()
}

/// The band of a Limiter node: the scale, the band and the marker of the input.
fn band_row(face: &NodeFace, p: &Palette) -> Div {
    let mut scale = div().relative().w_full().h(units(GUIDE_ROW)).flex_none();
    let Some(band) = &face.band else {
        return scale;
    };
    let (start, end) = (band.place(band.min), band.place(band.max));
    scale = scale
        .child(
            div()
                .absolute()
                .left_0()
                .right_0()
                .top(units(GUIDE_ROW / 2.))
                .h(px(1.))
                .bg(p.border_strong),
        )
        .child(
            div()
                .absolute()
                .left(gpui::relative(start.min(end)))
                .w(gpui::relative((end - start).abs()))
                .top(units(2.))
                .h(units(GUIDE_ROW - 4.))
                .bg(p.accent_tint)
                .border_1()
                .border_color(p.accent),
        );
    if let Some(input) = band.input {
        scale = scale.child(
            div()
                .absolute()
                .left(gpui::relative(band.place(input)))
                .top_0()
                .w(units(2.))
                .h(units(GUIDE_ROW))
                .bg(p.fg),
        );
    }
    scale
}

/// The content of one node of the canvas. The canvas adds the pins and the place.
fn node_element(node: &FlowNode, look: &CanvasLook) -> AnyElement {
    let p = Palette::new(look.dark);
    let Some(face) = node
        .id
        .parse::<u32>()
        .ok()
        .and_then(|id| look.faces.get(&id))
    else {
        return div().into_any_element();
    };
    // The ring of a selection or of a broken rule is outside the outline, so the node does not
    // move.
    let ring = if node.selected {
        Some(p.accent_bg)
    } else if face.breaks_rule {
        Some(p.warning)
    } else {
        None
    };
    let ring = ring.map(|color| {
        div()
            .absolute()
            .top(px(-3.))
            .left(px(-3.))
            .right(px(-3.))
            .bottom(px(-3.))
            .rounded(units(11.))
            .border_2()
            .border_color(color)
    });
    let frame = div()
        .relative()
        .w(units(face.width))
        .h(units(face.height))
        .rounded(units(9.))
        .text_color(p.fg)
        .text_size(units(12.))
        .line_height(units(16.));
    if let Some(text) = &face.comment {
        return frame
            .bg(p.hover)
            .p(units(8.))
            .overflow_hidden()
            .text_color(p.fg_dim)
            .child(text.clone())
            .children(ring)
            .into_any_element();
    }

    let mut title = node_row(TITLE_ROW)
        .text_size(units(14.))
        .line_height(units(20.))
        .children(face.warns.then(|| small_icon(NcIcon::Warn, p.warning)))
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(FontWeight::BOLD)
                .child(face.title.clone()),
        );
    if let Some(detail) = &face.detail {
        title = title.child(div().flex_none().text_color(p.fg_dim).child(detail.clone()));
    }
    if let Some(tag) = &face.tag {
        title = title.child(div().flex_1()).child(
            div()
                .flex_none()
                .px(units(6.))
                .rounded_full()
                .bg(p.accent_tint)
                .text_color(p.accent)
                .text_size(units(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .child(tag.clone()),
        );
    }
    let mut rows = v_flex()
        .size_full()
        .px(units(8.))
        .py(units(8.))
        .child(title);
    if let Some(subject) = &face.subject {
        let text = div().min_w_0().truncate().child(subject.clone());
        rows = rows.child(node_row(SUBJECT_ROW).child(if face.subject_mono {
            text.font_family("Adwaita Mono")
        } else {
            text
        }));
    }
    if let Some(note) = &face.note {
        rows = rows.child(
            node_row(CAPTION_ROW).child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(p.warning)
                    .child(note.clone()),
            ),
        );
    }
    let ink = if face.dim { p.fg_dim } else { p.fg };
    rows = rows.child(
        node_row(VALUE_ROW)
            .children(face.label.clone().map(|label| {
                div()
                    .min_w_0()
                    .truncate()
                    .font_family("Adwaita Mono")
                    .text_color(p.fg_dim)
                    .child(label)
            }))
            .child(div().flex_1())
            .child(
                div()
                    .flex_none()
                    .text_size(units(16.))
                    .line_height(units(22.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ink)
                    .child(face.value.clone()),
            )
            .children(
                face.unit
                    .clone()
                    .map(|unit| div().flex_none().text_color(p.fg_dim).child(unit)),
            ),
    );
    if let Some(state) = &face.state {
        rows = rows.child(
            node_row(CAPTION_ROW).child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(p.fg_dim)
                    .child(state.clone()),
            ),
        );
    }
    if face.band.is_some() {
        rows = rows.child(band_row(face, &p));
    }
    for pin in &face.pins {
        rows = rows.child(
            node_row(PIN_ROW)
                .pl(units(4.))
                .text_color(p.fg_dim)
                .child(pin.name.clone()),
        );
    }
    frame
        .bg(p.card)
        .border_1()
        .border_color(p.border_strong)
        .when(node.dashed, |frame| frame.border_dashed())
        .when(!p.dark, |frame| frame.shadow_sm())
        .child(rows)
        .children(ring)
        .into_any_element()
}

impl ScreenView {
    /// Close the editor: its draft, its canvas and its text inputs go.
    pub(crate) fn close_graph_editor(&mut self) {
        if self.graph_editor.take().is_some() {
            self.inputs
                .retain(|key, _| !matches!(key, InputKey::Graph(_)));
        }
    }

    /// Keep the editor in step with the Graph that the Graphs screen opened: open it, open it on
    /// another Graph, or close it.
    fn sync_graph_editor(
        &mut self,
        shown: &GraphView,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let wanted = self.graphs.editor.as_ref().map(|graph| graph.id.clone());
        let open = self
            .graph_editor
            .as_ref()
            .map(|host| host.ui.draft.graph().id.clone());
        if wanted == open {
            return;
        }
        self.close_graph_editor();
        let Some(graph) = self.graphs.editor.clone() else {
            return;
        };
        let flow = cx.new(|_| new_flow_state(&Shape::default()));
        let look = Rc::new(RefCell::new(CanvasLook::default()));
        let canvas = new_canvas(&flow, &look, p, cx);
        cx.subscribe_in(
            &flow,
            window,
            |this: &mut Self, _, event: &FlowEvent, window, cx| {
                this.on_flow_event(event.clone(), window, cx);
            },
        )
        .detach();
        self.graph_editor = Some(EditorHost {
            ui: EditorUi::open(graph, shown),
            flow,
            canvas,
            look,
            shape: Shape::default(),
            fitted: false,
            dark: p.dark,
            events: Vec::new(),
            list_focus: cx.focus_handle().tab_stop(true),
            add_focus: cx.focus_handle().tab_stop(true),
        });
    }

    /// An event of the canvas. The events of one input come one by one: they wait here, and the
    /// end of the input applies them as one undo step.
    fn on_flow_event(&mut self, event: FlowEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.graph_editor.as_mut() else {
            return;
        };
        host.events.push(event);
        if host.events.len() == 1 {
            cx.defer_in(window, Self::flush_flow_events);
        }
    }

    fn flush_flow_events(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(host) = self.graph_editor.as_mut() {
            let events = std::mem::take(&mut host.events);
            host.ui.canvas_events(&events, now_us());
        }
        cx.notify();
    }

    /// Give the canvas the nodes and the connections of the draft, the selection, and the
    /// faces that its node renderer draws.
    fn sync_canvas(&mut self, model: &EditorModel, p: &Palette, cx: &mut Context<Self>) {
        let Some(host) = self.graph_editor.as_mut() else {
            return;
        };
        {
            let mut look = host.look.borrow_mut();
            look.faces = model
                .nodes
                .iter()
                .map(|face| (face.id, face.clone()))
                .collect();
            look.dark = p.dark;
        }
        if host.dark != p.dark {
            host.dark = p.dark;
            host.canvas = new_canvas(&host.flow, &host.look, p, cx);
        }
        let shape = shape_of(&model.nodes, &model.edges);
        let changed = shape != host.shape;
        let selection = flow_selection(host.ui.selected.as_ref());
        let fit = !host.fitted;
        let fitted = host.flow.update(cx, |state, cx| {
            let mut touched = false;
            if changed {
                state.set_graph(flow_nodes(&shape), flow_edges(&shape));
                touched = true;
            }
            if state.selection() != selection {
                state.set_selection(selection);
                touched = true;
            }
            // The first paint gives the size of the canvas.
            let size = state.size;
            let fitted = fit && size.0 > 0.0 && size.1 > 0.0;
            if fitted {
                opening_view(state, size);
                touched = true;
            }
            if touched {
                cx.notify();
            }
            fitted
        });
        host.fitted |= fitted;
        host.shape = shape;
    }

    /// The page of the Graph editor.
    pub(crate) fn render_graph_editor(
        &mut self,
        shown: &GraphView,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.sync_graph_editor(shown, p, window, cx);
        let app = self.model.clone();
        let width = f32::from(window.viewport_size().width);
        let Some(host) = self.graph_editor.as_mut() else {
            return div().into_any_element();
        };
        host.ui.set_width(width);
        let (zoom, centre) = {
            let state = host.flow.read(cx);
            (state.viewport.zoom, view_centre(state))
        };
        host.ui.zoom = zoom;
        host.ui.view_centre = centre;
        let model = with_screen_input(&app, cx, |input| EditorModel::build(input, shown, &host.ui));
        self.sync_canvas(&model, p, cx);

        let view = cx.entity();
        let toolbar = self.graph_toolbar(&model, p, window, cx);
        let list = (model.list.form == ListForm::Column).then(|| {
            v_flex()
                .w(px(LIST_WIDTH))
                .flex_none()
                .border_r_1()
                .border_color(p.border)
                .bg(p.headerbar)
                .child(self.graph_node_list(&model.list, p, window, cx))
        });
        let canvas = self.graph_canvas(&model, p, cx);
        v_flex()
            .id("graph-editor")
            .size_full()
            .debug_selector(|| "graph-editor".to_owned())
            .on_key_down({
                let view = view.clone();
                move |event: &KeyDownEvent, _, cx| {
                    let handled = view.update(cx, |this, cx| this.graph_editor_key(event, cx));
                    if handled {
                        cx.stop_propagation();
                    }
                }
            })
            .child(toolbar)
            // A plain row stretches its children to its height. `h_flex` centers them.
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .children(list)
                    .child(canvas),
            )
            .into_any_element()
    }

    /// A key of the editor: undo, redo and the zoom of the canvas. Returns whether the editor
    /// took the key.
    fn graph_editor_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let keys = &event.keystroke;
        let editing = self.edited.is_some();
        let Some(key) = editor_key(
            keys.key.as_str(),
            keys.modifiers.control,
            keys.modifiers.shift,
            editing,
        ) else {
            return false;
        };
        let Some(host) = self.graph_editor.as_mut() else {
            return false;
        };
        match key {
            EditorKey::ZoomIn | EditorKey::ZoomOut | EditorKey::ShowAll => {
                host.flow.update(cx, |state, cx| {
                    let size = state.size;
                    match key {
                        EditorKey::ZoomIn => state.zoom_in(size.0, size.1),
                        EditorKey::ZoomOut => state.zoom_out(size.0, size.1),
                        _ => opening_view(state, size),
                    }
                    cx.notify();
                });
            }
            EditorKey::Undo | EditorKey::Redo => {
                host.ui.press(key);
            }
        }
        cx.notify();
        true
    }

    /// The bar above the canvas: back, the name, the draft state, undo, redo, the first broken
    /// rule, "Try" and "Apply".
    fn graph_toolbar(
        &mut self,
        model: &EditorModel,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let bar = &model.toolbar;
        let name = self.input(
            InputKey::Graph("name".to_owned()),
            &bar.name,
            "",
            window,
            cx,
        );
        let step = |id: &'static str, glyph: NcIcon, key: EditorKey, action: &super::Action| {
            Self::icon_button(id, glyph)
                .disabled(!action.enabled)
                .tip(action.label.clone())
                .on_click(act(&view, move |this, _, _| {
                    if let Some(host) = this.graph_editor.as_mut() {
                        host.ui.press(key);
                    }
                }))
        };
        let add = bar.add_node.as_ref().map(|action| {
            let add_focus = self
                .graph_editor
                .as_ref()
                .map(|host| host.add_focus.clone());
            // Escape in the popover gives the focus back to the button.
            if let Some(host) = self.graph_editor.as_mut()
                && std::mem::take(&mut host.ui.list.focus_button)
            {
                window.focus(&host.add_focus, cx);
            }
            let mut button =
                Self::small_button("graph-add-node", action.label.clone(), Some(NcIcon::Plus))
                    .ghost()
                    .selected(model.list.open)
                    .on_click(act(&view, |this, _, _| {
                        if let Some(host) = this.graph_editor.as_mut() {
                            host.ui.toggle_list();
                        }
                    }));
            if let Some(focus) = &add_focus {
                button = button.track_focus(focus);
            }
            let list = model.list.open.then(|| {
                popover(p)
                    .w(px(POPOVER_WIDTH))
                    .overflow_hidden()
                    .debug_selector(|| "graph-node-popover".to_owned())
                    .child(self.graph_node_list(&model.list, p, window, cx))
                    .into_any_element()
            });
            let outside = view.clone();
            with_popover(button, list, move |_, _, cx| {
                outside.update(cx, |this, cx| {
                    if let Some(host) = this.graph_editor.as_mut() {
                        host.ui.press_outside_list();
                    }
                    cx.notify();
                });
            })
            .flex_none()
        });
        h_flex()
            .w_full()
            .h(px(46.))
            .flex_none()
            .px(px(12.))
            .gap(px(9.))
            .items_center()
            .border_b_1()
            .border_color(p.border)
            .bg(p.headerbar)
            .child(
                Self::small_button("graph-editor-back", bar.back.clone(), Some(NcIcon::Left))
                    .ghost()
                    .on_click(act(&view, |this, _, _| {
                        this.graphs.editor = None;
                    })),
            )
            .child(
                div()
                    .w(px(240.))
                    .flex_none()
                    .child(Self::field_input(&name, None, false, p)),
            )
            .children(
                bar.draft_state
                    .clone()
                    .map(|text| caption(text, p).flex_none()),
            )
            .children(add)
            .child(div().flex_1())
            .child(step("graph-undo", NcIcon::Undo, EditorKey::Undo, &bar.undo))
            .child(step("graph-redo", NcIcon::Redo, EditorKey::Redo, &bar.redo))
            .children(bar.rule.clone().map(|text| {
                h_flex()
                    .min_w_0()
                    .gap(px(6.))
                    .items_center()
                    .child(icon(NcIcon::Warn, p.warning))
                    .child(label(text))
            }))
            .child(
                Self::small_button("graph-try", bar.try_run.label.clone(), None)
                    .disabled(!bar.try_run.enabled),
            )
            .child(
                Self::small_button("graph-apply", bar.apply.label.clone(), None)
                    .primary()
                    .disabled(!bar.apply.enabled),
            )
            .into_any_element()
    }

    /// The node list: the search field, then the kinds by family. The column and the popover
    /// draw the same list.
    fn graph_node_list(
        &mut self,
        list: &NodeList,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        let locale = self.model.read(cx).locale;
        let typed = self
            .graph_editor
            .as_ref()
            .map(|host| host.ui.list.search.clone())
            .unwrap_or_default();
        let search = self.input(
            InputKey::Graph("search".to_owned()),
            &typed,
            &list.search,
            window,
            cx,
        );
        let search_focus = gpui::Focusable::focus_handle(search.read(cx), cx);
        let Some(host) = self.graph_editor.as_mut() else {
            return div().into_any_element();
        };
        if std::mem::take(&mut host.ui.list.focus_search) {
            window.focus(&search_focus, cx);
        }
        let list_focus = host.list_focus.clone();
        let in_popover = list.form == ListForm::Popover;
        let keys = {
            let (view, search_focus) = (view.clone(), search_focus.clone());
            move |event: &KeyDownEvent, window: &mut Window, cx: &mut App| {
                if event.keystroke.modifiers.modified() {
                    return;
                }
                let Some(key) = ListKey::parse(event.keystroke.key.as_str()) else {
                    return;
                };
                let in_search = search_focus.is_focused(window);
                // Left and Right move the caret of the search field. Escape closes the popover
                // only: in the column it goes on to the field that has an open edit.
                if (in_search && matches!(key, ListKey::Left | ListKey::Right))
                    || (key == ListKey::Escape && !in_popover)
                {
                    return;
                }
                cx.stop_propagation();
                view.update(cx, |this, cx| {
                    if let Some(host) = this.graph_editor.as_mut() {
                        host.ui.list_key(key, locale);
                    }
                    cx.notify();
                });
            }
        };
        // The column gives the rows the height that is left. The popover has no height of its
        // own: its rows have the limit.
        let mut rows = v_flex()
            .id("graph-kinds")
            .overflow_y_scroll()
            .track_focus(&list_focus)
            .pb(px(6.))
            .map(|rows| {
                if in_popover {
                    rows.max_h(px(POPOVER_MAX_HEIGHT - SEARCH_HEIGHT))
                } else {
                    rows.flex_1().min_h_0()
                }
            });
        for family in &list.families {
            let folds = family.family;
            rows = rows.child(
                h_flex()
                    .id(SharedString::from(format!(
                        "graph-family-{}",
                        family_key(family.family)
                    )))
                    .h(px(28.))
                    .px(px(9.))
                    .gap(px(6.))
                    .items_center()
                    .cursor_pointer()
                    .text_size(px(12.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(p.fg_dim)
                    .child(small_icon(
                        if family.open {
                            NcIcon::Down
                        } else {
                            NcIcon::Right
                        },
                        p.fg_dim,
                    ))
                    .child(label(family.name.clone()).flex_1())
                    .child(
                        div()
                            .flex_none()
                            .font_weight(FontWeight::NORMAL)
                            .child(family.count.to_string()),
                    )
                    .on_click(act(&view, move |this, _, _| {
                        if let Some(host) = this.graph_editor.as_mut() {
                            host.ui.toggle_family(folds);
                        }
                    })),
            );
            if family.open {
                for row in &family.rows {
                    rows = rows.child(Self::graph_kind_row(row, &view, p));
                }
            }
        }
        if let Some(text) = &list.empty {
            rows = rows.child(div().p(px(12.)).child(caption(text.clone(), p)));
        }
        v_flex()
            .w_full()
            .when(!in_popover, |list| list.flex_1().min_h_0())
            .capture_key_down(keys)
            .child(div().p(px(9.)).flex_none().child(
                Self::field_input(&search, None, true, p).prefix(icon(NcIcon::Search, p.fg_dim)),
            ))
            .child(rows)
            .into_any_element()
    }

    /// One kind of the node list. A double click adds a node at the centre of the view. In the
    /// column form a drag onto the canvas adds a node where the pointer lets it go.
    fn graph_kind_row(row: &ListRow, view: &Entity<Self>, p: &Palette) -> AnyElement {
        let kind = row.kind;
        let line = h_flex()
            .id(SharedString::from(format!("graph-kind-{}", kind.as_str())))
            .h(px(28.))
            .flex_none()
            .pl(px(27.))
            .pr(px(9.))
            .items_center()
            .child(label(row.name.clone()))
            .tip(row.tooltip.clone());
        if !row.enabled {
            return line.opacity(0.5).into_any_element();
        }
        let hover = p.hover;
        let line = line
            .cursor_pointer()
            .hover(move |line| line.bg(hover))
            .when(row.marked, |line| line.bg(hover))
            .on_click({
                let view = view.clone();
                move |event, _, cx| {
                    if event.click_count() == 2 {
                        view.update(cx, |this, cx| {
                            if let Some(host) = this.graph_editor.as_mut() {
                                host.ui.list_double_click(kind);
                            }
                            cx.notify();
                        });
                    }
                }
            });
        if !row.draggable {
            return line.into_any_element();
        }
        let drag = DraggedKind {
            kind,
            label: row.name.clone().into(),
        };
        line.on_drag(drag, |dragged, _, _, cx| {
            cx.new(|_| KindGhost(dragged.label.clone()))
        })
        .into_any_element()
    }

    /// A text input of the editor. `field` names the control.
    pub(crate) fn on_graph_input(
        &mut self,
        field: &str,
        value: &str,
        changes: bool,
        commits: bool,
    ) {
        let Some(host) = self.graph_editor.as_mut() else {
            return;
        };
        match field {
            "search" if changes => {
                host.ui.list.search = value.to_owned();
                host.ui.list.cursor = None;
            }
            "name" if commits => {
                host.ui.draft.set_name(value.trim());
            }
            _ => {}
        }
    }

    /// A row of the node list that the user lets go on the canvas.
    fn drop_graph_kind(&mut self, kind: Kind, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(host) = self.graph_editor.as_mut() else {
            return;
        };
        let point = canvas_point(host.flow.read(cx), position);
        host.ui.drop_from_list(kind, point);
    }

    /// A key on the canvas, before the canvas gets it. An arrow key selects the nearest node in
    /// that direction, and Alt with an arrow key moves the selected node. Escape clears the
    /// selection. Returns whether the editor took the key.
    fn graph_canvas_key(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) -> bool {
        let keys = &event.keystroke;
        let Some(host) = self.graph_editor.as_mut() else {
            return false;
        };
        let direction = match keys.key.as_str() {
            "left" => (-1, 0),
            "right" => (1, 0),
            "up" => (0, -1),
            "down" => (0, 1),
            "escape" => {
                host.ui.selected = None;
                cx.notify();
                // The canvas cancels its drag with the same key.
                return false;
            }
            _ => return false,
        };
        let modifiers = keys.modifiers;
        if modifiers.control || modifiers.platform || modifiers.shift {
            return false;
        }
        if modifiers.alt {
            host.ui.nudge_selected(direction);
        } else if host.ui.select_towards(direction)
            && let Some(Selected::Node(id)) = host.ui.selected
            && let Some(node) = host.ui.draft.graph().node(id)
        {
            // The view follows the selection.
            let (x, y) = (node.x as f32, node.y as f32);
            host.flow.update(cx, |state, cx| {
                let size = state.size;
                state.set_center(x, y, size.0, size.1);
                cx.notify();
            });
        }
        cx.notify();
        true
    }

    /// The canvas with the text of a new Graph over it, and the status line under it.
    fn graph_canvas(&mut self, model: &EditorModel, p: &Palette, cx: &mut Context<Self>) -> Div {
        let view = cx.entity();
        let Some(host) = self.graph_editor.as_ref() else {
            return div();
        };
        let canvas_focus = host.canvas.read(cx).focus_handle().clone();
        let zoom = |id: &'static str, glyph: NcIcon, tip: &str, zoom_in: bool| {
            let flow = host.flow.clone();
            Self::icon_button(id, glyph)
                .tip(tip.to_owned())
                .on_click(move |_, _, cx| {
                    flow.update(cx, |state, cx| {
                        let size = state.size;
                        if zoom_in {
                            state.zoom_in(size.0, size.1);
                        } else {
                            state.zoom_out(size.0, size.1);
                        }
                        cx.notify();
                    });
                })
        };
        let status = h_flex()
            .w_full()
            .h(px(28.))
            .flex_none()
            .px(px(12.))
            .gap(px(6.))
            .items_center()
            .border_t_1()
            .border_color(p.border)
            .bg(p.headerbar)
            .text_size(px(13.))
            .children(model.status.text.clone().map(|text| {
                h_flex()
                    .min_w_0()
                    .gap(px(6.))
                    .items_center()
                    .child(icon(NcIcon::Info, p.fg_dim))
                    .child(label(text))
            }))
            .child(div().flex_1())
            .child(zoom(
                "graph-zoom-out",
                NcIcon::Minus,
                &model.status.zoom_out,
                false,
            ))
            .child(
                div()
                    .flex_none()
                    .font_weight(FontWeight::BOLD)
                    .child(model.status.zoom.clone()),
            )
            .child(zoom(
                "graph-zoom-in",
                NcIcon::Plus,
                &model.status.zoom_in,
                true,
            ));
        let area = div()
            .id("graph-canvas")
            .relative()
            .flex_1()
            .min_h_0()
            .w_full()
            .overflow_hidden()
            .capture_key_down({
                let view = view.clone();
                move |event: &KeyDownEvent, window, cx| {
                    if !canvas_focus.is_focused(window) {
                        return;
                    }
                    let handled = view.update(cx, |this, cx| this.graph_canvas_key(event, cx));
                    if handled {
                        cx.stop_propagation();
                    }
                }
            })
            .on_drop::<DraggedKind>({
                let view = view.clone();
                move |dragged, window, cx| {
                    let (kind, position) = (dragged.kind, window.mouse_position());
                    view.update(cx, |this, cx| {
                        this.drop_graph_kind(kind, position, cx);
                        cx.notify();
                    });
                }
            })
            .child(host.canvas.clone())
            .children(model.hint.clone().map(|text| {
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(p.fg_dim)
                    .child(text)
            }));
        v_flex().flex_1().min_w_0().child(area).child(status)
    }
}
