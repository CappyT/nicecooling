# Leakshield: Settings tab (wizard)

Comp: `../comps/leakshield-wizard.html` (step 4, 1280x800; also 1100x700 in `../comps/shots/`).

The Settings tab is a wizard with eight steps. The user can also open one step directly to change one setting.

## Layout

1. Stepper card: Reservoir, Pump, Flow, Pressure, Loop, Standby, Mode, Summary. A done step has a check. A click on a step opens it.
2. Step card: title, "Step 4 of 8", one row per setting, then the footer with "Cancel" at the left and the back and next buttons at the right. The back and next buttons carry the name of their step.
3. "Pending changes" card, under the step card: each changed setting with its old and new value, and the write path.

Each setting row has: the name, the verification badge, a caption, and the control at the right.

## Steps

| Step | Settings |
|---|---|
| Reservoir | "Reservoir": select with the known types and "Custom". "Total volume" (ml), "Air volume at maximum fill level" (ml), "Air volume at minimum fill level" (ml): read-only with a known type. "Automatic fill-level measurement": switch, with a warning caption |
| Pump | "Pump": select with the known types and "Custom". "Maximum pressure" (mbar), "Maximum speed" (rpm): read-only with a known type. "More than one pump": switch. "Pump pressure now" and "Speed limit": read-only texts. "Speed source": read-only text "Pump channel of Main loop: Waterpump" with the link "Open Loop" |
| Flow | "Flow source": read-only text "Flow sensor of Main loop: Water Flow, 115 L/h" with "Open Loop". Without a flow sensor: "None. The Leakshield uses the pump speed only." |
| Pressure | "Pressure values": the radio rows "Recommended values" and "My own values". The band diagram. "Maximum pressure", "Target pressure", "Minimum pressure" in mbar, each with a caption. "Adaptive pressure": switch. "Time-controlled vacuum generation": switch and two times. See "Pressure step" |
| Loop | "Height difference" (cm), 0 to 200. Above 100 cm a warning line shows under it |
| Standby | "Default pump speed" (rpm), 1000 to 10000, and "Default flow" (L/h), 30 to 500, each with the button "Use the current value". "Reduced vacuum in standby": switch. "Pressure in standby" (mbar): 30 to the minimum pressure, editable only with the switch on |
| Mode | The three Modes as radio rows with their captions |
| Summary | The diff table (see `../widgets.md`), notes and warnings above it, and the button "Apply 5 changes" |

## Presets

A known type writes its numbers into the pending changes when the user selects it. The numbers then show read-only with the padlock badge "Preset value". "Custom" keeps the numbers and opens them for an edit.

| Pump | Maximum speed (rpm) | Start speed (rpm) | Maximum pressure (mbar) |
|---|---|---|---|
| D5 | 4800 | 4800 | 375 |
| aquastream | 5500 | 5500 | 400 |
| DDC-1T | 3900 | 3900 | 350 |
| DDC-1T Plus | 4500 | 4500 | 450 |
| DDC 3.25 | 4850 | 5500 | 530 |
| Eheim 1046 | 3000 | 3000 | 120 |
| Eheim 1048 | 3000 | 3000 | 150 |
| Eheim 1250 | 3000 | 3000 | 200 |

| Reservoir | Total volume (ml) | Air volume at maximum fill level (ml) | Air volume at minimum fill level (ml) |
|---|---|---|---|
| ULTITUBE 100 | 240 | 50 | 200 |
| ULTITUBE 150 | 360 | 50 | 300 |
| ULTITUBE 200 | 470 | 70 | 400 |
| aqualis 100 | 100 | 40 | 80 |
| aqualis 150 | 150 | 50 | 100 |
| aqualis 450 | 450 | 70 | 350 |
| aqualis 880 | 880 | 100 | 750 |
| aquainlet 100 | 100 | 40 | 80 |
| aquainlet 150 | 150 | 50 | 100 |
| aquatube | 160 | 50 | 100 |
| aquabox | 114 | 50 | 80 |

- The start speed of a pump preset goes to "Default pump speed" on the Standby step. It stays editable there. It is never above the maximum speed: the wizard cuts it to the maximum speed.
- A type that is not in the tables, as "undefined" after a factory reset, shows as the first choice of its select. Its numbers accept an edit.

## Pump step

- "More than one pump" is `pump_flags` bit 1. Pumps in a row add their pressures, so the wizard counts twice the maximum pressure: for the recommended values, for the warning below and for the helper. While the switch is on, a warning line shows under it.
- The pump pressure, twice with more than one pump, above 430 mbar gives a warning row at the top of the Pump, Pressure and Mode steps: 430 mbar is the highest minimum of the control band. The Summary has the same warning while a change of the pump is pending. It does not disable "Apply".
- The helper is two read-only rows. "Pump pressure now" is the pressure of the pump at the speed of the Pump channel of the Loop: the maximum pressure times the square of speed over maximum speed. "Speed limit" is the speed at which the pump makes 450 mbar, the highest target pressure, when that speed is below the maximum speed.

## Pressure step

The three values are the control band of SHIELD. They are not alarm limits: no value raises a Warning or an Alarm.

| Value | What the Leakshield does |
|---|---|
| Minimum pressure | Below it, the Leakshield starts its pump |
| Target pressure | The pump builds the vacuum up to it and stops |
| Maximum pressure | Above it, the Leakshield opens its vent valve and releases vacuum |

- "Pressure values" is `shield_flags` bit 4. Bit set: the recommended values. Bit clear: the values of the user. With the recommended values the three inputs are read-only and have the padlock badge "Recommended value". With own values a warning line shows under the choice. It has the recommended values as numbers.
- The recommended values come from the pump: the minimum is the maximum pressure of the pump, or twice that with more than one pump, cut to 430 mbar. The target is 50 mbar above the pump pressure. The maximum is 40 mbar above the target. The standby minimum is the pump pressure, 80 mbar at most. For a pump of 375 mbar: 375, 425, 465 and 80 mbar.
- The wizard puts the recommended values into the pending changes when the user selects "Recommended values", and with each change of the pump type, of the maximum pressure of the pump or of "More than one pump" while that choice is on.
- The button "Use the recommended values" is in the row of the choice. With own values it writes the recommended values and keeps the choice. With the recommended values it shows only when the Leakshield holds other values. An information line then says so, and the inputs show what the Leakshield holds.
- The band diagram (`../widgets.md`, "Pressure band diagram") is between the choice and the three inputs. It is true to scale. The inputs are in the order of the lines: maximum, target, minimum.
- The solid lines are the band that the Leakshield uses now: the lower limit, the setpoint and the upper limit of its live sensors. The marker of the live pressure is at its place between them. A line whose stored value differs has it under the live value: "set: 425 mbar". The line under the diagram has the three live limits: "Now: minimum 215, target 265, maximum 465 mbar."
- The dashed lines are the values of the wizard, when they differ from what the Leakshield holds: an edited own value, or the recommended values that wait for "Apply". Each has the text "new: 490 mbar". The line under the diagram then ends with "Dashed lines: the new values, not written yet."
- "Adaptive pressure" is `shield_flags` bit 3. The Leakshield then lowers the minimum and the target with the pump speed of the Feed. It keeps the stored distance between minimum and target and the stored maximum. So the switch never dims or locks the three values.
- Without a live band the solid lines are the stored values, the diagram has no marker, and the line under it says "The Leakshield is not regulating in SHIELD now. The lines are the stored values." This is the case while the Leakshield does not answer, in RELEASE, in MONITOR, during an Action, and while the three live limits are not in order.

The wizard corrects a value of the band. It shows no error for it:

| Value | Range in mbar | Rule | Stored field |
|---|---|---|---|
| Minimum pressure | 30 to 430 | | `shield_pressure_low` |
| Target pressure | 50 to 450 | At least the minimum plus 20 | `shield_pressure_working_range`: target minus minimum |
| Maximum pressure | 80 to 500 | At least the target plus 30 | `shield_pressure_max_range`: maximum minus target |

- An edit takes whole mbar and moves the neighbors that are too close. A higher minimum raises the target. A higher target raises the maximum. A lower maximum lowers the target. A lower target lowers the minimum. A lower minimum lowers the standby minimum.
- The caption of each value ends with its range: "30 to 430 mbar."
- A value at a limit has an information line under its caption: the maximum at the target plus 30, the target at the minimum plus 20, the minimum at 30 or at 430.

## Loop step

"Height difference" is `loop_height`, in whole cm. The wizard corrects a value outside 0 to 200 cm. The Leakshield adds the static pressure of this height to the pump pressure. The recommended pressure values do not count the height, so above 100 cm the step warns and points to own values.

## Standby step

- The wizard corrects "Default pump speed" to 1000 to 10000 rpm and "Default flow" to 30 to 500 L/h, in whole numbers. "Use the current value" takes the same correction. The caption of each value has its range.
- "Reduced vacuum in standby" is `shield_flags` bit 5. "Pressure in standby" is `shield_pressure_low_standby`. The wizard keeps it in 30 mbar to the minimum pressure of the control band, and a lower minimum pressure lowers it.
- With the switch off, the input of "Pressure in standby" is dim and takes no edit. Its caption says which switch to turn on.

## States

| State | What the user sees |
|---|---|
| Empty | No pending change: the "Pending changes" card says "No change yet." The Summary says "Nothing to apply." and "Apply" is disabled |
| Loading | "Reading the settings" with a spinner in the step card |
| Device offline | Offline banner. Controls are disabled. Pending changes stay |
| Read-only field | Dashed field, padlock, "Read-only", and the reason: "Not a Verified field: this Safety field has no Verification write yet." with "Open Fields" |
| Unknown firmware | Banner 3 of the shell. Every field is read-only. Per-field reasons are hidden |
| Not authorized | A user outside `wheel` gets the polkit dialog on Apply when the Mode step holds RELEASE |
| Value out of range | Only for a reservoir volume, the maximum speed and the maximum pressure of the pump, when the Leakshield cannot store the number. The input has a red outline and a caption with the range. The next button stays active, and the Summary lists the error and disables "Apply". The wizard corrects every other number to its range and shows no error |
| Applying | Each row of the diff table shows its write status. "Apply" becomes "Applying 2 of 5" |
| Write failed | The row has the `alarm-tint` ground and the text "Write failed: the read-back does not match." The writes stop. Button "Restore the backup", and "Keep the current state" |
| Applied | Success row: "5 settings written and verified." The changes go into the active Profile |
| Alarm | The poweroff banner shows above the wizard. The wizard stays open |

## Interactions

- The wizard writes nothing before "Apply". "Cancel" discards the pending changes after a confirmation: "Discard 2 changes?"
- Leaving the tab with pending changes keeps them. The tab label then has a dot.
- Summary warnings come from the settings: "MONITOR does not prevent a leak.", "The pump can make 480 mbar. The Leakshield holds 430 mbar at most, so it cannot fully protect the Loop at full pump speed.", "Automatic fill-level measurement releases the pressure for a short time." A warning never disables "Apply".
- Left and Right arrow keys do not change the step. Alt+Left and Alt+Right do.

## Copy

| Place | Text |
|---|---|
| Reservoir | "A known reservoir sets the three volumes. Select Custom to enter your own." |
| Pump | "A known pump sets its maximum pressure, its maximum speed and its start speed. Select Custom to enter your own." |
| More than one pump | "Pumps in a row add their pressures. nicecooling then counts twice the maximum pressure." With the switch on: "With more than one pump the Leakshield needs twice the vacuum. Check the values on the Pressure step." |
| Pump pressure now | "About 131 mbar at 2 840 rpm." Without a speed: "No pump speed from the Loop. Without a speed the Leakshield counts with the maximum speed." |
| Speed limit | "Above 4 469 rpm the pump makes more than 450 mbar, the highest target pressure of the Leakshield." or "At every speed the pump stays under 450 mbar, the highest target pressure of the Leakshield." |
| Pressure values | "The three values are a control band for SHIELD: the Leakshield keeps the pressure between the minimum and the maximum. No value raises an Alarm." |
| Own values, warning | "With your own values the Leakshield can fail to protect the Loop. Recommended for your pump: minimum 375, target 425, maximum 465 mbar." |
| Other values than the recommended ones | "The Leakshield holds other values. Recommended for your pump: minimum 375, target 425, maximum 465 mbar." |
| Maximum pressure | "Above this value the Leakshield opens its vent valve and releases vacuum." |
| Target pressure | "The pressure that you want in SHIELD. The pump of the Leakshield builds the vacuum up to this value and stops." |
| Minimum pressure | "Below this value the Leakshield starts its pump." |
| Maximum at its distance | "The maximum is as close to the target as the Leakshield accepts: 30 mbar." |
| Target at its distance | "The target is as close to the minimum as the Leakshield accepts: 20 mbar." |
| Minimum at 30 or 430 mbar | "This is the lowest minimum that the Leakshield accepts.", "This is the highest minimum that the Leakshield accepts." |
| Adaptive pressure | "The Leakshield lowers the minimum and the target with the pump speed of the Feed. The distance between minimum and target and the maximum stay as set here." |
| Height difference | "The height from the lowest point of the Loop to its highest point. The Leakshield adds the pressure of this water column to the pump pressure. The default is 30 cm." Above 100 cm: "Above 100 cm the recommended pressure values can be too low. Set your own values on the Pressure step." |
| Time-controlled | "The Leakshield builds the negative pressure at two times of the day, so that you choose when its pump makes noise." |
| Automatic measurement | "The Leakshield measures the fill level after a change, for example when you open the reservoir. During the measurement the Loop has no leak protection." |
| Standby | "After standby the PC sends no Feed yet. The Leakshield then uses these values." |
| Reduced vacuum in standby | "In standby the pump of the Loop does not run. The Leakshield can then hold a lower minimum pressure, so that its own pump runs less." |
| Pressure in standby | "The minimum pressure in standby: 30 mbar to the minimum pressure, 375 mbar." With the switch off: "Turn on "Reduced vacuum in standby" to set it." |
| Pending card | "The wizard writes nothing before you select Apply on the Summary step. Then each write has four steps: Backup saved, Written, Read back, Verified: only the expected bytes changed." |
