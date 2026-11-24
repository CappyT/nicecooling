# UI component inventory

This inventory lists every screen, state and widget that the nicecooling desktop app needs for version 1. It comes from the decisions in `GLOSSARY.md`, `docs/adr/` and `openspec/changes/add-nicecooling-v1/`, and from the Leakshield manual. The design work in `docs/ui/` must cover each item.

## Constraints

- Toolkit: GPUI with gpui-component (sidebar, tabs, select, slider, switch, input, table, dialog, notification, stepper, progress, area and line charts, plot, popover, menu, tag, settings form, resizable panels). Anything else is a custom GPUI element.
- Linux desktop, GNOME on Wayland, light and dark theme from the system.
- Languages: English and Italian. Layouts must hold Italian strings, which are often 20-30% longer.
- The app runs in the background with a tray icon. It opens a window on request or on an Alarm.
- Units: degrees Celsius, L/h, mbar, rpm, percent Duty, ml, ml/h, mbar/min.

## App shell

- Sidebar navigation: Overview, Loops, Channels, Sensors, one entry per Leakshield, Profiles, Alarms, History, Settings.
- Status bar: daemon connection, active Profile (manual, by rule or the Default Profile), count of active Alarms.
- Global banners, one style per severity: daemon offline; "daemon down, Board on BIOS curves" when the restart limit is reached; Leakshield unreachable; Leakshield with unknown firmware (read-only); Immediate switching on; poweroff countdown running.
- Write feedback for every device write: pending, backup saved, written, read-back verified, or failed with an offer to restore the backup.
- Authorization: no control has a lock icon for authorization and the app asks no password. The read-only lock of a field that is not Verified stays (`widgets.md`). The system polkit dialog shows only for a user outside the group `wheel`. The app warns before an action that lowers the protection and does not refuse it.

## Tray icon

- Icon states: normal, Warning, Alarm, daemon offline.
- Menu: one-line status per Loop (water temperature, flow, pressure), active Profile with a submenu to switch, "Cancel poweroff" while a countdown runs, "Open nicecooling", "Quit".

## Notifications

- Alarm notification with critical urgency and a "Cancel poweroff" action while a countdown runs. The notification has no action when the server lacks the `actions` capability. The tray menu and the window then offer the cancel.
- Leakshield unreachable Alarm.
- External change detected on a Leakshield, with the note that the next Profile activation overwrites it.
- Profile switched by a rule (silent by default, option in Settings).
- Pump-stop Alarm.

## Screens

### Overview

- One card per Loop: water temperature, flow (calibrated, with out-of-range flag), pump speed and Duty, Leakshield pressure against its target, Mode, status.
- Key temperatures: CPU, GPU, chipset, VRM.
- Active Alarms list with links to their source.
- Active Profile card with the rule that activated it, or the Default Profile.

### Loops

- Loop list and editor: Pump channels, flow sensor, water temperature sensor, Leakshield link.
- Pump floor control (percent) with an explanation.
- Calibration table editor: table of points plus a plot, preset for the Alphacool 17558, live marker at the current raw rate.
- Feed status per Leakshield: last values sent, age.

### Channels

- Channel list: name, rpm, Duty, Input, Curve or fixed Duty, Pump channel tag.
- Curve editor: XY plot of Input against Duty, draggable points, add and remove points, live operating point, shaded zone under the Pump floor, Input picker.
- Rename a Channel.
- BIOS snapshot panel: what the chip does when the daemon stops.

### Sensors

- All sensors grouped by device (Board chips, Leakshield, GPU, NVMe), with live value and sparkline.
- Rename and hide a sensor.
- Derived sensor editor: maximum, mean, difference; source picker.
- Calibration tables of flow sensors (same editor as in Loops).

### Leakshield (one entry per device)

Tabs: Overview, Settings, Sensors, Display, Lighting, Alarms, Status, System, Fields.

- Overview: pressure gauge with current, minimum, target and maximum; status; fill level percent; volume change ml/h; pressure change mbar/min; Mode selector (SHIELD, MONITOR, RELEASE); Action bar; pressure chart; pump runtime per day chart (tight systems stay under 15 s per day).
- Action views:
  - Test: pressure input 80 to 450 mbar; live chart with a selectable source (absolute pressure, difference to reference, pressure change); "Reference" button; zoom; Cancel.
  - Measure fill level: progress bar; warning that protection is off during the measurement.
  - Deaerate: two progress bars (current cycle, overall).
  - Refill: an instruction card, not a button: start Refill with the device button with the PC in standby.
- Settings wizard (stepper), also usable to edit single steps:
  - Reservoir: model list or custom total volume, air volume at maximum and minimum fill level, automatic fill-level measurement toggle.
  - Pump: pump type list or custom maximum pressure and maximum speed; speed source is the Loop's Pump channel.
  - Flow: source is the Loop's flow sensor.
  - Pressure: adaptive pressure toggle, expert values, time-controlled vacuum generation with two times.
  - Standby: default speed and flow, reduced vacuum toggle and value.
  - Mode.
  - Summary: table of changes with notes and warnings; Apply.
- Sensors: names of the 18 device sensors; the 12 Software sensors with data source picker, unit, scale and offset.
- Display: grid of 10 pages with previews; template gallery dialog; per-page template settings (sensor pickers, icons, logo upload with conversion preview); show in standby and in normal operation toggles; display settings (brightness, brightness when inactive, display off, automatic page change and interval, rotate 180 degrees, invert, automatic invert, off in standby).
- Lighting (RGBpx): LED timeline with 6 integrated LEDs and the external LEDs; up to 8 controllers as draggable and resizable color bars, vertical order is priority; effect picker dialog; effect parameters (colors, speed, sensor-driven parameters); duplicate and delete.
- Alarms: leakage Warning threshold slider (volume change) with acoustic toggle; small and large leak Alarms; fill level Warning and Alarm thresholds; system Alarm. For each: the device output (signal short press, signal long press, key sequence, none) and the nicecooling alarm response (none, notify, poweroff with delay). Overlap warnings follow ADR-0007.
- Status: schematic of the Leakshield (vacuum pump, valves, active parts highlighted, measured values); event list with time and details; export.
- System: serial, firmware version, maintenance indicator percent, device description; factory reset; reset service counter next to the maintenance indicator; refill history table with a reset button; backup list with export, import and restore (restore locked); clock (device time, time zone, date and time format, sync now); standby behavior per USB state (display, alarm evaluation, lighting).
- Fields: every field with its verification state from the registry (code-defined, plausible read, Verification write passed) and risk class; explanation of why a field is read-only. The tab has no control that writes a verification value.

### Profiles

- Profile list: active marker (manual, rule or Default Profile), create, duplicate, rename, delete (not for the Default Profile), activate, revoke manual activation. The number of Profiles is not limited.
- Profile state after a switch: applied, or "Curves applied, Leakshield failed" with the offer to restore the backup.
- Profile content: Curve per Channel; the Leakshield slot select (1 to 4) with the Profiles that share the slot; a summary of the Leakshield settings of the slot; "differs from other slots" warnings with "apply to all slots".
- Rule group editor: ordered list of groups, each with its rules and its target Profile. Rule kinds: sensor comparison with a hold time, process list (one of, all of, none of), time window with weekdays, interval timer. No hotkey rule. One Rule delay timer state per group.
- Options: Rule delay and Profile dwell values; Immediate switching with a warning dialog and a persistent checkbox.

### Alarms

- Active Alarms and Alarm history.
- Poweroff countdown dialog: Alarm, source, seconds left, "Cancel poweroff". Any session's app can cancel.
- Alarm response table for every Warning and Alarm type (Leakshield and nicecooling): alarm response and delay.

### History

- Charts of any set of sensors over 1 h, 24 h, 48 h, 7 days, 30 days and 1 year; 1 s data for 48 h, 1 min aggregates (mean, minimum, maximum) after that.
- Sensor multi-select, pan and zoom, gaps shown where no session recorded data.

### Settings

- Language: system, English, Italiano.
- Start at login; tray icon; hint to enable the GNOME AppIndicator extension when the tray is not visible.
- Notification options.
- Daemon connection and version; about.

### First run

- Board detection result and Board file in use.
- Loop setup: Pump channel, flow sensor with Calibration table preset, water temperature sensor, Leakshield link.
- BIOS snapshot explanation.
- Read-only state of Leakshield fields that the registry does not mark as verified.

## Custom widgets

These have no gpui-component equivalent and need a design spec of their own:

- Curve editor (XY plot with draggable points, operating point, floor zone).
- Calibration table editor (table bound to a plot).
- Pressure gauge (current against minimum, target and maximum).
- LED timeline editor for RGBpx.
- Display page preview and logo conversion preview.
- Leakshield schematic.
- Sensor picker (searchable, grouped by device, live values).
- Process picker.
- Diff table (Profile against device, wizard summary).
- Countdown dialog.
- Verification state badge and locked-field indicator.
