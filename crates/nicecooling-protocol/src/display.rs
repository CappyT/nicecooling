//! The 10 display pages of report 3, read through the field registry. A page holds raw values:
//! the names and icons are indexes into the display assets of report 8, which stay read-only.

use crate::registry::Registry;
use crate::report3::Report3;

pub const PAGES: usize = 10;
/// Value slots in one page.
pub const SLOTS: usize = 8;

/// Registry ids of the page fields. Each field has one repeat for each page.
pub const PAGE_FIELDS: [&str; 7] = [
    "page_template",
    "page_options",
    "page_icons",
    "page_names",
    "page_sources",
    "page_precisions",
    "page_values",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    /// Layout code, 255 for none.
    pub template: u8,
    /// Bit 0: show in standby. Bit 1: hide in normal operation.
    pub options: u8,
    /// Index into the icons of report 8, 255 for none.
    pub icons: [u8; 4],
    /// Index into the names of report 8, 255 for none.
    pub names: [u8; SLOTS],
    /// Value source of each slot: the sensor index times 4, 255 for none. A value of 80 or more
    /// has no sensor.
    pub sources: [u8; SLOTS],
    pub precisions: [u8; SLOTS],
    pub values: [i16; SLOTS],
}

fn bytes<const N: usize>(raw: &[i64]) -> [u8; N] {
    std::array::from_fn(|i| raw[i] as u8)
}

impl Page {
    /// Page `index`. `None` when the registry lacks a page field or `index` is not below `PAGES`.
    pub fn read(registry: &Registry, report3: &Report3, index: usize) -> Option<Self> {
        let raw = |id: &str| registry.read_repeat(report3, id, index);
        Some(Self {
            template: raw("page_template")?[0] as u8,
            options: raw("page_options")?[0] as u8,
            icons: bytes(&raw("page_icons")?),
            names: bytes(&raw("page_names")?),
            sources: bytes(&raw("page_sources")?),
            precisions: bytes(&raw("page_precisions")?),
            values: {
                let values = raw("page_values")?;
                std::array::from_fn(|i| values[i] as i16)
            },
        })
    }

    /// The raw values of one page field, as `Registry::read_repeat` returns them.
    pub fn raw(&self, field: &str) -> Option<Vec<i64>> {
        let bytes = |b: &[u8]| b.iter().map(|&v| i64::from(v)).collect();
        Some(match field {
            "page_template" => vec![i64::from(self.template)],
            "page_options" => vec![i64::from(self.options)],
            "page_icons" => bytes(&self.icons),
            "page_names" => bytes(&self.names),
            "page_sources" => bytes(&self.sources),
            "page_precisions" => bytes(&self.precisions),
            "page_values" => self.values.iter().map(|&v| i64::from(v)).collect(),
            _ => return None,
        })
    }
}

/// All pages. `None` when the registry lacks a page field.
pub fn read_pages(registry: &Registry, report3: &Report3) -> Option<Vec<Page>> {
    (0..PAGES)
        .map(|index| Page::read(registry, report3, index))
        .collect()
}
