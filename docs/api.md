# D-Bus API contract

`crates/nicecooling-api/dbus/io.github.cappyt.NiceCooling1.xml` is the interface contract. This file gives the semantics that the XML cannot hold. A test compares the introspection of the daemon with the XML (task 4.1).

## Bus and objects

- Bus: system bus, name `io.github.cappyt.NiceCooling1`.
- Root object `/io/github/cappyt/NiceCooling1`: interfaces `Manager`, `Profiles`, `Alarms` and `org.freedesktop.DBus.ObjectManager`. The app uses `InterfacesAdded` and `InterfacesRemoved` for hot-plug and new objects.
- Child objects: `board`, `sensor/<id>`, `channel/<id>`, `loop/<id>`, `leakshield/<id>`, `profile/<id>`, all under the root path.
- An object path `/` in an argument or a property means "none".

## Identifiers

Identifiers contain only `[a-z0-9_]`, so the object path element is the identifier itself. An identifier does not change between boots.

| Object | Identifier | Example |
|---|---|---|
| hwmon sensor | `<hwmon name>_<attribute prefix>`; a second chip with the same hwmon name gets `_<n>`, in device path order | `nct6798_temp1`, `asusec_fan1` |
| Channel | `<hwmon name>_pwm<N>` | `nct6798_pwm6` |
| Leakshield | `leakshield_<serial with - replaced by _>` | `leakshield_20417_31072` |
| Leakshield sensor | `<leakshield id>_<field id>` | `leakshield_20417_31072_pressure` |
| Derived sensor | `derived_<n>` | `derived_1` |
| Calibrated flow | `<loop id>_flow` | `loop_1_flow` |
| Loop | `loop_<n>` | `loop_1` |
| Profile | `profile_<n>`; the Default Profile is `profile_default` | `profile_2` |
| Rule | `rule_<n>` | `rule_3` |
| Calibration table | preset: the preset file id; user table: `table_<n>` | `alphacool_17558`, `table_1` |

The daemon keeps the map from identifier to hardware identity in its state (`storage.md`). The hardware identity is the hwmon `name` plus the resolved path of the hwmon `device` link, never `hwmonN`.

## Values and units

- Duty is a percent from 0 to 100 as `d`. Raw is the `pwmN` value 0-255.
- Temperatures in degrees Celsius, flow in L/h, pressure in mbar, volumes in ml, times in microseconds since the Unix epoch (`t`) unless an argument name says `_s`.
- `SensorsUpdated` flags, `sensors` array: bit 0 no value (stale for more than 3 s, or missing), bit 1 out of range of its Calibration table.
- `SensorsUpdated` flags, `channels` array: bit 0 override active (Lost Input or pump stop), bit 1 on its Hardware curve.

## Event identifiers

Alarms and Warnings use one identifier each. The app maps an identifier to its text in English and Italian. The daemon sends no text.

| Event | Severity | Source |
|---|---|---|
| `pump-stop` | alarm | Loop id |
| `leakshield-unreachable` | alarm | Leakshield id |
| `lost-input` | warning | Channel id |
| `another-writer` | warning | Channel id |
| `manual-at-snapshot` | warning | Channel id |
| `state-fallback` | warning | empty: the state loaded from the last-known-good copy, or no copy loaded |
| `leakshield.<name>` | warning or alarm | Leakshield id; one identifier for each Warning and Alarm that report 1 encodes, named in `docs/protocol/leakshield.md` |

## Read-only reasons

`ReadOnlyReason` and the reason of `GetFields` use these identifiers: `unknown-firmware`, `unknown-structure`, `not-verified`, `disconnected`, `action-running`.

## Leakshield fields

- A field id is our own snake_case name from the field registry, for example `reservoir_total_volume`. `docs/protocol/leakshield.md` lists them.
- The variant type comes from the registry: `b` for a flag, `s` for an enum value (our own lowercase name), `d` for a scaled number, `x` for an unscaled integer, `s` for a text.
- `SetFields` runs the write path of design D11 once for all fields. It returns after the read-back. The save-settings command (report 2) goes out 4 s after the last write; a failure then emits `WriteFailed`.

## Profile rule parameters

| Type | Key | Type | Meaning |
|---|---|---|---|
| process | `match` | `s` | `name`: exact, case-sensitive match on the basename of `/proc/<pid>/exe` or on `comm`. `cmdline`: case-insensitive substring of the command line. |
| process | `pattern` | `s` | The name or the substring. |
| time | `days` | `ay` | ISO weekdays, 1 Monday to 7 Sunday. |
| time | `start`, `end` | `q` | Minutes after midnight, system local time. When `end` is before `start`, the range ends on the next day and belongs to the day of `start`. |
| threshold | `sensor` | `o` | The Input. |
| threshold | `direction` | `s` | `above` or `below`. |
| threshold | `value` | `d` | The threshold in the unit of the sensor. |

## Errors

Error names have the prefix `io.github.cappyt.NiceCooling1.Error.`. polkit denials use `org.freedesktop.DBus.Error.AccessDenied`.

| Error | When |
|---|---|
| `InvalidArgument` | A value out of range, an unknown identifier type, a Curve with fewer than 2 points or points not sorted by Input. |
| `NotFound` | An object or an identifier does not exist. |
| `InUse` | A delete of a sensor, Calibration table or Profile that something else uses. |
| `DefaultProfile` | A delete of the Default Profile. |
| `ReadOnly` | A write to a preset or to a Leakshield that is read-only. The message holds the reason identifier. |
| `NotVerified` | A Leakshield field that is not Verified for this firmware. The message holds the field id. |
| `PumpRunning` | `StartAction("refill")` while a Pump channel of the Loop reads above 0 rpm. |
| `Busy` | The Leakshield write worker already holds a queued write. |
| `DeviceChanged` | Report 3 changed outside the volatile mask between the first read and the write. |
| `VerifyFailed` | The read-back differs outside the volatile mask. The message holds the backup id. |
| `Disconnected` | The device is not connected. |
| `NoPoweroff` | `CancelPoweroff` for an instance with no pending poweroff. |
| `NotSupported` | `CancelAction` or `ClearPumpRuntime` when the field registry has no such command for the firmware of the device. |

## Authorization

| Annotation value | polkit action | Default (any, inactive, active) | Rule |
|---|---|---|---|
| `read` | none | - | - |
| `write` | `io.github.cappyt.NiceCooling1.write` | `auth_admin_keep` x3 | YES for a `wheel` member, `subject.local` and `subject.active` |
| `cancel-poweroff` | `io.github.cappyt.NiceCooling1.cancel-poweroff` | `auth_admin_keep` x3 | YES for a `wheel` member, `subject.local`, active or not |
| `critical` | `io.github.cappyt.NiceCooling1.critical` | `auth_admin_keep` x3 | none |

- `write-or-critical` (`SetMode`): `critical` for `release`, `write` for the other Modes.
- A caller with uid 0 needs no polkit check.
- The daemon passes `AllowUserInteraction` to polkit only when the method call carries the D-Bus flag `ALLOW_INTERACTIVE_AUTHORIZATION`. The app sets this flag, so polkit can show its prompt.
- Files: policy `/usr/share/polkit-1/actions/io.github.cappyt.NiceCooling1.policy`, rule `/usr/share/polkit-1/rules.d/50-nicecooling.rules`, bus policy `/usr/share/dbus-1/system.d/io.github.cappyt.NiceCooling1.conf`. The bus policy lets root own the name and lets every user call every method; polkit decides.

## Daemon state seen from the app

When the daemon is not on the bus, the app reads the systemd unit `nicecoolingd.service` through `org.freedesktop.systemd1`. `ActiveState` `failed` with `Result` `start-limit-hit` means the restart limit is reached: the app shows "daemon down, Board on BIOS curves".
