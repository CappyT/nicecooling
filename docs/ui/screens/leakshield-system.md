# Leakshield: System tab

No comp. Rows follow `../comps/leakshield-wizard.html`. Locked buttons follow the Mode selector of `../comps/leakshield-overview.html`.

## Layout

Five cards.

1. "Device": rows "Serial" (mono), "Firmware version" (with the tag "Known" or the warning tag "Unknown"), "Maintenance indicator" (a progress bar and percent), "Device description" (input).
2. "Clock": "Device time" (live), "Time zone" (select), "Date format" and "Time format" (selects), button "Set from the PC".
3. "Standby behavior": a table with one row per USB state ("Active", "Suspended", "No USB host") and three switch columns: "Display", "Alarm evaluation", "Lighting".
4. "Backups": a table with "Time", "Reason" ("Before a write", "Manual", "Imported") and "Size", newest first. Row buttons: "Export", "Restore" with the padlock. Header buttons: "Back up now", "Import".
5. "Reset": "Clear the pump runtime chart" (button) and "Reset to factory settings" (destructive button with the padlock).

## States

| State | What the user sees |
|---|---|
| Empty | No backup yet: "No backup yet. nicecooling saves one before each write." |
| Loading | Row frames with grey bars |
| Device offline | Offline banner. Backups stay listed, and "Export" works. Other controls are disabled |
| Unknown firmware | Firmware row: warning tag "Unknown" and the text "nicecooling has no field layout for this firmware. Every field is read-only." No firmware update function exists: "To update the firmware, use aquasuite on Windows." |
| Maintenance indicator at 0 % | Warning row: "The vacuum pump and the valves need maintenance by Aqua Computer." |
| Clock not set | Warning in the Clock card: "The Leakshield has no battery for its clock. After a power loss, set the time again." |
| Alarm evaluation off in a state | Warning under the table: "With alarm evaluation off, the Leakshield does not detect a leak in that state." |
| Locked | "Restore" and "Reset to factory settings" have the padlock and open the polkit dialog. After a denial: "Not authorized. Nothing changed." |
| Restoring | Write status with the four steps. Other writes are disabled until it ends |
| Restore failed | "Restore failed: the read-back does not match the backup." Buttons "Try again" and "Close" |
| Alarm | Not used |

## Interactions

- "Restore" asks first: "Restore the backup from 2026-11-21 18:02? Every setting of the Leakshield goes back to that state. nicecooling saves a backup of the current state first." Buttons: "Cancel", "Restore".
- "Reset to factory settings" asks: "Reset the Leakshield to factory settings? You must set it up again with the wizard. Until then the leak protection can be wrong for your pump." The user types the word RESET to enable the button "Reset".
- "Clear the pump runtime chart" asks for a simple confirmation.
- "Import" opens the file dialog and adds the file to the list. It does not write the device.
- "Set from the PC" writes the time and shows "Applied".

## Copy

| Place | Text |
|---|---|
| Maintenance indicator | "100 % is new. At 0 % the vacuum pump and the valves need maintenance by Aqua Computer." |
| Standby behavior | "These settings say what the Leakshield does in each USB state." |
| Backups caption | "Backups are in /var/lib/nicecooling/backups, one folder per serial." |
