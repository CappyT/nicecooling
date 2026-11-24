# Custom widgets

These widgets have no gpui-component equivalent. Each one is a custom GPUI element. Tokens and severity rules are in `DESIGN.md`. Sizes are in px.

Common rules:

- Each widget is one focus stop. Arrow keys move inside it. Tab leaves it.
- Each widget has an accessible name and a text equivalent of its state.
- A widget paints with tokens only. It has a light and a dark look through the tokens.
- When the daemon is offline, a widget shows its last values in `fg-dim`, and its edit handles are hidden.

## Loop diagram

Comps: `comps/overview.html`, `comps/overview-min.html`.

Draws one Loop from the Loop model. It never draws a part that the model does not hold.

| Part | Geometry (width W, height 168, wide diagram) |
|---|---|
| Supply leg | Horizontal line at y 100, 3 px `water` |
| Return leg | Horizontal line at y 150, joined to the supply leg by a 10 px corner at x W-24 |
| Reservoir | 64x78 at x 24, y 82, only when the Loop has a Leakshield. Fill level as a `water` fill at 30% opacity, with the value and the label "Fill level" |
| Leakshield | 96x28 capsule at x 8, y 32, joined to the reservoir by a dashed `air` line |
| Pump | Circle r 14 at x 0.34 W on the supply leg, with a triangle that points in the flow direction |
| Flow sensor | 36x22 box at x 0.58 W |
| Water temperature | Dot r 4.5 at x 0.80 W |
| Bubble | Circle r 18 at y 46 above its part, with a 1 px leader. Tag in `tag-mono` |
| Value | `value` text 8 px right of the bubble, caption under it |
| Part name | Caption centered under the part at y 128: the display name of the Channel or the sensor |
| Not measured | Dashed box "Blocks and radiators" on the return leg at 0.69 W |
| Flow arrows | One on each leg |

- Tags: PT pressure, ST pump speed, FT flow, TT water temperature, LT fill level.
- A Loop without a Leakshield: no reservoir, no capsule, no PT bubble. The legs close at the left with a 10 px corner. The top left shows "No Leakshield" and "Pressure is not measured."
- A Loop without a flow sensor or a water temperature sensor: that part and its bubble are absent. The other parts keep their place.
- Wide diagram, 640 px and more: one row, 168 px high, as in the table. A part never gets closer to the part at its left than the value and the caption of that part need: the pump, the flow sensor and the water temperature then move to the right of their fractions.
- Narrow diagram, under 640 px (the Loops screen at the minimum window is 528 px): two rows, 212 px high. The Leakshield capsule, the PT bubble and its text keep their place and have the first row for themselves. The water path, the reservoir, the parts, their bubbles and the "Blocks and radiators" box are 44 px lower, and the dashed line from the capsule to the reservoir is 44 px longer. The arrow of the return leg is under the arrow of the supply leg. A Loop without a Leakshield has "No Leakshield" and "Pressure is not measured." in the first row.
- A bubble without a live value shows "--" without a unit, and its caption is the last value: "last 298 mbar". The reservoir then shows no fill level.
- More than one Pump channel: one pump symbol, and the ST bubble shows the lowest speed. The caption says "2 pumps".
- Bubble states: normal, Warning (amber fill, triangle icon before the caption), Alarm (red fill, white tag, octagon icon, value in `alarm`), offline (dashed outline, value "--").
- Out-of-range flag: a flow above the last Calibration table point has a normal bubble and an information icon before the caption "above table range". It is a note on the value. It is not a Warning, so it has no amber and no entry in Active Alarms.
- Captions stay short, because the space between two bubbles is about 170 px at the minimum window: "target 300", "Duty 62 %", "calibrated", "water", "below minimum 250", "above table range".
- Interaction: a click on a bubble opens the source (Sensors, Channels or the Leakshield). Hover or focus shows a tooltip with the full name, the value, the limits and the age. Left and Right move between bubbles. Enter opens the source.
- Accessible text: "Main loop. Pressure 142 mbar, Alarm, below minimum 250. Pump 2 840 rpm. Flow 115 L/h. Water 31.4 degrees Celsius. Fill level 68 percent."

## Device schematic

Comp: `comps/leakshield-status.html`.

Uses the painter and the symbols of the Loop diagram: dashed `air` lines, 1.5 px part outlines, bubbles.

- Parts: atmosphere bar, vent valve and pump valve (two triangles that meet), vacuum pump (circle with a triangle), membrane, reservoir with the fill level. The part list is illustrative: the protocol work gives the real list.
- An active part has the `accent-tint` fill, the `accent` outline and a label with its state: "Vacuum pump, running", "Pump valve, open". An idle part has the `part` fill and the label "closed" or no state.
- Bubbles: PT with pressure, target and pressure change. LT with fill level and volume change.
- No interaction except tooltips.

## Curve editor

Comp: `comps/channels.html`.

- Plot: x is the Input in its unit, y is Duty from 0 to 100%. Grid lines every 5 units on x and every 20% on y. The grid is the snap grid: a point snaps to 1 unit and 1%.
- Curve: 2.5 px line in `s1`. Before the first point and after the last point the line is horizontal.
- Points: circle r 6, `card` fill, `s1` outline. The selected point has the `s1` fill and an `accent-tint` halo r 11. Between 2 and 8 points.
- Operating point: `fg` dot r 6 on the curve at the smoothed Input, with dashed lines to both axes and a label "Now 31.4 C, 62 %". It shows the Duty that the daemon sets, after the step limit.
- Pump floor zone: on a Pump channel, the area under the Pump floor has a 45 degree hatch and a dashed top line with the label "Pump floor 40 %: no Curve goes below it". A point cannot go into the zone.
- Lost Input: the operating point is absent. A Warning strip above the plot says "Input lost: the Channel runs at 100 % until the Input returns."
- Mouse: drag a point. Double click on the line adds a point. Double click on a point removes it. A point cannot pass its neighbors on x.
- Keyboard: Left and Right select a point. Shift with an arrow moves it by 1. Insert adds a point after the selected one. Delete removes it.
- The points also show as number inputs under the plot. The plot and the inputs edit the same data.
- Each change goes to the daemon when the drag ends or the input loses focus. Write feedback shows at the right of the readout row.

## Calibration table editor

A table and a plot side by side.

- Table columns: raw rate (rpm), flow (L/h), a remove button. Rows sort by raw rate. "Add point" adds a row. "Load preset" has the Alphacool 17558 table.
- Plot: x raw rate, y flow. Points and straight segments. A dashed segment from 0,0 to the first point. A dashed extension after the last point, with the label "Out of range".
- Live marker: a vertical line at the current raw rate, with the computed flow: "671 rpm, 115 L/h". It turns to the Warning color in the out-of-range part.
- A selected table row highlights its point, and a selected point highlights its row.

## Pressure gauge

Comp: `comps/leakshield-overview.html`.

- A horizontal scale from 0 to 450 mbar: 10 px track, ticks at 0, 100, 200, 300, 400 and 450.
- The band from the minimum to the maximum has the `accent-tint` fill and an `accent` outline. The target is a 2 px `accent` tick with the label "Target 300".
- The current value is a 6x30 `fg` marker. The value is also a `numeral` above the scale.
- States: in range (tag "In range"), below the minimum or above the maximum (marker and value in `alarm` or `warning`, tag with the state), offline (no marker, "--").
- No interaction. Tooltip with the four values.

## Pressure band diagram

Used by the Pressure step of the Leakshield wizard. It is true to scale: one linear mbar axis holds every value.

- A plot 220 px high. At its left the marks of the axis, at its right the values of its lines.
- The axis covers the live limits, the stored values, the values of the wizard and the live pressure, with a margin of a fifth of their span at each end. Its marks are round values, six at most, as 12 px `fg-dim` text with a 5 px tick.
- Solid lines: the maximum (1.5 px `fg`), the target (2 px `accent`) and the minimum (1.5 px `fg`) of the band that the Leakshield uses now. These are its live limits. While it does not regulate in SHIELD, they are the stored values: see the last item.
- The band between the maximum and the minimum has the `accent-tint` fill. The zones above the maximum and below the minimum have a 45 degree hatch in `fg-dim`.
- Each zone has one line of 12 px text on a `card` plate at its left: "Vent valve open: releases vacuum" above the maximum, "Pump off, vent valve closed" in the wider part of the band, "Pump on: builds vacuum up to the target" below the minimum. A text that does not fit truncates. A zone lower than 26 px has no text.
- The value of a solid line is bold text at the right of the plot, on the height of the line: "Maximum 465 mbar". The target value is in `accent`. When the stored value differs from the live one, a caption under it has the stored value: "set: 425 mbar".
- Dashed lines: the values of the wizard, when they differ from the stored values. They have the colors and the widths of the solid lines, and the 12 px text "new: 490 mbar" at the right of the plot.
- Two values at the right that are close together move apart, so that no text covers another one.
- The live pressure is an `fg` disc r 6 near the right edge of the plot, at its place on the axis, with the bold text "Now 398 mbar" on a `card` plate at its left.
- Without a live band the diagram has the stored band as solid lines and no marker. This is the case while the Leakshield does not answer, in RELEASE, in MONITOR, during an Action, and while the three live limits are not in order.
- No interaction.

## LED timeline editor

- Top row: one cell per LED, 28 px wide: 6 integrated LEDs, then the external LEDs. Each cell shows the live color. A gap separates the two groups.
- Under it: up to 8 controller bars, one per row, 28 px high. A bar covers the LEDs that the controller drives and shows its effect colors. The top row has the highest priority.
- Mouse: drag a bar sideways to move it, drag an edge to change its length, drag it up or down to change the priority. A click selects it. The parameters of the selected controller show under the timeline.
- Keyboard: Up and Down select a bar. Alt with Left or Right moves it. Shift with Left or Right changes its length. Alt with Up or Down changes the priority.
- Toolbar: Add, Duplicate, Delete. Add is disabled at 8 controllers, with the reason in its tooltip.
- The color of a bar comes from the user. It is data, so it does not follow the severity rules.

## Display page preview, template schematic and import preview

- Display page preview: the image of 128x64 pixels that the device draws for the page, at a whole scale: 1 px for each pixel in the page grid, 2 px on the page card. A lit pixel is a `display-ink` square on the `display` ground. Each run of lit pixels in a line is one rectangle, so the pixels are sharp. The ground has a frame of 4 px and radius 6.
- A preview without an image has the same size: a `track` box with a dashed `border-strong` outline and a short text in `fg-dim` that says why. It is never a dark box.
- A page that the device shows in no state has 40% opacity and the caption "Not shown". The selected page has a 2 px `accent-bg` outline.
- Template schematic: 96x48 px on the `display` ground. It sketches the layout of a template with three shapes on the grid of 128x64: a dim bar for a line of text, a lit bar for a value or the filled part of a bar, and an outline for an icon or a bar.
- Import preview: the one-bit result of the conversion on the `display` ground, 2 px for each pixel for the logo and 4 px for an icon. Under it a threshold slider and the switch "Swap lit and dark". The result updates while the slider moves.
- An icon of the device (23x23) draws the same way: 1 px for each pixel in a row of the page card, 2 px in the icon dialog.

## Sensor picker

- A `popover` 360 px wide and up to 420 px high, with a search field at the top.
- Rows are grouped by device: Board chips, Leakshield, GPU, NVMe, Derived sensors. Each row has the sensor name, the device caption, and the live value at the right.
- Typing filters by name and by device. Up and Down move. Enter selects. Escape closes.
- The closed control is a `select` that shows the name and the live value. A long name truncates and the value keeps its width.
- A sensor without a value is listed with "--" and an offline icon. The user can select it.
- Multi-select mode (History): each row has a check mark, and the popover stays open.

## Process picker

- The same popover as the sensor picker. Rows are the running processes: the process name in bold, the command line as a caption, one row per name. The rule can list several names: the picker is multi-select.
- The search field also accepts a name that does not run now: the first row is then "Use 'name'".
- A caption under the list says: "A rule matches the process name, without case."

## Diff table

Used by the wizard Summary and by Profile activation.

| Column | Content |
|---|---|
| Setting | Field name, with a padlock when the field is read-only |
| Now | Value on the device |
| New | Value to write. A changed value is bold |
| Note | Warning or note text, with its icon |
| Write | Write status of the row, after Apply |

- Rows without a change are hidden behind "Show 14 unchanged settings".
- A row that cannot be written has the reason in the Note column and is not counted in "Apply 5 changes".
- Write status per row: pending (spinner), backup saved, written, verified (check, `success`), failed (octagon, `alarm`, the row gets the `alarm-tint` ground).

## Countdown dialog

Comp: `comps/countdown-dialog.html`.

- 440 px wide, radius 15, over the scrim. Head: `alarm-bg` field with the octagon, the Alarm name and one line of text.
- Body: the seconds as `countdown` text, "seconds left of 30", a 6 px progress bar in `alarm-bg` that gets shorter.
- A list with Source, Raised, the value that caused the Alarm, and the alarm response.
- Buttons: "Cancel poweroff" (suggested, 46 px high, has the focus when the dialog opens) and "Hide". "Hide" closes the dialog. The countdown continues in the banner.
- Escape does not cancel the poweroff: it acts as "Hide". Enter and Space press "Cancel poweroff".
- When the countdown ends, the dialog says "The PC powers off now." and has no buttons.
- When another Alarm with a shorter countdown is raised, the dialog shows the shortest countdown and "2 Alarms".

## Verification state badge and locked-field indicator

| State | Icon | Color | Label |
|---|---|---|---|
| Code-defined | code brackets | `fg-dim` | "Code-defined" |
| Plausible read | eye | `fg-dim` | "Plausible read" |
| Verification write passed | shield with a check | `success` | "Verification write passed" |
| Verified field (short form outside the Fields tab) | shield with a check | `success` | "Verified field" |
| Read-only | padlock | `fg-dim` | "Read-only" |

- The badge is an icon and a 12 px label next to the field name. It is not a button.
- A read-only field has a dashed outline, no fill, and a caption with the reason and a link "Open Fields".
- With an unknown firmware version every field has the read-only look, and one banner gives the reason. The per-field captions are then hidden.

## Write status

A composite, listed here because every screen that writes uses it.

| Step | Look |
|---|---|
| Pending | Spinner, "Writing" |
| Backup saved | Check, "Backup saved" |
| Written | Check, "Written" |
| Read-back verified | Check in `success`, "Verified 21:44:02" |
| Failed | Octagon in `alarm`, "Write failed: 3 unexpected bytes changed" and a button "Restore the backup" |

- A Board write (Curve, Pump floor) has one step: "Applied 21:44:02".
- The status stays next to the control until the next change. A failure also shows as a `notification` that stays until the user closes it.
- When polkit denies a write: "Not authorized. Nothing changed."
