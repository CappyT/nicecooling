# Proposal

## Why

On Linux, the Aqua Computer LEAKSHIELD is only half supported: the kernel driver `aquacomputer_d5next` reads its sensors, but no tool configures it, runs its Actions or sends it the Feed (pump speed and flow) it needs to compute the loop pressure. Board fan control exists in hwmon, but the tools for it cannot use Leakshield data or a calibrated flow sensor. nicecooling gives one Linux desktop full control of its water cooling, without aquasuite and without Windows.

## What Changes

- A root daemon, `nicecoolingd`, that discovers hwmon sensors and Channels on any Board, runs software Curves, and restores the BIOS snapshot when it stops. It reads each device in its own task, feeds a systemd watchdog, and keeps all its state in `/var/lib/nicecooling/`.
- Loops that group Pump channels, a flow sensor with a Calibration table, a water temperature sensor and a Leakshield; per-Loop Pump floor and pump-stop Alarm.
- Full Leakshield support except firmware update: Modes, Actions, every setting of the aquasuite wizard, alarms, display pages, Software sensors, RGBpx lighting, clock, backup and restore. Writes are limited to Verified fields.
- Alarm handling with a configurable alarm response per Warning and Alarm, including a poweroff after a cancellable delay that ends in at most 60 s.
- Profiles that bundle Curves and a full Leakshield snapshot, with a Default Profile and Profile rules on process, time and sensor thresholds.
- A D-Bus system API with polkit authorization.
- A GPUI desktop app: tray icon, notifications, sensor history, Curve editor, Leakshield pages, English and Italian.
- An RPM spec for Fedora, with a drop-in for `poweroff.target`.

## Capabilities

### New Capabilities

- `hardware-discovery`: discovery of Boards, sensors and Channels; Board files and user names; Derived sensors; Calibration tables; Loops.
- `fan-control`: the failure model, Curves, the Pump floor, the pump stop, Channel reconciliation, the BIOS snapshot and its restore.
- `leakshield-settings`: reading, decoding, writing, backing up and restoring Leakshield settings, the field registry, external change detection and the device clock.
- `leakshield-operation`: Modes, Actions, the Feed, Software sensor values, hot-plug, and Leakshield status and events.
- `alarms`: Leakshield Alarms and nicecooling Alarms, their alarm responses, the poweroff delay and its time bound, and the coordination with the device's own alarm outputs.
- `profiles`: Profiles, the Default Profile, Profile rules, Rule delay, Profile dwell, Immediate switching and the Maintenance pause.
- `control-api`: the D-Bus system API and its polkit authorization.
- `desktop-app`: the GPUI session agent: windows, tray icon, notifications, sensor history and languages.

### Modified Capabilities

None: this is the first change of the project.

## Impact

- New Rust workspace with a daemon, a desktop app and shared protocol crates.
- System integration: a systemd unit with a watchdog, a drop-in for `poweroff.target`, a D-Bus system bus policy, a polkit policy, an XDG autostart entry, and the state directory `/var/lib/nicecooling/`.
- Dependencies: GPUI and gpui-component for the app, a D-Bus library, hidraw access and `nusb` for the bulk endpoint, SQLite in the app.
- Reverse engineering of aquasuite for interoperability. No decompiled code enters the repository (see `docs/adr/0001-leakshield-protocol-from-aquasuite-code.md`).
