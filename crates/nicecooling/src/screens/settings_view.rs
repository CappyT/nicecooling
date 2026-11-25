//! The window of the Settings screen: one card for each group, one row for each setting.

use gpui::prelude::*;
use gpui::{
    AnyElement, Context, Div, Entity, Focusable as _, FontWeight, SharedString, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::spinner::Spinner;
use gpui_component::switch::Switch;
use gpui_component::{h_flex, v_flex};

use crate::command::Command;
use crate::graphs::channel::GraphCommand;
use crate::i18n::Language;
use crate::icons::{NcIcon, icon};
use crate::screen_view::{InputKey, ScreenView, skeleton};
use crate::screens::controls::{Pick, act};
use crate::screens::settings::{
    BrokerState, Change, ConnectionKind, InfoRow, NetworkCard, NetworkChange, NetworkError,
    NetworkField, NetworkRow, SettingsModel, SwitchRow, stored_text,
};
use crate::screens::ui::{
    caption, card, card_header, card_row, field_row, heading, label, notice_row, page,
};
use crate::smtp::settings::SmtpSecurity;
use crate::tokens::Palette;

/// A row of a half of the group "Network": 36 px, the name at the left and the control at the
/// right.
fn net_row(p: &Palette) -> Div {
    h_flex()
        .w_full()
        .min_h(px(36.))
        .px(px(18.))
        .py(px(3.))
        .gap(px(12.))
        .border_t_1()
        .border_color(p.border)
}

/// The identifier of a security in the select.
fn security_id(security: SmtpSecurity) -> &'static str {
    match security {
        SmtpSecurity::Starttls => "starttls",
        SmtpSecurity::Tls => "tls",
        SmtpSecurity::None => "none",
    }
}

impl ScreenView {
    pub(crate) fn render_settings(
        &mut self,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (store, status) = {
            let app = self.model.read(cx);
            (app.settings.clone(), app.mqtt_status())
        };
        let model = self.with_input(cx, |input| {
            SettingsModel::build_with(input, &store, &status, &self.network)
        });
        let view = cx.entity();
        let network_card = self.network_card(&model.network, p, window, cx);

        let pick: Pick = std::rc::Rc::new(|this, id, _, cx| {
            let language = Language::from_setting(id.as_str());
            this.change_setting(Change::Language(language), cx);
        });
        let language_select = self.select_field(
            "settings-language",
            &model.language.shown,
            model
                .language
                .options
                .iter()
                .map(|(language, text)| {
                    (
                        match language {
                            Language::System => "system",
                            Language::English => "en",
                            Language::Italian => "it",
                            Language::German => "de",
                            Language::French => "fr",
                        }
                        .to_owned(),
                        text.clone(),
                        *language == model.language.value,
                    )
                })
                .collect(),
            true,
            false,
            p.track,
            pick,
            p,
            cx,
        );
        let language_card = card(p)
            .child(card_header(model.language_title.clone(), p))
            .child(
                field_row(p)
                    .child(label(model.language.label.clone()).flex_1())
                    .child(div().w(px(180.)).flex_none().child(language_select)),
            );

        let mut start_card = card(p)
            .child(card_header(model.start_title.clone(), p))
            .child(self.switch_row(&model.start, p, cx))
            .child(self.switch_row(&model.tray, p, cx));
        if let Some(hint) = &model.tray_hint {
            start_card = start_card.child(
                notice_row(format!("{} {}", hint.text, hint.link), true, p).child(
                    Self::small_button("tray-hint-dismiss", hint.dismiss.clone(), None).on_click(
                        act(&view, |this, _, cx| {
                            this.change_setting(Change::DismissTrayHint, cx);
                        }),
                    ),
                ),
            );
        }

        let mut notifications = card(p).child(card_header(model.notifications_title.clone(), p));
        for row in &model.notifications {
            notifications = notifications.child(self.switch_row(row, p, cx));
        }
        notifications =
            notifications.child(card_row(p).child(caption(model.notifications_note.clone(), p)));

        let daemon = &model.daemon;
        let mut daemon_card = card(p).child(card_header(daemon.title.clone(), p));
        let connection = &daemon.connection;
        let color = match connection.kind {
            ConnectionKind::Connected => p.success,
            ConnectionKind::Offline | ConnectionKind::Connecting => p.offline,
            ConnectionKind::Down => p.alarm,
        };
        let mut connection_row = field_row(p)
            .child(label(connection.label.clone()).flex_1())
            .child(match connection.kind {
                ConnectionKind::Down => icon(NcIcon::Alarm, color).into_any_element(),
                ConnectionKind::Offline => icon(NcIcon::Off, color).into_any_element(),
                _ => div()
                    .size(px(8.))
                    .flex_none()
                    .rounded_full()
                    .bg(color)
                    .into_any_element(),
            })
            .child(div().flex_none().child(connection.value.clone()));
        if let Some(text) = &connection.try_again {
            connection_row = connection_row.child(
                Self::small_button("settings-try-again", text.clone(), None).on_click(act(
                    &view,
                    |this, _, cx| {
                        this.model.update(cx, |app, cx| {
                            app.retry();
                            cx.notify();
                        });
                    },
                )),
            );
        }
        daemon_card = daemon_card.child(connection_row);
        if let Some(text) = &connection.text {
            daemon_card = daemon_card.child(card_row(p).child(text.clone()));
        }
        for command in &connection.commands {
            daemon_card = daemon_card
                .child(card_row(p).child(div().font_family("Adwaita Mono").child(command.clone())));
        }
        if let Some(text) = &daemon.mismatch {
            daemon_card = daemon_card.child(notice_row(text.clone(), true, p));
        }
        for row in &daemon.rows {
            daemon_card = daemon_card.child(info_row(row, daemon.loading, p));
        }
        if let Some(text) = &daemon.board_note {
            daemon_card = daemon_card.child(card_row(p).child(caption(text.clone(), p)));
        }

        let about = &model.about;
        let mut about_card = card(p).child(card_header(about.title.clone(), p));
        for row in &about.rows {
            about_card = about_card.child(info_row(row, false, p));
        }
        about_card = about_card
            .child(
                field_row(p)
                    .child(label(about.project_label.clone()).flex_none())
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_right()
                            .text_color(p.accent)
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(about.project_url),
                    ),
            )
            .child(card_row(p).child(caption(about.note.clone(), p)))
            .child(card_row(p).child(caption(about.firmware.clone(), p)));

        let mut content = page()
            .child(language_card)
            .child(start_card)
            .child(notifications)
            .child(network_card)
            .child(daemon_card)
            .child(about_card);
        if let Some(error) = &model.error {
            content = content.child(notice_row(error.clone(), true, p));
        }
        content.into_any_element()
    }

    fn switch_row(&self, row: &SwitchRow, p: &Palette, cx: &mut Context<Self>) -> AnyElement {
        let view = cx.entity();
        let key = row.key;
        // One row for each setting: the name and its caption at the left, the control at the
        // right.
        field_row(p)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(row.label.clone())
                    .children(row.text.clone().map(|text| caption(text, p))),
            )
            .child(
                Switch::new(gpui::SharedString::from(format!("setting-{:?}", row.key)))
                    .checked(row.on)
                    .on_click(move |on, _, cx| {
                        let on = *on;
                        view.update(cx, |this, cx| {
                            this.change_setting(key.change(on), cx);
                        })
                    }),
            )
            .into_any_element()
    }

    /// The card "Network": the MQTT broker at the left and the SMTP account at the right.
    fn network_card(
        &mut self,
        model: &NetworkCard,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let connection = &model.connection;
        let state = h_flex().gap(px(6.)).items_center();
        let state = match connection.state {
            BrokerState::Off => state.text_color(p.fg_dim),
            BrokerState::Connecting => state.child(Spinner::new()),
            BrokerState::Connected => state
                .font_weight(FontWeight::SEMIBOLD)
                .child(div().size(px(8.)).flex_none().rounded_full().bg(p.success)),
            BrokerState::Failed => state
                .font_weight(FontWeight::SEMIBOLD)
                .child(icon(NcIcon::Alarm, p.alarm)),
        }
        .child(connection.value.clone());
        let mut broker = v_flex()
            .flex_1()
            .min_w_0()
            .border_r_1()
            .border_color(p.border)
            .child(net_row(p).child(heading(model.mqtt_title.clone())))
            .child(
                net_row(p)
                    .child(label(connection.label.clone()).flex_1())
                    .child(
                        v_flex()
                            .flex_none()
                            .max_w(px(260.))
                            .items_end()
                            .child(state)
                            .children(connection.caption.clone().map(|text| caption(text, p))),
                    ),
            );
        for row in &model.mqtt {
            broker = broker.child(self.network_row(row, p, window, cx));
        }
        if let Some(text) = &model.mqtt_warning {
            broker = broker.child(notice_row(text.clone(), true, p));
        }
        let mut account = v_flex()
            .flex_1()
            .min_w_0()
            .child(net_row(p).child(heading(model.smtp_title.clone())));
        for row in &model.smtp {
            account = account.child(self.network_row(row, p, window, cx));
        }
        if let Some(text) = &model.smtp_warning {
            account = account.child(notice_row(text.clone(), true, p));
        }
        card(p)
            .child(card_header(model.title.clone(), p))
            // The two halves have the same height: the line between them runs to the note.
            .child(div().flex().w_full().child(broker).child(account))
            .child(card_row(p).child(caption(model.note.clone(), p)))
            .into_any_element()
    }

    fn network_row(
        &mut self,
        row: &NetworkRow,
        p: &Palette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let view = cx.entity();
        match row {
            NetworkRow::Input(input) => {
                let state = if input.password {
                    self.password_input(input.field, &input.placeholder, window, cx)
                } else {
                    self.input(
                        InputKey::Network(input.field),
                        &input.value,
                        &input.placeholder,
                        window,
                        cx,
                    )
                };
                let unit = input.unit.clone().map(SharedString::from);
                let control = Self::field_input(&state, unit, true, p);
                let control = if input.error.is_some() {
                    control.border_color(p.alarm)
                } else {
                    control
                };
                let width = if input.short { 126. } else { 200. };
                v_flex()
                    .w_full()
                    .child(
                        net_row(p)
                            .child(label(input.label.clone()).flex_1())
                            .child(div().w(px(width)).flex_none().child(control)),
                    )
                    // The reason of a refused text shows under the input.
                    .children(input.error.clone().map(|text| {
                        div()
                            .px(px(18.))
                            .pb(px(6.))
                            .child(caption(text, p).text_color(p.alarm))
                    }))
                    .into_any_element()
            }
            NetworkRow::Tls { label: name, on } => {
                net_row(p)
                    .child(label(name.clone()).flex_1())
                    .child(Switch::new("settings-mqtt-tls").checked(*on).on_click(
                        move |on, _, cx| {
                            let on = *on;
                            view.update(cx, |this, cx| {
                                let _ = this.change_network(NetworkChange::MqttTls(on), cx);
                            })
                        },
                    ))
                    .into_any_element()
            }
            NetworkRow::CaFile {
                label: name,
                path,
                none,
                choose,
            } => net_row(p)
                .child(label(name.clone()).flex_none())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_right()
                        .text_color(p.fg_dim)
                        .child(path.clone().unwrap_or_else(|| none.clone())),
                )
                .child(
                    Self::small_button("settings-mqtt-ca", choose.clone(), None)
                        .on_click(act(&view, |this, _, cx| this.choose_ca_file(cx))),
                )
                .into_any_element(),
            NetworkRow::Security {
                label: name,
                value,
                shown,
                options,
            } => {
                let pick: Pick = std::rc::Rc::new(|this, id, _, cx| {
                    let security = match id.as_str() {
                        "tls" => SmtpSecurity::Tls,
                        "none" => SmtpSecurity::None,
                        _ => SmtpSecurity::Starttls,
                    };
                    let _ = this.change_network(NetworkChange::Security(security), cx);
                });
                let select = self.select_field(
                    "settings-smtp-security",
                    shown,
                    options
                        .iter()
                        .map(|(security, text)| {
                            (
                                security_id(*security).to_owned(),
                                text.clone(),
                                security == value,
                            )
                        })
                        .collect(),
                    true,
                    true,
                    p.track,
                    pick,
                    p,
                    cx,
                );
                net_row(p)
                    .child(label(name.clone()).flex_1())
                    .child(div().w(px(200.)).flex_none().child(select))
                    .into_any_element()
            }
        }
    }

    /// The input of a password. It never holds the stored password: it starts empty, and it
    /// shows `hint` as dim text, which is eight dots while a password is set.
    fn password_input(
        &mut self,
        field: NetworkField,
        hint: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        let key = InputKey::Network(field);
        if !self.inputs.contains_key(&key) {
            let shown = hint.to_owned();
            let state = cx.new(|cx| InputState::new(window, cx).masked(true).placeholder(shown));
            let subscribed = key.clone();
            cx.subscribe_in(
                &state,
                window,
                move |this: &mut Self, state, event: &InputEvent, window, cx| {
                    this.on_input(&subscribed, state, event, window, cx);
                },
            )
            .detach();
            self.inputs.insert(key.clone(), state);
            self.password_hints.insert(field, hint.to_owned());
        }
        let state = self.input(key, "", hint, window, cx);
        if self.password_hints.get(&field).map(String::as_str) != Some(hint) {
            let shown = hint.to_owned();
            state.update(cx, |input, cx| input.set_placeholder(shown, window, cx));
            self.password_hints.insert(field, hint.to_owned());
        }
        state
    }

    /// "Choose": the file dialog of the system for the CA file.
    fn choose_ca_file(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(mut paths))) = paths.await
                && let Some(path) = paths.pop()
            {
                let _ = this.update(cx, |this, cx| {
                    let file = path.display().to_string();
                    let _ = this.change_network(NetworkChange::MqttCaFile(file), cx);
                });
            }
        })
        .detach();
    }

    /// Apply a change of the group "Network" at once. The MQTT client gets the new config, and
    /// the Graph service reads the settings again for its e-mail and MQTT responses.
    pub(crate) fn change_network(
        &mut self,
        change: NetworkChange,
        cx: &mut Context<Self>,
    ) -> Result<(), NetworkError> {
        let result = self.model.update(cx, |app, cx| {
            let result = crate::screens::settings::change_network(
                &mut app.settings,
                app.mqtt.as_ref(),
                change,
            );
            cx.notify();
            result
        });
        if result.is_ok() {
            self.send(Command::Graph(GraphCommand::NetworkSettingsEdited));
        }
        cx.notify();
        result
    }

    /// Enter or the loss of the focus in an input of the group "Network". A text that the app
    /// refuses stays in the input, and the input keeps the focus, until the user corrects it or
    /// presses Escape.
    pub(crate) fn commit_network(
        &mut self,
        field: NetworkField,
        text: &str,
        input: &Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let unchanged = !field.is_password()
            && text.trim() == stored_text(&self.model.read(cx).settings, field);
        let result = if unchanged {
            Ok(())
        } else {
            self.change_network(NetworkChange::Field(field, text.to_owned()), cx)
        };
        match result {
            Ok(()) => {
                if self
                    .network
                    .refused
                    .is_some_and(|(refused, _)| refused == field)
                {
                    self.network.refused = None;
                }
            }
            Err(error) => {
                self.network.refused = Some((field, error));
                self.edited = Some(InputKey::Network(field));
                let focus = input.read(cx).focus_handle(cx);
                window.focus(&focus, cx);
            }
        }
    }

    /// Apply a switch or the language at once. A new language changes the window now.
    pub(crate) fn change_setting(&mut self, change: Change, cx: &mut Context<Self>) {
        self.model.update(cx, |app, cx| {
            crate::screens::settings::apply(&mut app.settings, change);
            if matches!(change, Change::Language(_)) {
                app.locale = app.settings.values.language().resolve();
                gpui_component::set_locale(app.locale.code());
            }
            cx.notify();
        });
        cx.notify();
    }
}

/// A row with a fact: the name at the left, the value at the right.
fn info_row(row: &InfoRow, loading: bool, p: &Palette) -> AnyElement {
    field_row(p)
        .child(label(row.label.clone()).flex_none())
        .child(if loading && row.from_daemon {
            h_flex()
                .flex_1()
                .justify_end()
                .child(skeleton(p, 120.))
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .text_right()
                .text_color(p.fg_dim)
                .child(row.value.clone())
                .into_any_element()
        })
        .into_any_element()
}
