//! The Display tab (`docs/ui/screens/leakshield-display.md`): the 10 pages of the OLED display
//! and its settings.

use crate::command::Command;
use crate::i18n::Locale;
use crate::model::{FieldValue, LeakshieldData};
use crate::screens::format::{optional_value, plain};
use crate::screens::leakshield::{Device, TabState, fields_command, write_line};
use crate::screens::{FeedbackLine, ScreenInput};

pub const PAGES: usize = 10;
/// The value slots of one page in `page_sources`.
const SLOTS: usize = 8;
const BRIGHTNESS: [&str; 4] = ["maximum", "mid", "low", "off"];

/// "No source" as a sensor index and as a byte of `page_sources`.
pub const NO_SOURCE: u8 = 255;
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

#[derive(Clone, Debug, Default)]
pub struct DisplayUi {
    pub page: usize,
    /// The template gallery is open.
    pub gallery: bool,
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
            || !TEMPLATES.iter().any(|(template, _)| *template == name)
        {
            return None;
        }
        *slot = name.to_owned();
        Some(fields_command(
            id,
            "display:template",
            vec![("page_template".into(), FieldValue::TextList(list))],
        ))
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
            "display:source",
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
            "display:option",
            vec![("page_options".into(), FieldValue::List(list))],
        ))
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

pub fn set_interval(input: &ScreenInput<'_>, id: &str, seconds: f64) -> Option<Command> {
    let device = Device::new(input, id);
    if !device.access("display_page_time").writable || !(0.0..=6553.0).contains(&seconds) {
        return None;
    }
    Some(fields_command(
        id,
        "display:interval",
        vec![("display_page_time".into(), FieldValue::Number(seconds))],
    ))
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
    /// Both switches are off: the preview is dim.
    pub off: Option<String>,
    /// The lines of the preview: a name and its value.
    pub lines: Vec<(String, String)>,
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
    pub change: String,
    pub choose: Option<String>,
    pub rows: Vec<ValueRow>,
    pub logo_note: Option<String>,
    pub normal: Switch,
    pub standby: Switch,
    pub link: String,
    pub write_line: Option<FeedbackLine>,
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
    pub interval: String,
    pub interval_writable: bool,
    pub invert: Switch,
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
    /// The registry name and the label of each template.
    pub gallery: Option<Vec<(String, String)>>,
    pub gallery_title: String,
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
        let templates = device.text_list("page_template").unwrap_or(&[]);
        let options = device.list("page_options").unwrap_or(&[]);
        let source_list = device.list("page_sources").unwrap_or(&[]);
        let template_of = |page: usize| {
            templates
                .get(page)
                .map_or(NO_TEMPLATE, String::as_str)
                .to_owned()
        };
        let mask_of = |page: usize| options.get(page).map_or(3, |m| *m as u32);

        let lines_of = |page: usize| -> Vec<(String, String)> {
            let template = template_of(page);
            (0..template_values(&template))
                .map(|slot| {
                    let index = source_list
                        .get(page * SLOTS + slot)
                        .map_or(NO_SOURCE, |v| source_sensor(*v));
                    let (name, value, _) = sources.describe(index);
                    (name, optional_value(locale, value, &sources.unit(index)))
                })
                .collect()
        };

        let pages = (0..PAGES)
            .map(|page| {
                let template = template_of(page);
                let mask = mask_of(page);
                PageCard {
                    index: page,
                    number: (page + 1).to_string(),
                    template_name: template_name(locale, &template),
                    empty: template == NO_TEMPLATE,
                    template,
                    selected: page == ui.page,
                    off: (mask & 1 == 0 && mask & 2 != 0)
                        .then(|| locale.tr("leakshield.display.not_shown")),
                    lines: lines_of(page),
                }
            })
            .collect();

        let page = ui.page.min(PAGES - 1);
        let template = template_of(page);
        let mask = mask_of(page);
        let rows = (0..template_values(&template))
            .map(|slot| {
                let index = source_list
                    .get(page * SLOTS + slot)
                    .map_or(NO_SOURCE, |v| source_sensor(*v));
                let (text, _, has_source) = sources.describe(index);
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
                let (_, value, _) = sources.describe(index);
                ValueRow {
                    slot,
                    label: locale.fmt(
                        "leakshield.display.value_n",
                        &[("n", &(slot + 1).to_string())],
                    ),
                    sources: choices,
                    source_text: text.clone(),
                    value: optional_value(locale, value, &sources.unit(index)),
                    warning: (!has_source && (8..=19).contains(&index)).then(|| {
                        locale.fmt(
                            "leakshield.display.no_data_source",
                            &[("name", &sources.describe(index).0)],
                        )
                    }),
                }
            })
            .collect();
        let option_switch = |label: &str, on: bool| Switch {
            label: locale.tr(label),
            on,
            writable: device.access("page_options").writable,
        };
        let selected = SelectedPage {
            title: locale.fmt(
                "leakshield.display.page_n",
                &[("n", &(page + 1).to_string())],
            ),
            template_label: locale.tr("leakshield.display.template_label"),
            template_name: template_name(locale, &template),
            change: locale.tr("leakshield.display.change"),
            choose: (template == NO_TEMPLATE).then(|| locale.tr("leakshield.display.choose")),
            rows,
            logo_note: (template == "logo").then(|| locale.tr("leakshield.display.logo_note")),
            normal: option_switch("leakshield.display.show_normal", mask & 2 == 0),
            standby: option_switch("leakshield.display.show_standby", mask & 1 != 0),
            link: locale.tr("leakshield.display.open_sensors"),
            write_line: write_line(input, id, "display:template")
                .or_else(|| write_line(input, id, "display:source"))
                .or_else(|| write_line(input, id, "display:option")),
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
            auto_page: flag("leakshield.display.auto_page", flags & 1 == 0),
            interval_label: locale.tr("leakshield.display.interval"),
            interval: interval.map_or_else(String::new, |v| plain(locale, v)),
            interval_writable: device.access("display_page_time").writable,
            invert: flag("leakshield.display.invert", flags & 4 != 0),
            auto_invert: flag("leakshield.display.auto_invert", flags & 8 == 0),
            auto_invert_text: locale.tr("leakshield.display.auto_invert_text"),
            standby: flag("leakshield.display.off_standby", flags & 2 != 0),
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
        Self {
            state,
            pages_title: locale.tr("leakshield.display.pages"),
            pages,
            page: selected,
            settings,
            gallery: ui.gallery.then(|| {
                TEMPLATES
                    .iter()
                    .map(|(name, _)| ((*name).to_owned(), template_name(locale, name)))
                    .collect()
            }),
            gallery_title: locale.tr("leakshield.display.gallery"),
            hint: locale.tr("leakshield.display.hint"),
            reason,
        }
    }
}
