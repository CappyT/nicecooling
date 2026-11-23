//! The app-only items of issue 32: the First run entry points, the Up and Down keys of the
//! Channels list and the History progress bar. The hidden sensors have their test in
//! `settings.rs`.

mod support;

use std::time::Duration;

use nicecooling::app::AppModel;
use nicecooling::i18n::EN;
use nicecooling::model::{DaemonEvent, Link};
use nicecooling::screens::channels::{ChannelsModel, ChannelsUi, neighbor};
use nicecooling::screens::history::{PROGRESS_DELAY, progress_due};
use nicecooling::screens::overview::OverviewModel;
use nicecooling::screens::{Feedback, ScreenInput};
use nicecooling::shell::Page;
use support::fixtures::{NOW_US, full};

fn input<'a>(state: &'a nicecooling::model::AppState, feedback: &'a Feedback) -> ScreenInput<'a> {
    ScreenInput {
        state,
        now_us: NOW_US,
        locale: EN,
        down_since_us: None,
        feedback,
    }
}

#[test]
fn overview_set_up_a_loop_opens_first_run() {
    let mut state = full();
    state.snapshot.loops.clear();
    let feedback = Feedback::default();
    let model = OverviewModel::build(&input(&state, &feedback));
    let empty = model.empty.expect("the empty card");
    assert_eq!(empty.button, "Set up a Loop");
    assert_eq!(empty.open, Page::FirstRun);
}

#[test]
fn first_run_opens_once_when_the_daemon_has_no_loop() {
    let mut app = AppModel::new(EN);
    let mut empty = full().snapshot;
    empty.loops.clear();
    app.apply(&DaemonEvent::Snapshot(Box::new(empty.clone())));
    assert_eq!(app.page, Page::FirstRun);
    // The user leaves the flow: a later read does not bring it back.
    app.page = Page::Overview;
    app.apply(&DaemonEvent::Snapshot(Box::new(empty)));
    assert_eq!(app.page, Page::Overview);

    // A daemon that has a Loop starts on the Overview.
    let mut app = AppModel::new(EN);
    app.apply(&DaemonEvent::Snapshot(Box::new(full().snapshot)));
    assert_eq!(app.page, Page::Overview);
    app.apply(&DaemonEvent::Link(Link::Offline));
    assert_eq!(app.page, Page::Overview);
}

#[test]
fn up_and_down_move_the_channel_selection() {
    let state = full();
    let feedback = Feedback::default();
    let mut ui = ChannelsUi::default();
    let rows = |ui: &ChannelsUi| ChannelsModel::build(&input(&state, &feedback), ui).rows;
    let ids: Vec<String> = rows(&ui).iter().map(|row| row.id.clone()).collect();
    assert_eq!(ids.len(), 2);
    assert!(rows(&ui)[0].selected, "the first Channel starts selected");

    // Up at the top and Down at the bottom stay where they are.
    assert_eq!(neighbor(&rows(&ui), false), None);
    assert_eq!(neighbor(&rows(&ui), true), Some(ids[1].clone()));
    ui.select(&ids[1]);
    assert_eq!(neighbor(&rows(&ui), true), None);
    assert_eq!(neighbor(&rows(&ui), false), Some(ids[0].clone()));
    assert_eq!(neighbor(&[], true), None);
}

#[test]
fn history_progress_bar_shows_after_300_ms() {
    assert_eq!(PROGRESS_DELAY, Duration::from_millis(300));
    assert!(!progress_due(Duration::ZERO));
    assert!(!progress_due(Duration::from_millis(299)));
    assert!(progress_due(Duration::from_millis(300)));
    assert!(progress_due(Duration::from_secs(2)));
}
