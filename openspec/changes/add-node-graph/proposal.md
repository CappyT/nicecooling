# Proposal

## Why

aquasuite lets the user build a sensor or an alarm action from a graph of nodes: sources, arithmetic, logic, timers and filters, ending in a sensor or in an action. nicecooling has three fixed Derived sensor kinds and three fixed alarm responses. The owner wants nicecooling to do what aquasuite does here, and more: run a command, publish to MQTT, send an e-mail, play a sound, reboot or suspend.

## What Changes

- A **Graph** is a named set of nodes and connections that the daemon evaluates. A Graph ends in one sensor node or in one action node. One engine and one editor serve both uses.
- The node kinds are the 48 kinds of aquasuite X.96: sources, constant, random, process, arithmetic, trigonometry, round, limiter, table, comparisons, logic, switch, memory, persistent storage, trigger, on delay, off delay, window filters, low pass, trend, signal generators, counter, comment, and the two end nodes.
- A source without a value gives no value. No value travels through the Graph: a sensor node then has no value, and an action node does not fire. This differs from aquasuite, which turns a missing input into 0 and a lost source into a fallback number. A Curve must see a lost Input, not a number that the software made up.
- **BREAKING**: a Derived sensor becomes a Graph that ends in a sensor node. The daemon converts the stored Derived sensors (maximum, mean, difference) at the first start. The D-Bus methods of the Derived sensors go away.
- An action node has a kind: desktop notification, sound, command, e-mail, MQTT message, poweroff, reboot, suspend. It has a hold time, an optional repeat interval and a minimum interval between two runs.
- The app runs a command action in the user session. The daemon runs every other kind. A command does not run while no session is open.
- The active state of each Alarm and Warning event is a source, so a Graph can react to an event.
- The daemon gets an MQTT client: one broker (host, port, optional TLS, user name, password, client id, base topic). It publishes the event messages of the action nodes, and the value of each sensor on a fixed topic at an interval, retained.
- The daemon gets an SMTP account for the e-mail action.
- The app gets a Graph editor: a canvas with nodes, pins and connections, a node list by group, the live value of each node, and a test button on an action node. The editor code starts from the MIT crate `gpui-flow`, copied into the workspace.
- A Graph exports to one file and imports from one file. An import gets new identifiers.

## Capabilities

### New Capabilities

- `node-graph`: the Graph model, the node kinds and their arithmetic, the evaluation order and rate, the no-value rule, the cycle check, storage, export and import, the conversion of the Derived sensors, and the editor.
- `graph-actions`: the action kinds, the timing of an action node, which process runs each kind, the outcome records, and the SMTP account.
- `mqtt-publishing`: the broker settings, the connection rules, the event messages, and the sensor topics.

### Modified Capabilities

None. The capabilities of `add-nicecooling-v1` are not archived yet. This change depends on that change and reads its contracts. It replaces the Derived sensor requirements of `fan-control` and extends `control-api` and `desktop-app`; `design.md` lists each replaced requirement.

## Impact

- Code: `nicecooling-core` (the Graph model and the node functions), `nicecoolingd` (the evaluator, the action runner, the MQTT and SMTP clients, the D-Bus objects), `nicecooling-api` (the contract), `nicecooling` (the editor, the command runner, the sound), a new workspace crate with the copied editor code.
- API: new Graph objects on `io.github.cappyt.NiceCooling1`; the Derived sensor methods are removed.
- Dependencies: an MQTT client crate and an SMTP client crate in the daemon. The editor code is a copy of `gpui-flow` (MIT) with its license file.
- Storage: one file for the Graphs and one for the stored values of the memory nodes in the daemon state directory; the broker and SMTP passwords in a file that only root reads.
- The alarm response table (none, notify, poweroff) stays as it is: the poweroff path of an Alarm does not depend on a Graph.
- Reverse engineering source: the feature study of aquasuite X.96, sections 1 and 3 (kept outside the repository).
