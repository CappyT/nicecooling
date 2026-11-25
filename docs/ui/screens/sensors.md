# Sensors

No comp. The table follows the Channel list of `../comps/channels.html`.

The screen lists every sensor that nicecooling reads, lets the user name and hide sensors, defines Derived sensors and Calibration tables, and opens the Graph of a Graph sensor.

## Layout

One table, grouped by device. Group rows: "Board chips" (one group per chip, with the driver name as caption), "Leakshield", "GPU", "NVMe", "Derived sensors", "Graph sensors". A group row folds its sensors.

| Column | Content |
|---|---|
| Sensor | Name. A user name shows the Board file name as caption |
| Value | Live value and unit, right-aligned |
| Last 10 minutes | A line chart at row height, 120 px wide, with one shared scale per unit inside a group |
| Used by | "Curve of Waterpump", "Main loop", "Rule 3", or empty |
| Buttons | Rename, hide. A Graph sensor also has the link "Open Graph", or the button "Remove" when no Graph holds it |

Header: a search field, the switch "Show hidden sensors", and the button "New Derived sensor".

A flow sensor row opens a panel under it with the Calibration table editor, the same as in Loops.

## Graph sensors

The group "Graph sensors" lists the Graph sensors of the daemon. A Graph of the app computes each one (`graphs.md`).

- The group row has the link "New sensor" at its right. It opens the Graph editor on a new Graph with a sensor end node. The group row shows also without a Graph sensor, with the text "No Graph sensor."
- The name of a Graph sensor is the name of its Graph. The caption is its id in mono text: "graph_3".
- "Open Graph" opens the Graph editor on the Graph of the sensor (`graph-editor.md`).
- A Graph sensor that no Graph of this user holds has the caption "No Graph of this user holds this sensor." and the button "Remove" in place of "Open Graph". "Remove" asks, as on the Graphs screen.
- "New Derived sensor" still opens the Derived sensor editor. A Graph does not replace a Derived sensor.

## Derived sensor editor

A dialog, 480 px wide:

1. "Name": input.
2. "Function": select with "Maximum", "Mean", "Difference".
3. "Sources": two or more sensor pickers. "Difference" has exactly two: "Minuend" and "Subtrahend". All sources must have the same unit.
4. Live result: "Now 46.2 °C".
5. Buttons: "Cancel", "Save".

## States

| State | What the user sees |
|---|---|
| Empty | No sensor found: "No sensor found. Check that the hwmon drivers of the Board are loaded." |
| Loading | Group rows with grey bars |
| Daemon offline | Offline banner. Values are dim. Rename and hide are disabled |
| Sensor without value | "--" and the dashed-circle icon. The chart ends where the value stopped |
| Hidden sensor | Absent. With "Show hidden sensors" it shows dim, with the button "Show" |
| Sensor in use | Hide is disabled, tooltip: "A Curve uses this sensor." |
| Derived sensor with a missing source | "--" and the Warning icon. Caption: "Source without value: GPU temperature" |
| Graph sensor without value | "--" and the dashed-circle icon, never a number. This is the case while its Graph is disabled, while a source of the Graph has no value, and while the app sends no value |
| Graph sensor in use | "Remove" does not remove it. The row names its user: "The Curve of Chassis Fan 1 uses this sensor." |
| Mixed units in the editor | "Save" is disabled. Text: "All sources must have the same unit." |
| Alarm | A sensor that caused an active Alarm or Warning has the severity icon before its name |

## Interactions

- Rename: a popover with one input. An empty name restores the Board file name.
- A click on a row selects it. Enter opens History with this sensor selected.
- Delete of a Derived sensor that a Curve uses is refused: "The Curve of Chassis Fan 1 uses this sensor."

## Copy

| Place | Text |
|---|---|
| Header | "Search sensors", "Show hidden sensors", "New Derived sensor" |
| Graph sensors | "Graph sensors", "New sensor", "Open Graph", "Remove", "No Graph sensor.", "No Graph of this user holds this sensor." |
| Columns | "Sensor", "Value", "Last 10 minutes", "Used by" |
| Function help | "Maximum: the highest source." / "Mean: the average of the sources." / "Difference: the first source minus the second." |
