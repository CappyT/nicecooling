//! The host of the screens of tasks 8.5 to 8.9. It owns the state that the user changes
//! (selections, drafts, open popovers), builds the model of the page and draws it.

use std::cell::Cell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, Context, Entity, Focusable as _, KeyDownEvent, Pixels, Render,
    SharedString, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::slider::{SliderEvent, SliderState};
use gpui_component::{ActiveTheme as _, Theme, v_flex};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::{AppModel, now_us};
use crate::command::Command;
use crate::history::HistoryDb;
use crate::model::AppState;
use crate::screens::ScreenInput;
use crate::screens::alarms::AlarmsUi;
use crate::screens::channels::ChannelsUi;
use crate::screens::first_run::FirstRunUi;
use crate::screens::graphs::GraphsUi;
use crate::screens::history::{ChartData, HistoryUi};
use crate::screens::leakshield::LeakshieldUi;
use crate::screens::loops::LoopsUi;
use crate::screens::profiles::ProfilesUi;
use crate::screens::sensors::SensorsUi;
use crate::shell::Page;
use crate::tokens::Palette;

/// What a text input of a screen means.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InputKey {
    SensorSearch,
    SensorRename,
    DerivedName,
    ChannelRename,
    LoopRename,
    HistorySearch,
    PickerSearch(String),
    /// The Fixed Duty of a Channel.
    FixedDuty(String),
    /// `(channel, point, 0 for the Input, 1 for the Duty)`.
    CurvePoint(String, usize, usize),
    CalibrationCell(String, usize, usize),
    ProfileRename,
    RuleDelay,
    ProfileDwell,
    /// A field of the rule editor, by its API key.
    RuleField(&'static str),
    /// The delay of an alarm response, by event.
    ResponseDelay(String),
    /// An input of the Leakshield page. The first part of the key names the control.
    Ls(String),
    /// An input of the group "Network" of the Settings screen.
    Network(crate::screens::settings::NetworkField),
    /// An input of the Graph editor. The first part of the key names the control.
    Graph(String),
}

use crate::screens::history::{PROGRESS_DELAY, progress_due};

type HistoryKey = (Vec<String>, crate::screens::history::Range, i64, i64);

/// The data of the History chart for one window.
pub struct HistoryCache {
    key: HistoryKey,
    loaded_at: i64,
    data: ChartData,
}

/// A drag in a plot: the point that moves.
#[derive(Clone, Copy, Debug)]
pub enum Drag {
    CurvePoint(usize),
    /// A drag in the History chart selects a time range: `start` is the fraction of the width.
    HistoryZoom {
        start: f32,
    },
    /// Shift and drag pans the History chart.
    HistoryPan {
        start: f32,
        from: i64,
        to: i64,
    },
}

pub struct ScreenView {
    pub(crate) model: Entity<AppModel>,
    pub(crate) commands: UnboundedSender<Command>,
    pub(crate) sensors: SensorsUi,
    pub(crate) channels: ChannelsUi,
    pub(crate) loops: LoopsUi,
    pub(crate) history: HistoryUi,
    pub(crate) profiles: ProfilesUi,
    pub(crate) graphs: GraphsUi,
    /// The question of "Delete" or "Remove" showed in the last frame.
    pub(crate) graphs_dialog_shown: bool,
    /// The Graph editor, while it is open on the Graph of `graphs.editor`.
    pub(crate) graph_editor: Option<crate::screens::graph_editor::view::EditorHost>,
    pub(crate) alarms: AlarmsUi,
    pub(crate) first_run: FirstRunUi,
    pub(crate) leakshield: LeakshieldUi,
    pub(crate) network: crate::screens::settings::NetworkUi,
    /// The dim text that each password input of the group "Network" shows now.
    pub(crate) password_hints: HashMap<crate::screens::settings::NetworkField, String>,
    /// The focus of the Fields table of the Leakshield page: Up and Down move the selection.
    pub(crate) ls_focus: Option<gpui::FocusHandle>,
    /// The focus of the Rule group list: Alt+Up and Alt+Down move the selected group.
    pub(crate) groups_focus: Option<gpui::FocusHandle>,
    /// The focus of the Channels list: Up and Down move the selection.
    pub(crate) channels_focus: Option<gpui::FocusHandle>,
    pub(crate) inputs: HashMap<InputKey, Entity<InputState>>,
    /// The input with text that the user typed and did not commit. It keeps that text
    /// without the focus, until the commit that the end of the frame starts.
    pub(crate) edited: Option<InputKey>,
    /// True when this frame has `edited` with the focus.
    edited_focused: bool,
    /// The input that Escape puts back to the value of the model.
    reverted: Option<InputKey>,
    /// The input of a dialog that takes the focus when the dialog is drawn.
    pub(crate) focus_input: Option<InputKey>,
    pub(crate) sliders: HashMap<String, Entity<SliderState>>,
    /// The sliders that the user drags now: they keep their value until the release.
    pub(crate) busy_sliders: BTreeSet<String>,
    /// The bounds of the plot that a drag reads, as the last frame painted them.
    pub(crate) plot_bounds: Rc<Cell<Bounds<Pixels>>>,
    pub(crate) drag: Option<Drag>,
    /// The focus of the Curve plot: it takes the keys of the Curve editor.
    pub(crate) plot_focus: Option<gpui::FocusHandle>,
    pub(crate) history_db: Option<HistoryDb>,
    /// The data of the chart, with the second it was read. The chart does not read the
    /// database at every frame.
    pub(crate) history_cache: Option<HistoryCache>,
    /// The read that runs now, and when it started.
    pub(crate) history_pending: Option<(HistoryKey, std::time::Instant)>,
    /// The pointer in the History chart, as a fraction of its width.
    pub(crate) hover: Option<f32>,
    pub(crate) chart_focus: Option<gpui::FocusHandle>,
    /// The focus of the Sensors table: Enter on a selected row opens History.
    pub(crate) sensors_focus: Option<gpui::FocusHandle>,
    pub(crate) history_error: Option<String>,
    /// The sensor picker of the Curve editor, History and the Derived sensor editor.
    pub(crate) open_picker: Option<String>,
    /// Where the press was that closed the popovers, and the sensor of the rename popover
    /// that it closed.
    pub(crate) closing_press: Option<(gpui::Point<gpui::Pixels>, Option<String>)>,
    pub(crate) preset_menu_open: bool,
    /// The focus of each select of `select_field`, by its key.
    pub(crate) select_focus: HashMap<String, gpui::FocusHandle>,
    /// What has the focus in this frame.
    pub(crate) focused: Option<gpui::FocusHandle>,
    /// The row that the keyboard marks in the open list of a select or a picker: the key of
    /// the list and the row.
    pub(crate) picker_cursor: Option<(String, usize)>,
    /// A picker that the keyboard opened: its key, the select that gets the focus back, and
    /// whether the search input took the focus.
    pub(crate) picker_keys: Option<(String, gpui::FocusHandle, bool)>,
}

impl ScreenView {
    pub fn new(
        model: Entity<AppModel>,
        commands: UnboundedSender<Command>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        // Hiding the window drops this view. Its Graph editor goes to the app model with its
        // draft, and the view of the next window opens it again.
        cx.on_release(|this, cx| {
            if let Some(host) = this.graph_editor.take() {
                this.model
                    .update(cx, |app, _| app.parked_editor = Some(host.ui));
            }
        })
        .detach();
        let graphs = GraphsUi {
            editor: model
                .read(cx)
                .parked_editor
                .as_ref()
                .map(|ui| ui.draft.graph().clone()),
            ..GraphsUi::default()
        };
        let sensors = SensorsUi {
            hidden: model.read(cx).settings.values.hidden_sensors(),
            ..SensorsUi::default()
        };
        Self {
            model,
            commands,
            sensors,
            channels: ChannelsUi::default(),
            loops: LoopsUi::default(),
            history: HistoryUi::default(),
            profiles: ProfilesUi::default(),
            graphs,
            graphs_dialog_shown: false,
            graph_editor: None,
            alarms: AlarmsUi::default(),
            first_run: FirstRunUi::default(),
            leakshield: LeakshieldUi::default(),
            network: Default::default(),
            password_hints: HashMap::new(),
            ls_focus: Some(cx.focus_handle()),
            groups_focus: Some(cx.focus_handle()),
            channels_focus: Some(cx.focus_handle()),
            inputs: HashMap::new(),
            sliders: HashMap::new(),
            busy_sliders: BTreeSet::new(),
            plot_bounds: Rc::new(Cell::new(Bounds::default())),
            drag: None,
            plot_focus: Some(cx.focus_handle()),
            history_db: None,
            history_cache: None,
            history_pending: None,
            hover: None,
            chart_focus: Some(cx.focus_handle()),
            sensors_focus: Some(cx.focus_handle()),
            history_error: None,
            open_picker: None,
            closing_press: None,
            preset_menu_open: false,
            select_focus: HashMap::new(),
            edited: None,
            edited_focused: false,
            reverted: None,
            focus_input: None,
            focused: None,
            picker_cursor: None,
            picker_keys: None,
        }
    }

    /// Open History with one sensor selected: Enter on a row of the Sensors screen.
    pub fn open_history_for(&mut self, sensor: &str, cx: &mut Context<Self>) {
        self.history.selected = vec![sensor.to_owned()];
        self.history.picker_open = false;
        self.history_cache = None;
        self.go(Page::History, cx);
    }

    /// Open the editor of a Rule group of the Profiles screen: a new group for `None`. The
    /// editor of the first rule opens too, when the group has one.
    pub fn open_group_editor(&mut self, index: Option<usize>, cx: &mut Context<Self>) {
        let state = self.state(cx);
        match index {
            Some(index) => self.profiles.open_group(&state, index),
            None => self.profiles.add_group(&state),
        }
        self.profiles.edit_rule(0);
        self.go(Page::Profiles, cx);
    }

    /// Show a step of the First run flow.
    pub fn show_first_run_step(
        &mut self,
        step: crate::screens::first_run::Step,
        cx: &mut Context<Self>,
    ) {
        self.first_run.step = step;
        self.go(Page::FirstRun, cx);
    }

    /// Add a sensor to the History chart, or take it out.
    pub fn toggle_history_sensor(&mut self, sensor: &str) {
        self.history.toggle_sensor(sensor);
        self.history_cache = None;
    }

    /// Write the hidden sensors to the settings file.
    pub(crate) fn persist_hidden(&self, cx: &mut Context<Self>) {
        let hidden = self.sensors.hidden.clone();
        self.model.update(cx, |app, _| {
            app.settings
                .update(|values| values.set_hidden_sensors(&hidden));
        });
    }

    pub(crate) fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    pub(crate) fn go(&self, page: Page, cx: &mut Context<Self>) {
        self.model.update(cx, |app, cx| {
            app.request_page(page);
            cx.notify();
        });
    }

    /// A function that opens a page, for a widget that has no access to the view.
    pub(crate) fn navigator(&self) -> impl Fn(Page, &mut Window, &mut App) + 'static {
        let model = self.model.clone();
        move |page, _, cx| {
            model.update(cx, |app, cx| {
                app.request_page(page);
                cx.notify();
            });
        }
    }

    /// A copy of the state, for a handler that changes the state of a screen.
    pub(crate) fn state(&self, cx: &App) -> AppState {
        self.model.read(cx).state.clone()
    }

    /// Run `f` with the input of the screens: the state, the time and the write feedback.
    pub(crate) fn with_input<R>(&self, cx: &App, f: impl FnOnce(&ScreenInput<'_>) -> R) -> R {
        with_screen_input(&self.model, cx, f)
    }

    /// The text input of `key`. It starts with `value`. While it has no focus it follows
    /// `value`, so a drag or a daemon answer changes the text. Text that the user typed
    /// stays until its commit.
    pub(crate) fn input(
        &mut self,
        key: InputKey,
        value: &str,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(state) = self.inputs.get(&key).cloned() {
            let focused = state.read(cx).focus_handle(cx).is_focused(window);
            let edited = self.edited.as_ref() == Some(&key);
            self.edited_focused |= edited && focused;
            let reverted = self.reverted.as_ref() == Some(&key);
            if reverted {
                self.reverted = None;
            }
            if (reverted || !(focused || edited)) && state.read(cx).value().as_ref() != value {
                let value = value.to_owned();
                state.update(cx, |input, cx| input.set_value(value, window, cx));
            }
            return state;
        }
        let initial = value.to_owned();
        let placeholder = placeholder.to_owned();
        let state = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(initial)
                .placeholder(placeholder)
        });
        let subscribed = key.clone();
        cx.subscribe_in(
            &state,
            window,
            move |this: &mut Self, state, event: &InputEvent, window, cx| {
                this.on_input(&subscribed, state, event, window, cx);
            },
        )
        .detach();
        self.inputs.insert(key, state.clone());
        state
    }

    /// A slider of 0 to 100. `Release` carries the value that the user chose.
    pub(crate) fn slider(
        &mut self,
        key: &str,
        value: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SliderState> {
        if let Some(state) = self.sliders.get(key) {
            if !self.busy_sliders.contains(key)
                && (state.read(cx).value().start() - value).abs() > 0.1
            {
                state.update(cx, |slider, cx| slider.set_value(value, window, cx));
            }
            return state.clone();
        }
        let state = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(value)
        });
        let name = key.to_owned();
        cx.subscribe(
            &state,
            move |this: &mut Self, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(value) => {
                    this.busy_sliders.insert(name.clone());
                    // The import dialog shows the result while the slider moves.
                    if name == crate::screens::leakshield::display_view::THRESHOLD_SLIDER {
                        this.on_slider(&name, value.start(), cx);
                    }
                }
                SliderEvent::Release(value) => {
                    this.busy_sliders.remove(&name);
                    this.on_slider(&name, value.start(), cx);
                }
            },
        )
        .detach();
        self.sliders.insert(key.to_owned(), state.clone());
        state
    }

    /// A slider with its own range. `Release` carries the value that the user chose.
    pub(crate) fn range_slider(
        &mut self,
        key: &str,
        range: (f32, f32),
        value: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<SliderState> {
        if let Some(state) = self.sliders.get(key) {
            if !self.busy_sliders.contains(key)
                && (state.read(cx).value().start() - value).abs() > 0.01
            {
                state.update(cx, |slider, cx| slider.set_value(value, window, cx));
            }
            return state.clone();
        }
        let (min, max) = range;
        let state = cx.new(|_| {
            SliderState::new()
                .min(min)
                .max(max)
                .step(1.)
                .default_value(value)
        });
        let name = key.to_owned();
        cx.subscribe(
            &state,
            move |this: &mut Self, _, event: &SliderEvent, cx| match event {
                SliderEvent::Change(_) => {
                    this.busy_sliders.insert(name.clone());
                }
                SliderEvent::Release(value) => {
                    this.busy_sliders.remove(&name);
                    this.on_slider(&name, value.start(), cx);
                }
            },
        )
        .detach();
        self.sliders.insert(key.to_owned(), state.clone());
        state
    }

    /// The data of the History chart. A live window reads again after 2 s, a fixed window
    /// when it changes. The read runs in the background: the axes show at once and the lines
    /// appear when the query ends.
    pub(crate) fn history_data(&mut self, cx: &mut Context<Self>) -> ChartData {
        let Some(path) = self.model.read(cx).history_path.clone() else {
            return ChartData::Loaded(Vec::new());
        };
        let now = (now_us() / 1_000_000) as i64;
        let (from, to) = self.history.window(now);
        let live = self.history.follows_now();
        let key: HistoryKey = (
            self.history.selected.clone(),
            self.history.range,
            if live { 0 } else { from },
            if live { 0 } else { to },
        );
        if let Some(cache) = &self.history_cache
            && cache.key == key
            && (!live || now - cache.loaded_at < 2)
        {
            return cache.data.clone();
        }
        if self
            .history_pending
            .as_ref()
            .is_none_or(|(pending, _)| *pending != key)
        {
            self.history_pending = Some((key.clone(), std::time::Instant::now()));
            let selected = self.history.selected.clone();
            let shown = path.clone();
            let task = cx.background_executor().spawn(async move {
                let db = HistoryDb::open(&shown).map_err(|error| error.to_string())?;
                selected
                    .iter()
                    .map(|id| crate::history::query::load_series(&db, id, from, to, now))
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())
            });
            let started = key.clone();
            cx.spawn(async move |this, cx| {
                let result = task.await;
                let _ = this.update(cx, |this, cx| {
                    // A newer read may have replaced this one.
                    if this.history_pending.as_ref().map(|(k, _)| k) != Some(&started) {
                        return;
                    }
                    this.history_pending = None;
                    this.history_cache = Some(HistoryCache {
                        key: started,
                        loaded_at: now,
                        data: match result {
                            Ok(series) => ChartData::Loaded(series),
                            Err(message) => ChartData::Error {
                                path: path.display().to_string(),
                                message,
                            },
                        },
                    });
                    cx.notify();
                });
            })
            .detach();
            // The bar shows after 300 ms: draw again then.
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(PROGRESS_DELAY).await;
                let _ = this.update(cx, |_, cx| cx.notify());
            })
            .detach();
        }
        match &self.history_cache {
            Some(cache) if cache.key == key => cache.data.clone(),
            _ => ChartData::Loaded(Vec::new()),
        }
    }

    /// A query that runs for 300 ms or more shows a thin progress bar under the toolbar.
    pub(crate) fn history_progress(&self) -> bool {
        self.history_pending
            .as_ref()
            .is_some_and(|(_, started)| progress_due(started.elapsed()))
    }

    /// "Try again" on a database error.
    pub(crate) fn retry_history(&mut self, cx: &mut Context<Self>) {
        self.history_db = None;
        self.history_error = None;
        self.history_cache = None;
        self.history_pending = None;
        cx.notify();
    }

    pub(crate) fn palette(cx: &App) -> Palette {
        Palette::new(Theme::global(cx).is_dark())
    }
}

impl Render for ScreenView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = self.model.read(cx).page.clone();
        let p = Self::palette(cx);
        crate::screens::ui::dialog_frame(window, cx);
        self.return_picker_focus(window, cx);
        self.focused = window.focused(cx);
        // Each page, and each tab of a Leakshield, keeps its own scroll position.
        let scroll_id = match &page {
            Page::Leakshield(_) => format!("screen-leakshield-{:?}", self.leakshield.tab),
            other => format!("screen-{}", other.title_key()),
        };
        let on_graphs = page == Page::Graphs;
        let mut strip = None;
        let content: AnyElement = match page {
            Page::Overview => self.render_overview(&p, window, cx),
            Page::Sensors => self.render_sensors(&p, window, cx),
            Page::Channels => self.render_channels(&p, window, cx),
            Page::Loops => self.render_loops(&p, window, cx),
            Page::History => self.render_history(&p, window, cx),
            Page::Profiles => self.render_profiles(&p, window, cx),
            Page::Graphs => self.render_graphs(&p, window, cx),
            Page::Alarms => self.render_alarms(&p, window, cx),
            Page::Settings => self.render_settings(&p, window, cx),
            Page::FirstRun => self.render_first_run(&p, window, cx),
            Page::Leakshield(id) => {
                let (tabs, content) = self.render_leakshield(&id, &p, window, cx);
                strip = Some(tabs);
                content
            }
        };
        // A focus change draws a frame before gpui sends `Blur`, and a window that is not
        // active gets no `Blur`. So the frame that has the edited input without the focus,
        // or not at all, starts the commit.
        if self.edited.is_some() && !std::mem::take(&mut self.edited_focused) {
            cx.defer_in(window, Self::leave_input);
        }
        // The Graph editor takes the height of the window: its canvas pans, and the page does
        // not scroll.
        let fills = on_graphs && self.graph_editor.is_some();
        let body = if fills {
            div()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(content)
                .into_any_element()
        } else {
            v_flex()
                .id(SharedString::from(scroll_id))
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .debug_selector(|| "screen-scroll".to_owned())
                .child(
                    div()
                        .w_full()
                        .flex_none()
                        .debug_selector(|| "screen-page".to_owned())
                        .child(content),
                )
                .into_any_element()
        };
        v_flex()
            .size_full()
            .text_color(cx.theme().foreground)
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if event.keystroke.key == "escape"
                    && let Some(key) = this.edited.take()
                {
                    this.drop_rename(&key);
                    this.reverted = Some(key);
                    cx.notify();
                }
            }))
            .children(strip)
            .child(body)
    }
}

/// `with_input` for a handler that changes the state of a screen in the same call.
pub(crate) fn with_screen_input<R>(
    model: &Entity<AppModel>,
    cx: &App,
    f: impl FnOnce(&ScreenInput<'_>) -> R,
) -> R {
    let app = model.read(cx);
    f(&ScreenInput {
        state: &app.state,
        now_us: now_us(),
        locale: app.locale,
        down_since_us: app.down_since_us,
        feedback: &app.feedback,
    })
}

/// Grey bars in place of the values: the Loading state.
pub(crate) fn skeleton(p: &Palette, width: f32) -> impl IntoElement {
    div().h(px(14.)).w(px(width)).rounded(px(6.)).bg(p.track)
}
