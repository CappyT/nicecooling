rust_i18n::i18n!("locales", fallback = "en");

pub mod app;
pub mod command;
pub mod daemon_client;
pub mod history;
pub mod i18n;
pub mod model;
pub mod notify;
pub mod screen_view;
pub mod screens;
pub mod services;
pub mod shell;
pub mod shell_view;
pub mod tokens;
pub mod tray;
pub mod widgets;
