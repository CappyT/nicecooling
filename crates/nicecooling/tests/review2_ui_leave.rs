//! Review reproducers: the question on leaving the Graph editor.

mod support;

use nicecooling::graphs::view::{Applied, GraphRow, GraphView, RowState};
use nicecooling::i18n::EN;
use nicecooling::screens::graph_editor::apply::Leave;
use nicecooling::screens::graph_editor::{EditorModel, EditorUi};
use nicecooling::screens::{Feedback, ScreenInput};
use nicecooling_graph::eval::EndValue;
use nicecooling_graph::model::{Graph, StoredGraph};
use support::fixtures::online;
use support::graphs::constant_graph;

const NOW_US: u64 = 1_766_000_000_000_000;

fn row_of(graph: &Graph) -> GraphRow {
    GraphRow {
        id: graph.id.clone(),
        name: graph.name.clone(),
        enabled: graph.enabled,
        state: RowState::Running,
        values: Default::default(),
        end: EndValue::None,
        stored: StoredGraph::Valid(graph.clone()),
        run: None,
    }
}


/// graph-editor.md, "Leave with a draft": the dialog says "The Graph '...' has changes that are
/// not applied." The user presses "Apply", then the back button while the Apply waits. The
/// service stores the draft. The draft is now applied, but the question stays open and says
/// that the changes are not applied.
#[test]
fn the_question_on_leaving_closes_when_a_waiting_apply_stores_the_draft() {
    let stored = constant_graph("graph_1", 5.0);
    let view = GraphView {
        rows: vec![row_of(&stored)],
        ..GraphView::default()
    };
    let mut ui = EditorUi::open(stored.clone(), &view);
    assert!(ui.draft.move_node(1, 48.0, 48.0));
    assert!(ui.apply(&view, true).is_some(), "Apply sends the draft");
    // The back button while the Apply waits: the question opens with its buttons off.
    assert_eq!(ui.request_leave(Leave::List), None);

    // The service stores the draft and answers.
    let applied = ui.draft.candidate();
    let after = GraphView {
        rows: vec![row_of(&applied)],
        applied: Some(Applied {
            graph: "graph_1".to_owned(),
            result: Ok("graph_1".to_owned()),
        }),
        ..GraphView::default()
    };
    let left = ui.follow(&after, NOW_US);
    assert!(!ui.draft.is_dirty(), "the draft is applied");

    let state = online();
    let feedback = Feedback::default();
    let model = EditorModel::build(
        &ScreenInput {
            state: &state,
            now_us: NOW_US,
            locale: EN,
            down_since_us: None,
            feedback: &feedback,
        },
        &after,
        &ui,
    );
    assert!(
        left.is_some() || model.leave.is_none(),
        "the question stays open for a draft with no change: {:?}",
        model.leave
    );
}
