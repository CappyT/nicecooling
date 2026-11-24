# Proposal

## Why

aquasuite lets the user build a sensor or an alarm action from a graph of nodes: sources, arithmetic, logic, timers and filters, ending in a sensor or in an action. nicecooling has three fixed Derived sensor kinds and three fixed alarm responses. The owner wants nicecooling to do what aquasuite does here, and more: run a command, publish to MQTT, send an e-mail, play a sound, reboot or suspend (issue #88).

## What Changes

- A **Graph** is a named set of nodes and connections that the app evaluates once per second in the user session. A Graph ends in one sensor end node or in one response end node. One engine and one editor serve both uses.
- The node kinds are all 49 kinds of aquasuite X.96, plus a source for the state of an Alarm event and a node "has value".
- A source without a value gives no value, and no value goes to every node after it. aquasuite turns a missing input into 0 and a lost source into a fallback number. A Curve must see a lost Input.
- A **Graph sensor** is the result of a Graph with a sensor end node. The app sends its number to the daemon once per second. The daemon treats it as a sensor: the Input of a Curve, the source of a Profile rule, a Software sensor. When the app does not run, the Graph sensor has no value.
- A **Graph response** is what a Graph with a response end node does: a notification, a sound, a command, an e-mail, an MQTT message, a poweroff, a reboot or a suspend. The app runs every kind in the user session. The root daemon runs none.
- Three poweroffs or reboots in a row from Graph responses stop all Graph responses until the user turns them on again.
- The app gets an MQTT client: one broker, the messages of the Alarm events, and the value of each sensor on a fixed topic, retained.
- The app gets an SMTP account for the e-mail response.
- The app gets a Graph editor: a canvas with nodes, pins and connections, a node list by family, the live value of each node, and a way to try a draft. The canvas code starts from the MIT crate `gpui-flow`, copied into the workspace.
- A Graph exports to one file and imports from one file.
- The Derived sensors and the alarm response table of the daemon do not change.

## Capabilities

### New Capabilities

- `node-graph`: the Graph model, the node kinds and their arithmetic, the no-value rule, the evaluation in the app, the Graph sensor and its feed to the daemon, storage, export and import, and the editor.
- `graph-responses`: the response kinds, the timing of a response end node, the stored run state, the response pause, the response log and the SMTP account.
- `mqtt-publishing`: the broker settings, the connection rules, the event messages and the sensor topics.

### Modified Capabilities

None. The capabilities of `add-nicecooling-v1` are not archived yet. This change depends on that change and extends its contracts. `design.md` lists each extended capability.

## Impact

- Code: a new crate `nicecooling-graph` (model, node functions, validator, evaluator), a new crate with the copied canvas code, `nicecooling` (evaluation service, responses, MQTT and SMTP clients, editor), `nicecooling-core` (one shared interpolation function), `nicecoolingd` and `nicecooling-api` (the Graph sensor and its feed).
- API: methods to create, rename, remove and feed a Graph sensor on `io.github.cappyt.NiceCooling1`. No method is removed.
- Dependencies of the app: an MQTT client crate and an SMTP client crate. The daemon gets no new dependency.
- Storage: files of the app in the configuration and data directories of the user; one new key in `state.json` of the daemon.
- Reverse engineering source: the feature study of aquasuite X.96, sections 1 and 3, kept outside the repository.
