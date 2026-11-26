# Fan control

## Purpose

Defines the failure model of the daemon, how it runs Curves in software, how it protects the pump and keeps its Channel writes correct, and how it gives the Channels back to the Board with the BIOS snapshot.

## ADDED Requirements

### Requirement: Source tasks and value table
The daemon SHALL read each source in its own task and publish timestamped values to a value table. The sources are each hwmon chip, the Leakshield HID interface and the Leakshield bulk interface. The safety tick SHALL read only the value table and SHALL NOT wait on device I/O.

#### Scenario: Stuck device
- **WHEN** a source task blocks on a device read
- **THEN** the tick still runs every second and does not wait for that task

### Requirement: Stale value counts as no value
A value in the value table that is older than 3 seconds SHALL count as no value.

#### Scenario: Stuck Input
- **WHEN** the task of the Input sensor of a Curve publishes no value for 4 seconds
- **THEN** the Input has no value and the Lost Input rule applies

### Requirement: First tick after a pause
A pause is a suspend of the PC (the suspended time grows by 1 second or more between two ticks), or a gap of more than 2 seconds between two ticks. A value stamped before the end of the last pause SHALL count as no value in every later tick, also a Graph sensor value until the app sends a new one. The first tick after a pause SHALL wait at most 2 seconds for each sensor to read again, with no Warning if it does. Timers SHALL count the tick clock, which does not count a suspend.

#### Scenario: Resume from suspend
- **WHEN** the PC resumes after 6 hours of suspend and the Input sensor of a Curve reads 31.5 degC within 2 seconds
- **THEN** the Curve uses 31.5 degC and the daemon raises no `lost-input` Warning

#### Scenario: Suspend with no gap in the tick clock
- **WHEN** the tick clock advances 1 second, the suspended time grows by 6 hours and the Input sensor does not read within 2 seconds
- **THEN** the Input has no value, though its last value is 1 second old, and the Lost Input rule applies

#### Scenario: Sensor gone after the pause
- **WHEN** the PC resumes and the Input sensor of a Curve does not read within 2 seconds
- **THEN** the Input has no value and the Lost Input rule applies

#### Scenario: Value from before the pause
- **WHEN** the app sent 46.2 for a Graph sensor 1 second before a suspend and the PC resumes, with a gap of 0.5 seconds in the tick clock
- **THEN** the Graph sensor has no value in the first, the second and the third tick after the resume, until the app sends a new value

#### Scenario: Noise in the suspended time
- **WHEN** the suspended time grows by 0.9 seconds between two ticks
- **THEN** the tick is no pause and keeps its values

### Requirement: Watchdog and restart
The systemd unit SHALL use `Type=notify`, `WatchdogSec=10s`, `TimeoutAbortSec=5s`, `Restart=on-failure` and `RestartSec=2s`. Its `[Unit]` section SHALL set `StartLimitIntervalSec=60s` and `StartLimitBurst=5`. The tick SHALL ping the watchdog after stages 1 to 10 of the tick order. `ExecStopPost=/usr/libexec/nicecoolingd --restore-bios` SHALL restore the BIOS snapshot.

#### Scenario: Tick hangs
- **WHEN** the tick does not reach the watchdog ping for 10 seconds
- **THEN** systemd sends SIGABRT, kills the daemon after 5 more seconds if it still runs, and the restore step runs

#### Scenario: Frozen daemon
- **WHEN** the daemon process is frozen with SIGSTOP
- **THEN** at most 15 seconds after the last ping systemd kills it, and the Channels have their BIOS snapshot attributes within 20 seconds

#### Scenario: Restart limit reached
- **WHEN** the daemon fails 6 times within 60 seconds
- **THEN** systemd stops restarting it and the Channels have their BIOS snapshot attributes

### Requirement: Tick order
Every second the daemon SHALL run these stages in order: read the value table; compute Derived sensors and calibrated flows; evaluate the Alarms; evaluate the Profile rules; compute each Curve from its smoothed Input; apply the Duty step limit; apply the Pump floor; apply the overrides; reconcile and write each Channel; send the Feed message; ping the watchdog.

#### Scenario: Override after the step limit
- **WHEN** the tick applies the pump stop override
- **THEN** the override sets 100% after the step limit and the Pump floor stage

### Requirement: Software Curves
The daemon SHALL compute each Curve in software every second and set the Duty of its Channel. A Curve SHALL interpolate linearly between its points.

#### Scenario: Duty between two points
- **WHEN** a Curve has the points 40 C = 30% and 60 C = 70% and its Input reads 50 C
- **THEN** the daemon sets the Duty to 50%

#### Scenario: Input outside the Curve
- **WHEN** the Input is below the first point or above the last point
- **THEN** the Duty equals the Duty of the nearest point

### Requirement: Input smoothing
A Curve SHALL read its Input through a moving average. The default window is 5 seconds, and the user SHALL be able to change it per Curve.

#### Scenario: Short spike
- **WHEN** the Input jumps from 40 C to 70 C for one second and goes back to 40 C
- **THEN** the Curve sees at most 46 C

### Requirement: Duty step limit
The Duty of a Channel under a Curve SHALL change by at most a fixed step per second. The default step is 5%, and the user SHALL be able to change it per Curve. The overrides Lost Input and pump stop SHALL bypass the step limit. When an override ends, the step limit SHALL ramp the Duty down from 100%.

#### Scenario: Large change
- **WHEN** the Curve computes 80% and the current Duty is 40%
- **THEN** the Duty reaches 80% after 8 seconds, in steps of 5%

#### Scenario: Override ignores the step limit
- **WHEN** an override sets 100% while the Duty is 40%
- **THEN** the Duty is 100% in the same tick

#### Scenario: Override ends
- **WHEN** an override ends and the Curve computes 40%
- **THEN** the Duty falls from 100% in steps of 5% per second

### Requirement: Lost Input
When the Input of a Curve has no value, the daemon SHALL set the Channel to 100%. It SHALL raise a Warning until the Input has a value again.

#### Scenario: Probe disconnected
- **WHEN** the water temperature sensor of a Curve stops giving values
- **THEN** the Channel goes to 100% and a lost-Input Warning is raised

#### Scenario: Input returns
- **WHEN** the Input gives a value again
- **THEN** the Warning closes and the Curve controls the Channel again

#### Scenario: Graph sensor not fed
- **WHEN** the Input of a Curve is a Graph sensor and the app sends no value for 4 seconds
- **THEN** the Channel goes to 100% and a lost-Input Warning is raised

### Requirement: Pump floor
The daemon SHALL apply the Pump floor of each Loop after the Duty step limit. Neither a Curve nor a fixed Duty SHALL set the Duty of a Pump channel below it.

#### Scenario: Curve below the floor
- **WHEN** the Pump floor is 40% and a Curve computes 20%
- **THEN** the daemon sets the Duty to 40%

#### Scenario: Fixed Duty below the floor
- **WHEN** the Pump floor is 40% and the user sets a fixed Duty of 20% on the Pump channel
- **THEN** the daemon sets the Duty to 40%

### Requirement: Reconcile Channel writes
Each tick, the daemon SHALL read back `pwmN` and `pwmN_enable` of each Channel it controls. A mismatch is a `pwmN` that differs from the desired raw value, or a mode that is not manual. The daemon SHALL write a Channel on a mismatch. A Channel in manual mode at raw 255 reads `pwmN_enable` 0, and the daemon SHALL treat enable 0 with raw 255 as manual at 255. Three mismatches in 60 seconds SHALL raise an "another writer" Warning.

#### Scenario: Manual at 255
- **WHEN** a Channel is at raw 255 and `pwmN_enable` reads 0
- **THEN** the daemon counts no mismatch for a desired Duty of 100%

### Requirement: Duty to raw value
The daemon SHALL convert a Duty to the raw value `round(duty * 255 / 100)`, half away from zero, clamped to 0-255.

#### Scenario: Rounding
- **WHEN** the desired Duty is 50%
- **THEN** the daemon writes the raw value 128

#### Scenario: Constant Duty
- **WHEN** a Curve gives the same Duty for 10 ticks and the read-back matches
- **THEN** the daemon writes the Channel once

#### Scenario: Another writer
- **WHEN** another tool changes `pwmN` and the daemon corrects it three times in 60 seconds
- **THEN** the daemon raises an "another writer" Warning

#### Scenario: Resume
- **WHEN** the PC resumes from suspend and the Board reset a Channel
- **THEN** the daemon writes the desired Duty to that Channel within one tick

### Requirement: Take control of Channels
The daemon SHALL put each Channel that a Curve controls in manual mode and set its Duty. Manual mode SHALL mean `pwmN_enable` = 1. A Hardware curve cannot read the Leakshield, the GPU or a Derived sensor.

#### Scenario: Channel under a Curve
- **WHEN** the daemon activates a Curve on a Channel
- **THEN** the Channel is in manual mode and follows the Duty of the Curve

### Requirement: GPU fans are never written
The daemon SHALL NOT write any `pwmN` attribute of a hwmon device whose driver is `amdgpu`, `radeon`, `nouveau`, `i915`, `xe` or `aquacomputer_d5next`. The BIOS snapshot, the takeover, the restore and the pump stop SHALL exclude those devices.

#### Scenario: GPU node with pwm1
- **WHEN** the fake tree has an `amdgpu` hwmon device with `pwm1` and the pump stop override runs
- **THEN** the daemon writes no attribute of that device and the BIOS snapshot holds none of its attributes

### Requirement: Channels without a Curve stay on the Board
The daemon SHALL NOT take control of a Channel that has no Curve and no fixed Duty. That Channel SHALL stay on its Hardware curve, except while the pump stop override runs.

#### Scenario: Unused Channel
- **WHEN** the CPU Fan Channel has no Curve and no fixed Duty
- **THEN** the daemon does not write it and the chip runs its Hardware curve

### Requirement: BIOS snapshot
Before the daemon takes control, it SHALL save the BIOS snapshot of every writable attribute of every Channel. It SHALL write a takeover lease file before the first manual write. It SHALL refresh the snapshot only when no lease exists.

#### Scenario: First start
- **WHEN** the daemon starts and no snapshot exists
- **THEN** it saves the snapshot before it changes any Channel

#### Scenario: Restart while in control
- **WHEN** the daemon restarts after a crash and a lease exists
- **THEN** the daemon keeps the existing snapshot unchanged

#### Scenario: First start with a manual Channel
- **WHEN** the daemon starts with no snapshot and a Channel already in manual mode
- **THEN** it takes the snapshot, marks that Channel "manual at snapshot" and raises a Warning

### Requirement: Unreadable state
When the daemon state file and its last-known-good copy both fail to load, the daemon SHALL start without Curves, leave every Channel on its Hardware curve, keep the unreadable files, and raise a Warning.

#### Scenario: Both copies corrupt
- **WHEN** the state file and the last-known-good copy both fail to parse at start
- **THEN** no Channel is in manual mode, the broken files stay on disk, and the `state-fallback` Warning is raised

### Requirement: Restore the BIOS snapshot
When the daemon stops, and after a crash, the system SHALL write the BIOS snapshot back to every Channel. The daemon SHALL remove the takeover lease only after a restore whose read-back matches the snapshot.

#### Scenario: Normal stop
- **WHEN** the service stops
- **THEN** every Channel has its snapshot attributes again and the lease is gone

#### Scenario: Crash
- **WHEN** the daemon process dies
- **THEN** the restore step still runs and writes the snapshot back

#### Scenario: Restore does not match
- **WHEN** the read-back after a restore differs from the snapshot
- **THEN** the lease stays and the next start does not refresh the snapshot

#### Scenario: A Channel stays off its snapshot at the stop
- **WHEN** `nicecoolingd --restore-bios` restores a Channel and its read-back differs from the snapshot
- **THEN** that Channel is in manual mode at raw 255 and the command exits with an error

#### Scenario: Restore from an automatic mode
- **WHEN** a Channel reads `pwmN_enable` 2 or more at restore time
- **THEN** the restore writes no `pwmN` while the Channel is in an automatic mode

#### Scenario: Restore command without a lease
- **WHEN** `nicecoolingd --restore-bios` runs and no lease exists
- **THEN** it changes nothing and exits 0

#### Scenario: Restore command without a snapshot
- **WHEN** `nicecoolingd --restore-bios` runs, a lease exists, and neither the snapshot nor its copy can be read
- **THEN** every controlled Channel goes to manual mode at raw 255 and the `state-fallback` Warning is raised
