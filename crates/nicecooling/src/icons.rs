//! The icons of the comps, embedded in the binary, and the asset source of the app.
//! The files in `assets/nc/` come from the sprite of `docs/ui/comps/nc.js`.

use std::borrow::Cow;

use gpui::{AssetSource, Hsla, SharedString, Styled as _, px};
use gpui_component::{Icon, IconNamed, Sizable as _};

macro_rules! icons {
    ($($name:ident => $file:literal,)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum NcIcon {
            $($name,)*
        }

        impl NcIcon {
            pub const ALL: &[NcIcon] = &[$(NcIcon::$name,)*];

            /// The asset path and the SVG.
            fn file(self) -> (&'static str, &'static [u8]) {
                match self {
                    $(Self::$name => (
                        concat!("nc/", $file, ".svg"),
                        include_bytes!(concat!("../assets/nc/", $file, ".svg")),
                    ),)*
                }
            }
        }
    };
}

icons! {
    Alarm => "alarm",
    App => "app",
    Bell => "bell",
    Chart => "chart",
    Check => "check",
    Clock => "clock",
    Code => "code",
    Copy => "copy",
    Down => "down",
    Eye => "eye",
    Fan => "fan",
    Flask => "flask",
    Gauge => "gauge",
    Grip => "grip",
    Home => "home",
    Info => "info",
    Layers => "layers",
    Left => "left",
    Lock => "lock",
    Loop => "loop",
    Menu => "menu",
    Moon => "moon",
    Off => "off",
    Pencil => "pencil",
    Play => "play",
    Plus => "plus",
    Power => "power",
    Right => "right",
    Search => "search",
    Shield => "shield",
    ShieldOk => "shieldok",
    Sliders => "sliders",
    Trash => "trash",
    Undo => "undo",
    Warn => "warn",
    X => "x",
}

impl IconNamed for NcIcon {
    fn path(self) -> SharedString {
        self.file().0.into()
    }
}

/// An icon of 16 px in `color`.
pub fn icon(name: NcIcon, color: Hsla) -> Icon {
    Icon::new(name).with_size(px(16.)).text_color(color)
}

/// An icon of 12 px in `color`, for a caption or a tag.
pub fn small_icon(name: NcIcon, color: Hsla) -> Icon {
    Icon::new(name).with_size(px(12.)).text_color(color)
}

/// The icons of the app, then the icons that the gpui-component controls draw.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        let own = NcIcon::ALL
            .iter()
            .map(|icon| icon.file())
            .find(|(name, _)| *name == path);
        match own {
            Some((_, svg)) => Ok(Some(Cow::Borrowed(svg))),
            None => gpui_kit_assets::Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut paths = gpui_kit_assets::Assets.list(path)?;
        paths.extend(
            NcIcon::ALL
                .iter()
                .map(|icon| icon.file().0)
                .filter(|name| name.starts_with(path))
                .map(SharedString::from),
        );
        Ok(paths)
    }
}
