# Graphs

A Graph is a set of nodes that the app evaluates once per second. It ends in a Graph sensor or in a Graph response. This file gives the model, the rules of the values, the 51 node kinds, the response nodes, the texts and the account settings. `docs/storage.md` defines the files. `docs/api.md` defines the Graph sensor on the daemon. `docs/app.md` describes the Graph service, the logind calls and the Response pause.

## Model

- A Graph has an id, a name, an enabled flag, nodes and connections. A Graph with a sensor end node has a `sensor_id` `graph_<n>`, or none.
- A node has an id (a positive integer, unique in its Graph), a kind, an optional label, parameters and a position on the canvas. A label has 1 to 32 letters, digits or `_`, and is unique in its Graph, with upper and lower case taken as equal.
- A node has input pins and at most one output pin `out`. A connection joins one output pin to one input pin. An input pin takes one connection. An output pin takes any number.
- A Graph has exactly one end node: a sensor end node or a response end node. An end node has no output pin. The comment node is neither a source nor an end node.
- Limits: 64 Graphs (an invalid Graph counts), 256 nodes in one Graph, a name of 64 characters, and 200 000 window samples for all Graphs together. A window node holds at most its span in seconds plus 1 samples.
- One validator serves the editor and the load of `graphs.json`. It returns every refusal of a Graph, each with one code, in this order: `no-end-node`, `two-end-nodes`, `end-node-output`, `unknown-kind`, `unknown-pin`, `pin-occupied`, `same-direction`, `same-node`, `loop`, `loop-across-graphs`, `limit-graphs`, `limit-nodes`, `limit-name`, `limit-samples`, `limit-graph-sensors`, `duplicate-label`, `duplicate-table-input`, `bad-parameter` (with the name of the parameter) and `bad-sensor-id`.
- A Graph that does not parse or does not pass the validator stays in `graphs.json` and does not run. The other Graphs load.
- A Graph that reads a Graph sensor of another Graph runs after it. A loop of nodes, or of Graphs through their sensors, is refused.

## Values and the no-value rule

- A pin carries a number or no value. A logic output is 1 or 0. A logic input is true above 0 and false at 0 or below.
- A node with a connected input pin that has no value gives no value and keeps its state. The node `has_value` is the one exception.
- An input pin with no connection takes the default of its kind. The table of the kinds lists it. The input pin of an end node has no default: a sensor end node with no connection gives no value, and a response end node with no connection counts as false.
- A division by 0, a function outside its domain and a result that is not finite give no value.
- A sensor source node gives no value while its sensor has none. The Graph never replaces a missing value with a number: a Curve that reads a Graph sensor with no value follows the Lost Input rule, so its Channel goes to 100% and the daemon raises the Warning `lost-input`.
- The daemon counts a Graph sensor value that is older than 3 s as no value (`docs/api.md`, Graph sensors). Without a running app, a Graph sensor has no value.
- A response end node holds its state while its input has no value. A false input clears the state.

## Node kinds

`out` is the output pin of every kind except `comment`, `sensor_end` and `response_end`. A parameter has the default in brackets. The kind of a node is its JSON name in `graphs.json`.

| Kind | Input pins (default) | Parameters (default) | Output value |
|---|---|---|---|
| `sensor_source` | none | `sensor` ("") | The value of the sensor, rounded to 8 decimals. No value while the sensor has none, while the id is unknown, and while the daemon is offline. |
| `alarm_event_source` | none | `event` (""), `source` ("") | 1 while the Alarm or Warning `event` of `source` is active, else 0. No value while the daemon is offline. |
| `constant` | none | `value` (0) | `value`. |
| `random` | none | `min` (-100), `max` (100) | A new number from `min` to `max` in each pass, with 3 decimals. The validator refuses `min` above `max`. |
| `process` | none | `name` ("") | 1 while a process with this name runs, else 0. See Process source and process Profile rule. |
| `add` | `a` (0), `b` (0) | none | a + b |
| `subtract` | `a` (0), `b` (0) | none | a - b |
| `multiply` | `a` (0), `b` (0) | none | a x b |
| `divide` | `a` (0), `b` (0) | none | a / b. No value when b is 0. |
| `minimum` | `a` (0), `b` (0) | none | The smaller of a and b. |
| `maximum` | `a` (0), `b` (0) | none | The larger of a and b. |
| `average` | `a` (0), `b` (0) | none | (a + b) / 2 |
| `logarithm` | `a` (0), `b` (e) | none | The logarithm of a to the base b. No value when a or b is 0 or below, and when b is 1. |
| `power` | `a` (0), `b` (2) | none | a to the power b. No value when the result is not finite. |
| `sine` | `a` (0) | none | sin(a), a in radians. |
| `cosine` | `a` (0) | none | cos(a), a in radians. |
| `tangent` | `a` (0) | none | tan(a), a in radians. No value when the result is not finite. |
| `square_root` | `a` (0) | none | The square root of a. No value when a is below 0. |
| `round` | `a` (0) | `decimals` (0), `mode` ("nearest") | a rounded to `decimals` (0 to 8) in the mode `nearest` (a tie goes away from 0), `down` or `up`. |
| `limiter` | `a` (0) | `min` (-100), `max` (100) | a held between `min` and `max`. The validator refuses `min` above `max`. |
| `table` | `a` (0) | `mode` ("linear"), `pairs` ([[-100,-100],[100,100]]) | The output for a from 2 to 64 pairs of an input and an output, by the interpolation of the Curve. `mode` is `linear` or `step`. Below the first input the output is the first output, above the last input it is the last output. The validator refuses two pairs with the same input. |
| `less` | `a` (0), `b` (0) | none | 1 when a < b, else 0. |
| `less_equal` | `a` (0), `b` (0) | none | 1 when a <= b, else 0. |
| `equal` | `a` (0), `b` (0) | none | 1 when the difference is at most 1e-9, else 0. |
| `greater_equal` | `a` (0), `b` (0) | none | 1 when a >= b, else 0. |
| `greater` | `a` (0), `b` (0) | none | 1 when a > b, else 0. |
| `not_equal` | `a` (0), `b` (0) | none | 1 when the difference is above 1e-9, else 0. |
| `and` | `a` (0), `b` (0) | none | 1 when a and b are true, else 0. |
| `or` | `a` (0), `b` (0) | none | 1 when a or b is true, else 0. |
| `not` | `a` (0) | none | 1 when a is false, else 0. |
| `switch` | `s` (0), `a` (0), `b` (0) | none | a while s is false, b while s is true. |
| `has_value` | `a` (no default) | none | 1 when a has a value, else 0. The one kind that sees a missing value. With no connection it gives 0. |
| `memory` | `s` (0), `a` (0) | none | Takes a while s is true and holds it while s is false. The start value is 0. It starts again at 0 after a restart of the app. |
| `persistent_storage` | `s` (0), `a` (0) | none | As memory. The app stores the value in `graph-state.json`, so it survives a restart. |
| `trigger` | `a` (0) | none | 1 for one pass when a goes from false to true, else 0. The first pass with a value gives 0. |
| `on_delay` | `a` (0) | `delay_s` (1) | 1 when a has been true for `delay_s` seconds (0 to 3600), else 0. A false input and no value start the time again. |
| `off_delay` | `a` (0) | `delay_s` (1) | 1 while a is true and for `delay_s` seconds after it becomes false. No value drops the delay. |
| `counter` | `count` (0), `enable` (1), `reset` (0), `period` (10) | none | While enable is true, adds count to the output once each period (at least 0.1 s, rounded to 0.1 s), at most once per pass. Reset true sets the output and the timer to 0. The start value is 0. |
| `peak` | `a` (0) | `span_s` (10) | The largest sample of the last `span_s` seconds (1 to 1800), rounded to 5 decimals. |
| `window_minimum` | `a` (0) | `span_s` (10) | The smallest sample of the last `span_s` seconds, rounded to 5 decimals. |
| `window_average` | `a` (0) | `span_s` (10) | The mean of the samples of the last `span_s` seconds, rounded to 5 decimals. |
| `low_pass` | `a` (0) | `span_s` (10), `damping` (25) | With fewer than 3 samples, their mean. Otherwise the output moves toward the trimmed mean of the window by the factor 1 - `damping` / 100 (`damping` 0 to 99.99). The trimmed mean drops the lowest and the highest samples in pairs while at least a third of the samples remain. |
| `trend` | `a` (0) | `span_s` (10) | The least-squares slope of the samples against the sample number, rounded to 5 decimals. With fewer than 3 samples it is 0. |
| `generator_linear` | `enable` (1), `reset` (0), `period` (10) | none | The phase t advances while enable is true. Reset true sets t to 0. The period is at least 0.1 s, rounded to 0.1 s. While enable is false the output is 0 and t holds. Shape: min(t / period, 1): rises once from 0 to 1 and stays at 1. |
| `generator_sawtooth` | `enable` (1), `reset` (0), `period` (10) | none | The phase t advances while enable is true. Reset true sets t to 0. The period is at least 0.1 s, rounded to 0.1 s. While enable is false the output is 0 and t holds. Shape: u, with u = t / period modulo 1. |
| `generator_triangle` | `enable` (1), `reset` (0), `period` (10) | none | The phase t advances while enable is true. Reset true sets t to 0. The period is at least 0.1 s, rounded to 0.1 s. While enable is false the output is 0 and t holds. Shape: 2u for u below 0.5, else 2(1 - u). |
| `generator_sine` | `enable` (1), `reset` (0), `period` (10) | none | The phase t advances while enable is true. Reset true sets t to 0. The period is at least 0.1 s, rounded to 0.1 s. While enable is false the output is 0 and t holds. Shape: (1 + sin(2 pi t / period)) / 2 |
| `generator_rectangle` | `enable` (1), `reset` (0), `period` (10) | none | The phase t advances while enable is true. Reset true sets t to 0. The period is at least 0.1 s, rounded to 0.1 s. While enable is false the output is 0 and t holds. Shape: 1 for u below 0.5, else 0. |
| `comment` | none | `text` (""), `width` (180), `height` (60) | No pins and no output. The comment is not evaluated and is not an end node. `text` has up to 1024 characters, `width` is 60 to 800, `height` is 20 to 600. |
| `sensor_end` | `a` (no default) | `unit` ("degC") | The value of the Graph sensor of the Graph. No value when a has none. `unit` is one of `degC`, `rpm`, `%`, `V`, `A`, `W`, `l/h`, `mbar`. No output pin. |
| `response_end` | `a` (no default) | `kind` ("notification"), `hold_s` (0), `repeat_s` (null), `min_interval_s` (0 or 600) | The input of the response state machine: true above 0. No connection counts as false. No output pin. Kind-specific parameters: see Response end node parameters. |

## Response end node parameters

A response end node has the parameters of its kind and these four. The validator refuses an empty value where the table gives a minimum of 1.

| Parameter | Range | Default |
|---|---|---|
| `kind` | `notification`, `sound`, `command`, `email`, `mqtt`, `poweroff`, `reboot`, `suspend` | `notification` |
| `hold_s` | 0 to 3600 s | 0 |
| `repeat_s` | `null` (off) or 1 to 86400 s | `null` |
| `min_interval_s` | 0 to 86400 s | 600 for `poweroff`, `reboot` and `suspend`, else 0 |

| Kind | Parameters |
|---|---|
| `notification` | `title` (1 to 64 characters), `text` (up to 1024 characters), `urgency` (`low`, `normal` or `critical`, default `normal`) |
| `sound` | `sound` (1 to 64 characters, a name of the freedesktop sound theme) |
| `command` | `path` (an absolute path), `args` (a list of up to 32 arguments of up to 1024 characters, default empty), `time_limit_s` (1 to 3600 s, default 30) |
| `email` | `to` (one address with one `@`, up to 254 characters), `subject` (up to 128 characters), `text` (up to 4096 characters) |
| `mqtt` | `topic` (1 to 128 characters, no `+`, no `#`, no empty level, no leading or trailing `/`), `payload` (up to 4096 bytes) |
| `poweroff`, `reboot`, `suspend` | `countdown_s` (10 to 3600 s, default 30) |

`docs/app.md` (Responses) says what the app does for each kind, and which kinds it runs.

## Evaluation order

1. Once per second the app takes the newest sensor values and Alarms from the events of the daemon. The delay, window, hold, repeat and generator times count seconds of the monotonic clock. This clock does not count the time of a suspend. The minimum interval of a response counts wall clock time.
2. A pass evaluates the enabled Graphs. A Graph that reads the Graph sensor of another enabled Graph comes after it. Graphs on a loop come last, in the stored order. A disabled Graph is not evaluated, keeps the state of its nodes, and its Graph sensor has no value.
3. In a Graph, a node comes after every node that feeds one of its input pins. Nodes that are ready at the same time come in the order of their ids. The comment node is left out.
4. A sensor source node reads the value of a Graph sensor from the same pass when its Graph already ran. It reads any other sensor from the last event of the daemon.
5. After the pass the app sends the Graph sensor values to the daemon in one call. It then steps the response end nodes.
6. A Graph that panics is stored as disabled, and a notification names it. The other Graphs of the pass are not affected.
7. Apply of a Graph starts the state of its nodes again. A persistent storage node keeps its value when its id and kind stay.

## Response state machine

Each enabled response end node has the state `not run` or `run`, and a trigger time. Each pass steps it with its input.

| Input | Effect |
|---|---|
| false | The trigger time clears and the state is `not run`. Nothing runs. |
| no value | The state stays. The trigger time clears, so the hold time starts again. |
| true, state `not run` | The first true pass sets the trigger time. When the input has been true for `hold_s`, the response runs and the state is `run`. |
| true, state `run`, repeat on | The response runs again when `repeat_s` have passed since the last run or dropped run. |
| true, state `run`, repeat off | Nothing runs. The node runs once for each rise. |

- A run that starts less than `min_interval_s` after the last run is dropped. The state is still `run`, and the run leaves no log row. A last run that is later than the wall clock moves back to the wall clock.
- The app stores the state and the time of the last run of each node in `graph-state.json`. After a restart with the stored state `run` and a true input, the node stays in `run`, and the repeat interval counts from the start of the app.
- An Apply of the Graph keeps the stored state of the node while the node keeps its id, also when only the response kind changes. The hold time and the repeat interval start again at the Apply.
- In a session that is not active no response runs, and a true input counts as no value (`docs/app.md`, Active session).
- A run that the app holds back is not a run. While the Response pause is on, and for a power response while an Alarm poweroff countdown runs, a true input in the state `not run` counts as no value: the state stays `not run`, the node gets no time of last run and no attempt for an interval, and the run writes no log row. The hold time starts again when the responses are on or the Alarm countdown is gone. A false input still sets `not run`, and a node in `run` stays in it while its input stays true (`docs/app.md`, Response pause).
- Each run writes one row to `response-log.json` with the outcome `ok`, `failed`, `timeout`, `not-configured`, `busy` or `cancelled`. A dropped run writes none.
- "Run once" runs the response of the draft at once. It ignores the state, the minimum interval and the Response pause. The validator reads the draft first: a draft that it refuses runs nothing, and the app shows the name of the Graph and the refusal code.

## Placeholders

The text of a notification expands these placeholders. The title, the sound name and the arguments of a command do not. The subject and text of an e-mail and the payload of an MQTT message use the same rules when the app runs these kinds.

| Placeholder | Result |
|---|---|
| `{graph}` | The name of the Graph. |
| `{time}` | The local date and time of the run as `YYYY-MM-DD HH:MM:SS`. |
| `{value:<label>}` | The value of the node with this label in the pass of the run. `--` for no value. At most 2 decimals, a tie goes away from 0, no trailing zero. |
| `{unit:<label>}` | The display text of the unit (`°C` for `degC`) for a sensor source node. Empty for every other node. |
| `{{` and `}}` | `{` and `}`. |

A placeholder with an unknown label, and any other text in braces, stays as written.

A command response gets its values in the environment, in addition to the environment of the app. `NICECOOLING_GRAPH_ID`, `NICECOOLING_GRAPH_NAME` and `NICECOOLING_RUN_ID` hold the Graph and the run. Each labelled node has `NICECOOLING_NODE_<LABEL>` with the label in upper case. The value is decimal text rounded to 8 decimals, with no trailing zero and no exponent. It is empty for a node with no value. A node with no label has no variable.

## MQTT topics

`<base>` is `base_topic` of the table `[mqtt]` (`docs/storage.md`). The client only publishes. `docs/app.md` (MQTT and SMTP clients) describes the connection, the reconnect rule and the queue.

| Topic | Message | QoS | Retained |
|---|---|---|---|
| `<base>/status` | `online` after each connect. The broker publishes `offline` as the last will. | 1 | yes |
| `<base>/sensor/<sensor id>` | `{"value", "unit", "name", "time"}` of each sensor that is not hidden: measured, Derived and Graph sensors. | 0 | yes |
| `<base>/event/<event id>` | `{"event", "source", "severity", "state", "time", "details"}` for each raise and each clear of an Alarm or Warning that the app sees while it runs. | 1 | no |
| `<base>/<topic>` | The payload of an MQTT message response. | 1 | no |

- A sensor message is sent at `interval_s` and at once after a connect. `value` is `null` for a sensor with no value, also for the Graph sensor of a disabled Graph. `unit` is the display text. `time` is in microseconds since the Unix epoch.
- `severity` is `alarm` or `warning`, `state` is `raised` or `cleared`. The clear message takes `source` and `details` from the Alarm that the app stored at the raise. An Alarm that is active when the app starts or reads the daemon again has no raise message. The event id keeps its dot, for example `<base>/event/leakshield.leak-alarm`.
- An MQTT message response with an empty payload sends `{"graph_id": "<id>", "graph_name": "<name>", "time": <microseconds>}`.
- The client subscribes to no topic. No message from the broker changes a Graph, a sensor or a setting.

## SMTP account

The app holds one account for the e-mail response, in the table `[smtp]` of `settings.toml` (`docs/storage.md`). The password is in `secrets.toml`.

| Setting | Meaning |
|---|---|
| `host` | The server. An account needs a host and a sender address. Without them the outcome of an e-mail response is `not-configured`. |
| `security` | `starttls` (port 587), `tls` (port 465) or `none` (port 25). The port setting replaces the default port. |
| `user` | The login name. The sender logs in when it is set, in every security. |
| `sender` | The `From` address. |

- The secure modes check the certificate of the server against the system certificates and the host name. The sender has no way to skip the check.
- With the security `none` and a user, the login is not encrypted. The settings warn and do not refuse.
- The app sends one plain text message in its own task, with a limit of 30 s, so a slow server never delays a pass. It removes line breaks from the subject.

## Process source and process Profile rule

The process node and a process Profile rule use the same scan of `/proc` and the same name match. They differ in the user that runs the scan.

- The daemon runs as root. A process Profile rule reads the `comm` value and the program path (`/proc/<pid>/exe`) of every process, and with `match_on` `cmdline` the command line.
- The app runs as the user. The process node reads the `comm` value of every process, and the program path only of the processes of this user. It has one name and no `cmdline` match.
- A name that matches only the program path of a process of another user makes the rule true and the node 0. A name that matches a `comm` value gives the same result in both.

## Example: the larger of two temperatures for a Curve

The Graph gives the larger of the temperatures `k10temp_temp1` and `nct6798_temp1` as the Graph sensor `graph_1`.

```json
{
  "id": "graph_1",
  "name": "Hottest",
  "enabled": true,
  "sensor_id": "graph_1",
  "nodes": [
    {"id": 1, "kind": "sensor_source", "label": null, "params": {"sensor": "k10temp_temp1"}, "x": 40.0, "y": 40.0},
    {"id": 2, "kind": "sensor_source", "label": null, "params": {"sensor": "nct6798_temp1"}, "x": 40.0, "y": 140.0},
    {"id": 3, "kind": "maximum", "label": null, "params": {}, "x": 240.0, "y": 90.0},
    {"id": 4, "kind": "sensor_end", "label": null, "params": {"unit": "degC"}, "x": 440.0, "y": 90.0}
  ],
  "connections": [
    {"from": {"node": 1, "pin": "out"}, "to": {"node": 3, "pin": "a"}},
    {"from": {"node": 2, "pin": "out"}, "to": {"node": 3, "pin": "b"}},
    {"from": {"node": 3, "pin": "out"}, "to": {"node": 4, "pin": "a"}}
  ]
}
```

1. Apply the Graph. The app calls `CreateGraphSensor` with the name "Hottest" and the unit `degC`, and stores the id of the new sensor in `sensor_id`.
2. Each pass sends the value of `graph_1`. With `k10temp_temp1` at 62.5 and `nct6798_temp1` at 48, the value is 62.5.
3. Use the sensor as the Input of a Curve. This call gives 20% at 40 degrees and 100% at 80 degrees. At 62.5 the Curve gives 65%, before smoothing and the step limit:

   ```text
   busctl --allow-interactive-authorization=true call io.github.cappyt.NiceCooling1 /io/github/cappyt/NiceCooling1/channel/nct6798_pwm1 io.github.cappyt.NiceCooling1.Channel SetCurve 'oa(dd)dd' /io/github/cappyt/NiceCooling1/sensor/graph_1 2 40 20 80 100 5 5
   ```

4. When one source has no value, `maximum` gives no value, and so does `graph_1`. The Curve then follows the Lost Input rule: the Channel goes to 100% and the Warning `lost-input` shows. The same happens within 3 s after the app stops.

## Downgrade

The daemon keeps the Graph sensors in `state.json` (`docs/storage.md`). The app keeps the Graphs in its own files. A downgrade converts no data.

1. Quit the app in each user session, so no Graph runs and no response fires.
2. Install the older version. The older daemon loads `state.json` and ignores the keys `graph_sensors` and `graph_sensor_next`. It drops them at its next save.
3. A Curve whose Input was a Graph sensor now has a Lost Input. Its Channel runs at 100% and the Warning `lost-input` shows. Set another Input for the Curve if the 100% is not wanted.
4. To go forward again, install the new version and start the app. The app finds that the Graph sensors of its Graphs do not exist, and creates them again with new ids. A new id is above every `graph_<n>` that the stored state still names, so the Curve of an old Graph sensor never reads a new one.
5. Pick the Input of each such Curve again. The Curve still names the old id and stays on Lost Input until you do.
