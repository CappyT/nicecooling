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
