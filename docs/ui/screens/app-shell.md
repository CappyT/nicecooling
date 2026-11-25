# App shell

Comps: every file in `../comps/` shows the shell. `../comps/overview.html` shows the poweroff banner. `../comps/settings-network.html` shows the Response pause banner.

The shell is the frame of every screen: sidebar, header bar, global banners, content and status bar.

## Layout

| Area | Size | Content |
|---|---|---|
| Sidebar | 220 px | App name, navigation entries in groups, Settings at the bottom |
| Header bar | 46 px | Page title, subtitle on a device page, primary menu button, one round close button. A drag on the bar or on the app name of the sidebar moves the window. A double click maximizes or restores it |
| Banners | 46 px each, 64 px for the poweroff banner | Under the header bar, above the tab strip |
| Content | rest | The screen. It scrolls vertically |
| Status bar | 28 px | Daemon connection, active Profile, Alarm count |

### Sidebar entries

1. Overview, Loops, Channels, Sensors.
2. Group "Devices": one entry per Leakshield. The label is the device description, or "Leakshield". Two devices with the same label also show the last 5 digits of the serial. A device with an active Alarm, a Warning or no connection has the severity icon at the end of its row.
3. Group "Control": Profiles, Graphs, Alarms, History. Graphs has the icon `graph` of the sprite: three nodes and two connections. Alarms shows the count of active Alarms and Warnings. "Daemon down" counts as one.
4. Settings, at the bottom.

A long label truncates with an ellipsis. Icons and counts keep their width.

### Status bar

| Item | Text | Click |
|---|---|---|
| Daemon | Green dot and "Daemon connected", the offline icon and "Daemon offline", or the octagon and "Daemon down" in the Alarm color | Settings, daemon section |
| Profile | "Profile: Quiet (rule)" or "Profile: Gaming (manual)". With the daemon down: "Profile: none" | Profiles |
| Alarms | "No active Alarms", "2 Warnings", "1 Alarm", or "1 Alarm, poweroff in 0:21". With the daemon down: the octagon and "Protection off" in the Alarm color | Alarms |

## Global banners

A banner spans the content width. The order from the top is fixed. A screen shows three banners at most; the last one then says "2 more notices" and opens a list. The Response pause banner is one of the first three, so no other banner hides it.

| Order | Banner | Style | Text | Button |
|---|---|---|---|---|
| 1 | Poweroff countdown | Alarm, solid | "Large leak Alarm on Leakshield" and "Main loop. The PC powers off when the countdown ends." Countdown "0:21" at the right | "Cancel poweroff" |
| 2 | Daemon offline | Offline tint | "The daemon is offline. The values are not live, and you cannot change settings." | "Try again" |
| 2 | Daemon down | Alarm, solid | "The daemon is down. nicecooling protection is off." and "No Curve, no Feed, no alarm response. The Board runs its Hardware curves from the BIOS snapshot." | "Show how to start it" |
| 3 | Response pause | Warning tint | "Response pause: 3 power responses in a row" and "No Graph response runs by itself. The Graph sensors still work." | "Turn the Graph responses on" |
| 4 | Unknown firmware | Warning tint | "Leakshield firmware 1042 is unknown. Every field is read-only." | "Open Fields" |
| 5 | Immediate switching | Warning tint | "Immediate switching is on. Rule groups change the Profile without delay." | "Open Profiles" |

The poweroff banner shows on every screen while a countdown runs. No screen can hide it.

The Response pause banner shows on every screen from the third poweroff or reboot response in a row until the user selects "Turn the Graph responses on". It stays over a restart of the app. The button ends the pause at once and asks no password. The banner has the Warning icon and two lines of text.

## States

| State | Behavior |
|---|---|
| Loading | At start the content shows "Connecting to the daemon" with a spinner for at most 2 s. Then the offline state applies |
| Daemon offline | Banner 2. Live values show in `fg-dim` with their age. Every control that writes is disabled, and its tooltip says "The daemon is offline." |
| Daemon down | The service is in the failed state: systemd gave up its restarts. Banner 2 in its "Daemon down" form replaces the offline form: it has the solid Alarm field, because the state is as serious as an Alarm. "Active Alarms" lists "Daemon down", and the window opens by itself. "Show how to start it" opens a dialog with `systemctl status nicecoolingd` and `systemctl restart nicecoolingd`. A Leakshield still protects the Loop by itself |
| Not authorized | A write that polkit refuses shows "Not authorized. Nothing changed." next to the control |
| Alarm | Banner 1, the Alarm count in the status bar and in the sidebar, the icon on the device entry. The window opens by itself and shows the Overview |
| Response pause | Banner 3. No Graph response runs by itself. "Run once" in the Graph editor still works. The Graph sensors, the Curves and the alarm responses of the daemon work as before |
| Window closed | The close button hides the window. The app stays in the tray |

## Write feedback

Every write shows its steps next to the control that started it: see "Write status" in `../widgets.md`. A Leakshield write has four steps: "Backup saved", "Written", "Read back", "Verified". A failed write keeps its message until the user acts, and offers "Restore the backup".

## Interactions

- Alt+1 to Alt+9 open the first nine sidebar entries. F10 opens the primary menu. Ctrl+W hides the window.
- Tab and Shift+Tab move the focus through the controls. The control with the focus has the focus ring. Enter and Space press it. In a segmented control, Left and Right move the focus between its items. On a select, Down opens the list, Up and Down move the mark, Enter picks the marked row and Escape closes the list. A disabled control takes no focus.
- While a dialog is open, the focus stays in it. The focus moves into the dialog when it opens. Tab and Shift+Tab go through its controls and through "Cancel poweroff" of the banner. No key reaches the screen under the scrim. When the dialog closes, the control that opened it has the focus again.
- Primary menu: "Keyboard shortcuts", "About nicecooling", "Quit".
- Escape closes a popover or a dialog. It never cancels a poweroff.

## Copy

| English | Italian (length check) |
|---|---|
| Overview | Panoramica |
| Loops | Loop |
| Channels | Canali |
| Sensors | Sensori |
| Devices | Dispositivi |
| Profiles | Profili |
| Graphs | Grafi |
| Alarms | Allarmi |
| History | Cronologia |
| Settings | Impostazioni |
| Daemon connected | Demone connesso |
| No active Alarms | Nessun Allarme attivo |
| Cancel poweroff | Annulla lo spegnimento |
| Turn the Graph responses on | Riattiva le risposte dei Grafi |
