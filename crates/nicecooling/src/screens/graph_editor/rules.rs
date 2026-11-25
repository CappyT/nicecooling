//! The text of each refusal code of the validator, and the names that these texts use. The editor,
//! the Graphs screen and the import show these texts, so one rule reads the same in each place.

use std::collections::BTreeSet;

use nicecooling_graph::model::{Graph, Kind, Node, OUTPUT_PIN, ParamType};
use nicecooling_graph::validate::{Refusal, RefusalCode};

use super::number_text;
use crate::i18n::Locale;

/// The name of a kind in the node list.
pub fn kind_name(kind: Kind, locale: Locale) -> String {
    locale.tr(&format!("graph_editor.kind.{}.name", kind.as_str()))
}

/// The name of a node in a text: "End node" for an end node, the name of its kind for another.
pub fn node_name(node: &Node, locale: Locale) -> String {
    if node.kind.is_end() {
        locale.tr("graphs.columns.end")
    } else {
        kind_name(node.kind, locale)
    }
}

/// The name of a parameter in the parameter panel.
pub fn param_name(name: &str, locale: Locale) -> String {
    locale.tr(&format!("graph_editor.param.{name}"))
}

/// The range of a parameter type, as the panel shows it under a field: "0 to 3600 s".
pub fn range_text(ty: ParamType, locale: Locale) -> String {
    let between = |key: &str, min: &str, max: &str| locale.fmt(key, &[("min", min), ("max", max)]);
    match ty {
        ParamType::Number { min, max } if min.is_finite() && max.is_finite() => between(
            "graph_editor.range.number",
            &number_text(locale, min),
            &number_text(locale, max),
        ),
        ParamType::Number { .. } => locale.tr("graph_editor.range.any_number"),
        ParamType::Seconds { min, max } | ParamType::OptionalSeconds { min, max } => between(
            "graph_editor.range.seconds",
            &min.to_string(),
            &max.to_string(),
        ),
        ParamType::Whole { min, max } => between(
            "graph_editor.range.number",
            &min.to_string(),
            &max.to_string(),
        ),
        ParamType::Text { min, max } => between(
            "graph_editor.range.text",
            &min.to_string(),
            &max.to_string(),
        ),
        ParamType::Bytes { max } => {
            locale.fmt("graph_editor.range.bytes", &[("max", &max.to_string())])
        }
        ParamType::Choice(_) => locale.tr("graph_editor.range.choice"),
        ParamType::Pairs => locale.tr("graph_editor.range.pairs"),
        ParamType::Args => locale.tr("graph_editor.range.args"),
        ParamType::AbsolutePath => locale.tr("graph_editor.range.path"),
        ParamType::Email => locale.tr("graph_editor.range.email"),
        ParamType::Topic => locale.tr("graph_editor.range.topic"),
        ParamType::Unit => locale.tr("graph_editor.range.unit"),
    }
}

/// The reason of a refused connection. The status line of the editor shows it after
/// "Not connected: ". A code that is no reason of a connection gives an empty text.
pub fn connection_reason(code: RefusalCode, locale: Locale) -> String {
    match code {
        RefusalCode::PinOccupied => locale.tr("graph_editor.rule.pin_occupied"),
        RefusalCode::SameDirection => locale.tr("graph_editor.rule.same_direction"),
        RefusalCode::SameNode => locale.tr("graph_editor.rule.same_node"),
        RefusalCode::Loop => locale.tr("graph_editor.rule.loop"),
        _ => String::new(),
    }
}

/// The text of one refusal of the validator for `graph`. `others` are the stored Graphs.
pub fn rule_text(refusal: &Refusal, graph: &Graph, others: &[Graph], locale: Locale) -> String {
    let node = refusal.node.and_then(|id| graph.node(id));
    match refusal.code {
        RefusalCode::NoEndNode | RefusalCode::TwoEndNodes => {
            locale.tr("graph_editor.rule.one_end_node")
        }
        RefusalCode::EndNodeOutput => locale.tr("graph_editor.rule.end_node_output"),
        RefusalCode::UnknownKind => locale.fmt(
            "graphs.invalid.unknown_kind",
            &[("kind", node.map_or("", |node| node.kind.as_str()))],
        ),
        RefusalCode::UnknownPin => locale.fmt(
            "graph_editor.rule.unknown_pin",
            &[("pin", &unknown_pin(graph, refusal.node))],
        ),
        RefusalCode::PinOccupied
        | RefusalCode::SameDirection
        | RefusalCode::SameNode
        | RefusalCode::Loop => connection_reason(refusal.code, locale),
        RefusalCode::LoopAcrossGraphs => match loop_through(graph, others) {
            Some(name) => locale.fmt("graph_editor.rule.loop_across_graphs", &[("name", &name)]),
            None => locale.tr("graph_editor.rule.own_sensor"),
        },
        RefusalCode::LimitGraphs => locale.tr("graph_editor.rule.limit_graphs"),
        RefusalCode::LimitNodes => locale.tr("graph_editor.rule.limit_nodes"),
        RefusalCode::LimitName => locale.tr("graph_editor.rule.limit_name"),
        RefusalCode::LimitSamples => locale.tr("graph_editor.rule.limit_samples"),
        RefusalCode::LimitGraphSensors => locale.tr("graph_editor.rule.limit_graph_sensors"),
        RefusalCode::DuplicateLabel => locale.fmt(
            "graph_editor.rule.duplicate_label",
            &[(
                "label",
                node.and_then(|node| node.label.as_deref())
                    .unwrap_or_default(),
            )],
        ),
        RefusalCode::DuplicateTableInput => {
            let input = node
                .and_then(duplicate_input)
                .map(|input| number_text(locale, input))
                .unwrap_or_default();
            locale.fmt(
                "graph_editor.rule.duplicate_table_input",
                &[("input", &input)],
            )
        }
        RefusalCode::BadParameter => parameter_text(refusal, node, locale),
        RefusalCode::BadSensorId => locale.tr("graph_editor.rule.bad_sensor_id"),
    }
}

/// The text of the first refusal, for a place that shows one rule.
pub fn first_rule_text(
    refusals: &[Refusal],
    graph: &Graph,
    others: &[Graph],
    locale: Locale,
) -> Option<String> {
    refusals
        .first()
        .map(|refusal| rule_text(refusal, graph, others, locale))
}

/// "On delay: Delay is 0 to 3600 s".
fn parameter_text(refusal: &Refusal, node: Option<&Node>, locale: Locale) -> String {
    let name = refusal.parameter.as_deref().unwrap_or_default();
    let Some(node) = node.filter(|_| name != "id") else {
        return locale.tr("graph_editor.rule.bad_id");
    };
    let spec = node
        .param_specs()
        .into_iter()
        .find(|spec| spec.name == name);
    let range = match spec {
        _ if name == "label" => locale.tr("graph_editor.range.label"),
        // The validator names the maximum when the two values are in range and in the wrong
        // order.
        Some(spec)
            if name == "max"
                && matches!(node.kind, Kind::Random | Kind::Limiter)
                && spec.ty.accepts(&node.param(name).unwrap_or_default()) =>
        {
            locale.tr("graph_editor.range.max")
        }
        Some(spec) => range_text(spec.ty, locale),
        None => locale.tr("graph_editor.range.choice"),
    };
    locale.fmt(
        "graph_editor.rule.parameter",
        &[
            ("node", &node_name(node, locale)),
            ("name", &param_name(name, locale)),
            ("range", &range),
        ],
    )
}

/// The name of the first pin that a connection uses and its node does not have.
fn unknown_pin(graph: &Graph, node: Option<u32>) -> String {
    let known = |id: u32, pin: &str| {
        graph.node(id).is_some_and(|node| {
            node.kind.input(pin).is_some() || (pin == OUTPUT_PIN && node.kind.has_output())
        })
    };
    graph
        .connections
        .iter()
        .flat_map(|c| [&c.from, &c.to])
        .find(|end| node.is_none_or(|id| end.node == id) && !known(end.node, &end.pin))
        .map(|end| end.pin.clone())
        .unwrap_or_default()
}

/// The input that two pairs of a table node share.
fn duplicate_input(node: &Node) -> Option<f64> {
    let mut inputs: Vec<f64> = node.pairs().iter().map(|pair| pair.0).collect();
    inputs.sort_by(f64::total_cmp);
    inputs
        .windows(2)
        .find(|pair| pair[0] == pair[1])
        .map(|pair| pair[0])
}

/// The Graph of `others` that holds the Graph sensor `sensor`. The stored version of the Graph
/// `own` does not count.
fn holder<'a>(others: &'a [Graph], own: &str, sensor: &str) -> Option<&'a Graph> {
    others
        .iter()
        .find(|graph| graph.id != own && graph.sensor_id.as_deref() == Some(sensor))
}

/// Whether `start` reads the Graph sensor of `graph`, through any number of Graphs.
fn reads_back(start: &Graph, graph: &Graph, others: &[Graph]) -> bool {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut stack = vec![start];
    while let Some(current) = stack.pop() {
        for sensor in current.source_sensors() {
            if graph.sensor_id.as_deref() == Some(sensor.as_str()) {
                return true;
            }
            if let Some(next) = holder(others, &graph.id, &sensor)
                && seen.insert(next.id.as_str())
            {
                stack.push(next);
            }
        }
    }
    false
}

/// The name of the Graph through which `graph` reads its own Graph sensor: the first Graph of
/// the chain. `None` when no other Graph is between.
fn loop_through(graph: &Graph, others: &[Graph]) -> Option<String> {
    graph
        .source_sensors()
        .filter_map(|sensor| holder(others, &graph.id, &sensor))
        .find(|first| reads_back(first, graph, others))
        .map(|first| first.name.clone())
}
