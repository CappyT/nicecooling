//! The Display tab (`docs/ui/screens/leakshield-display.md`): the 10 pages of the OLED display
//! and its settings.

use crate::bitmap::{Bitmap, DEFAULT_THRESHOLD, Gray, ICON_SIZE, ImportError, PAGE_SIZE};
use crate::command::{Command, PAGE_CONTROL, PREVIEW_CONTROL};
use crate::i18n::Locale;
use crate::model::{DisplayAssets, FieldValue, LeakshieldData, Link, PagePreview};
use crate::screens::format::{optional_value, plain};
use crate::screens::leakshield::{Device, TabState, fields_command, write_line};
use crate::screens::{FeedbackLine, ScreenInput, WriteOutcome};

pub const PAGES: usize = 10;
/// The slots of one page in `page_sources`, `page_precisions`, `page_names` and `page_values`.
const SLOTS: usize = 8;
/// The slots of one page in `page_icons`.
const ICON_SLOTS: usize = 4;
/// "None" in `page_names` and `page_icons`.
pub const NO_ASSET: u8 = 255;
/// The names of report 8 that hold page texts: an own text goes into one of them.
pub const PAGE_TEXTS: (u8, u8) = (20, 34);

fn page_texts() -> std::ops::RangeInclusive<u8> {
    PAGE_TEXTS.0..=PAGE_TEXTS.1
}
/// The longest text of a name, in characters.
pub const TEXT_LIMIT: usize = 23;
/// The limits of the minimum and the maximum of a bar.
const RANGE_LIMIT: f64 = 32000.0;
const BRIGHTNESS: [&str; 4] = ["maximum", "mid", "low", "off"];

/// "No source" as a sensor index and as a byte of `page_sources`.
pub const NO_SOURCE: u8 = 255;
/// `display_page_time` of 30 s or more turns the automatic page change off.
const PAGE_TIME_OFF: f64 = 30.0;
/// The interval that "Automatic page change" writes when it has no earlier value.
const PAGE_TIME_ON: f64 = 29.0;
/// The first byte of `page_sources` that no sensor uses: the device has 20 sensors.
const SOURCE_LIMIT: u8 = 80;

/// The sensor index of a byte of `page_sources`. The byte is the sensor index times 4. A byte of
/// 80 or more has no sensor.
pub fn source_sensor(byte: f64) -> u8 {
    if (0.0..f64::from(SOURCE_LIMIT)).contains(&byte) {
        byte as u8 / 4
    } else {
        NO_SOURCE
    }
}

/// The byte of `page_sources` for a sensor index. `None` for an index that the device lacks.
pub fn source_byte(sensor: u8) -> Option<u8> {
    match sensor {
        NO_SOURCE => Some(NO_SOURCE),
        _ if sensor < SOURCE_LIMIT / 4 => Some(sensor * 4),
        _ => None,
    }
}

/// The name of an empty page in `page_template`.
pub const NO_TEMPLATE: &str = "none";

/// The templates of the device by registry name, with the number of values that each shows.
pub const TEMPLATES: [(&str, usize); 14] = [
    ("logo", 0),
    ("chart", 1),
    ("values-1", 1),
    ("values-2", 2),
    ("values-4", 4),
    ("values-3a", 3),
    ("values-3b", 3),
    ("time", 0),
    ("bars-2", 2),
    ("bars-4a", 4),
    ("bars-4b", 4),
    ("value-and-bar", 1),
    ("value-and-time", 1),
    ("leakshield-state", 0),
];

/// How many values a template shows.
pub fn template_values(name: &str) -> usize {
    TEMPLATES
        .iter()
        .find(|(template, _)| *template == name)
        .map_or(0, |(_, values)| *values)
}

/// The slots of a page that a template uses. Each count is a prefix: `texts: 2` is the texts 1
/// and 2. The table follows the page layouts of the device software.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Slots {
    /// Values: each has a data source and a number of decimals.
    pub values: usize,
    pub texts: usize,
    pub icons: usize,
    /// Values that have a minimum and a maximum: the bars.
    pub ranges: usize,
    /// The page shows the start image.
    pub logo: bool,
}

const fn slots(values: usize, texts: usize, icons: usize, ranges: usize) -> Slots {
    Slots {
        values,
        texts,
        icons,
        ranges,
        logo: false,
    }
}

/// The slots of each template, in the order of `TEMPLATES`.
pub const LAYOUTS: [(&str, Slots); 14] = [
    (
        "logo",
        Slots {
            values: 0,
            texts: 0,
            icons: 0,
            ranges: 0,
            logo: true,
        },
    ),
    ("chart", slots(1, 0, 0, 0)),
    ("values-1", slots(1, 1, 1, 0)),
    ("values-2", slots(2, 2, 2, 0)),
    ("values-4", slots(4, 4, 0, 0)),
    ("values-3a", slots(3, 1, 2, 0)),
    ("values-3b", slots(3, 1, 1, 0)),
    ("time", slots(0, 0, 0, 0)),
    ("bars-2", slots(2, 2, 2, 2)),
    ("bars-4a", slots(4, 0, 2, 4)),
    ("bars-4b", slots(4, 1, 0, 4)),
    ("value-and-bar", slots(1, 1, 0, 1)),
    ("value-and-time", slots(1, 1, 1, 0)),
    ("leakshield-state", slots(0, 0, 0, 0)),
];

/// The slots of a template. A name that the table lacks, and an empty page, use none.
pub fn template_slots(name: &str) -> Slots {
    LAYOUTS
        .iter()
        .find(|(template, _)| *template == name)
        .map_or_else(Slots::default, |(_, slots)| *slots)
}

pub fn template_name(locale: Locale, name: &str) -> String {
    if name == NO_TEMPLATE {
        locale.tr("leakshield.display.empty")
    } else if TEMPLATES.iter().any(|(template, _)| *template == name) {
        locale.tr(&format!("leakshield.display.template.{name}"))
    } else {
        // A name that the registry does not list, such as `unknown-14`.
        name.to_owned()
    }
}

/// What an import writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportTarget {
    Logo,
    /// The icon that this icon slot of the selected page shows.
    Icon {
        slot: usize,
    },
}

impl ImportTarget {
    pub fn size(self) -> (u16, u16) {
        match self {
            Self::Logo => PAGE_SIZE,
            Self::Icon { .. } => ICON_SIZE,
        }
    }
}

/// The import of a PNG file as the logo or as an icon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub target: ImportTarget,
    /// The name of the file that the user picked.
    pub file: Option<String>,
    pub picture: Option<Gray>,
    pub error: Option<ImportError>,
    /// The brightness from which a pixel is lit, in percent.
    pub threshold: u8,
    pub invert: bool,
}

impl Import {
    /// The one-bit image that the dialog shows and that the write sends.
    pub fn result(&self) -> Option<Bitmap> {
        self.picture
            .as_ref()
            .map(|picture| picture.to_bitmap(self.threshold, self.invert))
    }
}

/// The dialog of the Display tab, other than the template gallery.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    /// An own text for a text slot of the selected page.
    OwnText {
        slot: usize,
        text: String,
    },
    /// The 16 icons, for an icon slot of the selected page.
    Icons {
        slot: usize,
    },
    Import(Import),
}

#[derive(Clone, Debug, Default)]
pub struct DisplayUi {
    pub page: usize,
    /// The template gallery is open.
    pub gallery: bool,
    /// The interval before the user turned the automatic page change off, in seconds.
    pub last_interval: Option<f64>,
    pub dialog: Option<Dialog>,
}

impl DisplayUi {
    /// Arrow keys move in the grid of 5 columns.
    pub fn step(&mut self, dx: isize, dy: isize) {
        let moved = self.page as isize + dx + dy * 5;
        if (0..PAGES as isize).contains(&moved) {
            self.page = moved as usize;
        }
    }

    pub fn select(&mut self, page: usize) {
        self.page = page.min(PAGES - 1);
    }

    /// The control that shows the result of a write of the selected page.
    fn control(&self) -> String {
        format!("{PAGE_CONTROL}{}", self.page)
    }

    pub fn close_dialog(&mut self) {
        self.gallery = false;
        self.dialog = None;
    }

    /// The template of the selected page: one of the 14 templates, or `NO_TEMPLATE` for a page
    /// that the device skips.
    pub fn set_template(
        &mut self,
        input: &ScreenInput<'_>,
        id: &str,
        name: &str,
    ) -> Option<Command> {
        self.gallery = false;
        let device = Device::new(input, id);
        let mut list = device.text_list("page_template")?.to_vec();
        let slot = list.get_mut(self.page)?;
        if !device.access("page_template").writable
            || !(name == NO_TEMPLATE || TEMPLATES.iter().any(|(template, _)| *template == name))
        {
            return None;
        }
        *slot = name.to_owned();
        Some(fields_command(
            id,
            &self.control(),
            vec![("page_template".into(), FieldValue::TextList(list))],
        ))
    }

    /// One entry of a field that has `per_page` entries for each page.
    fn set_entry(
        &self,
        input: &ScreenInput<'_>,
        id: &str,
        field: &str,
        (per_page, slot): (usize, usize),
        value: f64,
    ) -> Option<Command> {
        let device = Device::new(input, id);
        let mut list = device.list(field)?.to_vec();
        if slot >= per_page || !device.access(field).writable {
            return None;
        }
        *list.get_mut(self.page * per_page + slot)? = value;
        Some(fields_command(
            id,
            &self.control(),
            vec![(field.to_owned(), FieldValue::List(list))],
        ))
    }

    /// The decimals of one value of the selected page, 0 to 3.
    pub fn set_decimals(
        &self,
        input: &ScreenInput<'_>,
        id: &str,
        slot: usize,
        decimals: u8,
    ) -> Option<Command> {
        if decimals > 3 {
            return None;
        }
        self.set_entry(
            input,
            id,
            "page_precisions",
            (SLOTS, slot),
            f64::from(decimals),
        )
    }

    /// The minimum (`maximum` false) or the maximum of the bar of one value. The device takes
    /// -32000 to 32000 in whole steps: the value is rounded and cut to that range.
    pub fn set_range(
        &self,
        input: &ScreenInput<'_>,
        id: &str,
        slot: usize,
        maximum: bool,
        value: f64,
    ) -> Option<Command> {
        if !value.is_finite() || slot >= SLOTS / 2 {
            return None;
        }
        let value = value.round().clamp(-RANGE_LIMIT, RANGE_LIMIT);
        let entry = slot * 2 + usize::from(maximum);
        self.set_entry(input, id, "page_values", (SLOTS, entry), value)
    }

    /// The text of a text slot: an index into the 40 names, or `NO_ASSET`.
    pub fn set_text(
        &self,
        input: &ScreenInput<'_>,
        id: &str,
        slot: usize,
        name: u8,
    ) -> Option<Command> {
        if name >= 40 && name != NO_ASSET {
            return None;
        }
        self.set_entry(input, id, "page_names", (SLOTS, slot), f64::from(name))
    }

    /// The icon of an icon slot: an index into the 16 icons, or `NO_ASSET`. It closes the icon
    /// dialog.
    pub fn set_icon(
        &mut self,
        input: &ScreenInput<'_>,
        id: &str,
        slot: usize,
        icon: u8,
    ) -> Option<Command> {
        self.dialog = None;
        if icon >= 16 && icon != NO_ASSET {
            return None;
        }
        self.set_entry(input, id, "page_icons", (ICON_SLOTS, slot), f64::from(icon))
    }

    /// Read the image of the selected page from the device. `None` while the device takes no
    /// command, and for a page without a template: the device does not show it.
    pub fn preview_command(&self, input: &ScreenInput<'_>, id: &str) -> Option<Command> {
        let device = Device::new(input, id);
        let template = device
            .text_list("page_template")
            .and_then(|list| list.get(self.page))?;
        if blocked(&device).is_some() || template == NO_TEMPLATE {
            return None;
        }
        Some(Command::ReadDisplayPreview {
            leakshield: id.to_owned(),
            page: self.page as u8,
        })
    }

    pub fn open_own_text(&mut self, slot: usize) {
        self.gallery = false;
        self.dialog = Some(Dialog::OwnText {
            slot,
            text: String::new(),
        });
    }

    pub fn open_icons(&mut self, slot: usize) {
        self.gallery = false;
        self.dialog = Some(Dialog::Icons { slot });
    }

    pub fn open_import(&mut self, target: ImportTarget) {
        self.gallery = false;
        self.dialog = Some(Dialog::Import(Import {
            target,
            file: None,
            picture: None,
            error: None,
            threshold: DEFAULT_THRESHOLD,
            invert: false,
        }));
    }

    /// The file that the user picked for the open import, decoded or not.
    pub fn import_loaded(&mut self, file: String, result: Result<Gray, ImportError>) {
        if let Some(Dialog::Import(import)) = &mut self.dialog {
            import.file = Some(file);
            match result {
                Ok(picture) => {
                    import.picture = Some(picture);
                    import.error = None;
                }
                Err(error) => {
                    import.picture = None;
                    import.error = Some(error);
                }
            }
        }
    }

    pub fn set_threshold(&mut self, percent: f64) {
        if let Some(Dialog::Import(import)) = &mut self.dialog {
            import.threshold = percent.round().clamp(0.0, 100.0) as u8;
        }
    }

    pub fn set_invert(&mut self, invert: bool) {
        if let Some(Dialog::Import(import)) = &mut self.dialog {
            import.invert = invert;
        }
    }

    pub fn type_own_text(&mut self, typed: &str) {
        if let Some(Dialog::OwnText { text, .. }) = &mut self.dialog {
            *text = typed.to_owned();
        }
    }

    /// "Write": the logo or the icon of the open import. It closes the dialog.
    pub fn write_import(&mut self, input: &ScreenInput<'_>, id: &str) -> Option<Command> {
        let Some(Dialog::Import(import)) = &self.dialog else {
            return None;
        };
        let device = Device::new(input, id);
        let bitmap = import.result()?;
        let (leakshield, page) = (id.to_owned(), self.page as u8);
        let command = match import.target {
            _ if blocked(&device).is_some() => return None,
            ImportTarget::Logo => Command::SetDisplayLogo {
                leakshield,
                page,
                logo: bitmap,
            },
            ImportTarget::Icon { slot } => Command::SetDisplayIcon {
                leakshield,
                page,
                index: icon_of(&device, self.page, slot)?,
                icon: bitmap,
            },
        };
        self.dialog = None;
        Some(command)
    }

    /// "Save" of the own text: the text goes into a page text of report 8, and the slot of the
    /// page then selects that text. It closes the dialog.
    pub fn save_own_text(&mut self, input: &ScreenInput<'_>, id: &str) -> Option<Command> {
        let Some(Dialog::OwnText { slot, text }) = &self.dialog else {
            return None;
        };
        let device = Device::new(input, id);
        let data = device.data?;
        if own_text_error(text).is_some()
            || blocked(&device).is_some()
            || !device.access("page_names").writable
        {
            return None;
        }
        let index = text_target(data, self.page, *slot)?;
        let mut names = device.list("page_names")?.to_vec();
        let position = self.page * SLOTS + *slot;
        let select = (*names.get(position)? != f64::from(index)).then(|| {
            names[position] = f64::from(index);
            names
        });
        let command = Command::SetDisplayName {
            leakshield: id.to_owned(),
            page: self.page as u8,
            index,
            text: text.clone(),
            select,
        };
        self.dialog = None;
        Some(command)
    }

    /// The source of one value of the selected page: a sensor index, or `NO_SOURCE`. The field
    /// holds the index times 4.
    pub fn set_source(
        &self,
        input: &ScreenInput<'_>,
        id: &str,
        slot: usize,
        source: u8,
    ) -> Option<Command> {
        let device = Device::new(input, id);
        let mut list = device.list("page_sources")?.to_vec();
        if slot >= SLOTS || !device.access("page_sources").writable {
            return None;
        }
        *list.get_mut(self.page * SLOTS + slot)? = f64::from(source_byte(source)?);
        Some(fields_command(
            id,
            &self.control(),
            vec![("page_sources".into(), FieldValue::List(list))],
        ))
    }

    /// A bit of `page_options` of the selected page: 1 shows in standby, 2 hides in normal use.
    pub fn set_option(
        &self,
        input: &ScreenInput<'_>,
        id: &str,
        bit: u32,
        on: bool,
    ) -> Option<Command> {
        let device = Device::new(input, id);
        let mut list = device.list("page_options")?.to_vec();
        if !device.access("page_options").writable {
            return None;
        }
        let entry = list.get_mut(self.page)?;
        let mask = *entry as u32;
        *entry = f64::from(if on { mask | bit } else { mask & !bit });
        Some(fields_command(
            id,
            &self.control(),
            vec![("page_options".into(), FieldValue::List(list))],
        ))
    }
}

/// Why the device takes no command and no write now: the daemon is away, the Leakshield does
/// not answer, or it is read-only. `None` when it takes them.
fn blocked(device: &Device<'_>) -> Option<String> {
    let locale = device.locale;
    if device.state() != TabState::Ready {
        return Some(locale.tr(if device.link == Link::Online {
            "leakshield.offline"
        } else {
            "leakshield.display.daemon_offline"
        }));
    }
    device
        .info
        .filter(|info| !info.read_only_reason.is_empty())
        .map(|info| device.reason_text(&info.read_only_reason))
}

/// Why a field of the pages takes no write. `None` when it takes one.
fn field_blocked(device: &Device<'_>, field: &str) -> Option<String> {
    blocked(device).or_else(|| {
        let access = device.access(field);
        (!access.writable).then(|| device.reason_text(&access.reason))
    })
}

/// The index of the icon that an icon slot of a page shows. `None` for "no icon".
fn icon_of(device: &Device<'_>, page: usize, slot: usize) -> Option<u8> {
    let value = *device.list("page_icons")?.get(page * ICON_SLOTS + slot)?;
    (0.0..16.0).contains(&value).then_some(value as u8)
}

/// Why a text is not a text of the device: it is too long, or it has a control character.
/// `None` for a text that the app sends.
pub fn own_text_error(text: &str) -> Option<&'static str> {
    if text.chars().count() > TEXT_LIMIT {
        Some("leakshield.display.own_text.too_long")
    } else if text.chars().any(char::is_control) {
        Some("leakshield.display.own_text.control")
    } else {
        None
    }
}

/// The name of report 8 that takes an own text for a text slot of a page. The rule:
/// 1. the page text that the slot shows now, when no other slot refers to it;
/// 2. else the first page text (names 20 to 34) that is empty and that no other slot refers to;
/// 3. else the first page text that no other slot refers to.
///
/// A slot refers to a name when `page_names` holds it, on any of the 10 pages and whatever the
/// template of the page: a page that is off now shows the text again when the user turns it on.
/// `None` when all 15 page texts are in use, or when the app has no names of the device.
pub fn text_target(data: &LeakshieldData, page: usize, slot: usize) -> Option<u8> {
    let assets = data.display_assets.as_ref()?;
    let names = data.list("page_names")?;
    let used_elsewhere = |name: u8| {
        names
            .iter()
            .enumerate()
            .any(|(entry, value)| entry != page * SLOTS + slot && *value == f64::from(name))
    };
    let current = names.get(page * SLOTS + slot).copied().unwrap_or(255.0);
    let free = |name: &u8| !used_elsewhere(*name);
    page_texts()
        .filter(|name| f64::from(*name) == current)
        .find(free)
        .or_else(|| {
            page_texts()
                .filter(|name| {
                    assets
                        .names
                        .get(usize::from(*name))
                        .is_none_or(|text| text.is_empty())
                })
                .find(free)
        })
        .or_else(|| page_texts().find(free))
}

/// A switch of the display settings and the bit of `display_flags` behind it. With `inverted` the
/// bit is clear while the switch is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FlagSwitch {
    pub key: &'static str,
    pub bit: u32,
    pub inverted: bool,
}

/// Bit 0: the content turns by 180 degrees.
pub const ROTATE: FlagSwitch = FlagSwitch {
    key: "rotate",
    bit: 1,
    inverted: false,
};
/// Bit 1 clear: the display goes off after 10 minutes in standby.
pub const OFF_IN_STANDBY: FlagSwitch = FlagSwitch {
    key: "standby",
    bit: 2,
    inverted: true,
};
/// Bit 2: the display shows inverted.
pub const INVERT: FlagSwitch = FlagSwitch {
    key: "invert",
    bit: 4,
    inverted: false,
};
/// Bit 3 clear: the device inverts the display by itself.
pub const AUTO_INVERT: FlagSwitch = FlagSwitch {
    key: "auto_invert",
    bit: 8,
    inverted: true,
};

impl FlagSwitch {
    /// The state of the switch for the value of `display_flags`.
    pub fn is_on(self, flags: u32) -> bool {
        (flags & self.bit != 0) != self.inverted
    }

    /// The write for a new state of the switch.
    pub fn set(self, input: &ScreenInput<'_>, id: &str, on: bool) -> Option<Command> {
        set_flag(input, id, self.bit, on != self.inverted)
    }
}

/// A bit of `display_flags`.
pub fn set_flag(input: &ScreenInput<'_>, id: &str, bit: u32, on: bool) -> Option<Command> {
    let device = Device::new(input, id);
    if !device.access("display_flags").writable {
        return None;
    }
    let mask = device.number("display_flags")? as u32;
    let mask = if on { mask | bit } else { mask & !bit };
    Some(fields_command(
        id,
        &format!("display:flag:{bit}"),
        vec![("display_flags".into(), FieldValue::Number(f64::from(mask)))],
    ))
}

pub fn set_brightness(
    input: &ScreenInput<'_>,
    id: &str,
    field: &str,
    level: &str,
) -> Option<Command> {
    let device = Device::new(input, id);
    if !device.access(field).writable || !BRIGHTNESS.contains(&level) {
        return None;
    }
    Some(fields_command(
        id,
        &format!("display:{field}"),
        vec![(field.to_owned(), FieldValue::Text(level.to_owned()))],
    ))
}

/// The interval of the automatic page change. The device takes 1 to 29 s: a longer time is the
/// same as the switch off, so the value is cut to the range.
pub fn set_interval(input: &ScreenInput<'_>, id: &str, seconds: f64) -> Option<Command> {
    let device = Device::new(input, id);
    if !device.access("display_page_time").writable || !seconds.is_finite() {
        return None;
    }
    Some(page_time_command(id, seconds.clamp(1.0, PAGE_TIME_ON)))
}

fn page_time_command(id: &str, seconds: f64) -> Command {
    fields_command(
        id,
        "display:interval",
        vec![("display_page_time".into(), FieldValue::Number(seconds))],
    )
}

impl DisplayUi {
    /// The switch "Automatic page change". Off writes 30 s. On writes the interval from before,
    /// or 29 s.
    pub fn set_auto_page(
        &mut self,
        input: &ScreenInput<'_>,
        id: &str,
        on: bool,
    ) -> Option<Command> {
        let device = Device::new(input, id);
        if !device.access("display_page_time").writable {
            return None;
        }
        if on {
            let seconds = self.last_interval.unwrap_or(PAGE_TIME_ON);
            return Some(page_time_command(id, seconds.clamp(1.0, PAGE_TIME_ON)));
        }
        if let Some(current) = device
            .number("display_page_time")
            .filter(|v| (1.0..PAGE_TIME_OFF).contains(v))
        {
            self.last_interval = Some(current);
        }
        Some(page_time_command(id, PAGE_TIME_OFF))
    }
}

fn brightness_of(device: &Device<'_>, field: &str) -> Option<&'static str> {
    match &device.field(field)?.value {
        FieldValue::Text(text) => BRIGHTNESS.iter().find(|b| **b == text).copied(),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ValueRow {
    pub slot: usize,
    pub label: String,
    /// The source sensors: index, text, selected.
    pub sources: Vec<(u8, String, bool)>,
    pub source_text: String,
    pub value: String,
    /// "Software sensor 3 has no data source." with the link to the Sensors tab.
    pub warning: Option<String>,
    /// The decimals 0 to 3: number, text, selected.
    pub decimals: Vec<(u8, String, bool)>,
    pub decimals_text: String,
    /// The minimum and the maximum of the bar, for a value that has a bar.
    pub range: Option<(String, String)>,
}

/// A button, and why it is disabled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub label: String,
    pub enabled: bool,
    pub reason: Option<String>,
}

impl Action {
    fn new(label: String, reason: Option<String>) -> Self {
        Self {
            label,
            enabled: reason.is_none(),
            reason,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TextRow {
    pub slot: usize,
    pub label: String,
    /// The names: index, text, selected. The last one is "No text".
    pub choices: Vec<(u8, String, bool)>,
    pub shown: String,
    pub own: Action,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IconRow {
    pub slot: usize,
    pub label: String,
    /// The image of the icon, when the slot has one and the app has the icons of the device.
    pub bitmap: Option<Bitmap>,
    /// "Icon 8", or "No icon".
    pub name: String,
    pub change: Action,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LogoRow {
    pub label: String,
    pub text: String,
    pub bitmap: Option<Bitmap>,
    pub change: Action,
}

/// What a preview shows: the image of the device, or the reason why it has none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    Image(Bitmap),
    /// A short text that fits the preview: "Not read yet", "Not shown".
    Note(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageCard {
    pub index: usize,
    pub number: String,
    /// The registry name of the template: `values-2`, or `none` for an empty page.
    pub template: String,
    pub template_name: String,
    pub selected: bool,
    pub empty: bool,
    /// The page is hidden in normal operation and in standby: the preview is dim.
    pub off: Option<String>,
    pub screen: Screen,
    /// A read of the image runs.
    pub loading: bool,
}

/// The large preview of the selected page.
#[derive(Clone, Debug, PartialEq)]
pub struct PreviewPanel {
    pub screen: Screen,
    pub loading: bool,
    /// "The Leakshield shows this page for 30 s after each read."
    pub caption: String,
    /// The full reason of a preview without an image, or the state of the read.
    pub note: Option<String>,
    pub refresh: Action,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Switch {
    pub label: String,
    pub on: bool,
    pub writable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SelectedPage {
    pub title: String,
    pub template_label: String,
    pub template_name: String,
    /// "Change template", or "Choose a template" for an empty page.
    pub change: Action,
    pub preview: PreviewPanel,
    /// The titles of the value table: value, data source, decimals, minimum, maximum.
    pub value_columns: Vec<String>,
    pub rows: Vec<ValueRow>,
    pub sources_writable: bool,
    pub decimals_writable: bool,
    pub ranges_writable: bool,
    pub texts: Vec<TextRow>,
    pub texts_writable: bool,
    pub icons: Vec<IconRow>,
    pub logo: Option<LogoRow>,
    /// "This template has no settings.", for a template without a slot.
    pub no_settings: Option<String>,
    /// Bit 1 of `page_options`.
    pub hide_normal: Switch,
    /// Bit 0 of `page_options`.
    pub standby: Switch,
    pub link: String,
    pub write_line: Option<FeedbackLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GalleryItem {
    /// The registry name, or `none`.
    pub template: String,
    pub label: String,
    pub selected: bool,
}

/// The dialog that is open over the tab.
#[derive(Clone, Debug, PartialEq)]
pub enum DialogModel {
    Gallery {
        title: String,
        items: Vec<GalleryItem>,
        cancel: String,
    },
    OwnText {
        title: String,
        text: String,
        value: String,
        /// "The text goes into page text 4."
        note: Option<String>,
        save: Action,
        cancel: String,
    },
    Icons {
        title: String,
        text: String,
        /// The icons: index, image, selected.
        icons: Vec<(u8, Bitmap, bool)>,
        none: Action,
        import: Action,
        cancel: String,
    },
    Import {
        title: String,
        text: String,
        choose: String,
        file: Option<String>,
        /// The result of the conversion, and the scale that the dialog draws it at.
        result: Option<(Bitmap, u32)>,
        /// "The file is 640 x 480 pixels. nicecooling scales it to 128 x 64."
        note: Option<String>,
        error: Option<String>,
        threshold_label: String,
        threshold: u8,
        invert: Switch,
        write: Action,
        cancel: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub field: String,
    pub label: String,
    pub shown: String,
    pub options: Vec<(String, String, bool)>,
    pub writable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingsCard {
    pub title: String,
    pub brightness: Vec<Choice>,
    pub auto_page: Switch,
    pub interval_label: String,
    pub interval_text: String,
    pub interval: String,
    /// The interval is writable while the automatic page change is on.
    pub interval_writable: bool,
    pub rotate: Switch,
    pub invert: Switch,
    /// Why "Invert" is disabled: the automatic invert is on.
    pub invert_text: Option<String>,
    pub auto_invert: Switch,
    pub auto_invert_text: String,
    pub standby: Switch,
    pub standby_text: String,
    pub write_line: Option<FeedbackLine>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DisplayModel {
    pub state: TabState,
    pub pages_title: String,
    pub pages: Vec<PageCard>,
    pub page: SelectedPage,
    pub settings: SettingsCard,
    pub dialog: Option<DialogModel>,
    pub hint: String,
    /// The reason of a read-only tab.
    pub reason: Option<String>,
}

/// The device sensors, in order: index and the identifier suffix.
const DEVICE_SENSORS: [(u8, &str, &str); 6] = [
    (0, "pressure", "mbar"),
    (1, "fill_level", "%"),
    (2, "volume_change", "ml/h"),
    (3, "pressure_change", "mbar/min"),
    (4, "feed_rpm", "rpm"),
    (5, "flow", "l/h"),
];

struct Sources<'a> {
    locale: Locale,
    input: &'a ScreenInput<'a>,
    id: &'a str,
    data: Option<&'a LeakshieldData>,
}

impl Sources<'_> {
    /// The name and the value of a source index.
    fn describe(&self, index: u8) -> (String, Option<f64>, bool) {
        let state = self.input.state;
        let sample = state.sensors.as_ref().or(state.last_sensors.as_ref());
        if let Some((_, name, _)) = DEVICE_SENSORS.iter().find(|(i, _, _)| *i == index) {
            let sensor = format!("{}_{name}", self.id);
            let text = state.snapshot.sensors.get(&sensor).map_or_else(
                || self.locale.tr(&format!("leakshield.display.sensor.{name}")),
                |s| s.name.clone(),
            );
            let value = sample.and_then(|s| s.sensor_value(&sensor));
            return (text, value, true);
        }
        if (8..=19).contains(&index) {
            let slot = u16::from(index) - 7;
            let source = self
                .data
                .and_then(|d| d.software_sensors.iter().find(|(n, _)| *n == slot))
                .and_then(|(_, sensor)| sensor.clone());
            let label = self.locale.fmt(
                "leakshield.sensors.software_name",
                &[("n", &slot.to_string())],
            );
            return match source {
                Some(sensor) => {
                    let name = state
                        .snapshot
                        .sensors
                        .get(&sensor)
                        .map_or(sensor.clone(), |s| s.name.clone());
                    (
                        format!("{label}: {name}"),
                        sample.and_then(|s| s.sensor_value(&sensor)),
                        true,
                    )
                }
                None => (label, None, false),
            };
        }
        (self.locale.tr("leakshield.display.no_source"), None, false)
    }

    fn unit(&self, index: u8) -> String {
        if let Some((_, _, unit)) = DEVICE_SENSORS.iter().find(|(i, _, _)| *i == index) {
            return (*unit).to_owned();
        }
        let slot = u16::from(index).wrapping_sub(7);
        self.data
            .and_then(|d| d.software_sensors.iter().find(|(n, _)| *n == slot))
            .and_then(|(_, sensor)| sensor.as_ref())
            .and_then(|sensor| self.input.state.snapshot.sensors.get(sensor))
            .map_or_else(String::new, |sensor| sensor.unit.clone())
    }
}

/// The key of the short text and of the full reason of a preview without an image.
fn failure_key(error: &str) -> &'static str {
    match error {
        "PageNotShown" => "not_shown",
        "ReadOnly" | "NotVerified" => "read_only",
        "Disconnected" => "offline",
        "UnknownMethod" => "no_method",
        "NotSupported" => "not_supported",
        _ => "failed",
    }
}

fn pending(input: &ScreenInput<'_>, id: &str, control: &str) -> bool {
    matches!(
        input.feedback.get(&format!("ls:{id}:{control}")),
        Some(WriteOutcome::Pending)
    )
}

/// What the preview of a page shows, whether a read runs, and the full reason when it shows no
/// image.
fn page_screen(
    input: &ScreenInput<'_>,
    id: &str,
    page: usize,
    template: &str,
) -> (Screen, bool, Option<String>) {
    let locale = input.locale;
    let short = |key: &str| locale.tr(&format!("leakshield.display.preview.short.{key}"));
    let long = |key: &str| locale.tr(&format!("leakshield.display.preview.long.{key}"));
    let loading = pending(input, id, &format!("{PREVIEW_CONTROL}{page}"))
        || pending(input, id, &format!("{PAGE_CONTROL}{page}"));
    if template == NO_TEMPLATE {
        return (Screen::Note(short("empty")), false, Some(long("empty")));
    }
    let cached = input
        .state
        .display_previews
        .get(&(id.to_owned(), page as u8));
    match cached {
        Some(PagePreview::Image(bitmap)) => (
            Screen::Image(bitmap.clone()),
            loading,
            loading.then(|| long("reading")),
        ),
        _ if loading => (Screen::Note(short("reading")), true, Some(long("reading"))),
        Some(PagePreview::Failed(error)) => {
            let key = failure_key(error);
            let reason = if key == "failed" {
                locale.fmt(
                    "leakshield.display.preview.long.failed",
                    &[("error", error)],
                )
            } else {
                long(key)
            };
            (Screen::Note(short(key)), false, Some(reason))
        }
        // A device that takes no command gets no read: the reason is that state.
        None => match blocked_key(&Device::new(input, id)) {
            Some("daemon") => (
                Screen::Note(short("daemon")),
                false,
                Some(locale.tr("leakshield.display.daemon_offline")),
            ),
            Some(key) => (Screen::Note(short(key)), false, Some(long(key))),
            None => (
                Screen::Note(short("not_read")),
                false,
                Some(long("not_read")),
            ),
        },
    }
}

/// The state that keeps the app from a preview read: `daemon`, `offline` or `read_only`.
fn blocked_key(device: &Device<'_>) -> Option<&'static str> {
    if device.state() != TabState::Ready {
        Some(if device.link == Link::Online {
            "offline"
        } else {
            "daemon"
        })
    } else if device
        .info
        .is_some_and(|info| !info.read_only_reason.is_empty())
    {
        Some("read_only")
    } else {
        None
    }
}

/// The name of report 8 at `index`, or "Name 21" when the app has no names or the name is empty.
fn name_text(locale: Locale, assets: Option<&DisplayAssets>, index: u8) -> String {
    assets
        .and_then(|assets| assets.names.get(usize::from(index)))
        .filter(|name| !name.is_empty())
        .cloned()
        .unwrap_or_else(|| {
            locale.fmt(
                "leakshield.display.name_n",
                &[("n", &(u16::from(index) + 1).to_string())],
            )
        })
}

impl DisplayModel {
    pub fn build(input: &ScreenInput<'_>, ui: &DisplayUi, id: &str) -> Self {
        let locale = input.locale;
        let device = Device::new(input, id);
        let state = device.state();
        let sources = Sources {
            locale,
            input,
            id,
            data: device.data,
        };
        let assets = device.data.and_then(|data| data.display_assets.as_ref());
        let templates = device.text_list("page_template").unwrap_or(&[]);
        let options = device.list("page_options").unwrap_or(&[]);
        let source_list = device.list("page_sources").unwrap_or(&[]);
        let template_of = |page: usize| {
            templates
                .get(page)
                .map_or(NO_TEMPLATE, String::as_str)
                .to_owned()
        };
        let mask_of = |page: usize| options.get(page).map_or(1, |m| *m as u32);

        let pages = (0..PAGES)
            .map(|page| {
                let template = template_of(page);
                let mask = mask_of(page);
                let (screen, loading, _) = page_screen(input, id, page, &template);
                PageCard {
                    index: page,
                    number: (page + 1).to_string(),
                    template_name: template_name(locale, &template),
                    empty: template == NO_TEMPLATE,
                    template,
                    selected: page == ui.page,
                    off: (mask & 1 == 0 && mask & 2 != 0)
                        .then(|| locale.tr("leakshield.display.not_shown")),
                    screen,
                    loading,
                }
            })
            .collect();

        let page = ui.page.min(PAGES - 1);
        let template = template_of(page);
        let layout = template_slots(&template);
        let mask = mask_of(page);
        let numbered = |key: &str, slot: usize| locale.fmt(key, &[("n", &(slot + 1).to_string())]);
        let entry = |field: &str, per_page: usize, slot: usize| {
            device
                .list(field)
                .and_then(|list| list.get(page * per_page + slot))
                .copied()
        };
        let rows = (0..layout.values)
            .map(|slot| {
                let index = source_list
                    .get(page * SLOTS + slot)
                    .map_or(NO_SOURCE, |v| source_sensor(*v));
                let (text, value, has_source) = sources.describe(index);
                let mut choices: Vec<(u8, String, bool)> = DEVICE_SENSORS
                    .iter()
                    .map(|(i, _, _)| *i)
                    .chain(8..=19)
                    .map(|i| (i, sources.describe(i).0, i == index))
                    .collect();
                choices.push((
                    NO_SOURCE,
                    locale.tr("leakshield.display.no_source"),
                    index == NO_SOURCE,
                ));
                let decimals = entry("page_precisions", SLOTS, slot);
                ValueRow {
                    slot,
                    label: numbered("leakshield.display.value_n", slot),
                    sources: choices,
                    source_text: text.clone(),
                    value: optional_value(locale, value, &sources.unit(index)),
                    warning: (!has_source && (8..=19).contains(&index)).then(|| {
                        locale.fmt("leakshield.display.no_data_source", &[("name", &text)])
                    }),
                    decimals: (0..=3u8)
                        .map(|n| (n, n.to_string(), decimals == Some(f64::from(n))))
                        .collect(),
                    decimals_text: decimals.map_or_else(|| "--".to_owned(), |n| plain(locale, n)),
                    range: (slot < layout.ranges).then(|| {
                        let text = |entry: Option<f64>| {
                            entry.map_or_else(String::new, |value| format!("{value:.0}"))
                        };
                        (
                            text(entry("page_values", SLOTS, slot * 2)),
                            text(entry("page_values", SLOTS, slot * 2 + 1)),
                        )
                    }),
                }
            })
            .collect();

        let names_blocked = field_blocked(&device, "page_names");
        let no_names = assets
            .is_none()
            .then(|| locale.tr("leakshield.display.no_assets"));
        let texts = (0..layout.texts)
            .map(|slot| {
                let current = entry("page_names", SLOTS, slot).map_or(NO_ASSET, |v| v as u8);
                let mut choices: Vec<(u8, String, bool)> = (0..40u8)
                    .filter(|index| {
                        // An empty name shows only while the slot uses it.
                        *index == current
                            || assets.is_none_or(|assets| {
                                assets
                                    .names
                                    .get(usize::from(*index))
                                    .is_some_and(|name| !name.is_empty())
                            })
                    })
                    .map(|index| (index, name_text(locale, assets, index), index == current))
                    .collect();
                choices.push((
                    NO_ASSET,
                    locale.tr("leakshield.display.no_text"),
                    !(0..40).contains(&current),
                ));
                let full = device
                    .data
                    .filter(|_| assets.is_some())
                    .is_some_and(|data| text_target(data, page, slot).is_none())
                    .then(|| locale.tr("leakshield.display.own_text.full"));
                TextRow {
                    slot,
                    label: numbered("leakshield.display.text_n", slot),
                    shown: choices
                        .iter()
                        .find(|(_, _, selected)| *selected)
                        .map_or_else(String::new, |(_, text, _)| text.clone()),
                    choices,
                    own: Action::new(
                        locale.tr("leakshield.display.own_text.button"),
                        names_blocked.clone().or(no_names.clone()).or(full),
                    ),
                }
            })
            .collect();

        let icons_blocked = field_blocked(&device, "page_icons");
        let icons = (0..layout.icons)
            .map(|slot| {
                let index = icon_of(&device, page, slot);
                IconRow {
                    slot,
                    label: numbered("leakshield.display.icon_n", slot),
                    bitmap: index.and_then(|index| {
                        assets.and_then(|assets| assets.icons.get(usize::from(index)).cloned())
                    }),
                    name: index.map_or_else(
                        || locale.tr("leakshield.display.no_icon"),
                        |index| numbered("leakshield.display.icon_n", usize::from(index)),
                    ),
                    change: Action::new(
                        locale.tr("leakshield.display.change_icon"),
                        icons_blocked.clone(),
                    ),
                }
            })
            .collect();

        let logo = layout.logo.then(|| LogoRow {
            label: locale.tr("leakshield.display.logo"),
            text: locale.tr("leakshield.display.logo_text"),
            bitmap: assets.map(|assets| assets.logo.clone()),
            change: Action::new(
                locale.tr("leakshield.display.change_logo"),
                blocked(&device),
            ),
        });

        let (screen, loading, note) = page_screen(input, id, page, &template);
        let preview = PreviewPanel {
            screen,
            loading,
            caption: locale.tr("leakshield.display.preview.caption"),
            note,
            refresh: Action {
                enabled: blocked(&device).is_none() && template != NO_TEMPLATE && !loading,
                ..Action::new(
                    locale.tr("leakshield.display.preview.refresh"),
                    blocked(&device).or_else(|| {
                        (template == NO_TEMPLATE)
                            .then(|| locale.tr("leakshield.display.preview.long.empty"))
                    }),
                )
            },
        };
        let option_switch = |label: &str, on: bool| Switch {
            label: locale.tr(label),
            on,
            writable: device.access("page_options").writable,
        };
        let writable = |field: &str| device.access(field).writable;
        let no_slot = layout == Slots::default() && template != NO_TEMPLATE;
        let selected = SelectedPage {
            title: numbered("leakshield.display.page_n", page),
            template_label: locale.tr("leakshield.display.template_label"),
            template_name: template_name(locale, &template),
            change: Action::new(
                locale.tr(if template == NO_TEMPLATE {
                    "leakshield.display.choose"
                } else {
                    "leakshield.display.change"
                }),
                field_blocked(&device, "page_template"),
            ),
            preview,
            value_columns: ["value", "source", "decimals", "minimum", "maximum"]
                .iter()
                .take(if layout.ranges > 0 { 5 } else { 3 })
                .map(|name| locale.tr(&format!("leakshield.display.column.{name}")))
                .collect(),
            rows,
            sources_writable: writable("page_sources"),
            decimals_writable: writable("page_precisions"),
            ranges_writable: writable("page_values"),
            texts,
            texts_writable: names_blocked.is_none(),
            icons,
            logo,
            no_settings: no_slot.then(|| locale.tr("leakshield.display.no_settings")),
            hide_normal: option_switch("leakshield.display.hide_normal", mask & 2 != 0),
            standby: option_switch("leakshield.display.show_standby", mask & 1 != 0),
            link: locale.tr("leakshield.display.open_sensors"),
            write_line: write_line(input, id, &format!("{PAGE_CONTROL}{page}")),
        };

        let flags = device.number("display_flags").unwrap_or(0.0) as u32;
        let flag_access = device.access("display_flags");
        let flag = |label: &str, on: bool| Switch {
            label: locale.tr(label),
            on,
            writable: flag_access.writable,
        };
        let choice = |field: &str, label: &str| {
            let current = brightness_of(&device, field);
            let options: Vec<(String, String, bool)> = BRIGHTNESS
                .iter()
                .map(|level| {
                    (
                        (*level).to_owned(),
                        locale.tr(&format!("leakshield.display.brightness.{level}")),
                        Some(*level) == current,
                    )
                })
                .collect();
            Choice {
                field: field.to_owned(),
                label: locale.tr(label),
                shown: options
                    .iter()
                    .find(|(_, _, s)| *s)
                    .map_or_else(|| "--".to_owned(), |(_, t, _)| t.clone()),
                options,
                writable: device.access(field).writable,
            }
        };
        let interval = device.number("display_page_time");
        let auto_page = interval.is_some_and(|v| v < PAGE_TIME_OFF);
        let settings = SettingsCard {
            title: locale.tr("leakshield.display.settings"),
            brightness: vec![
                choice(
                    "display_brightness_active",
                    "leakshield.display.brightness_active",
                ),
                choice(
                    "display_brightness_idle",
                    "leakshield.display.brightness_idle",
                ),
            ],
            auto_page: Switch {
                label: locale.tr("leakshield.display.auto_page"),
                on: auto_page,
                writable: device.access("display_page_time").writable,
            },
            interval_label: locale.tr("leakshield.display.interval"),
            interval_text: locale.tr("leakshield.display.interval_text"),
            interval: interval.map_or_else(String::new, |v| plain(locale, v)),
            interval_writable: auto_page && device.access("display_page_time").writable,
            rotate: flag("leakshield.display.rotate", ROTATE.is_on(flags)),
            invert: Switch {
                writable: flag_access.writable && !AUTO_INVERT.is_on(flags),
                ..flag("leakshield.display.invert", INVERT.is_on(flags))
            },
            invert_text: AUTO_INVERT
                .is_on(flags)
                .then(|| locale.tr("leakshield.display.invert_text")),
            auto_invert: flag("leakshield.display.auto_invert", AUTO_INVERT.is_on(flags)),
            auto_invert_text: locale.tr("leakshield.display.auto_invert_text"),
            standby: flag(
                "leakshield.display.off_standby",
                OFF_IN_STANDBY.is_on(flags),
            ),
            standby_text: locale.tr("leakshield.display.off_standby_text"),
            write_line: [
                "display_brightness_active",
                "display_brightness_idle",
                "interval",
            ]
            .iter()
            .find_map(|c| write_line(input, id, &format!("display:{c}")))
            .or_else(|| write_line(input, id, "display:interval"))
            .or_else(|| {
                [1u32, 2, 4, 8]
                    .iter()
                    .find_map(|b| write_line(input, id, &format!("display:flag:{b}")))
            }),
        };
        let reason = device
            .info
            .filter(|info| !info.read_only_reason.is_empty())
            .map(|info| device.reason_text(&info.read_only_reason));
        let cancel = locale.tr("leakshield.cancel");
        let dialog = if ui.gallery {
            Some(DialogModel::Gallery {
                title: locale.tr("leakshield.display.gallery"),
                items: TEMPLATES
                    .iter()
                    .map(|(name, _)| *name)
                    .chain([NO_TEMPLATE])
                    .map(|name| GalleryItem {
                        template: name.to_owned(),
                        label: if name == NO_TEMPLATE {
                            locale.tr("leakshield.display.no_page")
                        } else {
                            template_name(locale, name)
                        },
                        selected: name == template,
                    })
                    .collect(),
                cancel,
            })
        } else {
            ui.dialog.as_ref().map(|dialog| match dialog {
                Dialog::OwnText { slot, text } => {
                    let target = device.data.and_then(|data| text_target(data, page, *slot));
                    let error = own_text_error(text).map(|key| locale.tr(key));
                    DialogModel::OwnText {
                        title: locale.tr("leakshield.display.own_text.title"),
                        text: locale.fmt(
                            "leakshield.display.own_text.text",
                            &[
                                ("slot", &(slot + 1).to_string()),
                                ("page", &(page + 1).to_string()),
                            ],
                        ),
                        value: text.clone(),
                        note: target.map(|index| {
                            locale.fmt(
                                "leakshield.display.own_text.target",
                                &[("n", &(index - PAGE_TEXTS.0 + 1).to_string())],
                            )
                        }),
                        save: Action::new(
                            locale.tr("leakshield.display.own_text.save"),
                            field_blocked(&device, "page_names").or(error).or_else(|| {
                                target
                                    .is_none()
                                    .then(|| locale.tr("leakshield.display.own_text.full"))
                            }),
                        ),
                        cancel,
                    }
                }
                Dialog::Icons { slot } => {
                    let current = icon_of(&device, page, *slot);
                    let blocked_reason = field_blocked(&device, "page_icons");
                    DialogModel::Icons {
                        title: locale.tr("leakshield.display.icons.title"),
                        text: locale.fmt(
                            "leakshield.display.icons.text",
                            &[
                                ("slot", &(slot + 1).to_string()),
                                ("page", &(page + 1).to_string()),
                            ],
                        ),
                        icons: assets
                            .map(|assets| {
                                assets
                                    .icons
                                    .iter()
                                    .take(16)
                                    .enumerate()
                                    .map(|(index, icon)| {
                                        (index as u8, icon.clone(), Some(index as u8) == current)
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                        none: Action::new(
                            locale.tr("leakshield.display.no_icon"),
                            blocked_reason.clone(),
                        ),
                        import: Action::new(
                            locale.tr("leakshield.display.icons.import"),
                            blocked(&device).or_else(|| {
                                current
                                    .is_none()
                                    .then(|| locale.tr("leakshield.display.icons.pick_first"))
                            }),
                        ),
                        cancel,
                    }
                }
                Dialog::Import(import) => {
                    let (width, height) = import.target.size();
                    let size = format!("{width} x {height}");
                    let icon = match import.target {
                        ImportTarget::Icon { slot } => icon_of(&device, page, slot),
                        ImportTarget::Logo => None,
                    };
                    let is_logo = import.target == ImportTarget::Logo;
                    let result = import.result();
                    let no_file = result
                        .is_none()
                        .then(|| locale.tr("leakshield.display.import.no_file"));
                    let no_icon = (!is_logo && icon.is_none())
                        .then(|| locale.tr("leakshield.display.icons.pick_first"));
                    DialogModel::Import {
                        title: locale.tr(if is_logo {
                            "leakshield.display.import.logo_title"
                        } else {
                            "leakshield.display.import.icon_title"
                        }),
                        text: if is_logo {
                            locale.fmt("leakshield.display.import.logo_text", &[("size", &size)])
                        } else {
                            locale.fmt(
                                "leakshield.display.import.icon_text",
                                &[
                                    ("size", &size),
                                    ("n", &icon.map_or(0, |i| u16::from(i) + 1).to_string()),
                                ],
                            )
                        },
                        choose: locale.tr("leakshield.display.import.choose"),
                        file: import.file.clone(),
                        result: result.map(|bitmap| (bitmap, if is_logo { 2 } else { 4 })),
                        note: import
                            .picture
                            .as_ref()
                            .filter(|picture| picture.scaled())
                            .map(|picture| {
                                locale.fmt(
                                    "leakshield.display.import.scaled",
                                    &[
                                        ("width", &picture.source.0.to_string()),
                                        ("height", &picture.source.1.to_string()),
                                        ("size", &size),
                                    ],
                                )
                            }),
                        error: import.error.as_ref().map(|error| match error {
                            ImportError::NotPng => locale.tr("leakshield.display.import.not_png"),
                            ImportError::Read(message) => locale.fmt(
                                "leakshield.display.import.unreadable",
                                &[("error", message)],
                            ),
                        }),
                        threshold_label: locale.fmt(
                            "leakshield.display.import.threshold",
                            &[("value", &import.threshold.to_string())],
                        ),
                        threshold: import.threshold,
                        invert: Switch {
                            label: locale.tr("leakshield.display.import.invert"),
                            on: import.invert,
                            writable: true,
                        },
                        write: Action::new(
                            locale.tr(if is_logo {
                                "leakshield.display.import.write_logo"
                            } else {
                                "leakshield.display.import.write_icon"
                            }),
                            blocked(&device).or(no_icon).or(no_file),
                        ),
                        cancel,
                    }
                }
            })
        };
        Self {
            state,
            pages_title: locale.tr("leakshield.display.pages"),
            pages,
            page: selected,
            settings,
            dialog,
            hint: locale.tr("leakshield.display.hint"),
            reason,
        }
    }
}
