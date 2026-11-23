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
| Rule group | `group_<n>` | `group_3` |
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
| `no-bios-snapshot` | warning | Channel id: the kept BIOS snapshot has no entry for the Channel, so the daemon leaves it on its Hardware curve |
| `state-fallback` | warning | empty: the state loaded from the last-known-good copy, or no copy loaded |
| `leakshield-unknown-firmware` | warning | Leakshield id: the field registry does not cover the firmware of the device, so the daemon sends no Feed message to it. The Warning clears when the registry covers the firmware |
| `leakshield.fill-level-low-warning` | warning | Leakshield id: bit 0 of `alarm_bits` |
| `leakshield.fill-level-low-alarm` | alarm | Leakshield id: bit 1 |
| `leakshield.leak-warning` | warning | Leakshield id: bit 2 |
| `leakshield.pump-idle-time-warning` | warning | Leakshield id: bit 3 |
| `leakshield.leak-alarm-small` | alarm | Leakshield id: the small leak Alarm, bit 4 |
| `leakshield.leak-alarm` | alarm | Leakshield id: the large leak Alarm, bit 5 or bit 6 |
| `leakshield.system-alarm` | alarm | Leakshield id: the system Alarm, any of the bits 7 to 10 |

The Leakshield events follow the groups of `docs/protocol/leakshield.md`. A group is one event and one alarm response: bits 5 and 6 together are one large leak instance. The alarm response table (`GetResponses`) has one row for each event of this table, not one row for each bit. The event list of the device (`GetEvents`, codes 100 to 110) keeps the name of each bit.

An instance is one raise of an event for one source. Its identifier is `alarm_<start>_<n>`: `<start>` is the start time of the daemon in microseconds since the Unix epoch and `<n>` counts the instances of that start from 1. An identifier is unique across daemon restarts, so a cancel from a notification of an earlier start never reaches another instance. The alarm response of an instance is the response of its event when the instance rises. A change with `SetResponse` applies to the instances that rise after it. `CancelPoweroff` ends a pending countdown or retry, and the Alarm stays active. It returns `NoPoweroff` for an instance that has no pending poweroff and for an unknown instance, and `PoweroffStarted` for an instance whose call to logind is under way or was accepted: the PC powers off and no `PoweroffCancelled` follows. The daemon counts a logind call that does not answer in 10 s as failed: it emits `PoweroffFailed` and calls again after 5 s. A response `poweroff` with `delay_s` 0 calls logind in the tick where the instance rises.

Each active Alarm or Warning (`Alarms.Active`) and each `AlarmRaised` signal carries `details` (`a{sv}`) for the notification text: `value` (`d`) and `limit` (`d`) in `unit` (`s`) when the event compares a value with a limit, and `sensor` (`o`) when a sensor caused it, and `bits` (`u`) for a Leakshield event: the bits of `alarm_bits` that are set now in the group of the event (bits 5 and 6 for the large leak). The `bits` of an active instance follow `alarm_bits`; the signal carries the bits at the raise. A key is absent when it does not apply.

## Read-only reasons

`ReadOnlyReason` and the reason of `GetFields` use these identifiers: `unknown-firmware`, `unknown-structure`, `not-verified`, `disconnected`. An Action that runs does not make the Leakshield read-only: `SetFields`, `RestoreBackup` and a Profile activation write while it runs.

## Leakshield fields

- A field id is our own snake_case name from the field registry, for example `reservoir_total_volume`. `docs/protocol/leakshield.md` lists them.
- The variant type comes from the registry: `b` for a flag, `s` for an enum value (our own lowercase name), `d` for a scaled number, `x` for an unscaled integer, `s` for a text.
- `SetFields` runs the write path of design D11 once for all fields. It returns after the read-back. The save-settings command (report 2) goes out 4 s after the last write; a failure then emits `WriteFailed`.

## Leakshield backups

- `GetBackup` returns the firmware version of the backup, then reports 3, 5 and 8. `ImportBackup` takes the firmware as its first argument, then the three reports. The daemon stores the backup with that firmware.
- `ImportBackup` fails with `NotSupported` when the field registry does not cover the firmware. It fails with `InvalidArgument` for a wrong size, a bad CRC, a field out of its range or another structure id than the device has.
- `RestoreBackup` refuses a backup whose firmware differs from the firmware of the device.

## Leakshield events and Software sensors

- `GetEvents` returns the event list of report 1, oldest first. The identifier is `leakshield.<name>`, with the names of `docs/protocol/leakshield.md` (section Event list). A code that the document does not name gives `leakshield.event-<code>`. The time is the device time in microseconds since the Unix epoch: the clock offset is not applied. `details` holds `entry_id`, `mode`, `procedure_state`, `pressure` (mbar), `pressure_leak_rate`, `volume_leak_rate`, `value_1` and `value_2`, all as `d`. A value that the device marks as "no value" is absent.
- A Software sensor slot is a number from 1 to 12 and maps to Feed message slot 3 + n. `GetSoftwareSensors` returns all 12 slots, with `/` for an empty one. `SetSoftwareSensor` fills a slot with a sensor, or clears it with `/`. A slot outside 1 to 12 and a sensor whose unit has no device unit code fail with `InvalidArgument`. An unknown sensor fails with `NotFound`. The daemon keeps the assignment in `state.json` (`storage.md`), so a restart restores it. A stored sensor that no longer exists leaves its slot empty and stays in `state.json` until the slot is set again. The Feed message of a Loop carries the slots of its Leakshield.

## Device response proposals

`GetDeviceResponseProposals` returns one row for each Leakshield event that has a Device response: `leakshield.fill-level-low-warning`, `leakshield.fill-level-low-alarm`, `leakshield.leak-warning`, `leakshield.leak-alarm-small`, `leakshield.leak-alarm` and `leakshield.system-alarm`. A row is (event, current, proposed, overlap). `current` and `proposed` are `none`, `signal-short`, `signal-long` or `key-sequence`. `overlap` is empty or a warning id:

- `key-sequence-on`: the current Device response is the key sequence. The Leakshield types it into the focused window. The proposal is `none`, and `signal-long` for the system Alarm.
- `double-poweroff`: the current Device response is a signal and the alarm response of the event is poweroff. The proposal is `none`.

Without an overlap, the proposal for the system Alarm is `signal-long`, and the proposal for every other event is its current Device response. A row is absent when the daemon cannot read its field (unknown firmware or structure, or a disconnected device). The method is class `read`. The daemon never writes a proposal: the user accepts one with `SetFields`, and the write path applies its normal rules.

## Leakshield slots

A Leakshield slot is a number from 1 to 4, as in aquasuite (`docs/protocol/leakshield.md`, section Device profile numbers). The number is the device profile number: report 3 `device_profile` and report 1 `active_profile`. Each Profile references one slot with the property `Slot` of the interface `Profile`. `SetSlot` changes it (class `write`). The Default Profile has slot 1, and `SetSlot` on it fails with `InvalidArgument`. A slot outside 1 to 4 fails with `InvalidArgument`.

- The Leakshield settings of a slot are stored per Leakshield and per slot, not per Profile (`storage.md`). Profiles that share a slot share these settings.
- `CreateProfile` gives the new Profile the slot of the Profile that it copies. With `copy_from` `/`, it gives the slot of the active Profile.
- `SetSlot` on the active Profile applies the new slot as an activation does (design D14).

## Rule groups

A Rule group has an identifier, a target Profile, an `enabled` flag and an ordered list of rules. `GetRuleGroups` and `SetRuleGroups` use the type `a(soba(sa{sv}))`: group id, target Profile, enabled, and the rules as (kind, parameters). The list order is the priority.

- A group holds when it is enabled and all its rules hold. A group without rules never holds.
- The first group in list order that holds activates its Profile. Rule delay, Profile dwell and Immediate switching apply to the group (design D14). When no group holds, the Default Profile is active.
- `SetRuleGroups` replaces the whole list. An empty group id creates a group. It returns the group ids in list order. A target Profile that does not exist fails with `NotFound`. An unknown kind, a missing key or a value out of range fails with `InvalidArgument`.
- Hotkeys are not a rule kind. Wayland has no standard global shortcut.
- The property `RuleGroupState` gives, for each group, whether it holds and since when (`a(sbt)`: group id, holds, since in microseconds, 0 when it does not hold). It changes only when a group starts or stops holding.

| Kind | Key | Type | Meaning |
|---|---|---|---|
| sensor | `sensor` | `o` | The sensor to compare. A sensor with no value (`SensorsUpdated` flag bit 0) makes the rule false. |
| sensor | `comparison` | `s` | `lt`, `le`, `eq`, `ge`, `gt`, `ne` or `in_range`. |
| sensor | `value` | `d` | The compared value in the unit of the sensor. For `in_range`, the lower bound. |
| sensor | `value_max` | `d` | The upper bound. Only for `in_range`, and not less than `value`. |
| sensor | `hold_s` | `u` | The comparison must hold for this time before the rule holds. 0 means no hold time. |
| process | `names` | `as` | The process names. At least one. |
| process | `match` | `s` | `any`: one name matches. `all`: every name matches. `none`: no name matches. |
| process | `match_on` | `s` | `name` (the default) or `cmdline`. Owner decision: `cmdline` finds games that run under Proton, where the executable is the Wine loader. |
| time | `days` | `ay` | ISO weekdays, 1 Monday to 7 Sunday. |
| time | `start`, `end` | `q` | Minutes after midnight, system local time. When `end` is before `start`, the range ends on the next day and belongs to the day of `start`. |
| interval | `interval_s` | `u` | The period of the timer, at least 1. |
| interval | `delay_s` | `u` | The start delay inside each period. |
| interval | `duration_s` | `u` | The time that the rule holds, at least 1. |

- With `match_on` `name`, a process name matches when it equals, without case, the `comm` or the basename of `/proc/<pid>/exe` of a running process. With `match_on` `cmdline`, it matches when it is a substring, without case, of `/proc/<pid>/cmdline`. The daemon reads the process list once per second.
- The interval rule holds from `delay_s` to `delay_s + duration_s` after the start of each period of `interval_s`. A window that ends after the period is cut at the period end. The first period starts when the daemon starts or when `SetRuleGroups` saves the rule.
- A window that is shorter than the Rule delay never activates its Profile, unless Immediate switching is on.

## Limits

A write above a limit fails with `InvalidArgument`. Every limit is checked in the daemon.

| Item | Limit |
|---|---|
| Points of a Curve | 2 to 8 (the limit of the Curve editor, `docs/ui/widgets.md`) |
| Points of a Calibration table | 2 to 64 |
| Name of a sensor, Channel, Loop, Derived sensor or Calibration table | up to 64 characters |
| Derived sensors | up to 64 |
| Loops | up to 16 |
| Calibration tables of the user | up to 32 (presets do not count) |

All numbers in a Curve or a Calibration table are finite. A Duty is a percent from 0 to 100.

## Errors

Error names have the prefix `io.github.cappyt.NiceCooling1.Error.`. polkit denials use `org.freedesktop.DBus.Error.AccessDenied`.

| Error | When |
|---|---|
| `InvalidArgument` | A value out of range or above a limit, an unknown identifier type, a Curve with fewer than 2 points, points not sorted by Input, or a number that is not finite. |
| `NotFound` | An object or an identifier does not exist. |
| `InUse` | A delete of a sensor, Calibration table or Profile that something else uses. A Rule group that targets a Profile uses it. |
| `DefaultProfile` | A delete of the Default Profile. |
| `ReadOnly` | A write to a preset, to a Leakshield that is read-only, or a `SetCurve` or `SetFixedDuty` on a Channel without a BIOS snapshot entry (`no-bios-snapshot`). The message holds the reason identifier. |
| `NotVerified` | A Leakshield field that is not Verified for this firmware. The message holds the field id. |
| `PumpRunning` | `StartAction("refill")` while a Pump channel of the Loop reads above 0 rpm or has no tach value. |
| `Busy` | The Leakshield write worker already holds a queued write. |
| `DeviceChanged` | Report 3 changed outside the volatile mask between the first read and the write. |
| `VerifyFailed` | The read-back differs outside the volatile mask. The message holds the backup id. |
| `Disconnected` | The device is not connected. |
| `NoPoweroff` | `CancelPoweroff` for an instance with no pending poweroff. |
| `PoweroffStarted` | `CancelPoweroff` for an instance whose call to logind is under way or was accepted. |
| `NotSupported` | `ImportBackup` with a firmware that the field registry does not cover. `CancelAction`, `ResetRefillHistory`, `ResetServiceCounter` or `FactoryReset` when the field registry of the device firmware lacks the command (`cancel_action`, `reset_refill_history`, `reset_service_counter`, `factory_defaults`). |

## Authorization

| Annotation value | polkit action | Default (any, inactive, active) | Rule |
|---|---|---|---|
| `read` | none | - | - |
| `write` | `io.github.cappyt.NiceCooling1.write` | `auth_admin_keep` x3 | YES for a `wheel` member, `subject.local` and `subject.active` |
| `cancel-poweroff` | `io.github.cappyt.NiceCooling1.cancel-poweroff` | `auth_admin_keep` x3 | YES for a `wheel` member, `subject.local`, active or not |
| `critical` | `io.github.cappyt.NiceCooling1.critical` | `auth_admin_keep` x3 | none |

- `write-or-critical` (`SetMode`): `critical` for `release`, `write` for the other Modes. The Mode arguments are lowercase: `shield`, `monitor`, `release`. The check asks `critical` for `release` in any case and with surrounding whitespace.
- A caller with uid 0 needs no polkit check.
- The daemon passes `AllowUserInteraction` to polkit only when the method call carries the D-Bus flag `ALLOW_INTERACTIVE_AUTHORIZATION`. The app sets this flag, so polkit can show its prompt.
- Files: policy `/usr/share/polkit-1/actions/io.github.cappyt.NiceCooling1.policy`, rule `/usr/share/polkit-1/rules.d/50-nicecooling.rules`, bus policy `/usr/share/dbus-1/system.d/io.github.cappyt.NiceCooling1.conf`. The bus policy lets root own the name and lets every user call every method; polkit decides.

## Daemon state seen from the app

When the daemon is not on the bus, the app reads the systemd unit `nicecoolingd.service` through `org.freedesktop.systemd1`. `ActiveState` `failed` with `Result` `start-limit-hit` means the restart limit is reached: the app shows "daemon down, Board on BIOS curves".

## Examples with busctl

The examples use the Channel `nct6798_pwm1` and the sensors `k10temp_temp1` and `nct6798_temp1` of the test installation. `busctl tree io.github.cappyt.NiceCooling1` lists the ids of your installation. A write on the system bus asks polkit. Add `--allow-interactive-authorization=true` to let polkit show its prompt. The test `crates/nicecoolingd/tests/busctl_examples.rs` runs each block that is fenced as `busctl` against a test daemon on a private bus.

List the objects of the daemon:

```busctl
busctl tree io.github.cappyt.NiceCooling1
```

Read the version and the state health:

```busctl
busctl get-property io.github.cappyt.NiceCooling1 /io/github/cappyt/NiceCooling1 io.github.cappyt.NiceCooling1.Manager Version
busctl get-property io.github.cappyt.NiceCooling1 /io/github/cappyt/NiceCooling1 io.github.cappyt.NiceCooling1.Manager StateHealth
```

Read the Duty of a Channel and the value of a sensor:

```busctl
busctl get-property io.github.cappyt.NiceCooling1 /io/github/cappyt/NiceCooling1/channel/nct6798_pwm1 io.github.cappyt.NiceCooling1.Channel Duty
busctl get-property io.github.cappyt.NiceCooling1 /io/github/cappyt/NiceCooling1/sensor/k10temp_temp1 io.github.cappyt.NiceCooling1.Sensor Value
```

Show every property and method of a Channel:

```busctl
busctl introspect io.github.cappyt.NiceCooling1 /io/github/cappyt/NiceCooling1/channel/nct6798_pwm1 io.github.cappyt.NiceCooling1.Channel
```

Set a fixed Duty of 50%. The argument is a `d`:

```busctl
busctl --allow-interactive-authorization=true call io.github.cappyt.NiceCooling1 /io/github/cappyt/NiceCooling1/channel/nct6798_pwm1 io.github.cappyt.NiceCooling1.Channel SetFixedDuty d 50
```

Write a Curve. The signature `oa(dd)dd` holds the Input sensor, the number of points followed by the (input, duty) pairs, the smoothing in seconds and the step limit in percent per second. This Curve gives 20% at 40 degrees and 100% at 80 degrees:

```busctl
busctl --allow-interactive-authorization=true call io.github.cappyt.NiceCooling1 /io/github/cappyt/NiceCooling1/channel/nct6798_pwm1 io.github.cappyt.NiceCooling1.Channel SetCurve 'oa(dd)dd' /io/github/cappyt/NiceCooling1/sensor/k10temp_temp1 2 40 20 80 100 5 5
```

Create a Derived sensor, the maximum of two sensors. The reply holds its object path:

```busctl
busctl call io.github.cappyt.NiceCooling1 /io/github/cappyt/NiceCooling1 io.github.cappyt.NiceCooling1.Manager CreateDerivedSensor 'ssao' "Hottest" maximum 2 /io/github/cappyt/NiceCooling1/sensor/k10temp_temp1 /io/github/cappyt/NiceCooling1/sensor/nct6798_temp1
```

Create a Calibration table with two points (raw rpm, flow in L/h). The reply holds its id:

```busctl
busctl call io.github.cappyt.NiceCooling1 /io/github/cappyt/NiceCooling1 io.github.cappyt.NiceCooling1.Manager SetCalibrationTable 'ssa(dd)' "" "Test table" 2 100 10 200 20
```

Give a Channel back to its Hardware curve. The daemon restores the BIOS snapshot attributes of the Channel, then sets `Control` to `hardware`. When the restore fails, the call returns `org.freedesktop.DBus.Error.Failed` and `Control` stays as it was. A Channel that is on its Hardware curve already returns at once:

```busctl
busctl --allow-interactive-authorization=true call io.github.cappyt.NiceCooling1 /io/github/cappyt/NiceCooling1/channel/nct6798_pwm1 io.github.cappyt.NiceCooling1.Channel ReleaseToHardware
```

Watch the `SensorsUpdated` signal. This command runs until you stop it, so the test does not run it:

```text
busctl monitor --match "type='signal',interface='io.github.cappyt.NiceCooling1.Manager',member='SensorsUpdated'"
```
