use nicecooling::app::{self, Backend, StartMode};
use nicecooling::daemon_client::DaemonClient;
use nicecooling::i18n::Language;
use nicecooling::notify::NotifyPrefs;
use nicecooling::services;
use tokio::sync::{broadcast, mpsc};

fn main() {
    let mode = StartMode::from_args(std::env::args());
    // zbus and the sink threads use this runtime. GPUI has none of its own.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("the Tokio runtime");
    let (events_tx, events_rx) = broadcast::channel(1024);
    let (commands_tx, inbox) = mpsc::unbounded_channel();
    let language = Language::System;
    let locale = language.resolve();

    let client = match runtime.block_on(zbus::Connection::system()) {
        Ok(conn) => {
            let client = DaemonClient::new(conn);
            runtime.spawn(client.clone().run(events_tx.clone()));
            Some(client)
        }
        Err(error) => {
            eprintln!("nicecooling: no system bus: {error}");
            None
        }
    };

    services::spawn_tray(events_tx.subscribe(), commands_tx.clone(), locale);
    services::spawn_notifications(
        events_tx.subscribe(),
        commands_tx.clone(),
        locale,
        NotifyPrefs::default(),
    );
    services::spawn_history(events_tx.subscribe(), services::history_path());

    let backend = Backend {
        events: events_rx,
        commands: commands_tx,
        inbox,
        client,
        runtime: runtime.handle().clone(),
        language,
    };
    gpui_platform::application().run(move |cx| {
        app::init(cx, mode, backend);
    });
}
