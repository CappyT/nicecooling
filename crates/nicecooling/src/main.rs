use nicecooling::app::{self, Backend, StartMode};
use nicecooling::app_settings::SettingsStore;
use nicecooling::daemon_client::DaemonClient;
use nicecooling::graphs::store::GraphPaths;
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
    let settings = SettingsStore::open();
    let language = settings.values.language();
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

    let graph_view = nicecooling::graphs::view::shared_view();
    if settings.values.tray {
        services::spawn_tray(
            events_tx.subscribe(),
            commands_tx.clone(),
            locale,
            settings.tray_visible.clone(),
            graph_view.clone(),
        );
    }
    let (notify_tx, notify_rx) = mpsc::unbounded_channel();
    services::spawn_notifications(
        events_tx.subscribe(),
        notify_rx,
        commands_tx.clone(),
        locale,
        settings.notify_prefs.clone(),
    );
    services::spawn_history(events_tx.subscribe(), services::history_path());
    let graphs = services::spawn_graphs(
        events_tx.subscribe(),
        GraphPaths::user(),
        notify_tx,
        locale,
        graph_view,
    );
    // The end of the session takes the path of "Quit".
    if let Err(error) = services::spawn_sigterm(runtime.handle(), commands_tx.clone()) {
        eprintln!("nicecooling: no SIGTERM handler: {error}");
    }

    let backend = Backend {
        events: events_rx,
        commands: commands_tx,
        inbox,
        client,
        graphs: Some(graphs),
        runtime: runtime.handle().clone(),
        language,
        settings,
    };
    gpui_platform::application()
        .with_assets(nicecooling::icons::Assets)
        .run(move |cx| {
            app::init(cx, mode, backend);
        });
}
