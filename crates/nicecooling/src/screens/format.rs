//! Numbers, units and times as the screens show them.

use chrono::{Local, TimeZone as _};

use crate::i18n::Locale;

impl Locale {
    /// A decimal number with `digits` decimals. Italian uses a comma.
    pub fn decimal(self, value: f64, digits: usize) -> String {
        let text = format!("{value:.digits$}");
        if self.code() == "it" {
            text.replace('.', ",")
        } else {
            text
        }
    }
}

/// The text of a unit identifier of `docs/api.md`.
pub fn unit_text(unit: &str) -> &str {
    match unit {
        "degC" => "°C",
        "l/h" => "L/h",
        other => other,
    }
}

/// A number for an input: no decimals for a whole number, one decimal otherwise.
pub fn plain(locale: Locale, value: f64) -> String {
    if (value - value.round()).abs() < 1e-9 {
        locale.decimal(value, 0)
    } else {
        locale.decimal(value, 1)
    }
}

/// The decimals that a unit shows.
fn digits(unit: &str) -> usize {
    match unit {
        "degC" | "V" | "A" | "W" | "mbar/min" => 1,
        _ => 0,
    }
}

/// A number in the precision of its unit, without the unit.
pub fn number(locale: Locale, value: f64, unit: &str) -> String {
    locale.decimal(value, digits(unit))
}

/// "31.4 °C", "115 L/h", "62 %".
pub fn value_text(locale: Locale, value: f64, unit: &str) -> String {
    let number = number(locale, value, unit);
    match unit_text(unit) {
        "" => number,
        text => format!("{number} {text}"),
    }
}

/// The text of a sensor value, or "--" when it has none.
pub fn optional_value(locale: Locale, value: Option<f64>, unit: &str) -> String {
    value.map_or_else(|| "--".to_owned(), |v| value_text(locale, v, unit))
}

/// A Unix time in microseconds, formatted in the local time zone.
fn local_text(us: u64, pattern: &str) -> String {
    Local
        .timestamp_micros(us as i64)
        .single()
        .map_or_else(String::new, |time| time.format(pattern).to_string())
}

/// "21:46:51" in the local time zone.
pub fn clock(us: u64) -> String {
    local_text(us, "%H:%M:%S")
}

/// "21:46" in the local time zone.
pub fn clock_minutes(us: u64) -> String {
    local_text(us, "%H:%M")
}

/// "2026-11-20 08:14".
pub fn date_time(us: u64) -> String {
    local_text(us, "%Y-%m-%d %H:%M")
}

/// "2026-11-21 18:02:11".
pub fn date_time_seconds(us: u64) -> String {
    local_text(us, "%Y-%m-%d %H:%M:%S")
}
