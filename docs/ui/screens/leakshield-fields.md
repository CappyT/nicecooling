# Leakshield: Fields tab

No comp. The badges are in `../widgets.md`: "Verification state badge and locked-field indicator".

The tab lists every field of the device settings with its verification state and its risk class, and says why a field is read-only. It has no write and no button that changes a field.

## Layout

Header row: a search field, a filter select ("All fields", "Read-only fields", "Safety fields"), and a summary: "112 fields: 96 Verified fields, 16 read-only".

One table, grouped by setting group (Mode, Pressure, Reservoir, Pump, Alarms, Standby, Display, Lighting, Clock):

| Column | Content |
|---|---|
| Field | Name, and the offset in mono as caption |
| Value | The value on the device, with its unit |
| Risk | Tag "Safety field" (warning tag) or "Low risk" (neutral tag) |
| Verification | The verification badge: "Code-defined", "Plausible read" or "Verification write passed" |
| Writable | "Yes", or "Read-only" with the padlock |

A selected row opens a panel under it with the explanation: why the field is read-only, and how it becomes a Verified field.

## States

| State | What the user sees |
|---|---|
| Empty | The filter has no match: "No field matches." |
| Loading | Table frame with grey bars |
| Leakshield unreachable | Offline banner. The table shows the last read, in dim text |
| Unknown firmware | Banner 4 of the shell. The table shows the raw report only: "Offset" and "Bytes", and no field name. Text: "nicecooling cannot decode these settings. Every field is read-only." |
| Code-defined | Read-only. Explanation: "aquasuite's code defines this field. nicecooling did not confirm it on your device yet." |
| Plausible read, low risk | Writable. Explanation: "The value on the device is in the range and equals the default. That is enough for a low-risk field." |
| Plausible read, Safety field | Read-only. Explanation: "A Safety field needs a Verification write before nicecooling writes it. The app does not run a Verification write: it is a supervised procedure. See the project documentation." |
| Verification write passed | Writable. Explanation: "A Verification write passed on 2026-11-20 for firmware 1022." |
| Contradicting read | Read-only, Warning icon. Explanation: "The value 913 is outside the range 0 to 450. The field layout can be wrong for this firmware." |
| Alarm | Not used |

## Interactions

- A click on a row, or Enter, opens its explanation. Up and Down move in the table.
- The link "Open Fields" on other tabs opens this tab with the filter "Read-only fields" and the row selected.
- "Copy as text" in the header copies the table, for a bug report.

## Copy

| Place | Text |
|---|---|
| Summary | "112 fields: 96 Verified fields, 16 read-only" |
| Risk help | "A Safety field affects the leak protection: Mode, pressure, reservoir, pump, alarms and standby behavior." |
| Verification help | "Code-defined: from aquasuite's code only." / "Plausible read: the value on the device agrees with the code." / "Verification write passed: a supervised write changed only this field." |
