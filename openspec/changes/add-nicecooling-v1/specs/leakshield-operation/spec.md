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

### Requirement: Refill only in standby
nicecooling SHALL NOT start a Refill while a Pump channel of the Loop of the Leakshield reads above 0 rpm. It SHALL tell the user to start the Refill with the device button with the PC in standby.

#### Scenario: Refill requested with pump on
- **WHEN** the user requests a Refill while a Pump channel of the Loop of the Leakshield reads above 0 rpm
- **THEN** the daemon refuses and shows the instruction to use the device button

#### Scenario: Pump of another Loop runs
- **WHEN** the user requests a Refill while only a Pump channel of another Loop reads above 0 rpm
- **THEN** the daemon starts the Refill

### Requirement: Cancel an Action
The user SHALL be able to cancel a running Action when the registry has a cancel command for the firmware. Without one, the daemon SHALL refuse the cancel with `NotSupported`.

#### Scenario: No cancel command
- **WHEN** the user cancels an Action and the registry has no cancel command for the firmware
- **THEN** the daemon replies `NotSupported` and the Action continues

### Requirement: Clear the pump runtime
The user SHALL be able to clear the pump runtime of the Leakshield.

#### Scenario: Clear
- **WHEN** the user clears the pump runtime
- **THEN** the daemon sends the documented command to the Leakshield

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

### Requirement: Device status and events
The daemon SHALL decode the Leakshield sensor report each tick and expose pressure, fill level, Mode, running Action, Warnings and Alarms.

#### Scenario: Warning decoded
- **WHEN** the device raises a Warning
- **THEN** the API shows the Warning with its type

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
