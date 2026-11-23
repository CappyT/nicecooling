# Storage contract

This file defines every file that nicecooling writes or ships. Design decision D2 of the OpenSpec change `add-nicecooling-v1` gives the rules for atomic writes and the last-known-good copy.

## Common rules

- Every JSON or TOML file has a top-level `schema_version` integer. This document defines version 1.
- A daemon that finds a higher `schema_version` than it knows does not write the file. It loads the last-known-good copy and raises the `state-fallback` Warning.
- Atomic write: write `<file>.tmp` in the same directory, `fsync` the file, rename it over `<file>`, `fsync` the directory.
- Last-known-good copy: `<file>.lkg`. After a load that succeeds, the daemon writes the loaded content to `<file>.lkg` with the same atomic write, when the content differs.
- Times are microseconds since the Unix epoch, UTC. Identifiers follow `api.md`.

## Daemon state: `/var/lib/nicecooling/`

systemd creates the directory with `StateDirectory=nicecooling` and `StateDirectoryMode=0700`. The RPM owns the directory but no file in it.

| Path | Format | Content |
|---|---|---|
| `state.json` | JSON | Global state, see below. |
| `profiles/<profile id>/profile.json` | JSON | One Profile, see below. |
| `leakshield/<leakshield id>/slot-<n>/report3.bin` | binary | The Leakshield settings of slot n (1 to 4): the whole report 3 with its report ID byte, as the device produced it, except the `device_profile` byte (the number n) and the CRC that follows it. Every Profile with slot n uses it. A Profile activation writes it when n differs from report 1 `active_profile`. |
| `leakshield/<leakshield id>/slot-<n>/meta.json` | JSON | `{"schema_version":1, "firmware", "structure"}` of the stored `report3.bin`. |
| `leakshield/<leakshield id>/slot-<n>/assets/<asset id>.bin` | binary | Display assets of slot n. The asset write path is open until the reverse engineering answers it. |
| `bios-snapshot.json` | JSON | The BIOS snapshot, see below. |
| `lease.json` | JSON | The takeover lease: `{"schema_version":1,"created_us":<t>,"boot_id":"<from /proc/sys/kernel/random/boot_id>"}`. The file exists from before the first manual write until a restore whose read-back matches. |
| `leakshield/<leakshield id>/last-known-report3.bin` | binary | Report 3 with its report ID byte, as the daemon last read or wrote it. External change detection compares against it, also after a daemon restart. |
| `backups/<leakshield id>/<UTC yyyymmddThhmmssZ>-<sha256 first 12 hex>/` | directory | One backup: `report3.bin`, `report5.bin`, `report8.bin` (each with its report ID byte) and `meta.json`. |

Every JSON file above has a `.lkg` copy, except `lease.json` and the backup `meta.json`.

The daemon creates the files of the four slots of a Leakshield when it first reads the Leakshield. Each slot gets a copy of the report 3 of the device with its own number and a valid CRC. A Profile delete never deletes a slot file.

### `state.json`

| Key | Type | Content |
|---|---|---|
| `schema_version` | integer | 1 |
| `identities` | object | Map from sensor or Channel id to `{"hwmon_name", "device_path", "attribute"}`. The daemon writes an entry when it first sees the sensor or Channel and never removes it. Discovery builds the id prefix of a chip from its entries, so a chip with the same hwmon name that appears later gets a new prefix and the known ids stay. |
| `names` | object | Map from object id to the user name. |
| `derived_sensors` | array | `{"id", "name", "type", "sources": [sensor id]}`. |
| `calibration_tables` | array | User tables: `{"id", "name", "points": [[rpm, lph]]}`. Presets are not stored here. |
| `loops` | array | `{"id", "name", "pump_channels": [id], "flow_sensor", "calibration_table", "water_temperature", "leakshield", "pump_floor"}`. An absent link is `null`. |
| `alarm_responses` | object | Map from event id to `{"response", "delay_s"}`. An event without an entry uses its default. |
| `rule_groups` | array | Ordered, the first group has the highest priority: `{"id", "profile", "enabled", "rules": [{"kind", "params"}]}` with the kinds and keys of `api.md`. |
| `software_sensors` | object | Map from Leakshield id to a map from slot number ("1" to "12") to the sensor id in that slot. An empty slot has no entry. |
| `profiles` | object | `{"default": "profile_default", "active": <profile id>, "cause": "default"|"rule"|"manual", "immediate_switching": false, "rule_delay_s": 30, "dwell_s": 300}`. |

### `profiles/<id>/profile.json`

The first start creates the Default Profile `profile_default` with slot 1 and three more Profiles with the slots 2, 3 and 4.

| Key | Type | Content |
|---|---|---|
| `schema_version` | integer | 1 |
| `id`, `name` | string | |
| `channels` | object | Map from Channel id to `{"control": "hardware"|"curve"|"fixed", "input", "points": [[input, duty]], "smoothing_s", "step_limit_pct_per_s", "fixed_duty"}`. |
| `pump_floors` | object | Map from Loop id to the Pump floor in percent. |
| `slot` | integer | The Leakshield slot of the Profile, 1 to 4. The Default Profile has 1. Several Profiles can have the same slot. |

`GetContent` returns the same keys as an `a{sv}`, plus `leakshield_fields`: for each Leakshield id, the decoded fields of the stored report 3 of the slot of the Profile.

### `bios-snapshot.json`

| Key | Type | Content |
|---|---|---|
| `schema_version` | integer | 1 |
| `taken_us` | integer | |
| `board` | object | `{"vendor", "model"}` from DMI. |
| `chips` | array | `{"hwmon_name", "device_path", "channels": [{"channel", "manual_at_snapshot", "attributes": [[name, value]]}]}`. |

`attributes` lists every `pwm<N>` and `pwm<N>_*` attribute of the Channel that root can write, as sysfs text, in restore order: all others first, then `pwm<N>`, then `pwm<N>_enable`. A snapshot fails when one of these attributes cannot be read, or when `pwm<N>` or `pwm<N>_enable` is not writable. The restore read-back accepts the value that the `nct6775` driver stores for `pwm<N>_stop_time`, `pwm<N>_step_up_time` and `pwm<N>_step_down_time`: 1 to 255 steps of 400 ms (`pwm<N>_mode` 1) or 100 ms (mode 0).

### Backup `meta.json`

`{"schema_version":1, "created_us", "leakshield", "serial", "firmware", "reason": "first"|"before-write"|"manual"|"import", "sha256": <hex of report3.bin || report5.bin || report8.bin>}`. The first backup that comes from the device has the reason `first`, whatever the caller asked for. An import never has it. Retention keeps the `first` backup of a serial, the 20 newest and the backup that was just saved; a backup with the same `sha256` as an existing one is not written.

## Shipped data: `/usr/share/nicecooling/`

| Path | Repository source | Format |
|---|---|---|
| `boards/<board file id>.toml` | `data/boards/` | Board file, see below. |
| `calibration/<table id>.toml` | `data/calibration/` | Calibration table preset, see below. |

The Leakshield field registry is not a shipped file: the protocol crate compiles it in from `crates/nicecooling-protocol/registry/leakshield.toml`.

### Board file

```toml
schema_version = 1
id = "asus_rog_crosshair_viii_dark_hero"

[match]
board_vendor = "ASUSTeK COMPUTER INC."
board_name = "ROG CROSSHAIR VIII DARK HERO"

[[channel]]
hwmon_name = "nct6798"
pwm = 6
name = "Waterpump"
pump = true          # proposed as Pump channel in the first-run screen

[[sensor]]
hwmon_name = "asusec"
attribute = "fan1"   # example; the reference file lists the measured attributes
name = "Water flow"
```

The daemon selects the Board file whose `[match]` equals `/sys/class/dmi/id/board_vendor` and `board_name`.

### Calibration table preset

```toml
schema_version = 1
id = "alphacool_17558"
name = "Alphacool 17558 flow sensor"
points = [[327.0, 40.0], [369.0, 50.0]]   # [raw rpm, flow L/h], all datasheet points
```

## App settings: `$XDG_CONFIG_HOME/nicecooling/settings.toml`

The app is the only writer, and the Settings screen is its only editor. The app writes the file with the atomic write of the common rules. The daemon never reads it. A missing or unreadable file gives the defaults below.

```toml
schema_version = 1
language = "system"                 # "system", "en" or "it"
tray = true
[notifications]
warnings = true
profile_switched_by_rule = false
external_change = true
[hints]
dismissed = []                      # ids of hints and dialogs closed with "Do not show this again"
[sensors]
hidden = []                         # sensor ids that the Sensors screen hides; the daemon has no hidden flag
```

Known hint ids: `appindicator` (the tray hint) and `immediate_switching_warning` (the dialog of the Profiles screen).

### Start at login

The package installs the system autostart entry `/etc/xdg/autostart/io.github.cappyt.NiceCooling.desktop`, which runs `nicecooling --background`. The setting "Start at login" is not in `settings.toml`. It is the user override file `~/.config/autostart/io.github.cappyt.NiceCooling.desktop`:

- Off: the app writes the override with `Hidden=true`. It has `[Desktop Entry]`, `Type=Application`, `Name=NiceCooling` and `Hidden=true`. Its name equals the system entry, so the session ignores the system entry.
- On: the app removes the override file.
- The app shows "on" when no override with `Hidden=true` exists.

## App history: `$XDG_DATA_HOME/nicecooling/history.db`

SQLite in WAL mode. The app is the only writer.

```sql
CREATE TABLE meta    (key TEXT PRIMARY KEY, value TEXT NOT NULL);          -- schema_version = 1
CREATE TABLE series  (id INTEGER PRIMARY KEY, sensor TEXT NOT NULL UNIQUE, unit TEXT NOT NULL);
CREATE TABLE samples (series INTEGER NOT NULL, t INTEGER NOT NULL, value REAL,   -- t: Unix s; NULL: no value
                      PRIMARY KEY (series, t)) WITHOUT ROWID;
CREATE TABLE minutes (series INTEGER NOT NULL, t INTEGER NOT NULL,              -- t: start of the minute, Unix s
                      mean REAL, min REAL, max REAL, n INTEGER NOT NULL,
                      PRIMARY KEY (series, t)) WITHOUT ROWID;
```

- The app inserts one transaction every 10 s. It writes the `minutes` row of each complete minute in the same transaction.
- The hourly prune deletes `samples` older than 48 hours and `minutes` older than 365 days.
- A gap is a time with no row. The views draw it as a break.
- Estimate for 60 sensors: about 10 million `samples` rows, between 200 and 300 MB. Task 8.4 measures the real size.
