# Leakshield: System tab

No comp. Rows follow `../comps/leakshield-wizard.html`. Locked buttons follow the Mode selector of `../comps/leakshield-overview.html`.

## Layout

Six cards.

1. "Device": rows "Serial" (mono), "Firmware version" (with the tag "Known" or the warning tag "Unknown"), "Maintenance indicator" (a progress bar and percent, and the button "Reset service counter" at the end of the row), "Device description" (input).
2. "Clock": "Device time" (live), "Time zone" (select), "Date format" and "Time format" (selects), button "Set from the PC".
3. "Standby behavior": a table with one row per USB state ("Active", "Suspended", "No USB host") and three switch columns: "Display", "Alarm evaluation", "Lighting".
4. "Backups": a table with "Time", "Reason" ("Before a write", "Manual", "Imported") and "Size", newest first. Row buttons: "Export", "Restore" with the padlock. Header buttons: "Back up now", "Import".
5. "Refill history": a table with "Time", "Duration" and "Volume", one row for each of the 7 entries of the device, newest first. Header button: "Reset refill history".
6. "Reset": "Reset to factory settings" (destructive button with the padlock).

## States

| State | What the user sees |
|---|---|
| Empty | No backup yet: "No backup yet. nicecooling saves one before each write." No refill yet: "The refill history is empty." |
| Loading | Row frames with grey bars |
| Device offline | Offline banner. Backups stay listed, and "Export" works. Other controls are disabled |
| Unknown firmware | Firmware row: warning tag "Unknown" and the text "nicecooling has no field layout for this firmware. Every field is read-only." No firmware update function exists: "To update the firmware, use aquasuite on Windows." |
| Maintenance indicator at 0 % | Warning row: "The vacuum pump and the valves need maintenance by Aqua Computer." |
| Clock not set | Warning in the Clock card: "The Leakshield has no battery for its clock. After a power loss, set the time again." |
| Alarm evaluation off in a state | Warning under the table: "With alarm evaluation off, the Leakshield does not detect a leak in that state." |
| Locked | "Restore" and "Reset to factory settings" have the padlock and open the polkit dialog. After a denial: "Not authorized. Nothing changed." "Reset refill history" and "Reset service counter" have no padlock: they need the `write` authorization of any other change |
| Reset not supported | The daemon replies `NotSupported`. Text under the button: "This firmware has no such command." |
| Restoring | Write status with the four steps. Other writes are disabled until it ends |
| Restore failed | "Restore failed: the read-back does not match the backup." Buttons "Try again" and "Close" |
| Alarm | Not used |

## Interactions

- "Restore" asks first: "Restore the backup from 2026-11-21 18:02? Every setting of the Leakshield goes back to that state. nicecooling saves a backup of the current state first." Buttons: "Cancel", "Restore".
- "Reset to factory settings" asks: "Reset the Leakshield to factory settings? You must set it up again with the wizard. Until then the leak protection can be wrong for your pump." The user types the word RESET to enable the button "Reset".
- "Reset refill history" asks: "Delete the refill history on the Leakshield? This cannot be undone." Buttons: "Cancel", "Reset history".
- "Reset service counter" asks: "Reset the service counter of the Leakshield? Do this only after Aqua Computer did the maintenance. This cannot be undone." Buttons: "Cancel", "Reset counter".
- The Leakshield has no pump runtime reset. The chart "Pump runtime per day" is read-only.
- "Import" opens the file dialog and adds the file to the list. It does not write the device.
- "Set from the PC" writes the time and shows "Applied".

## Copy

| Place | Text |
|---|---|
| Maintenance indicator | "100 % is new. At 0 % the vacuum pump and the valves need maintenance by Aqua Computer." |
| Standby behavior | "These settings say what the Leakshield does in each USB state." |
| Reset buttons | "Reset refill history", "Reset service counter" |
| Backups caption | "Backups are in /var/lib/nicecooling/backups, one folder per serial." |
