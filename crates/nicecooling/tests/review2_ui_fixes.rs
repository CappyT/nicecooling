//! Tests of the fixes of the second UI review that have no reproducer of their own: the tick of
//! one second, the keyboard focus over a change of the theme, the editor of a hidden window, the
//! check of the program of a "Command" response, and the texts of each kind of error.

mod support;

use std::cell::Cell;
use std::os::unix::fs::PermissionsExt as _;
use std::rc::Rc;
use std::time::Duration;

use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext, WindowHandle};
use nicecooling::app::{self, AppModel, Backend, StartMode};
use nicecooling::command::Command;
use nicecooling::graphs::channel::GraphCommand;
use nicecooling::graphs::values::SensorError;
use nicecooling::graphs::view::{
    ApplyError, ForeignSensor, GraphRow, GraphView, RowState, SharedView, shared_view,
};
use nicecooling::i18n::{EN, IT, Language, Locale};
use nicecooling::model::DaemonEvent;
use nicecooling::screen_view::ScreenView;
use nicecooling::screens::graph_editor::apply::{Leave, refusal_text};
use nicecooling::screens::graph_editor::canvas::{PLACE_LIMIT, canvas_place};
use nicecooling::screens::graph_editor::response_panel::ProgramCheck;
use nicecooling::screens::graph_editor::{EditorModel, EditorUi};
use nicecooling::screens::graphs::{GraphsModel, GraphsUi};
use nicecooling::screens::{Feedback, ScreenInput};
use nicecooling::shell::Page;
use nicecooling_graph::eval::EndValue;
use nicecooling_graph::model::{FileError, Graph, Kind, Node, StoredGraph};
use support::fixtures::online;
use support::graphs::{Dir, constant_graph};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::{broadcast, mpsc};

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

/// The app on the Graphs page, with a window that holds the screens.
struct Rig {
    // The runtime lives as long as the app.
    _runtime: tokio::runtime::Runtime,
    model: Entity<AppModel>,
    shared: SharedView,
    window: WindowHandle<ScreenView>,
    /// The commands that the screens sent.
    sent: UnboundedReceiver<Command>,
    screen_commands: mpsc::UnboundedSender<Command>,
    /// The inbox of the app: a tray entry sends its command here.
    commands: mpsc::UnboundedSender<Command>,
}

fn screens_window(
    cx: &mut TestAppContext,
    model: &Entity<AppModel>,
    commands: &mpsc::UnboundedSender<Command>,
) -> WindowHandle<ScreenView> {
    let (model, commands) = (model.clone(), commands.clone());
    cx.update(|cx| {
        cx.open_window(gpui::WindowOptions::default(), |_, cx| {
            cx.new(|cx| ScreenView::new(model, commands, cx))
        })
        .expect("a window")
    })
}

fn rig(cx: &mut TestAppContext, rows: Vec<GraphRow>) -> Rig {
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
        language: Language::English,
        settings: Default::default(),
    };
    let model = cx.update(|cx| app::init(cx, StartMode::Background, backend));
    events
        .send(DaemonEvent::Snapshot(Box::new(online().snapshot.clone())))
        .unwrap();
    cx.run_until_parked();
    let shared = shared_view();
    shared.lock().unwrap().rows = rows;
    cx.update(|cx| {
        model.update(cx, |app, _| {
            app.graph_view = Some(shared.clone());
            app.page = Page::Graphs;
        });
    });
    let (screen_commands, sent) = mpsc::unbounded_channel();
    let window = screens_window(cx, &model, &screen_commands);
    Rig {
        _runtime: runtime,
        model,
        shared,
        window,
        sent,
        screen_commands,
        commands,
    }
}

fn draw(cx: &mut TestAppContext, window: WindowHandle<ScreenView>) {
    for _ in 0..2 {
        cx.run_until_parked();
        let mut visual = VisualTestContext::from_window(window.into(), cx);
        visual.update(|window, cx| window.draw(cx).clear(cx));
    }
    cx.run_until_parked();
}

fn end(cx: &mut TestAppContext, rig: Rig) {
    cx.update(|cx| cx.quit());
    drop(rig);
}

/// The tick of one second draws the Graphs page and the Settings page again only when the tick
/// finds new data for the page.
#[gpui::test]
fn the_tick_draws_a_page_only_when_its_data_changed(cx: &mut TestAppContext) {
    let stored = constant_graph("graph_1", 5.0);
    let rig = rig(cx, vec![row_of(&stored)]);
    // The fixture has no countdown and no Profile dwell: these draw at each tick.
    cx.read(|cx| {
        let app = rig.model.read(cx);
        assert!(app.state.next_poweroff().is_none());
        assert_eq!(app.state.snapshot.dwell_until_us, 0);
    });
    let draws = Rc::new(Cell::new(0_u32));
    let seen = draws.clone();
    cx.update(|cx| {
        cx.observe(&rig.model, move |_, _| seen.set(seen.get() + 1))
            .detach();
    });
    let tick = |cx: &mut TestAppContext, count: u32| {
        for _ in 0..count {
            cx.executor().advance_clock(Duration::from_secs(1));
            cx.run_until_parked();
        }
        draws.get()
    };
    // The first tick takes the view as new, and one more draw follows a change.
    let start = tick(cx, 2);
    assert_eq!(
        tick(cx, 5),
        start,
        "the Graphs page drew again with no new data"
    );

    // A new value of a node: one draw for it, and one more for the read of the editor.
    rig.shared.lock().unwrap().rows[0]
        .values
        .insert(1, Some(6.0));
    assert_eq!(tick(cx, 1), start + 1);
    assert_eq!(tick(cx, 1), start + 2);
    assert_eq!(tick(cx, 5), start + 2);

    // A text of the editor that shows for a limited time draws up to the end of that time.
    cx.update(|cx| {
        rig.model.update(cx, |app, _| {
            app.graph_timed_until_us = app::now_us() + 60_000_000
        });
    });
    assert_eq!(tick(cx, 3), start + 5);
    cx.update(|cx| rig.model.update(cx, |app, _| app.graph_timed_until_us = 0));

    // The Settings page: the state of the MQTT client does not change here.
    cx.update(|cx| rig.model.update(cx, |app, _| app.page = Page::Settings));
    let settings = tick(cx, 1);
    assert_eq!(tick(cx, 5), settings, "the Settings page drew again");
    end(cx, rig);
}

/// A change of the theme makes a new canvas. The keyboard focus of the old canvas goes to it.
#[gpui::test]
fn a_change_of_the_theme_keeps_the_focus_of_the_canvas(cx: &mut TestAppContext) {
    // A Graph with its end node only: the middle of the canvas is empty.
    let mut graph = Graph::new("graph_1", "Theme");
    graph.nodes = vec![Node::new(1, Kind::SensorEnd)];
    let mut rig = rig(cx, vec![row_of(&graph)]);
    let window = rig.window;
    window
        .update(cx, |view, _, _| view.graphs_ui().editor = Some(graph))
        .unwrap();
    draw(cx, window);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let editor = visual.debug_bounds("graph-editor").expect("the editor");
    visual.simulate_click(editor.center(), gpui::Modifiers::none());
    draw(cx, window);
    let focused = |cx: &mut TestAppContext| {
        window
            .update(cx, |_, window, cx| window.focused(cx).is_some())
            .unwrap()
    };
    assert!(focused(cx), "a press gives the canvas the focus");

    window
        .update(cx, |_, window, cx| {
            gpui_component::Theme::change(gpui_component::ThemeMode::Dark, Some(window), cx);
        })
        .unwrap();
    draw(cx, window);
    assert!(focused(cx), "the change of the theme dropped the focus");
    // The canvas has the focus: an arrow key selects a node.
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_keystrokes("right");
    draw(cx, window);
    let selected = window
        .update(cx, |view, _, _| {
            view.graph_editor_ui()
                .is_some_and(|ui| ui.selected.is_some())
        })
        .unwrap();
    assert!(selected, "the canvas did not take the arrow key");
    while rig.sent.try_recv().is_ok() {}
    end(cx, rig);
}

/// graph-editor.md, "Leaving": "Hiding the window keeps the editor and the draft." The close
/// button removes the window and its views. The next window opens the editor again with the
/// draft, and "Quit" of the tray then asks.
#[gpui::test]
fn hiding_the_window_keeps_the_editor_and_its_draft(cx: &mut TestAppContext) {
    let stored = constant_graph("graph_1", 5.0);
    let rig = rig(cx, vec![row_of(&stored)]);
    let window = rig.window;
    window
        .update(cx, |view, _, _| view.graphs_ui().editor = Some(stored))
        .unwrap();
    draw(cx, window);
    window
        .update(cx, |view, _, cx| {
            let ui = view.graph_editor_ui().expect("the editor");
            assert!(ui.draft.move_node(1, 48.0, 48.0));
            cx.notify();
        })
        .unwrap();
    draw(cx, window);
    assert!(cx.read(|cx| rig.model.read(cx).graph_draft));

    // The close button of the window.
    window
        .update(cx, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    assert!(cx.read(|cx| rig.model.read(cx).graph_draft));

    // The window that shows next has the editor with the draft.
    let again = screens_window(cx, &rig.model, &rig.screen_commands);
    draw(cx, again);
    let (dirty, x) = again
        .update(cx, |view, _, _| {
            let ui = view.graph_editor_ui().expect("the editor is open again");
            (ui.draft.is_dirty(), ui.draft.graph().node(1).unwrap().x)
        })
        .unwrap();
    assert!(dirty, "the draft is lost");
    assert_eq!(x, 48.0);
    assert!(cx.read(|cx| rig.model.read(cx).graph_draft));

    // "Quit" of the tray: the app does not end, and the editor asks.
    rig.commands.send(Command::Quit).unwrap();
    draw(cx, again);
    let asked = again
        .update(cx, |view, _, _| {
            view.graph_editor_ui().and_then(|ui| ui.apply.leave.clone())
        })
        .unwrap();
    assert_eq!(asked, Some(Leave::Quit), "the editor did not ask");
    end(cx, rig);
}

/// SIGTERM and the end of the session send `Command::Terminate`: the app asks nothing, also
/// with a draft that is not applied.
#[gpui::test]
fn terminate_never_asks(cx: &mut TestAppContext) {
    let rig = rig(cx, Vec::new());
    cx.update(|cx| {
        rig.model.update(cx, |app, _| {
            app.page = Page::Overview;
            app.graph_draft = true;
        });
    });
    rig.commands.send(Command::Terminate).unwrap();
    cx.run_until_parked();
    let (asked, page) = cx.read(|cx| {
        let app = rig.model.read(cx);
        (app.leave_request.clone(), app.page.clone())
    });
    assert_eq!(asked, None, "SIGTERM opened the question");
    assert_eq!(page, Page::Overview);
    drop(rig);
}

/// The check of the program of a "Command" response runs off the UI thread, when the editor
/// opens and when the field "Program" commits. A draw of the panel reads the stored result and
/// not the disk.
#[gpui::test]
fn the_program_check_runs_at_the_open_and_at_a_commit(cx: &mut TestAppContext) {
    let dir = Dir::new("review2-program");
    let program = dir.path().join("program");
    std::fs::write(&program, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = program.to_str().unwrap().to_owned();
    let mut graph = Graph::new("graph_1", "Command");
    graph.nodes = vec![
        Node::new(1, Kind::ResponseEnd)
            .with_param("kind", "command")
            .with_param("path", path.as_str()),
    ];
    let rig = rig(cx, vec![row_of(&graph)]);
    let window = rig.window;
    window
        .update(cx, |view, _, _| view.graphs_ui().editor = Some(graph))
        .unwrap();
    draw(cx, window);
    let check = |cx: &mut TestAppContext| {
        window
            .update(cx, |view, _, _| {
                view.graph_editor_ui().expect("the editor").program.clone()
            })
            .unwrap()
    };
    let result = |executable: bool| {
        Some(ProgramCheck {
            path: path.clone(),
            executable,
        })
    };
    assert_eq!(check(cx), result(true), "the check at the open");

    // The file changes on the disk: a draw does not read it again.
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o644)).unwrap();
    draw(cx, window);
    assert_eq!(check(cx), result(true));

    // A commit of the field checks the program again.
    window
        .update(cx, |view, _, cx| {
            let ui = view.graph_editor_ui().expect("the editor");
            assert!(ui.commit_param(1, "path", &path, EN));
            cx.notify();
        })
        .unwrap();
    draw(cx, window);
    assert_eq!(check(cx), result(false), "the check at the commit");
    end(cx, rig);
}

/// Escape cancels the question of "Delete". The question of "Remove" has the focus on "Cancel"
/// too: Enter removes nothing.
#[gpui::test]
fn escape_cancels_the_delete_question_and_remove_has_the_focus_on_cancel(cx: &mut TestAppContext) {
    let stored = constant_graph("graph_1", 5.0);
    let mut rig = rig(cx, vec![row_of(&stored)]);
    rig.shared.lock().unwrap().foreign_sensors = vec![ForeignSensor {
        id: "graph_7".to_owned(),
        name: "Case air delta".to_owned(),
        unit: "degC".to_owned(),
    }];
    let window = rig.window;
    draw(cx, window);

    window
        .update(cx, |view, _, cx| {
            view.graphs_ui().delete = Some("graph_1".to_owned());
            cx.notify();
        })
        .unwrap();
    draw(cx, window);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_keystrokes("escape");
    draw(cx, window);
    let open = window
        .update(cx, |view, _, _| view.graphs_ui().delete.clone())
        .unwrap();
    assert_eq!(open, None, "Escape did not cancel the question");

    window
        .update(cx, |view, _, cx| {
            view.graphs_ui().remove = Some("graph_7".to_owned());
            cx.notify();
        })
        .unwrap();
    draw(cx, window);
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    visual.simulate_keystrokes("enter");
    draw(cx, window);
    let open = window
        .update(cx, |view, _, _| view.graphs_ui().remove.clone())
        .unwrap();
    assert_eq!(open, None, "Enter did not press Cancel");
    while let Ok(command) = rig.sent.try_recv() {
        assert!(
            !matches!(
                command,
                Command::DeleteGraphSensor { .. } | Command::Graph(GraphCommand::Delete { .. })
            ),
            "{command:?}"
        );
    }
    end(cx, rig);
}

fn input_of<'a>(
    state: &'a nicecooling::model::AppState,
    feedback: &'a Feedback,
    locale: Locale,
) -> ScreenInput<'a> {
    ScreenInput {
        state,
        now_us: NOW_US,
        locale,
        down_since_us: None,
        feedback,
    }
}

/// The store keeps any finite position. The canvas takes a position of 100000 units at most.
#[test]
fn the_canvas_takes_a_node_position_inside_its_limit() {
    assert_eq!(canvas_place(1e300), PLACE_LIMIT as f32);
    assert_eq!(canvas_place(-1e300), -PLACE_LIMIT as f32);
    assert_eq!(canvas_place(48.5), 48.5);
    let mut graph = Graph::new("graph_1", "Far");
    let mut far = Node::new(1, Kind::SensorEnd);
    (far.x, far.y) = (1e300, -1e300);
    graph.nodes = vec![far];
    let view = GraphView {
        rows: vec![row_of(&graph)],
        ..GraphView::default()
    };
    let ui = EditorUi::open(graph, &view);
    let (state, feedback) = (online(), Feedback::default());
    let model = EditorModel::build(&input_of(&state, &feedback, EN), &view, &ui);
    assert_eq!(
        (model.nodes[0].x, model.nodes[0].y),
        (100_000.0, -100_000.0)
    );
    // The draft keeps the stored position: the editor changes nothing by itself.
    assert_eq!(ui.draft.graph().node(1).unwrap().x, 1e300);
    assert!(!ui.draft.is_dirty());
}

/// The import refuses a path that is no regular file, and a file of more than 1 MiB.
#[test]
fn the_import_refuses_a_directory_and_a_large_file() {
    let dir = Dir::new("review2-import");
    let mut ui = GraphsUi::default();
    assert_eq!(ui.import_from(dir.path(), &GraphView::default(), EN), None);
    assert_eq!(
        ui.notice.as_ref().map(|notice| notice.text.as_str()),
        Some("Not imported: the file is not a nicecooling Graph.")
    );
    let large = dir.path().join("large.json");
    std::fs::write(&large, vec![b' '; 1024 * 1024 + 1]).unwrap();
    assert_eq!(ui.import_from(&large, &GraphView::default(), IT), None);
    assert_eq!(
        ui.notice.as_ref().map(|notice| notice.text.as_str()),
        Some("Non importato: il file supera 1 MiB.")
    );
}

/// Each kind of file error of `graphs.json` has its own text in the language of the screen.
#[test]
fn each_file_error_has_its_text() {
    let detail = |error: String, locale: Locale| {
        let view = GraphView {
            file_error: Some(error),
            ..GraphView::default()
        };
        let (state, feedback) = (online(), Feedback::default());
        let model = GraphsModel::build(
            &input_of(&state, &feedback, locale),
            Some(&view),
            &GraphsUi::default(),
        );
        model.file_error.expect("the file error")
    };
    let version = FileError::HigherSchemaVersion(7).to_string();
    assert_eq!(
        detail(version.clone(), EN),
        (
            "nicecooling cannot read graphs.json.".to_owned(),
            "graphs.json has schema version 7. This app reads version 1.".to_owned()
        )
    );
    assert_eq!(
        detail(version, IT).1,
        "graphs.json ha la versione di schema 7. Questa app legge la versione 1."
    );
    // A file that does not parse.
    for error in [
        FileError::NotJson("expected value at line 1 column 1".to_owned()),
        FileError::NotAGraphList,
    ] {
        assert_eq!(
            detail(error.to_string(), EN).1,
            "graphs.json is not a Graph file of nicecooling."
        );
        assert_eq!(
            detail(error.to_string(), IT).1,
            "graphs.json non è un file dei Grafi di nicecooling."
        );
    }
    // A file that the system does not open: its reason shows after the text of the app.
    let denied = "Permission denied (os error 13)".to_owned();
    assert_eq!(
        detail(denied.clone(), EN).1,
        "The app cannot open the file: Permission denied (os error 13)"
    );
    assert_eq!(
        detail(denied, IT).1,
        "L'app non riesce ad aprire il file: Permission denied (os error 13)"
    );
}

/// Each kind of refused Apply has one sentence in the language of the screen. A text that the
/// app does not know shows after such a sentence, as its detail.
#[test]
fn each_refused_apply_has_its_sentence() {
    let graph = constant_graph("graph_1", 5.0);
    let view = GraphView {
        rows: vec![row_of(&graph)],
        ..GraphView::default()
    };
    let ui = EditorUi::open(graph, &view);
    let (state, feedback) = (online(), Feedback::default());
    let text = |error: ApplyError, locale: Locale| {
        refusal_text(&error, &input_of(&state, &feedback, locale), &view, &ui)
    };
    let sensor = |error: SensorError| ApplyError::Sensor(error);
    let cases = [
        (
            sensor(SensorError::Unavailable),
            "Not applied: the app has no connection to the daemon.",
            "Non applicato: l'app non ha una connessione al demone.",
        ),
        (
            sensor(SensorError::Denied),
            "Not applied: the system does not allow this user to change sensors.",
            "Non applicato: il sistema non permette a questo utente di cambiare i sensori.",
        ),
        (
            sensor(SensorError::Missing),
            "Not applied: the Graph sensor does not exist on the daemon.",
            "Non applicato: il sensore Grafo non esiste sul demone.",
        ),
        // The text of the daemon at its limit.
        (
            sensor(SensorError::Refused("at most 64 Graph sensors".to_owned())),
            "Not applied: the limit of 64 Graph sensors is reached.",
            "Non applicato: il limite di 64 sensori Grafo è raggiunto.",
        ),
        (
            sensor(SensorError::Refused("the unit is not known".to_owned())),
            "Not applied: the daemon refused the Graph sensor. Detail: the unit is not known",
            "Non applicato: il demone ha rifiutato il sensore Grafo. Dettaglio: the unit is not known",
        ),
        // The snapshot has no user of the sensor: the reason of the daemon is the detail.
        (
            sensor(SensorError::InUse("graph_3 is read by slot 4".to_owned())),
            "Not applied: something uses the Graph sensor. Detail: graph_3 is read by slot 4",
            "Non applicato: qualcosa usa il sensore Grafo. Dettaglio: graph_3 is read by slot 4",
        ),
        (
            sensor(SensorError::Failed("the bus closed".to_owned())),
            "Not applied: the call to the daemon failed. Detail: the bus closed",
            "Non applicato: la chiamata al demone non è riuscita. Dettaglio: the bus closed",
        ),
        (
            ApplyError::Other("No space left on device".to_owned()),
            "Not applied. Detail: No space left on device",
            "Non applicato. Dettaglio: No space left on device",
        ),
    ];
    for (error, english, italian) in cases {
        assert_eq!(text(error.clone(), EN), english);
        assert_eq!(text(error, IT), italian);
    }
    // The service gives up a call after 120 s, with the text of its language.
    for (locale, wanted) in [
        (EN, "Not applied: the daemon did not answer in 120 s."),
        (IT, "Non applicato: il demone non ha risposto entro 120 s."),
    ] {
        let late = sensor(SensorError::Failed(locale.tr("graph_refusal.no_answer")));
        assert_eq!(text(late, locale), wanted);
    }
}
