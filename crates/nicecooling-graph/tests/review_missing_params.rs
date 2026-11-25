//! Review reproducers: the validator checks only the parameters that a node stores. A response
//! end node of a stored file that leaves out a parameter gets its default, and an empty default
//! passes: an empty title, an empty sound name, an empty program path, an empty address and an
//! empty topic.

use nicecooling_graph::model::{Connection, Graph, Kind, Node};
use nicecooling_graph::validate::{RefusalCode, validate};
use serde_json::{Map, json};

/// A Graph with a constant into a response end node that stores only `params`.
fn response(params: serde_json::Value) -> Graph {
    let mut end = Node::new(2, Kind::ResponseEnd);
    end.params = params.as_object().cloned().unwrap_or_else(Map::new);
    let mut g = Graph::new("graph_1", "Hot water");
    g.nodes = vec![Node::new(1, Kind::Constant), end];
    g.connections = vec![Connection::new(1, 2, "a")];
    g
}

fn refuses(graph: &Graph, parameter: &str) {
    let refusals = validate(graph, &[]);
    assert!(
        refusals
            .iter()
            .any(|r| r.code == RefusalCode::BadParameter
                && r.parameter.as_deref() == Some(parameter)),
        "{parameter}: {refusals:?}"
    );
}

/// Spec "Notification response", scenario "Empty title".
#[test]
fn notification_without_a_title_is_refused() {
    refuses(&response(json!({})), "title");
    refuses(&response(json!({"kind": "notification"})), "title");
}

/// Spec "Sound response", scenario "Empty sound name".
#[test]
fn sound_without_a_name_is_refused() {
    refuses(&response(json!({"kind": "sound"})), "sound");
}

/// Spec "Command response", scenario "Relative path": the path is absolute.
#[test]
fn command_without_a_path_is_refused() {
    refuses(&response(json!({"kind": "command"})), "path");
}

/// Spec "Parameter types": an e-mail address has one `@`.
#[test]
fn email_without_an_address_is_refused() {
    refuses(&response(json!({"kind": "email"})), "to");
}

/// Spec "MQTT message response": the topic has 1 to 128 characters.
#[test]
fn mqtt_without_a_topic_is_refused() {
    refuses(&response(json!({"kind": "mqtt"})), "topic");
}
