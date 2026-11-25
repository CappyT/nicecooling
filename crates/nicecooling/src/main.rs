use nicecooling::app::{self, Backend, StartMode};
use nicecooling::app_settings::SettingsStore;
use nicecooling::daemon_client::DaemonClient;
use nicecooling::graphs::secrets::Secrets;
use nicecooling::graphs::store::GraphPaths;
use nicecooling::instance::{self, Claim};
use nicecooling::mqtt::client::MqttConfig;
use nicecooling::mqtt::settings::hostname;
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
    // One instance for each user session: a second start shows the window of the first and ends.
    // The connection stays open until the end of `main`: it holds the name.
    let _instance = match runtime.block_on(instance::claim(None, commands_tx.clone())) {
        Claim::First(connection) => Some(connection),
        Claim::Taken => {
            if mode == StartMode::Normal
                && let Err(error) = runtime.block_on(instance::show_first(None))
            {
                eprintln!(
                    "nicecooling: cannot ask the running instance to show its window: {error}"
                );
            }
            return;
        }
        Claim::NoBus(error) => {
            eprintln!("nicecooling: no session bus, so no single-instance guard: {error}");
            None
        }
    };
    let settings = SettingsStore::open();
    let language = settings.values.language();
    let locale = language.resolve();

    let system = match runtime.block_on(zbus::Connection::system()) {
        Ok(conn) => {
            let client = DaemonClient::new(conn.clone());
            runtime.spawn(client.clone().run(events_tx.clone()));
            Some((client, conn))
        }
        Err(error) => {
            eprintln!("nicecooling: no system bus: {error}");
            None
        }
    };
    let client = system.as_ref().map(|(client, _)| client.clone());

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
    // MQTT stays off while the host is empty. The app model holds the client, so the Settings screen
    // can show its state and give it a new config. The Graph service takes the channels of the MQTT
    // message response.
    let broker_password = Secrets::open(&GraphPaths::user().secrets).mqtt;
    let services::MqttService {
        client: mqtt_client,
        responses,
        done,
    } = services::spawn_mqtt(
        runtime.handle(),
        events_tx.subscribe(),
        MqttConfig::new(&settings.values.mqtt, &broker_password, &hostname()),
        settings.hidden_sensors.clone(),
    );
    let graph_system = services::graph_system(system, runtime.handle(), graph_view.clone());
    let graphs = services::spawn_graphs(
        events_tx.subscribe(),
        GraphPaths::user(),
        notify_tx,
        locale,
        graph_view,
        graph_system,
        services::GraphNetwork {
            runtime: runtime.handle().clone(),
            responses,
            done,
        },
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
            let model = app::init(cx, mode, backend);
            model.update(cx, |app, _| app.mqtt = Some(mqtt_client));
        });
}
