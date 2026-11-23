# Profiles

Comp: `../comps/profiles.html` (1280x800; also 1100x700 and Italian in `../comps/shots/`).

A Profile is a named set of Curves and Leakshield settings that becomes active as one unit. The screen manages Profiles, Profile rules and the switching options.

## Layout

List column (292 px):

1. Card "Profiles": one row per Profile with its name and a caption ("Active by rule 2", "Rule 1", "No rule. Mode RELEASE"). The active Profile has the tag "Active". A Profile with Mode RELEASE has the Warning icon. Header button: "New".
2. Card "Options": "Rule delay" (seconds), "Profile dwell" (minutes), "Immediate switching" (switch).

Detail column:

1. Card with the Profile name. Header: a tag ("Active by rule", "Active manually" or none), then "Activate manually" or "Revoke manual activation", and a menu button with "Duplicate", "Rename" and "Delete". Body in two halves:
   - "Curves": one row per Channel with a Curve or a fixed Duty, with a summary ("Curve, 4 points", "Fixed Duty 30 %"). Link "Open", which opens Channels.
   - "Leakshield snapshot": rows "Mode", "Display", "Lighting", "Alarm thresholds". Link "Open", which opens the Leakshield.
   - Difference rows under the two halves, with the warning tint.
2. Card "Profile rules": an ordered list. Each rule row has a drag handle, its number, a condition icon, the condition as a sentence, an arrow, the target Profile, and the tag "True since 22:00" on a rule that is true. A long condition wraps to a second line. Header button: "Add rule". The selected rule opens the rule editor under its row.

## Rule editor

Row 1: "Condition" (select) and "Activate Profile" (select). Row 2: the fields of that condition. Row 3: "Delete rule" at the left, "Cancel" and "Save rule" at the right.

| Condition | Fields | Sentence in the list |
|---|---|---|
| Process | Process picker (see `../widgets.md`) | "Process steam runs" |
| Time window | "From" and "To" times, and the days of the week | "Time window 22:00 to 07:00" |
| Sensor threshold | Sensor picker, "Is" (above, below), "Value", "For" (seconds) | "Sensor Water temperature above 38 °C for 60 s" |

## States

| State | What the user sees |
|---|---|
| Default Profile | The Default Profile always exists. Its caption is "Active when no rule is true". It is active when no rule wins and no manual activation exists. Its "Delete" menu entry is disabled, tooltip: "The Default Profile cannot be deleted." Rename and Duplicate work |
| Empty, Profiles | Not possible: the Default Profile always exists |
| Empty, rules | "No rule. The Default Profile is active until you activate another Profile by hand." with the button "Add rule" |
| Loading | Row frames with grey bars |
| Daemon offline | Offline banner. Every button is disabled |
| Manual activation | Tag "Active manually". Information row in the rules card: "Profile rules wait until you revoke the manual activation." Button "Revoke" |
| Rule delay runs | The rule tag says "True, activates in 18 s" |
| Profile dwell runs | The active Profile caption says "Stays for 3 min more (Profile dwell)" |
| Immediate switching on | Banner 4 of the shell. "Rule delay" and "Profile dwell" are disabled, caption "Not used with Immediate switching" |
| Profile with RELEASE | Warning row: "This Profile sets Mode RELEASE: the Loop has no leak protection while it is active." Saving it needs no administrator password |
| Differs from other Profiles | Warning row: "Alarm thresholds differ from 2 other Profiles" with the values, and "Apply to all Profiles" |
| External change | Information row: "Display brightness changed outside nicecooling. Activating another Profile overwrites it." |
| Activating | Write status. With Leakshield differences, a dialog first shows the diff table and "Activate" |
| Activation failed | The diff table row has the failure. "Restore the backup" with the padlock. The Curves stay active |
| Alarm | Not used. The poweroff banner shows above |
| Locked | Only "Restore the backup" after a failed write has the padlock |

## Interactions

- Drag a rule by its handle to change the order. Alt+Up and Alt+Down do the same from the keyboard. The first rule that is true wins.
- "Activate manually" holds the Profile until the user selects "Revoke manual activation".
- "Duplicate" makes "Quiet copy" and opens the rename popover.
- Delete asks: "Delete the Profile 'Gaming'? Rule 1 uses it and is deleted too." Buttons: "Cancel", "Delete Profile".
- Turning on "Immediate switching" opens a warning dialog: "With Immediate switching, a rule that changes fast also changes the Profile fast. Each change can write the Leakshield." A checkbox "Do not show this again", then "Cancel" and "Turn on".
- A change to a Curve or a Leakshield setting on another screen goes into the active Profile. To edit another Profile, activate it manually.

## Copy

| Place | Text |
|---|---|
| Rules caption | "The first rule that is true wins. Drag to change the order." |
| Options | "Rule delay", "Profile dwell", "Immediate switching", "No Rule delay, no Profile dwell" |
| Options help | "Rule delay: a condition must stay true for this time before its rule activates a Profile." / "Profile dwell: a Profile that a rule activated stays active for at least this time." |
| Editor | "Condition", "Sensor", "Is", "Value", "For", "Activate Profile", "Delete rule", "Save rule" |
