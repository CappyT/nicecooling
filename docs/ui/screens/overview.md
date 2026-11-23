# Overview

Comps: `../comps/overview.html` (Alarm, 1280x800), `../comps/overview-min.html` (two Loops, 1100x700), `../comps/daemon-down.html`, `../comps/countdown-dialog.html`.

The Overview answers one question: is every Loop safe now. It is the screen that opens on an Alarm.

## Layout

From the top:

1. Global banners (see `app-shell.md`).
2. One card per Loop, full width. Header: Loop name, Mode tag when the Loop has a Leakshield, a caption ("Leakshield, 1 Pump channel"), and the link "Open Loop". Body: the Loop diagram, 168 px (see `../widgets.md`).
3. Pressure card, only while a Leakshield Alarm or Warning is active: the value that caused it over the last 10 minutes, with its limit as a dashed line and a vertical line at the time of the Alarm.
4. A row of three cards: "Active Alarms", "Key temperatures", "Active Profile".

- Key temperatures: CPU, GPU, chipset and VRM. A sensor that the Board does not have is absent.
- Active Profile: the Profile name, a tag "By rule" or "Manual", and the rule that activated it.
- Two Loops and the row of three cards fit in 1100x700 without a scroll. With an Alarm banner or a third Loop the page scrolls, and the banner stays in place.

## States

| State | What the user sees |
|---|---|
| Empty | No Loop exists: one card "No Loop yet" with the text "A Loop groups a pump, a flow sensor, a water temperature sensor and a Leakshield." and the button "Set up a Loop". Key temperatures still show |
| Loading | Card frames with grey bars in place of the values, for at most 2 s |
| Daemon offline | Offline banner. Bubbles have a dashed outline and "--". The caption of each card says "Last value 21:40:12" |
| Sensor without value | That bubble is dashed and shows "--". The others stay live |
| Flow above the Calibration table | The FT bubble is normal. Its caption has an information icon and "above table range". No Warning exists for this: nothing shows in "Active Alarms" |
| Daemon down | The red banner "The daemon is down. nicecooling protection is off." Every bubble is dashed with "--" and its last value as caption. The Loop header has the tag "Not live" and "Last values 21:52:07". "Active Alarms" lists "Daemon down". Key temperatures show "--". The Profile card says "None" |
| Warning | The bubble of the part is amber with a triangle before its caption. The Warning is a row in "Active Alarms". No banner |
| Alarm | The bubble of the part is red. The pressure card shows. "Active Alarms" lists the Alarm with its alarm response |
| Leakshield unreachable | No data for 10 s: the PT bubble is dashed with "--", the fill level is absent, and "Active Alarms" lists the Alarm "Leakshield unreachable". The pump, flow and temperature bubbles stay live |
| Alarm with poweroff | The same, plus the poweroff banner and the countdown dialog |
| Action runs | The Mode tag is replaced by "Test running" |
| Mode RELEASE | The Mode tag is a warning tag "RELEASE". The PT caption says "no protection" |
| Locked | Not used: the Overview has no locked control |
| Loop without Leakshield | The diagram has no reservoir and no PT bubble. Text: "No Leakshield" and "Pressure is not measured." |

## Interactions

- A click on a bubble opens its source: PT and the reservoir open the Leakshield, ST opens the Channel, FT and TT open the sensor.
- "Open Loop" opens the Loop in Loops.
- A row in "Active Alarms" opens the source of the Alarm. "Open Leakshield" and "Open Profiles" are the last rows of their cards.
- "Cancel poweroff" is in the banner and in the dialog. Both cancel the pending poweroff.
- Hover on a bubble shows the limits and the age of the value.

## Copy

| Place | Text |
|---|---|
| Loop caption | "Leakshield, 1 Pump channel" / "No Leakshield, 2 Pump channels" |
| PT caption | "target 300" / "below minimum 250" / "above maximum 350" / "no protection" |
| ST caption | "Duty 62 %" / "stopped" |
| FT caption | "calibrated" / "above table range" (flag, with the information icon) / "below 40 L/h" |
| TT caption | "water" |
| Pressure card | "Pressure", "Leakshield, last 10 minutes", "Target 300 mbar", "Minimum 250 mbar", "Alarm raised 21:46:51" |
| Active Alarms, empty | "No active Alarms" |
| Alarm row | "Large leak" and "Leakshield, since 21:46:51. Alarm response: poweroff after 30 s." |
| Warning row | "Input lost" and "Chassis Fan 3, since 20:58. Alarm response: notify." |
| Daemon down row | "Daemon down" and "Since 21:52:07. The daemon runs the alarm responses: none runs now." |
| Profile card, rule | "Rule 2: time window 22:00 to 07:00." |
| Profile card, manual | "Profile rules wait until you revoke it." |
