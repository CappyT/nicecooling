# Leakshield: Status tab

Comp: `../comps/leakshield-status.html` (1280x800).

The tab shows what the Leakshield does now: its operating data, its internal parts and its events.

## Layout

1. Card "Operating data": four radio tiles in a row: "Status", "Pressure", "Volume change", "Pressure change". Each tile has its live value. The selected tile has the accent tint, and its value is charted below over the last 10 minutes. The Status tile charts the state as bands.
2. Card "Schematic": the device schematic, 176 px high (see `../widgets.md`). It uses the notation of the Loop diagram: dashed air lines, part symbols, bubbles for the pressure and the fill level. An active part is filled and has a label with its state.
3. Card "Events": a table with "Time", "Event" and "Details", newest first, and the button "Export".

## States

| State | What the user sees |
|---|---|
| Empty | No event yet: "No event since the Leakshield got power." |
| Loading | "Reading the Leakshield" with a spinner |
| Leakshield unreachable | Offline banner. The schematic is dim, bubbles are dashed with "--". The event list keeps its last content |
| Idle | No part is filled. Status: "Protecting" |
| Pump runs | The vacuum pump and the pump valve are filled, with "running" and "open". Status: "Building pressure" |
| Action runs | Status: "Test", "Measuring the fill level", "Deaerating", "Refill" |
| Warning | The value that caused it has the triangle in its tile. The event list has a row with the triangle |
| Alarm | The PT bubble is red. The event row has the octagon. The poweroff banner shows when the alarm response is poweroff |
| Unknown firmware | The values that nicecooling can still decode show. The others show "--". Banner 3 |
| Locked | Not used |

## Interactions

- A click on a tile, or Left and Right with focus on the tiles, selects the charted value.
- The line between the chart and the schematic is a drag handle: it changes the chart height between 84 and 240 px.
- "Export" saves the event list as a text file through the file dialog of the system. The file has the serial, the firmware version and one line per event.
- A click on an event row with a Warning or an Alarm opens the Alarms screen at that entry.

## Copy

| Place | Text |
|---|---|
| Card captions | "select a value to chart it", "active parts are filled and labelled" |
| Part labels | "Atmosphere", "Vent valve, closed", "Pump valve, open", "Vacuum pump, running", "Membrane", "Reservoir", "Fill level" |
| Status values | "Protecting", "Building pressure", "Monitoring", "Released", "Test", "Measuring the fill level", "Deaerating", "Refill" |
| Event examples | "Vacuum pump started" / "Pressure 281 mbar, below the target band"; "Fill level measured" / "71 %, automatic measurement"; "Mode changed" / "MONITOR -> SHIELD, by nicecooling" |
| Schematic note | "The part list comes from the device protocol. Until the protocol work confirms it, the schematic is a draft." (docs only, not UI) |
