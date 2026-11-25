# Control API

## Purpose

Defines the D-Bus system API of the daemon and the polkit authorization of each action, so that clients and scripts can read and control nicecooling.

## ADDED Requirements

### Requirement: System bus name
The daemon SHALL expose its API on the system bus under the name `io.github.cappyt.NiceCooling1`.

#### Scenario: Name owned
- **WHEN** the daemon runs
- **THEN** `busctl` shows the name on the system bus

### Requirement: Objects
The API SHALL expose objects for Devices, Loops, Channels, sensors, Profiles and Alarms.

#### Scenario: Introspection
- **WHEN** a client introspects the daemon
- **THEN** it finds an object for each Channel and each Loop

#### Scenario: Graph sensor object
- **WHEN** the app creates a Graph sensor
- **THEN** the client finds a sensor object for it with the origin `graph`

### Requirement: Contract
The interfaces, methods, signals, errors and identifiers of the API SHALL match `crates/nicecooling-api/dbus/io.github.cappyt.NiceCooling1.xml` and `docs/api.md` of this change.

#### Scenario: Introspection equals the contract
- **WHEN** a test normalizes the introspection of the test daemon
- **THEN** it equals the XML of the contract

#### Scenario: No Verification write
- **WHEN** a client lists the methods of the API
- **THEN** no method performs a Verification write

### Requirement: State as properties
The API SHALL expose state as properties of its objects.

#### Scenario: Read the Duty
- **WHEN** a client reads the Duty property of a Channel
- **THEN** it gets the current Duty in percent

### Requirement: Sensor update signal
The daemon SHALL emit one SensorsUpdated signal for each tick.

#### Scenario: Ticks
- **WHEN** a client listens for 5 seconds
- **THEN** it receives 5 SensorsUpdated signals

### Requirement: Write methods
Every change of Curves, Profiles, settings and alarm responses SHALL go through a method of the API. nicecooling SHALL NOT ship a separate CLI and SHALL NOT read a hand-edited configuration file.

#### Scenario: Script control
- **WHEN** a script calls a write method with `busctl`
- **THEN** the daemon applies the change if polkit authorizes it

### Requirement: Read methods
The API SHALL allow read methods without a polkit check. No polkit action exists for them.

#### Scenario: Read as any user
- **WHEN** any local user reads a sensor
- **THEN** the daemon answers

### Requirement: Polkit actions
The API SHALL use three polkit actions: `io.github.cappyt.NiceCooling1.write`, `io.github.cappyt.NiceCooling1.cancel-poweroff` and `io.github.cappyt.NiceCooling1.critical`. Each method SHALL carry the annotation `io.github.cappyt.NiceCooling1.Authorization` in the XML, and the daemon SHALL ask the polkit action of that annotation. The default result of each action SHALL be `auth_admin_keep`.

#### Scenario: Every method has an annotation
- **WHEN** a test reads the XML of the contract
- **THEN** every method has an authorization annotation

#### Scenario: Polkit denial
- **WHEN** polkit denies a caller
- **THEN** the daemon returns `org.freedesktop.DBus.Error.AccessDenied`

### Requirement: Polkit call rules
A caller with uid 0 SHALL need no polkit check. The daemon SHALL pass `AllowUserInteraction` to polkit only when the method call carries the D-Bus flag `ALLOW_INTERACTIVE_AUTHORIZATION`.

#### Scenario: Root caller
- **WHEN** a caller with uid 0 calls a write method
- **THEN** the daemon applies it without a polkit check

#### Scenario: Interactive flag
- **WHEN** a method call carries no `ALLOW_INTERACTIVE_AUTHORIZATION` flag
- **THEN** the daemon does not pass `AllowUserInteraction` to polkit

### Requirement: Authorization action write
The action `write` SHALL be allowed for a member of group `wheel` in the active local session. For other callers it SHALL need admin authentication. The `write` methods are the changes of Curves, the Pump floor, Profiles, alarm responses and Leakshield settings. A `write` method that lowers an alarm response, the Pump floor or a pump Duty SHALL need no admin password.

#### Scenario: Wheel member
- **WHEN** a wheel member in the active local session changes a Curve
- **THEN** the daemon applies the change without a prompt

#### Scenario: Remote session
- **WHEN** a caller in a remote session changes a Curve
- **THEN** polkit does not grant it without authentication

#### Scenario: Lower an alarm response
- **WHEN** a wheel member in the active local session sets the alarm response of an Alarm to none
- **THEN** the daemon applies the change without a prompt

### Requirement: Authorization action cancel-poweroff
The action `cancel-poweroff` SHALL be allowed for a member of group `wheel` in a local session, whether the session is active or not. For other callers it SHALL need admin authentication. Only `CancelPoweroff` uses it.

#### Scenario: Cancel the poweroff
- **WHEN** a wheel member in the active local session calls CancelPoweroff
- **THEN** the daemon cancels the pending poweroff without a prompt

#### Scenario: Cancel from an inactive session
- **WHEN** a wheel member in an inactive local session calls CancelPoweroff
- **THEN** the daemon cancels the pending poweroff without a prompt

#### Scenario: Cancel from an SSH session
- **WHEN** a caller in an SSH session calls CancelPoweroff
- **THEN** polkit asks for admin authentication

### Requirement: Authorization action critical
The action `critical` SHALL be allowed for a member of group `wheel` in the active local session, as the action `write`. For other callers it SHALL need admin authentication, kept for a short time. The `critical` methods are: factory reset, setting Mode RELEASE directly (`SetMode` with the argument RELEASE) and restoring a backup. `SetMode` with another Mode SHALL use `write`.

#### Scenario: Set RELEASE directly
- **WHEN** a wheel member in the active local session sets Mode RELEASE
- **THEN** the daemon applies the change without a prompt

#### Scenario: Set RELEASE from an SSH session
- **WHEN** a caller in an SSH session sets Mode RELEASE
- **THEN** polkit asks for admin authentication

### Requirement: Alarm API
The API SHALL emit the signal AlarmRaised for each Alarm and offer the method CancelPoweroff. A signal SHALL carry event identifiers and no text.

#### Scenario: Cancel
- **WHEN** a client calls CancelPoweroff during the delay
- **THEN** the pending poweroff is cancelled

### Requirement: Channel snapshot and fan sensor
Each Channel object SHALL expose its BIOS snapshot entry (`BiosSnapshot`: the snapshot time and the saved attributes in restore order) and its fan speed sensor (`FanSensor`), by the pairing rule of the pump-stop Alarm.

#### Scenario: Channel with a snapshot entry
- **WHEN** the snapshot holds the attributes of `nct6798_pwm6`
- **THEN** `BiosSnapshot` of that Channel returns the snapshot time and those attributes in restore order

#### Scenario: Channel without a snapshot entry
- **WHEN** the snapshot exists and has no entry for a Channel
- **THEN** `BiosSnapshot` returns the snapshot time and no attribute

#### Scenario: No snapshot
- **WHEN** the daemon holds no snapshot
- **THEN** `BiosSnapshot` returns time 0 and no attribute

#### Scenario: Fan sensor
- **WHEN** the sensor `nct6798_fan6` exists
- **THEN** `FanSensor` of `nct6798_pwm6` is the object of that sensor, and `/` when the sensor does not exist

### Requirement: Curve readout
The `channels` array of `SensorsUpdated` SHALL carry the smoothed Input and the Curve target of each Channel, with a flag bit that marks both valid only when the Channel runs its Curve, no override runs, and the tick added a sample.

#### Scenario: Channel on a Curve
- **WHEN** a Channel runs a Curve with smoothing 5 s and the tick added a sample
- **THEN** its row carries the mean of the window, the Curve target and flag bit 2

#### Scenario: Lost Input
- **WHEN** the Input of the Curve has no value
- **THEN** the row has no flag bit 2, and the first row after the Input returns holds only new samples

#### Scenario: Input change
- **WHEN** `SetCurve` sets another Input sensor
- **THEN** the next mean holds only samples of the new sensor

#### Scenario: Channel not on a Curve
- **WHEN** a Channel has a fixed Duty or its Hardware curve
- **THEN** its row carries 0 and 0 without flag bit 2

### Requirement: Feed state
Each Leakshield object SHALL expose the state of its Feed message (`off`, `waiting`, `ok`, `failing`) and the time and the values of the last good send. The state SHALL be `off` while no Loop links the Leakshield, the registry does not cover its firmware, the device is disconnected or a firmware update runs.

#### Scenario: Feed fails and recovers
- **WHEN** a Feed send fails after good sends, and a later send succeeds
- **THEN** `FeedState` goes `ok`, `failing`, `ok`, and `LastFeed` holds the time and the values of the last good send

#### Scenario: Before the first send
- **WHEN** a Loop links the Leakshield and no send result exists yet
- **THEN** `FeedState` is `waiting` and `LastFeed` is (0, 0, 0)

#### Scenario: Loop unlinked while failing
- **WHEN** sends fail and the user removes the Leakshield from its Loop
- **THEN** `FeedState` becomes `off` and the failing time of the value table is reset

### Requirement: Leakshield slot and pressure data
Each Leakshield object SHALL expose the active slot of report 1 (`ActiveSlot`), the backup id of its last failed Profile write (`FailedWriteBackup`), and the sensors `pressure_setpoint`, `pressure_low` and `pressure_high` of report 1.

#### Scenario: Failed Profile write
- **WHEN** the Leakshield part of an activation fails on one of two Leakshields
- **THEN** `FailedWriteBackup` of that Leakshield holds the backup id of the write, and the other Leakshield has an empty value

#### Scenario: Good write after a failure
- **WHEN** a later Profile write of that Leakshield succeeds, or the user restores the backup
- **THEN** `FailedWriteBackup` becomes empty

#### Scenario: Pressure limits
- **WHEN** report 1 holds `pressure_setpoint` 3000 and `pressure_low` 2500
- **THEN** the sensors read 300 mbar and 250 mbar

### Requirement: Leakshield runtime values
Each Leakshield object SHALL expose the sensors of `docs/api.md` (Leakshield sensors and counters) and the method `GetCounters` (class `read`), which returns `pump_runtime_s`, `pump_short_cycle_runtime_s`, `pump_revolution_count`, `fill_valve_cycles`, `release_valve_cycles`, `total_runtime_s` and `power_up_count` of report 1.

#### Scenario: Leak rates
- **WHEN** report 1 holds a volume leak rate of 12 and a pressure leak rate of -4
- **THEN** the sensors `volume_change` and `pressure_change` read 12 ml/h and -0.4 mbar/min

#### Scenario: No value
- **WHEN** report 1 holds 32767 for the pump pressure
- **THEN** the sensor `pump_head_pressure` has no value
