# Design

## Context

See `proposal.md` for the motivation, `GLOSSARY.md` for the terms and `docs/adr/` for the decisions that are hard to reverse. The contracts of this change are `crates/nicecooling-api/dbus/io.github.cappyt.NiceCooling1.xml` and `docs/api.md` (D-Bus API), and `docs/storage.md` (files and schemas). The owner approved them. This design cites them and does not repeat them.

Facts measured on the reference desktop (Fedora 45, kernel 7.3):

- **Board.** ASUS ROG CROSSHAIR VIII DARK HERO. `nct6775` exposes `nct6798`: 7 Channels with `pwmN` and `pwmN_enable`. Every `pwmN_enable` read 5. Each Channel has a 5-point Hardware curve and writable `pwmN*` attributes: 22 for each Channel, except `pwm2` (the CPU Fan), which has 28 because of six `pwm2_weight_*` attributes. A Channel in manual mode at raw 255 reads `pwmN_enable` 0. `nct6775` caches its readings for about 1.5 s. The chip also has many temperature and voltage inputs.
- **Embedded controller.** `asus_ec_sensors` exposes `asusec`: Chipset, CPU, Motherboard, T_Sensor, VRM, Water_In, Water_Out, CPU_Opt fan, Water_Flow, CPU current and `in0` (CPU Core). Water_In and Water_Out have no probe connected. Channel names come from LibreHardwareMonitor: 1 Chassis Fan 1, 2 CPU Fan, 3 Chassis Fan 2, 4 Chassis Fan 3, 5 High Amp Fan, 6 Waterpump, 7 AIO Pump.
- **GPU.** The `amdgpu` hwmon device exposes `pwm1` and `pwm1_enable`.
- **Loop.** The pump is on Channel 6. An Alphacool 17558 sensor on W_FLOW and T_SENSOR gives flow and water temperature. Its datasheet table converts a raw rate of 671 rpm to about 115 L/h. No air temperature probe exists.
- **Leakshield.** The Aqua Computer LEAKSHIELD, USB `0c70:f014`. Interface 0: vendor bulk, EP 0x02 OUT and 0x81 IN. Interface 1: HID with report 1 input 836 B (sensors), report 2 output 10 B, report 3 feature 749 B (settings), report 5 feature 1027 B and report 8 feature 3090 B. Interface 2: HID keyboard, used by the key-sequence output of the device alarms. Report 1 bytes 13-14 hold the firmware version, big-endian (fw1019 on the reference device before the update to 1022).
- **Kernel drivers.** The in-tree driver `aquacomputer_d5next` reads report 1 and exposes the hwmon device `leakshield`. Its `fan2_input` and `fan3_input` show the Feed and return ENODATA until a Feed arrives. The out-of-tree driver of the same name (GitHub `aleksamagicka/aquacomputer_d5next-hwmon`) also sends the Feed message on EP 0x02: 51 bytes, 49 bytes that start with `0x04`, then a big-endian CRC-16/USB over those 49 bytes.
- **systemd.** Since version 257, logind obeys block inhibitors also for root. `PowerOffWithFlags` with `SD_LOGIND_SKIP_INHIBITORS` (0x10) skips them. Delay inhibitors still delay the job by up to `InhibitDelayMaxSec` (default 5 s).
- **aquasuite.** The Aqua Computer Windows software. Its installer is a downloader with no device code. The device logic is in the aquasuite .NET service. Neither X.75 nor X.96 accepts firmware 1019: X.75 accepts 1020 and X.96 accepts 1022. The wire format is the same in both versions (`docs/protocol/leakshield.md`).
- **VM.** `win11-aquasuite` is a Windows 11 VM on the workstation with a virtual GPU. It runs aquasuite for firmware updates (ADR-0004). `docs/hardware-checks/firmware-update.md` gives the procedure.

Terms that the safety decisions use (`GLOSSARY.md` has the full list):

- **Duty**: the output level of a Channel, in percent. A **fixed Duty** is a Duty that the user sets on a Channel that has no Curve.
- **Hardware curve**: the 5-point curve that the Super I/O chip applies without the host.
- **Pump channel**: a Channel that drives a Loop pump. The **Pump floor** is the minimum Duty of that Channel.
- **Feed**: the pump speed and flow that the host sends to the Leakshield. The **Feed message** is the message on the bulk endpoint that carries them.
- **Warning** and **Alarm**: events that signal a problem. The Leakshield and nicecooling both raise them.
- **Alarm response**: what nicecooling does for a Warning or an Alarm: none, notify, or poweroff after a delay.
- **Mode**: the persistent state of the Leakshield: SHIELD, MONITOR or RELEASE. RELEASE releases the vacuum so that the loop can be opened. The protection is off.
- **Action**: a temporary Leakshield procedure: Test, Measure fill level, Deaerate or Refill. In this design, "polkit action" always names a polkit permission, never an Action.
- **Safety field**: a Leakshield setting that affects the leak protection. **Verified field**: a setting that nicecooling may write.
- **Leakshield slot**: a number from 1 to 4 that is also the device profile number (report 1 `active_profile`, report 3 `device_profile`). The Leakshield settings are stored per slot. Each Profile references one slot, and a Profile activation writes the Leakshield only when the slot number differs from the number of the device.
- **Profile rule**: one condition of a Rule group. A **Rule group** holds when all its rules hold, and then activates its Profile. The **Rule delay** is how long a group must hold. The **Profile dwell** is the minimum time the Profile stays active.
- **BIOS snapshot**: the writable `pwmN*` attributes of all Channels, as the Board had them before nicecooling took control.

## Goals / Non-Goals

**Goals:**

- Without a user session, the daemon runs the Curves, sends the Feed message, evaluates the Alarms and runs the alarm responses.
- From the end of the alarm delay to power off, 60 s is the goal. It is not a hard bound: see the poweroff drop-in in the Alarms decisions.
- nicecooling writes only Verified fields to the Leakshield.
- The domain core (Loops, Curves, Profiles, Rule groups, Alarms) runs in tests without hardware.

**Non-Goals:**

- Leakshield firmware update, Board RGB and Corsair devices, GPU fan control (ADR-0004).
- Write support for other Aqua Computer devices (ADR-0004). Their sensors still appear through generic hwmon discovery when the kernel driver exposes them.
- Profile rules on suspend and resume, and hotkeys as a rule kind.
- Water-to-air delta T in this version: no air probe exists. The Derived sensor type difference stays available.
- A dedicated CLI: `busctl` covers scripting.
- A hand-edited configuration file.
- Compatibility with aquasuite profile files.

## Repository and install layout

The Cargo workspace has these crates under `crates/`:

| Crate | Content |
|---|---|
| `nicecooling-core` | Domain types, the tick stages, Curves, Profiles, Rule groups, Alarms and a simulated clock. |
| `nicecooling-protocol` | Leakshield report codecs, the field registry (`registry/leakshield.toml`, compiled in), the Feed message and the CRC. |
| `nicecooling-hwmon` | The sysfs layer, discovery and Board files. |
| `nicecooling-api` | The zbus interface definitions, the error names and the authorization map. |
| `nicecoolingd` | A library and a binary. The sources, the value table, the tick runner, the state store, the write worker, the D-Bus server, polkit and logind. |
| `nicecooling` | The GPUI app. |

Other directories:

- `tools/nicecooling-verify`: development binary for Verification writes. It depends on the `nicecoolingd` library (device access, write path, save-settings timer and backups), on `nicecooling-protocol`, on `zbus` and on `tokio`, and enables no test feature. It is not packaged.
- `tools/re/`: the identifier check, and the generators of the registry and of the hardware check documents. The reverse engineering notes and tools are not in the repository.
- `data/boards/` and `data/calibration/`: Board files and Calibration table presets.
- `packaging/`: the spec file, the unit, the drop-in, the bus policy, the polkit policy and rule, the autostart and desktop entries, and the Containerfiles and scripts of the packaging checks. The icons are in `docs/ui/icons/`.
- `.github/workflows/ci.yml`: the CI workflow.
- `docs/protocol/`, `docs/hardware-checks/`, `tests/fixtures/leakshield/fw1019/` and `fw1022/`.

Install paths:

| Source | Install path |
|---|---|
| `crates/nicecooling` | `/usr/bin/nicecooling` |
| `crates/nicecoolingd` | `/usr/libexec/nicecoolingd` |
| `packaging/nicecoolingd.service` | `/usr/lib/systemd/system/nicecoolingd.service` |
| `packaging/poweroff.target.d/50-nicecooling.conf` | `/usr/lib/systemd/system/poweroff.target.d/50-nicecooling.conf` |
| bus policy, polkit policy, polkit rule | the three paths in `docs/api.md`, section Authorization |
| autostart entry (runs `nicecooling --background`) | `/etc/xdg/autostart/io.github.cappyt.NiceCooling.desktop` |
| desktop entry | `/usr/share/applications/io.github.cappyt.NiceCooling.desktop` |
| icons from `docs/ui/icons/` | `/usr/share/icons/hicolor/scalable/apps/io.github.cappyt.NiceCooling.svg`, the symbolic icons in `/usr/share/icons/hicolor/symbolic/apps/` |
| `data/boards/`, `data/calibration/` | `/usr/share/nicecooling/` |

The RPM builds offline from a vendored crates tarball (`cargo vendor`, `cargo build --offline`) and makes one binary package `nicecooling` that holds the daemon and the app.

The daemon state is in `/var/lib/nicecooling/`. The app history is in `$XDG_DATA_HOME/nicecooling/history.db` and the app settings are in `$XDG_CONFIG_HOME/nicecooling/settings.toml` (`docs/storage.md`). Each task in `tasks.md` names its crate and its files.

## Dependencies

| Crate | Version | Use |
|---|---|---|
| `tokio` | 1.54 | Async runtime of the daemon. |
| `zbus` | 6.5 | D-Bus server and client. |
| `zbus_polkit` | 6.0 | polkit checks. |
| `sd-notify` | 0.5 | `READY`, `WATCHDOG` and `STATUS` messages. |
| `nusb` | 0.2.9 | Bulk Feed message and hot-plug watch. No libudev. |
| `rustix` | 1.1 | hidraw ioctls. No hidapi. |
| `crc` | 3.4 | `CRC_16_USB`. |
| `sha2` | 0.11 | Backup hash. |
| `serde`, `serde_json` | 1.0 | Daemon state. |
| `toml` | 1.2 | Board files, presets and the registry. |
| `rusqlite` | 0.43, bundled SQLite | History. |
| `ksni` | 0.3.7, feature `blocking` | StatusNotifierItem and DBusMenu. GPUI has no Tokio runtime, so the app uses the blocking API. |
| `notify-rust` | 4.19 | Notifications. |
| `rust-i18n` | 4.2 | English and Italian. |
| `gpui-component` | 0.10.0 | App widgets. |
| `gpui-pre` | `=0.3.12` | GPUI. |
| `gpui-pre-platform` | `=0.3.12`, features `wayland` and `x11` | The GPUI application entry point (`gpui_platform::application()`). |

CI runs on GitHub Actions on `ubuntu-24.04`: the identifier check of D9, `cargo fmt --check`, `cargo clippy` with warnings as errors, and `cargo test --workspace`. The local hook enforces the commit format. CI has no commit check.

Rejected: `hidapi` and `libudev`. Both need C libraries. `nusb` watches hot-plug in pure Rust, and the ioctls of hidraw are few.

## Decisions

### D1. Processes (ADR-0002, ADR-0005)

- `nicecoolingd`: root, systemd service, Tokio. It owns hwmon writes, hidraw and the bulk endpoint, and it runs the tick, the Rule groups and the Alarms.
- `nicecooling`: GPUI app, started at login with `--background` through XDG autostart. It owns the tray icon, the notifications and the history.
- Shared crates: the domain core, the protocol codecs, the hwmon layer and the D-Bus interface definitions.

Rejected: one process in the user session, because Curves and the Feed message must run with no user logged in.

### D2. State storage

The daemon owns all state in `/var/lib/nicecooling/`: Loops, names, Calibration tables, Curves, Profiles, Rule groups, alarm responses, options, the BIOS snapshot and the backups. `docs/storage.md` defines the files and their schemas. Nobody hand-edits a configuration file. Scripts use `busctl`.

- systemd creates the directory with `StateDirectory=nicecooling` and `StateDirectoryMode=0700`.
- Each Profile lives in one place. The Leakshield settings and their binary assets live per Leakshield and per slot.
- Every write is atomic: temporary file, `fsync`, rename, `fsync` of the directory.
- Each file carries a schema version. The daemon keeps a last-known-good copy.
- A file with a higher schema version is not written. The daemon loads the last-known-good copy and raises the `state-fallback` Warning.
- When the state file and the last-known-good copy both fail to load, the daemon starts without Curves, leaves every Channel on its Hardware curve, keeps the broken files and raises the `state-fallback` Warning.
- The Curve editor applies a change live in memory. The daemon persists it 2 s after the last change. The API has no event for the end of a drag.

Rejected: a hand-edited file in `/etc` that the daemon also rewrites. It needs a reload rule, comment preservation and an RPM rule for upgrades.

### D3. Device access (hardware-discovery)

- hwmon through sysfs. The identity of a sensor or Channel is the hwmon `name` plus the resolved path of the hwmon `device` link, never `hwmonN`, because the index changes between boots. The sysfs root is a parameter, so tests run on a fake tree.
- The driver of a hwmon device is the name of its `device/driver` link. Exclusion lists use this name, not the hwmon `name`.
- Channels come only from hwmon devices whose driver is not a GPU driver (`amdgpu`, `radeon`, `nouveau`, `i915`, `xe`) and not `aquacomputer_d5next`. Their sensors stay Inputs. ADR-0004 keeps the GPU fan and write support for other Aqua Computer devices out of scope, and generic discovery would otherwise make their `pwmN` outputs Channels.
- The kernel hwmon device `leakshield` is excluded from discovery. The daemon decodes report 1 itself, so the Leakshield has one source.
- Leakshield HID through `/dev/hidraw*`: the daemon reads report 1 with `read()` of the input reports that the device sends (no `HIDIOCGINPUT`), and it uses the ioctls for get and set feature. The kernel driver also reads report 1. Task 5.2 checks on the reference device that both readers work together.
- Leakshield bulk endpoint through `nusb` on interface 0, which no kernel driver claims.
- A `nusb` hot-plug event reattaches the Leakshield, matched by USB path. The hidraw node comes from sysfs.

Rejected: `libusb` and `libudev`, because `nusb` is pure Rust and needs no C library.

### D4. Board files, Derived sensors, GPU Inputs and Loops (hardware-discovery)

- A Board file is TOML data in the RPM, in `/usr/share/nicecooling/boards/`. The directory is a parameter in tests. The daemon selects the file by the DMI values `board_vendor` and `board_name`. An unknown Board uses the kernel names. A user name replaces a Board file name.
- Derived sensors are a maximum, a mean and a difference of other sensors. A Curve accepts a Derived sensor as Input. The difference type has no user in v1. It costs little, and an air probe can use it later.
- GPU sensors from the hwmon driver are Inputs. The daemon never writes the GPU fan (D3).
- A Loop groups its Pump channels, its flow sensor with a Calibration table, its water temperature sensor and, optionally, a Leakshield.

### D5. Flow calibration (hardware-discovery)

The daemon interpolates linearly between Calibration table points. Below the first point, the line goes from 0 rpm = 0 L/h to the first point. Above the last point, the last segment continues and the value carries an out-of-range flag. The preset `data/calibration/alphacool_17558.toml` holds the 27 datasheet points.

### D6. Failure model (fan-control)

One task per source reads its device and publishes timestamped values to a value table. The sources are each hwmon chip, the Leakshield HID interface and the Leakshield bulk interface.

- The safety tick reads only the value table. It never waits on device I/O.
- A value older than 3 s counts as no value. The Lost Input rule then covers a stuck device.
- Leakshield setting writes run in a separate worker with a queue of one.
- The safety stages are stages 1 to 10 of D7. The tick pings the watchdog after them.
- Unit values: `Type=notify`, `WatchdogSec=10s`, `TimeoutAbortSec=5s`, `Restart=on-failure`, `RestartSec=2s`, `StateDirectory=nicecooling`, `StateDirectoryMode=0700`. In `[Unit]`: `StartLimitIntervalSec=60s` and `StartLimitBurst=5`.
- A watchdog timeout sends SIGABRT. A frozen daemon does not react, so systemd sends SIGKILL after 5 s. At most 15 s pass from the last ping to the kill.
- `ExecStopPost=/usr/libexec/nicecoolingd --restore-bios` restores the BIOS snapshot.
- When the restart limit is reached, the app shows "daemon down, Board on BIOS curves". It detects the limit as `docs/api.md` describes.

Rejected: one sequential tick that does all device I/O. A blocked transfer would stall the Pump floor, the pump stop and the Feed message.

### D7. Control tick (fan-control)

Every second the daemon runs these stages in order:

1. Read the value table.
2. Compute Derived sensors and calibrated flows.
3. Evaluate the Alarms.
4. Evaluate the Rule groups.
5. Compute each Curve from its smoothed Input.
6. Apply the Duty step limit.
7. Apply the Pump floor.
8. Apply the overrides: Lost Input and pump stop. They set 100% and bypass the step limit.
9. Reconcile and write each Channel.
10. Send the Feed message of each Loop.
11. Ping the watchdog.

Rules for the stages:

- Curves interpolate linearly between their points.
- A Curve reads its Input through a moving average. Its Duty changes by at most a fixed step per second. The defaults are 5 s and 5% per second. Both are settings of the Curve.
- When an override ends, the step limit ramps the Duty down from 100%.
- The Pump floor clamps the Duty of a Pump channel, whether it comes from a Curve or from a fixed Duty.
- Lost Input: when the Input of a Curve has no value, its Channel goes to 100% and nicecooling raises a Warning, until the Input returns.
- Pump stop is raised when any Pump channel of a Loop reads 0 rpm for 5 s while its Duty is above zero. For a Channel on its Hardware curve, the Duty is the current `pwmN` value.
- Pump stop takes every Channel to 100%, Hardware-curve Channels included.
- Pump stop closes when every Pump channel of the Loop reads more than 0 rpm for 10 s. Channels without a Curve or a fixed Duty then get their BIOS snapshot attributes back.
- Duty to raw value: `round(duty * 255 / 100)`, half away from zero, clamped to 0-255.
- Reconcile: the daemon reads back `pwmN` and `pwmN_enable`. It counts a mismatch when `pwmN` differs from the desired raw value or the mode is not manual. A Channel in manual mode at raw 255 reads `pwmN_enable` 0, so enable 0 with raw 255 counts as manual at 255. The daemon writes the Channel on a mismatch. Three mismatches in 60 s raise an "another writer" Warning. After a resume from suspend, the same rule corrects a Channel that the Board reset.
- `nct6775` caches readings for about 1.5 s. The reconcile test uses a fake tree that returns the old value for 1.5 s.
- A Channel without a Curve or a fixed Duty stays on its Hardware curve. The daemon does not take control of it.

### D8. BIOS snapshot and takeover lease (fan-control, ADR-0003)

The BIOS snapshot holds every writable `pwmN*` attribute of every Channel: 22 per Channel on the reference `nct6798`, and 28 for `pwm2` (six `pwm2_weight_*` more). Channels of GPU drivers are not part of it (D3). It lives in `/var/lib/nicecooling/` (`docs/storage.md`). Manual mode means `pwmN_enable` = 1.

- The first daemon start takes the snapshot, before it changes any Channel.
- The daemon writes a takeover lease file before the first manual write.
- The daemon removes the lease only after a restore whose read-back matches the snapshot.
- The daemon refreshes the snapshot only when no lease exists.
- A restore writes the attributes in the order of the snapshot file: all others first, then `pwmN`, then `pwmN_enable`.
- On first start with a Channel already in manual mode, the daemon takes the snapshot anyway. It marks that Channel "manual at snapshot" and raises a Warning.
- `--restore-bios` with no lease does nothing and exits 0.
- `--restore-bios` with a lease and no readable snapshot or copy sets every controlled Channel to manual mode at raw 255 and raises the `state-fallback` Warning.

Rejected: a refresh rule that tests "no Channel in manual mode". A partial restore can leave a manual Channel and poison the snapshot.

### D9. Protocol from aquasuite's code (leakshield-settings, ADR-0001)

A reverse engineering pipeline outside the product code reads the aquasuite service. Its notes and its tools are not in the repository. Artifacts go to `re/`, which git ignores.

- The aquasuite GUI, its service and its installer never run.
- The repository receives only our description of the protocol in `docs/protocol/`, binary dumps of the reference device in `tests/fixtures/` and the RE notes in `docs/re/`. No decompiled code enters the repository. RE notes may cite aquasuite names and paths as references, never code.
- Naming rule: `docs/protocol/`, `docs/api.md`, `docs/storage.md` and `crates/` use our own snake_case names. Identifiers of aquasuite code do not appear. A user-visible aquasuite label may describe a field.
- `tools/re/check-identifiers.py` enforces the rule. It checks the text files of `docs/protocol/`, `docs/api.md`, `docs/storage.md` and `crates/`. A token is a maximal run of `[A-Za-z0-9_]` with 6 or more characters that does not start with a digit and is a compound name (an inner underscore, a lowercase letter before an uppercase letter, or a letter before a digit). Plain words are not checked. The script compares each token, case-sensitive, by its SHA-256 with the hashes of the identifiers of the decompiled code in `tools/re/identifier-hashes.txt`, minus `tools/re/identifier-allowlist.txt`. CI runs it.

Rejected: USB captures of aquasuite in a Windows VM, because they make Windows part of every protocol change.

### D10. Field registry (leakshield-settings, ADR-0001)

The protocol crate holds one registry entry per field in `registry/leakshield.toml`. An entry has the report, offset, type, scale, range, aquasuite default, risk class (Safety field or low-risk) and verification state. The registry also holds the volatile mask: the bytes that the device changes by itself. The registry is keyed by device model and firmware version. The Safety classification lives in the registry only. `docs/protocol/leakshield.md` links to it.

- Conventions (`docs/protocol/leakshield.md`): offsets count from the report ID byte at offset 0, as in the fixtures. Multi-byte fields are big-endian. The report 3 CRC is CRC-16/USB over the 747 struct bytes (offsets 1 to 747), stored big-endian at offsets 748-749.
- The serial is in report 1 bytes 3-6, as two big-endian u16. The firmware version is in bytes 13-14.
- `verification_state` has four values: `code-defined` (the aquasuite code defines the field, no check yet), `plausible-read` (the value in the fw1022 fixtures is in range), `verified-write` (a Verification write passed) and `failed` (a Verification write failed). A low-risk field is writable at `plausible-read` or `verified-write`. A Safety field is writable only at `verified-write`. A field in any other state is read-only.
- The Safety fields `signal_output_config` and `alarm_start_delay` have no safe test value, because the aquasuite code does not define their values. They get no Verification write and stay read-only (`docs/hardware-checks/verification-writes.md`).
- Verification is a development-time procedure on the reference Leakshield: plausible reads on the fixtures and supervised Verification writes in the manual hardware checks. `tools/nicecooling-verify` runs one Verification write (task 5.12).
- The result is static registry data. The app has no runtime ledger and no write control for a Verification write.
- The registry is keyed on firmware 1022. The fw1019 fixtures stay as decode tests, and firmware 1019 is read-only. An unknown firmware version is read-only. The UI says why.
- Neither aquasuite version accepts firmware 1019. The reference Leakshield runs firmware 1022, the version that aquasuite X.96 accepts. Its report 3 is byte-identical to the fw1019 dump. The fw1019 fixtures stay as decode tests.

Rejected: reuse of a known layout for unknown firmware when sizes and values match. It is speculative, and a wrong layout writes a Safety field.

### D11. Write path for Leakshield settings (leakshield-settings)

A write follows these steps:

1. Save a backup of reports 3, 5 and 8.
2. Read report 3 again just before the write. Abort if it changed outside the volatile mask.
3. Change only the target fields and recompute the CRC-16/USB. The CRC bytes are expected to change.
4. Write report 3, then read it back.
5. Compare outside the volatile mask. An unexpected change raises an error and offers the restore.
6. Send one output report 2 save-settings command (code name `SaveSettings`, bytes `02 00000002 00000000 34C6`) 4 s after the last report 3 write. The write worker owns the 4 s timer. More writes inside 4 s give one save-settings command. A failed save-settings command emits `WriteFailed`.

aquasuite X.75 and X.96 send the save-settings command this way (`docs/protocol/leakshield.md`). The secondary report 2 that the kernel driver sends after each control write is not used.

Backups keep reports 3, 5 and 8, but a restore writes only report 3, until reports 5 and 8 are decoded. Retention: the first backup per serial plus the last 20, deduplicated by hash.

External change detection: the daemon reads report 3 every 10 s and compares it with the last known value outside the volatile mask. A difference is an external change (ADR-0006). After its own verified write, the daemon updates the last known value. The daemon keeps the last known report 3 on disk, so detection survives a restart.

A Profile activation does not use steps 2 and 5. It writes the stored report 3 as stored and does not read it back (D14). Steps 1 and 6 apply to it.

Rejected: a user write without a second read, because another tool can change report 3 between the first read and the write.

### D12. Mode, Action, Feed message and clock (leakshield-operation)

- The daemon sends the Feed message of each Loop that has a Leakshield on every tick. The Leakshield computes the loop pressure from it.
- The out-of-tree driver is the reference for the Feed message codec and its golden tests. nicecooling does not depend on it. The driver covers only the Feed message, and it sends a second Feed message when it runs next to the daemon.
- Mode and Action use one transport: report 2 command 12 (`set_mode`) with the Mode or Action code as parameter (`docs/protocol/leakshield.md`).
- The Feed message has 16 slots: slot 0 is the pump speed, slot 1 is the flow, slots 2 and 3 are spare, and slots 4 to 15 are the 12 Software sensors.
- The aquasuite code does not define what the Leakshield does when the Feed stops. Task 5.11 measures it.
- The daemon refuses a Refill while a Pump channel of the same Loop reads above 0 rpm (ADR-0007).
- `CancelAction` sends `cancel_action` (command 12, code 0). It fails with `NotSupported` only when the registry of the device firmware lacks the command.
- The device has no command that clears the pump runtime. "Reset pump history" of aquasuite clears only a log on the PC, so nicecooling has no such function. `ResetRefillHistory` sends `reset_refill_history` (command 13). `ResetServiceCounter` sends `reset_service_counter` (command 25000, parameter: serial XOR 0xFFFFFFFF). Both are `write` class and fail with `NotSupported` when the registry of the firmware lacks the command. The code does not say which report 1 values they clear.
- The Leakshield stamps its event list and its time-controlled vacuum generation with its clock, and it has no battery for it. The user can set the device clock from the PC clock at any offset (`SyncClock`). The daemon sets the device clock by itself only when the device time differs from the PC time by more than 60 s. This is its only Leakshield write without a user action or a Profile activation.
- A manual check reads `fan2_input` and `fan3_input` of the kernel hwmon device `leakshield`. They show that the device accepted the Feed.

### D13. Alarms (alarms, ADR-0007)

Sources: Leakshield Warnings and Alarms decoded from report 1, and nicecooling events. The nicecooling Alarms are pump stop (D7) and "Leakshield unreachable". The Lost Input Warning and the "another writer" Warning are nicecooling Warnings. `docs/api.md` lists the event identifiers.

- Leakshield unreachable: report 1 is missing, or the Feed message fails, for 10 s. It closes when report 1 arrives and the Feed message succeeds for 10 s in a row. Default alarm response: notify.
- One table sets the alarm response of every Warning and Alarm: none, notify, or poweroff after a delay. Defaults: large leak and pump stop power off after 30 s. Everything else notifies.
- The daemon emits `AlarmRaised`. At expiry it calls logind `PowerOffWithFlags` with `SD_LOGIND_SKIP_INHIBITORS` (0x10). On a call error it emits `PoweroffFailed` and retries every 5 s. A call that does not answer in 10 s is an error. `CancelPoweroff` fails with `PoweroffStarted` once the call is under way, because logind may already queue the poweroff.
- A daemon restart during a countdown restarts the countdown if the Alarm is still active.
- The RPM ships a drop-in for `poweroff.target`: `JobTimeoutSec=55s` and `JobTimeoutAction=poweroff-force`. Delay inhibitors still wait up to `InhibitDelayMaxSec` (default 5 s) before logind queues the job. With the default value, the forced poweroff starts 60 s after the end of the alarm delay at the latest. The 60 s are a goal and not a hard bound: `poweroff-force` ends the job but not the shutdown, because `systemd-shutdown` still sends SIGTERM and SIGKILL to the remaining processes and waits for them. The owner keeps `poweroff-force` after 55 s and accepts that a stuck process can take longer.
- Cancel path: the app shows the Alarm notification with critical urgency and checks the `actions` capability of the notification server. "Cancel poweroff" is also in the tray menu and in the window. The app of any session can cancel.
- The daemon proposes these device settings: the key-sequence Device response off on every alarm, and the signal output only as a long press on the system Alarm. `GetDeviceResponseProposals` returns them. It warns when a setting overlaps with its alarm responses: `key-sequence-on`, and `double-poweroff` when the signal output is set on a Leakshield Alarm whose alarm response is poweroff. The daemon does not write the proposals on its own (ADR-0007).

Rejected: a forced poweroff from the daemon after a time bound. systemd stops the daemon early in a shutdown, so the bound would not run.

Rejected: the plain logind `PowerOff` call. Since systemd 257, logind obeys block inhibitors for root, so the poweroff can fail.

### D14. Profiles, Leakshield slots and Rule groups (profiles, ADR-0006, ADR-0007)

- A Default Profile always exists and cannot be deleted. It is active when no Rule group wins and no manual activation exists. A Curve change made then goes into it. When the winning group stops holding, its Profile stays until the Profile dwell ends, and then the Default Profile becomes active.
- Rule groups follow the aquasuite model (`docs/protocol/leakshield.md`, section Device profiles in aquasuite). The groups are an ordered list. A group holds when it is enabled and all its rules hold. The first group that holds activates its Profile. Without a holding group, the Default Profile is active. Each group has its own Rule delay timer. The winner is the first group in list order that has held for the Rule delay.
- A group activates its Profile only when it becomes the winner. It does not write again on later ticks.
- Rule kinds (`docs/api.md`, section Rule groups):
  - A sensor rule compares a sensor value with `lt`, `le`, `eq`, `ge`, `gt`, `ne` or `in_range`, and the comparison must hold for a hold time. A sensor without a value makes the rule false.
  - A process rule lists names and matches `any`, `all` or `none`. A name equals, without case, `comm` or the basename of `/proc/<pid>/exe`. The daemon scans `/proc` every second.
  - A time rule has ISO weekdays and a range in minutes after midnight, in system local time. A range that ends before it starts crosses midnight and belongs to the day of its start.
  - An interval rule has an interval, a start delay and a duration. It holds from the start delay to the start delay plus the duration, in each period.
  - There is no hotkey rule: Wayland has no standard global shortcut. aquasuite also switches at service start. nicecooling does not: the daemon start writes nothing to the Leakshield.
- The Rule delay and the Profile dwell are settings of the daemon. They apply to every group. `SetRuleTiming` changes them. The defaults are 30 s and 5 min (300 s). Immediate switching removes both. A window of an interval rule that is shorter than the Rule delay never wins, unless Immediate switching is on.
- A manual activation wins over every Rule group at once, also during the Profile dwell. It stays until the user returns to automatic (`ReturnToAutomatic`).
- A Curve change made in nicecooling goes into the active Profile at once, with no confirm step. A Leakshield setting change goes into the slot of the active Profile, so into every Profile with that slot.
- The Leakshield settings live per Leakshield and per slot, as aquasuite stores them: the whole report 3 of the device and the binary assets, in `leakshield/<leakshield id>/slot-<n>/` (`docs/storage.md`). A Profile has no Leakshield data of its own.
- Each Profile references one Leakshield slot (1 to 4), as the four fixed slots of aquasuite (`docs/protocol/leakshield.md`, section Device profile numbers). The number of Profiles is not limited. The Default Profile has slot 1 and its slot cannot change. The first start creates three more Profiles with the slots 2, 3 and 4. A new Profile takes the slot of the Profile it copies (of the active Profile for a new Profile from the current state). The user can change the slot of a Profile (`SetSlot`). The number is in `profile.json` as `slot` and in the `device_profile` byte of the stored report 3 of the slot. The daemon sets the byte and the CRC when it stores a report in a slot. When the daemon first reads a Leakshield, it copies the report 3 of the device into the four slots, each with its own number.
- A switch writes the Curves, then, for each Leakshield, the stored whole report 3 of the slot as stored. It writes only when the slot number of the Profile differs from report 1 `active_profile` of the device. A switch between two Profiles with the same slot writes only the Curves. It does not compare bytes and does not read the report back. The save-settings command follows 4 s after the last report 3 write (the worker timer of D11). The backup step of D11 applies before the write. `SetSlot` on the active Profile runs the same switch for the new slot.
- The write goes out also while an Action runs and while the Mode is RELEASE. The owner accepted the risk that a Rule group writes SHIELD on an open loop (ADR-0007).
- Spacing, as in aquasuite: a request writes at once when no write waits and the last profile write is older than 2 s. Otherwise the worker keeps only the newest request and writes it 1.5 s after the last profile write. The Rule delay and the Profile dwell still decide when a Rule group activates a Profile.
- The daemon writes no Leakshield part when the firmware of the device is unknown, when it differs from the firmware of the stored report of the slot, or when the serial differs from the serial of that report. It reports `leakshield-failed`.
- A switch can thus write bytes of fields that are not Verified fields. The device produced these bytes on the same firmware (owner decision).
- After a Profile write, the daemon stores the written report as the last known report 3 (D11). After a verified write that the user started in nicecooling, the daemon stores the read-back report 3 in the slot of the active Profile.
- When the write of the Leakshield part fails, the Profile shows "Curves applied, Leakshield failed" and offers the restore.

Rejected: Profile rules on suspend and resume. They need a logind inhibitor and a bypass of the delay and the dwell. They also need a full Leakshield write in a window of about 5 s, and they have no effect on Curves. The reconcile rule of D7 covers resume.

### D15. D-Bus API and polkit (control-api, ADR-0002)

Bus name `io.github.cappyt.NiceCooling1` on the system bus. The interfaces, objects, methods, signals, errors and identifiers are in `crates/nicecooling-api/dbus/io.github.cappyt.NiceCooling1.xml` and `docs/api.md`. Properties hold state. One `SensorsUpdated` signal per tick. Methods perform writes. Each method carries an authorization annotation in the XML.

There is no polkit action for read methods. Three polkit actions cover the other methods:

| Polkit action ID | Methods | Default | Rule |
|---|---|---|---|
| `io.github.cappyt.NiceCooling1.write` | changes of Curves, Pump floor, Profiles, alarm responses and Leakshield settings | `auth_admin_keep` | Yes for a `wheel` member in the active local session. |
| `io.github.cappyt.NiceCooling1.cancel-poweroff` | `CancelPoweroff` | `auth_admin_keep` | Yes for a `wheel` member in a local session, active or not. |
| `io.github.cappyt.NiceCooling1.critical` | factory reset, `SetMode` to RELEASE, restore a backup | `auth_admin_keep` | None. |

The polkit rule is JavaScript that tests the `wheel` group. A caller with uid 0 needs no polkit check. The daemon passes `AllowUserInteraction` to polkit only when the method call carries the D-Bus flag `ALLOW_INTERACTIVE_AUTHORIZATION`.

A `write` method can lower an alarm response, the Pump floor or a pump Duty, and it needs no admin password (see Risks).

Rejected: a group-owned Unix socket, which has no per-action authorization.

### D16. Desktop app (desktop-app, ADR-0005)

- The app starts with no window through the autostart entry (`nicecooling --background`) and shows a StatusNotifierItem tray icon with a DBusMenu menu (`ksni`). It shows Alarm notifications through `org.freedesktop.Notifications` (`notify-rust`).
- A spike proves a windowless GPUI process with a StatusNotifierItem and notification actions, and records its idle CPU and RAM.
- History: the app writes samples to `$XDG_DATA_HOME/nicecooling/history.db` in batches every 10 s and prunes hourly. 1 s samples stay for 48 hours. 1 minute aggregates (mean, minimum, maximum) stay for one year. The schema is in `docs/storage.md`.
- App settings: the app keeps its language, tray and notification switches and dismissed hints in `$XDG_CONFIG_HOME/nicecooling/settings.toml`. "Start at login" is off when the app writes `~/.config/autostart/io.github.cappyt.NiceCooling.desktop` with `Hidden=true`, which overrides the system autostart entry (`docs/storage.md`).
- Languages: English and Italian, in the app only. The daemon sends event identifiers, not text. The app owns the text of every notification.
- When the daemon is unreachable, the app shows it in the window and the tray icon, and disables every write control. When the unit `nicecoolingd.service` is `failed` with `Result` `start-limit-hit`, it shows "daemon down, Board on BIOS curves".

Rejected: a history database in `/var/lib` written by the daemon (ADR-0005).

### D17. Testing

- Protocol codecs: golden tests on dumps of the reference device, and on the golden bytes of the out-of-tree driver for the Feed message.
- hwmon layer: tests on a fake sysfs tree. The fixture for the reference Board comes from the real attribute list, with an `amdgpu` node that has `pwm1`.
- Domain core: unit tests for the tick stages, Curves, Pump floor, Rule groups, timers and Alarms with a simulated clock.
- D-Bus: integration tests on a private `dbus-daemon` with a simulated hardware layer. An `Authorizer` trait has a polkit implementation and a test implementation. The test harness builds the daemon library with the test implementation. The release binary has no switch for it.
- Fakes: logind, the notification server and the StatusNotifierWatcher run as fakes on the private bus. The AppIndicator extension of GNOME ships the interface XML files for them.
- A simulated Leakshield device (`nicecooling-protocol/src/sim.rs`, behind the Cargo feature `sim` that only dev-dependencies enable) serves the write path and the hot-plug tests.
- No test writes to real hardware. Hardware checks are MANUAL tasks: a person runs the check on the reference desktop with a procedure file in `docs/hardware-checks/`. A procedure file lists steps, expected results and one pass or fail line per step. Checks that a container can run are automated.

### D18. Contract items for the screens (control-api, desktop-app)

The screens of section 8 need data that the contract of section 4 did not carry ([issue #32](https://github.com/CappyT/nicecooling/issues/32)). The owner decided that every item goes into version 1. That decision approves the contract rewrites listed below. Task 11.1 writes the semantics into `docs/api.md` and `docs/storage.md`. The task of each item changes the XML, the daemon and the app together, so the workspace builds after each task.

Every new time is in microseconds since the Unix epoch. The daemon reads the wall clock at the transition. A later change of the wall clock does not move a stored time. The timers of the daemon stay monotonic.

| Item | Contract |
|---|---|
| BIOS snapshot of a Channel | `Channel.BiosSnapshot` (`(ta(ss))`, const): the snapshot time and the attributes of the Channel, as in `bios-snapshot.json`, in restore order. `(0, [])`: the daemon holds no snapshot. `(t, [])`: the snapshot has no entry for the Channel. |
| Fan sensor of a Channel | `Channel.FanSensor` (`o`, const): the sensor `<chip>_fan<N>` of `<chip>_pwm<N>`, or `/` when it does not exist. It uses `tach_sensor` of `nicecooling-core/src/loops.rs`, the rule of the pump-stop Alarm. |
| Curve readout | `SensorsUpdated` `channels` becomes `a(sdddy)`: id, Duty, smoothed Input, Curve target, flags. Flag bit 2: both values are valid. Without bit 2 both are 0. |
| Feed | `Leakshield.FeedState` (`s`): `off`, `waiting`, `ok` or `failing`. `Leakshield.LastFeed` (`(tdd)`): the time of the last good send, and the pump speed (rpm) and the flow (L/h) that it carried; `(0, 0, 0)` before the first good send. |
| Pressure target and limits | Leakshield sensors `<id>_pressure_setpoint`, `<id>_pressure_low` and `<id>_pressure_high` from report 1 (offsets 293, 291, 295, 0.1 mbar), kind `pressure`, unit mbar. |
| Active slot | `Leakshield.ActiveSlot` (`y`): report 1 `active_profile`, 0 when not known. |
| Stored slot settings | `Leakshield.GetStoredFields(slot y) -> a{sv}` (class `read`) and `Leakshield.SetStoredFields(slot y, fields a{sv})` (class `write`). |
| Failed Profile write | `Leakshield.FailedWriteBackup` (`s`): the backup id of the last Profile write of this Leakshield that failed. Empty after a good Profile write and after `RestoreBackup`. |
| Profile dwell | `Profiles.DwellUntil` (`t`): the end of the running Profile dwell, 0 when no dwell runs. |
| Alarm history | `Alarms.GetHistory(since_us t, limit u) -> a(sssssttsa{sv})`: instance, event, severity, source, response, raised_us, cleared_us (0 while active), outcome, details. Class `read`. |
| Alarm response per Leakshield | `SetResponse(event s, source s, response s, delay_s u)`, `ResetResponse(event s, source s)` (class `write`), `GetResponses() -> a(sssub)`: event, source, response, delay, override. Source `""` is the default of the event. |

Every new property emits `PropertiesChanged` when its value changes, except the const ones.

#### Curve readout

- The Curve stage keeps the mean of the window and the target in `ChannelTick` (`input: Option<f64>`, `target: Option<f64>`), set again in every tick.
- Bit 2 is set when the Control is `curve`, no override runs, and this tick added a sample to the window.
- The Curve stage clears the window of a Channel on Lost Input and when `SetCurve` changes the Input sensor. Thus the readout never shows a mean of stale samples or of two sensors.

#### Feed

- `off`: no Loop links the Leakshield, the registry does not cover its firmware, the device is disconnected, or a firmware update runs. `waiting`: the Feed is on and no send result exists yet. `ok`: the last send succeeded. `failing`: the last send failed.
- `LastFeed` carries the values of the Feed message from `leakshield/feed.rs`, so the app does not compute them again. It changes with each good send, about once a second.
- A change to `off` resets the failing time of the value table (`feed_failing_since`).
- The `Info` watch of `leakshield/attached.rs` carries both values to the D-Bus object.

#### Stored slot settings

- Both methods run as a job of the write worker of the Leakshield. Thus they are serialized with Profile writes and `SetFields`, and the active slot check runs inside the job.
- The errors, in this order: a slot outside 1 to 4 (`InvalidArgument`); a slot with no stored report 3 (`NotFound`); a stored firmware that the registry does not cover (`NotSupported`); for `SetStoredFields` only: the active slot (`InvalidArgument`, use `SetFields`), the field `device_profile` (`InvalidArgument`), then per field the rules of `SetFields` (`InvalidArgument`, `NotVerified`). The active slot is the last known report 1 `active_profile`, so the check works while the device is disconnected.
- The checks use the firmware and the structure of the slot `meta.json`, not report 1. The methods work while the device is disconnected. They send nothing to the device.
- `SetStoredFields` decodes the stored report 3, changes the fields, encodes it, computes the CRC again, writes `report3.bin` atomically, and sets `edited: true` in `meta.json`.
- Owner decision: the next activation of an edited slot runs the verified write path of D11 (backup, write, read-back), not the stored write of D14. A good read-back clears the mark. A failed read-back keeps the mark and reports `leakshield-failed`. The D14 rule "the device produced these bytes" does not hold for an edited slot.

#### Failed Profile write

- An activation writes every Leakshield and takes one backup per device. Thus the backup id is a property of each Leakshield, not an argument of `ProfileActivated`.
- The app builds the restore offer from it. An app that starts after the failure still finds it.

#### Profile dwell

- `DwellUntil` is the time of the last switch to the active Profile plus the Profile dwell, while the cause is `rule`, Immediate switching is off and that time is in the future. Otherwise it is 0. A daemon restart restarts the dwell, so the time then counts from the daemon start.
- The daemon computes it again on an activation, a change of the cause, `SetRuleTiming`, `SetImmediateSwitching` and `ReturnToAutomatic`, and sets it to 0 when the dwell ends.

#### Alarm history

- The file is `/var/lib/nicecooling/alarm-history.json`: `{"schema_version": 1, "rows": [...]}`. A row holds `instance`, `event`, `severity`, `source`, `response`, `raised_us`, `cleared_us`, `outcome`, `closed_by_restart` and `details`. `details` maps a key to `{"<signature>": value}`, with the signature `d`, `u`, `s` or `o`.
- It follows the common file rules of `storage.md`: atomic write and a `.lkg` copy. A damaged file loads the `.lkg` copy, else an empty history. The daemon logs it. `StateHealth` does not change, because the history does not control anything.
- The history has its own writer. A row change marks it dirty, and the writer saves at most once per second. A change to `poweroff-requested` saves at once, before the call to logind. The daemon saves the history at stop.
- It keeps the last 1000 Alarm rows and the last 1000 Warning rows. Each group drops its oldest row. Thus a Warning that flaps does not push Alarms out.
- Outcomes:
  - At the raise: response `none` gives `none`, `notify` gives `notified`, `poweroff` gives `poweroff-pending`.
  - From `poweroff-pending`: `CancelPoweroff` gives `poweroff-cancelled`, a clear before the call gives `poweroff-stopped`, the call to logind gives `poweroff-requested`.
  - From `poweroff-requested`: `PoweroffFailed` gives `poweroff-failed`.
  - From `poweroff-failed`: the retry gives `poweroff-requested`, a cancel in the retry wait gives `poweroff-cancelled`, a clear gives `poweroff-stopped`.
  - `none`, `notified`, `poweroff-cancelled` and `poweroff-stopped` are final. The app shows `poweroff-requested` as "PC powered off".
- At start, the daemon closes every row of an earlier start that has `cleared_us` 0: `cleared_us` becomes the start time of this daemon and `closed_by_restart` becomes true. `GetHistory` shows it as the detail `closed_by_restart` (`b`, true) of the row, and only on such a row. An instance that is still active rises again as a new instance.
- `GetHistory` returns the rows that were active at or after `since_us` (`cleared_us` 0 or at least `since_us`), newest raise first, at most `limit` rows. `limit` 0 means no limit.

#### Alarm response per Leakshield

- An override exists only for an event `leakshield.*` and a Leakshield id. The other events keep one row each. A Leakshield id comes from the serial, so a new device never takes the id of an old one.
- Resolution when an instance rises: the forced `notify` of a firmware update (change `add-leakshield-firmware-update`), then the override of (event, Leakshield), then the default of the event, then the built-in default.
- `SetResponse` with source `""` sets the default of the event. With a source: an event that is not `leakshield.*` fails with `InvalidArgument`, and a source that is not a known Leakshield id fails with `NotFound`. A known Leakshield is one that the daemon attached since its start, connected or not.
- `ResetResponse(event, "")` sets the default of the event back to the built-in default. `ResetResponse(event, source)` removes the override. It accepts any stored key, also the key of a Leakshield that no longer exists.
- `GetResponses` returns, in the event order of `api.md`: one row per event with source `""` and override false, then, for each `leakshield.*` event, one resolved row per known Leakshield, with override true when an override exists.
- `state.json` stores an override under the key `<event>@<source>`. A state file without overrides loads as before.
- `GetDeviceResponseProposals` uses the resolved response of each Leakshield for its `double-poweroff` overlap.

#### Contract rewrites

- `api.md`, Values and units: the flag bit 2 of `channels`.
- `api.md`, Event identifiers: the response table has rows per Leakshield for the `leakshield.*` events, and an instance takes the resolved response, not the response of its event.
- `storage.md`: the `<event>@<source>` key of `alarm_responses`, the file `alarm-history.json`, and `edited` in the slot `meta.json`.
- XML: the signatures of `SetResponse`, `GetResponses` and `SensorsUpdated`.

#### App-only items

- The diff table before an activation compares `GetStoredFields` of the target slot with `GetFields`, when the target slot differs from `ActiveSlot`.
- "Differs from other slots" compares the alarm thresholds of the slot with `GetStoredFields` of every other slot that has stored settings. "Apply to all slots" calls `SetStoredFields` for each other slot and shows one result row per slot that failed.
- The overlap icon of the Alarms screen uses the `overlap` of `GetDeviceResponseProposals`.
- Overview shows the pressure target and limits from the three new sensors. The active Profile card shows its Rule group from `GetRuleGroups`.
- First run with "Empty table" does not call `SetFlowSensor`, and a note says to build the table on the Loops screen.
- The snapshot table: the app reads `pwm<N>_auto_point<K>_temp` (millidegree Celsius) and `pwm<N>_auto_point<K>_pwm` (0-255) as rows of temperature and Duty. Without these attributes it shows the text only. The daemon restores the attributes as raw text and has no rule for them, so this is a view, not a copy of a daemon rule.
- The owner approved the new texts (history outcomes, "Not sent yet", the First run note, the slot error). `docs/ui/screens/` holds them.

Rejected:

- A diff method in the daemon. The app has both inputs, and the diff is a view.
- The backup id as an argument of `ProfileActivated`. One activation writes every Leakshield, and an app that starts later misses the signal.
- Overrides for Loop and Channel sources. Their ids come back after a delete (`nicecooling-api/src/ids.rs`, `next_number`), so an old override would apply to a new Loop. The Alarms screen groups the responses per Leakshield only.
- A decoded snapshot property. The daemon would get a driver rule that it does not use.

## Risks / Trade-offs

- [The rebuild of the fan-control stack costs work] -> ADR-0008 records the reason. Code may come from CoolerControl. `THIRD_PARTY.md` records each taken file. The GPL-3.0 license must be compatible before any distribution.
- [The legal basis of decompiling aquasuite] -> The purpose is interoperability with a device that has no documentation: Directive 2009/24/EC, Article 6. No decompiled code enters the repository, and the identifier check covers the protocol documents and the crates.
- [A firmware update moves fields] -> The registry is keyed by firmware version. Unknown versions are read-only.
- [aquasuite in the VM and the daemon both write the Leakshield] -> A Leakshield passed to the VM is unreachable for the daemon, and the daemon raises "Leakshield unreachable". After the device returns, external change detection reports a change made in the VM.
- [The out-of-tree driver and the daemon both send the Feed message] -> Document the conflict. Do not load the out-of-tree driver next to the daemon.
- [The kernel driver and the daemon both read report 1] -> Task 5.2 checks that both readers work together.
- [The Feed message stops] -> The "Leakshield unreachable" Alarm covers a failed Feed message. The device behavior without the Feed is an Open Question.
- [The Leakshield disconnects during a write] -> The write path reads back and offers the restore. The reattach rule (D3) finds the device again.
- [The BIOS snapshot is missing or corrupt] -> Atomic writes and a last-known-good copy protect the file (D2). `--restore-bios` has a fallback (D8).
- [A `wheel` member in the active session writes without an admin password, even to lower an alarm response, the Pump floor or a pump Duty] -> Accepted by the owner.
- [Any local user can read sensors and process rule names] -> Accepted. The data has low impact.
- [A root daemon parses USB input] -> The codecs reject reports of the wrong size. Golden tests on the fixtures cover them.
- [The unknown write endurance of the Leakshield settings] -> A Profile switch writes only the fields that differ.
- [A poweroff with skipped block inhibitors can interrupt an RPM transaction] -> Accepted. The alarm poweroff protects the hardware, and the owner decided to skip the inhibitors.
- [History storage: about 60 sensors at 1 Hz] -> The app writes in batches every 10 s and prunes hourly. Task 8.4 measures the size.
- [GPUI is pre-1.0 and changes often] -> Pin `gpui-pre` to `=0.3.12` and `gpui-component` to 0.10.0.
- [The kernel hangs and Channels stay at the last Duty] -> Accepted (ADR-0003). The optional hardware watchdog (`RuntimeWatchdogSec` in `/etc/systemd/system.conf`) is a host setting, listed in the README.
- [A false Alarm powers off the PC] -> Delay with cancel. The alarm response is configurable per Alarm.
- [Saving a Profile with RELEASE needs no admin password] -> Accepted by the owner (ADR-0007).

## Migration Plan

New project, nothing to migrate. Before the first Leakshield write, the owner updates the Leakshield to firmware 1022 with aquasuite X.96 (task 2.7). Install the RPM. The first daemon start takes the BIOS snapshot.

Rollback:

1. Stop the service. This restores the BIOS snapshot.
2. If wanted, restore the first Leakshield backup of the device.
3. Remove the RPM. The RPM owns every file it installs, the polkit rule file included, so removal deletes them.

Files that remain after removal: `/var/lib/nicecooling/`, and for each user the history in `~/.local/share/nicecooling/`, the app settings in `~/.config/nicecooling/` and the "Start at login" override `~/.config/autostart/io.github.cappyt.NiceCooling.desktop`.

## Open Questions

- The report 1 values that `reset_refill_history` and `reset_service_counter` clear. The aquasuite code does not say.
- The behavior of the Leakshield when the Feed stops. The aquasuite code does not define it. Task 5.11 measures it.
- The license of nicecooling and its compatibility with CoolerControl (GPL-3.0), before any distribution.
