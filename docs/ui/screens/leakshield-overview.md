# Leakshield: Overview tab and Action views

Comp: `../comps/leakshield-overview.html` (1280x800; also 1100x700 and Italian in `../comps/shots/`).

Every Leakshield page has the same frame: the header bar with the device label as title and "S/N" plus the serial as subtitle, then the tab strip: Overview, Settings, Sensors, Display, Lighting, Alarms, Status, System, Fields. Tabs that do not fit go into a "More" menu.

## Layout

1. Row of two cards.
   - "Pressure": the pressure gauge (see `../widgets.md`) with the tag "In range".
   - "Status": a status tag ("Protecting"), then rows: "Fill level" with the time of the measurement, "Volume change" in ml/h, "Pressure change" in mbar/min, "Feed" with its age.
2. Card "Mode and Actions".
   - Mode selector: three toggle buttons, SHIELD, MONITOR and RELEASE. RELEASE has the padlock.
   - Action bar: "Test", "Measure fill level", "Deaerate", "Refill".
   - A caption with the meaning of the selected Mode.
   - The Refill instruction row: it says that Refill can also start on the device.
3. Row of two cards: "Pressure" (area chart with 1 h, 24 h and 7 d, the target as a dashed line, minimum and maximum as dotted lines) and "Pump runtime per day" (bar chart of 14 days, with a dashed line at 15 s).

While an Action runs, the Action view replaces item 3, and the Action bar shows only "Cancel".

## Action views

| Action | View |
|---|---|
| Test, setup | Number input "Test pressure" from 80 to 450 mbar, text "Stop the pump before a Test: a running pump changes the pressure.", buttons "Start Test" and "Cancel" |
| Test, running | Line chart of the selected source. Select "Source": "Pressure", "Difference to the reference", "Pressure change". Buttons "Reference", "Reset zoom", "Cancel". Drag in the chart zooms |
| Measure fill level | One progress bar with the step ("Releasing the pressure", "Building 100 mbar"), then the result "Fill level 71 %". Warning banner while it runs |
| Deaerate | Two progress bars: "Current cycle" and "Overall", with "Cycle 3 of 12". Button "Cancel" |
| Refill | Not a view. The button starts the Refill on the device. The instruction row opens a dialog with the four steps of the start on the device |

## States

| State | What the user sees |
|---|---|
| Empty | No value yet after connect: "Reading the Leakshield" with a spinner |
| Loading | The same |
| Leakshield unreachable | Offline banner "The Leakshield does not answer. Check its USB cable." The gauge has no marker. Mode and Actions are disabled. After 10 s without data nicecooling raises the Alarm "Leakshield unreachable" |
| Unknown firmware | Banner 3 of the shell. Mode and Actions are disabled, tooltip: "Read-only: unknown firmware." |
| Mode not verified | The Mode selector shows the Mode and is disabled. Caption: "Read-only: Mode is not a Verified field yet." with "Open Fields" |
| Locked | RELEASE has the padlock. The click opens the polkit dialog only for a user outside `wheel`. After a denial: "Not authorized. The Mode did not change." |
| Warning | The status tag is a warning tag ("Leak Warning"). The row that caused it has the triangle |
| Alarm | The status tag is the Alarm tag. The gauge marker and value are red. The poweroff banner shows when the alarm response is poweroff |
| Action runs | Action view and "Cancel" |
| Mode RELEASE | Warning banner: "Mode RELEASE: the Loop has no leak protection." |
| Pump runs (Refill) | The click on "Refill" opens the warning "Start Refill?". The instruction row always shows |

## Interactions

- A Mode change asks for confirmation when it lowers the protection: "Change the Mode to MONITOR? MONITOR detects leaks and does not prevent them." Buttons: "Cancel", "Set MONITOR".
- A Mode change is a Leakshield write: the write status shows under the selector.
- "Test" opens the setup. "Measure fill level" asks first: "During the measurement the Loop has no leak protection. Do not start it when the Loop can leak." Buttons: "Cancel", "Measure".
- "Refill" starts the Refill at once while every Pump channel of the Loop reads 0 rpm. When a Pump channel reads above 0 rpm or has no tach value, the click opens the warning "Start Refill?". The user decides: "Start Refill" sends the Refill.
- The chart range buttons change both the chart and its scale.

## Copy

| Place | Text |
|---|---|
| Mode captions | "SHIELD: full protection by negative pressure, and leak detection." / "MONITOR: low negative pressure, leak detection only." / "RELEASE: the Leakshield is off and the pressure equalizes." |
| Action hint | "An Action is temporary: when it ends, the Leakshield goes back to its Mode." |
| Refill row | "Refill can also start on the device:" Link: "Show the steps" |
| Refill warning | Title "Start Refill?". Text "The pump runs. The Leakshield manual asks for the pump off during Refill." Buttons "Cancel", "Start Refill" |
| Refill steps | "1. Put the PC in standby." / "2. Hold the Leakshield button for more than 1 s and select FILL." / "3. Put the fill hose in the coolant. The Leakshield sucks the coolant in." / "4. Select EXIT. The Leakshield goes back to its Mode." |
| Measure banner | "The fill level measurement runs. The Loop has no leak protection until it ends." |
| Pump runtime | "15 s per day: the limit of a tight Loop", "Average 8 s" |
