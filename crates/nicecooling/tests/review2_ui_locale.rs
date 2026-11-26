//! Review reproducers: texts of the new screens that are not in the four locale files.

mod support;

use nicecooling::graphs::values::SensorError;
use nicecooling::graphs::view::{ApplyError, GraphRow, GraphView, RowState};
use nicecooling::i18n::{IT, Locale};
use nicecooling::screens::graph_editor::EditorUi;
use nicecooling::screens::graph_editor::apply::refusal_text;
use nicecooling::screens::{Feedback, ScreenInput};
use nicecooling_graph::eval::EndValue;
use nicecooling_graph::model::StoredGraph;
use support::fixtures::online;
use support::graphs::constant_graph;

const NOW_US: u64 = 1_766_000_000_000_000;

fn refusal(error: &ApplyError, locale: Locale) -> String {
    let graph = constant_graph("graph_1", 5.0);
    let view = GraphView {
        rows: vec![GraphRow {
            id: graph.id.clone(),
            name: graph.name.clone(),
            enabled: true,
            state: RowState::Running,
            values: Default::default(),
            end: EndValue::None,
            stored: StoredGraph::Valid(graph.clone()),
            run: None,
        }],
        ..GraphView::default()
    };
    let ui = EditorUi::open(graph, &view);
    let state = online();
    let feedback = Feedback::default();
    let input = ScreenInput {
        state: &state,
        now_us: NOW_US,
        locale,
        down_since_us: None,
        feedback: &feedback,
    };
    refusal_text(error, &input, &view, &ui)
}

/// graph-editor.md, "Daemon refuses at Apply": the toolbar shows the reason. `refusal_text`
/// puts the `Display` text of `SensorError` into the translated frame. That text is English and
/// is in no locale file, so the Italian toolbar shows a sentence in two languages.
#[test]
fn a_refused_apply_has_no_english_text_in_italian() {
    for error in [SensorError::Unavailable, SensorError::Denied] {
        let text = refusal(&ApplyError::Sensor(error.clone()), IT);
        let english = ["no connection to the daemon", "does not allow this user"];
        assert!(
            !english.iter().any(|words| text.contains(words)),
            "the Italian toolbar shows: {text}"
        );
    }
}
