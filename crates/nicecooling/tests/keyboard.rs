//! The keyboard on the controls that the app draws itself: the segmented control, the select
//! and the sensor picker.

mod support;

use std::cell::Cell;
use std::rc::Rc;

use gpui::prelude::*;
use gpui::{Bounds, Pixels, TestAppContext, VisualTestContext, div, px};
use nicecooling::app::{self, AppModel, Backend, StartMode};
use nicecooling::command::Command;
use nicecooling::i18n::Language;
use nicecooling::model::DaemonEvent;
use nicecooling::screens::ui::{seg, seg_item, select_trigger};
use nicecooling::shell::Page;
use nicecooling::tokens::Palette;
use tokio::sync::{broadcast, mpsc};

/// A select, a segmented control with a disabled item, then a disabled select, in a screen
/// that takes Enter for itself.
struct Controls {
    picked: Rc<Cell<usize>>,
    screen_enters: Rc<Cell<usize>>,
    focus: gpui::FocusHandle,
}

impl gpui::Render for Controls {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let p = Palette::new(false);
        let (picked, enters) = (self.picked.clone(), self.screen_enters.clone());
        let item =
            |name: &'static str, on, enabled, window: &mut gpui::Window, cx: &mut gpui::App| {
                seg_item(name, name, on, enabled, &p, window, cx)
                    .debug_selector(move || name.to_owned())
            };
        let (curve, fixed, hardware) = (
            item("curve", true, true, window, cx),
            item("fixed", false, true, window, cx),
            item("hardware", false, false, window, cx),
        );
        let select =
            |name: &'static str, enabled, window: &mut gpui::Window, cx: &mut gpui::App| {
                let trigger = select_trigger(name, name, None, false, enabled, p.track, window, cx);
                div()
                    .mt(px(20.))
                    .w(px(280.))
                    .child(trigger.debug_selector(move || name.to_owned()))
            };
        let (first, disabled) = (
            select("first", true, window, cx),
            select("disabled", false, window, cx),
        );
        div()
            .size_full()
            .p(px(40.))
            .track_focus(&self.focus)
            .on_key_down(move |event, _, _| {
                if event.keystroke.key == "enter" {
                    enters.set(enters.get() + 1);
                }
            })
            .child(first)
            .child(
                seg("control", &p, window, cx)
                    .mt(px(20.))
                    .child(curve)
                    .child(fixed.on_click(move |_, _, _| picked.set(picked.get() + 1)))
                    .child(hardware),
            )
            .child(disabled)
    }
}

/// The bounds of the focus ring, after a new frame.
fn ring(visual: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    named(visual, "nc-focus-ring")
}

fn named(visual: &mut VisualTestContext, name: &'static str) -> Option<Bounds<Pixels>> {
    visual.run_until_parked();
    visual.update(|window, cx| window.draw(cx).clear(cx));
    visual.debug_bounds(name)
}

/// Enter or Space: the key goes down and up, as a click of the keyboard needs.
fn press(visual: &mut VisualTestContext, key: &'static str) {
    visual.simulate_keystrokes(key);
    visual.simulate_event(gpui::KeyUpEvent {
        keystroke: gpui::Keystroke::parse(key).expect("a key"),
    });
    visual.run_until_parked();
}

/// The ring of `DESIGN.md`: 2 px wide, 2 px from the control.
fn around(control: Bounds<Pixels>) -> Bounds<Pixels> {
    control.dilate(px(4.))
}

#[gpui::test]
fn a_segmented_control_takes_the_keyboard(cx: &mut TestAppContext) {
    let (picked, screen_enters) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let window = cx.add_window({
        let (picked, screen_enters) = (picked.clone(), screen_enters.clone());
        move |window, cx| {
            let focus = cx.focus_handle();
            window.focus(&focus, cx);
            Controls {
                picked,
                screen_enters,
                focus,
            }
        }
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    assert!(ring(&mut visual).is_none(), "nothing has the focus ring");
    let mut bounds = |name| named(&mut visual, name).expect(name);
    let (first, disabled) = (bounds("first"), bounds("disabled"));
    let (curve, fixed, hardware) = (bounds("curve"), bounds("fixed"), bounds("hardware"));

    // Tab stops at the select, then at the first item: the ring shows around each.
    visual.update(|window, cx| window.focus_next(cx));
    assert_eq!(ring(&mut visual), Some(around(first)));
    visual.update(|window, cx| window.focus_next(cx));
    assert_eq!(ring(&mut visual), Some(around(curve)));

    // Right moves to the next item. It does not select it.
    visual.simulate_keystrokes("right");
    assert_eq!(ring(&mut visual), Some(around(fixed)));
    assert_eq!(picked.get(), 0, "an arrow key moves the focus only");

    // The disabled item takes no focus, and the focus does not leave the control.
    visual.simulate_keystrokes("right");
    assert_eq!(ring(&mut visual), Some(around(fixed)));

    // Enter and Space run the click handler. The screen does not get the key too.
    press(&mut visual, "enter");
    assert_eq!(picked.get(), 1, "Enter");
    press(&mut visual, "space");
    assert_eq!(picked.get(), 2, "Space");
    assert_eq!(screen_enters.get(), 0, "the screen got Enter too");

    visual.simulate_keystrokes("left");
    assert_eq!(ring(&mut visual), Some(around(curve)));
    visual.simulate_keystrokes("left");
    assert_eq!(ring(&mut visual), Some(around(curve)), "Left stays in it");

    // Tab goes item by item. It never stops at a disabled item or at a disabled select.
    for _ in 0..7 {
        visual.update(|window, cx| window.focus_next(cx));
        let ring = ring(&mut visual).expect("a ring");
        let stops = [around(first), around(curve), around(fixed)];
        assert!(stops.contains(&ring), "{ring:?}");
        assert!(ring != around(disabled) && ring != around(hardware));
    }

    // A press runs the handler and moves no focus: the ring is for the keyboard.
    let before = ring(&mut visual);
    visual.simulate_click(fixed.center(), gpui::Modifiers::none());
    assert_eq!(picked.get(), 3);
    assert_eq!(ring(&mut visual), before);
}

struct Rig {
    events: broadcast::Sender<DaemonEvent>,
    inbox: mpsc::UnboundedReceiver<Command>,
    _commands: mpsc::UnboundedSender<Command>,
    _runtime: tokio::runtime::Runtime,
}

fn start(cx: &mut TestAppContext, page: Page) -> (gpui::Entity<AppModel>, Rig) {
    start_with(cx, support::fixtures::full(), page)
}

fn start_with(
    cx: &mut TestAppContext,
    state: nicecooling::model::AppState,
    page: Page,
) -> (gpui::Entity<AppModel>, Rig) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (events, events_rx) = broadcast::channel(64);
    let (commands, inbox) = mpsc::unbounded_channel();
    let (sent, watch) = mpsc::unbounded_channel();
    drop(inbox);
    let backend = Backend {
        events: events_rx,
        commands: sent,
        inbox: mpsc::unbounded_channel().1,
        client: None,
        runtime: runtime.handle().clone(),
        language: Language::English,
        settings: Default::default(),
    };
    let model = cx.update(|cx| app::init(cx, StartMode::Normal, backend));
    events
        .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
        .unwrap();
    events
        .send(DaemonEvent::Sensors(state.sensors.clone().unwrap()))
        .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        model.update(cx, |app, cx| {
            app.page = page;
            cx.notify();
        });
    });
    cx.run_until_parked();
    (
        model,
        Rig {
            events,
            inbox: watch,
            _commands: commands,
            _runtime: runtime,
        },
    )
}

/// Press Tab until the focus ring is on `control`.
fn tab_to(visual: &mut VisualTestContext, control: Bounds<Pixels>) {
    for _ in 0..80 {
        visual.simulate_keystrokes("tab");
        if ring(visual) == Some(around(control)) {
            return;
        }
    }
    panic!("Tab did not reach the control at {control:?}");
}

#[gpui::test]
fn a_select_takes_the_keyboard(cx: &mut TestAppContext) {
    let (_model, mut rig) = start(cx, Page::Alarms);
    let handle = cx.windows()[0];
    let mut visual = VisualTestContext::from_window(handle, cx);
    let (select, list) = ("select-history-filter", "select-list-history-filter");
    let control = named(&mut visual, select).expect("the filter select");
    tab_to(&mut visual, control);
    assert!(named(&mut visual, list).is_none(), "Tab opens nothing");

    // Down opens the list with the mark on the current choice. The select keeps the focus.
    visual.simulate_keystrokes("down");
    assert!(named(&mut visual, list).is_some(), "Down opens the list");
    assert!(named(&mut visual, "choice-history-filter-0").is_some());
    assert_eq!(ring(&mut visual), Some(around(control)));

    // Up and Down move the mark and stay in the list.
    visual.simulate_keystrokes("up");
    assert!(named(&mut visual, "choice-history-filter-0").is_some());
    visual.simulate_keystrokes("down down");
    assert!(named(&mut visual, "choice-history-filter-2").is_some());
    visual.simulate_keystrokes("down down down");
    assert!(named(&mut visual, "choice-history-filter-2").is_some());
    visual.simulate_keystrokes("up");
    assert!(named(&mut visual, "choice-history-filter-1").is_some());

    // Enter picks the marked choice and closes the list. The next list starts on it.
    press(&mut visual, "enter");
    assert!(named(&mut visual, list).is_none(), "Enter closes the list");
    assert_eq!(ring(&mut visual), Some(around(control)));
    visual.simulate_keystrokes("down");
    assert!(
        named(&mut visual, "choice-history-filter-1").is_some(),
        "the picked choice"
    );

    // Escape closes the list. Enter and Space open and close it, as a press does.
    visual.simulate_keystrokes("escape");
    assert!(named(&mut visual, list).is_none(), "Escape");
    press(&mut visual, "enter");
    assert!(named(&mut visual, list).is_some(), "Enter opens");
    visual.simulate_keystrokes("escape");
    press(&mut visual, "space");
    assert!(named(&mut visual, list).is_some(), "Space opens");
    // Space picks the marked choice: the list closes.
    press(&mut visual, "space");
    assert!(named(&mut visual, list).is_none(), "Space picks");
    assert!(rig.inbox.try_recv().is_err(), "the filter writes nothing");
    let _ = &rig.events;
}

#[gpui::test]
fn a_sensor_picker_takes_the_keyboard(cx: &mut TestAppContext) {
    let (_model, mut rig) = start(cx, Page::Loops);
    let handle = cx.windows()[0];
    let mut visual = VisualTestContext::from_window(handle, cx);
    let (select, list) = ("select-part-water", "picker-part-water");
    let control = named(&mut visual, select).expect("the water temperature select");
    tab_to(&mut visual, control);
    assert!(named(&mut visual, list).is_none(), "Tab opens nothing");

    // Down opens the picker, and its search input takes the focus: a row has the mark.
    visual.simulate_keystrokes("down");
    assert!(named(&mut visual, list).is_some(), "Down opens");
    assert!(
        ring(&mut visual).is_none(),
        "the focus is in the search input"
    );
    let marked = |visual: &mut VisualTestContext| {
        (0..40).find(|row| {
            let name: &'static str = format!("pick-part-water-{row}").leak();
            named(visual, name).is_some()
        })
    };
    // The mark starts on the sensor of the Loop: the last row here.
    let start = marked(&mut visual).expect("a marked row");
    assert!(start > 0, "the fixture has the sensor in a later row");
    visual.simulate_keystrokes("up");
    assert_eq!(marked(&mut visual), Some(start - 1), "Up moves the mark");
    visual.simulate_keystrokes("down");
    assert_eq!(marked(&mut visual), Some(start), "Down moves the mark");

    // Escape closes the picker without a pick, and the select has the focus again.
    visual.simulate_keystrokes("escape");
    assert!(named(&mut visual, list).is_none(), "Escape");
    assert_eq!(ring(&mut visual), Some(around(control)));
    assert!(rig.inbox.try_recv().is_err(), "Escape picks nothing");

    // Enter opens it too. Enter in the list picks the marked sensor and closes the picker.
    press(&mut visual, "enter");
    assert!(named(&mut visual, list).is_some(), "Enter opens");
    visual.simulate_keystrokes("up");
    press(&mut visual, "enter");
    assert!(named(&mut visual, list).is_none(), "Enter picks and closes");
    assert!(rig.inbox.try_recv().is_ok(), "the pick sends its command");
    assert_eq!(ring(&mut visual), Some(around(control)));
}

/// Two controls and a screen that takes Enter, under a dialog with two controls. Enter on
/// the first control opens the dialog.
struct UnderDialog {
    open: bool,
    /// The presses of the two controls under the scrim, then of the two in the dialog.
    presses: [Rc<Cell<usize>>; 4],
    screen_keys: Rc<Cell<usize>>,
    root: gpui::FocusHandle,
    stops: [gpui::FocusHandle; 4],
}

impl gpui::Render for UnderDialog {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        nicecooling::screens::ui::dialog_frame(window, cx);
        let control = |index: usize| {
            let presses = self.presses[index].clone();
            div()
                .id(("control", index))
                .size(px(40.))
                .track_focus(&self.stops[index])
                .on_click(move |_, _, _| presses.set(presses.get() + 1))
        };
        let keys = self.screen_keys.clone();
        let opens = cx.listener(|this, _, _, cx| {
            this.open = true;
            cx.notify();
        });
        div()
            .size_full()
            .track_focus(&self.root)
            .on_key_down(move |event, _, _| {
                if event.keystroke.key != "escape" {
                    keys.set(keys.get() + 1);
                }
            })
            .child(control(0).on_click(opens))
            .child(control(1))
            .children(self.open.then(|| {
                nicecooling::screens::ui::modal(
                    "Reset the counter?",
                    "This cannot be undone.",
                    Vec::new(),
                    vec![control(2).into_any_element(), control(3).into_any_element()],
                    &Palette::new(false),
                )
            }))
    }
}

/// While a dialog is open the focus stays in it: Tab and Shift+Tab go through its controls,
/// no key reaches a control or the screen under the scrim, and the control that opened the
/// dialog has the focus again when it closes.
#[gpui::test]
fn a_dialog_keeps_the_focus(cx: &mut TestAppContext) {
    let presses: [Rc<Cell<usize>>; 4] = Default::default();
    let screen_keys = Rc::new(Cell::new(0));
    let window = cx.add_window({
        let (presses, screen_keys) = (presses.clone(), screen_keys.clone());
        move |window, cx| {
            let root = cx.focus_handle();
            window.focus(&root, cx);
            UnderDialog {
                open: false,
                presses,
                screen_keys,
                root,
                stops: std::array::from_fn(|_| cx.focus_handle().tab_stop(true)),
            }
        }
    });
    let stops = window
        .update(cx, |screen, _, _| screen.stops.clone())
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let frame = |visual: &mut VisualTestContext| {
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear(cx));
        visual.run_until_parked();
        visual.update(|window, cx| window.draw(cx).clear(cx));
    };
    let focused = |visual: &mut VisualTestContext| {
        visual.update(|window, _| stops.iter().position(|stop| stop.is_focused(window)))
    };
    frame(&mut visual);

    // Enter on the first control opens the dialog: the focus leaves the control.
    visual.update(|window, cx| window.focus_next(cx));
    frame(&mut visual);
    assert_eq!(focused(&mut visual), Some(0));
    press(&mut visual, "enter");
    frame(&mut visual);
    assert!(visual.debug_bounds("modal-scrim").is_some(), "the dialog");
    assert_eq!(focused(&mut visual), None, "the focus is in the dialog");
    let (opened, screen) = (presses[0].get(), screen_keys.get());

    // Tab and Shift+Tab stay in the dialog, and Enter presses only its controls.
    let mut seen = std::collections::BTreeSet::new();
    for key in ["tab"; 9].into_iter().chain(["shift-tab"; 9]) {
        visual.simulate_keystrokes(key);
        frame(&mut visual);
        let stop = focused(&mut visual).expect("a control of the dialog");
        assert!(stop >= 2, "{key} moved the focus under the scrim");
        seen.insert(stop);
        press(&mut visual, "enter");
        frame(&mut visual);
        visual.simulate_keystrokes("down alt-2 f10");
    }
    assert_eq!(seen.into_iter().collect::<Vec<_>>(), [2, 3]);
    assert_eq!(
        presses[0].get(),
        opened,
        "Enter reached the control that opened it"
    );
    assert_eq!(
        presses[1].get(),
        0,
        "Enter reached a control under the scrim"
    );
    assert_eq!(
        presses[2].get() + presses[3].get(),
        18,
        "the dialog takes Enter"
    );
    assert_eq!(
        screen_keys.get(),
        screen,
        "a key reached the screen under the scrim"
    );

    // The dialog closes: the control that opened it has the focus again.
    window
        .update(cx, |screen, _, cx| {
            screen.open = false;
            cx.notify();
        })
        .unwrap();
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    frame(&mut visual);
    assert!(visual.debug_bounds("modal-scrim").is_none());
    assert_eq!(focused(&mut visual), Some(0), "the focus is back");
    visual.update(|window, cx| window.focus_next(cx));
    frame(&mut visual);
    assert_eq!(
        focused(&mut visual),
        Some(1),
        "Tab moves on the screen again"
    );
}

/// The poweroff banner and the countdown dialog draw over the scrim of a dialog: "Cancel
/// poweroff" works in both while a dialog of a screen is open, by a press and by the keyboard.
#[gpui::test]
fn cancel_poweroff_works_over_a_dialog(cx: &mut TestAppContext) {
    use support::fixtures::{LEAKSHIELD, alarm, with_leakshield};

    let state = with_leakshield();
    let (_model, mut rig) = start_with(cx, state.clone(), Page::Leakshield(LEAKSHIELD.to_owned()));
    let handle = cx.windows()[0];
    let mut visual = VisualTestContext::from_window(handle, cx);
    let click = |visual: &mut VisualTestContext, name: &'static str| {
        let control = named(visual, name).unwrap_or_else(|| panic!("no control {name}"));
        visual.simulate_click(control.center(), gpui::Modifiers::none());
    };
    let mut cancelled = |what: &str| {
        let command = rig.inbox.try_recv();
        assert!(
            matches!(command, Ok(Command::CancelPoweroff { .. })),
            "{what}: {command:?}"
        );
        assert!(rig.inbox.try_recv().is_err(), "{what}: one command");
    };

    // A dialog of the screen is open. Then an Alarm starts a poweroff countdown.
    click(&mut visual, "ls-refill-steps");
    assert!(named(&mut visual, "modal-scrim").is_some(), "the dialog");
    let mut snapshot = state.snapshot.clone();
    let mut leak = alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        nicecooling::model::Response::Poweroff,
        30,
    );
    leak.raised_us = app::now_us();
    leak.poweroff_at_us = leak.raised_us + 30_000_000;
    snapshot.alarms = vec![leak];
    rig.events
        .send(DaemonEvent::Snapshot(Box::new(snapshot)))
        .unwrap();

    // The countdown dialog is over the scrim: its button takes the press.
    click(&mut visual, "countdown-cancel");
    cancelled("the countdown dialog");
    click(&mut visual, "countdown-hide");
    assert!(named(&mut visual, "countdown-cancel").is_none(), "hidden");
    assert!(
        named(&mut visual, "modal-scrim").is_some(),
        "the dialog stays"
    );

    // The banner is over the scrim too.
    click(&mut visual, "banner-cancel");
    cancelled("the banner");

    // Tab goes to the banner and to the dialog, and to nothing else.
    visual.simulate_keystrokes("tab");
    assert!(named(&mut visual, "modal-scrim").is_some());
    press(&mut visual, "enter");
    cancelled("Enter on the banner");
    visual.simulate_keystrokes("tab");
    assert!(named(&mut visual, "modal-scrim").is_some());
    press(&mut visual, "enter");
    assert!(
        named(&mut visual, "modal-scrim").is_none(),
        "Enter on Close"
    );
    assert!(named(&mut visual, "banner-cancel").is_some());
}
