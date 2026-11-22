use std::thread;
use std::time::Duration;

use gpui::{App, AsyncApp};
use ksni::blocking::TrayMethods;
use notify_rust::{Notification, Timeout};

enum Event {
    Notify,
    Action(String),
    Quit,
}

struct Tray {
    events: async_channel::Sender<Event>,
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "gpui-tray-spike".into()
    }

    fn title(&self) -> String {
        "GPUI tray spike".into()
    }

    fn icon_name(&self) -> String {
        "applications-system".into()
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::StandardItem;
        vec![
            StandardItem {
                label: "Show notification".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.events.send_blocking(Event::Notify);
                }),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|tray: &mut Self| {
                    let _ = tray.events.send_blocking(Event::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

fn show_notification(events: async_channel::Sender<Event>) {
    thread::spawn(move || {
        let handle = Notification::new()
            .appname("gpui-tray-spike")
            .summary("GPUI tray spike")
            .body("Click Acknowledge, or the notification body.")
            .action("default", "Open")
            .action("ack", "Acknowledge")
            .timeout(Timeout::Never)
            .show();
        match handle {
            Ok(handle) => {
                println!("notification shown, id {}", handle.id());
                handle.wait_for_action(|action| {
                    println!("action invoked: {action}");
                    let _ = events.send_blocking(Event::Action(action.to_owned()));
                });
            }
            Err(error) => println!("notification failed: {error}"),
        }
    });
}

async fn handle_events(
    events: async_channel::Receiver<Event>,
    tx: async_channel::Sender<Event>,
    cx: &mut AsyncApp,
) {
    while let Ok(event) = events.recv().await {
        match event {
            Event::Notify => show_notification(tx.clone()),
            Event::Action(action) => println!("gpui foreground received action: {action}"),
            Event::Quit => {
                println!("quit");
                cx.update(|cx| cx.quit());
                break;
            }
        }
    }
}

fn start(cx: &mut App, notify_at_start: bool) {
    gpui_component::init(cx);
    println!("gpui started, windows: {}", cx.windows().len());
    match notify_rust::get_capabilities() {
        Ok(capabilities) => println!("capabilities: {capabilities:?}"),
        Err(error) => println!("capabilities failed: {error}"),
    }

    let (tx, rx) = async_channel::unbounded();
    match (Tray { events: tx.clone() }).spawn() {
        Ok(handle) => {
            println!("tray registered");
            std::mem::forget(handle);
        }
        Err(error) => println!("tray failed: {error}"),
    }

    if notify_at_start {
        let tx = tx.clone();
        thread::spawn(move || {
            thread::sleep(Duration::from_secs(1));
            let _ = tx.send_blocking(Event::Notify);
        });
    }

    cx.spawn(async move |cx| handle_events(rx, tx, cx).await)
        .detach();
}

fn main() {
    let notify_at_start = std::env::args().any(|arg| arg == "--notify");
    gpui_platform::application().run(move |cx| start(cx, notify_at_start));
}
