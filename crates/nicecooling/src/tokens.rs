//! The color tokens of `docs/ui/DESIGN.md`, for the parts that the shell draws itself.

use gpui::{Hsla, rgb};

pub struct Palette {
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
}

impl Palette {
    pub fn new(dark: bool) -> Self {
        let pick = |light: u32, dark_value: u32| -> Hsla {
            rgb(if dark { dark_value } else { light }).into()
        };
        Self {
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
        }
    }
}
