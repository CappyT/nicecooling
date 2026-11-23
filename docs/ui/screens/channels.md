# Channels

Comp: `../comps/channels.html` (1280x800; also rendered at 1100x700 and with Italian strings in `../comps/shots/`).

A Channel is one fan or pump output of the Board. The screen lists the Channels and edits the control of one Channel.

## Layout

List column (360 px): a table with the columns "Channel", "rpm" and "Duty". Each row has the Channel name and a second line with its control: "Curve on Water temperature", "Fixed Duty", "Hardware curve, not controlled". A Pump channel has the "Pump channel" tag. The second line wraps when it carries a state.

Detail column, one card, from the top:

1. Header: Channel name, rename button, "Pump channel" tag.
2. "Control": a button group with "Curve", "Fixed Duty" and "Hardware curve".
3. With "Curve": three fields in a row: "Input" (sensor picker with the live value), "Input smoothing" (seconds of average, default 5), "Duty step limit" (percent per second, default 5).
4. The Curve editor, 292 px high (see `../widgets.md`).
5. Readout row: smoothed Input, Curve output, Duty, and the write feedback at the right.
6. The points as number inputs, then "Add point" and "Remove point" on their own row.
7. "BIOS snapshot": a collapsed row. Open, it shows the Hardware curve of this Channel from `BiosSnapshot` as a table of temperature and Duty (the columns "Input, °C" and "Duty, %"), and the date of the snapshot. A snapshot without auto points shows the text and the date only. The readout row shows the smoothed Input and the Curve output that the daemon reports in `SensorsUpdated`: "--" while it reports none.

With "Fixed Duty", items 3 to 6 are one slider with a number input. With "Hardware curve", they are one text and the snapshot table.

## States

| State | What the user sees |
|---|---|
| Empty | No Channel found: "No Channel found. nicecooling needs a hwmon driver for the fan controller of this Board." and the link "Open Settings" |
| Loading | Table frame with grey bars |
| Daemon offline | Offline banner. The editor shows the last Curve, and its handles are hidden |
| Hardware curve | List: "Hardware curve, not controlled". Detail: "nicecooling does not control this Channel. The Board runs its Hardware curve." |
| No speed signal | List: dashed-circle icon and "no speed signal". The rpm value is dim |
| Lost Input | List: triangle and "Curve, Input lost: GPU temperature", Duty 100 %. Detail: a Warning strip above the plot |
| Pump channel | The hatched Pump floor zone in the plot. A fixed Duty below the floor is refused: "The Pump floor is 40 %." |
| Pump-stop Alarm | Every row shows 100 %. A red row at the top: "Pump stopped. Every Channel is at 100 % until the pump runs for 10 s." The editor is read-only |
| Write failed | Readout row: octagon and "Not applied: the daemon refused the Curve." with "Try again" |
| Locked | Not used: a Curve change is a normal write |

## Interactions

- A click on a row opens the Channel. Up and Down move in the list.
- A change of "Control" applies at once. From "Curve" or "Fixed Duty" to "Hardware curve", a dialog asks: "Give this Channel back to the Board? nicecooling stops its Curve." Buttons: "Cancel", "Use the Hardware curve".
- Curve editing: see "Curve editor" in `../widgets.md`. Each change goes into the active Profile.
- Rename: a popover with one input. Enter saves. An empty name restores the Board file name.

## Copy

| Place | Text |
|---|---|
| Fields | "Input", "Input smoothing", "s average", "Duty step limit", "% per s" |
| Plot | "Duty, %", "Input, °C", "Now 31.4 °C, 62 %", "Pump floor 40 %: no Curve goes below it" |
| Readout | "Input 31.4 °C", "5 s average", "Curve 62 %", "Duty 62 %", "Applied 21:44:02" |
| Lost Input strip | "Input lost: the Channel runs at 100 % until GPU temperature returns." |
| BIOS snapshot | "When the daemon stops, the Board runs its Hardware curve for this Channel." and "Saved 2026-11-20 08:14" |
| Field help | "Input smoothing: the Curve reads the average of the last 5 s, so a short spike does not move the fan." / "Duty step limit: the Duty changes by 5 % per second at most." |
