//! Review reproducers: "Quit" with a draft of the Graph editor that is not applied.

mod support;

use gpui::TestAppContext;
use nicecooling::app::{self, Backend, LeaveRequest, StartMode};
use nicecooling::command::Command;
use nicecooling::i18n::Language;
use nicecooling::shell::Page;
use tokio::sync::{broadcast, mpsc};

/// graph-editor.md, "Leaving": "The back button, a sidebar entry and "Quit" ask when the draft
/// is not applied." and "Hiding the window keeps the editor and the draft." The tray menu has
/// "Quit" too (tray-and-notifications.md). Its command goes to `Command::Quit`, which quits
/// at once: the draft is lost without the question.
#[gpui::test]
fn quit_of_the_tray_asks_first_with_a_draft(cx: &mut TestAppContext) {
    let runtime = tokio::runtime::Runtime::new().expect("runtime");
    let (_events, events_rx) = broadcast::channel(64);
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
    cx.update(|cx| {
        model.update(cx, |app, _| {
            app.page = Page::Overview;
            app.graph_draft = true;
        })
    });
    // The tray sends this command for its entry "Quit".
    commands.send(Command::Quit).unwrap();
    cx.run_until_parked();
    let (asked, page) = cx.read(|cx| {
        let app = model.read(cx);
        (app.leave_request.clone(), app.page.clone())
    });
    assert_eq!(
        asked,
        Some(LeaveRequest::Quit),
        "the app quit without the question"
    );
    assert_eq!(page, Page::Graphs);
}
