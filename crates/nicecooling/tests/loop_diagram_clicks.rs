//! A bubble of the Loop diagram opens its page, and a change that rounds to zero has no sign.

mod support;

use gpui::{TestAppContext, VisualTestContext, px};
use nicecooling::app::{self, Backend, StartMode};
use nicecooling::i18n::{EN, IT, Language};
use nicecooling::model::DaemonEvent;
use nicecooling::screens::leakshield::change_text;
use nicecooling::shell::Page;
use support::fixtures::{LEAKSHIELD, full};
use tokio::sync::{broadcast, mpsc};

/// The doc of `change_text` says: a value that rounds to zero has no sign.
#[test]
fn a_change_that_rounds_to_zero_has_no_sign() {
    assert_eq!(change_text(EN, Some(0.3), "ml/h"), "0 ml/h");
    assert_eq!(change_text(EN, Some(-0.3), "ml/h"), "0 ml/h");
    assert_eq!(change_text(IT, Some(-0.04), "mbar/min"), "0,0 mbar/min");
}

/// Draw `page` at `width` x 700, click the bubble `tag` and return the page after it.
fn click_bubble(cx: &mut TestAppContext, page: Page, width: f32, tag: &str) -> Page {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (events, events_rx) = broadcast::channel(64);
    let (commands, inbox) = mpsc::unbounded_channel();
    let backend = Backend {
        events: events_rx,
        commands,
        inbox,
        client: None,
        graphs: None,
        runtime: runtime.handle().clone(),
        language: Language::English,
        settings: Default::default(),
    };
    let state = full();
    let model = cx.update(|cx| app::init(cx, StartMode::Normal, backend));
    cx.run_until_parked();
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
        })
    });
    let handle = cx.windows()[0];
    cx.simulate_window_resize(handle, gpui::size(px(width), px(700.)));
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let name: &'static str = Box::leak(format!("loop-bubble-{tag}").into_boxed_str());
    let bubble = visual.debug_bounds(name).expect("a bubble");
    visual.simulate_click(bubble.center(), gpui::Modifiers::none());
    visual.run_until_parked();
    let after = cx.update(|cx| model.read(cx).page.clone());
    cx.update(|cx| cx.quit());
    drop(runtime);
    after
}

#[gpui::test]
fn a_click_on_the_pt_bubble_of_the_overview_opens_the_leakshield(cx: &mut TestAppContext) {
    assert_eq!(
        click_bubble(cx, Page::Overview, 1280., "PT"),
        Page::Leakshield(LEAKSHIELD.to_owned())
    );
}

#[gpui::test]
fn a_click_on_the_st_bubble_of_the_narrow_diagram_opens_the_channels(cx: &mut TestAppContext) {
    assert_eq!(click_bubble(cx, Page::Loops, 1100., "ST"), Page::Channels);
}
