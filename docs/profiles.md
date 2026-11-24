# Profiles

This file describes how Profiles, Leakshield slots and Rule groups work, with example states. `api.md` defines the D-Bus methods. `storage.md` defines the files. The test `profiles_md_examples_load` loads every `json` block of this file with the state loader of the daemon.

## What a Profile holds

A Profile holds the Curves and fixed Duties of the Channels, the Pump floor of each Loop, and one Leakshield slot number from 1 to 4. It does not hold Leakshield settings. The daemon stores them for each Leakshield and each slot (`storage.md`). Profiles with the same slot share them.

The number of Profiles is not limited. The first start creates the Default Profile `profile_default` with slot 1, and three more Profiles with the slots 2, 3 and 4. The Default Profile cannot be deleted, and its slot cannot change.

The file `profiles/profile_default/profile.json` of a Default Profile with one Curve, one fixed Duty and a Pump floor:

```json
{
  "schema_version": 1,
  "id": "profile_default",
  "name": "Default",
  "channels": {
    "nct6798_pwm1": {
      "control": "curve",
      "input": "k10temp_temp1",
      "points": [[40.0, 20.0], [60.0, 50.0], [80.0, 100.0]],
      "smoothing_s": 5.0,
      "step_limit_pct_per_s": 5.0,
      "fixed_duty": null
    },
    "nct6798_pwm3": {
      "control": "fixed",
      "input": null,
      "points": [],
      "smoothing_s": null,
      "step_limit_pct_per_s": null,
      "fixed_duty": 50.0
    }
  },
  "pump_floors": {"loop_1": 40.0},
  "slot": 1
}
```

A Profile for gaming. It has the slot 3, so it uses the Leakshield settings of slot 3. A Channel that the Profile does not list stays on its Hardware curve:

```json
{
  "schema_version": 1,
  "id": "profile_3",
  "name": "Gaming",
  "channels": {
    "nct6798_pwm1": {
      "control": "curve",
      "input": "k10temp_temp1",
      "points": [[30.0, 40.0], [70.0, 100.0]],
      "smoothing_s": 2.0,
      "step_limit_pct_per_s": 10.0,
      "fixed_duty": null
    }
  },
  "pump_floors": {"loop_1": 60.0},
  "slot": 3
}
```

A quiet Profile. It shares the slot 3 with the Gaming Profile, so both use the same Leakshield settings:

```json
{
  "schema_version": 1,
  "id": "profile_5",
  "name": "Quiet",
  "channels": {},
  "pump_floors": {},
  "slot": 3
}
```

## Edits go into the active Profile

A change to a Curve goes into the active Profile. When no Rule group wins, the active Profile is the Default Profile. A change to a Leakshield setting that the user makes in nicecooling goes into the slot of the active Profile, so into every Profile with that slot.

## Activation

A switch writes the Curves, then, for each Leakshield, the stored report 3 of the slot of the Profile. It writes only when the slot number differs from the number that the device reports (report 1 `active_profile`). It does not compare bytes and does not read the report back. Thus:

- A switch between two Profiles with the same slot writes only the Curves.
- A switch to a Profile with another slot writes the whole report 3 of that slot over any change that another tool made.
- A change that the device button made stays when the slot number is the number of the device.
- The write also goes out while an Action runs and while the Mode is RELEASE (ADR-0007).
- The daemon start writes no setting and no report 3 to a Leakshield. It sets the device clock (`api.md`, Leakshield clock and sleep).

The daemon does not write a Leakshield whose firmware is unknown, whose firmware differs from the firmware of the stored report, or whose serial differs from the serial of that report. A write error has the same result. In each case the Curves stay applied, `Profiles.ActiveState` reads `leakshield-failed`, and the app offers the restore of the backup.

The daemon saves a backup before the write. The save-settings command follows 4 s after the last report 3 write.

### Spacing

The Leakshield write of a switch goes out at once when no write waits and the last profile write is older than 2 s. Else the worker keeps only the newest request and writes it 1.5 s after the last profile write. It drops the older requests.

## Rule groups

A Rule group holds a target Profile, an `enabled` flag and an ordered list of rules. A group holds when it is enabled and all its rules hold. A group without rules never holds. The list order is the priority. A rule is a sensor comparison, a process list, weekdays with a time window, or an interval timer (`api.md`, section Rule groups). There is no hotkey rule.

This `state.json` has one group of each rule kind. Group 1 holds while the process list has a command line with `Cyberpunk2077.exe`. Group 2 holds on Friday and Saturday from 22:00 to 02:00. Group 3 holds when the sensor reads more than 70 for 60 s. Group 4 holds for 120 s, 60 s after the start of each period of 10 minutes:

```json
{
  "schema_version": 1,
  "rule_groups": [
    {
      "id": "group_1",
      "profile": "profile_3",
      "enabled": true,
      "rules": [
        {"kind": "process", "params": {"names": ["Cyberpunk2077.exe"], "match": "any", "match_on": "cmdline"}}
      ]
    },
    {
      "id": "group_2",
      "profile": "profile_5",
      "enabled": true,
      "rules": [
        {"kind": "time", "params": {"days": [5, 6], "start": 1320, "end": 120}}
      ]
    },
    {
      "id": "group_3",
      "profile": "profile_3",
      "enabled": true,
      "rules": [
        {"kind": "sensor", "params": {"sensor": "k10temp_temp1", "comparison": "gt", "value": 70.0, "hold_s": 60}}
      ]
    },
    {
      "id": "group_4",
      "profile": "profile_5",
      "enabled": false,
      "rules": [
        {"kind": "interval", "params": {"interval_s": 600, "delay_s": 60, "duration_s": 120}}
      ]
    }
  ],
  "profiles": {
    "default": "profile_default",
    "active": "profile_default",
    "cause": "default",
    "immediate_switching": false,
    "rule_delay_s": 30,
    "dwell_s": 300
  }
}
```

The `sensor` rule with the comparison `in_range` has the key `value_max`. The key `match_on` of a process rule is `name` or `cmdline`. With `name`, the name must equal the `comm` or the executable name of a process, without case. With `cmdline`, the name must be a part of the command line, without case. A game under Proton needs `cmdline`, because its executable is the Wine loader. The daemon reads the process list once per second.

A time range that ends before it starts crosses midnight and belongs to the day of its start. In group 2, Saturday 01:00 belongs to Friday.

An interval window that ends after the period is cut at the end of the period. The first period starts when the daemon starts, or when `SetRuleGroups` saves a new or changed rule.

### Timing

- Each group has its own Rule delay timer. The winner is the first group in list order that has held for the Rule delay. A group that stops holding loses its timer.
- A group activates its Profile only when it becomes the winner. It does not write again on later ticks.
- A Profile that a group activated stays active for the Profile dwell. When the winning group stops holding, its Profile stays until the dwell ends. Then the winner of that time becomes active, or the Default Profile when no group wins.
- The Rule delay and the Profile dwell apply to every group. The defaults are 30 s and 300 s. `SetRuleTiming` changes them.
- A window of an interval rule that is shorter than the Rule delay never activates its Profile, unless Immediate switching is on.
- Immediate switching removes the Rule delay and the Profile dwell. A group change applies in the tick where it happens.

### Manual activation

A Profile that the user activates by hand becomes active at once, also during the Profile dwell. It stays active until `ReturnToAutomatic`. This `state.json` is the state after a manual activation of the Profile `profile_5`:

```json
{
  "schema_version": 1,
  "profiles": {
    "default": "profile_default",
    "active": "profile_5",
    "cause": "manual",
    "immediate_switching": true,
    "rule_delay_s": 10,
    "dwell_s": 60
  }
}
```

## Delete a Profile

`Delete` fails with `DefaultProfile` on the Default Profile. It fails with `InUse` for the active Profile and for a Profile that a Rule group targets. A delete removes `profiles/<id>/` and keeps the slot files of the Leakshields.

## Saving a Profile with RELEASE

A Profile contains RELEASE when its slot holds `default_mode` 3. Saving such a Profile asks the polkit action `write`, as any other save does. Setting the Mode RELEASE directly asks `critical`. A `wheel` member in the active local session gives no password for either action (ADR-0007).
