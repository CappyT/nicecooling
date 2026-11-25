//! The Loop diagram at its two widths: on the Loops screen at 1100x700 it is 528 px wide and
//! has two rows, and on the Overview it has one row. No text reaches a bubble or leaves the
//! diagram, in English, Italian and German, with the long captions of an Alarm and of a flag.

mod support;

use gpui::{Bounds, Pixels, TestAppContext, VisualTestContext, px};
use nicecooling::app::{self, Backend, StartMode};
use nicecooling::i18n::Language;
use nicecooling::model::{AppState, DaemonEvent, FLAG_OUT_OF_RANGE, Response};
use nicecooling::shell::Page;
use support::fixtures::{LEAKSHIELD, alarm, full};
use tokio::sync::{broadcast, mpsc};

const TAGS: [&str; 4] = ["PT", "ST", "FT", "TT"];

/// The pressure under its minimum with a leak Alarm, and the flow above its table: the
/// longest captions of the diagram.
fn long_captions() -> AppState {
    let mut state = full();
    state.snapshot.alarms = vec![alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        Response::Notify,
        0,
    )];
    let flow = state.snapshot.loops[0].flow.clone().expect("a flow sensor");
    let pressure = format!("{LEAKSHIELD}_pressure");
    for sample in [&mut state.sensors, &mut state.last_sensors]
        .into_iter()
        .flatten()
    {
        for (id, value, flags) in &mut sample.sensors {
            if *id == pressure {
                *value = 142.0;
            } else if *id == flow {
                *flags |= FLAG_OUT_OF_RANGE;
            }
        }
    }
    state
}

/// Draw `page` at `width` x 700 and return the bounds of the diagram, of each bubble and of
/// each text.
#[allow(clippy::type_complexity)]
fn draw(
    cx: &mut TestAppContext,
    language: Language,
    state: &AppState,
    page: Page,
    width: f32,
) -> (
    Bounds<Pixels>,
    Vec<(&'static str, Bounds<Pixels>)>,
    Vec<(String, Bounds<Pixels>)>,
) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (events, events_rx) = broadcast::channel(64);
    let (commands, inbox) = mpsc::unbounded_channel();
    let backend = Backend {
        events: events_rx,
        commands,
        inbox,
        client: None,
        runtime: runtime.handle().clone(),
        language,
        settings: Default::default(),
    };
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
    let diagram = visual.debug_bounds("loop-diagram").expect("the diagram");
    let mut bubbles = Vec::new();
    let mut texts = Vec::new();
    for tag in TAGS {
        let bubble: &'static str = Box::leak(format!("loop-bubble-{tag}").into_boxed_str());
        bubbles.push((tag, visual.debug_bounds(bubble).expect("a bubble")));
        for kind in ["value", "caption"] {
            let name: &'static str = Box::leak(format!("loop-{kind}-{tag}").into_boxed_str());
            texts.push((
                format!("{kind} {tag}"),
                visual.debug_bounds(name).expect("a text"),
            ));
        }
    }
    cx.update(|cx| cx.quit());
    drop(runtime);
    (diagram, bubbles, texts)
}

fn check_narrow(cx: &mut TestAppContext, language: Language, state: &AppState) {
    let (diagram, bubbles, texts) = draw(cx, language, state, Page::Loops, 1100.);
    let tag = format!("{language:?}");
    assert_eq!(
        diagram.size.width,
        px(528.),
        "{tag}: the width of the diagram"
    );
    assert_eq!(diagram.size.height, px(212.), "{tag}: two rows");
    for (name, text) in &texts {
        assert!(text.size.width > px(0.), "{tag}: {name} has no width");
        assert!(
            text.left() >= diagram.left() && text.right() <= diagram.right(),
            "{tag}: {name} {text:?} leaves the diagram {diagram:?}"
        );
        for (bubble_tag, bubble) in &bubbles {
            assert!(
                !text.intersects(bubble),
                "{tag}: {name} {text:?} reaches the {bubble_tag} bubble {bubble:?}"
            );
        }
    }
    // The Leakshield group has the first row: its bubble is above the others.
    let top = |tag: &str| bubbles.iter().find(|(t, _)| *t == tag).unwrap().1.top();
    assert_eq!(top("ST") - top("PT"), px(44.), "{tag}");
    assert_eq!(top("ST"), top("TT"), "{tag}");
}

#[gpui::test]
fn narrow_diagram_keeps_its_texts_apart_en(cx: &mut TestAppContext) {
    check_narrow(cx, Language::English, &full());
}

#[gpui::test]
fn narrow_diagram_keeps_its_texts_apart_it(cx: &mut TestAppContext) {
    check_narrow(cx, Language::Italian, &full());
}

#[gpui::test]
fn narrow_diagram_keeps_long_captions_apart_en(cx: &mut TestAppContext) {
    check_narrow(cx, Language::English, &long_captions());
}

#[gpui::test]
fn narrow_diagram_keeps_long_captions_apart_it(cx: &mut TestAppContext) {
    check_narrow(cx, Language::Italian, &long_captions());
}

#[gpui::test]
fn narrow_diagram_keeps_its_texts_apart_de(cx: &mut TestAppContext) {
    check_narrow(cx, Language::German, &full());
}

#[gpui::test]
fn narrow_diagram_keeps_long_captions_apart_de(cx: &mut TestAppContext) {
    check_narrow(cx, Language::German, &long_captions());
}

#[gpui::test]
fn wide_diagram_has_one_row(cx: &mut TestAppContext) {
    for width in [1100., 1280.] {
        let (diagram, bubbles, texts) = draw(
            cx,
            Language::English,
            &long_captions(),
            Page::Overview,
            width,
        );
        assert_eq!(diagram.size.width, px(width - 220. - 48.), "{width}");
        assert_eq!(diagram.size.height, px(168.), "{width}: one row");
        let tops: Vec<_> = bubbles.iter().map(|(_, bubble)| bubble.top()).collect();
        assert!(tops.iter().all(|top| *top == tops[0]), "{width}: {tops:?}");
        for (name, text) in &texts {
            assert!(text.right() <= diagram.right(), "{width}: {name} {text:?}");
            for (bubble_tag, bubble) in &bubbles {
                assert!(
                    !text.intersects(bubble),
                    "{width}: {name} {text:?} reaches the {bubble_tag} bubble {bubble:?}"
                );
            }
        }
    }
}
