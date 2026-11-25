//! Review: at 1100x700, in English, Italian and German, every row of the alarm tables ends in its
//! card (inside the 18 px row padding), with and without a pending poweroff.

mod support;

use gpui::{TestAppContext, VisualTestContext, px};
use nicecooling::app::{self, Backend, StartMode};
use nicecooling::i18n::Language;
use nicecooling::model::{
    AlarmDetails, AlarmRecord, DaemonEvent, Outcome, Response, ResponseRule, Severity,
};
use nicecooling::shell::Page;
use support::fixtures::{LEAKSHIELD, alarm, online};
use tokio::sync::{broadcast, mpsc};

fn check(cx: &mut TestAppContext, language: Language, poweroff: bool) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (events, events_rx) = broadcast::channel(64);
    let (commands, inbox) = mpsc::unbounded_channel();
    let backend = Backend {
        events: events_rx,
        commands: commands.clone(),
        inbox,
        client: None,
        graphs: None,
        runtime: runtime.handle().clone(),
        language,
        settings: Default::default(),
    };
    let model = cx.update(|cx| app::init(cx, StartMode::Normal, backend));
    cx.run_until_parked();
    let mut state = online();
    let response = if poweroff {
        Response::Poweroff
    } else {
        Response::Notify
    };
    state.snapshot.alarms = vec![alarm(
        "a1",
        "leakshield.leak-alarm",
        LEAKSHIELD,
        response,
        30,
    )];
    state.snapshot.alarm_history = vec![AlarmRecord {
        instance: "a0".into(),
        event: "leakshield.leak-alarm".into(),
        severity: Severity::Alarm,
        source: LEAKSHIELD.into(),
        response: Response::Poweroff,
        raised_us: support::fixtures::NOW_US,
        cleared_us: Some(support::fixtures::NOW_US + 3_725_000_000),
        outcome: Outcome::PoweroffCancelled,
        closed_by_restart: true,
        details: AlarmDetails::default(),
    }];
    state.snapshot.responses = vec![ResponseRule {
        event: "leakshield-unknown-firmware".into(),
        source: String::new(),
        response: Response::Poweroff,
        delay_s: 30,
        overridden: true,
    }];
    events
        .send(DaemonEvent::Snapshot(Box::new(state.snapshot.clone())))
        .unwrap();
    cx.run_until_parked();
    cx.update(|cx| {
        model.update(cx, |app, cx| {
            app.page = Page::Alarms;
            cx.notify();
        })
    });
    let handle = cx.windows()[0];
    cx.simulate_window_resize(handle, gpui::size(px(1100.), px(700.)));
    cx.run_until_parked();
    let mut visual = VisualTestContext::from_window(handle, cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let b = |v: &mut VisualTestContext, n: &'static str| {
        v.debug_bounds(n)
            .unwrap_or_else(|| panic!("{language:?} poweroff={poweroff}: no {n}"))
    };
    let content = b(&mut visual, "shell-content");
    let inner = content.right() - px(24. + 18.);
    let active = b(&mut visual, "active-event-cell");
    let history = b(&mut visual, "history-event-cell");
    let notes = b(&mut visual, "response-notes");
    // The cells after the Event cell, with their 12 px gaps.
    let active_end = active.right() + px(4. * 12. + 150. + 70. + 150. + 170.);
    let history_end = history.right() + px(3. * 12. + 140. + 100. + 200.);
    let tag = format!("{language:?} poweroff={poweroff}");
    eprintln!(
        "{tag}: inner {inner:?} active {active_end:?} ({:?}) history {history_end:?} ({:?}) notes {:?}",
        active.size.width,
        history.size.width,
        notes.right()
    );
    assert!(
        active.size.width >= px(160.) && history.size.width >= px(160.),
        "{tag}"
    );
    assert!(
        active_end <= inner,
        "{tag}: active row ends at {active_end:?}"
    );
    assert!(
        history_end <= inner,
        "{tag}: history row ends at {history_end:?}"
    );
    assert!(
        notes.right() <= inner,
        "{tag}: response row ends at {:?}",
        notes.right()
    );
    if poweroff {
        let cancel = b(&mut visual, "active-cancel");
        assert!(
            cancel.right() <= inner,
            "{tag}: cancel ends at {:?}",
            cancel.right()
        );
    }
}

#[gpui::test]
fn review_alarm_rows_fit_en(cx: &mut TestAppContext) {
    check(cx, Language::English, false);
}
#[gpui::test]
fn review_alarm_rows_fit_en_poweroff(cx: &mut TestAppContext) {
    check(cx, Language::English, true);
}
#[gpui::test]
fn review_alarm_rows_fit_it(cx: &mut TestAppContext) {
    check(cx, Language::Italian, false);
}
#[gpui::test]
fn review_alarm_rows_fit_it_poweroff(cx: &mut TestAppContext) {
    check(cx, Language::Italian, true);
}
#[gpui::test]
fn review_alarm_rows_fit_de(cx: &mut TestAppContext) {
    check(cx, Language::German, false);
}
#[gpui::test]
fn review_alarm_rows_fit_de_poweroff(cx: &mut TestAppContext) {
    check(cx, Language::German, true);
}
