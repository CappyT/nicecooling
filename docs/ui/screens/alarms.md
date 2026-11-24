# Alarms

Comps: `../comps/countdown-dialog.html` (poweroff countdown dialog), `../comps/overview.html` (poweroff banner).

The screen lists the active Warnings and Alarms, their history, and the alarm response of every Warning and Alarm type.

## Layout

Three cards.

1. "Active": a table with the severity icon, "Event", "Source", "Since", "Value" (the value and its limit), "Alarm response". "Event" takes the rest of the row and is 160 px wide at least; the other cells keep their width. An event with a running countdown shows "Poweroff in 0:21" and the button "Cancel poweroff" on a second line of its row.
2. "Alarm responses": one row per Warning and Alarm type of every source. Columns: "Event", "Source", "Alarm response" (select: "None", "Notify", "Poweroff after a delay"), "Delay" (seconds). Groups: "Leakshield" with the default of each Leakshield event (source `""` of `GetResponses`), then one group per known Leakshield ("Leakshield 20417-31072") with its resolved rows, then "nicecooling" with "Pump stopped" (Alarm), "Leakshield unreachable" (Alarm) and "Input lost" (Warning). A change in a Leakshield row is an override for that Leakshield. The link "Reset" shows on a Leakshield row with an override, and on a default row that differs from the built-in default.
3. "History": a table with "Time", "Event", "Source", "Duration", "Response", newest first, with a filter select ("All", "Alarms", "Warnings"). "Event" takes the rest of the row and is 160 px wide at least. A long text in "Duration" or "Response" wraps in its cell. The rows come from `GetHistory`: the daemon keeps them across app and daemon restarts. The app reads them again on `AlarmRaised`, `AlarmCleared`, `PoweroffCancelled` and `PoweroffFailed`. The "Response" column always shows the outcome, also on a row that a daemon restart closed: "PC powered off" stays readable after the reboot that the poweroff caused. On such a row (`closed_by_restart`) the "Duration" column shows "Closed by a daemon restart" in place of the duration.

## Poweroff countdown dialog

See "Countdown dialog" in `../widgets.md`. It opens with the window when an Alarm with the poweroff response is raised. The default focus is on "Cancel poweroff".

## States

| State | What the user sees |
|---|---|
| Empty, active | "No active Alarms" with a check icon |
| Empty, history | "No Alarm and no Warning yet." |
| Loading | Table frames with grey bars |
| Daemon down | "Active" lists "Daemon down" with the octagon, source "nicecooling", and the text "nicecooling protection is off." The app makes this entry itself, because the daemon cannot |
| Daemon offline or down | Banner 2. The alarm response table is disabled. Text: "The daemon runs the alarm responses. While it does not run, no alarm response runs." |
| Warning | Row with the triangle. No banner |
| Alarm, notify | Row with the octagon. Desktop notification. No countdown |
| Alarm, poweroff | Row with the countdown, the poweroff banner, the dialog and the notification |
| Poweroff cancelled | The row says "Poweroff cancelled 21:47:02. The Alarm is still active." The banner goes away. The dialog closes |
| Pump-stop Alarm | Row "Pump stopped", source "Main loop: Waterpump". Value "0 rpm at Duty 62 %". Caption: "Every Channel is at 100 %. The Alarm closes when the pump runs for 10 s." |
| Defaults | "Large leak" and "Pump stopped": "Poweroff after a delay", 30 s. Every other row, "Leakshield unreachable" included: "Notify" |
| Leakshield unreachable | Row with the octagon, source "Main loop: Leakshield", value "No data for 14 s". It closes when data arrive again |
| Delay 0 | Warning under the row: "With 0 s you cannot cancel the poweroff." |
| Overlap with a Device response | Warning icon in the row. Tooltip: "The Leakshield also reacts to this Alarm. See its Alarms tab." |

## Interactions

- A click on an active row opens its source.
- A change in the alarm response table applies at once, with the feedback "Applied".
- "Cancel poweroff" cancels the pending poweroff of that Alarm. The Alarm stays active, and the same Alarm does not start a new countdown until it closes and is raised again.
- A history row opens History at that time, with the sensor of the Alarm selected.

## Copy

| Place | Text |
|---|---|
| Events, Leakshield | "Leak Warning", "Small leak", "Large leak", "Fill level Warning", "Fill level Alarm", "System Alarm" |
| Events, nicecooling | "Pump stopped", "Leakshield unreachable", "Input lost" |
| Alarm response values | "None", "Notify", "Poweroff after a delay" |
| Response, history | "None", "Notified", "Poweroff pending", "Poweroff cancelled by the user", "Poweroff stopped: the Alarm closed", "PC powered off" (`poweroff-requested`), "Poweroff failed". Duration column of a row with `closed_by_restart`: "Closed by a daemon restart" |
| Dialog | "Large leak Alarm", "The PC powers off when the countdown ends.", "seconds left of 30", "Cancel poweroff", "Hide", "The countdown continues in the banner." |
