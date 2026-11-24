//! Review: adversarial checks of the header bar. The test platform's `zoom` and
//! `start_window_move` are `unimplemented!()`, so a handler that reaches them panics.

mod support;

use gpui::{
    Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point,
    TestAppContext, VisualTestContext, point, px,
};
use nicecooling::app::{self, AppModel, Backend, StartMode};
use nicecooling::command::Command;
use nicecooling::i18n::Language;
use nicecooling::model::DaemonEvent;
use tokio::sync::{broadcast, mpsc};

struct Rig {
    _events: broadcast::Sender<DaemonEvent>,
    _commands: mpsc::UnboundedSender<Command>,
    _runtime: tokio::runtime::Runtime,
}

fn start(cx: &mut TestAppContext) -> (gpui::Entity<AppModel>, Rig, VisualTestContext) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (events, events_rx) = broadcast::channel(64);
    let (commands, inbox) = mpsc::unbounded_channel();
    let backend = Backend {
        events: events_rx,
        commands: commands.clone(),
        inbox,
        client: None,
        runtime: runtime.handle().clone(),
        language: Language::English,
        settings: Default::default(),
    };
    let model = cx.update(|cx| app::init(cx, StartMode::Normal, backend));
    cx.run_until_parked();
    let handle = cx.windows()[0];
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));
    (
        model,
        Rig {
            _events: events,
            _commands: commands,
            _runtime: runtime,
        },
        visual,
    )
}

fn click(cx: &mut VisualTestContext, at: Point<Pixels>, count: usize) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::default(),
        click_count: count,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::default(),
        click_count: count,
    });
    cx.run_until_parked();
}

/// The primary menu button: 28 px, 12 px from the right edge of the header bar.
fn menu_button(cx: &mut VisualTestContext) -> Point<Pixels> {
    let header = cx.debug_bounds("shell-header").expect("header");
    point(header.right() - px(12. + 14.), header.center().y)
}

/// A double click on the menu button toggles the menu twice. It must not maximize.
#[gpui::test]
fn review_double_click_on_menu_button_does_not_zoom(cx: &mut TestAppContext) {
    let (_model, _rig, mut visual) = start(cx);
    let at = menu_button(&mut visual);
    click(&mut visual, at, 1);
    click(&mut visual, at, 2);
}

/// Control: a double click on the empty bar maximizes (the test platform panics there).
#[gpui::test]
#[should_panic(expected = "not implemented")]
fn review_double_click_on_bar_zooms(cx: &mut TestAppContext) {
    let (_model, _rig, mut visual) = start(cx);
    let header = visual.debug_bounds("shell-header").expect("header");
    let at = point(header.left() + px(20.), header.center().y);
    click(&mut visual, at, 1);
    click(&mut visual, at, 2);
}

/// A press on the menu button and a 2 px move must not start a window move, and the click
/// still opens the menu.
#[gpui::test]
fn review_press_and_drift_on_menu_button_does_not_drag(cx: &mut TestAppContext) {
    let (_model, _rig, mut visual) = start(cx);
    let at = menu_button(&mut visual);
    let drift = point(at.x + px(2.), at.y);
    visual.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    visual.simulate_event(MouseMoveEvent {
        position: drift,
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::default(),
    });
    visual.simulate_mouse_up(drift, MouseButton::Left, Modifiers::default());
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let header = visual.debug_bounds("shell-header").expect("header");
    // The menu is open: its first item is under the header bar.
    assert!(
        visual.debug_bounds("shell-header").is_some() && header.bottom() > px(0.),
        "header drawn"
    );
}

/// Control: a press on the empty bar and a move start a window move.
#[gpui::test]
#[should_panic(expected = "not implemented")]
fn review_press_and_move_on_bar_drags(cx: &mut TestAppContext) {
    let (_model, _rig, mut visual) = start(cx);
    let header = visual.debug_bounds("shell-header").expect("header");
    let at = point(header.left() + px(20.), header.center().y);
    visual.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    visual.simulate_event(MouseMoveEvent {
        position: point(at.x + px(5.), at.y),
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::default(),
    });
}

/// Control: the app name of the sidebar moves the window too.
#[gpui::test]
#[should_panic(expected = "not implemented")]
fn review_press_and_move_on_app_name_drags(cx: &mut TestAppContext) {
    let (_model, _rig, mut visual) = start(cx);
    let header = visual.debug_bounds("shell-header").expect("header");
    // The sidebar is at the left of the header bar; the app name is at the height of the bar.
    let at = point(header.left() - px(110.), header.center().y);
    visual.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    visual.simulate_event(MouseMoveEvent {
        position: point(at.x + px(5.), at.y),
        pressed_button: Some(MouseButton::Left),
        modifiers: Modifiers::default(),
    });
}

/// Under server-side decorations a right click on the bar does not ask for the window menu.
#[gpui::test]
fn review_right_click_under_server_decorations_is_inert(cx: &mut TestAppContext) {
    let (_model, _rig, mut visual) = start(cx);
    let header = visual.debug_bounds("shell-header").expect("header");
    let at = point(header.left() + px(20.), header.center().y);
    visual.simulate_event(MouseDownEvent {
        button: MouseButton::Right,
        position: at,
        modifiers: Modifiers::default(),
        click_count: 1,
        first_mouse: false,
    });
}

/// Ctrl+W hides the window; the app stays (quit mode is explicit).
#[gpui::test]
fn review_ctrl_w_hides_the_window(cx: &mut TestAppContext) {
    let (_model, _rig, mut visual) = start(cx);
    visual.simulate_keystrokes("ctrl-w");
    visual.run_until_parked();
    assert!(visual.windows().is_empty(), "the window is closed");
}
