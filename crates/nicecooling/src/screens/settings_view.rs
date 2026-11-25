//! The window of the Settings screen: one card for each group, one row for each setting.

use gpui::prelude::*;
use gpui::{AnyElement, Context, Window, div, px};
use gpui_component::switch::Switch;
use gpui_component::{h_flex, v_flex};

use crate::i18n::Language;
use crate::icons::{NcIcon, icon};
use crate::screen_view::{ScreenView, skeleton};
use crate::screens::controls::{Pick, act};
use crate::screens::settings::{Change, ConnectionKind, InfoRow, SettingsModel, SwitchRow};
use crate::screens::ui::{
    caption, card, card_header, card_row, field_row, label, notice_row, page,
};
use crate::tokens::Palette;

impl ScreenView {
    pub(crate) fn render_settings(
        &mut self,
        p: &Palette,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let store = self.model.read(cx).settings.clone();
        let model = self.with_input(cx, |input| SettingsModel::build(input, &store));
        let view = cx.entity();

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
