//! The RGBpx lighting of report 3, read through the field registry: the LED strip and the two LED
//! controllers. Values are raw. The strip brightness is 0-255 (the registry scale shows percent).

use crate::registry::Registry;
use crate::report3::Report3;

pub const CONTROLLERS: usize = 2;
/// Effect parameters of one controller.
pub const PARAMETERS: usize = 12;
/// Colors of one controller.
pub const COLORS: usize = 6;

pub const STRIP_FIELDS: [&str; 2] = ["led_strip_brightness", "led_strip_flags"];

/// Registry ids of the controller fields. Each field has one repeat for each controller.
pub const CONTROLLER_FIELDS: [&str; 19] = [
    "led_controller_strip",
    "led_controller_first_led",
    "led_controller_led_count",
    "led_controller_mode",
    "led_controller_flags",
    "led_controller_source",
    "led_controller_source_filter",
    "led_controller_binding_1_x1",
    "led_controller_binding_1_x2",
    "led_controller_binding_1_y1",
    "led_controller_binding_1_y2",
    "led_controller_binding_2_x1",
    "led_controller_binding_2_x2",
    "led_controller_binding_2_y1",
    "led_controller_binding_2_y2",
    "led_controller_parameters",
    "led_controller_color_hue",
    "led_controller_color_saturation",
    "led_controller_color_value",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedStrip {
    /// 0-255.
    pub brightness: u8,
    /// Bit 1: strip off. Bit 2: strip on in standby.
    pub flags: u16,
}

/// Maps the source value range `x1..x2` to the output range `y1..y2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub x1: i16,
    pub x2: i16,
    pub y1: u8,
    pub y2: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    /// 0-360 degrees.
    pub hue: u16,
    pub saturation: u8,
    pub value: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedController {
    pub strip: u8,
    pub first_led: u8,
    pub led_count: u8,
    /// Effect code, 0 for unused.
    pub mode: u8,
    /// Bit 14: binding 1 off. Bit 15: binding 2 off. Other bits are effect options.
    pub flags: u16,
    /// Sensor that drives the effect, -1 for none.
    pub source: i16,
    /// Rise and fall filter of the source.
    pub source_filter: [u8; 2],
    pub bindings: [Binding; 2],
    pub parameters: [i16; PARAMETERS],
    pub colors: [Color; COLORS],
}

impl LedStrip {
    pub fn read(registry: &Registry, report3: &Report3) -> Option<Self> {
        let raw = |id: &str| registry.read_repeat(report3, id, 0);
        Some(Self {
            brightness: raw("led_strip_brightness")?[0] as u8,
            flags: raw("led_strip_flags")?[0] as u16,
        })
    }

    /// The raw values of one strip field.
    pub fn raw(&self, field: &str) -> Option<Vec<i64>> {
        Some(match field {
            "led_strip_brightness" => vec![i64::from(self.brightness)],
            "led_strip_flags" => vec![i64::from(self.flags)],
            _ => return None,
        })
    }
}

impl LedController {
    /// Controller `index`. `None` when the registry lacks a controller field or `index` is not
    /// below `CONTROLLERS`.
    pub fn read(registry: &Registry, report3: &Report3, index: usize) -> Option<Self> {
        let raw = |id: &str| registry.read_repeat(report3, id, index);
        let one = |id: &str| raw(id).map(|v| v[0]);
        let binding = |n: u8| -> Option<Binding> {
            Some(Binding {
                x1: one(&format!("led_controller_binding_{n}_x1"))? as i16,
                x2: one(&format!("led_controller_binding_{n}_x2"))? as i16,
                y1: one(&format!("led_controller_binding_{n}_y1"))? as u8,
                y2: one(&format!("led_controller_binding_{n}_y2"))? as u8,
            })
        };
        let parameters = raw("led_controller_parameters")?;
        let hue = raw("led_controller_color_hue")?;
        let saturation = raw("led_controller_color_saturation")?;
        let value = raw("led_controller_color_value")?;
        let filter = raw("led_controller_source_filter")?;
        Some(Self {
            strip: one("led_controller_strip")? as u8,
            first_led: one("led_controller_first_led")? as u8,
            led_count: one("led_controller_led_count")? as u8,
            mode: one("led_controller_mode")? as u8,
            flags: one("led_controller_flags")? as u16,
            source: one("led_controller_source")? as i16,
            source_filter: std::array::from_fn(|i| filter[i] as u8),
            bindings: [binding(1)?, binding(2)?],
            parameters: std::array::from_fn(|i| parameters[i] as i16),
            colors: std::array::from_fn(|i| Color {
                hue: hue[i] as u16,
                saturation: saturation[i] as u8,
                value: value[i] as u8,
            }),
        })
    }

    /// The raw values of one controller field, as `Registry::read_repeat` returns them.
    pub fn raw(&self, field: &str) -> Option<Vec<i64>> {
        let [b1, b2] = self.bindings;
        let colors = |f: fn(&Color) -> i64| self.colors.iter().map(f).collect();
        Some(match field {
            "led_controller_strip" => vec![i64::from(self.strip)],
            "led_controller_first_led" => vec![i64::from(self.first_led)],
            "led_controller_led_count" => vec![i64::from(self.led_count)],
            "led_controller_mode" => vec![i64::from(self.mode)],
            "led_controller_flags" => vec![i64::from(self.flags)],
            "led_controller_source" => vec![i64::from(self.source)],
            "led_controller_source_filter" => {
                self.source_filter.iter().map(|&v| i64::from(v)).collect()
            }
            "led_controller_binding_1_x1" => vec![i64::from(b1.x1)],
            "led_controller_binding_1_x2" => vec![i64::from(b1.x2)],
            "led_controller_binding_1_y1" => vec![i64::from(b1.y1)],
            "led_controller_binding_1_y2" => vec![i64::from(b1.y2)],
            "led_controller_binding_2_x1" => vec![i64::from(b2.x1)],
            "led_controller_binding_2_x2" => vec![i64::from(b2.x2)],
            "led_controller_binding_2_y1" => vec![i64::from(b2.y1)],
            "led_controller_binding_2_y2" => vec![i64::from(b2.y2)],
            "led_controller_parameters" => self.parameters.iter().map(|&v| i64::from(v)).collect(),
            "led_controller_color_hue" => colors(|c| i64::from(c.hue)),
            "led_controller_color_saturation" => colors(|c| i64::from(c.saturation)),
            "led_controller_color_value" => colors(|c| i64::from(c.value)),
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Lighting {
    pub strip: LedStrip,
    pub controllers: Vec<LedController>,
}

/// The strip and both controllers. `None` when the registry lacks a lighting field.
pub fn read_lighting(registry: &Registry, report3: &Report3) -> Option<Lighting> {
    Some(Lighting {
        strip: LedStrip::read(registry, report3)?,
        controllers: (0..CONTROLLERS)
            .map(|index| LedController::read(registry, report3, index))
            .collect::<Option<_>>()?,
    })
}
