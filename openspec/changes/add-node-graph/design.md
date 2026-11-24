# Design

## Context

This design belongs to the change of `proposal.md`. `GLOSSARY.md` defines the project terms. The specs of this change hold the numbers and the formulas: `specs/node-graph/spec.md`, `specs/graph-responses/spec.md` and `specs/mqtt-publishing/spec.md`. This document gives the decisions and their reasons.

`add-nicecooling-v1` gives the project two processes (its D1). The root daemon `nicecoolingd` owns the hardware, the control tick, the Alarms and the alarm response. The desktop app runs in the user session, starts at login without a window, and owns the notifications and the history (ADR-0005).

aquasuite X.96, the vendor program that nicecooling replaces, has one engine for two features: a graph of nodes that ends in a sensor or in an action. Its system service evaluates the graphs with system rights.

The owner decisions that bind this design are in issue #88:

- One Graph engine and one editor serve sensors and responses.
- All node kinds of aquasuite are in the first version, plus a node "has value".
- A source without a value gives no value. No value goes to every node after it.
- Graphs, their responses and the network clients run in the app. Nothing of this runs in the root daemon.
- The app sends the number of a Graph sensor to the daemon, so a Curve can use it.
- The Derived sensors of the daemon stay as they are.
- Three poweroffs or reboots in a row that a Graph caused stop the Graph responses.
- The app warns and does not refuse a valid choice of the user.
- The change reuses existing code before it adds new code.

## Goals / Non-Goals

**Goals:**

- A user builds a sensor or a response from nodes and sees the live value of each node.
- A Graph sensor is usable where a measured sensor is: as the Input of a Curve, as the source of a Profile rule, as a Software sensor, in the history and in another Graph.
- A Curve never gets a substitute value for a missing source.
- The root daemon gets no user logic, no command runner and no network client.
- A wrong Graph cannot stop the control tick.

**Non-Goals:**

- Graphs that run without a user session. When the app does not run, a Graph sensor has no value and no Graph response runs. The alarm response of the daemon covers the case that needs no session.
- The aquasuite file formats. The app does not import an aquasuite graph.
- The aquasuite rules "a missing input is 0" and "a lost source gives a fallback number".
- Hotkeys, input controls (button, slider, dial), the web export to the vendor service.
- Values received from MQTT. Home Assistant discovery messages.
- A defense against another local user (a Non-Goal of `add-nicecooling-v1`).
- A refusal of a choice that is unsafe and valid: a login without encryption gets a warning and works.
- A change of the alarm response table. No Graph can remove or delay the poweroff of an Alarm.

## Terms

`GLOSSARY.md` gets these entries with this change. The specs and the interface use them.

| Term | Meaning |
|---|---|
| Graph | A named set of nodes and connections that the app evaluates once per second. |
| Node, pin, connection | A node is one step of a Graph. It has input pins and at most one output pin. A connection joins an output pin to an input pin. |
| End node | The node where a Graph ends: a sensor end node or a response end node. |
| Graph sensor | The sensor that a Graph with a sensor end node gives. The app computes it and sends its value to the daemon. |
| Graph response | What a Graph with a response end node does when its input becomes true: a notification, a command, a message, a power call. |
| Response pause | The state in which no Graph response runs, after three power responses in a row. |

The glossary keeps Action for a Leakshield procedure, and it already avoids "alarm action" and "device action" in favor of Alarm response and Device response. "Graph response" follows that pattern. The editor button that runs a draft is "Try", because "Test" is a Leakshield Action. The entry Input changes to "a measured sensor, a Derived sensor or a Graph sensor".

## Decisions

### G1. The Graph engine is in the app (node-graph)

A new workspace crate `nicecooling-graph` holds the model, the node functions, the validator and the evaluator. It has no I/O. The app depends on it. The daemon does not.

The app evaluates every enabled Graph once per second in a service thread, next to the history writer and the notification service. It takes the source values from the sensor events of the daemon, which arrive once per second.

Rejected: evaluation in the daemon, as aquasuite does. A root process would run logic, commands and network clients that a user wrote. The owner rejects this as a privilege escalation risk (issue #88, decision 5). It would also put 51 node kinds in front of the watchdog ping of the control tick.

Rejected: an unprivileged helper service for the evaluation. It runs without a session, but it adds a third process, an IPC protocol and a second owner of notifications, against ADR-0005.

Cost of this decision: no Graph runs before the login or after the app stops. G4 and G9 state what happens then.

### G2. Model (node-graph)

A Graph has an id, a name, an enabled flag, nodes and connections. A node has an id, a kind, an optional label, parameters and a canvas position. An input pin has at most one connection. An output pin has any number of connections.

A Graph has exactly one end node, and an end node has no output pin. The validator refuses a Graph with no end node or with more than one, because such a Graph has no result.

Limits: 64 Graphs, 256 nodes per Graph, 200 000 window samples for all Graphs together, a name of 64 characters. aquasuite has no limits. The sample limit bounds the memory to about 5 MB and the cost of one evaluation. The spec gives the calculation.

One validator in `nicecooling-graph` serves the editor and the load of the stored file.

Rejected: keep the Derived sensor list and add functions to it. The response use needs the Graph, and two models cost more than one. The three Derived kinds stay in the daemon because the Curves of a machine without a session depend on them.

### G3. Values and the no-value rule (node-graph)

A pin carries a number or no value. A logic output is 1 or 0. A logic input is true above 0.

- A sensor source node gives no value while its sensor has no value.
- A node gives no value when a connected input has no value. The node "has value" is the one exception: it gives 1 when its input has a value and 0 when it has none.
- An input pin with no connection takes the default of its kind. The spec lists the defaults. The input of an end node has no default: a sensor end node with no connection gives no value, and a response end node with no connection is false.
- A division by 0, a function outside its domain and a result that is not finite give no value.
- A response end node counts no value as false.

Reason: a Curve that gets no value follows the Lost Input rule of `add-nicecooling-v1` (D7): its Channel goes to 100% and the daemon raises a Warning. A substitute number would hide the loss.

Rejected: hold the last value for a set time, and the aquasuite fallback value. Both give a Curve a number that no sensor measured. The node "has value" gives the user the signal that these rules were for, as a logic value that no Curve can take for a temperature.

The sensor end node has no "keep the value over a restart" option. aquasuite has one. It would publish a value from before the restart while the sources have no value, which is the case that this rule excludes. The persistent storage node stays: it is an explicit node of the Graph, and it gives no value while its inputs have none.

### G4. Graph sensors reach the daemon as numbers (node-graph)

The daemon gets a new kind of sensor, the Graph sensor: an id, a name and a unit, created and removed through the D-Bus API, stored in `state.json`. The app sends the values of all Graph sensors in one call per second. The daemon puts them in the value table.

- A value older than 3 s counts as no value, as for every sensor (`add-nicecooling-v1` D6). So a Graph sensor has no value when the app does not run, stops or hangs.
- The daemon checks only that a value is a finite number. It runs no Graph code.
- A Graph sensor is a normal entry of the value table: a Curve, a Profile rule, a Software sensor slot and the Feed can use it. The delete of a Graph sensor that something uses gives `InUse`, as for a Derived sensor.
- At most 64 Graph sensors exist.

Reason for the numbers-only interface: the daemon keeps one small, typed entry point, and the failure of the app is the same event as the loss of a hwmon sensor.

Rejected: Graph sensors that only the app sees. The owner wants a formula as the Input of a Curve (issue #88, decision 6).

Consequence that the editor must show: a Curve on a Graph sensor runs at 100% before the login and while the app is closed. The Curve editor shows this text when the user picks a Graph sensor as Input.

### G5. Node kinds (node-graph)

The model has 51 kinds: the 49 kinds of aquasuite X.96, the source "alarm event state" and the node "has value". The spec has one requirement per family with the formula of each kind.

- The alarm event state source gives 1 while a chosen Alarm or Warning of the daemon is active, else 0. aquasuite reads device alarms as device data. nicecooling has its own event list.
- The comparisons use the `Comparison` type of the Profile rules, with its tolerance of 1e-9. A Profile rule and a Graph that compare the same sensor then agree. aquasuite compares exactly.
- The on delay, the off delay and the hold time of a response use the hold logic of the sensor Profile rule: a false input or no value resets the time.
- The table node uses one interpolation function that this change extracts in `nicecooling-core`: linear or step, with the first and the last output outside the pairs. The Calibration table keeps its own boundary rule and its results do not change.
- A time-window filter removes every sample that is older than its span, each second. aquasuite removes at most one sample per cycle, which keeps old samples after a gap. While its input has no value, a filter gives no value and adds no sample, and its samples still age.
- The process source uses the process scan of the app. The Profile rules of the daemon keep their own scan.

Rejected: a smaller first set of kinds. The owner chose all kinds (issue #88, decision 2). Each kind is a function of its inputs and its stored state, so its tests are tables.

### G6. Time (node-graph, graph-responses)

Every delay, window and interval of a Graph counts seconds of the monotonic clock of the app. The tests replace the clock.

- The monotonic clock does not count the time of a suspend. A delay or a window continues after a resume as if no time passed.
- The minimum interval between two runs of a response and the response pause use the wall clock, because they must count over a restart (G8).

### G7. Fault isolation (node-graph)

The app evaluates each Graph inside a panic boundary. A Graph whose evaluation panics is disabled and stored as disabled, and the app shows a notification that names it. Its Graph sensor then has no value.

The control tick of the daemon does not depend on the evaluation: a slow or stopped app only makes the Graph sensors stale.

### G8. Graph responses (graph-responses)

A response end node has a kind, a hold time, an optional repeat interval and a minimum interval. The spec has the state machine. It follows aquasuite: the response runs when the input stays true for the hold time, it can repeat while the input is true, and it has no run on the falling edge.

| Kind | Parameters | How the app runs it |
|---|---|---|
| Notification | title, text, urgency | the notification service of the app |
| Sound | a sound name of the freedesktop sound theme | a notification with the sound hint |
| Command | program path, arguments, time limit | a child process of the app, killed at the time limit |
| E-mail | recipient, subject, text | the SMTP client of the app (G10) |
| MQTT message | topic, payload | the MQTT client of the app (G9) |
| Poweroff, reboot, suspend | countdown | a logind call of the session, after a countdown with cancel |

Rules that the review of this design added:

- Stored run state: the app stores the state and the last run time of each response end node, with the wall clock. After a restart of the app, a response whose input stayed true does not run again, and the minimum interval counts over a reboot.
- Start delay: for 10 s after its start the app evaluates the Graphs and runs no response. An input that is true at the end of the delay counts as a rising edge then.
- Power kinds: their default minimum interval is 600 s. The other kinds default to 0 s.
- Response pause: the app counts the poweroff and reboot responses that it runs. The count goes back to 0 when the app runs for 10 minutes with no such run. At 3 the app enters the response pause: no Graph response runs, the Graph sensors still work, and the app shows a banner with "Turn the Graph responses on". This is the maintenance mode of aquasuite (issue #88, decision 10).
- The alarm poweroff wins: while the daemon reports a poweroff countdown of an Alarm, the app starts no power response and cancels its own countdown. A Graph cannot delay the poweroff of an Alarm.
- One app runs the responses: only the app of the active session runs Graph responses. The app reads this from logind. A second instance in another session runs none.
- Text: the text of a notification, an e-mail and an MQTT payload can hold placeholders for the Graph name and for the value and unit of a labelled node. The spec gives the syntax.
- Log: each run adds a row to a response log of the app: time, Graph, kind, outcome. The editor shows the newest rows of its Graph.

Rejected: the daemon runs a command as the user who wrote the Graph. A root process would start programs from user data.

Rejected: no response pause. The stored minimum interval alone does not protect a user who sets it to 0.

The power kinds call logind with the rights of the session. logind can refuse, for example when another user is logged in. The outcome is then `failed`, and the app does not ask for a password.

### G9. MQTT (mqtt-publishing)

The app holds one MQTT client with the settings of issue #88, decision 8. MQTT is off until a host is set.

| Topic | Content | QoS | Retained |
|---|---|---|---|
| `<base>/status` | `online`, and `offline` as the last will | 1 | yes |
| `<base>/sensor/<sensor id>` | value, unit, name and time of each sensor that is not hidden | 0 | yes |
| `<base>/event/<event id>` | one message per raise and per clear of an Alarm or Warning | 1 | no |
| the topic of an MQTT response | the payload of the response | 1 | no |

- The app publishes while it runs. After the app stops, the broker publishes `offline`, and a consumer knows that the retained values are old.
- A sensor without a value publishes `null`.
- The client keeps one pending value per sensor topic, where a new value replaces the old one, and a queue of 256 for events and responses. A broker that is not reachable never blocks the evaluation.
- With TLS off and a password set, the settings show a warning.
- "Hidden" is the hidden-sensor setting of the app, which the app already has.

Client crate: `rumqttc` with `rustls`. The workspace builds `rustls` today and does not build `rumqttc`. The first MQTT task is a spike that proves the pending-slot rule and the mapping of a broker acknowledgment to a response run with this crate.

Rejected: the client in the daemon or in a helper service (G1). Rejected: publish only response messages, as aquasuite does. A home automation system needs the values, and the owner chose them (issue #88, decision 8).

### G10. E-mail (graph-responses)

The app holds one SMTP account: host, port, security (STARTTLS, TLS or none), user name, password and sender address. A send has a time limit of 30 s and runs outside the evaluation. Client crate: `lettre` with `rustls`, new in the workspace.

Rejected: a call of a local `sendmail`. A desktop has no configured mail transfer agent as a rule.

### G11. Storage (node-graph, graph-responses, mqtt-publishing)

| File | Owner | Content |
|---|---|---|
| `~/.config/nicecooling/graphs.json` | app | all Graphs |
| `~/.local/share/nicecooling/graph-state.json` | app | the values of the persistent storage nodes, the run state of the responses, the response pause counter |
| `~/.local/share/nicecooling/response-log.json` | app | the newest 500 response runs |
| `~/.config/nicecooling/secrets.toml`, mode 0600 | app | the MQTT and SMTP passwords |
| `state.json`, key `graph_sensors` | daemon | id, name and unit of each Graph sensor |

The app writes `graph-state.json` at most every 30 s, at its stop, and before a power call. The timed writer of the history fits this use.

Rejected: the Secret Service of the desktop for the passwords. The app starts at login without a window, and a locked keyring would then stop MQTT with a dialog. A file that only the user reads is what mail and MQTT tools on a desktop use.

### G12. Export and import (node-graph)

The app writes one Graph as one JSON file. The app reads a file, gives the Graph and its nodes new ids, and adds it as a disabled Graph, so a response from another machine does not run before the user sees it. A sensor source that this machine does not have stays as an unbound source with no value, and the editor marks it. The daemon has no part in this.

### G13. Editor (node-graph)

A new workspace crate holds a copy of the node canvas `gpui-flow` (MIT, https://github.com/pacifio/gpui-flow, commit `cfa5787`), with an entry in `THIRD_PARTY.md`. The upstream crate depends on GPUI from a git repository, and the app uses the pinned `gpui-pre` release, so the two cannot be in one dependency tree. The copy changes only its GPUI dependency line to compile against `gpui-pre`. This was checked on 2026-11-24. Its on-screen behavior is not checked.

- The first task of the editor is a spike: the copy shows a Graph in the app with drag, connect, pan and zoom, in light and dark. The owner sees it before more editor work starts.
- If the spike fails, the fallback is a canvas written on the drawing primitives that the Loop diagram of the app already uses. The tasks name this branch.
- A screen "Graphs" in the group "Control" lists the Graphs with name, kind of end node, enabled state and live result.
- The editor holds a draft. Apply validates it and stores it. The live value of each node comes from the evaluator of the app: a node of the applied Graph shows its value, and a node that exists only in the draft shows none until Apply.
- "Try" evaluates the draft in the app with test values for its sources and shows the node values. A try runs no response. A response end node has its own button "Run once", which runs the response one time and ignores the minimum interval.

Rejected: a git dependency on the upstream crate (not possible, see above). Rejected: write the canvas first. The copy saves about 2800 lines when the spike passes.

### G14. Daemon API and authorization (node-graph)

New methods on `io.github.cappyt.NiceCooling1`: create, rename and remove a Graph sensor, and feed the values. All are in the polkit action `write`, which gives a member of `wheel` in an active session access without a password. The daemon caches the authorization of a bus sender for the feed call, so one call per second does not ask polkit each time.

The daemon sends no request for a response to the app, and the app accepts none from the bus.

## Requirements of add-nicecooling-v1 that this change extends

`add-nicecooling-v1` is not archived, so this change has no delta against `openspec/specs/`. It extends these contracts, and its tasks edit them:

| Capability | Change |
|---|---|
| `fan-control` | An Input can be a Graph sensor. The Lost Input rule covers a Graph sensor that the app does not feed. |
| `hardware-discovery` | The sensor list has a third origin: Graph sensor, next to measured and Derived. |
| `profiles` | The source of a sensor Profile rule can be a Graph sensor. |
| `leakshield-operation` | A Software sensor slot can take a Graph sensor. |
| `control-api` | The Graph sensor methods, the `graph_sensors` key of `state.json`. No method is removed. |
| `desktop-app` | The Graphs screen, the editor, the MQTT and SMTP settings, the response pause banner, the text in the Curve editor for a Graph sensor Input. |

Files of the contract that change: `crates/nicecooling-api/dbus/io.github.cappyt.NiceCooling1.xml`, `docs/api.md`, `docs/storage.md`, `docs/app.md`, `GLOSSARY.md`, `THIRD_PARTY.md`, and a new `docs/graphs.md`.

## Migration and rollback

This change converts no stored data. The Derived sensors stay.

- Forward: the daemon adds the key `graph_sensors` to `state.json`. The app creates its files at the first use.
- Back: an older daemon follows the schema rule of `docs/storage.md` for a file that it does not know. The tasks check which case applies to a new key and state the downgrade steps in `docs/graphs.md`. A Curve whose Input was a Graph sensor has a Lost Input on an older daemon and runs at 100%.

## Risks / Trade-offs

- [No Graph runs without a session] -> a Curve on a Graph sensor runs at 100% then, with a Warning. The Curve editor says so. The alarm response of the daemon needs no Graph.
- [A wrong Graph gives a valid, wrong number to a Curve] -> nicecooling does not detect this. The Pump floor and the overrides of the control tick still apply.
- [A power response in a loop] -> the stored minimum interval with its 600 s default, the countdown with cancel, the 10 s start delay and the response pause.
- [A command response runs a program with the rights of the user] -> it is the program that the user chose, in the session of the user. No root process starts it.
- [The app parses data from the network (MQTT, SMTP, TLS)] -> the app has the rights of the user and no access to the devices. The daemon takes only numbers from it.
- [The app can feed a wrong value on purpose] -> the feed is in the `write` action, which already allows a direct write of a Fixed Duty.
- [The copied canvas code is 2800 lines of one author with no tests] -> the spike decides before more work depends on it, and the fallback is named.
- [51 node kinds] -> one table test per kind, and one validator and one evaluator for all.
- [`rumqttc` keeps its own queue and replays requests after a reconnect] -> the MQTT spike decides if the pending-slot rule holds. If not, the spec changes before the client is built.

## Open Questions

- The result of the editor spike (G13).
- The result of the MQTT spike (G9).
