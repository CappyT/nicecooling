# Leakshield: Lighting tab

No comp. The LED timeline editor is in `../widgets.md`.

The tab sets the RGBpx lighting: 6 integrated LEDs and the external LEDs, with up to 8 controllers.

## Layout

1. Card "LEDs and controllers": the LED timeline editor. Toolbar at the right of the header: "Add", "Duplicate", "Delete".
2. Card with the name of the selected controller: its parameters.
   - "Name": input. "Effect": the effect name and the button "Change effect".
   - "LEDs": first LED and count, as number inputs. They mirror the bar.
   - Effect parameters, one row each: colors (color picker), "Speed" (slider), "Brightness" (slider).
   - A parameter that a sensor can drive has a switch "Driven by a sensor" and then a picker (device sensors and Software sensors), "From" and "To" values.

## Effect picker

A dialog, 560 px wide: a list of effects with a name and a one-line text. A selected effect shows a live preview strip of 12 cells at the top. Buttons: "Cancel", "Use this effect".

## States

| State | What the user sees |
|---|---|
| Empty | No controller: the timeline shows the LED row and the text "No controller. The LEDs are off." with the button "Add a controller" |
| Loading | Grey timeline |
| Device offline | Offline banner. The timeline shows the last settings without handles |
| Limit reached | "Add" and "Duplicate" are disabled at 8 controllers. Tooltip: "The Leakshield has 8 controllers." |
| Covered controller | A bar that a higher bar covers completely is dim. Caption: "Covered by Controller 1" |
| LEDs used by the device | LEDs that show a Leakshield state now (an alarm) have a small padlock in their cell. Caption: "The Leakshield uses these LEDs while an alarm is active." |
| Sensor-driven without value | Warning icon on the parameter. The effect uses the "From" value |
| Read-only | Unknown firmware: banner 3, no handles |
| Alarm | See "LEDs used by the device" |
| Locked | Not used |

## Interactions

- Timeline editing: see "LED timeline editor" in `../widgets.md`. A change writes when the drag ends.
- "Delete" removes the selected controllers at once and shows a notification with "Undo" for 10 s. Undo writes the controller back.
- The LED cells show the live colors that the controllers compute, so a change shows before the write ends.

## Copy

| Place | Text |
|---|---|
| Priority hint | "A controller higher in the list wins where two controllers share an LED." |
| External LEDs | "External LEDs: 0 connected. Set the count to match the strip on the RGBpx output." |
| Sensor-driven | "Driven by a sensor", "From", "To" |
