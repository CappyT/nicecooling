# Graph editor

Comps: `../comps/graph-editor.html` (a Graph with a sensor end node, one node selected) and `../comps/graph-editor-response.html` (a Graph with a response end node, the response panel and a try). Both are 1280x800, and also 1100x700 in `../comps/shots/`. The 1100x700 renders have the node list as a popover: closed in the `-min` renders, open in `graph-editor-min-add-node-light.png` and `-dark.png`.

The editor builds one Graph: nodes on a canvas, connections between their pins, and one end node. It works on a draft. The app stores the Graph at "Apply". The editor opens from the Graphs screen (`graphs.md`) and from a Graph sensor of the Sensors screen. The sidebar keeps "Graphs" selected.

The canvas is the copy of `gpui-flow` (`docs/spikes/gpui-flow.md`). The section "Canvas" names what the design needs from it.

## Layout

| Area | Size | Content |
|---|---|---|
| Toolbar | 46 px high | Back button, name, draft state, "Add node" below 1280 px, undo, redo, the first broken rule, "Try", "Apply" |
| Try banner | 46 px high | Only while a try runs: "Try running, nothing is published", the time left, "Stop" |
| Node list | 168 px wide | Search field and the node kinds by family. A column at a window width of 1280 px or more, a popover below it |
| Canvas | the rest | Nodes, pins, connections. A status line of 28 px under it |
| Parameter panel | 272 px wide | The parameters of the selected node. For a response end node it is the response panel |

The layout has two forms. The width of the window chooses the form.

| Window width | Columns | Node list |
|---|---|---|
| 1280 px or more | Three: node list, canvas, parameter panel | The left column |
| Below 1280 px | Two: canvas, parameter panel | A popover that the button "Add node" of the toolbar opens |

The parameter panel has the same width in both forms. At 1280x800 the canvas is 620 px wide, and at 1100x700 it is 608 px wide: a Graph of four columns of nodes shows at 82% and at 80%.

### Toolbar

From the left:

1. The flat button "Graphs" with the left chevron. It goes back to the Graphs screen.
2. The name of the Graph: an input, 240 px wide, 64 characters at most. A Graph with a sensor end node gives this name to its Graph sensor.
3. The draft state as a caption: "Not applied" while the draft differs from the stored Graph, "Applied 21:44:02" after an Apply, or nothing.
4. Below 1280 px only: the button "Add node" with the plus icon. It opens and closes the node list popover.
5. At the right: the icon buttons "Undo" and "Redo", then the first broken rule with the Warning icon, then "Try" and "Apply". "Apply" is the suggested button.

"Apply" is off when nothing changed and when the draft breaks a rule of the validator. "Try" is off when the draft breaks a rule and while a try runs.

### Node list

- A search field at the top: "Search nodes". It filters by the name of the kind, without case. "delay" gives "On delay" and "Off delay".
- Seven family rows: "Sources", "Math", "Logic", "State and time", "Filters", "Generators", "Other". A family row has a chevron, its name and its count. A click folds or unfolds it. All families are open at first. With a text in the search field, the list shows the families that have a match, open.
- One row per kind, 28 px high, with the name of the kind. The list holds 51 kinds: see "Node kinds".
- "Sensor end node" and "Response end node" are disabled, with the tooltip "The Graph has its end node."
- A double click on a row adds a node of that kind at the centre of the view.
- A long name truncates with an ellipsis. The tooltip has the full name and the help text of the kind.
- Column form, at 1280 px or more: the list is the left column of the editor. A drag of a row onto the canvas adds the node where the pointer lets it go.
- Popover form, below 1280 px: the list is a `popover` under the button "Add node", 240 px wide and 480 px high at most. It has the same search field and the same families, and its rows scroll. When it opens, the search field has the focus. A double click or Enter adds the node at the centre of the view, and the popover stays open. Escape and a press outside the popover close it. A drag of a row adds no node.
- A change of the window width to 1280 px or more closes the popover.

### Canvas

- Ground: `view`, with a dot of `border` every 24 units. One unit is 1 px at 100%.
- A node is a box 152 units wide: `card` fill, 1 px `border-strong` outline, radius 9. In the light theme it has the card shadow. Its rows, from the top:

| Row | Height | Content |
|---|---|---|
| Title | 20 | The name of the kind in `heading`. A kind with one main parameter shows it after the name in `fg-dim`: "On delay 10 s", "Limiter 20 to 60", "Average filter 30 s". An end node has the title "End node" |
| Subject | 16 | Only for five kinds, in `caption` size and `fg`: the name of the sensor, the Alarm event and its source, the process name, "Graph sensor" on a sensor end node, the kind of the response on a response end node |
| Value | 22 | At the left the label of the node in mono `caption`, when it has one. At the right the live value in `value` and the unit in `fg-dim`. The output pin is on the right edge of this row |
| Guide | 10 | Only on a Limiter: the band guide (see "Band guide") |
| Pins | 20 each | One row per input pin: the pin on the left edge and its name in `caption`: "A", "B", "S", "Count", "Enable", "Reset", "Period" |

- A value shows 4 significant digits at most. No value is "--". A node with a logic output shows "1" with "true" or "0" with "false" in the place of the unit.
- A sensor source shows the unit of its sensor. A sensor end node shows the unit of the Graph sensor. No other node has a unit.
- An end node has no output pin. Its value row shows the value of its input. A response end node has one more caption row with its state: "Fired 14:05:09" or "Not fired".
- A comment node is a box of its own width and height with `hover` fill, no outline, and its text in `caption`. It has no pins and no value.
- Pin: a circle of 10 units, `card` fill, 1.5 px `fg-dim` outline. A connected pin has the `fg-dim` fill. The snap target of a connection drag has the `accent-bg` fill.
- Connection: a curve of 1.5 px in `fg-dim` from an output pin to an input pin, with an arrow at the input pin. A connection has no routing: it can pass behind a node.
- Selection: one node or one connection at a time. The selected node has a 2 px `accent-bg` ring outside its outline, so the node does not move. The selected connection is 2 px `accent-bg`.
- A node that exists only in the draft has a dashed outline and the value "--" until "Apply".
- The draft line of a connection drag is 1.5 px `accent`, dashed.
- Status line, 28 px, under the canvas: at the left one line of text (the reason of a refused connection, or a read error), at the right the zoom: the buttons "Zoom out" and "Zoom in" as icons and the value "100 %".
- The editor opens with the whole Graph in the view, at 100% at most.

What the design needs from the copied canvas, with the state that `docs/spikes/gpui-flow.md` records:

| Need | State of the copy |
|---|---|
| Node content with rows, live values and pins at their rows | Works. The app gives each pin its offset from the row heights above |
| Connect by drag, with the rules of the validator | Works at the release. The reason of a refusal goes to the status line: the canvas has no place for it |
| No snap highlight on a pin that refuses the connection | Missing: the copy validates at the release only |
| Selection of one node, shown without a move of the node | The copy grows the border by 1 px. The ring replaces it |
| Delete, not for the end node | `deletable` exists |
| Undo and redo of the draft with Ctrl+Z | The copy has its own history on the Super key. The draft owns the history |
| A release outside the canvas or after a key press ends the drag | Two open defects of the copy |
| Add a node by a drag from the node list, in the column form only | The copy has no drop target. The app adds one on the canvas area |
| Focus ring and keys on the canvas | Missing in the copy |
| Dashed outline, dashed draft line | Check against the pinned `gpui` version |
| Line width and arrow size follow the zoom | Open defect of the copy |

The design does not use the box selection, the selection of several nodes, the minimap, the controls of the copy, or an input control inside a node.

### Parameter panel

The panel shows the selected node. With no selection it shows the end node. From the top:

1. Header: the name of the kind, and the family as a caption. Under it one line of help text.
2. "Label": an input in mono text, 1 to 32 characters of letters, digits and "_". Caption: "A label names the node in a message: {value:water}."
3. The parameters of the kind: one row per parameter, the name at the left and the control at the right. See "Node kinds".
4. A guide, for a kind that has one: the band guide of a Limiter, the plot of a Table.
5. "Inputs": one row per input pin of the selected node, with the name of the pin and a select. This is the keyboard path for a connection.
   - The select lists "None" and each other node that has an output pin. An entry has the title of the node, and its label when it has one: "Switch", "On delay 10 s", "Sensor cpu".
   - A choice connects the output of that node to the pin. It replaces the connection that the pin has. One choice is one undo step.
   - A choice that closes a loop does not connect. The select keeps its earlier entry, and the status line shows the reason for 5 s: "Not connected: the connection closes a loop".
   - "None" removes the connection. A pin without a connection shows "None" and its default: "None, 0". The pin of an end node has no default and shows "None".
6. "Try", only on a Sensor node: "Test value" and "For" (seconds).
7. "Delete node": a flat button.

The panel of an end node has no "Label" and no "Delete node": a Graph keeps its end node.

A parameter edit commits on Enter or when the field loses the focus. Escape keeps the earlier value. One committed edit is one undo step. A value outside its range does not commit: the field keeps the focus and shows the range under it ("0 to 3600 s").

The panel of a sensor end node has:

- Header: "End node" and a segmented control with "Sensor" and "Response". The control changes the kind of the end node.
- "Unit": a select with "°C", "rpm", "%", "V", "A", "W", "L/h", "mbar".
- "Graph sensor": the id in mono text ("graph_3"), or "Created at Apply".
- "Used by": the Curves, Profile rules and Software sensor slots that use the Graph sensor, or "Not used".
- "Inputs" with the one pin "A".

### Response panel

The panel of a response end node. From the top:

1. Header: "End node" and the segmented control "Sensor", "Response". Under it the help text.
2. "Kind": a select with "Notification", "Sound", "Command", "E-mail", "MQTT message", "Poweroff", "Reboot", "Suspend".
3. The warning of the kind, when one applies: a row with the Warning icon on `warning-tint`.
4. The parameters of the kind:

| Kind | Parameters |
|---|---|
| Notification | "Title" (1 to 64 characters), "Text" (1024 at most), "Urgency": "Low", "Normal", "Critical". Default "Normal" |
| Sound | "Sound": a name of the sound theme of the desktop, 64 characters at most ("dialog-warning") |
| Command | "Program": an absolute path. "Arguments": a list, one input per argument, 32 at most. "Time limit": 1 to 3600 s, default 30 |
| E-mail | "To", "Subject" (128 characters at most), "Text" (4096 at most) |
| MQTT message | "Topic": the base topic shows before the input as dim text ("nicecooling/"). "Payload": a text of 4096 bytes at most. Empty gives the default message |
| Poweroff, Reboot, Suspend | "Countdown": 10 to 3600 s, default 30 |

5. "Hold time": 0 to 3600 s, default 0. "Repeat every": a switch and a field, 1 to 86400 s, default 60 when on. "Minimum interval": 0 to 86400 s. A change of the kind to "Poweroff", "Reboot" or "Suspend" sets it to 600 s, and a change to another kind sets it to 0 s.
6. "Run once": a button. Caption: "Runs now with the parameters of the draft." For "Poweroff" and "Reboot" the caption adds: "It counts for the Response pause."
7. "Inputs" with the one pin "A".
8. "Last runs": the newest rows of the response log of this Graph, 20 at most. The list scrolls with the panel. A row has the time in mono text and the outcome: "OK", "Failed", "Timeout", "Not configured", "Busy", "Cancelled". A row of another kind than the kind of the draft starts with that kind: "E-mail: OK". Without a row: "No run yet."

A text field of a notification, an e-mail or an MQTT payload has the caption: "Placeholders: {graph}, {time}, {value:label}, {unit:label}".

A warning never turns "Apply" or "Run once" off.

| Warning | When | Text |
|---|---|---|
| Power kind | The kind is "Poweroff", "Reboot" or "Suspend" | "This response can power off or restart the PC" |
| Command path | "Program" is not an executable file | "The program is not an executable file" |
| No SMTP account | The kind is "E-mail" and Settings has no SMTP host | "No SMTP account" and the link "Open Settings" |
| No broker | The kind is "MQTT message" and Settings has no broker host | "No MQTT broker" and the link "Open Settings" |

### Band guide

A guide shows a value against the range of its node. It uses the look of the pressure gauge (`../widgets.md`).

- In the panel of a Limiter: a scale 56 px high. The band from the minimum to the maximum has the `accent-tint` fill and an `accent` outline. The scale has a margin of a fifth of the band at each side. The input A is a 6x22 `fg` marker with the text "A 46.2" above it. The texts "Minimum 20" and "Maximum 60" are under the ends of the band. A marker outside the band stays `fg`: the limit is not a Warning.
- On the node of a Limiter: the same band and marker in a row of 10 units, without text.
- In the panel of a Table: the pairs as a line on the scales of the Curve editor, 120 px high, with a dot at the input A. The pairs also show as number inputs under the plot.
- Without a live value the guide has the band and no marker.

## Node kinds

The node list has 51 kinds. "Title" is what the node shows after the name of its kind. Every time is a whole number of seconds. A pin without a connection takes 0, or the default in this table.

| Family | Name (stored kind) | Input pins | Parameters, range, default | Title | Help text |
|---|---|---|---|---|---|
| Sources | Sensor (`sensor_source`) | none | "Sensor": the sensor picker. Any sensor, Derived sensors and Graph sensors included | none | "Gives the value of a sensor." |
| Sources | Alarm event (`alarm_event_source`) | none | "Event": a select of the Warning and Alarm types. "Source": a select of the Loops and the Leakshields | none | "Gives 1 while this Warning or Alarm is active, else 0." |
| Sources | Constant (`constant`) | none | "Value", default 0 | none | "Gives a fixed number." |
| Sources | Random (`random`) | none | "Minimum" -100, "Maximum" 100 | "-100 to 100" | "Gives a new number in the range each second." |
| Sources | Process (`process`) | none | "Process": the process picker, one name of 1 to 255 characters | none | "Gives 1 while a process with this name runs, else 0." |
| Math | Add, Subtract, Multiply, Divide, Minimum, Maximum, Average (`add`, `subtract`, `multiply`, `divide`, `minimum`, `maximum`, `average`) | A, B | none | none | "A + B" / "A - B" / "A x B" / "A / B. No value when B is 0." / "The smaller of A and B." / "The larger of A and B." / "The mean of A and B." |
| Math | Logarithm (`logarithm`) | A, B. B alone: e | none | none | "The logarithm of A to the base B." |
| Math | Power (`power`) | A, B. B alone: 2 | none | none | "A to the power B." |
| Math | Sine, Cosine, Tangent (`sine`, `cosine`, `tangent`) | A | none | none | "The sine of A. A is in radians." and the same for the two others |
| Math | Square root (`square_root`) | A | none | none | "The square root of A. No value when A is below 0." |
| Math | Round (`round`) | A | "Decimals" 0 to 8, default 0. "Mode": "Nearest", "Down", "Up". Default "Nearest" | "1 decimal" | "Rounds A." |
| Math | Limiter (`limiter`) | A | "Minimum" -100, "Maximum" 100 | "20 to 60" | "Holds A between the minimum and the maximum." |
| Math | Table (`table`) | A | "Mode": "Linear", "Step". "Pairs": 2 to 64 pairs of an input and an output. Default (-100, -100) and (100, 100) | "3 pairs" | "Maps A through a table of pairs." |
| Logic | Less, Less or equal, Equal, Greater or equal, Greater, Not equal (`less`, `less_equal`, `equal`, `greater_equal`, `greater`, `not_equal`) | A, B | none | none | "1 when A is less than B, else 0." and the same for the five others |
| Logic | And, Or (`and`, `or`) | A, B | none | none | "1 when A and B are true." / "1 when A or B is true." |
| Logic | Not (`not`) | A | none | none | "1 when A is false." |
| Logic | Switch (`switch`) | S, A, B | none | none | "Gives A while S is false, and B while S is true." |
| Logic | Has value (`has_value`) | A | none | none | "1 when A has a value, 0 when it has none." |
| State and time | Memory (`memory`) | S, A | none | none | "Takes A while S is true. Holds the last value while S is false. Starts at 0 with the app." |
| State and time | Persistent storage (`persistent_storage`) | S, A | none | none | "As Memory. Keeps its value over a restart of the app." |
| State and time | Trigger (`trigger`) | A | none | none | "Gives 1 for one second when A becomes true." |
| State and time | On delay (`on_delay`) | A | "Delay" 0 to 3600 s, default 1 | "10 s" | "Gives 1 when A has been true for the delay." |
| State and time | Off delay (`off_delay`) | A | "Delay" 0 to 3600 s, default 1 | "10 s" | "Gives 1 while A is true, and for the delay after it." |
| State and time | Counter (`counter`) | Count, Enable, Reset, Period. Enable alone: 1. Period alone: 10 s | none | none | "While Enable is true, adds Count once each period. Reset sets it to 0." |
| Filters | Peak filter, Minimum filter, Average filter (`peak`, `window_minimum`, `window_average`) | A | "Span" 1 to 1800 s, default 10 | "30 s" | "The largest value of the span." / "The smallest value of the span." / "The mean of the span." |
| Filters | Low pass (`low_pass`) | A | "Span" 1 to 1800 s, default 10. "Damping" 0 to 99.99, default 25 | "30 s" | "A damped mean of the span, without its extremes." |
| Filters | Trend (`trend`) | A | "Span" 1 to 1800 s, default 10 | "30 s" | "The slope of the values of the span." |
| Generators | Linear generator, Sawtooth generator, Triangle generator, Sine generator, Rectangle generator (`generator_linear`, `generator_sawtooth`, `generator_triangle`, `generator_sine`, `generator_rectangle`) | Enable, Reset, Period. Enable alone: 1. Period alone: 10 s | none | none | "A signal from 0 to 1 that repeats each period." For the linear one: "Rises from 0 to 1 in one period and stays at 1." |
| Other | Comment (`comment`) | none | "Text": 1024 characters at most. "Width" 60 to 800, default 180. "Height" 20 to 600, default 60 | none | "A note on the canvas." |
| Other | Sensor end node (`sensor_end`) | A | "Unit" | none | "Gives the value of A to the Graph sensor." |
| Other | Response end node (`response_end`) | A | The response panel | none | "Runs the response when A becomes true." |

## Try

- A Sensor node has the fields "Test value" (empty at first) and "For" (1 to 600 s, default 60) in its panel. A Sensor node with a test value has the tag "Test" on its title row.
- "Try" evaluates the draft in the app. A source with a test value gives that value for its time, then its live value. A source without a test value gives its live value.
- The try ends when the longest time of a test value ends, after 60 s when no source has a test value, on "Stop", and when the draft changes.
- While a try runs: the Try banner shows, with "Stop" and the time left ("24 s left"). Each node shows the value of the try, also a node that exists only in the draft. A response end node shows "Not run in a try" in place of its state.
- A try sends no Graph sensor value and runs no response. The Graph sensor keeps the value of the applied Graph.

## States

| State | What the user sees |
|---|---|
| New Graph | The canvas has the end node only, at the right of the view. Text in the middle of the canvas, in `fg-dim`: "Double-click a node in the list to add it." Below 1280 px the text is: "Press Add node to add a node." |
| Window below 1280 px | The editor has no node list column. The toolbar has the button "Add node" |
| Node list popover open | The popover shows under "Add node", above the canvas, and the button has the `selected` fill. The canvas and the panel do not change. Escape, a press outside or a width of 1280 px or more closes it |
| Loading | The canvas shows the nodes with "--" until the first values arrive, 2 s at most |
| Live values | Each node of the applied Graph shows its value, read once per second. A node whose parameters differ in the draft still shows the value of the applied Graph |
| No value | "--" in the place of the value. The nodes after it show "--" too |
| Draft-only node | Dashed outline and "--" until "Apply". "Try" gives it a value |
| Disabled Graph | Every node shows "--". A caption in the status line: "The Graph is disabled. Turn it on in the list." "Try" still works |
| Broken rule | "Apply" and "Try" are off. The toolbar shows the Warning icon and the first broken rule: "A Graph has one end node". The node or the field of the rule has a 2 px `warning` outline |
| Refused connection | The canvas draws nothing. The status line shows the reason for 5 s: "Not connected: input already connected" |
| Unbound source | A Sensor node whose sensor does not exist on this machine. The title row has the Warning icon. The subject row has the stored sensor id in mono text. A caption row under it says "Sensor not found" in the Warning color. The value is "--". The panel has the sensor picker, so the user can choose another sensor. "Apply" stays on |
| Daemon offline | Banner 2 of the shell. The node values are dim: a Sensor node and an Alarm event node show "--". The draft stays, and every edit works. For a Graph with a sensor end node "Apply" is off, with the text "The daemon is offline. The Graph sensor needs the daemon." A Graph with a response end node can be applied |
| Read error, Graph file | The app cannot read `graphs.json`. No editor opens. The Graphs screen shows the error and "Retry" (`graphs.md`) |
| Read error, node values | The app cannot read the node values. Every node shows "--" in `fg-dim`. The status line shows "The node values are not available." and the link "Retry". The draft and every edit still work |
| Try running | The Try banner. See "Try" |
| Applied | "Apply" turns off. The draft state says "Applied 21:44:02". The nodes that were draft-only get a solid outline and a value |
| Daemon refuses at Apply | The editor keeps the draft and stores nothing. The toolbar shows the octagon and the reason: "Not applied: the limit of 64 Graph sensors is reached." |
| End node kind change in use | Apply of a change from "Sensor" to "Response" while something uses the Graph sensor: "Not applied: the Curve of Waterpump uses the Graph sensor." The app stores nothing |
| Unit change in use | Apply of a new unit while a Software sensor slot uses the Graph sensor: "Not applied: Software sensor 4 uses the Graph sensor." |
| Leave with a draft | A dialog: "The Graph 'Render temperature' has changes that are not applied." Buttons: "Cancel", "Discard", "Apply". The default focus is on "Apply". When Apply fails, the editor stays open |
| Run once blocked | While a poweroff countdown of an Alarm runs, "Run once" of a power kind runs nothing. A caption under the button: "Not run: the poweroff countdown of an Alarm runs." |
| Power countdown | A power response of this or another Graph shows the countdown dialog of `../widgets.md` with "Overheat: power off in 30 s" and the button "Cancel" |
| Response pause | Banner 3 of the shell. "Run once" still works |
| No access | The status line shows: "nicecooling has no access to the daemon. The Graph sensors have no value." |
| Alarm | Not used. The poweroff banner shows above |

## Interactions

Mouse:

- A drag on the empty canvas pans. The wheel zooms from 25% to 400%, in steps of a factor 1.2, toward the pointer.
- A drag on a node moves it. One drag is one undo step.
- A drag from an output pin to an input pin connects them. An input pin takes one connection. An output pin takes any number. Escape cancels the drag.
- A click selects a node or a connection. A click on the empty canvas clears the selection.

Keyboard:

- Tab order: the toolbar, the search field, the node list, the canvas, the parameter panel, the zoom buttons.
- Node list: Up and Down move. Enter adds the kind at the centre of the view. Left and Right fold and unfold a family.
- Below 1280 px the toolbar has "Add node" in the place of the two list stops. Enter or Space on it opens the popover with the focus in the search field. Tab and the keys of the node list work inside the popover. Escape closes it and gives the focus back to "Add node".
- The canvas is one focus stop with the focus ring. The arrow keys select the nearest node in that direction, and the view follows it. Alt with an arrow key moves the selected node by 12 units. Enter moves the focus to the parameter panel. Escape clears the selection.
- Delete removes the selected node with its connections, or the selected connection. It does not remove the end node.
- Ctrl+Z is undo and Ctrl+Shift+Z is redo, 100 steps. They work in every part of the editor except inside a text field with an open edit.
- Ctrl with "+" and Ctrl with "-" zoom. Ctrl+0 puts the whole Graph in the view.
- A connection without the mouse: select the node, then set the pin in "Inputs" of the panel.

Leaving:

- The back button, a sidebar entry and "Quit" ask when the draft is not applied: see "Leave with a draft".
- Hiding the window keeps the editor and the draft.
- The end of the session discards the draft.

## Copy

| Place | Text |
|---|---|
| Toolbar | "Graphs", "Add node", "Undo", "Redo", "Try", "Apply", "Not applied", "Applied 21:44:02" |
| Try banner | "Try running, nothing is published", "24 s left", "Stop" |
| Node list | "Search nodes", "Sources", "Math", "Logic", "State and time", "Filters", "Generators", "Other", "No node kind has this name." |
| New Graph | "Double-click a node in the list to add it.", "Press Add node to add a node." |
| Panel | "Label", "Inputs", "None", "Test value", "For", "Delete node", "End node", "Sensor", "Response", "Unit", "Graph sensor", "Used by", "Not used", "Created at Apply" |
| Response panel | "Kind", "Hold time", "Repeat every", "Minimum interval", "Countdown", "Run once", "Last runs", "No run yet." |
| Status line | "Zoom out", "Zoom in", "The node values are not available.", "Retry" |
| Unbound source | "Sensor not found" |

Text of a broken rule. The validator gives the codes.

| Code | Text |
|---|---|
| `no-end-node`, `two-end-nodes` | "A Graph has one end node" |
| `end-node-output` | "An end node has no output" |
| `unknown-kind` | "Unknown node kind: quantum" |
| `unknown-pin` | "Unknown pin: x" |
| `pin-occupied` | "input already connected" |
| `same-direction` | "connect an output pin to an input pin" |
| `same-node` | "a node cannot connect to itself" |
| `loop` | "the connection closes a loop" |
| `loop-across-graphs` | "The Graph reads its own Graph sensor through the Graph 'Hot water'" |
| `limit-graphs` | "The limit is 64 Graphs" |
| `limit-nodes` | "The limit is 256 nodes in a Graph" |
| `limit-name` | "The name has 64 characters at most" |
| `limit-samples` | "The filters of all Graphs hold more than 200 000 samples. Make a span shorter." |
| `limit-graph-sensors` | "The limit of 64 Graph sensors is reached" |
| `duplicate-label` | "Two nodes have the label water" |
| `duplicate-table-input` | "Two pairs of the table have the input 20" |
| `bad-parameter` | The name of the node, the name of the parameter and its range: "On delay: Delay is 0 to 3600 s" |
| `bad-sensor-id` | "The Graph sensor id of this Graph is not valid" |

The four texts in lower case are reasons of a refused connection. The status line shows them after "Not connected: ".
