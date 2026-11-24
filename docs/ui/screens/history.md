# History

No comp. The chart follows the pressure chart of `../comps/leakshield-overview.html`.

The screen charts any set of sensors over time. The app records the data, so the history has gaps while no session ran the app.

## Layout

1. Toolbar: the sensor picker in multi-select mode ("4 sensors"), the range button group ("1 h", "24 h", "48 h", "7 d", "30 d", "1 y"), and the buttons "Back", "Forward", "Now" and "Reset zoom".
2. The history chart: it fills the content. One y axis per unit, at most two axes shown (left and right). Sensors with a third unit get their own chart under the first.
3. Legend under the chart: one row per sensor with its series color, name, and the value at the cursor: "Water temperature 31.4 °C, min 30.9, max 31.8".

Chart rules:

- Up to 48 hours back the chart draws the 1 s samples. Older data are 1 minute aggregates: the line is the mean, and a band of the series color at 20% opacity goes from the minimum to the maximum.
- A gap in the data is an empty region with a hatched ground and the label "No data: the app did not run". Lines do not cross a gap.
- Warnings and Alarms are vertical markers at their time, with the severity icon at the top.
- Series use the colors `s1` to `s6` in order. A seventh series repeats `s1` with a dashed line.

## States

| State | What the user sees |
|---|---|
| Empty | No sensor selected: "Select sensors to chart." with the picker open |
| No data | The range has no sample: "No data in this range. The app records the history while it runs." |
| Loading | The axes show at once. The lines appear when the query ends. A thin progress bar runs under the toolbar after 300 ms |
| Daemon offline | The chart still works: the history is in the app. The live edge stops, and the offline banner shows |
| Database error | "The history database cannot be read." with the path and the button "Try again" |
| Alarm | Markers in the chart. The poweroff banner shows above when a countdown runs |

## Interactions

- Drag in the chart selects a time range and zooms to it. The mouse wheel zooms around the cursor. Shift and drag pans. Double click resets the zoom.
- Left and Right move the cursor by one sample. Plus and Minus zoom. Home goes to the start of the range, End to now.
- Hover shows a vertical cursor line and the values in the legend.
- A click on a legend row hides or shows that series.
- With "Now" active the chart follows the live values.
- A click on an Alarm marker opens the Alarms screen at that entry.

## Copy

| Place | Text |
|---|---|
| Toolbar | "Sensors", "1 h", "24 h", "48 h", "7 d", "30 d", "1 y", "Now", "Reset zoom" |
| Aggregate hint | "Older than 48 hours: 1 minute mean, with minimum and maximum." |
| Gap label | "No data: the app did not run" |
