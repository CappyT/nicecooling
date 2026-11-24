# Loops

No comp. The screen uses the patterns of `../comps/channels.html` (list and detail) and `../comps/profiles.html` (rows with controls).

A Loop is one water circuit: its Pump channels, its flow sensor, its water temperature sensor and, when present, its Leakshield.

## Layout

List column (292 px): one row per Loop with its name and a caption ("Leakshield, 1 Pump channel"), and the button "New".

Detail column, from the top:

1. Header: Loop name with a rename button, and a menu with "Delete Loop".
2. Preview: the Loop diagram of this Loop, live.
3. Card "Parts": one row per part, each with a picker.
   - "Pump channels": one or more Channels. Each chosen Channel gets the "Pump channel" tag everywhere.
   - "Flow sensor": sensor picker, limited to sensors with a pulse rate. "None" is a choice.
   - "Water temperature": sensor picker. "None" is a choice.
   - "Leakshield": a select with the connected devices that no other Loop uses, and "None".
4. Card "Pump floor": a slider from 0 to 100% with a number input, and the text below.
5. Card "Calibration table", only with a flow sensor: the Calibration table editor (see `../widgets.md`).
6. Card "Feed", only with a Leakshield: the pump speed and the flow of the last good send (`LastFeed`) and its age.

## States

| State | What the user sees |
|---|---|
| Empty | "No Loop yet" with the button "Set up a Loop", which opens the First run Loop step |
| Loading | Row frames with grey bars |
| Daemon offline | Offline banner. Pickers and the slider are disabled |
| No Pump channel | Warning row in "Parts": "A Loop needs a Pump channel. Without one, nicecooling cannot see a pump stop." |
| Pump floor at 0 % | Warning under the slider: "With a Pump floor of 0 %, a Curve can stop the pump." |
| Feed not sent yet | `FeedState` is `waiting` (whatever `LastFeed` holds: after a reconnect it can keep an older send), or `off` with no send yet (`LastFeed` is zero). The values show "--" and the age text is "Not sent yet" |
| Feed stale | The age is in the Warning color after 5 s, and while `FeedState` is `failing` or `off`: "Last Feed 12 s ago" |
| Flow out of range | The live marker of the Calibration table is in the dashed part. Text: "671 rpm is above the last point. The flow is an estimate." |
| Alarm | A pump-stop Alarm shows as a red row at the top of the detail: "Pump stopped: Waterpump reads 0 rpm. Every Channel is at 100 %." |

## Interactions

- A change of a part applies at once and shows write feedback in its row.
- The slider moves in steps of 1%. The number input accepts 0 to 100.
- "Delete Loop" asks: "Delete the Loop 'Main loop'? Its Channels keep their Curves. The Pump floor and the pump-stop Alarm stop." Buttons: "Cancel", "Delete Loop".
- "Load preset" in the Calibration table offers "Alphacool 17558". It replaces the table after a confirmation.

## Copy

| Place | Text |
|---|---|
| Pump floor text | "The Pump floor is the minimum Duty of the Pump channel. No Curve and no Profile goes below it. It keeps the water moving when a Curve reads a low temperature." |
| Feed card | "Feed", "Pump speed 2 840 rpm", "Flow 115 L/h", "Sent 1 s ago" / "Last Feed 12 s ago" / "Not sent yet". Caption: "The Leakshield uses the Feed to compute the pressure of the Loop." |
| Leakshield row, none | "None. Pressure and fill level are not measured, and no Feed is sent." |
| Calibration table header | "Raw rate (rpm)", "Flow (L/h)" |
