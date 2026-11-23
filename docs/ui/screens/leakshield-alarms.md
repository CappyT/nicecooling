# Leakshield: Alarms tab

No comp. Rows follow the settings rows of `../comps/leakshield-wizard.html`.

The tab sets the Warning and Alarm thresholds of one Leakshield, its Device response and the alarm response of nicecooling. The Device response is what the Leakshield does by itself: signal output, key sequence, sound. The Alarms screen has the same alarm responses for every source.

## Layout

One card per group. Each event is one row with these columns:

| Column | Content |
|---|---|
| Event | Name and a caption with its trigger |
| Threshold | A slider with a number input, where the event has one |
| Device response | Select: "None", "Signal: short press", "Signal: long press", "Key sequence". Then the switch "Sound", only for a Warning: an Alarm always sounds |
| Alarm response | What nicecooling does. Select: "None", "Notify", "Poweroff after a delay", and the delay in seconds |

| Group | Events |
|---|---|
| Leakage | "Leak Warning": threshold on the volume change in ml/h. "Small leak Alarm" and "Large leak Alarm": no threshold, the Leakshield decides |
| Fill level | "Fill level Warning" and "Fill level Alarm": thresholds in percent |
| System | "System Alarm": a fault of the Leakshield or of its power supply. No threshold |
| nicecooling | "Leakshield unreachable": nicecooling gets no data for 10 s. It has an alarm response only |

Above the cards: the row "Reset device alarms", then the recommendation card.

## Reset device alarms

The row shows while the Leakshield holds a latched alarm bit (`latched_bits` of an active event of this Leakshield is not 0). It has the text "The Leakshield holds a latched alarm. It stays active until you reset it." and the button "Reset device alarms". The button calls `ResetAlarms` (class `write`), with write status in the row. It is disabled while the Leakshield is read-only or does not answer. A firmware without the command shows the standard "This firmware has no such command." text.

## Recommendation and overlap warnings

nicecooling proposes device settings and never writes them alone.

| Condition | Row |
|---|---|
| Key sequence is on for any event | Warning: "The key sequence is on for the Small leak Alarm. The Leakshield types it into the focused window, a terminal included." Button "Turn it off" |
| Signal is on for a leak Alarm, and nicecooling also powers off | Warning: "Two poweroff responses overlap for the Large leak Alarm: the device signal and nicecooling. The signal cuts the power without a clean poweroff." Button "Set the Device response to None" |
| System Alarm signal is not a long press | Information: "Recommended: signal as a long press for the System Alarm. It is the only protection when Linux hangs." Button "Use the recommendation" |
| All as recommended | Success row: "The Device responses match the recommendation." |

Each button writes one setting through the normal write path, with write status in the row.

## States

| State | What the user sees |
|---|---|
| Empty | Not used: the events are fixed |
| Loading | Row frames with grey bars |
| Device offline | Offline banner. Controls are disabled |
| Read-only | Alarm settings are Safety fields. A field that is not a Verified field is dashed with the padlock and the reason. The alarm response is always editable, because the daemon holds it |
| Defaults | Large leak Alarm: "Poweroff after a delay", 30 s. Every other event, "Leakshield unreachable" included: "Notify" |
| Threshold order wrong | Fill level Alarm above the Warning: red outline. Text: "The Alarm threshold must be below the Warning threshold." |
| Delay 0 | Warning under the row: "With 0 s you cannot cancel the poweroff." |
| Event active | The row of an active event has its severity icon and the caption "Active since 21:46:51" |
| Locked | Not used |

## Interactions

- A slider writes when the drag ends. A select writes on choice.
- The alarm response writes the daemon configuration, not the Leakshield. Its feedback is "Applied".
- "Test the signal" is not offered: a signal test powers off the PC.

## Copy

| Place | Text |
|---|---|
| Leak Warning caption | "The volume change passes the threshold. Temperature changes also move this value." |
| Leak Alarm caption | "The Leakshield detects the leak size. You cannot set a threshold." |
| System Alarm caption | "A fault of the Leakshield or of its power supply." |
| Device response help | "Signal: the Leakshield presses the power button of the Board through its cable." |
