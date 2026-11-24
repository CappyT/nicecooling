//! Numbers, units and times as the screens show them.

use chrono::{Local, TimeZone as _};

use crate::i18n::Locale;

/// The space between two groups of three digits: "2 840". It is a no-break space, so a line
/// never ends in the middle of a number.
pub const GROUP: char = '\u{a0}';

impl Locale {
    /// A decimal number with `digits` decimals, for a text: a space groups the thousands.
    /// Italian uses a comma for the decimals.
    pub fn decimal(self, value: f64, digits: usize) -> String {
        let text = self.fixed(value, digits);
        let sign = usize::from(text.starts_with('-'));
        let whole = text[sign..]
            .find(|c: char| !c.is_ascii_digit())
            .map_or(text.len(), |end| sign + end);
        let mut grouped = String::with_capacity(text.len() + 4);
        grouped.push_str(&text[..sign]);
        for (index, digit) in text[sign..whole].chars().enumerate() {
            if index > 0 && (whole - sign - index) % 3 == 0 {
                grouped.push(GROUP);
            }
            grouped.push(digit);
        }
        grouped.push_str(&text[whole..]);
        grouped
    }

    /// A decimal number with `digits` decimals and no grouping, as the user types it.
    fn fixed(self, value: f64, digits: usize) -> String {
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

/// The decimals of a number without a unit: none for a whole number, one otherwise.
fn plain_digits(value: f64) -> usize {
    usize::from((value - value.round()).abs() >= 1e-9)
}

/// A number for an input: no decimals for a whole number, one decimal otherwise. It has no
/// grouping, because the input parses the text again.
pub fn plain(locale: Locale, value: f64) -> String {
    locale.fixed(value, plain_digits(value))
}

/// A number for a text, in the form of `plain` with the thousands grouped.
pub fn compact(locale: Locale, value: f64) -> String {
    locale.decimal(value, plain_digits(value))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{EN, IT};

    #[test]
    fn a_space_groups_the_thousands_of_a_text_in_both_languages() {
        assert_eq!(EN.decimal(2840.0, 0), "2\u{a0}840");
        assert_eq!(IT.decimal(2840.0, 0), "2\u{a0}840");
        assert_eq!(EN.decimal(1_234_567.25, 1), "1\u{a0}234\u{a0}567.2");
        assert_eq!(IT.decimal(12345.5, 1), "12\u{a0}345,5");
        assert_eq!(EN.decimal(-1200.0, 0), "-1\u{a0}200");
        assert_eq!(EN.decimal(999.0, 0), "999");
        assert_eq!(EN.decimal(-999.4, 1), "-999.4");
        assert_eq!(EN.decimal(f64::NAN, 0), "NaN");
        assert_eq!(value_text(EN, 2840.0, "rpm"), "2\u{a0}840 rpm");
        assert_eq!(value_text(IT, 1031.4, "degC"), "1\u{a0}031,4 °C");
        assert_eq!(compact(EN, 4800.0), "4\u{a0}800");
        assert_eq!(compact(IT, 4800.5), "4\u{a0}800,5");
    }

    #[test]
    fn a_number_for_an_input_has_no_grouping() {
        assert_eq!(plain(EN, 4800.0), "4800");
        assert_eq!(plain(IT, 4800.5), "4800,5");
        assert_eq!(plain(EN, 62.0), "62");
    }
}
