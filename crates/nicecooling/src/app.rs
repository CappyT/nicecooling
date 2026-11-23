//! The GPUI side: the app model, the window and the loops that feed them.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use gpui::{App, AppContext as _, Entity, QuitMode};
use gpui_component::{Root, Theme};
use tokio::sync::broadcast;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use crate::command::Command;
use crate::daemon_client::DaemonClient;
use crate::i18n::{Language, Locale};
use crate::model::{AppState, DaemonEvent, Link, Severity};
use crate::screens::{Feedback, WriteOutcome};
use crate::shell::{Page, ShellModel};
use crate::shell_view::ShellView;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartMode {
    Normal,
    /// `nicecooling --background`: no window, the tray icon only.
    Background,
}

impl StartMode {
    pub fn from_args(args: impl IntoIterator<Item = String>) -> Self {
        if args.into_iter().any(|arg| arg == "--background") {
            Self::Background
        } else {
            Self::Normal
        }
    }
}

/// What the GPUI side needs from the rest of the process.
pub struct Backend {
    pub events: broadcast::Receiver<DaemonEvent>,
    /// For the window: a button sends a command here.
    pub commands: UnboundedSender<Command>,
    /// Every command of the tray, the notifications and the window.
    pub inbox: UnboundedReceiver<Command>,
    pub client: Option<DaemonClient>,
    pub runtime: tokio::runtime::Handle,
    pub language: Language,
}

/// The state that the window draws.
pub struct AppModel {
    pub state: AppState,
    pub page: Page,
    pub locale: Locale,
    pub now_us: u64,
    /// The result of the last write of each control.
    pub feedback: Feedback,
    /// When the daemon left the bus, for the text "Since 21:52:07".
    pub down_since_us: Option<u64>,
    /// The history database that the History screen reads.
    pub history_path: Option<std::path::PathBuf>,
    connecting_since: Instant,
}

impl AppModel {
    pub fn new(locale: Locale) -> Self {
        Self {
            state: AppState::default(),
            page: Page::Overview,
            locale,
            now_us: now_us(),
            feedback: Feedback::default(),
            down_since_us: None,
            history_path: Some(crate::services::history_path()),
            connecting_since: Instant::now(),
        }
    }

    /// Apply a daemon event, and note when the link went away.
    pub fn apply(&mut self, event: &DaemonEvent) {
        self.state.apply(event);
        self.now_us = now_us();
        match event {
            DaemonEvent::Link(Link::Offline | Link::Down) => {
                self.down_since_us.get_or_insert(self.now_us);
            }
            DaemonEvent::Link(_) | DaemonEvent::Snapshot(_) => self.down_since_us = None,
            _ => {}
        }
    }

    /// "Try again": show "Connecting" again. The offline state returns after 2 s.
    pub fn retry(&mut self) {
        self.state.apply(&DaemonEvent::Link(Link::Connecting));
        self.connecting_since = Instant::now();
    }

    /// "Connecting" lasts 2 s at most.
    fn connecting_expired(&self) -> bool {
        self.state.link == Link::Connecting && self.connecting_since.elapsed() >= CONNECTING_LIMIT
    }

    pub fn shell(&self) -> ShellModel {
        ShellModel::build(&self.state, self.now_us, self.locale)
    }
}

const CONNECTING_LIMIT: Duration = Duration::from_secs(2);

pub fn now_us() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_micros() as u64)
}

/// A new Alarm or a down daemon opens the window by itself.
fn needs_attention(event: &DaemonEvent) -> bool {
    match event {
        DaemonEvent::AlarmRaised(alarm) => alarm.severity == Severity::Alarm,
        DaemonEvent::Link(link) => *link == Link::Down,
        _ => false,
    }
}

/// Start the app. In background mode no window opens.
pub fn init(cx: &mut App, mode: StartMode, backend: Backend) -> Entity<AppModel> {
    let Backend {
        mut events,
        commands,
        mut inbox,
        client,
        runtime,
        language,
    } = backend;
    let locale = language.resolve();
    gpui_component::init(cx);
    gpui_component::set_locale(locale.code());
    // Closing the window hides it: only "Quit" ends the app.
    cx.set_quit_mode(QuitMode::Explicit);
    Theme::sync_system_appearance(None, cx);
    let model = cx.new(|_| AppModel::new(locale));

    cx.spawn({
        let model = model.clone();
        let commands = commands.clone();
        async move |cx| {
            loop {
                match events.recv().await {
                    Ok(event) => cx.update(|cx| {
                        model.update(cx, |app, cx| {
                            app.apply(&event);
                            cx.notify();
                        });
                        if needs_attention(&event) {
                            model.update(cx, |app, _| app.page = Page::Overview);
                            show_window(cx, &model, &commands);
                        }
                    }),
                    Err(broadcast::error::RecvError::Lagged(_)) => {}
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    })
    .detach();

    cx.spawn({
        let model = model.clone();
        let commands = commands.clone();
        async move |cx| {
            while let Some(command) = inbox.recv().await {
                match command {
                    Command::OpenWindow(page) => cx.update(|cx| {
                        if let Some(page) = page {
                            model.update(cx, |app, cx| {
                                app.page = page;
                                cx.notify();
                            });
                        }
                        show_window(cx, &model, &commands);
                    }),
                    Command::Quit => cx.update(|cx| cx.quit()),
                    daemon_command => {
                        let Some(client) = client.clone() else {
                            continue;
                        };
                        let key = daemon_command.feedback_key();
                        if let Some(key) = &key {
                            cx.update(|cx| {
                                model.update(cx, |app, cx| {
                                    app.feedback.set(key.clone(), WriteOutcome::Pending);
                                    cx.notify();
                                });
                            });
                        }
                        // Each write has its own task: a polkit prompt can last, and the
                        // next command (a poweroff cancel) must not wait for it.
                        let task = runtime.spawn(async move {
                            let result = daemon_command.run_on_daemon(&client).await;
                            if let Err(error) = &result {
                                eprintln!("nicecooling: {daemon_command:?}: {error}");
                            }
                            result
                        });
                        let model = model.clone();
                        cx.spawn(async move |cx| {
                            let result = task.await;
                            let Some(key) = key else { return };
                            let outcome = match result {
                                Ok(Ok(_)) => WriteOutcome::Applied { at_us: now_us() },
                                Ok(Err(error)) => WriteOutcome::from_error(&error),
                                Err(error) => WriteOutcome::Refused {
                                    error: error.to_string(),
                                },
                            };
                            cx.update(|cx| {
                                model.update(cx, |app, cx| {
                                    app.feedback.set(key, outcome);
                                    cx.notify();
                                });
                            });
                        })
                        .detach();
                    }
                }
            }
        }
    })
    .detach();

    // One tick per second: the countdown shows live, and "Connecting" ends after 2 s.
    cx.spawn({
        let model = model.clone();
        async move |cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                cx.update(|cx| {
                    model.update(cx, |app, cx| {
                        if app.connecting_expired() {
                            app.state.apply(&DaemonEvent::Link(Link::Offline));
                            cx.notify();
                        } else if app.state.next_poweroff().is_some() {
                            app.now_us = now_us();
                            cx.notify();
                        }
                    });
                });
            }
        }
    })
    .detach();

    if mode == StartMode::Normal {
        show_window(cx, &model, &commands);
    }
    model
}

/// Open the window, or bring the open one to the front.
pub fn show_window(cx: &mut App, model: &Entity<AppModel>, commands: &UnboundedSender<Command>) {
    if let Some(handle) = cx.windows().first().copied() {
        let _ = handle.update(cx, |_, window, _| window.activate_window());
        return;
    }
    let options = gpui_component::TitleBar::window_options();
    let model = model.clone();
    let commands = commands.clone();
    let opened = cx.open_window(options, move |window, cx| {
        let view = cx.new(|cx| ShellView::new(model, commands, window, cx));
        cx.new(|cx| Root::new(view, window, cx))
    });
    if let Err(error) = opened {
        eprintln!("nicecooling: cannot open the window: {error}");
    }
    cx.activate(true);
}
