//! The host of the screens of tasks 8.5 to 8.9. It owns the state that the user changes
//! (selections, drafts, open popovers), builds the model of the page and draws it.

use std::cell::Cell;
use std::collections::{BTreeSet, HashMap};
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{
    AnyElement, App, Bounds, Context, Entity, Focusable as _, Pixels, Render, Window, div, px,
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
use crate::screens::channels::ChannelsUi;
use crate::screens::history::{ChartData, HistoryUi};
use crate::screens::loops::LoopsUi;
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
    /// `(channel, point, 0 for the Input, 1 for the Duty)`.
    CurvePoint(String, usize, usize),
    CalibrationCell(String, usize, usize),
}

/// The data of the History chart for one window.
pub struct HistoryCache {
    key: (Vec<String>, crate::screens::history::Range, i64, i64),
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
    pub(crate) inputs: HashMap<InputKey, Entity<InputState>>,
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
    /// The pointer in the History chart, as a fraction of its width.
    pub(crate) hover: Option<f32>,
    pub(crate) chart_focus: Option<gpui::FocusHandle>,
    /// The focus of the Sensors table: Enter on a selected row opens History.
    pub(crate) sensors_focus: Option<gpui::FocusHandle>,
    pub(crate) history_error: Option<String>,
    /// The sensor picker of the Curve editor, History and the Derived sensor editor.
    pub(crate) open_picker: Option<String>,
    pub(crate) preset_menu_open: bool,
}

impl ScreenView {
    pub fn new(
        model: Entity<AppModel>,
        commands: UnboundedSender<Command>,
        cx: &mut Context<Self>,
    ) -> Self {
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        Self {
            model,
            commands,
            sensors: SensorsUi::default(),
            channels: ChannelsUi::default(),
            loops: LoopsUi::default(),
            history: HistoryUi::default(),
            inputs: HashMap::new(),
            sliders: HashMap::new(),
            busy_sliders: BTreeSet::new(),
            plot_bounds: Rc::new(Cell::new(Bounds::default())),
            drag: None,
            plot_focus: Some(cx.focus_handle()),
            history_db: None,
            history_cache: None,
            hover: None,
            chart_focus: Some(cx.focus_handle()),
            sensors_focus: Some(cx.focus_handle()),
            history_error: None,
            open_picker: None,
            preset_menu_open: false,
        }
    }

    /// Open History with one sensor selected: Enter on a row of the Sensors screen.
    pub fn open_history_for(&mut self, sensor: &str, cx: &mut Context<Self>) {
        self.history.selected = vec![sensor.to_owned()];
        self.history.picker_open = false;
        self.history_cache = None;
        self.go(Page::History, cx);
    }

    /// Add a sensor to the History chart, or take it out.
    pub fn toggle_history_sensor(&mut self, sensor: &str) {
        self.history.toggle_sensor(sensor);
        self.history_cache = None;
    }

    pub(crate) fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }

    pub(crate) fn go(&self, page: Page, cx: &mut Context<Self>) {
        self.model.update(cx, |app, cx| {
            app.page = page;
            cx.notify();
        });
    }

    /// A function that opens a page, for a widget that has no access to the view.
    pub(crate) fn navigator(&self) -> impl Fn(Page, &mut Window, &mut App) + 'static {
        let model = self.model.clone();
        move |page, _, cx| {
            model.update(cx, |app, cx| {
                app.page = page;
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
    /// `value`, so a drag or a daemon answer changes the text.
    pub(crate) fn input(
        &mut self,
        key: InputKey,
        value: &str,
        placeholder: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(state) = self.inputs.get(&key) {
            let focused = state.read(cx).focus_handle(cx).is_focused(window);
            if !focused && state.read(cx).value().as_ref() != value {
                let value = value.to_owned();
                state.update(cx, |input, cx| input.set_value(value, window, cx));
            }
            return state.clone();
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
    /// when it changes.
    pub(crate) fn history_data(&mut self, cx: &App) -> ChartData {
        let Some(path) = self.model.read(cx).history_path.clone() else {
            return ChartData::Loaded(Vec::new());
        };
        let now = (now_us() / 1_000_000) as i64;
        let (from, to) = self.history.window(now);
        let live = self.history.follows_now();
        let key = (
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
        if self.history_db.is_none() {
            match HistoryDb::open(&path) {
                Ok(db) => {
                    self.history_db = Some(db);
                    self.history_error = None;
                }
                Err(error) => self.history_error = Some(error.to_string()),
            }
        }
        let data = match (&self.history_db, &self.history_error) {
            (Some(db), _) => {
                let loaded: Result<Vec<_>, _> = self
                    .history
                    .selected
                    .iter()
                    .map(|id| crate::history::query::load_series(db, id, from, to, now))
                    .collect();
                match loaded {
                    Ok(series) => ChartData::Loaded(series),
                    Err(error) => ChartData::Error {
                        path: path.display().to_string(),
                        message: error.to_string(),
                    },
                }
            }
            (None, message) => ChartData::Error {
                path: path.display().to_string(),
                message: message.clone().unwrap_or_default(),
            },
        };
        self.history_cache = Some(HistoryCache {
            key,
            loaded_at: now,
            data: data.clone(),
        });
        data
    }

    /// "Try again" on a database error.
    pub(crate) fn retry_history(&mut self, cx: &mut Context<Self>) {
        self.history_db = None;
        self.history_error = None;
        self.history_cache = None;
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
        let content: AnyElement = match page {
            Page::Overview => self.render_overview(&p, window, cx),
            Page::Sensors => self.render_sensors(&p, window, cx),
            Page::Channels => self.render_channels(&p, window, cx),
            Page::Loops => self.render_loops(&p, window, cx),
            Page::History => self.render_history(&p, window, cx),
            _ => div().into_any_element(),
        };
        v_flex()
            .id("screen")
            .size_full()
            .overflow_y_scroll()
            .text_color(cx.theme().foreground)
            .child(content)
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
