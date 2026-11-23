//! The window of the app shell. `ShellModel` decides what shows. This file only draws it.

use gpui::prelude::*;
use gpui::{
    ClickEvent, Context, Entity, FocusHandle, Hsla, IntoElement, KeyDownEvent, Render, Window, div,
    px,
};
use gpui_component::button::{Button, ButtonVariants as _};
use gpui_component::spinner::Spinner;
use gpui_component::{Sizable as _, TitleBar, h_flex, v_flex};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::AppModel;
use crate::command::Command;
use crate::model::{Link, Severity};
use crate::shell::{Banner, BannerAction, BannerStyle, NavBadge, NavEntry, Page, ShellModel};
use crate::tokens::Palette;

pub struct ShellView {
    model: Entity<AppModel>,
    commands: UnboundedSender<Command>,
    focus: FocusHandle,
    menu_open: bool,
    down_dialog_open: bool,
    notices_open: bool,
}

impl ShellView {
    pub fn new(
        model: Entity<AppModel>,
        commands: UnboundedSender<Command>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        cx.observe(&model, |_, _, cx| cx.notify()).detach();
        Self {
            model,
            commands,
            focus,
            menu_open: false,
            down_dialog_open: false,
            notices_open: false,
        }
    }

    fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        self.model.update(cx, |app, cx| {
            app.page = page;
            cx.notify();
        });
    }

    fn toggle(
        &self,
        field: fn(&mut Self) -> &mut bool,
        cx: &mut Context<Self>,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static {
        let view = cx.entity();
        move |_, _, cx| {
            view.update(cx, |this, cx| {
                let flag = field(this);
                *flag = !*flag;
                cx.notify();
            });
        }
    }

    fn on_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let keys = &event.keystroke;
        if keys.modifiers.alt
            && let Ok(number) = keys.key.parse::<usize>()
            && (1..=9).contains(&number)
        {
            let pages = self.model.read(cx).shell().pages();
            if let Some(page) = pages.get(number - 1) {
                self.go(page.clone(), cx);
            }
        } else if keys.key == "f10" {
            self.menu_open = !self.menu_open;
            cx.notify();
        } else if keys.modifiers.control && keys.key == "w" {
            window.remove_window();
        } else if keys.key == "escape" {
            // Escape closes a popover or a dialog. It never cancels a poweroff.
            self.menu_open = false;
            self.down_dialog_open = false;
            self.notices_open = false;
            cx.notify();
        }
    }

    fn banner_action(
        &self,
        action: BannerAction,
        cx: &mut Context<Self>,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut gpui::App) + 'static {
        let view = cx.entity();
        let commands = self.commands.clone();
        move |_, _, cx| match action.clone() {
            BannerAction::CancelPoweroff { instance } => {
                let _ = commands.send(Command::CancelPoweroff { instance });
            }
            BannerAction::TryAgain => view.update(cx, |this, cx| {
                this.model.update(cx, |app, cx| {
                    app.retry();
                    cx.notify();
                });
            }),
            BannerAction::ShowNotices => view.update(cx, |this, cx| {
                this.notices_open = !this.notices_open;
                cx.notify();
            }),
            BannerAction::ShowHowToStart => view.update(cx, |this, cx| {
                this.down_dialog_open = true;
                cx.notify();
            }),
            BannerAction::OpenFields { leakshield } => {
                view.update(cx, |this, cx| {
                    this.go(Page::Leakshield(leakshield.clone()), cx)
                });
            }
            BannerAction::OpenProfiles => view.update(cx, |this, cx| this.go(Page::Profiles, cx)),
        }
    }

    fn render_banner(
        &self,
        index: usize,
        banner: &Banner,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (ground, ink) = match banner.style {
            BannerStyle::Alarm => (p.alarm_bg, p.alarm_fg),
            BannerStyle::Offline => (p.offline_tint, p.fg),
            BannerStyle::Warning => (p.warning_tint, p.fg),
        };
        let height = if banner.countdown.is_some() {
            64.0
        } else {
            46.0
        };
        let mut row = h_flex()
            .id(("banner", index))
            .min_h(px(height))
            .px(px(24.))
            .gap(px(12.))
            .items_center()
            .bg(ground)
            .text_color(ink)
            .child(
                v_flex()
                    .flex_1()
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(banner.title.clone()),
                    )
                    .children(
                        banner
                            .detail
                            .clone()
                            .map(|detail| div().text_sm().child(detail)),
                    ),
            );
        if let Some(countdown) = &banner.countdown {
            row = row.child(
                div()
                    .text_size(px(28.))
                    .font_weight(gpui::FontWeight::EXTRA_BOLD)
                    .child(countdown.clone()),
            );
        }
        if let Some((label, action)) = &banner.button {
            let button = Button::new(("banner-button", index))
                .small()
                .label(label.clone())
                .on_click(self.banner_action(action.clone(), cx));
            row = row.child(if banner.style == BannerStyle::Alarm {
                button.danger()
            } else {
                button
            });
        } else {
            row = row.on_click(self.toggle(|this| &mut this.notices_open, cx));
        }
        row
    }

    fn render_nav_entry(
        &self,
        entry: &NavEntry,
        active: bool,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let page = entry.page.clone();
        let view = cx.entity();
        let badge = entry.badge.as_ref().map(|badge| match badge {
            NavBadge::Count(count) => (count.to_string(), p.fg_dim),
            NavBadge::Severity(Severity::Alarm) => ("!".to_owned(), p.alarm),
            NavBadge::Severity(Severity::Warning) => ("!".to_owned(), p.warning),
            NavBadge::Offline => ("--".to_owned(), p.offline),
        });
        h_flex()
            .id(format!("nav-{}", entry.label))
            .h(px(34.))
            .px(px(12.))
            .gap(px(6.))
            .items_center()
            .rounded(px(9.))
            .when(active, |row| {
                row.bg(p.selected).font_weight(gpui::FontWeight::SEMIBOLD)
            })
            .hover(|row| row.bg(p.hover))
            .on_click(move |_, _, cx| view.update(cx, |this, cx| this.go(page.clone(), cx)))
            .child(div().flex_1().truncate().child(entry.label.clone()))
            .children(badge.map(|(text, color)| div().text_color(color).child(text)))
    }

    fn render_sidebar(
        &self,
        shell: &ShellModel,
        page: &Page,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut column = v_flex()
            .w(px(220.))
            .h_full()
            .flex_none()
            .bg(p.sidebar)
            .p(px(6.))
            .gap(px(3.))
            .child(
                div()
                    .px(px(12.))
                    .py(px(12.))
                    .font_weight(gpui::FontWeight::BOLD)
                    .child("nicecooling"),
            );
        for group in &shell.groups {
            if let Some(title) = &group.title {
                column = column.child(
                    div()
                        .px(px(12.))
                        .pt(px(12.))
                        .text_size(px(12.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(p.fg_dim)
                        .child(title.clone()),
                );
            }
            for entry in &group.entries {
                column = column.child(self.render_nav_entry(entry, &entry.page == page, p, cx));
            }
        }
        column.child(div().flex_1()).child(self.render_nav_entry(
            &shell.settings,
            &shell.settings.page == page,
            p,
            cx,
        ))
    }

    fn render_status_bar(
        &self,
        shell: &ShellModel,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let status = &shell.status;
        let dot: Hsla = match status.link {
            Link::Online => p.ok_dot,
            Link::Down => p.alarm,
            Link::Offline | Link::Connecting => p.offline,
        };
        let view = cx.entity();
        let to_page = |page: Page| {
            let view = view.clone();
            move |_: &ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                view.update(cx, |this, cx| this.go(page.clone(), cx))
            }
        };
        h_flex()
            .h(px(28.))
            .px(px(12.))
            .gap(px(18.))
            .items_center()
            .flex_none()
            .bg(p.headerbar)
            .border_t_1()
            .border_color(p.border)
            .text_size(px(12.))
            .child(
                h_flex()
                    .id("status-daemon")
                    .gap(px(6.))
                    .items_center()
                    .on_click(to_page(Page::Settings))
                    .child(div().size(px(8.)).rounded_full().bg(dot))
                    .child(status.daemon.clone()),
            )
            .children(status.profile.clone().map(|text| {
                div()
                    .id("status-profile")
                    .on_click(to_page(Page::Profiles))
                    .child(text)
            }))
            .child(div().flex_1())
            .child(
                div()
                    .id("status-alarms")
                    .on_click(to_page(Page::Alarms))
                    .text_color(match status.alarm_severity {
                        Some(Severity::Alarm) => p.alarm,
                        Some(Severity::Warning) => p.warning,
                        None => p.fg_dim,
                    })
                    .child(status.alarms.clone()),
            )
    }

    fn render_menu(
        &self,
        shell: &ShellModel,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let locale = self.model.read(cx).locale;
        let commands = self.commands.clone();
        let item = |id: &'static str, label: String| Button::new(id).ghost().small().label(label);
        let _ = shell;
        v_flex()
            .absolute()
            .top(px(40.))
            .right(px(8.))
            .p(px(6.))
            .rounded(px(12.))
            .bg(p.headerbar)
            .border_1()
            .border_color(p.border)
            .child(item("menu-shortcuts", locale.tr("menu.shortcuts")))
            .child(item("menu-about", locale.tr("menu.about")))
            .child(
                item("menu-quit", locale.tr("menu.quit")).on_click(move |_, _, _| {
                    let _ = commands.send(Command::Quit);
                }),
            )
    }

    fn render_down_dialog(&self, p: &Palette, cx: &mut Context<Self>) -> impl IntoElement {
        let locale = self.model.read(cx).locale;
        div()
            .absolute()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                v_flex()
                    .w(px(480.))
                    .p(px(18.))
                    .gap(px(12.))
                    .rounded(px(15.))
                    .bg(p.headerbar)
                    .border_1()
                    .border_color(p.border)
                    .child(
                        div()
                            .text_size(px(18.))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(locale.tr("dialog.daemon_down.title")),
                    )
                    .child(locale.tr("dialog.daemon_down.body"))
                    .child(
                        div()
                            .font_family("monospace")
                            .child("systemctl status nicecoolingd"),
                    )
                    .child(
                        div()
                            .font_family("monospace")
                            .child("systemctl restart nicecoolingd"),
                    )
                    .child(
                        h_flex().justify_end().child(
                            Button::new("down-close")
                                .label(locale.tr("dialog.daemon_down.close"))
                                .on_click(self.toggle(|this| &mut this.down_dialog_open, cx)),
                        ),
                    ),
            )
    }
}

impl Render for ShellView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (shell, page, locale) = {
            let app = self.model.read(cx);
            (app.shell(), app.page.clone(), app.locale)
        };
        let p = Palette::new(gpui_component::Theme::global(cx).is_dark());
        let writes = shell.writes.clone();

        let mut banners = v_flex().flex_none();
        for (index, banner) in shell.banners.iter().enumerate() {
            banners = banners.child(self.render_banner(index, banner, &p, cx));
        }
        if self.notices_open {
            for (index, banner) in shell.more_notices.iter().enumerate() {
                banners = banners.child(self.render_banner(100 + index, banner, &p, cx));
            }
        }

        let content = if shell.connecting {
            v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap(px(12.))
                .child(Spinner::new())
                .child(locale.tr("status.connecting"))
                .into_any_element()
        } else {
            // The screens of tasks 8.5 and later replace this placeholder. A screen reads
            // `writes` to disable its write controls.
            let title = match &page {
                Page::Leakshield(id) => format!("{} {id}", locale.tr(page.title_key())),
                other => locale.tr(other.title_key()),
            };
            v_flex()
                .size_full()
                .p(px(24.))
                .gap(px(12.))
                .child(
                    div()
                        .text_size(px(18.))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(title),
                )
                .children(
                    writes
                        .tooltip
                        .clone()
                        .map(|text| div().text_color(p.fg_dim).child(text)),
                )
                .into_any_element()
        };

        let menu_button = Button::new("primary-menu")
            .ghost()
            .small()
            .label(locale.tr("menu.primary"))
            .on_click(self.toggle(|this| &mut this.menu_open, cx));
        let _ = window;

        v_flex()
            .id("shell")
            .key_context("Shell")
            .track_focus(&self.focus)
            .on_key_down(
                cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    this.on_key(event, window, cx)
                }),
            )
            .size_full()
            .bg(p.window)
            .text_color(p.fg)
            .child(
                TitleBar::new().child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .child(
                            div()
                                .flex_1()
                                .text_center()
                                .font_weight(gpui::FontWeight::BOLD)
                                .child(locale.tr(page.title_key())),
                        )
                        .child(menu_button),
                ),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_sidebar(&shell, &page, &p, cx))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(banners)
                            .child(div().flex_1().overflow_hidden().child(content)),
                    ),
            )
            .child(self.render_status_bar(&shell, &p, cx))
            .when(self.menu_open, |root| {
                root.child(self.render_menu(&shell, &p, cx))
            })
            .when(self.down_dialog_open, |root| {
                root.child(self.render_down_dialog(&p, cx))
            })
    }
}
