# Storage contract

This file defines every file that nicecooling writes or ships. Design decision D2 of the OpenSpec change `add-nicecooling-v1` gives the rules for atomic writes and the last-known-good copy.

## Common rules

- Every JSON or TOML file has a top-level `schema_version` integer. This document defines version 1.
- A daemon that finds a higher `schema_version` than it knows does not write the file. It loads the last-known-good copy and raises the `state-fallback` Warning.
- Atomic write: write `<file>.tmp` in the same directory, `fsync` the file, rename it over `<file>`, `fsync` the directory.
- The app uses the same steps with its own temporary file: a new file `.<file>.<pid>.<n>.tmp` in the same directory, created with `O_EXCL` and the final mode. The app never deletes or replaces a file `<file>.tmp` of the user. When `<file>` is a symbolic link, the app writes the target of the link and the link stays a link. A crash can leave a temporary file: the app never reads it.
- Last-known-good copy: `<file>.lkg`. After a load that succeeds, the daemon writes the loaded content to `<file>.lkg` with the same atomic write, when the content differs.
- Times are microseconds since the Unix epoch, UTC. Identifiers follow `api.md`.

## Daemon state: `/var/lib/nicecooling/`

systemd creates the directory with `StateDirectory=nicecooling` and `StateDirectoryMode=0700`. The RPM owns the directory but no file in it.

| Path | Format | Content |
|---|---|---|
| `state.json` | JSON | Global state, see below. |
| `profiles/<profile id>/profile.json` | JSON | One Profile, see below. |
| `leakshield/<leakshield id>/slot-<n>/report3.bin` | binary | The Leakshield settings of slot n (1 to 4): the whole report 3 with its report ID byte, as the device produced it, except the `device_profile` byte (the number n) and the CRC that follows it. Every Profile with slot n uses it. A Profile activation writes it when n differs from report 1 `active_profile`. |
| `leakshield/<leakshield id>/slot-<n>/meta.json` | JSON | `{"schema_version":1, "firmware", "structure", "serial", "edited"}` of the stored `report3.bin`. `serial` is the text form of the Leakshield serial that produced the report. A Profile activation writes the report only to a device with that serial and that firmware. `edited` is a boolean. It is true when `SetStoredFields` changed the report. A file without `edited` reads as false. The next activation of an edited slot uses the verified write path (read-back), and a good read-back sets `edited` to false. |
| `leakshield/<leakshield id>/slot-<n>/assets/<asset id>.bin` | binary | Display assets of slot n. The asset write path is open until the reverse engineering answers it. |
| `bios-snapshot.json` | JSON | The BIOS snapshot, see below. |
| `alarm-history.json` | JSON | The Alarm history, see below. |
| `lease.json` | JSON | The takeover lease: `{"schema_version":1,"created_us":<t>,"boot_id":"<from /proc/sys/kernel/random/boot_id>"}`. The file exists from before the first manual write until a restore whose read-back matches. |
| `leakshield/<leakshield id>/last-known-report3.bin` | binary | Report 3 with its report ID byte, as the daemon last read or wrote it. External change detection compares against it, also after a daemon restart. |
| `backups/<leakshield id>/<UTC yyyymmddThhmmssZ>-<sha256 first 12 hex>/` | directory | One backup: `report3.bin`, `report5.bin`, `report8.bin` (each with its report ID byte) and `meta.json`. |
| `leakshield/<leakshield id>/refill-history.json` | JSON | The refill history, see below. |
| `leakshield/<leakshield id>/firmware-update.json` | JSON | The record of a firmware update that is not finished, see below. It exists only from before the restart into the bootloader until the device is back in normal operation. |

Every JSON file above has a `.lkg` copy, except `lease.json`, `firmware-update.json` and the backup `meta.json`.

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
| `alarm_responses` | object | Map from key to `{"response", "delay_s"}`. The key `<event>` is the default of the event. The key `<event>@<source>` is the override of a `leakshield.*` event for the Leakshield `<source>`. A key without an entry uses the next step of the resolution (`api.md`, Alarm response per Leakshield). A state file without `@` keys loads as before. |
| `rule_groups` | array | Ordered, the first group has the highest priority: `{"id", "profile", "enabled", "rules": [{"kind", "params"}]}` with the kinds and keys of `api.md`. |
| `graph_sensors` | array | `{"id", "name", "unit"}` of each Graph sensor. `id` is `graph_<n>`. `unit` is one of `degC`, `rpm`, `%`, `V`, `A`, `W`, `l/h`, `mbar`. At most 64 entries: at load the daemon keeps the first 64 with a unique id `graph_<n>` and drops the others with one journal line that names the count. A state file without this key loads as an empty list. |
| `graph_sensor_next` | integer | The number `n` of the next Graph sensor id. Default 1. The daemon adds 1 at each create and never lowers it, so it never gives an id twice. A new id is also above every Graph sensor and every `graph_<n>` that `names`, `software_sensors`, a Profile Curve Input or a rule sensor still holds: a downgrade drops the counter, and the old id must not return. The counter never wraps: at 4294967295 the last id is given and the next create fails. A state file without this key loads with 1. |
| `software_sensors` | object | Map from Leakshield id to a map from slot number ("1" to "12") to the sensor id in that slot. An empty slot has no entry. |
| `software_sensor_scales` | object | Map from Leakshield id to a map from slot number ("1" to "12") to `{"scale", "offset"}`. A slot with scale 1 and offset 0 has no entry. A state file without this key loads with the default for every slot. |
| `profiles` | object | `{"default": "profile_default", "active": <profile id>, "cause": "default"|"rule"|"manual", "immediate_switching": false, "rule_delay_s": 30, "dwell_s": 300}`. |

At load, the daemon drops an entry of `derived_sensors`, `loops` or `graph_sensors` whose `id` is not `[a-z0-9_]`, and writes one journal line that names it. It ignores a key of `identities` whose id prefix is not `[a-z0-9_]`: discovery gives that chip a new prefix. The ids are object path elements, so a bad id would stop the start.

### `profiles/<id>/profile.json`

The first start creates the Default Profile `profile_default` with slot 1 and three more Profiles with the slots 2, 3 and 4.

| Key | Type | Content |
|---|---|---|
| `schema_version` | integer | 1 |
| `id`, `name` | string | |
| `channels` | object | Map from Channel id to `{"control": "hardware"|"curve"|"fixed", "input", "points": [[input, duty]], "smoothing_s", "step_limit_pct_per_s", "fixed_duty"}`. |
| `pump_floors` | object | Map from Loop id to the Pump floor in percent. |
| `slot` | integer | The Leakshield slot of the Profile, 1 to 4. The Default Profile has 1. Several Profiles can have the same slot. |

`GetContent` returns the same keys as an `a{sv}`, plus `leakshield_fields`: for each Leakshield id, the decoded fields of the stored report 3 of the slot of the Profile, with the variant types of `GetFields` (`api.md`, Leakshield fields).

### `alarm-history.json`

| Key | Type | Content |
|---|---|---|
| `schema_version` | integer | 1 |
| `rows` | array | The rows, oldest raise first. At most 1000 rows with severity `alarm` and 1000 with severity `warning`. |

A row holds `instance`, `event`, `severity`, `source`, `response`, `raised_us`, `cleared_us` (0 while active), `outcome` (`api.md`, Alarm history), `closed_by_restart` (boolean) and `details`. `details` maps a key to `{"<signature>": value}`, with the signature `d`, `u`, `s` or `o`.

The file follows the common rules, with one change: the history has its own writer. A row change marks the history dirty and the writer saves at most once per second. A change to `poweroff-requested` saves at once, before the call to logind. The daemon saves the history at stop. A damaged file loads the `.lkg` copy, else an empty history, and the daemon logs it. `StateHealth` does not change, because the history controls nothing.

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

### `refill-history.json`

```json
{"schema_version": 1, "entries": [{"time_us": 1795000000000000, "duration_s": 12, "pump_turns": 60, "volume_tenth_ml": 1234}]}
```

| Key | Type | Content |
|---|---|---|
| `time_us` | integer | The time of the refill entry of report 1: device clock seconds since 2009-01-01 00:00 UTC, as microseconds since the Unix epoch. It is the key: the file holds one entry for each time, sorted by it. |
| `duration_s` | integer | The pump duration of the refill, in seconds. |
| `pump_turns` | integer | The pump revolutions of the refill. Above 0 for every stored entry. |
| `volume_tenth_ml` | integer | The volume of the refill, in 0.1 ml. |

The daemon writes the file when a report 1 adds or changes an entry, or when an entry passes 90 days of age by the PC clock. The device command `reset_refill_history` does not change it.

### `firmware-update.json`

```json
{"schema_version": 1, "serial": "20417-31072", "usb_path": "1-3", "hardware": 1000, "bootloader": 100,
 "firmware_before": 1022, "backup": "20261123T100000Z-0123456789ab", "started_us": 0,
 "image": {"id": 26, "version": 1022, "language": "en"}}
```

| Key | Type | Content |
|---|---|---|
| `serial` | string or null | The text form of the Leakshield serial. |
| `usb_path` | string | The USB port path of the device when the update started. |
| `hardware`, `bootloader`, `firmware_before` | integer or null | The values of report 1 before the restart into the bootloader. |
| `backup` | string or null | The directory name of the backup that the update saved. |
| `started_us` | integer | The start of the update. |
| `image` | object | The chosen image: the row `id` of the package database, its `version` and its `language` (`en` or `de`). It holds no image bytes. |

`serial`, `hardware`, `bootloader`, `firmware_before` and `backup` are null after a resume that had no record. The file has no `.lkg` copy. A file that does not parse, or that has a higher `schema_version`, counts as absent: the daemon logs it and the next update needs the option `unverified_target`. A job that ends with the device in normal operation deletes the file. The daemon also deletes it when it finds the device in normal operation.

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
language = "system"                 # "system", "en", "it", "de" or "fr"
tray = true
[notifications]
warnings = true
profile_switched_by_rule = false
external_change = true
[hints]
dismissed = []                      # ids of hints and dialogs closed with "Do not show this again"
[sensors]
hidden = []                         # sensor ids that the Sensors screen hides; the daemon has no hidden flag
[mqtt]                              # the MQTT broker; MQTT is off while `host` is empty
host = ""
port = 8883                         # absent: 1883, or 8883 with `tls = true`
tls = true
ca_file = ""                        # path of a CA file; empty: the system certificates
user = ""
client_id = ""                      # absent: "nicecooling-<hostname>"
base_topic = "nicecooling"
interval_s = 10                     # 1 to 3600
[smtp]                              # the SMTP account of the e-mail response
host = ""
port = 587                          # absent: 587 for "starttls", 465 for "tls", 25 for "none"
security = "starttls"               # "starttls", "tls" or "none"
user = ""
sender = ""                         # the From address
```

The tables `[mqtt]` and `[smtp]` hold no password. The passwords are in `secrets.toml`.

Known hint ids: `appindicator` (the tray hint) and `immediate_switching_warning` (the dialog of the Profiles screen).

### Start at login

The package installs the system autostart entry `/etc/xdg/autostart/io.github.cappyt.NiceCooling.desktop`, which runs `nicecooling --background`. The setting "Start at login" is not in `settings.toml`. It is the user override file `~/.config/autostart/io.github.cappyt.NiceCooling.desktop`:

- Off: the app writes the override with `Hidden=true`. It has `[Desktop Entry]`, `Type=Application`, `Name=NiceCooling` and `Hidden=true`. Its name equals the system entry, so the session ignores the system entry.
- On: the app removes the override file.
- The app shows "on" when no override with `Hidden=true` exists.

## App Graphs: `$XDG_CONFIG_HOME/nicecooling/` and `$XDG_DATA_HOME/nicecooling/`

The app is the only writer of these files. The daemon never reads them. One instance of the app runs for each user session: it holds the name `io.github.cappyt.NiceCooling` on the session bus, and a second start shows the window of the first and exits. Each file is written with the atomic write of the common rules, and has no `.lkg` copy.

| Path | Mode | Content |
|---|---|---|
| `$XDG_CONFIG_HOME/nicecooling/graphs.json` | default | All Graphs. |
| `$XDG_CONFIG_HOME/nicecooling/secrets.toml` | 0600 | The MQTT and SMTP passwords. |
| `$XDG_DATA_HOME/nicecooling/graph-state.json` | default | The stored values and the run state of the Graphs. |
| `$XDG_DATA_HOME/nicecooling/response-log.json` | default | The newest 500 response runs. |

A file that is not valid JSON or TOML, or that has a higher `schema_version` than the app knows, stays unchanged. The app writes no new content over it and shows an error. A missing file is the empty content.

### `graphs.json`

```json
{
  "schema_version": 1,
  "graphs": [
    {
      "id": "graph_1",
      "name": "Five",
      "enabled": true,
      "sensor_id": "graph_1",
      "nodes": [
        {"id": 1, "kind": "constant", "label": null, "params": {"value": 5}, "x": 40.0, "y": 80.0},
        {"id": 2, "kind": "sensor_end", "label": null, "params": {"unit": "degC"}, "x": 240.0, "y": 80.0}
      ],
      "connections": [{"from": {"node": 1, "pin": "out"}, "to": {"node": 2, "pin": "a"}}]
    }
  ]
}
```

- `id` is the id of the Graph. `sensor_id` is the id of its Graph sensor (`graph_<n>`), or `null` while the Graph has none. A Graph with a response end node has `sensor_id` `null`.
- A node has `id` (an integer, unique in its Graph), `kind` (one of the 51 names of `graphs.md`), `label` (`null` or 1 to 32 characters), `params`, and the canvas position `x` and `y`.
- A connection joins the output pin `out` of `from.node` to the input pin `to.pin` of `to.node`. The input pin names are in `graphs.md`.
- The app reads the file one Graph at a time. A Graph that does not parse, has a node of an unknown kind, or breaks a rule of the validator stays in the file with its stored text. The app lists it as "invalid" and does not run it. The other Graphs load.
- A change of the Graph list reads the file again first and edits what the file holds, so a change of another writer is not lost. A file that no longer loads stays as it is.
- The file holds at most 64 Graphs. An invalid Graph counts as one.
- A new Graph gets an id that no entry of the file holds, an invalid entry included. A write never replaces an entry that the app did not load as a valid Graph.

### `graph-state.json`

```json
{
  "schema_version": 1,
  "values": {"graph_2/4": 7.0},
  "responses": {"graph_3/5": {"state": "run", "last_run_us": 1766000000000000}},
  "pause": {"count": 1, "quiet_s": 840, "paused": false}
}
```

- `values` holds the value of each persistent storage node. The key is `<graph id>/<node id>`. The app deletes the entry of a node or a Graph that no longer exists.
- While `graphs.json` does not load, the app deletes no entry and writes no state. The file keeps its content until `graphs.json` loads again.
- `responses` holds the run state of each response end node, with the same key. `state` is `run` or `not_run`. `last_run_us` is the wall clock time of the last run, or `null`.
- `pause` holds the Response pause: `count` is the number of poweroff and reboot runs, `quiet_s` is the app run time in seconds since the last of them, and `paused` is true while the Response pause is on.
- The app writes the file at most every 30 s while a value or a run state changes, at its stop, and before a power call. A change of `quiet_s` alone does not start a write.

### `response-log.json`

```json
{
  "schema_version": 1,
  "rows": [
    {"time_us": 1766000000000000, "graph_id": "graph_3", "graph_name": "Hot water", "kind": "email", "outcome": "ok"}
  ]
}
```

- `kind` is one of `notification`, `sound`, `command`, `email`, `mqtt`, `poweroff`, `reboot` and `suspend`.
- `outcome` is one of `ok`, `failed`, `timeout`, `not-configured`, `busy` and `cancelled`.
- The rows are in the order of their runs. The app keeps the newest 500.

### `secrets.toml`

```toml
schema_version = 1
[mqtt]
password = ""
[smtp]
password = ""
```

The app creates the file with the mode 0600 and keeps this mode at each write. It never shows a stored password, never writes one to a log, and never puts one in an export file.

### Export file

The export of one Graph is one JSON file. It holds no `sensor_id`, no stored value, no run state and no password.

```json
{
  "format": "nicecooling-graph",
  "format_version": 1,
  "graph": {"name": "Five", "nodes": [], "connections": []}
}
```

`nodes` and `connections` have the form of `graphs.json`. The app refuses a file over 1 MiB and a file with a `format_version` above 1. An import runs the validator on the Graph of the file first and refuses a Graph that fails. Then it drops each parameter that the kind of its node does not define, gives the Graph and its nodes new ids and adds the Graph disabled, with `sensor_id` `null`.

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
