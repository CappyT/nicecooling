# Profiles

Comp: `../comps/profiles.html` (1280x800; also 1100x700 and Italian in `../comps/shots/`).

A Profile is a named set of Curves and Leakshield settings that becomes active as one unit. The screen manages Profiles, their Leakshield slots, Rule groups and the switching options.

## Layout

List column (292 px):

1. Card "Profiles": one row per Profile with its name and a caption ("Active by group 2", "Group 1", "No group. Slot 3. Mode RELEASE"). The list has no limit. The active Profile has the tag "Active". A Profile with Mode RELEASE has the Warning icon. Header button: "New".
2. Card "Options": "Rule delay" (seconds), "Profile dwell" (minutes), "Immediate switching" (switch).

Detail column:

1. Card with the Profile name. Header: a tag ("Active by group", "Active manually" or none), then "Activate manually" or "Revoke manual activation", and a menu button with "Duplicate", "Rename" and "Delete". Body in two halves:
   - "Curves": one row per Channel with a Curve or a fixed Duty, with a summary ("Curve, 4 points", "Fixed Duty 30 %"). Link "Open", which opens Channels.
   - "Leakshield settings": a select "Slot" (1 to 4) and the rows "Mode", "Display", "Lighting", "Alarm thresholds" of that slot. Link "Open", which opens the Leakshield. Under the select, the caption names the Profiles that share the slot ("Shared with Profile 3").
   - Difference rows under the two halves, with the warning tint.
2. Card "Rule groups": an ordered list. Each group row has a drag handle, its number, the rules as sentences (one line each), an arrow, the target Profile, and the tag "Holds since 22:00" on a group that holds. A long rule wraps to a second line. Header button: "Add group". The selected group opens the group editor under its row.

## Group editor

Row 1: "Activate Profile" (select) and the switch "Enabled". Row 2: the list of the rules of the group. Each rule row has its sentence, "Edit" and "Remove". Button: "Add rule". The editor of a rule has the select "Condition" and the fields of that condition. Row 3: "Delete group" at the left, "Cancel" and "Save group" at the right. A group holds when all its rules hold.

| Condition | Fields | Sentence in the list |
|---|---|---|
| Sensor | Sensor picker, "Is" (less than, at most, equal to, at least, more than, not equal to, between), "Value" (and "Up to" for between), "For" (seconds) | "Sensor Water temperature more than 38 °C for 60 s" |
| Process | Process picker (see `../widgets.md`) for one or more names, "Match" (one of, all of, none of) | "Process steam runs" |
| Time window | "From" and "To" times, and the days of the week | "Time window 22:00 to 07:00" |
| Interval | "Every" (seconds), "Start after" (seconds), "Lasts" (seconds) | "Every 10 min, from 1 min after the start, for 2 min" |

The editor has no hotkey condition. Wayland has no standard global shortcut.

## States

| State | What the user sees |
|---|---|
| Default Profile | The Default Profile always exists. Its caption is "Active when no group holds". It is active when no group wins and no manual activation exists. Its "Delete" menu entry is disabled, tooltip: "The Default Profile cannot be deleted." Its slot is 1 and the select is disabled. Rename and Duplicate work |
| Empty, Profiles | Not possible: the Default Profile always exists |
| Empty, groups | "No group. The Default Profile is active until you activate another Profile by hand." with the button "Add group" |
| Group without rules | The group row has the warning "No rule: this group never holds." |
| Short interval | Warning row in the editor: "The window is shorter than the Rule delay. The group never activates its Profile." Not shown with Immediate switching |
| Loading | Row frames with grey bars |
| Daemon offline | Offline banner. Every button is disabled |
| Manual activation | Tag "Active manually". Information row in the groups card: "Rule groups wait until you revoke the manual activation." Button "Revoke" |
| Rule delay runs | The group tag says "Holds, activates in 18 s" |
| Profile dwell runs | The active Profile caption says "Stays for 3 min more (Profile dwell)" |
| Immediate switching on | Banner 4 of the shell. "Rule delay" and "Profile dwell" are disabled, caption "Not used with Immediate switching" |
| Profile with RELEASE | Warning row: "This Profile sets Mode RELEASE: the Loop has no leak protection while it is active." Saving it needs no administrator password |
| Shared slot | Information row: "Slot 2 is shared with Profile 3. A Leakshield change here changes both." |
| Differs from other slots | One warning row for each Leakshield whose slots differ: "Alarm thresholds differ from 2 other slots" and the button "Apply to all slots". With more than one Leakshield, the row starts with the device label ("Leakshield 31072: "). The app compares the thresholds of this slot with `GetStoredFields` of every other slot of that Leakshield that has stored settings. The button applies the thresholds to every other slot of that Leakshield that has stored settings: `SetFields` for the slot that the device runs (`ActiveSlot`), `SetStoredFields` for the others. A slot that is not changed gets one row: "Slot 3: not changed. Not applied: the daemon refused the change." |
| Slot change | A dialog before the change of the slot of the active Profile: "Slot 3 has other Leakshield settings. They are written to the Leakshield now." Buttons: "Cancel", "Change slot". The change of the slot of another Profile needs no dialog |
| External change | Information row: "Display brightness changed outside nicecooling. Activating a Profile with another slot overwrites it." |
| Activating | Write status. When the slot of the Profile differs from `ActiveSlot` and the stored slot differs from the device, a dialog first shows the diff table (the columns "Field", "Leakshield now" and "Slot 2") and the buttons "Cancel" and "Activate". The table compares `GetStoredFields` of the slot with `GetFields` |
| Activation failed | The diff table row has the failure. "Restore the backup" with the padlock: the button shows while a Leakshield holds a `FailedWriteBackup`, also after an app restart. The Curves stay active |
| Rule group of the active Profile | A row under the header: "Group 2: time window 22:00 to 07:00." It shows while a group activated the Profile |
| Alarm | Not used. The poweroff banner shows above |
| Locked | Only "Restore the backup" after a failed write has the padlock |

## Interactions

- Drag a group by its handle to change the order. Alt+Up and Alt+Down do the same from the keyboard. The first group that holds wins.
- "Activate manually" holds the Profile until the user selects "Revoke manual activation".
- "Duplicate" makes "Quiet copy" with the same slot and opens the rename popover. "New" makes a Profile with the slot of the active Profile.
- Delete asks: "Delete the Profile 'Gaming'? Group 1 uses it and is deleted too." Buttons: "Cancel", "Delete Profile". The Leakshield settings of its slot stay.
- Turning on "Immediate switching" opens a warning dialog: "With Immediate switching, a group that changes fast also changes the Profile fast. Each change can write the Leakshield." A checkbox "Do not show this again", then "Cancel" and "Turn on".
- A change to a Curve on another screen goes into the active Profile. A change to a Leakshield setting goes into the slot of the active Profile. To edit another Profile, activate it manually.

## Copy

| Place | Text |
|---|---|
| Groups caption | "A group holds when all its rules hold. The first group that holds wins. Drag to change the order." |
| Options | "Rule delay", "Profile dwell", "Immediate switching", "No Rule delay, no Profile dwell" |
| Diff dialog | "Activate the Profile 'Quiet'?", "Slot 2 has other settings than the Leakshield. These fields change on the Leakshield.", "Field", "Leakshield now", "Activate" |
| Slots | "Apply to all slots", "Slot 3: not changed. %{reason}" |
| Profile dwell | "Stays for 3 min more (Profile dwell)" |
| Options help | "Rule delay: a group must hold for this time before it activates a Profile." / "Profile dwell: a Profile that a group activated stays active for at least this time." |
| Editor | "Condition", "Sensor", "Is", "Value", "Up to", "For", "Match", "Every", "Start after", "Lasts", "Activate Profile", "Enabled", "Add rule", "Remove", "Delete group", "Save group", "Slot" |
