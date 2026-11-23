//! The Fields tab (`docs/ui/screens/leakshield-fields.md`): every field with its verification
//! state and its risk class. The tab has no control that writes.

use crate::i18n::Locale;
use crate::model::{FieldEntry, FieldRisk, FieldValue};
use crate::screens::ScreenInput;
use crate::screens::format::plain;
use crate::screens::leakshield::{Device, TabState};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    ReadOnly,
    Safety,
}

impl Filter {
    pub const ALL: [Filter; 3] = [Filter::All, Filter::ReadOnly, Filter::Safety];

    pub fn key(self) -> &'static str {
        match self {
            Self::All => "leakshield.fields.filter.all",
            Self::ReadOnly => "leakshield.fields.filter.read_only",
            Self::Safety => "leakshield.fields.filter.safety",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct FieldsUi {
    pub filter: Filter,
    pub search: String,
    pub selected: Option<String>,
}

impl FieldsUi {
    /// Select a row, or close its panel when it is selected already.
    pub fn select(&mut self, id: &str) {
        self.selected = if self.selected.as_deref() == Some(id) {
            None
        } else {
            Some(id.to_owned())
        };
    }

    /// Up and Down move in the table. `ids` are the visible rows in order.
    pub fn step(&mut self, ids: &[String], delta: isize) {
        let current = self
            .selected
            .as_ref()
            .and_then(|id| ids.iter().position(|row| row == id));
        let next = match current {
            Some(index) => index
                .saturating_add_signed(delta)
                .min(ids.len().saturating_sub(1)),
            None => 0,
        };
        self.selected = ids.get(next).cloned();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Verification {
    CodeDefined,
    PlausibleRead,
    VerificationWritePassed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldRow {
    pub id: String,
    pub name: String,
    pub value: String,
    pub risk: FieldRisk,
    pub risk_text: String,
    pub verification: Verification,
    pub verification_text: String,
    pub writable: bool,
    pub access_text: String,
    pub selected: bool,
    /// The panel under a selected row.
    pub explanation: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldGroup {
    pub title: String,
    pub rows: Vec<FieldRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldsState {
    Loading,
    /// The table has no row for the filter.
    Empty,
    /// Unknown firmware: the daemon cannot decode the settings.
    UnknownFirmware,
    Ready,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FieldsModel {
    pub state: FieldsState,
    /// The Leakshield does not answer: the table shows its last read, dim.
    pub dim: bool,
    pub summary: String,
    pub filter: Filter,
    pub filter_label: String,
    pub filters: Vec<(Filter, String)>,
    pub search_placeholder: String,
    pub columns: Vec<String>,
    pub groups: Vec<FieldGroup>,
    pub empty: Option<String>,
    pub unknown_text: Option<String>,
    pub risk_help: String,
    pub verification_help: Vec<String>,
    pub copy_label: String,
    pub copy_text: String,
}

/// The setting group of a field, as the table groups them.
pub fn group_of(id: &str) -> &'static str {
    const MODE: [&str; 5] = [
        "default_mode",
        "test_pressure",
        "deaeration_pressure",
        "refill_pressure",
        "device_profile",
    ];
    if MODE.contains(&id) {
        "mode"
    } else if id.contains("_pressure_") || id == "shield_flags" || id.starts_with("vacuum_timer") {
        "pressure"
    } else if id.starts_with("reservoir_") || id == "fill_level_calibration" || id == "loop_height"
    {
        "reservoir"
    } else if id.starts_with("pump_") || id == "refill_duration_warning" {
        "pump"
    } else if id.starts_with("alarm_")
        || id.ends_with("_threshold")
        || id.contains("_response_")
        || id == "signal_output_config"
    {
        "alarms"
    } else if id == "standby_conditions" {
        "standby"
    } else if id.starts_with("display_") || id.starts_with("page_") || id.starts_with("sensor_") {
        "display"
    } else if id.starts_with("led_") {
        "lighting"
    } else if id.starts_with("clock_") {
        "clock"
    } else {
        "mode"
    }
}

const GROUPS: [&str; 9] = [
    "mode",
    "pressure",
    "reservoir",
    "pump",
    "alarms",
    "standby",
    "display",
    "lighting",
    "clock",
];

/// The unit of a field, from its name. The contract sends the value in this unit.
pub fn unit_of(id: &str) -> &'static str {
    if id.contains("leak_rate") {
        "ml/h"
    } else if id.contains("pressure") {
        "mbar"
    } else if id.ends_with("_speed") {
        "rpm"
    } else if id.ends_with("_flow") {
        "L/h"
    } else if id.contains("volume") {
        "ml"
    } else if id.contains("threshold") || id.ends_with("brightness") {
        "%"
    } else if id == "display_page_time" {
        "s"
    } else {
        ""
    }
}

pub fn humanize(id: &str) -> String {
    let mut text = id.replace('_', " ");
    if let Some(first) = text.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    text
}

fn value_text(locale: Locale, field: &FieldEntry) -> String {
    let unit = unit_of(&field.id);
    let with_unit = |text: String| {
        if unit.is_empty() {
            text
        } else {
            format!("{text} {unit}")
        }
    };
    match &field.value {
        FieldValue::Flag(on) => locale.tr(if *on {
            "leakshield.yes"
        } else {
            "leakshield.no"
        }),
        FieldValue::Number(number) => with_unit(plain(locale, *number)),
        FieldValue::Text(text) => text.clone(),
        FieldValue::List(list) => {
            let shown: Vec<String> = list.iter().take(6).map(|v| plain(locale, *v)).collect();
            let more = if list.len() > 6 { ", ..." } else { "" };
            format!("[{}{more}]", shown.join(", "))
        }
        FieldValue::TextList(names) => {
            let more = if names.len() > 6 { ", ..." } else { "" };
            format!(
                "[{}{more}]",
                names.iter().take(6).cloned().collect::<Vec<_>>().join(", ")
            )
        }
    }
}

fn verification_of(field: &FieldEntry) -> Verification {
    match (field.writable, field.risk) {
        (false, _) => Verification::CodeDefined,
        (true, FieldRisk::Low) => Verification::PlausibleRead,
        (true, FieldRisk::Safety) => Verification::VerificationWritePassed,
    }
}

fn explanation(locale: Locale, device: &Device<'_>, field: &FieldEntry) -> String {
    match verification_of(field) {
        Verification::CodeDefined if field.reason == "not-verified" || field.reason.is_empty() => {
            locale.tr("leakshield.fields.explain.code_defined")
        }
        Verification::CodeDefined => device.reason_text(&field.reason),
        Verification::PlausibleRead => locale.tr("leakshield.fields.explain.plausible_low"),
        Verification::VerificationWritePassed => locale.fmt(
            "leakshield.fields.explain.passed",
            &[(
                "firmware",
                &device.info.map_or(0, |info| info.firmware).to_string(),
            )],
        ),
    }
}

impl FieldsModel {
    pub fn build(input: &ScreenInput<'_>, ui: &FieldsUi, id: &str) -> Self {
        let locale = input.locale;
        let device = Device::new(input, id);
        let fields: &[FieldEntry] = device.data.map_or(&[], |data| &data.fields);
        let unknown = device.unknown_firmware();
        let verified = fields.iter().filter(|f| f.writable).count();
        let query = ui.search.trim().to_lowercase();
        let matches = |field: &FieldEntry| {
            let filter = match ui.filter {
                Filter::All => true,
                Filter::ReadOnly => !field.writable,
                Filter::Safety => field.risk == FieldRisk::Safety,
            };
            filter
                && (query.is_empty()
                    || field.id.contains(&query)
                    || humanize(&field.id).to_lowercase().contains(&query))
        };
        let row = |field: &FieldEntry| {
            let verification = verification_of(field);
            let selected = ui.selected.as_deref() == Some(field.id.as_str());
            FieldRow {
                id: field.id.clone(),
                name: humanize(&field.id),
                value: value_text(locale, field),
                risk: field.risk,
                risk_text: locale.tr(match field.risk {
                    FieldRisk::Safety => "leakshield.fields.risk.safety",
                    FieldRisk::Low => "leakshield.fields.risk.low",
                }),
                verification,
                verification_text: locale.tr(match verification {
                    Verification::CodeDefined => "leakshield.fields.verification.code_defined",
                    Verification::PlausibleRead => "leakshield.fields.verification.plausible",
                    Verification::VerificationWritePassed => {
                        "leakshield.fields.verification.passed"
                    }
                }),
                writable: field.writable,
                access_text: locale.tr(if field.writable {
                    "leakshield.yes"
                } else {
                    "leakshield.fields.read_only"
                }),
                selected,
                explanation: selected.then(|| explanation(locale, &device, field)),
            }
        };
        let groups: Vec<FieldGroup> = GROUPS
            .iter()
            .filter_map(|group| {
                let rows: Vec<FieldRow> = fields
                    .iter()
                    .filter(|f| group_of(&f.id) == *group && matches(f))
                    .map(&row)
                    .collect();
                (!rows.is_empty()).then(|| FieldGroup {
                    title: locale.tr(&format!("leakshield.fields.group.{group}")),
                    rows,
                })
            })
            .collect();
        let state = if device.state() == TabState::Loading {
            FieldsState::Loading
        } else if unknown {
            FieldsState::UnknownFirmware
        } else if groups.is_empty() && !fields.is_empty() {
            FieldsState::Empty
        } else {
            FieldsState::Ready
        };
        let summary = locale.fmt(
            "leakshield.fields.summary",
            &[
                ("total", &fields.len().to_string()),
                ("verified", &verified.to_string()),
                ("read_only", &(fields.len() - verified).to_string()),
            ],
        );
        let copy_text = groups
            .iter()
            .flat_map(|group| &group.rows)
            .map(|row| {
                format!(
                    "{}\t{}\t{}\t{}\t{}",
                    row.id, row.value, row.risk_text, row.verification_text, row.access_text
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Self {
            state,
            dim: device.state() == TabState::Offline,
            summary,
            filter: ui.filter,
            filter_label: locale.tr(ui.filter.key()),
            filters: Filter::ALL
                .iter()
                .map(|filter| (*filter, locale.tr(filter.key())))
                .collect(),
            search_placeholder: locale.tr("leakshield.fields.search"),
            columns: ["field", "value", "risk", "verification", "writable"]
                .iter()
                .map(|name| locale.tr(&format!("leakshield.fields.column.{name}")))
                .collect(),
            groups,
            empty: (state == FieldsState::Empty).then(|| locale.tr("leakshield.fields.empty")),
            unknown_text: unknown.then(|| locale.tr("leakshield.fields.unknown")),
            risk_help: locale.tr("leakshield.fields.risk_help"),
            verification_help: [
                "leakshield.fields.help.code_defined",
                "leakshield.fields.help.plausible",
                "leakshield.fields.help.passed",
            ]
            .iter()
            .map(|key| locale.tr(key))
            .collect(),
            copy_label: locale.tr("leakshield.fields.copy"),
            copy_text,
        }
    }

    /// The identifiers of the visible rows, in table order.
    pub fn row_ids(&self) -> Vec<String> {
        self.groups
            .iter()
            .flat_map(|group| group.rows.iter().map(|row| row.id.clone()))
            .collect()
    }
}
