# Design

## Context

`add-nicecooling-v1` gives the daemon a value table, a control tick, Derived sensors with three fixed kinds, a Rule engine for Profiles and an alarm response table (none, notify, poweroff). The app is a GPUI program that reads and writes the daemon through D-Bus.

aquasuite X.96 has one engine for two features: a graph of nodes that ends in a sensor node ("virtual sensor") or in an action node ("output"). The feature study of 2026-11-24 documents its model, its 48 node kinds, its timing and its MQTT client. It is kept outside the repository; this design states every rule that the implementation needs.

Owner decisions that bind this design:

- One Graph engine and one editor serve sensors and actions.
- All 48 node kinds of aquasuite are in the first version.
- A source without a value gives no value, and no value travels through the Graph.
- The app runs a command action in the user session. The daemon runs every other kind.
- MQTT publishes the event messages and the sensor values.
- The action kinds are notification, sound, command, e-mail, MQTT message, poweroff, reboot and suspend.
- The app warns and does not refuse a choice of the user. No action asks for a password.
- Existing code is reused before new code is written.

## Goals / Non-Goals

**Goals:**

- A user builds a sensor or an action from nodes, as in aquasuite, and sees the live value of each node.
- A Graph sensor is a normal sensor: an Input of a Curve, a source of a Rule, a software sensor of a Leakshield, a series of the history, a source of another Graph.
- A Curve never gets a number that the software made up for a missing source.
- The daemon evaluates the Graphs without a user session.

**Non-Goals:**

- The aquasuite file formats. A Graph of aquasuite does not import.
- The aquasuite rule "a missing input is 0" and its fallback value of a source node.
- Hotkeys, input controls (button, slider, dial), web export to the vendor service.
- Receiving values from MQTT.
- A defense against another local user who writes a Graph (a Non-Goal of `add-nicecooling-v1`).
- A change of the alarm response table. The poweroff of an Alarm stays a fixed path that no Graph can remove.

## Decisions

### G1. Model

A **Graph** has an id, a name, an enabled flag, a list of nodes and a list of connections. A **node** has an id, a kind, parameters and a canvas position. A node has input pins and one output pin, by kind. A **connection** joins one output pin to one input pin. An input pin has at most one connection. An output pin has any number.

A Graph has exactly one end node: a sensor node or an action node. The daemon refuses a Graph with none or with two. This is a model rule, not a guard against the user: a Graph without an end has no meaning.

Limits: 64 Graphs, 256 nodes per Graph, a name of 64 characters. aquasuite has no limit; these numbers bound the cost of one tick.

Alternative: keep the Derived sensor list and add functions. Rejected by the owner: the action use needs the Graph, and two models cost more than one.

### G2. Values and the no-value rule

A pin carries `Option<f64>`. A logic output is 1 or 0. A logic input is true above 0.

- A source node gives no value while its sensor has no value.
- A node whose **connected** input has no value gives no value and keeps its state. This holds for every kind: arithmetic, logic, switch, memory, delays, filters and generators.
- An input pin **without a connection** takes the default of its kind: 0, except base e for the logarithm, exponent 2 for the power, enable 1 and period 10 s for the generators and the counter. A missing connection is a state of the drawing, not a missing measurement.
- A division by 0, a logarithm or a root outside its domain, and a result that is not finite give no value.
- A sensor end node with no value publishes no value. A Curve then has a lost Input and follows the failure model of `add-nicecooling-v1`.
- An action end node with no value counts as false: the trigger time is forgotten.

Alternative: hold the last value for a set time. Rejected by the owner.

### G3. Node kinds

The kinds and their arithmetic are those of aquasuite X.96, with G2 in place of its missing-input rule and without the fallback value of the source node. The `node-graph` spec has one requirement per family with the exact formula of each kind:

- Sources: sensor, alarm event state, constant, random, process.
- Functions of two inputs: addition, subtraction, multiplication, division, minimum, maximum, average, logarithm, power.
- Functions of one input: sine, cosine, tangent, square root, round (nearest, down, up; 0 to 8 decimals), limiter.
- Table: pairs sorted by input, linear or step, no extrapolation.
- Logic: six comparisons (exact, no tolerance), and, or, not, switch.
- State: memory, persistent storage, trigger, on delay, off delay, counter.
- Time-window filters: peak, minimum, average, low pass, trend. A window gets one sample per tick and removes at most one old sample per tick, as aquasuite does.
- Signal generators: linear, sawtooth, triangle, sine, rectangle.
- Comment: text only, no pins.
- End nodes: sensor, action.

The source node "alarm event state" is new against aquasuite: it gives 1 while an Alarm or Warning event of the daemon is active for a chosen source, else 0. In aquasuite a device alarm is a data source of the device; nicecooling has its own event list.

The process node reuses the process scan of the Profile rules. The table reuses the interpolation of the Calibration table.

### G4. Evaluation

- The daemon evaluates every enabled Graph once per second, in the control tick, after the sources are read and before the Curve stage. A Curve of the same tick sees the new value of a Graph sensor.
- Order inside a Graph: sources and constants first, then each node whose connected inputs are computed. The write of a Graph refuses a connection that closes a loop, a connection between two pins of the same direction, and a connection inside one node.
- Order between Graphs: a Graph that reads the sensor of another Graph runs after it. The write of a Graph refuses a loop across Graphs.
- A disabled Graph publishes no value and runs no action.
- A window length and a delay are wall-clock seconds from the simulated clock of the core, so the tests run without waiting.
- For 10 s after the start of the daemon no action runs: the sensors must settle first.

### G5. Storage

- `graphs.json` in the state directory: all Graphs. Written through the state store of `add-nicecooling-v1` on each change.
- `graph-values.json`: the stored values of the persistent storage nodes and of the sensor nodes with "keep the value over a restart". Written at most every 30 s and at shutdown.
- Export: one Graph as one JSON file, written by the app. Import: the app sends the file content; the daemon gives the Graph and its nodes new ids. A sensor source that does not exist on this machine stays in the Graph as an unbound source (no value), and the editor marks it.

### G6. Conversion of the Derived sensors

At the first start with this change the daemon converts each stored Derived sensor to a Graph with the same sensor id:

- maximum of N sources: a chain of N - 1 maximum nodes;
- mean of N sources: a chain of additions and one division by a constant N;
- difference: one subtraction.

The sensor id stays, so a Curve, a Rule and a software sensor that use it keep working. The Derived sensor objects and methods of the D-Bus API are removed, and the Sensors screen opens the Graph editor for these sensors. The old file key is kept in a backup copy of the state file.

### G7. Action node

Parameters common to every kind: a hold time (default 0 s), "repeat every" (off, default 60 s) and a minimum interval between two runs (default 0 s), with the state machine of aquasuite:

1. Input false or no value: forget the trigger time, state "not run".
2. Rising edge: trigger time = now.
3. State "not run" and trigger time + hold time <= now: run, state "run".
4. State "run", repeat on, last run + repeat interval <= now: run again.
5. A run that comes before last run + minimum interval is dropped.
6. "Test" in the editor runs once now and passes rule 5.

There is no action on the falling edge. The output pin of the action node is 1 in the state "run".

Kinds and who runs them:

| Kind | Parameters | Runs in |
|---|---|---|
| Notification | title, text, urgency | app (it owns the desktop notifications) |
| Sound | one of the sound names of the freedesktop sound theme | app, as the sound hint of a notification |
| Command | program path, argument list, time limit (default 30 s) | app, in the user session |
| E-mail | recipient, subject, text | daemon |
| MQTT message | topic below the base topic, payload template | daemon |
| Poweroff, reboot, suspend | countdown in seconds (default 30) | daemon, through logind |

- The daemon sends a D-Bus signal `ActionRequested(graph, run id, kind, parameters)` for the three kinds of the app. The app answers with `ReportActionOutcome(run id, outcome)`. Without an app in a session the outcome is "no session", and the daemon records it. The owner chose this: a command does not run without a session.
- The text of a notification, an e-mail and an MQTT payload can hold placeholders for the Graph name and for the current value and unit of named nodes. aquasuite attaches a list of data sources to a message; the placeholder does the same with less interface.
- Poweroff, reboot and suspend use the countdown, the cancel path and the notification of the alarm poweroff of `add-nicecooling-v1`. A cancel sets the node to the state "run" without the power action.
- The command receives the Graph id, the Graph name and the node values as environment variables. The app kills it at the time limit.
- Each run writes one row to an action log in the daemon: time, Graph, kind, outcome. The Graph editor shows the last rows of its Graph.

Alternative for the command: the daemon runs it as the user who wrote the Graph. Rejected by the owner.

### G8. MQTT

- One broker in the daemon settings: host, port (default 1883, 8883 with TLS), TLS on or off with the system certificates and an optional CA file, user name, password, client id (default `nicecooling-<hostname>`), base topic (default `nicecooling`), value interval (default 10 s, 1 s to 1 h). MQTT is off until a host is set.
- The password is in a file of the state directory that only root reads. The D-Bus API never returns it.
- Topics: `<base>/status` with `online` (retained) and the last will `offline`; `<base>/sensor/<sensor id>` with one JSON object (value, unit, name, time), QoS 0, retained, at the value interval, for every sensor that is not hidden; `<base>/event/<event id>` with one JSON object per raise and per clear of an Alarm or Warning, QoS 1, not retained; the topic of an MQTT action node, QoS 1, not retained.
- A sensor without a value publishes `null` as its value, so a consumer sees the loss.
- A broker that is unreachable never blocks the tick: the client has its own task and a bounded queue, and it drops the oldest value message first. An event message waits in the queue.
- Client crate: `rumqttc` with `rustls`, which the workspace already builds.

### G9. E-mail

- One SMTP account in the daemon settings: host, port, security (STARTTLS, TLS, none), user name, password, sender address. Stored like the MQTT password.
- A send has a 30 s time limit and never blocks the tick. The outcome goes to the action log.
- Client crate: `lettre` with `rustls`.

### G10. Editor

- A new workspace crate holds a copy of `gpui-flow` (MIT, commit `cfa5787`) with its license file. It compiles unchanged against the GPUI version of the app. The crate depends on GPUI from the git repository of its vendor, so it cannot be a normal dependency. The copy gets the theme tokens of `docs/ui/DESIGN.md` and the keyboard rules of the app.
- A screen "Graphs" in the group "Control": the list of Graphs with name, kind of end node, state and live result; "New sensor", "New action", import.
- The editor: the canvas with pan and zoom, a node list by family, pins that connect by drag, the live value on each node (one D-Bus read per second while the editor is open), a parameter panel for the selected node, "Test" on an action node, undo and redo, and Apply. The editor holds a draft: the daemon gets the Graph at Apply, and it checks the model rules of G1 and G4 again.
- A source node has a "test value": the user gives a source a number for a set time, to try the Graph. The daemon evaluates the test on a copy of the draft Graph and returns the node values. A test never changes a published sensor value, so it never reaches a Curve, and a test of an action node runs the action once only through "Test".

### G11. API and authorization

New objects `Graph` under `io.github.cappyt.NiceCooling1` with read methods in the read class and write methods in the polkit action `write`. The broker and SMTP settings are in the `write` action too. No method asks for a password.

## Risks / Trade-offs

- **A Graph drives a pump or a fan through a Curve.** The no-value rule and the failure model of the Curve bound the damage of a broken Graph. A Graph that computes a wrong but valid number is the user's choice.
- **An action with poweroff in a wrong Graph can shut the PC down in a loop.** The 10 s start delay, the countdown with cancel and the minimum interval limit this. aquasuite adds an automatic maintenance mode after three shutdowns in a row; the spec review must decide if nicecooling needs it.
- **48 kinds are much code and many tests.** Each kind is a pure function with state, so the tests are tables of inputs and outputs.
- **The copied editor code is 2800 lines of another author**, with one commit and no tests. The on-screen behavior is not checked yet: the first task is a spike that shows a Graph with drag, connect, pan and zoom in the app, in light and dark, before any other editor work.
- **MQTT and SMTP add network clients to a root daemon.** Both use `rustls`, have time limits and never run in the tick.
- **The removal of the Derived sensor API breaks the app screens that use it** in the same change; they move to the Graph editor.

## Open Questions

- The automatic maintenance mode after repeated shutdowns (see Risks).
- Home Assistant discovery messages: not in this change by owner choice; the topic layout leaves room for it.
