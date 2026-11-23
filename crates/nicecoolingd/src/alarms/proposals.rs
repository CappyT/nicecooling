//! The rows of `GetDeviceResponseProposals`: the Device responses that report 3 holds, the
//! proposals of `nicecooling_core::alarms::proposals`, and the alarm responses of the daemon.
//! Nothing here writes: the user accepts a proposal with `SetFields`, the normal write path.

use nicecooling_core::alarms::proposals::{DeviceResponse, propose};

use super::response::Response;
use crate::leakshield::fields::{FieldRow, FieldValue};

/// (device alarm id, current Device response, proposed Device response, overlap warning id or
/// empty).
pub type Row = (String, String, String, String);

/// `fields` are the rows of `GetFields`, `responses` the rows of `GetResponses`. An event whose
/// response field the device does not give, or gives in a state that nobody can trust (unknown
/// firmware or structure), has no row.
pub fn rows(fields: &[FieldRow], responses: &[(String, String, u32)]) -> Vec<Row> {
    let current = |field: &str| {
        let row = fields.iter().find(|row| row.id == field)?;
        if !(row.reason.is_empty() || row.reason == "not-verified") {
            return None;
        }
        match row.value {
            FieldValue::Number(code) => DeviceResponse::from_code(code),
            FieldValue::List(_) => None,
        }
    };
    let poweroff = |event: &str| {
        responses
            .iter()
            .find(|(id, _, _)| id == event)
            .and_then(|(_, response, _)| Response::parse(response))
            == Some(Response::Poweroff)
    };
    propose(current, poweroff)
        .into_iter()
        .map(|p| {
            (
                p.event.to_owned(),
                p.current.as_str().to_owned(),
                p.proposed.as_str().to_owned(),
                p.overlap.map_or("", |o| o.as_str()).to_owned(),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(id: &str, code: f64, reason: &str) -> FieldRow {
        FieldRow {
            id: id.to_owned(),
            risk: "safety",
            value: FieldValue::Number(code),
            writable: false,
            reason: reason.to_owned(),
        }
    }

    fn table(response: &str) -> Vec<(String, String, u32)> {
        vec![("leakshield.leak-alarm".to_owned(), response.to_owned(), 30)]
    }

    #[test]
    fn rows_name_the_events_and_the_responses() {
        let fields = [
            field("leak_rate_alarm_2_response_action", 2.0, "not-verified"),
            field("leak_rate_alarm_1_response_action", 3.0, "not-verified"),
            field("system_alarm_response_action", 0.0, "not-verified"),
        ];
        let rows = rows(&fields, &table("poweroff"));
        let row = |event: &str| rows.iter().find(|r| r.0 == event).unwrap().clone();
        assert_eq!(
            row("leakshield.leak-alarm"),
            (
                "leakshield.leak-alarm".into(),
                "signal-long".into(),
                "none".into(),
                "double-poweroff".into()
            )
        );
        assert_eq!(
            row("leakshield.leak-alarm-small"),
            (
                "leakshield.leak-alarm-small".into(),
                "key-sequence".into(),
                "none".into(),
                "key-sequence-on".into()
            )
        );
        assert_eq!(row("leakshield.system-alarm").2, "signal-long");
        // The other three fields are missing: no row for their events.
        assert_eq!(rows.len(), 3);
    }

    #[test]
    fn a_field_that_nobody_can_trust_has_no_row() {
        let fields = [
            field("leak_rate_alarm_2_response_action", 2.0, "unknown-firmware"),
            field("leak_rate_alarm_1_response_action", 9.0, "not-verified"),
        ];
        assert!(rows(&fields, &table("notify")).is_empty());
    }
}
