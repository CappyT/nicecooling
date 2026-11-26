//! Review reproducers: states of graphs.md that the Graphs screen does not show.

mod support;

use nicecooling::graphs::view::GraphView;
use nicecooling::i18n::{EN, IT};
use nicecooling::screens::graphs::{GraphsModel, GraphsState, GraphsUi};
use nicecooling::screens::{Feedback, ScreenInput};
use nicecooling_graph::model::FileError;
use support::fixtures::online;

const NOW_US: u64 = 1_766_000_000_000_000;

fn model(view: &GraphView, locale: nicecooling::i18n::Locale) -> GraphsModel {
    let state = online();
    let feedback = Feedback::default();
    GraphsModel::build(
        &ScreenInput {
            state: &state,
            now_us: NOW_US,
            locale,
            down_since_us: None,
            feedback: &feedback,
        },
        Some(view),
        &GraphsUi::default(),
    )
}

/// graphs.md, "Graph file not readable": "A file with a higher schema version has the text
/// 'graphs.json has schema version 2. This app reads version 1.'" The screen shows the generic
/// title and the English text of the store, in every language.
#[test]
fn a_graph_file_with_a_higher_schema_version_has_its_own_text() {
    let view = GraphView {
        file_error: Some(FileError::HigherSchemaVersion(2).to_string()),
        ..GraphView::default()
    };
    let english = model(&view, EN);
    assert_eq!(english.state, GraphsState::FileError);
    let (title, detail) = english.file_error.clone().unwrap_or_default();
    let shown = format!("{title} {detail}");
    assert!(
        shown.contains("graphs.json has schema version 2. This app reads version 1."),
        "the screen shows: {shown}"
    );
    // The Italian screen must not show the English text of the store.
    let italian = model(&view, IT);
    let (title, detail) = italian.file_error.unwrap_or_default();
    assert!(
        !format!("{title} {detail}").contains("this version knows"),
        "the Italian screen shows English text: {title} {detail}"
    );
}
