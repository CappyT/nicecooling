//! Review reproducers: work that the Graphs screen does on the UI thread.

mod support;

use std::sync::mpsc;
use std::time::Duration;

use nicecooling::graphs::view::GraphView;
use nicecooling::i18n::EN;
use nicecooling::screens::graphs::GraphsUi;

/// `GraphsView::import_graph` calls `GraphsUi::import_from` inside `this.update`, on the UI
/// thread. The file dialog of the system can return a named pipe. Opening a pipe for reading
/// waits for a writer, so the window stops drawing until someone writes to the pipe.
#[test]
fn import_of_a_named_pipe_returns_at_once() {
    let dir = std::env::temp_dir().join(format!("nc-review2-fifo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("directory");
    let fifo = dir.join("graph.json");
    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo");
    assert!(made.success());
    let (done, finished) = mpsc::channel();
    let path = fifo.clone();
    std::thread::spawn(move || {
        let mut ui = GraphsUi::default();
        let command = ui.import_from(&path, &GraphView::default(), EN);
        let _ = done.send((command.is_some(), ui.notice.is_some()));
    });
    let answer = finished.recv_timeout(Duration::from_secs(3));
    // Free the reader before the assertion, so the thread ends.
    if answer.is_err() {
        let _ = std::fs::OpenOptions::new().write(true).open(&fifo);
    }
    let _ = std::fs::remove_dir_all(&dir);
    assert!(
        answer.is_ok(),
        "the import of a named pipe blocks the UI thread for more than 3 s"
    );
}
