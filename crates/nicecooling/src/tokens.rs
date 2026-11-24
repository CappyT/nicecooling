//! The color tokens of `docs/ui/DESIGN.md`, for the parts that the app draws itself, and the
//! gpui-component theme that gives its controls the same tokens.

use std::rc::Rc;

use gpui::{App, Hsla, SharedString, rgb};
use gpui_component::{Theme, ThemeConfig, ThemeMode};

pub struct Palette {
    /// The dark theme: a card has no shadow there.
    pub dark: bool,
    pub window: Hsla,
    pub sidebar: Hsla,
    pub headerbar: Hsla,
    pub fg: Hsla,
    pub fg_dim: Hsla,
    pub border: Hsla,
    pub selected: Hsla,
    pub hover: Hsla,
    pub accent: Hsla,
    pub accent_tint: Hsla,
    pub alarm_bg: Hsla,
    pub alarm_fg: Hsla,
    pub alarm: Hsla,
    pub warning: Hsla,
    pub warning_tint: Hsla,
    pub offline: Hsla,
    pub offline_tint: Hsla,
    pub ok_dot: Hsla,
    pub card: Hsla,
    pub view: Hsla,
    pub border_strong: Hsla,
    pub track: Hsla,
    pub accent_bg: Hsla,
    pub alarm_tint: Hsla,
    pub warning_bg: Hsla,
    pub success: Hsla,
    pub water: Hsla,
    pub air: Hsla,
    pub part: Hsla,
    /// The chart series `s1` to `s6`, in order.
    pub series: [Hsla; 6],
}

impl Palette {
    pub fn new(dark: bool) -> Self {
        let pick = |light: u32, dark_value: u32| -> Hsla {
            rgb(if dark { dark_value } else { light }).into()
        };
        Self {
            dark,
            window: pick(0xFAFAFB, 0x222226),
            sidebar: pick(0xEBEBED, 0x2E2E32),
            headerbar: pick(0xFFFFFF, 0x2E2E32),
            fg: pick(0x2A2A2E, 0xFFFFFF),
            fg_dim: pick(0x5E5E66, 0xB4B4BC),
            border: pick(0xDCDCE0, 0x47474D),
            selected: pick(0xD9D9DD, 0x4A4A50),
            hover: pick(0xF0F0F2, 0x3E3E43),
            accent: pick(0x0461BE, 0x78AEED),
            accent_tint: pick(0xE3EEFB, 0x263A55),
            alarm_bg: rgb(0xC01C28).into(),
            alarm_fg: rgb(0xFFFFFF).into(),
            alarm: pick(0xC01C28, 0xFF938C),
            warning: pick(0x905400, 0xFFC252),
            warning_tint: pick(0xFDF3D9, 0x3D3421),
            offline: pick(0x6B6A70, 0xA3A3AB),
            offline_tint: pick(0xEBEBED, 0x2E2E32),
            ok_dot: rgb(0x2EC27E).into(),
            card: pick(0xFFFFFF, 0x36363A),
            view: pick(0xFFFFFF, 0x1D1D20),
            border_strong: pick(0xB8B8C0, 0x6A6A72),
            track: pick(0xE4E4E8, 0x444449),
            accent_bg: rgb(0x1C71D8).into(),
            alarm_tint: pick(0xFBE4E5, 0x4A2226),
            warning_bg: rgb(0xE5A50A).into(),
            success: pick(0x1B8553, 0x78E9AB),
            water: pick(0x1C71D8, 0x62A0EA),
            air: pick(0x77767B, 0x9A9996),
            part: pick(0xF6F5F4, 0x2A2A2E),
            series: [
                pick(0x1C71D8, 0x78AEED),
                pick(0x2190A4, 0x5BC8DC),
                pick(0x9141AC, 0xC29EE6),
                pick(0xD56199, 0xF08DBB),
                pick(0x3A944A, 0x7ED492),
                pick(0x6F8396, 0xA7B4C2),
            ],
        }
    }
}

/// `color` as `#RRGGBB`, for a theme file.
fn hex(color: Hsla) -> SharedString {
    let rgb = color.to_rgb();
    let byte = |channel: f32| (channel * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", byte(rgb.r), byte(rgb.g), byte(rgb.b)).into()
}

/// The first family of the design that has a bold face of its own. GPUI loads a variable font
/// at its default weight only, and Adwaita Sans often ships as one: its headings are not bold.
fn ui_font(cx: &App) -> Option<SharedString> {
    let text = cx.text_system();
    let installed = text.all_font_names();
    ["Adwaita Sans", "Inter"]
        .into_iter()
        .find(|family| {
            let regular = gpui::font(*family);
            installed.iter().any(|name| name == family)
                && text.resolve_font(&regular) != text.resolve_font(&regular.clone().bold())
        })
        .map(SharedString::from)
}

/// The gpui-component theme of one mode, from the tokens.
fn theme_config(dark: bool, font: Option<SharedString>, mono: Option<SharedString>) -> ThemeConfig {
    let p = Palette::new(dark);
    let mut config = ThemeConfig::default();
    config.name = if dark {
        "nicecooling dark"
    } else {
        "nicecooling light"
    }
    .into();
    config.mode = if dark {
        ThemeMode::Dark
    } else {
        ThemeMode::Light
    };
    config.font_family = font;
    config.mono_font_family = mono;
    config.radius = Some(9);
    config.radius_lg = Some(15);
    config.shadow = Some(false);
    let colors = &mut config.colors;
    colors.background = Some(hex(p.window));
    colors.foreground = Some(hex(p.fg));
    colors.border = Some(hex(p.border));
    colors.input = Some(hex(p.selected));
    colors.ring = Some(hex(p.accent_bg));
    colors.caret = Some(hex(p.fg));
    colors.link = Some(hex(p.accent));
    colors.muted = Some(hex(p.track));
    colors.muted_foreground = Some(hex(p.fg_dim));
    colors.accent = Some(hex(p.hover));
    colors.accent_foreground = Some(hex(p.fg));
    colors.popover = Some(hex(p.card));
    colors.popover_foreground = Some(hex(p.fg));
    colors.primary = Some(hex(p.accent_bg));
    colors.primary_foreground = Some(hex(p.alarm_fg));
    colors.button = Some(hex(p.selected));
    colors.button_hover = Some(hex(p.border_strong));
    colors.button_active = Some(hex(p.border_strong));
    colors.button_foreground = Some(hex(p.fg));
    colors.button_primary = Some(hex(p.accent_bg));
    colors.button_primary_foreground = Some(hex(p.alarm_fg));
    colors.secondary = Some(hex(p.selected));
    colors.secondary_hover = Some(hex(p.hover));
    colors.secondary_active = Some(hex(p.selected));
    colors.secondary_foreground = Some(hex(p.fg));
    colors.danger = Some(hex(p.alarm_bg));
    colors.danger_foreground = Some(hex(p.alarm_fg));
    colors.button_danger = Some(hex(p.alarm_bg));
    colors.button_danger_foreground = Some(hex(p.alarm_fg));
    colors.warning = Some(hex(p.warning_bg));
    colors.success = Some(hex(p.success));
    colors.switch = Some(hex(p.border_strong));
    colors.slider_bar = Some(hex(p.accent_bg));
    colors.progress_bar = Some(hex(p.accent_bg));
    colors.skeleton = Some(hex(p.track));
    colors.title_bar = Some(hex(p.headerbar));
    colors.title_bar_border = Some(hex(p.border));
    config
}

/// Give the gpui-component controls the tokens of the design, in light and dark, and follow
/// the appearance of the system.
pub fn apply_theme(cx: &mut App) {
    let font = ui_font(cx);
    let mono = cx
        .text_system()
        .all_font_names()
        .iter()
        .any(|name| name == "Adwaita Mono")
        .then(|| SharedString::from("Adwaita Mono"));
    let light = Rc::new(theme_config(false, font.clone(), mono.clone()));
    let dark = Rc::new(theme_config(true, font, mono));
    // `sync_system_appearance` loads the theme of the mode from these two.
    Theme::sync_system_appearance(None, cx);
    Theme::update(cx, |theme| {
        theme.light_theme = light;
        theme.dark_theme = dark;
    });
    Theme::sync_system_appearance(None, cx);
}
