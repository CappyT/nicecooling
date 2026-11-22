# Leakshield: Sensors tab

No comp. The table follows `../comps/channels.html`, and the pickers follow `../comps/profiles.html`.

The tab names the 18 device sensors and fills the 12 Software sensors.

## Layout

Two cards.

1. "Device sensors": a table with 18 rows. Columns: number, "Name" (inline input), "Value", "Type". Types: "Local" (measured by the Leakshield: pressure, fill level, volume change, pressure change) and "From the Feed" (pump speed, flow). The first six rows are in the order of the device: Pressure, Fill level, Volume change, Pressure change, Pump speed, Flow.
2. "Software sensors": a table with 12 rows. Columns: slot number, "Name", "Data source" (sensor picker), "Unit" (select), "Scale" and "Offset" (number inputs), "Value sent".

A caption under the second card: "A Software sensor carries a value from the PC to the Leakshield, for display pages and lighting. The Leakshield has the values only while nicecooling runs."

## States

| State | What the user sees |
|---|---|
| Empty | A Software sensor slot without a source shows "No source" in dim text and "--" as value |
| Loading | Table frames with grey bars |
| Device offline | Offline banner. Inputs are disabled. Values are dim |
| Read-only | A name field that is not a Verified field has the dashed outline and the padlock, with the reason in its tooltip |
| Source without value | "Value sent" shows "--" and the Warning icon. Tooltip: "GPU temperature has no value. The Leakshield keeps the last value." |
| Slot in use | A slot that a display page or a lighting effect uses shows "Used by display page 3" in the row. Clearing its source asks for confirmation |
| Unit mismatch | The unit of the slot differs from the unit of the source: Warning icon on the unit. Tooltip: "The source is in °C." |
| Alarm | Not used |
| Locked | Not used |

## Interactions

- A name saves when the input loses focus or on Enter. It is a Leakshield write with write status in the row.
- The sensor picker lists every sensor of nicecooling, also the Derived sensors.
- "Value sent" is the source value times the scale plus the offset.
- A row menu has "Clear" for a Software sensor.

## Copy

| Place | Text |
|---|---|
| Columns, device | "Name", "Value", "Type" |
| Columns, software | "Name", "Data source", "Unit", "Scale", "Offset", "Value sent" |
| Clear confirmation | "Clear Software sensor 3? Display page 3 uses it and then shows no value." Buttons: "Cancel", "Clear" |
