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
        match &row.value {
            FieldValue::Name(name) => DeviceResponse::parse(name),
            _ => None,
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

    fn field(id: &str, name: &str, reason: &str) -> FieldRow {
        FieldRow {
            id: id.to_owned(),
            risk: "safety",
            value: FieldValue::Name(name.to_owned()),
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
            field(
                "leak_rate_alarm_2_response_action",
                "signal-long",
                "not-verified",
            ),
            field(
                "leak_rate_alarm_1_response_action",
                "key-sequence",
                "not-verified",
            ),
            field("system_alarm_response_action", "none", "not-verified"),
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
    fn a_warning_with_a_signal_and_poweroff_has_no_overlap() {
        let fields = [field(
            "leak_rate_warning_response_action",
            "signal-long",
            "not-verified",
        )];
        let responses = vec![(
            "leakshield.leak-warning".to_owned(),
            "poweroff".to_owned(),
            30,
        )];
        let rows = rows(&fields, &responses);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            (rows[0].2.as_str(), rows[0].3.as_str()),
            ("signal-long", "")
        );
    }

    #[test]
    fn a_field_that_nobody_can_trust_has_no_row() {
        let fields = [
            field(
                "leak_rate_alarm_2_response_action",
                "signal-long",
                "unknown-firmware",
            ),
            field(
                "leak_rate_alarm_1_response_action",
                "unknown-9",
                "not-verified",
            ),
        ];
        assert!(rows(&fields, &table("notify")).is_empty());
    }
}
