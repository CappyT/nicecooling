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
The action `critical` SHALL need admin authentication, kept for a short time, for every caller. The `critical` methods are: factory reset, setting Mode RELEASE directly (`SetMode` with the argument RELEASE) and restoring a backup. `SetMode` with another Mode SHALL use `write`.

#### Scenario: Set RELEASE directly
- **WHEN** a wheel member sets Mode RELEASE
- **THEN** polkit asks for the admin password

### Requirement: Alarm API
The API SHALL emit the signal AlarmRaised for each Alarm and offer the method CancelPoweroff. A signal SHALL carry event identifiers and no text.

#### Scenario: Cancel
- **WHEN** a client calls CancelPoweroff during the delay
- **THEN** the pending poweroff is cancelled
