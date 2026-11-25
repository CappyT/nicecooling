# Graphs

Comp: `../comps/graphs.html` (1280x800; also 1100x700 and Italian in `../comps/shots/`).

A Graph is a named set of nodes and connections that the app evaluates once per second. It ends in one end node. A sensor end node gives a Graph sensor. A response end node gives a Graph response. The screen lists the Graphs of the user and opens the Graph editor (`graph-editor.md`). It is the entry "Graphs" in the group "Control" of the sidebar.

## Layout

From the top:

1. A button row: "New sensor" and "New response" at the left, "Import" at the right.
2. Card "Graphs": one table row per Graph, in the order of `graphs.json`.
3. Card "Graph sensors without a Graph": only when the daemon holds a Graph sensor that no Graph of the user holds.

Columns of the card "Graphs". A row is 48 px high.

| Column | Content |
|---|---|
| Graph | The name. The caption is the count of the nodes: "7 nodes". The name takes the rest of the row and truncates with an ellipsis |
| End node | "Sensor" or "Response". The caption is the id of the Graph sensor in mono text ("graph_3"), or the kind of the response ("Poweroff") |
| State | "Running", "Disabled", "Invalid" or "Paused": see "States" |
| Result | The live result. A Graph with a sensor end node: the value and the unit ("46.2 °C"), or "--". A Graph with a response end node: the tag "Fired" or the text "Not fired", and the caption "Last run 14:05:09". A last run of another day has the date and no seconds: "Last run 2026-11-24 21:40". A response that never ran has no caption |
| Buttons | The enable switch, "Edit", "Export", "Delete" |

The card "Graph sensors without a Graph" is a list. A row has the name of the sensor with its id in mono text as caption, its value, and the button "Remove".

## States

| State | What the user sees |
|---|---|
| Empty | "No Graph. A Graph computes a sensor or runs a response from nodes." with the buttons "New sensor" and "New response". The card has no table |
| Loading | Table frame with grey bars |
| Running | State "Running". The app evaluates the Graph once per second. The result changes once per second, without animation |
| Fired | A Graph whose response end node is in the state "run": the tag "Fired" and the caption "Last run 14:05:09". The state returns to "Not fired" when the input becomes false |
| Sensor without value | Result "--". The state stays "Running" |
| Disabled | State "Disabled" in `fg-dim`. The switch is off. A Graph with a sensor end node shows "--". A Graph with a response end node shows "Not fired" and its last run |
| Invalid | A stored Graph that does not parse, has an unknown node kind or breaks a rule of the validator. State: the Warning icon and "Invalid". The caption of the name is the reason: "Unknown node kind: quantum", or the text of the first broken rule (`graph-editor.md`, "Copy"). The Graph runs nothing. The switch is disabled. "Edit" and "Export" are disabled for a Graph that does not parse or has an unknown node kind, tooltip: "The app cannot read this Graph." "Delete" works |
| Response pause | Banner 3 of the shell. Each enabled Graph with a response end node has the state "Paused". The Graphs with a sensor end node stay "Running" |
| Stopped after a fault | The app disables a Graph whose evaluation fails, and shows the notification "Graph B stopped". The row is a disabled row |
| Graph sensor without a Graph | The second card. Text under its header: "No Graph of this user holds these Graph sensors. A Graph sensor can belong to another user." |
| Daemon offline | Banner 2 of the shell. A sensor result is "--" in `fg-dim`. For a Graph with a sensor end node, "Delete" is disabled, and the switch is disabled when the Graph has no Graph sensor yet. Tooltip: "The daemon is offline." A Graph with a response end node keeps every control: the app holds it. "Remove" is disabled |
| No access | A warning row above the table: "nicecooling has no access to the daemon. The Graph sensors have no value. The app tries again in 60 s." It shows after polkit denies the Graph sensor values |
| Graph file not readable | In place of the two cards: the Warning icon, "nicecooling cannot read graphs.json." with the error text, and the button "Retry". The three buttons of the button row are disabled. The app writes no Graph file in this state. A file with a higher schema version has the text "graphs.json has schema version 2. This app reads version 1." |
| Limit of 64 Graphs | "New sensor", "New response" and "Import" are disabled. A caption next to them says "The limit is 64 Graphs." |
| Delete refused | The Graph stays. The caption of its name becomes "Not deleted: the Curve of Chassis Fan 1 uses its Graph sensor." in the Warning color with the Warning icon. It names the Curve, the Profile rule or the Software sensor slot that the daemon reports |
| Enable refused | The switch stays off. The caption of the name becomes "Not turned on: the limit of 64 Graph sensors is reached.", or the reason of the daemon |
| Imported | The new Graph is the last row, disabled, and has the caption "Imported. Check it, then turn it on." until the user opens it or turns it on |
| Import refused | A warning row above the table with the reason, and a close button: see "Copy" |
| Alarm | Not used. The poweroff banner shows above |

## Interactions

- "New sensor" and "New response" open the Graph editor on a draft with one end node of that kind. The draft has the name "Graph 4": the lowest free number. The app stores nothing before "Apply".
- "Edit" opens the Graph editor on the Graph. A double click on a row and Enter do the same.
- The switch enables or disables the Graph at once. For a Graph with a sensor end node and no Graph sensor, the app creates the Graph sensor first.
- "Export" opens the file dialog of the system with the name of the Graph as file name. The file holds no Graph sensor id, no stored value, no run state and no password.
- "Import" opens the file dialog of the system. The Graph gets a new id and is added disabled. A sensor source that this machine does not have stays in the Graph as an unbound source.
- "Delete" asks: "Delete the Graph 'Render temperature'?" For a Graph with a sensor end node the dialog adds: "Its Graph sensor is removed too." Buttons: "Cancel", "Delete Graph". The default focus is on "Cancel".
- "Remove" asks: "Remove the Graph sensor 'Old delta T'? No Graph of this user holds it. It can belong to another user." Buttons: "Cancel", "Remove". A sensor in use is not removed, and the row names its user.
- No command of this screen asks a password.

## Copy

| Place | Text |
|---|---|
| Button row | "New sensor", "New response", "Import" |
| Columns | "Graph", "End node", "State", "Result" |
| End node | "Sensor", "Response" |
| State | "Running", "Disabled", "Invalid", "Paused" |
| Result | "Fired", "Not fired", "Last run 14:05:09" |
| Row buttons | "Edit", "Export", "Delete", "Remove" |
| Second card | "Graph sensors without a Graph" |
| Export done | "Exported to ~/Documents/Render temperature.json" |
| Import refused | "Not imported: the file is not a nicecooling Graph." / "Not imported: the file has format version 2. This app reads version 1." / "Not imported: the file is larger than 1 MiB." / "Not imported: " and the text of the first broken rule |
