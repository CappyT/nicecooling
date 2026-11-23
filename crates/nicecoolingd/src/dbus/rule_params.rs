//! The rules on the wire: `(kind, a{sv})` with the keys of `docs/api.md`, section Rule groups.

use std::collections::HashMap;

use nicecooling_api::Error;
use nicecooling_api::errors::Result;
use nicecooling_api::ids::{self, Kind};
use nicecooling_core::rules::group::RuleDef;
use nicecooling_core::rules::interval::IntervalRule;
use nicecooling_core::rules::process::{MatchMode, MatchOn, ProcessRule};
use nicecooling_core::rules::sensor::{Comparison, SensorRule};
use nicecooling_core::rules::time::TimeRule;
use zbus::{OwnedObjectPath, OwnedVariant, Variant};

pub type WireRule = (String, HashMap<String, OwnedVariant>);

fn variant(value: impl Into<Variant<'static>>) -> OwnedVariant {
    OwnedVariant::try_from(value.into()).expect("a plain value has no file descriptor")
}

fn take<T>(params: &mut HashMap<String, OwnedVariant>, key: &str) -> Result<T>
where
    T: TryFrom<Variant<'static>>,
{
    let value = params
        .remove(key)
        .ok_or_else(|| Error::invalid(format!("the key {key} is missing")))?;
    // A caller may wrap a value in a second variant.
    let mut value: Variant<'static> = value.into();
    while let Variant::Variant(inner) = value {
        value = *inner;
    }
    T::try_from(value).map_err(|_| Error::invalid(format!("the key {key} has the wrong type")))
}

fn take_or<T>(params: &mut HashMap<String, OwnedVariant>, key: &str, default: T) -> Result<T>
where
    T: TryFrom<Variant<'static>>,
{
    if params.contains_key(key) {
        take(params, key)
    } else {
        Ok(default)
    }
}

fn sensor_id(path: &OwnedObjectPath) -> Result<String> {
    ids::id_from_path(Kind::Sensor, path.as_str())
        .map(str::to_owned)
        .ok_or_else(|| Error::invalid(format!("{path} is not a sensor object path")))
}

/// Checks the kind, the keys and their types. The values are checked by `RuleDef::validate`.
pub fn decode_rule(kind: &str, mut params: HashMap<String, OwnedVariant>) -> Result<RuleDef> {
    let rule = match kind {
        "sensor" => {
            let comparison: String = take(&mut params, "comparison")?;
            let comparison = Comparison::parse(&comparison)
                .ok_or_else(|| Error::invalid(format!("{comparison:?} is not a comparison")))?;
            RuleDef::Sensor(SensorRule {
                sensor: sensor_id(&take(&mut params, "sensor")?)?,
                comparison,
                value: take(&mut params, "value")?,
                value_max: if params.contains_key("value_max") {
                    Some(take(&mut params, "value_max")?)
                } else {
                    None
                },
                hold_s: take(&mut params, "hold_s")?,
            })
        }
        "process" => {
            let mode: String = take(&mut params, "match")?;
            let on: String = take_or(&mut params, "match_on", "name".to_owned())?;
            RuleDef::Process(ProcessRule {
                names: take(&mut params, "names")?,
                match_mode: MatchMode::parse(&mode)
                    .ok_or_else(|| Error::invalid(format!("{mode:?} is not any, all or none")))?,
                match_on: MatchOn::parse(&on)
                    .ok_or_else(|| Error::invalid(format!("{on:?} is not name or cmdline")))?,
            })
        }
        "time" => RuleDef::Time(TimeRule {
            days: take(&mut params, "days")?,
            start: take(&mut params, "start")?,
            end: take(&mut params, "end")?,
        }),
        "interval" => RuleDef::Interval(IntervalRule {
            interval_s: take(&mut params, "interval_s")?,
            delay_s: take(&mut params, "delay_s")?,
            duration_s: take(&mut params, "duration_s")?,
        }),
        other => {
            return Err(Error::invalid(format!(
                "{other:?} is not a rule kind: sensor, process, time or interval"
            )));
        }
    };
    if let Some(key) = params.keys().min() {
        return Err(Error::invalid(format!("the key {key} is not a {kind} key")));
    }
    Ok(rule)
}

pub fn encode_rule(rule: &RuleDef) -> WireRule {
    let mut params = HashMap::new();
    let mut put = |key: &str, value: OwnedVariant| {
        params.insert(key.to_owned(), value);
    };
    match rule {
        RuleDef::Sensor(rule) => {
            put(
                "sensor",
                variant(
                    OwnedObjectPath::try_from(ids::object_path(Kind::Sensor, &rule.sensor))
                        .expect("an identifier is a valid path element"),
                ),
            );
            put("comparison", variant(rule.comparison.as_str()));
            put("value", variant(rule.value));
            if let Some(max) = rule.value_max {
                put("value_max", variant(max));
            }
            put("hold_s", variant(rule.hold_s));
        }
        RuleDef::Process(rule) => {
            put("names", variant(rule.names.clone()));
            put("match", variant(rule.match_mode.as_str()));
            put("match_on", variant(rule.match_on.as_str()));
        }
        RuleDef::Time(rule) => {
            put("days", variant(rule.days.clone()));
            put("start", variant(rule.start));
            put("end", variant(rule.end));
        }
        RuleDef::Interval(rule) => {
            put("interval_s", variant(rule.interval_s));
            put("delay_s", variant(rule.delay_s));
            put("duration_s", variant(rule.duration_s));
        }
    }
    (rule.kind().to_owned(), params)
}
