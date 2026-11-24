# Design

## Context

This design belongs to the change of `proposal.md`. `GLOSSARY.md` defines the project terms. The specs of this change hold the numbers and the formulas: `specs/node-graph/spec.md`, `specs/graph-responses/spec.md` and `specs/mqtt-publishing/spec.md`. This document gives the decisions and their reasons.

`add-nicecooling-v1` gives the project two processes (its D1). The root daemon `nicecoolingd` owns the hardware, the control tick, the Alarms and the alarm response. The desktop app runs in the user session, starts at login without a window, and owns the notifications and the history (ADR-0005).

aquasuite X.96, the vendor program that nicecooling replaces, has one engine for two features: a graph of nodes that ends in a sensor or in an action. Its system service evaluates the graphs with system rights.

The owner decisions that bind this design are in issue #88. This document cites them by their number there:

1. One Graph engine and one editor serve sensors and responses, as in aquasuite. Existing code is reused before new code is written.
2. All node kinds of aquasuite are in the first version.
3. A source without a value gives no value. No value goes to every node after it.
4. A node "has value" exists.
5. Graphs, their responses and the network clients run in the app. Nothing of this runs in the root daemon.
6. The app sends the number of a Graph sensor to the daemon, so a Curve can use it.
7. The Derived sensors of the daemon stay as they are.
8. MQTT publishes the Alarm events and the sensor values.
9. The response kinds are notification, sound, command, e-mail, MQTT message, poweroff, reboot and suspend.
10. Three poweroffs or reboots in a row that a Graph caused stop the Graph responses.
11. The alarm response table of the daemon does not change.

The owner rule of `add-nicecooling-v1` also holds: the app warns and does not refuse a valid choice of the user.

## Goals / Non-Goals

**Goals:**

- A user builds a sensor or a response from nodes and sees the live value of each node.
- A Graph sensor is usable as the Input of a Curve, as the source of a Profile rule, as a Software sensor, in the history, in MQTT and in another Graph.
- A user keeps a copy of a Graph and moves it to another machine: export and import, as aquasuite has.
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
| Response pause | The state in which no Graph response runs, after three poweroff or reboot responses in a row. |
| Graph sensor values | The numbers of the Graph sensors that the app sends to the daemon once per second. The glossary keeps Feed for the Leakshield. |

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

One validator in `nicecooling-graph` serves the editor and the load of the stored file. The app parses the stored file one Graph at a time: a Graph that does not parse or does not pass the validator stays in the list as invalid and does not run, and the other Graphs load.

The table of the units of a Graph sensor with the kind of each, and the limit of 64 Graph sensors, are in `nicecooling-core`, which the daemon and the app use. They exist once.

Rejected: keep the Derived sensor list and add functions to it. The response use needs the Graph, and two models cost more than one. The three Derived kinds stay in the daemon because the Curves of a machine without a session depend on them.

### G3. Values and the no-value rule (node-graph)

A pin carries a number or no value. A logic output is 1 or 0. A logic input is true above 0.

- A sensor source node gives no value while its sensor has no value.
- A node gives no value when a connected input has no value. The node "has value" is the one exception: it gives 1 when its input has a value and 0 when it has none.
- An input pin with no connection takes the default of its kind. The spec lists the defaults. The input of an end node has no default: a sensor end node with no connection gives no value, and a response end node with no connection is false.
- A division by 0, a function outside its domain and a result that is not finite give no value.
- A response end node holds its state while its input has no value. Only the hold time starts again. A false input clears the state.

Reason: a Curve that gets no value follows the Lost Input rule of `add-nicecooling-v1` (D7): its Channel goes to 100% and the daemon raises a Warning. A substitute number would hide the loss.

Rejected: hold the last value for a set time, and the aquasuite fallback value. Both give a Curve a number that no sensor measured. The node "has value" gives the user the signal that these rules were for, as a logic value that no Curve can take for a temperature.

The sensor end node has no "keep the value over a restart" option. aquasuite has one. It would publish a value from before the restart while the sources have no value, which is the case that this rule excludes. The persistent storage node stays: it is an explicit node of the Graph, and it gives no value while its inputs have none.

### G4. Graph sensors reach the daemon as numbers (node-graph)

The daemon gets a new kind of sensor, the Graph sensor: an id, a name and a unit, created and removed through the D-Bus API, stored in `state.json`. The app sends the values of all Graph sensors in one call per second. The daemon puts them in the value table.

- A value older than 3 s counts as no value, as for every sensor (`add-nicecooling-v1` D6). So a Graph sensor has no value when the app does not run, stops or hangs.
- The daemon checks that a value is a finite number and that its id is the id of a Graph sensor. It skips an entry for any other id, so the app cannot write the value of a measured sensor. It runs no Graph code.
- The daemon accepts a Graph sensor as the Input of a Curve, as the source of a sensor Profile rule and in a Software sensor slot. It refuses a Graph sensor as the source of a Derived sensor and as the flow or water temperature sensor of a Loop.
- The delete of a Graph sensor that a Curve, a Profile rule or a Software sensor slot uses gives `InUse`.
- At most 64 Graph sensors exist.

Reason for the numbers-only interface: the daemon keeps one small, typed entry point, and the failure of the app is the same event as the loss of a hwmon sensor.

Reason for the two refusals: a Derived sensor on a Graph sensor could close a loop through a Graph that reads the Derived sensor, and neither validator sees both sides. A Loop sensor goes into the Feed message, from which the Leakshield computes its pressure limit, and that path stays on measured values.

Rejected: Graph sensors that only the app sees. The owner wants a formula as the Input of a Curve (issue #88, decision 6).

Consequence that the editor must show: a Curve on a Graph sensor runs at 100% before the login and while the app is closed. The Curve editor shows this text when the user picks a Graph sensor as Input.

### G5. Node kinds (node-graph)

The model has 51 kinds: the 49 kinds of aquasuite X.96, the source "alarm event state" and the node "has value". The spec has one requirement per family with the formula of each kind.

- The alarm event state source gives 1 while a chosen Alarm or Warning of the daemon is active, else 0. aquasuite reads device alarms as device data. nicecooling has its own event list.
- The comparisons use the compare function of the sensor Profile rule, with its tolerance of 1e-9. This change makes that function public in `nicecooling-core`. A Profile rule and a Graph that compare the same sensor then agree. aquasuite compares exactly.
- The on delay and the hold time of a response use the hold logic of the sensor Profile rule, which this change makes usable without a rule: a false input or no value starts the time again. The off delay is new code with the same reset rule.
- The table node, the Curve and the Curve editor of the app use one interpolation function that this change extracts from the Curve in `nicecooling-core`: linear or step, with the first and the last output outside the pairs. The shared function gives no result for an input that is not a number and for an empty list of pairs. The Curve turns no result into its fail-safe Duty, as it does now. The Calibration table is not touched: it extrapolates, and it is on the path of the Feed.
- A time-window filter removes every sample that is older than its span, each second. aquasuite removes at most one sample per cycle, which keeps old samples after a gap. While its input has no value, a filter gives no value and adds no sample, and its samples still age.
- The process source uses the `/proc` scan of the Profile rules, which this change moves from the daemon to `nicecooling-core`. The app runs it as the user: it reads the name of every process and the program path only of the processes of the user. A process source can so differ from a process Profile rule for a process of another user, and `docs/graphs.md` says this.

Rejected: a smaller first set of kinds. The owner chose all kinds (issue #88, decision 2). Each kind is a function of its inputs and its stored state, so its tests are tables.

### G6. Time (node-graph, graph-responses)

A delay, a window, a hold time and a repeat interval count seconds of the monotonic clock of the app. `nicecooling-core` has the clock trait and a simulated clock. This change adds a system clock on the monotonic clock of the operating system and a wall-clock trait with a system and a simulated form. The tests replace both.

- The monotonic clock does not count the time of a suspend. A delay or a window continues after a resume as if no time passed, so a window holds samples from before the suspend for up to its span. The timers of the daemon behave the same way.
- The minimum interval between two runs of a response uses the wall clock, because it must count over a reboot.
- The Response pause counts run time of the app and stores it, so it adds up over restarts (G8).

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

More rules:

- Stored run state: the app stores the state and the last run time of each response end node, with the wall clock. After a restart of the app, a response whose input stayed true does not run again, and the minimum interval counts over a reboot. With the rule of G3 for no value, a restart of the daemon or a lost connection does not run a response again.
- After a start with the stored state "run", the repeat interval counts from the start of the app.
- Power kinds: their default minimum interval is 600 s. The other kinds default to 0 s.
- Response pause: the app counts the poweroff and reboot responses that it runs, "Run once" included. The count goes back to 0 after 60 minutes of app run time with no such run. At 3 the app enters the Response pause: no Graph response runs by itself, the Graph sensors still work, and the app shows a banner with "Turn the Graph responses on". This is the maintenance mode of aquasuite (decision 10). The 60 minutes are longer than the default minimum interval, so three runs at the defaults do start the pause.
- The alarm poweroff wins: while the daemon reports a poweroff countdown of an Alarm, the app starts no power response, "Run once" included, and cancels its own countdown. A Graph cannot delay the poweroff of an Alarm.
- One app runs the responses: only the app of the active session runs Graph responses. The app reads this from logind. A second instance in another session runs none.
- Text: the text of a notification, an e-mail and an MQTT payload can hold placeholders for the Graph name and for the value and unit of a labelled node. The spec gives the syntax.
- Log: each run adds a row to a response log of the app: time, Graph, kind, outcome. The editor shows the newest rows of its Graph.

Rejected: a delay of 10 s after the start of the app in which no response runs. The stored run state and the rule for no value already keep a response from a second run at a start.

Rejected: the daemon runs a command as the user who wrote the Graph. A root process would start programs from user data.

Rejected: no Response pause. The stored minimum interval alone does not protect a user who sets it to 0.

The power kinds call logind with the rights of the session. logind can refuse, for example when another user is logged in. The outcome is then `failed`, and the app does not ask for a password.

### G9. MQTT (mqtt-publishing)

The app holds one MQTT client for one broker. The spec lists its settings. MQTT is off until a host is set.

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
| `~/.local/share/nicecooling/graph-state.json` | app | the values of the persistent storage nodes, the run state of the responses, the count and the run time of the Response pause |
| `~/.local/share/nicecooling/response-log.json` | app | the newest 500 response runs |
| `~/.config/nicecooling/secrets.toml`, mode 0600 | app | the MQTT and SMTP passwords |
| `state.json`, key `graph_sensors` | daemon | id, name and unit of each Graph sensor |

The app writes `graph-state.json` at most every 30 s, at its stop, and before a power call. It uses the atomic write of the app settings. Every file has a `schema_version`, as `docs/storage.md` asks. `docs/storage.md` gets the format of each file before the code that writes it.

Rejected: the Secret Service of the desktop for the passwords. The app starts at login without a window, and a locked keyring would then stop MQTT with a dialog. A file that only the user reads is what mail and MQTT tools on a desktop use.

### G12. Export and import (node-graph)

aquasuite exports and imports one graph per file, and this change keeps that function (decision 1). The app writes one Graph as one JSON file. The app reads a file, gives the Graph and its nodes new ids, and adds it as a disabled Graph, so a response from another machine does not run before the user sees it. A sensor source that this machine does not have stays as an unbound source with no value, and the editor marks it. The daemon has no part in this.

### G13. Editor (node-graph)

A new workspace crate holds a copy of the node canvas `gpui-flow` (MIT, https://github.com/pacifio/gpui-flow, commit `cfa5787`), with an entry in `THIRD_PARTY.md`. The upstream crate depends on GPUI from a git repository, and the app uses the pinned `gpui-pre` release, so the two cannot be in one dependency tree. The copy compiles against `gpui-pre` with a change of its GPUI dependency line only. Its on-screen behavior is not checked. The editor work then changes the copy: the theme tokens, the keyboard rules, and what the lints of the workspace need. `THIRD_PARTY.md` names the upstream commit, and the history of the crate shows the changes.

- The first task of the editor is a spike: the copy shows a Graph in the app with drag, connect, pan and zoom, in light and dark. The owner sees it before more editor work starts.
- If the spike fails, the editor work stops. A canvas written on the drawing primitives of the Loop diagram is the fallback, and it gets its own tasks then, with the owner.
- The owner approves the screen files `docs/ui/screens/graphs.md` and `graph-editor.md` before the screens are built, as for the screens of `add-nicecooling-v1`.
- A screen "Graphs" in the group "Control" lists the Graphs with name, kind of end node, enabled state and live result.
- The editor holds a draft. Apply validates it and stores it. The live value of each node comes from the evaluator of the app: a node of the applied Graph shows its value, and a node that exists only in the draft shows none until Apply.
- "Try" evaluates the draft in the app with test values for its sources and shows the node values. A try runs no response. A response end node has its own button "Run once", which runs the response one time and ignores the minimum interval and the Response pause. A power response from "Run once" counts for the pause.

Rejected: a git dependency on the upstream crate (not possible, see above). Rejected: write the canvas first. The copy saves about 2800 lines when the spike passes.

### G14. Daemon API and authorization (node-graph)

The Graph sensor follows the pattern of the Derived sensor in `io.github.cappyt.NiceCooling1`:

| Member | Signature | Meaning |
|---|---|---|
| `Manager.CreateGraphSensor` | `(s name, s unit) -> o` | Creates a Graph sensor and returns its object path. |
| `GraphSensor.SetDefinition` | `(s name, s unit)` | Changes the name and the unit. |
| `GraphSensor.Delete` | `()` | Removes it. `InUse` when something uses it. |
| `Manager.SetGraphSensorValues` | `(a(sbd) values)` | One entry per Graph sensor: id, has a value, the value. |

- A Graph sensor is a Sensor object with the origin `graph`. Its `Kind` comes from its unit, as for a Derived sensor. The unit is one of the units that the daemon already knows.
- An entry with "has a value" false makes the sensor lose its value at once, so a Curve sees the loss in the next tick and not after 3 s.
- A call with a number that is not finite fails as a whole with `InvalidArgument`. An entry whose id is not the id of a Graph sensor is skipped: an unknown id, and the id of a measured, Derived or calibrated sensor.
- `SetDefinition` writes the default name. A name that the user set with `Sensor.SetName` still shows, as for a Derived sensor. A change of the unit fails with `InUse` while a Software sensor slot uses the sensor, because the slot holds the unit code of the device. After `SetDefinition` the daemon removes the object and adds it again, so a client reads the new name, unit and kind: these properties stay constant for the life of an object.
- The delete of any sensor that a Software sensor slot uses gives `InUse`. This also holds for a Derived sensor and for a calibrated flow, which the daemon let go before. `docs/api.md` states it.
- The ids are `graph_<n>`. A stored counter gives the next number, so the id of a removed Graph sensor is not used again.
- All four members are in the polkit action `write`, which gives a member of `wheel` in an active session access without a password. The daemon asks polkit at every call. The app calls `SetGraphSensorValues` on its own call path without the interactive flag, so a user outside `wheel` gets no dialog each second: the call fails, the app stops for 60 s and shows that it has no access. The other writes of the app keep the flag. The check with a real user outside `wheel` is a manual check, because the polkit mock of the tests knows no users.

Rejected: a cache of the authorization of a bus sender. One polkit check per second has no measured cost, and a cache keeps an authorization after a user switch.

The daemon sends no request for a response to the app, and the app accepts none from the bus.

### G15. The Graph service and its links in the app (node-graph, graph-responses, mqtt-publishing)

The app has service threads for the tray, the notifications and the history. Each one reads the daemon events and keeps its own copy of the state. The Graph service is one more thread of this kind. This decision names its links, because the window, the tray and the notifications are other threads.

| Link | Direction | Form |
|---|---|---|
| Sensor values and Alarms | daemon events -> service | the event channel that every service thread reads |
| Graph commands | window, tray, notification button -> service | new `Command` variants that the app routes to a channel of the service and not to the daemon client: apply, enable, delete, try, run once, cancel the countdown, turn the Graph responses on, settings changed |
| Graph view | service -> window, tray | one shared state behind a mutex, as the notification preferences of the settings: per Graph the live node values and the result, the countdown, the Response pause, the response log |
| Notifications | service -> notification service | a request on the channel that the notification service reads, so one thread owns the notification server |
| Graph sensor values and the Graph sensor calls | service -> daemon | the daemon client, with the call path without the interactive flag for the values |
| MQTT and SMTP | service -> clients | tasks on the multi-thread runtime that the app already owns. The service passes messages on a channel and gets the outcome back |

- Stop: "Quit" and the end of the session (SIGTERM) take one path. The app tells the service to stop, the service writes `graph-state.json` and cancels a running countdown with no power call, and the app waits for it for at most 2 s before it quits.
- Start: at its start and at each new snapshot of the daemon, the service compares its sensor Graphs with the Graph sensors of the daemon. It creates a missing one and stores the new id in the Graph. It leaves a Graph sensor that none of its Graphs holds: it can belong to another user. The Graphs screen lists such a sensor with a remove command.
- An app in a session that is not active sends no Graph sensor values and runs no response.
- Try: a source with no test value reads its live value.
- The text of a unit in a message is the display text of the app (`°C` for `degC`).
- The app sends an event message for a raise and a clear that it sees while it runs. It sends none for an Alarm that is already active when it starts or connects again: the message is a transition, and the status topic says that the app was away. The clear message takes its fields from the Alarm that the app stored at the raise.
- The time in a placeholder is a text that the service makes and gives to `nicecooling-graph`, which has no clock of its own for local time.

## Requirements of add-nicecooling-v1 that this change extends

`add-nicecooling-v1` is not archived, so this change has no delta against `openspec/specs/`. It extends these contracts, and its tasks edit them:

| Capability | Change |
|---|---|
| `fan-control` | An Input can be a Graph sensor. The Lost Input rule covers a Graph sensor that the app does not feed. |
| `hardware-discovery` | The sensor list has a third origin: Graph sensor, next to measured and Derived. |
| `profiles` | The source of a sensor Profile rule can be a Graph sensor. |
| `leakshield-operation` | A Software sensor slot can take a Graph sensor. |
| `control-api` | The Graph sensor members of G14, the keys `graph_sensors` and `graph_sensor_next` of `state.json`, the origin `graph`. No method is removed. |
| `desktop-app` | The Graphs screen, the editor, the MQTT and SMTP settings, the Response pause banner, the text in the Curve editor for a Graph sensor Input. |

Files of the contract that change: `crates/nicecooling-api/dbus/io.github.cappyt.NiceCooling1.xml`, `docs/api.md`, `docs/storage.md`, `docs/app.md`, `GLOSSARY.md`, `THIRD_PARTY.md`, and a new `docs/graphs.md`.

## Migration and rollback

This change converts no stored data. The Derived sensors stay.

- Forward: the daemon adds the key `graph_sensors` to `state.json`. The app creates its files at the first use.
- Back: the daemon state has no rule against unknown keys, and its schema version does not change. An older daemon loads the file, ignores `graph_sensors` and drops the key at its next save. A Curve whose Input was a Graph sensor then has a Lost Input and runs at 100%.
- Forward again after a downgrade: the app finds that the Graph sensors of its Graphs do not exist and creates them again with new ids. The user picks the Input of each such Curve again. `docs/graphs.md` has these steps.

## Risks / Trade-offs

- [No Graph runs without a session] -> a Curve on a Graph sensor runs at 100% then, with a Warning. The Curve editor says so. The alarm response of the daemon needs no Graph.
- [A wrong Graph gives a valid, wrong number to a Curve] -> nicecooling does not detect this. The Pump floor and the overrides of the control tick still apply.
- [A power response in a loop] -> the stored run state, the stored minimum interval with its 600 s default, the countdown with cancel and the Response pause.
- [A command response runs a program with the rights of the user] -> it is the program that the user chose, in the session of the user. No root process starts it.
- [The app parses data from the network (MQTT, SMTP, TLS)] -> the app has the rights of the user and no access to the devices. The daemon takes only numbers from it.
- [The app can feed a wrong value on purpose] -> the feed is in the `write` action, which already allows a direct write of a Fixed Duty.
- [The copied canvas code is 2800 lines of one author with no tests] -> the spike decides before more work depends on it, and the fallback is named.
- [51 node kinds] -> one table test per kind, and one validator and one evaluator for all.
- [`rumqttc` keeps its own queue and replays requests after a reconnect] -> the MQTT spike decides if the pending-slot rule holds. If not, the spec changes before the client is built.

## Open Questions

- The result of the editor spike (G13).
- The result of the MQTT spike (G9).
