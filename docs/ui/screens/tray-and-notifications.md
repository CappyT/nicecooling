# Tray icon and notifications

No comp: the desktop draws these parts. The app only gives the icon, the menu entries and the notification text.

## Tray icon

The icon is a symbolic icon (one color, the panel colors it). The state is a shape, because a panel can be light or dark. The files are in `../icons/`, and `../DESIGN.md` has their rules in "Icons". Preview: `../icons/preview-light.png`, `../icons/preview-dark.png`.

| State | Icon | File |
|---|---|---|
| Normal | The app symbol: a rounded loop with a drop in it | `nicecooling-symbolic.svg` |
| Warning: at least one active Warning, no Alarm | The app symbol with a triangle at the lower right | `nicecooling-warning-symbolic.svg` |
| Alarm: at least one active Alarm, or the daemon is down | The app symbol with an octagon at the lower right. The status is "NeedsAttention" | `nicecooling-alarm-symbolic.svg` |
| Offline: the app cannot reach the daemon | The app symbol dimmed, with a slash | `nicecooling-offline-symbolic.svg` |

The tooltip of the icon says the state in words: "nicecooling: 1 Alarm, poweroff in 0:21", "nicecooling: daemon down, protection is off".

## Tray menu

| Entry | Text | Notes |
|---|---|---|
| Countdown | "Cancel poweroff (0:21)" | First entry, only while a countdown runs |
| One line per Loop | "Main loop: 31.4 °C, 115 L/h, 298 mbar" | A Loop without a Leakshield has no pressure. A missing value is "--". Not clickable |
| Profile | "Profile: Quiet" with a submenu | The submenu lists every Profile with a mark on the active one, then "Revoke manual activation" when one is held |
| Open | "Open nicecooling" | Opens the window on the last screen |
| Quit | "Quit" | Stops the app. The daemon continues. A dialog says so the first time |

When the daemon is offline, the Loop lines are one line: "Daemon offline", and the Profile submenu is disabled. When the daemon is down, the first entry is "Daemon down: nicecooling protection is off", and the icon is in the Alarm state.

## Desktop notifications

| Event | Title | Body | Buttons | Urgency |
|---|---|---|---|---|
| Alarm with poweroff | "Large leak Alarm" | "Leakshield, Main loop. The PC powers off in 30 s." | "Cancel poweroff", "Open nicecooling" | Critical. It stays until the countdown ends |
| Alarm, notify only | "Fill level Alarm" | "Leakshield, Main loop. Fill level 18 %, limit 20 %." | "Open nicecooling" | Critical |
| Pump-stop Alarm | "Pump stopped" | "Main loop: Waterpump reads 0 rpm. Every Channel is at 100 %. The PC powers off in 30 s." | "Cancel poweroff", "Open nicecooling" | Critical |
| Leakshield unreachable (Alarm) | "Leakshield unreachable" | "Main loop: no data from the Leakshield for 10 s. Check its USB cable." | "Open nicecooling" | Critical |
| Daemon down | "nicecooling protection is off" | "The daemon is down. The Board runs its Hardware curves. No Feed and no alarm response until the daemon runs." | "Open nicecooling" | Critical |
| Warning | "Leak Warning" | "Leakshield, Main loop. Volume change 3.2 ml/h, limit 2.0 ml/h." | "Open nicecooling" | Normal |
| Lost Input | "Input lost" | "Chassis Fan 3 runs at 100 % until GPU temperature returns." | "Open Channels" | Normal |
| External change | "Leakshield setting changed outside nicecooling" | "Display brightness: 40 % -> 80 %. The next Profile activation overwrites it." | "Open Leakshield" | Normal |
| Profile switched by a rule | "Profile: Gaming" | "Rule 1: process steam runs." | none | Low. Off by default |
| Poweroff cancelled | "Poweroff cancelled" | "The Large leak Alarm is still active." | "Open nicecooling" | Normal |

- The title names the event. The body names the source and the consequence. No body is longer than two lines.
- "Cancel poweroff" works from the notification without opening the window. The notification then changes to "Poweroff cancelled".
- Every Alarm notification has the critical urgency.
- "Cancel poweroff" is in three places: the notification, the tray menu and the window.
- When the app does not run, no notification shows. The daemon still runs the alarm response.

## Tray not visible

On GNOME the tray icon needs the AppIndicator extension. When the app finds no StatusNotifier host, Settings shows a hint: "The tray icon is not visible. On GNOME, turn on the AppIndicator extension." with the link "How to turn it on". The window then also opens at login, unless the user turns that off.
