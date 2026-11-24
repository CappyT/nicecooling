# Leakshield operation

## Purpose

Defines how nicecooling operates a running Leakshield: Modes, Actions, the Feed message, Software sensor values, hot-plug and the device status and events.

## ADDED Requirements

### Requirement: Set the Mode
The user SHALL be able to set the Mode of the Leakshield to SHIELD, MONITOR or RELEASE. The Mode SHALL persist on the device.

#### Scenario: Set MONITOR
- **WHEN** the user sets the Mode to MONITOR
- **THEN** the device reports Mode MONITOR after the write

### Requirement: Run an Action
The user SHALL be able to start an Action: Test, Measure fill level, Deaerate or Refill. When an Action ends, the Leakshield SHALL return to its Mode.

#### Scenario: Test ends
- **WHEN** a Test ends
- **THEN** the Leakshield returns to its Mode

### Requirement: Refill in any pump state
The daemon SHALL start a Refill whatever the pump does. The app SHALL ask the user to confirm before it sends a Refill while a Pump channel of the Loop of the Leakshield reads above 0 rpm or has no tach value. The warning SHALL say that the pump runs and that the Leakshield manual asks for the pump off during Refill. The app SHALL send the Refill at once when every Pump channel of the Loop reads 0 rpm.

#### Scenario: Refill requested with pump on
- **WHEN** the user requests a Refill while a Pump channel of the Loop of the Leakshield reads above 0 rpm
- **THEN** the app shows the warning "Start Refill?" and sends nothing until the user confirms

#### Scenario: Refill confirmed with pump on
- **WHEN** the user confirms the warning
- **THEN** the daemon starts the Refill

#### Scenario: Refill requested with pump off
- **WHEN** the user requests a Refill while every Pump channel of the Loop reads 0 rpm
- **THEN** the app sends the Refill without a warning

#### Scenario: Pump of another Loop runs
- **WHEN** the user requests a Refill while only a Pump channel of another Loop reads above 0 rpm
- **THEN** the app sends the Refill without a warning

### Requirement: Cancel an Action
The user SHALL be able to cancel a running Action. The daemon SHALL send the command `cancel_action` (report 2 command 12, code 0). When the registry of the device firmware lacks that command, the daemon SHALL refuse the cancel with `NotSupported`.

#### Scenario: Cancel
- **WHEN** the user cancels a running Action
- **THEN** the daemon sends `cancel_action` and the Leakshield returns to its Mode

#### Scenario: No cancel command
- **WHEN** the user cancels an Action and the registry of the firmware has no cancel command
- **THEN** the daemon replies `NotSupported` and the Action continues

### Requirement: Reset the refill history and the service counter
The user SHALL be able to reset the refill history and the service counter of the Leakshield. The daemon SHALL send `reset_refill_history` (command 13) and `reset_service_counter` (command 25000, parameter: serial XOR 0xFFFFFFFF). When the registry of the device firmware lacks the command, the daemon SHALL refuse with `NotSupported`. The Leakshield has no command that clears the pump runtime, and nicecooling SHALL NOT offer one.

#### Scenario: Reset the refill history
- **WHEN** the user resets the refill history
- **THEN** the daemon sends command 13 to the Leakshield

#### Scenario: Reset the service counter
- **WHEN** the user resets the service counter
- **THEN** the daemon sends command 25000 with the serial XOR 0xFFFFFFFF as the parameter

#### Scenario: No command in the registry
- **WHEN** the user resets the refill history and the registry of the firmware has no such command
- **THEN** the daemon replies `NotSupported`

### Requirement: Send the Feed message
For each Loop with a Leakshield, the daemon SHALL send the Feed (pump speed and flow) in a Feed message every tick. The Feed message SHALL have 51 bytes: 49 bytes that start with `0x04`, then a big-endian CRC-16/USB over those 49 bytes. The message SHALL have 16 slots: slot 0 is the pump speed, slot 1 is the flow, slots 2 and 3 are spare, and slots 4 to 15 are the 12 Software sensors.

#### Scenario: Feed content
- **WHEN** the pump reads 3000 rpm and the calibrated flow is 115 L/h
- **THEN** the next Feed message carries those two values

#### Scenario: Pressure computed
- **WHEN** the Feed message arrives
- **THEN** the Leakshield computes the loop pressure from it

### Requirement: Software sensor values
The daemon SHALL write values into the 12 Software sensor slots of the Leakshield (Feed message slots 4 to 15), so that display pages and lighting can show them.

#### Scenario: Assign a sensor
- **WHEN** the user assigns the water temperature to a Software sensor slot
- **THEN** the slot carries that value on each update

The daemon SHALL give each slot a scale and an offset (default 1 and 0). A slot SHALL carry the value of its sensor times the scale plus the offset. The daemon SHALL keep both over a restart. It SHALL offer only sensors whose unit the device knows as the source of a slot.

#### Scenario: Scale and offset
- **WHEN** a slot has the scale 2 and the offset 5 and its sensor reads 10 degC
- **THEN** the slot carries 25 degC

#### Scenario: Result out of range
- **WHEN** the result does not fit the slot, or the sensor has no value
- **THEN** the slot carries no value

#### Scenario: Source list
- **WHEN** a client asks for the sources of a Software sensor slot
- **THEN** the list holds the sensors with a unit that has a device unit code and no sensor in ml

### Requirement: Device status and events
The daemon SHALL decode the Leakshield sensor report each tick and expose pressure, fill level, Mode, running Action, Warnings and Alarms.

The daemon SHALL also publish, for each Leakshield, the sensors `volume_change`, `pressure_change`, `leak_indicator`, `daily_pump_time`, `pump_head_pressure`, `air_volume`, `water_volume`, `supply_voltage`, `feed_rpm`, `flow` and `maintenance`, and SHALL return the counters of the report on request. A report value of 32767 SHALL give a sensor without a value.

#### Scenario: Values of the fw1022 report
- **WHEN** report 1 is the fw1022 dump
- **THEN** `pump_head_pressure` reads 101.2 mbar, `supply_voltage` 4.69 V, `air_volume` 108 ml, `water_volume` 162 ml, `feed_rpm` 2500 rpm and `flow` 130 L/h, and `volume_change`, `pressure_change` and `leak_indicator` have no value

#### Scenario: Maintenance indicator
- **WHEN** report 1 holds a pump runtime of 11431 s and 21 s in short cycles
- **THEN** the sensor `maintenance` reads 99.4 % (100 minus the squared pump hours over 16 and the squared short-cycle hours over 64, limited to 0 to 100)

#### Scenario: Leak rates of an event entry
- **WHEN** an event entry of report 1 holds a pressure leak rate of -2 and a volume leak rate of 3
- **THEN** `GetEvents` returns -0.2 and 0.3 for them

#### Scenario: Counters
- **WHEN** a client calls `GetCounters` and report 1 holds a pump runtime of 11431 s
- **THEN** `pump_runtime_s` is 11431

#### Scenario: Warning decoded
- **WHEN** the device raises a Warning
- **THEN** the API shows the Warning with its type

### Requirement: Alarm suppression at sleep
When the PC goes to sleep (logind `PrepareForSleep(true)`), the daemon SHALL send `suppress_alarm` (command 16, parameter 3600) to each Leakshield that it can write, before the sleep goes on. It SHALL hold a delay inhibitor of logind for this. The command SHALL NOT wait for a read of the device.

#### Scenario: PC goes to sleep
- **WHEN** logind sends `PrepareForSleep(true)` and a Leakshield is attached
- **THEN** the daemon sends command 16 with parameter 3600, and then releases the sleep

#### Scenario: Unknown firmware
- **WHEN** the Leakshield is read-only because the registry does not cover its firmware
- **THEN** the daemon sends it no command and still releases the sleep

### Requirement: Event log reset and Refill pause
The user SHALL be able to clear the event list of the Leakshield (`ResetEventLog`, command 14 with parameter 0) and to pause or resume a Refill (`ToggleRefillPause`, command 15 with parameter 0). Both SHALL be `write` class and SHALL fail with `NotSupported` when the registry of the firmware lacks the command.

#### Scenario: Reset the event list
- **WHEN** the user resets the event list
- **THEN** the daemon sends command 14 with parameter 0

#### Scenario: Pause a Refill
- **WHEN** the user pauses a running Refill
- **THEN** the daemon sends command 15 with parameter 0

### Requirement: Refill history
The daemon SHALL keep each refill entry of report 1 that has a time and pump revolutions above 0 and is at most 14 days old, keyed by its time, for 90 days, in its state directory, and SHALL return the entries with `GetRefillHistory`. The history SHALL stay when the device forgets its entries.

#### Scenario: Refill entries
- **WHEN** report 1 holds a refill of 12 s with 60 pump revolutions, a refill with no time and a refill with 0 pump revolutions
- **THEN** `GetRefillHistory` returns the first refill only

#### Scenario: Device history cleared
- **WHEN** the device no longer lists a stored refill, and the refill is less than 90 days old
- **THEN** `GetRefillHistory` still returns it, also after a restart of the daemon

#### Scenario: Retention
- **WHEN** a stored refill is older than 90 days
- **THEN** the daemon drops it

### Requirement: Coexistence with the in-tree kernel driver
The daemon SHALL read the Leakshield without disturbing the in-tree kernel driver that also reads report 1.

#### Scenario: Kernel driver loaded
- **WHEN** the in-tree kernel driver is loaded and the daemon runs
- **THEN** both read the sensors without errors

### Requirement: Hot-plug
The daemon SHALL reattach to the Leakshield on a udev event, matched by USB path, and SHALL resume reading and the Feed message after the reattach.

#### Scenario: Device replugged
- **WHEN** the Leakshield disconnects and connects again at the same USB path
- **THEN** the daemon reads report 1 and sends the Feed message again without a restart
