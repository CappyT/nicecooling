//! The Lighting tab (`docs/ui/screens/leakshield-lighting.md`): the RGBpx controllers on the
//! 6 integrated LEDs. A controller is one entry in each `led_controller_*` list.

use std::collections::BTreeSet;

use crate::command::Command;
use crate::i18n::Locale;
use crate::model::{FieldValue, LeakshieldData};
use crate::screens::leakshield::{Device, TabState, fields_command, write_line};
use crate::screens::{FeedbackLine, ScreenInput};

/// The integrated LEDs of the Leakshield.
pub const LEDS: usize = 6;
/// The controllers that the device holds, at most.
pub const MAX_CONTROLLERS: usize = 8;
const COLORS: usize = 6;
const PARAMS: usize = 12;
/// The effect of an unused controller.
const UNUSED: f64 = 0.0;
/// The effect of a new controller: a fixed color.
const FIXED: f64 = 1.0;
/// The effects of the device, by code: `docs/protocol/leakshield.md`.
pub const EFFECTS: [u8; 16] = [1, 2, 3, 19, 12, 4, 5, 7, 8, 9, 10, 11, 14, 18, 13, 33];

/// One color as hue (0 to 360), saturation and value (0 to 255).
pub type Hsv = (f64, f64, f64);

#[derive(Clone, Debug, PartialEq)]
pub struct Controller {
    pub mode: f64,
    pub first: f64,
    pub count: f64,
    /// The index of the sensor that drives the effect, or -1.
    pub source: f64,
    pub params: Vec<f64>,
    pub colors: Vec<Hsv>,
}

impl Controller {
    fn new() -> Self {
        let mut colors = vec![(0.0, 0.0, 0.0); COLORS];
        colors[0] = (0.0, 255.0, 255.0);
        let mut params = vec![0.0; PARAMS];
        params[0] = 50.0;
        params[1] = 100.0;
        Self {
            mode: FIXED,
            first: 0.0,
            count: LEDS as f64,
            source: -1.0,
            params,
            colors,
        }
    }

    pub fn speed(&self) -> f64 {
        self.params.first().copied().unwrap_or(0.0)
    }

    pub fn brightness(&self) -> f64 {
        self.params.get(1).copied().unwrap_or(0.0)
    }

    pub fn range(&self) -> std::ops::Range<usize> {
        let first = self.first.max(0.0) as usize;
        first.min(LEDS)..(first + self.count.max(0.0) as usize).min(LEDS)
    }
}

/// The `led_controller_*` fields as a table of controllers.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Table {
    pub controllers: Vec<Controller>,
}

impl Table {
    pub fn read(data: &LeakshieldData) -> Self {
        let list = |id: &str| data.list(id).map(<[f64]>::to_vec).unwrap_or_default();
        let (mode, first, count, source) = (
            list("led_controller_mode"),
            list("led_controller_first_led"),
            list("led_controller_led_count"),
            list("led_controller_source"),
        );
        let (params, hue, sat, val) = (
            list("led_controller_parameters"),
            list("led_controller_color_hue"),
            list("led_controller_color_saturation"),
            list("led_controller_color_value"),
        );
        let part = |values: &[f64], index: usize, size: usize, fill: f64| -> Vec<f64> {
            (0..size)
                .map(|i| values.get(index * size + i).copied().unwrap_or(fill))
                .collect()
        };
        let controllers = (0..mode.len())
            .map(|i| {
                let hues = part(&hue, i, COLORS, 0.0);
                let sats = part(&sat, i, COLORS, 0.0);
                let vals = part(&val, i, COLORS, 0.0);
                Controller {
                    mode: mode[i],
                    first: first.get(i).copied().unwrap_or(0.0),
                    count: count.get(i).copied().unwrap_or(0.0),
                    source: source.get(i).copied().unwrap_or(-1.0),
                    params: part(&params, i, PARAMS, 0.0),
                    colors: (0..COLORS).map(|c| (hues[c], sats[c], vals[c])).collect(),
                }
            })
            .collect();
        Self { controllers }
    }

    /// The controllers in use, in priority order: the first one wins.
    pub fn used(&self) -> Vec<usize> {
        (0..self.controllers.len())
            .filter(|i| self.controllers[*i].mode != UNUSED)
            .collect()
    }

    /// Every list of the table, as the fields to write.
    pub fn fields(&self) -> Vec<(String, FieldValue)> {
        let each = |f: &dyn Fn(&Controller) -> Vec<f64>| -> FieldValue {
            FieldValue::List(self.controllers.iter().flat_map(f).collect())
        };
        vec![
            ("led_controller_mode".into(), each(&|c| vec![c.mode])),
            ("led_controller_first_led".into(), each(&|c| vec![c.first])),
            ("led_controller_led_count".into(), each(&|c| vec![c.count])),
            ("led_controller_source".into(), each(&|c| vec![c.source])),
            (
                "led_controller_parameters".into(),
                each(&|c| c.params.clone()),
            ),
            (
                "led_controller_color_hue".into(),
                each(&|c| c.colors.iter().map(|k| k.0).collect()),
            ),
            (
                "led_controller_color_saturation".into(),
                each(&|c| c.colors.iter().map(|k| k.1).collect()),
            ),
            (
                "led_controller_color_value".into(),
                each(&|c| c.colors.iter().map(|k| k.2).collect()),
            ),
        ]
    }
}

#[derive(Clone, Debug, Default)]
pub struct LightingUi {
    /// The controllers that the user selected, by index.
    pub selected: BTreeSet<usize>,
    pub effect_picker: Option<u8>,
    /// The table before the last delete, for "Undo", with the time it expires.
    pub undo: Option<(Table, u64)>,
}

/// How long "Undo" shows after a delete.
const UNDO_US: u64 = 10_000_000;

impl LightingUi {
    /// A click on a bar adds it to the selection, or takes it out.
    pub fn toggle(&mut self, index: usize) {
        if !self.selected.remove(&index) {
            self.selected.insert(index);
        }
    }

    /// Up and Down select the bar above or below the selected one.
    pub fn step(&mut self, delta: isize, count: usize) {
        let current = self.selected.iter().next().copied();
        let next = current.map_or(0, |i| i.saturating_add_signed(delta));
        self.selected = BTreeSet::from([next.min(count.saturating_sub(1))]);
    }

    fn single(&self) -> Option<usize> {
        (self.selected.len() == 1).then(|| *self.selected.iter().next().expect("one entry"))
    }

    fn table_of(input: &ScreenInput<'_>, id: &str) -> Option<(Table, bool)> {
        let device = Device::new(input, id);
        let data = device.data?;
        Some((
            Table::read(data),
            device.access("led_controller_mode").writable,
        ))
    }

    fn write(id: &str, table: &Table) -> Command {
        fields_command(id, "lighting", table.fields())
    }

    /// Change the table with `f`. `None` when the device is read-only or `f` refuses.
    fn edit(
        &self,
        input: &ScreenInput<'_>,
        id: &str,
        f: impl FnOnce(&mut Table) -> bool,
    ) -> Option<Command> {
        let (mut table, writable) = Self::table_of(input, id)?;
        (writable && f(&mut table)).then(|| Self::write(id, &table))
    }

    pub fn add(&mut self, input: &ScreenInput<'_>, id: &str) -> Option<Command> {
        self.edit(input, id, |table| {
            if table.used().len() >= MAX_CONTROLLERS {
                return false;
            }
            match table.controllers.iter().position(|c| c.mode == UNUSED) {
                Some(slot) => table.controllers[slot] = Controller::new(),
                None if table.controllers.len() < MAX_CONTROLLERS => {
                    table.controllers.push(Controller::new());
                }
                None => return false,
            }
            true
        })
    }

    pub fn duplicate(&mut self, input: &ScreenInput<'_>, id: &str) -> Option<Command> {
        let index = self.single()?;
        self.edit(input, id, |table| {
            let Some(copy) = table.controllers.get(index).cloned() else {
                return false;
            };
            if table.used().len() >= MAX_CONTROLLERS {
                return false;
            }
            match table.controllers.iter().position(|c| c.mode == UNUSED) {
                Some(slot) => table.controllers[slot] = copy,
                None => table.controllers.push(copy),
            }
            true
        })
    }

    /// "Delete": the selected controllers go at once. "Undo" writes them back for 10 s.
    pub fn delete(&mut self, input: &ScreenInput<'_>, id: &str) -> Option<Command> {
        let (before, _) = Self::table_of(input, id)?;
        let selected = self.selected.clone();
        let command = self.edit(input, id, |table| {
            let mut removed = false;
            for index in &selected {
                if let Some(controller) = table.controllers.get_mut(*index) {
                    *controller = Controller {
                        mode: UNUSED,
                        ..Controller::new()
                    };
                    removed = true;
                }
            }
            removed
        })?;
        self.undo = Some((before, input.now_us + UNDO_US));
        self.selected.clear();
        Some(command)
    }

    pub fn undo(&mut self, id: &str) -> Option<Command> {
        self.undo.take().map(|(table, _)| Self::write(id, &table))
    }

    /// Move the selected bar sideways, or change its length.
    pub fn shift(
        &mut self,
        input: &ScreenInput<'_>,
        id: &str,
        dx: isize,
        dlen: isize,
    ) -> Option<Command> {
        let index = self.single()?;
        self.edit(input, id, |table| {
            let Some(c) = table.controllers.get_mut(index) else {
                return false;
            };
            let count = (c.count as isize + dlen).clamp(1, LEDS as isize);
            let first = (c.first as isize + dx).clamp(0, LEDS as isize - count);
            let changed = count as f64 != c.count || first as f64 != c.first;
            c.count = count as f64;
            c.first = first as f64;
            changed
        })
    }

    /// Alt with Up or Down: the priority of the bar.
    pub fn reorder(&mut self, input: &ScreenInput<'_>, id: &str, delta: isize) -> Option<Command> {
        let index = self.single()?;
        let other = index.checked_add_signed(delta)?;
        let command = self.edit(input, id, |table| {
            if other >= table.controllers.len() || index >= table.controllers.len() {
                return false;
            }
            table.controllers.swap(index, other);
            true
        })?;
        self.selected = BTreeSet::from([other]);
        Some(command)
    }

    pub fn set_effect(&mut self, input: &ScreenInput<'_>, id: &str, code: u8) -> Option<Command> {
        self.effect_picker = None;
        let index = self.single()?;
        if !EFFECTS.contains(&code) {
            return None;
        }
        self.edit(input, id, |table| {
            let Some(c) = table.controllers.get_mut(index) else {
                return false;
            };
            c.mode = f64::from(code);
            true
        })
    }

    /// Speed or brightness of the selected controller, in percent.
    pub fn set_param(
        &mut self,
        input: &ScreenInput<'_>,
        id: &str,
        which: usize,
        value: f64,
    ) -> Option<Command> {
        let index = self.single()?;
        self.edit(input, id, |table| {
            let Some(c) = table.controllers.get_mut(index) else {
                return false;
            };
            c.params[which] = value.clamp(0.0, 100.0);
            true
        })
    }

    pub fn set_hue(
        &mut self,
        input: &ScreenInput<'_>,
        id: &str,
        color: usize,
        hue: f64,
    ) -> Option<Command> {
        let index = self.single()?;
        self.edit(input, id, |table| {
            let Some(k) = table
                .controllers
                .get_mut(index)
                .and_then(|c| c.colors.get_mut(color))
            else {
                return false;
            };
            *k = (hue.rem_euclid(360.0), 255.0, 255.0);
            true
        })
    }

    pub fn set_range(
        &mut self,
        input: &ScreenInput<'_>,
        id: &str,
        first: f64,
        count: f64,
    ) -> Option<Command> {
        let index = self.single()?;
        self.edit(input, id, |table| {
            let Some(c) = table.controllers.get_mut(index) else {
                return false;
            };
            let count = count.clamp(1.0, LEDS as f64);
            c.first = first.clamp(0.0, LEDS as f64 - count);
            c.count = count;
            true
        })
    }

    /// The sensor that drives the effect: an index of the device sensors, or -1.
    pub fn set_source(
        &mut self,
        input: &ScreenInput<'_>,
        id: &str,
        source: i32,
    ) -> Option<Command> {
        let index = self.single()?;
        self.edit(input, id, |table| {
            let Some(c) = table.controllers.get_mut(index) else {
                return false;
            };
            c.source = f64::from(source);
            true
        })
    }
}

/// The colors of a 12 cell preview of an effect, from its first color.
pub fn preview_cells(code: u8, base: Hsv, cells: usize) -> Vec<Hsv> {
    (0..cells)
        .map(|i| {
            let t = i as f64 / cells.max(1) as f64;
            match code {
                3 | 19 => ((base.0 + t * 360.0) % 360.0, 255.0, 255.0),
                2 | 4 | 5 => (
                    base.0,
                    base.1,
                    base.2 * (0.25 + 0.75 * (t * std::f64::consts::PI).sin()),
                ),
                8..=11 => (base.0, base.1, if (i / 3) % 2 == 0 { base.2 } else { 40.0 }),
                33 => (base.0, base.1, base.2 * (0.2 + 0.8 * t)),
                _ => base,
            }
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Cell {
    pub led: usize,
    pub color: Hsv,
    /// Some controller drives the LED.
    pub lit: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub index: usize,
    pub title: String,
    pub first: usize,
    pub count: usize,
    pub selected: bool,
    pub colors: Vec<Hsv>,
    /// "Covered by Controller 1".
    pub covered_by: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolbarButton {
    pub label: String,
    pub enabled: bool,
    pub tooltip: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub title: String,
    pub effect_label: String,
    pub effect: String,
    pub change: String,
    pub first: f64,
    pub count: f64,
    pub range_label: String,
    pub speed: f64,
    pub brightness: f64,
    pub speed_label: String,
    pub brightness_label: String,
    pub colors: Vec<Hsv>,
    pub color_label: String,
    pub driven_label: String,
    pub driven: bool,
    pub sources: Vec<(i32, String, bool)>,
    pub from_label: String,
    pub to_label: String,
    pub writable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Picker {
    pub title: String,
    pub effects: Vec<(u8, String, String, bool)>,
    pub preview: Vec<Hsv>,
    pub cancel: String,
    pub confirm: String,
    pub current: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LightingModel {
    pub state: TabState,
    pub title: String,
    pub cells: Vec<Cell>,
    pub bars: Vec<Bar>,
    pub add: ToolbarButton,
    pub duplicate: ToolbarButton,
    pub delete: ToolbarButton,
    pub empty: Option<(String, String)>,
    pub panel: Option<Panel>,
    pub picker: Option<Picker>,
    pub undo: Option<(String, String)>,
    pub external: String,
    pub priority_hint: String,
    pub reason: Option<String>,
    pub write_line: Option<FeedbackLine>,
}

fn effect_name(locale: Locale, code: u8) -> String {
    locale.tr(&format!("leakshield.lighting.effect.{code}"))
}

impl LightingModel {
    pub fn build(input: &ScreenInput<'_>, ui: &LightingUi, id: &str) -> Self {
        let locale = input.locale;
        let device = Device::new(input, id);
        let state = device.state();
        let table = device.data.map_or_else(Table::default, Table::read);
        let used = table.used();
        let access = device.access("led_controller_mode");
        let writable = access.writable;

        let bars: Vec<Bar> = used
            .iter()
            .enumerate()
            .map(|(position, index)| {
                let c = &table.controllers[*index];
                let range = c.range();
                // A bar is covered when higher bars drive every one of its LEDs.
                let covered = !range.is_empty()
                    && range.clone().all(|led| {
                        used[..position]
                            .iter()
                            .any(|above| table.controllers[*above].range().contains(&led))
                    });
                let cover = used[..position].iter().find(|above| {
                    table.controllers[**above]
                        .range()
                        .any(|led| range.contains(&led))
                });
                Bar {
                    index: *index,
                    title: locale.fmt(
                        "leakshield.lighting.controller_n",
                        &[("n", &(position + 1).to_string())],
                    ),
                    first: range.start,
                    count: range.len(),
                    selected: ui.selected.contains(index),
                    colors: c.colors.iter().take(2).copied().collect(),
                    covered_by: covered.then(|| {
                        let above = cover
                            .and_then(|a| used.iter().position(|u| u == a))
                            .unwrap_or(0);
                        locale.fmt(
                            "leakshield.lighting.covered",
                            &[("n", &(above + 1).to_string())],
                        )
                    }),
                }
            })
            .collect();

        let cells = (0..LEDS)
            .map(|led| {
                let driver = used
                    .iter()
                    .find(|i| table.controllers[**i].range().contains(&led));
                match driver {
                    Some(i) => {
                        let c = &table.controllers[*i];
                        let cells =
                            preview_cells(c.mode as u8, c.colors[0], c.count.max(1.0) as usize);
                        let offset = led.saturating_sub(c.first as usize);
                        Cell {
                            led,
                            color: cells.get(offset).copied().unwrap_or(c.colors[0]),
                            lit: true,
                        }
                    }
                    None => Cell {
                        led,
                        color: (0.0, 0.0, 0.0),
                        lit: false,
                    },
                }
            })
            .collect();

        let button = |key: &str, enabled: bool, tooltip: Option<String>| ToolbarButton {
            label: locale.tr(key),
            enabled: enabled && device.can_write() && writable,
            tooltip,
        };
        let full = used.len() >= MAX_CONTROLLERS;
        let limit = locale.tr("leakshield.lighting.limit");
        let selected_one = ui.selected.len() == 1;

        let panel = ui
            .selected
            .iter()
            .next()
            .filter(|_| selected_one)
            .and_then(|index| {
                let c = table.controllers.get(*index)?;
                let position = used.iter().position(|u| u == index)?;
                Some(Panel {
                    title: locale.fmt(
                        "leakshield.lighting.controller_n",
                        &[("n", &(position + 1).to_string())],
                    ),
                    effect_label: locale.tr("leakshield.lighting.effect_label"),
                    effect: effect_name(locale, c.mode as u8),
                    change: locale.tr("leakshield.lighting.change_effect"),
                    first: c.first,
                    count: c.count,
                    range_label: locale.tr("leakshield.lighting.leds"),
                    speed: c.speed(),
                    brightness: c.brightness(),
                    speed_label: locale.tr("leakshield.lighting.speed"),
                    brightness_label: locale.tr("leakshield.lighting.brightness"),
                    colors: c.colors.iter().take(2).copied().collect(),
                    color_label: locale.tr("leakshield.lighting.color"),
                    driven_label: locale.tr("leakshield.lighting.driven"),
                    driven: c.source >= 0.0,
                    sources: (0..6)
                        .map(|i| {
                            let name = [
                                "pressure",
                                "fill_level",
                                "volume_change",
                                "pressure_change",
                                "feed_rpm",
                                "flow",
                            ][i as usize];
                            let sensor = format!("{id}_{name}");
                            (
                                i,
                                input
                                    .state
                                    .snapshot
                                    .sensors
                                    .get(&sensor)
                                    .map_or_else(|| name.to_owned(), |s| s.name.clone()),
                                c.source == f64::from(i),
                            )
                        })
                        .collect(),
                    from_label: locale.tr("leakshield.lighting.from"),
                    to_label: locale.tr("leakshield.lighting.to"),
                    writable: writable && device.can_write(),
                })
            });

        let picker = ui.effect_picker.map(|chosen| Picker {
            title: locale.tr("leakshield.lighting.picker"),
            effects: EFFECTS
                .iter()
                .map(|code| {
                    (
                        *code,
                        effect_name(locale, *code),
                        locale.tr(&format!("leakshield.lighting.effect_text.{code}")),
                        *code == chosen,
                    )
                })
                .collect(),
            preview: preview_cells(chosen, (200.0, 255.0, 255.0), 12),
            cancel: locale.tr("leakshield.cancel"),
            confirm: locale.tr("leakshield.lighting.effect_confirm"),
            current: chosen,
        });

        Self {
            state,
            title: locale.tr("leakshield.lighting.title"),
            cells,
            empty: bars.is_empty().then(|| {
                (
                    locale.tr("leakshield.lighting.empty"),
                    locale.tr("leakshield.lighting.add_first"),
                )
            }),
            bars,
            add: button(
                "leakshield.lighting.add",
                !full,
                full.then(|| limit.clone()),
            ),
            duplicate: button(
                "leakshield.lighting.duplicate",
                !full && selected_one,
                full.then(|| limit.clone()),
            ),
            delete: button("leakshield.lighting.delete", !ui.selected.is_empty(), None),
            panel,
            picker,
            undo: ui
                .undo
                .as_ref()
                .filter(|(_, until)| input.now_us < *until)
                .map(|_| {
                    (
                        locale.tr("leakshield.lighting.deleted"),
                        locale.tr("leakshield.lighting.undo"),
                    )
                }),
            external: locale.tr("leakshield.lighting.external"),
            priority_hint: locale.tr("leakshield.lighting.priority"),
            reason: (!writable && device.state() == TabState::Ready)
                .then(|| device.reason_text(&access.reason)),
            write_line: write_line(input, id, "lighting"),
        }
    }
}
