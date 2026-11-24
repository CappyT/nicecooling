# Settings

No comp. The screen is a settings form: one card per group, one row per setting, as in `../comps/leakshield-wizard.html`.

## Layout

| Card | Rows |
|---|---|
| Language | "Language": select with "System", "English", "Italiano" |
| Start and tray | "Start at login": switch. "Tray icon": switch. The hint row for the AppIndicator extension, only when no tray host exists |
| Notifications | "Warnings": switch. "Profile switched by a rule": switch, off by default. "External change on a Leakshield": switch. Text row: "Alarm notifications are always on." |
| Daemon | "Connection": "Connected", "Offline" or "Down", with "Try again". "Daemon version" and "App version". "Board": the Board name and the Board file in use. "State directory": the path `/var/lib/nicecooling/`, read-only text, no control. There is no configuration file |
| About | App name, version, license, the link to the project page, and the note about Aqua Computer below |

## States

| State | What the user sees |
|---|---|
| Empty | Not used |
| Loading | Row frames with grey bars for the daemon rows only |
| Daemon offline | "Connection: Offline" with the offline icon and the text "Start the daemon: systemctl start nicecoolingd". Language and notification settings still work: the app holds them |
| Daemon down | "Connection: Down" with the octagon in the Alarm color and the text "systemd stopped the restarts of the daemon. nicecooling protection is off. The Board runs its Hardware curves from the BIOS snapshot." Then the two commands: `systemctl status nicecoolingd` and `systemctl restart nicecoolingd` |
| Version mismatch | Warning row in the Daemon card: "The daemon is version 1.1 and the app is version 1.0. Update both to the same version." |
| Tray not visible | Hint row with the Warning icon: "The tray icon is not visible. On GNOME, turn on the AppIndicator extension." Link "How to turn it on" |
| No Board file | "Board: ASRock X570 Taichi. No Board file: Channels and sensors have the names of the kernel." |
| Language changed | The app changes at once, without a restart |
| Alarm | Not used |

## Interactions

- Each switch applies at once. The app writes the switches to `settings.toml` and "Start at login" to the autostart override (`docs/storage.md`).
- "Quit" is in the primary menu, not here.

## Copy

| Place | Text |
|---|---|
| Start at login | "The app starts without a window and shows its tray icon." |
| Tray icon off | "Without the tray icon, open the window from the app list." |
| About note | "nicecooling is not a product of Aqua Computer. LEAKSHIELD and aquasuite are names of Aqua Computer GmbH & Co. KG." |
| Firmware | "nicecooling does not update the firmware of any device." |
