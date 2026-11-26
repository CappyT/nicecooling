//! The window of the app shell. `ShellModel` decides what shows. This file only draws it.

use gpui::prelude::*;
use gpui::{
    AnyElement, ClickEvent, Context, Decorations, Div, Entity, FocusHandle, FontWeight,
    IntoElement, KeyDownEvent, MouseButton, MouseMoveEvent, Render, Stateful, Window,
    WindowControlArea, deferred, div, px, rgb,
};
use gpui_component::button::{Button, ButtonCustomVariant, ButtonVariants as _};
use gpui_component::spinner::Spinner;
use gpui_component::{Sizable as _, h_flex, v_flex};
use tokio::sync::mpsc::UnboundedSender;

use crate::app::AppModel;
use crate::command::Command;
use crate::graphs::channel::GraphCommand;
use crate::icons::{NcIcon, icon, small_icon, text_icon};
use crate::model::{Link, Severity};
use crate::screen_view::ScreenView;
use crate::screens::ui::Tip as _;
use crate::shell::{Banner, BannerAction, BannerStyle, NavBadge, NavEntry, Page, ShellModel};
use crate::tokens::Palette;

gpui::actions!(nicecooling, [FocusNext, FocusPrev]);

/// Tab and Shift+Tab in the shell. The component library binds them at its root: these
/// bindings are deeper, so the shell can keep the focus in an open dialog.
pub fn bind_keys(cx: &mut gpui::App) {
    cx.bind_keys([
        gpui::KeyBinding::new("tab", FocusNext, Some("Shell")),
        gpui::KeyBinding::new("shift-tab", FocusPrev, Some("Shell")),
    ]);
}

/// Move the focus to the next or the previous control. While a dialog is open, the focus
/// stays in it. The countdown dialog draws over every other dialog: it keeps the focus first.
fn move_focus(
    countdown: Option<&FocusHandle>,
    back: bool,
    window: &mut Window,
    cx: &mut gpui::App,
) {
    if let Some(scope) = countdown {
        crate::screens::ui::tab_in(scope, None, back, window, cx);
        return;
    }
    if crate::screens::ui::dialog_tab(back, window, cx) {
        return;
    }
    if back {
        window.focus_prev(cx);
    } else {
        window.focus_next(cx);
    }
}

pub struct ShellView {
    model: Entity<AppModel>,
    commands: UnboundedSender<Command>,
    focus: FocusHandle,
    menu_open: bool,
    down_dialog_open: bool,
    notices_open: bool,
    /// The Alarm whose countdown dialog the user hid with "Hide".
    countdown_hidden: Option<String>,
    countdown_focus: FocusHandle,
    /// The focus of the countdown dialog: its buttons are in it.
    countdown_scope: FocusHandle,
    /// True while the countdown dialog shows.
    countdown_open: bool,
    /// What had the focus when the countdown dialog opened. It gets the focus back.
    countdown_opener: Option<FocusHandle>,
    /// The focus of "Cancel poweroff" in the banner: it stays in the Tab order of a dialog.
    banner_focus: FocusHandle,
    /// The Alarm that had the focus for its dialog: the focus moves once, when it opens.
    countdown_focused: Option<String>,
    /// The screen of every page.
    screens: Entity<ScreenView>,
    /// A press on the header bar or on the app name of the sidebar: the next pointer move
    /// starts a window move.
    bar_press: bool,
}

/// The sidebar icon of a page, as the comps show it.
fn nav_icon(page: &Page) -> NcIcon {
    match page {
        Page::Overview | Page::FirstRun => NcIcon::Home,
        Page::Loops => NcIcon::Loop,
        Page::Channels => NcIcon::Fan,
        Page::Sensors => NcIcon::Gauge,
        Page::Leakshield(_) => NcIcon::Shield,
        Page::Profiles => NcIcon::Layers,
        Page::Graphs => NcIcon::Graph,
        Page::Alarms => NcIcon::Bell,
        Page::History => NcIcon::Chart,
        Page::Settings => NcIcon::Sliders,
    }
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
        // The desktop portal gives the system theme after the app starts, and the user can
        // change it while the app runs.
        gpui_component::Theme::sync_system_appearance(Some(window), cx);
        cx.observe_window_appearance(window, |_, window, cx| {
            gpui_component::Theme::sync_system_appearance(Some(window), cx);
        })
        .detach();
        let screens = cx.new(|cx| ScreenView::new(model.clone(), commands.clone(), cx));
        Self {
            model,
            commands,
            screens,
            focus,
            menu_open: false,
            down_dialog_open: false,
            notices_open: false,
            countdown_hidden: None,
            countdown_focus: cx.focus_handle(),
            countdown_scope: cx.focus_handle(),
            countdown_open: false,
            countdown_opener: None,
            banner_focus: cx.focus_handle().tab_stop(true),
            countdown_focused: None,
            bar_press: false,
        }
    }

    fn go(&mut self, page: Page, cx: &mut Context<Self>) {
        self.model.update(cx, |app, cx| {
            app.request_page(page);
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
            self.screens.update(cx, |screens, cx| {
                screens.close_popovers();
                // The dialogs of the Display tab have no list: Escape closes them.
                screens.leakshield_ui().display.close_dialog();
                cx.notify();
            });
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
            BannerAction::TurnResponsesOn => {
                let _ = commands.send(Command::Graph(GraphCommand::ResponsesOn));
            }
        }
    }

    fn render_banner(
        &self,
        index: usize,
        banner: &Banner,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (ground, ink, line, mark) = match banner.style {
            BannerStyle::Alarm => (p.alarm_bg, p.alarm_fg, p.alarm_bg, NcIcon::Alarm),
            BannerStyle::Offline => (p.offline_tint, p.fg, p.border, NcIcon::Off),
            BannerStyle::Warning => (p.warning_tint, p.fg, p.border, NcIcon::Warn),
        };
        let alarm = banner.style == BannerStyle::Alarm;
        let mark_color = match banner.style {
            BannerStyle::Alarm => p.alarm_fg,
            BannerStyle::Offline => p.offline,
            BannerStyle::Warning => p.warning,
        };
        let mut row = h_flex()
            .id(("banner", index))
            .min_h(px(if alarm { 64. } else { 46. }))
            .px(px(24.))
            .py(px(6.))
            .gap(px(12.))
            .bg(ground)
            .border_b_1()
            .border_color(line)
            .text_color(ink)
            .child(icon(mark, mark_color).with_size(px(if alarm { 24. } else { 20. })))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .when(alarm, |title| title.text_size(px(16.)).line_height(px(22.)))
                            .child(banner.title.clone()),
                    )
                    .children(banner.detail.clone().map(|detail| div().child(detail))),
            );
        if let Some(countdown) = &banner.countdown {
            row = row.child(
                div()
                    .flex_none()
                    .text_size(px(28.))
                    .line_height(px(32.))
                    .font_weight(FontWeight::EXTRA_BOLD)
                    .child(countdown.clone()),
            );
        }
        if let Some((label, action)) = &banner.button {
            let button = Button::new(("banner-button", index))
                .h(px(34.))
                .px(px(15.))
                .rounded(px(9.))
                .font_weight(FontWeight::BOLD)
                .label(label.clone())
                .on_click(self.banner_action(action.clone(), cx));
            let button = if matches!(action, BannerAction::CancelPoweroff { .. }) {
                button
                    .track_focus(&self.banner_focus)
                    .debug_selector(|| "banner-cancel".to_owned())
            } else if matches!(action, BannerAction::TurnResponsesOn) {
                button.debug_selector(|| "banner-responses-on".to_owned())
            } else {
                button
            };
            // On the Alarm field the button is white with dark red text.
            let on_alarm = ButtonCustomVariant::new(cx)
                .color(gpui::white())
                .foreground(rgb(0xA51D2D).into())
                .hover(rgb(0xF6F5F4).into())
                .active(rgb(0xDEDDDA).into());
            row = row.child(if alarm {
                // The custom variant draws its color as a tint: the white fill is set here.
                button.custom(on_alarm).bg(gpui::white())
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
        let badge: Option<AnyElement> = entry.badge.as_ref().map(|badge| match badge {
            NavBadge::Count(count) => div()
                .font_weight(FontWeight::BOLD)
                .child(count.to_string())
                .into_any_element(),
            NavBadge::Severity(Severity::Alarm) => icon(NcIcon::Alarm, p.alarm).into_any_element(),
            NavBadge::Severity(Severity::Warning) => {
                icon(NcIcon::Warn, p.warning).into_any_element()
            }
            NavBadge::Offline => icon(NcIcon::Off, p.offline).into_any_element(),
        });
        h_flex()
            .id(format!("nav-{}", entry.label))
            .debug_selector(|| format!("nav-{}", entry.label))
            .h(px(34.))
            .flex_none()
            .px(px(9.))
            .gap(px(9.))
            .rounded(px(9.))
            .map(|row| {
                if active {
                    row.bg(p.selected).font_weight(FontWeight::SEMIBOLD)
                } else {
                    row.hover(|row| row.bg(p.hover))
                }
            })
            .on_click(move |_, _, cx| view.update(cx, |this, cx| this.go(page.clone(), cx)))
            .child(icon(nav_icon(&entry.page), p.fg))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .child(entry.label.clone()),
            )
            .children(badge)
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
            .border_r_1()
            .border_color(p.border)
            .px(px(6.))
            .pb(px(6.))
            .gap(px(2.))
            .child(
                // The app name is at the height of the header bar: it moves the window too.
                Self::moves_window(
                    div()
                        .id("app-name")
                        .h(px(46.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .font_weight(FontWeight::BOLD),
                    cx,
                )
                .child("nicecooling"),
            );
        for group in &shell.groups {
            if let Some(title) = &group.title {
                column = column.child(
                    div()
                        .flex_none()
                        .px(px(9.))
                        .pt(px(12.))
                        .pb(px(3.))
                        .text_size(px(12.))
                        .line_height(px(16.))
                        .font_weight(FontWeight::BOLD)
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

    /// Make `bar` a part of the window frame: a press and then a pointer move start a window
    /// move, and a double click maximizes or restores the window. A button in the bar takes
    /// its press (it prevents the default), so its click does not move the window.
    fn moves_window(bar: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        bar.window_control_area(WindowControlArea::Drag)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.bar_press = !window.default_prevented();
                    // A press that moves the window must not start a text selection that
                    // follows the pointer after the move.
                    if this.bar_press {
                        gpui_component::GlobalState::suppress_text_selection(cx);
                    }
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.bar_press = false),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, _| {
                let pressed = event.pressed_button == Some(MouseButton::Left);
                if this.bar_press && pressed {
                    window.start_window_move();
                }
                this.bar_press = false;
            }))
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    window.zoom_window();
                }
            })
    }

    /// The header bar: the page title in the center, the primary menu button and the close
    /// button at the right. The bar is the title bar of the window.
    fn render_header(
        &self,
        title: String,
        subtitle: Option<String>,
        p: &Palette,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let locale = self.model.read(cx).locale;
        // With server-side decorations the window manager draws its own title bar and close
        // button over the header bar: a second close button here would be a duplicate.
        let client_frame = matches!(window.window_decorations(), Decorations::Client { .. });
        let menu_button = Button::new("primary-menu")
            .ghost()
            .size(px(28.))
            .rounded(px(7.))
            .icon(text_icon(NcIcon::Menu))
            .tip(locale.tr("menu.primary"))
            .on_click(self.toggle(|this| &mut this.menu_open, cx));
        // The close button hides the window. The app stays in the tray.
        let close_button = Button::new("window-close")
            .custom(
                ButtonCustomVariant::new(cx)
                    .color(p.selected)
                    .foreground(p.fg)
                    .hover(p.border_strong)
                    .active(p.border_strong),
            )
            // The custom variant draws its color as a tint: the fill is set here.
            .bg(p.selected)
            .size(px(24.))
            .p_0()
            .rounded(px(12.))
            .child(small_icon(NcIcon::X, p.fg))
            .tip(locale.tr("menu.close"))
            .on_click(|_, window, _| window.remove_window());
        Self::moves_window(
            div()
                .id("header-bar")
                .relative()
                .h(px(46.))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .px(px(96.))
                .bg(p.headerbar)
                .border_b_1()
                .border_color(p.border)
                .debug_selector(|| "shell-header".to_owned()),
            cx,
        )
        .when(client_frame, |bar| {
            bar.on_mouse_down(MouseButton::Right, |event, window, _| {
                window.show_window_menu(event.position)
            })
        })
        .child(
            v_flex()
                .min_w_0()
                .items_center()
                .debug_selector(|| "header-title".to_owned())
                .child(
                    div()
                        .max_w_full()
                        .truncate()
                        .font_weight(FontWeight::BOLD)
                        .line_height(px(18.))
                        .child(title),
                )
                .children(subtitle.map(|text| {
                    div()
                        .max_w_full()
                        .truncate()
                        .text_size(px(12.))
                        .line_height(px(14.))
                        .text_color(p.fg_dim)
                        .child(text)
                })),
        )
        .child(
            h_flex()
                .id("header-controls")
                .absolute()
                .top_0()
                .right(px(12.))
                .h_full()
                .gap(px(6.))
                // A click on a button bubbles to the bar: a double click there must not
                // maximize the window.
                .on_click(|_, _, cx| cx.stop_propagation())
                .child(menu_button)
                .when(client_frame, |controls| {
                    controls.child(
                        div()
                            .debug_selector(|| "header-close".to_owned())
                            .child(close_button),
                    )
                }),
        )
    }

    fn render_status_bar(
        &self,
        shell: &ShellModel,
        p: &Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let status = &shell.status;
        let view = cx.entity();
        let to_page = |page: Page| {
            let view = view.clone();
            move |_: &ClickEvent, _: &mut Window, cx: &mut gpui::App| {
                view.update(cx, |this, cx| this.go(page.clone(), cx))
            }
        };
        let small = |name: NcIcon, color| icon(name, color).with_size(px(14.));
        let daemon = h_flex()
            .id("status-daemon")
            .flex_none()
            .gap(px(6.))
            .on_click(to_page(Page::Settings));
        let daemon = match status.link {
            Link::Online => daemon.child(div().size(px(8.)).rounded_full().bg(p.ok_dot)),
            Link::Down => daemon
                .text_color(p.alarm)
                .font_weight(FontWeight::BOLD)
                .child(small(NcIcon::Alarm, p.alarm)),
            Link::Offline | Link::Connecting => daemon
                .text_color(p.offline)
                .child(small(NcIcon::Off, p.offline)),
        }
        .child(status.daemon.clone());
        let alarms = h_flex()
            .id("status-alarms")
            .flex_none()
            .gap(px(6.))
            .on_click(to_page(Page::Alarms));
        let alarms = match status.alarm_severity {
            Some(Severity::Alarm) => alarms
                .text_color(p.alarm)
                .font_weight(FontWeight::BOLD)
                .child(small(NcIcon::Alarm, p.alarm)),
            Some(Severity::Warning) => alarms
                .text_color(p.warning)
                .font_weight(FontWeight::BOLD)
                .child(small(NcIcon::Warn, p.warning)),
            None => alarms,
        }
        .child(status.alarms.clone());
        h_flex()
            .h(px(28.))
            .flex_none()
            .debug_selector(|| "shell-status".to_owned())
            .px(px(24.))
            .gap(px(18.))
            .overflow_hidden()
            .border_t_1()
            .border_color(p.border)
            .text_size(px(12.))
            .line_height(px(16.))
            .text_color(p.fg_dim)
            .child(daemon)
            .children(status.profile.clone().map(|text| {
                div()
                    .id("status-profile")
                    .flex_none()
                    .on_click(to_page(Page::Profiles))
                    .child(text)
            }))
            .child(alarms)
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
            .top(px(42.))
            .right(px(12.))
            .p(px(6.))
            .rounded(px(12.))
            .bg(p.headerbar)
            .border_1()
            .border_color(p.border)
            .child(item("menu-shortcuts", locale.tr("menu.shortcuts")))
            .child(item("menu-about", locale.tr("menu.about")))
            .child(
                // A draft of the Graph editor that is not applied asks first: the app model
                // decides, as for "Quit" of the tray.
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

        let cancels = |banner: &Banner| {
            matches!(
                banner.button,
                Some((_, BannerAction::CancelPoweroff { .. }))
            )
        };
        let cancel_shown = shell.banners.iter().any(cancels)
            || (self.notices_open && shell.more_notices.iter().any(cancels));
        crate::screens::ui::set_dialog_extra(
            cancel_shown.then_some(&self.banner_focus),
            window,
            cx,
        );
        let mut banners = v_flex().flex_none();
        for (index, banner) in shell.banners.iter().enumerate() {
            let row = self.render_banner(index, banner, &p, cx).into_any_element();
            // The scrim of a dialog covers the window. The poweroff banner draws over it:
            // "Cancel poweroff" works on every screen while a countdown runs.
            banners = banners.child(if banner.countdown.is_some() {
                deferred(row).with_priority(2).into_any_element()
            } else {
                row
            });
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
            self.screens.clone().into_any_element()
        };
        // A device page shows the device label, with the serial under it.
        let (title, subtitle) = match &page {
            Page::Leakshield(id) => {
                let frame = crate::screen_view::with_screen_input(&self.model, cx, |input| {
                    crate::screens::leakshield::Frame::build(input, id, false)
                });
                (frame.title, Some(frame.subtitle))
            }
            other => (locale.tr(other.title_key()), None),
        };

        // The countdown of an Alarm has the priority over the countdown of a Graph response.
        let graph_countdown = self.model.read(cx).graph_countdown();
        let countdown = crate::screen_view::with_screen_input(&self.model, cx, |input| {
            crate::screens::alarms::CountdownDialog::build(input)
        })
        .or_else(|| {
            graph_countdown
                .map(|found| crate::screens::alarms::CountdownDialog::for_graph(&found, locale))
        })
        .filter(|dialog| self.countdown_hidden.as_ref() != Some(&dialog.instance));
        if let Some(dialog) = &countdown
            && self.countdown_focused.as_ref() != Some(&dialog.instance)
        {
            self.countdown_focused = Some(dialog.instance.clone());
            if !self.countdown_open {
                self.countdown_opener = window.focused(cx);
            }
            window.focus(&self.countdown_focus, cx);
        }
        if self.countdown_open && countdown.is_none() {
            // The buttons of the dialog go away. A focus that was on them goes back.
            let focused = window.focused(cx);
            let opener = self.countdown_opener.take();
            if focused.is_none()
                || self.countdown_focus.is_focused(window)
                || self.countdown_scope.is_focused(window)
            {
                window.focus(opener.as_ref().unwrap_or(&self.focus), cx);
            }
        }
        self.countdown_open = countdown.is_some();
        let countdown_element = countdown.map(|dialog| {
            let hide_view = cx.entity();
            let (commands, command) = (self.commands.clone(), dialog.cancel_command());
            let instance = dialog.instance.clone();
            crate::screens::countdown_view::countdown_overlay(
                &dialog,
                &self.countdown_scope,
                &self.countdown_focus,
                move |_, cx| {
                    let _ = commands.send(command.clone());
                    let _ = cx;
                },
                move |window, cx| {
                    let instance = instance.clone();
                    hide_view.update(cx, |this, cx| {
                        this.countdown_hidden = Some(instance);
                        // The button that had the focus goes away: the keys go to the shell.
                        window.focus(&this.focus, cx);
                        cx.notify();
                    });
                },
                &p,
            )
        });

        let header = self
            .render_header(title, subtitle, &p, window, cx)
            .into_any_element();

        // `h_flex` centers its children on the cross axis. This row must stretch them: the
        // sidebar and the content column take the height of the window.
        div()
            .id("shell")
            .key_context("Shell")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &FocusNext, window, cx| {
                let scope = this.countdown_open.then_some(&this.countdown_scope);
                move_focus(scope, false, window, cx)
            }))
            .on_action(cx.listener(|this, _: &FocusPrev, window, cx| {
                let scope = this.countdown_open.then_some(&this.countdown_scope);
                move_focus(scope, true, window, cx)
            }))
            .on_key_down(
                cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    this.on_key(event, window, cx)
                }),
            )
            .flex()
            .size_full()
            .bg(p.window)
            .text_color(p.fg)
            .text_size(px(14.))
            .line_height(px(20.))
            .font_features(crate::tokens::font_features())
            .child(self.render_sidebar(&shell, &page, &p, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(header)
                    .child(banners)
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .debug_selector(|| "shell-content".to_owned())
                            .child(content),
                    )
                    .child(self.render_status_bar(&shell, &p, cx)),
            )
            .when(self.menu_open, |root| {
                root.child(self.render_menu(&shell, &p, cx))
            })
            .when(self.down_dialog_open, |root| {
                root.child(self.render_down_dialog(&p, cx))
            })
            // The countdown dialog draws over a dialog of a screen and over the banner.
            .children(countdown_element.map(|dialog| deferred(dialog).with_priority(3)))
    }
}
