# Product

<!-- impeccable:product-schema 1 -->

Source of each fact: `GLOSSARY.md`, `docs/adr/`, `docs/ui/components.md` and `openspec/changes/add-nicecooling-v1/`. The owner confirmed the content on 2026-11-22.

## Platform

web

The app is a native Linux desktop app (Rust, GPUI with gpui-component). The value `web` is the nearest value that the impeccable schema offers: the design references are HTML comps, and the iOS and Android guidance does not apply.

## Stack

Rust, GPUI and gpui-component, pinned to exact versions. D-Bus system bus to the root daemon `nicecoolingd`. SQLite for the sensor history. The comps in `docs/ui/comps/` are static HTML and are visual references only.

## Users

- The owner of one water-cooled Linux desktop, who built the loop and knows the hardware. Reference setup: Fedora, GNOME on Wayland, ASUS ROG CROSSHAIR VIII DARK HERO, one Loop with an Alphacool 17558 flow sensor and one Leakshield.
- The user is technical (Linux administrator level) and reads units and device terms without help.
- Version 1 supports several Boards through Board files and several Leakshields. It is tested on the reference desktop.

The user opens the window in four situations:

1. First run: detect the Board, set up the Loop, read the Leakshield.
2. Tuning: edit Curves, the Calibration table, Profiles and Profile rules.
3. Maintenance on the loop: Test, Measure fill level, Deaerate, Refill, settings wizard.
4. An Alarm: the window opens by itself, and the user must see the cause and the pending poweroff at once.

Most of the time the app runs in the background with a tray icon.

## Product Purpose

nicecooling controls the cooling of one Linux desktop: the fans and the pump on the Board, and the Aqua Computer LEAKSHIELD on the water loop. It replaces aquasuite and Windows for this job. Success: the loop stays safe when nobody looks at it, and the user can see and change every setting without a second tool.

## Positioning

- It is the only Linux tool that configures the Leakshield, runs its Actions and sends the Feed.
- It writes a Leakshield setting only when it can prove the write: backup, write, read back, compare. A field that is not a Verified field stays read-only, and the UI says why.
- Curves can read any sensor: Board, Leakshield, GPU, NVMe or a Derived sensor.

## Operating Context

- GNOME on Wayland, desktop window, minimum 1100x700, light and dark theme from the system.
- Background app with a StatusNotifierItem tray icon and desktop notifications. On GNOME the tray icon needs the AppIndicator extension.
- English and Italian. Italian strings are often 20-30% longer.
- polkit authorizes writes. Three controls need an admin password: factory reset, setting RELEASE directly and restoring a backup.
- Units: degrees Celsius, L/h, mbar, rpm, percent Duty, ml, ml/h, mbar/min.

## Capabilities and Constraints

- Scope of the UI: `docs/ui/components.md` lists every screen, state and widget of version 1.
- Terms: `GLOSSARY.md`. UI copy and docs use these terms exactly.
- Safety rules: `docs/adr/0007-safety-rules-for-profiles-actions-and-alarms.md` (Maintenance pause, Refill only in standby, the daemon handles leaks, no extra check on Profiles with critical values).
- The app is the session agent: `docs/adr/0005-desktop-app-is-the-session-agent.md`. The history has gaps while no session runs the app.
- With an unknown firmware version, every Leakshield field is read-only.
- Out of scope: Leakshield firmware update, Board RGB, Corsair devices, GPU fan control. Out of version 1: Experimental devices, a Verification write from the app, suspend Profile rules.
- Everything in the design must be buildable in GPUI. No web-only effects.
- Open: the content of some device reports, which the reverse engineering tasks answer.

## Brand Commitments

- Name: `nicecooling`, lowercase. Daemon: `nicecoolingd`.
- Device name in UI copy: Leakshield. Mode names in capitals: SHIELD, MONITOR, RELEASE.
- Visual standard: the app looks like a GNOME system tool. It sits next to GNOME Settings, Resources and Disks and matches their finish: neutral greys, blue accent, rounded cards.
- Signature: the Loop diagram. A Loop is drawn as a piping diagram with live values in instrument bubbles. The Leakshield Status schematic uses the same notation.
- No logo is fixed yet.

## Evidence on Hand

- Measured values of the reference desktop in `openspec/changes/add-nicecooling-v1/design.md`: 7 Channels with their names, raw flow rate 671 rpm that converts to about 115 L/h, Leakshield USB id and reports.
- No screenshots, no logo and no real history data exist yet. Values in the comps are synthetic and plausible.

## Product Principles

1. Safety state first. Alarm, locked, offline and unverified states read at a glance, before any other content.
2. Prove every write. The UI shows each step: pending, backup saved, written, read-back verified, or failed with a restore offer.
3. Say why. A read-only or locked control always carries its reason and the way out.
4. One vocabulary. The UI uses the glossary terms and the real units of the hardware.
5. Quiet when all is normal. The app stays in the tray, and a normal state uses little color.

## Accessibility & Inclusion

- Severity never depends on color alone. Each severity also has a shape, an icon and a text label.
- Text and controls meet WCAG 2.1 AA contrast in light and dark.
- Every interaction of the custom widgets (Curve editor, Calibration table, LED timeline) also works from the keyboard.
