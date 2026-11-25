//! The placeholders of a message and the environment of a command response.
//!
//! The text of the local time is an input: the crate has no clock for local time.

use crate::eval::GraphPass;
use crate::funcs::{RoundMode, round_to};
use crate::model::{Graph, Kind};

/// The value of one labelled node when a response runs.
#[derive(Clone, Debug, PartialEq)]
pub struct LabelledValue {
    pub label: String,
    /// `None` for a node with no value.
    pub value: Option<f64>,
    /// The display text of the unit. Only a sensor source node has one.
    pub unit: String,
}

/// What a message can name.
#[derive(Clone, Copy, Debug)]
pub struct Context<'a> {
    pub graph_name: &'a str,
    /// The local date and time as `YYYY-MM-DD HH:MM:SS`, made by the caller.
    pub time: &'a str,
    pub nodes: &'a [LabelledValue],
}

/// The labelled nodes of a Graph with their values of one pass. `sensor_unit` gives the display
/// text of the unit of a sensor id, and the empty text for a sensor that is unknown.
pub fn labelled_values(
    graph: &Graph,
    pass: &GraphPass,
    sensor_unit: impl Fn(&str) -> String,
) -> Vec<LabelledValue> {
    graph
        .nodes
        .iter()
        .filter_map(|node| {
            let label = node.label.clone()?;
            let value = *pass.values.get(&node.id)?;
            let unit = if node.kind == Kind::SensorSource {
                sensor_unit(&node.text("sensor"))
            } else {
                String::new()
            };
            Some(LabelledValue { label, value, unit })
        })
        .collect()
}

/// A value for a message: `--` for no value, otherwise at most 2 decimals, a tie away from 0, and
/// no trailing zero.
pub fn format_value(value: Option<f64>) -> String {
    let Some(value) = value else {
        return "--".to_owned();
    };
    round_to(value, 2, RoundMode::Nearest).to_string()
}

/// Expands `{graph}`, `{time}`, `{value:<label>}` and `{unit:<label>}`. `{{` gives `{` and `}}`
/// gives `}`. A placeholder that names no label, or that is not one of the four, stays as written.
pub fn expand(template: &str, context: &Context<'_>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(at) = rest.find(['{', '}']) {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        if rest.starts_with("{{") {
            out.push('{');
            rest = &rest[2..];
        } else if rest.starts_with("}}") {
            out.push('}');
            rest = &rest[2..];
        } else if rest.starts_with('{') {
            let Some(end) = rest.find('}') else {
                out.push_str(rest);
                return out;
            };
            let placeholder = &rest[..=end];
            match substitute(&rest[1..end], context) {
                Some(text) => out.push_str(&text),
                None => out.push_str(placeholder),
            }
            rest = &rest[end + 1..];
        } else {
            out.push('}');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

fn substitute(name: &str, context: &Context<'_>) -> Option<String> {
    match name {
        "graph" => Some(context.graph_name.to_owned()),
        "time" => Some(context.time.to_owned()),
        _ => {
            let (field, label) = name.split_once(':')?;
            let node = context.nodes.iter().find(|n| n.label == label)?;
            match field {
                "value" => Some(format_value(node.value)),
                "unit" => Some(node.unit.clone()),
                _ => None,
            }
        }
    }
}

/// The environment of a command response: the Graph, the run, and each labelled node as
/// `NICECOOLING_NODE_<LABEL>` with the label in upper case. The value is decimal text rounded to
/// 8 decimals, with no trailing zero and no exponent, and empty for a node with no value. A node with no label has no variable.
pub fn command_environment(
    graph_id: &str,
    graph_name: &str,
    run_id: &str,
    nodes: &[LabelledValue],
) -> Vec<(String, String)> {
    let mut env = vec![
        ("NICECOOLING_GRAPH_ID".to_owned(), graph_id.to_owned()),
        ("NICECOOLING_GRAPH_NAME".to_owned(), graph_name.to_owned()),
        ("NICECOOLING_RUN_ID".to_owned(), run_id.to_owned()),
    ];
    for node in nodes.iter().filter(|n| !n.label.is_empty()) {
        env.push((
            format!("NICECOOLING_NODE_{}", node.label.to_ascii_uppercase()),
            node.value
                .map(|v| round_to(v, 8, RoundMode::Nearest).to_string())
                .unwrap_or_default(),
        ));
    }
    env
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::eval::EndValue;
    use crate::model::Node;

    fn node(label: &str, value: Option<f64>, unit: &str) -> LabelledValue {
        LabelledValue {
            label: label.to_owned(),
            value,
            unit: unit.to_owned(),
        }
    }

    fn text(template: &str, nodes: &[LabelledValue]) -> String {
        let context = Context {
            graph_name: "Hot water",
            time: "2026-11-25 14:05:09",
            nodes,
        };
        expand(template, &context)
    }

    #[test]
    fn placeholder_value_and_unit() {
        let nodes = [node("water", Some(46.2), "°C")];
        assert_eq!(
            text("Water {value:water} {unit:water}", &nodes),
            "Water 46.2 °C"
        );
        assert_eq!(text("{graph} is hot", &nodes), "Hot water is hot");
        assert_eq!(text("{value:water}{value:water}", &nodes), "46.246.2");
    }

    #[test]
    fn placeholder_no_value_dashes() {
        let nodes = [node("water", None, "°C")];
        assert_eq!(text("{value:water}", &nodes), "--");
        // The unit of a node with no value is still its unit.
        assert_eq!(text("{value:water} {unit:water}", &nodes), "-- °C");
        assert_eq!(format_value(None), "--");
    }

    #[test]
    fn placeholder_unknown_label_kept() {
        let nodes = [node("water", Some(1.0), "")];
        assert_eq!(text("x {value:pump} y", &nodes), "x {value:pump} y");
        assert_eq!(text("{unit:pump}", &nodes), "{unit:pump}");
        // A name that is not a placeholder stays too, and so does an open brace.
        assert_eq!(text("{foo} {value}", &nodes), "{foo} {value}");
        assert_eq!(text("open { here", &nodes), "open { here");
        assert_eq!(text("{value:water", &nodes), "{value:water");
        assert_eq!(text("lone } brace", &nodes), "lone } brace");
        assert_eq!(text("{}", &nodes), "{}");
    }

    #[test]
    fn placeholder_escaped_braces() {
        assert_eq!(text("{{graph}}", &[]), "{graph}");
        assert_eq!(text("{{{graph}}}", &[]), "{Hot water}");
        assert_eq!(text("a {{ b }} c", &[]), "a { b } c");
        assert_eq!(
            text("{{value:x}}", &[node("x", Some(1.0), "")]),
            "{value:x}"
        );
    }

    #[test]
    fn placeholder_two_decimals() {
        assert_eq!(format_value(Some(46.200_000_1)), "46.2");
        assert_eq!(format_value(Some(50.0)), "50");
        assert_eq!(format_value(Some(7.14159)), "7.14");
        assert_eq!(format_value(Some(2.999)), "3");
        assert_eq!(format_value(Some(-0.001)), "0");
        assert_eq!(format_value(Some(-12.5)), "-12.5");
        assert_eq!(format_value(Some(100.0)), "100");
        // A tie goes away from 0.
        assert_eq!(format_value(Some(0.125)), "0.13");
        assert_eq!(format_value(Some(-0.125)), "-0.13");
        assert_eq!(format_value(Some(0.0)), "0");
        let nodes = [node("water", Some(46.200_000_1), "")];
        assert_eq!(text("{value:water}", &nodes), "46.2");
    }

    #[test]
    fn placeholder_time_is_the_given_text() {
        assert_eq!(text("{time}", &[]), "2026-11-25 14:05:09");
        assert_eq!(
            text("At {time}: {graph}", &[]),
            "At 2026-11-25 14:05:09: Hot water"
        );
        let context = Context {
            graph_name: "g",
            time: "any text",
            nodes: &[],
        };
        assert_eq!(expand("{time}", &context), "any text");
    }

    fn graph_with_nodes() -> (Graph, GraphPass) {
        let mut graph = Graph::new("graph_2", "Hot water");
        graph.nodes = vec![
            Node::new(1, Kind::SensorSource)
                .with_param("sensor", "nct_temp1")
                .with_label("water"),
            Node::new(2, Kind::Constant)
                .with_param("value", 50)
                .with_label("limit"),
            Node::new(3, Kind::Constant),
            Node::new(4, Kind::Comment).with_label("note"),
            Node::new(5, Kind::SensorSource).with_label("pump"),
        ];
        let pass = GraphPass {
            graph_id: "graph_2".into(),
            sensor_id: None,
            evaluated: true,
            values: BTreeMap::from([(1, Some(46.2)), (2, Some(50.0)), (3, Some(7.0)), (5, None)]),
            end: EndValue::None,
        };
        (graph, pass)
    }

    #[test]
    fn placeholder_unit_of_non_source_is_empty() {
        let (graph, pass) = graph_with_nodes();
        let nodes = labelled_values(&graph, &pass, |id| {
            if id == "nct_temp1" {
                "°C".into()
            } else {
                String::new()
            }
        });
        // The comment node and the node with no label are not in the list.
        let labels: Vec<_> = nodes.iter().map(|n| n.label.as_str()).collect();
        assert_eq!(labels, ["water", "limit", "pump"]);
        assert_eq!(text("Limit {value:limit}{unit:limit}", &nodes), "Limit 50");
        assert_eq!(text("{value:water} {unit:water}", &nodes), "46.2 °C");
        // A source whose sensor is unknown has an empty unit.
        assert_eq!(text("[{unit:pump}]", &nodes), "[]");
    }

    #[test]
    fn command_environment_uses_upper_case_labels() {
        let nodes = [
            node("water", Some(46.2), "°C"),
            node("Pump_2", Some(50.0), ""),
        ];
        let env = command_environment("graph_2", "Hot water", "run_7", &nodes);
        let get = |name: &str| env.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
        assert_eq!(get("NICECOOLING_GRAPH_ID"), Some("graph_2"));
        assert_eq!(get("NICECOOLING_GRAPH_NAME"), Some("Hot water"));
        assert_eq!(get("NICECOOLING_RUN_ID"), Some("run_7"));
        assert_eq!(get("NICECOOLING_NODE_WATER"), Some("46.2"));
        assert_eq!(get("NICECOOLING_NODE_PUMP_2"), Some("50"));
        assert_eq!(env.len(), 5);
    }

    #[test]
    fn command_environment_rounds_to_8_decimals() {
        let nodes = [
            node("a", Some(0.1 + 0.2), ""),
            node("b", Some(46.123_456_782_6), ""),
            node("c", Some(3_000.000_000_001), ""),
            node("d", Some(1e-9), ""),
        ];
        let env = command_environment("g", "n", "r", &nodes);
        let values: Vec<_> = env[3..].iter().map(|(_, v)| v.as_str()).collect();
        assert_eq!(values, ["0.3", "46.12345678", "3000", "0"]);
    }

    #[test]
    fn command_environment_skips_unlabelled_nodes() {
        // `labelled_values` lists no node without a label, and the environment skips an empty one.
        let (graph, pass) = graph_with_nodes();
        let nodes = labelled_values(&graph, &pass, |_| String::new());
        let env = command_environment("graph_2", "Hot water", "run_1", &nodes);
        let names: Vec<_> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(
            names,
            [
                "NICECOOLING_GRAPH_ID",
                "NICECOOLING_GRAPH_NAME",
                "NICECOOLING_RUN_ID",
                "NICECOOLING_NODE_WATER",
                "NICECOOLING_NODE_LIMIT",
                "NICECOOLING_NODE_PUMP",
            ]
        );
        let env = command_environment("g", "n", "r", &[node("", Some(1.0), "")]);
        assert_eq!(env.len(), 3);
    }

    #[test]
    fn command_environment_empty_for_no_value() {
        let (graph, pass) = graph_with_nodes();
        let nodes = labelled_values(&graph, &pass, |_| String::new());
        let env = command_environment("graph_2", "Hot water", "run_1", &nodes);
        let get = |name: &str| env.iter().find(|(k, _)| k == name).map(|(_, v)| v.clone());
        assert_eq!(get("NICECOOLING_NODE_PUMP"), Some(String::new()));
        assert_eq!(get("NICECOOLING_NODE_WATER"), Some("46.2".to_owned()));
    }
}
