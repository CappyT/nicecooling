# Leakshield settings

## Purpose

Defines how nicecooling reads, decodes, writes, backs up and restores the settings of the Leakshield, how the field registry holds the verification state, and how it handles the device clock and changes made by other tools.

## ADDED Requirements

### Requirement: Read and show every setting
nicecooling SHALL read the Leakshield settings from the device and show them. The settings SHALL cover Modes, Actions, every setting of the aquasuite wizard, alarms, display pages, Software sensors, RGBpx lighting and the clock.

#### Scenario: Settings shown
- **WHEN** a Leakshield is connected
- **THEN** the user can read each of those settings, writable or not

### Requirement: No firmware update
nicecooling SHALL NOT update the Leakshield firmware.

#### Scenario: Firmware update requested
- **WHEN** the user looks for a firmware update function
- **THEN** none exists and the documentation says to use aquasuite in the VM

### Requirement: Layout keyed by firmware version
nicecooling SHALL decode the settings with a field layout that matches the device model and the firmware version. The firmware version SHALL come from bytes 13-14 of report 1.

#### Scenario: Known firmware
- **WHEN** the device model and firmware version have a layout
- **THEN** nicecooling decodes the settings with that layout

#### Scenario: Unknown firmware
- **WHEN** the firmware version has no layout
- **THEN** every field is read-only and the UI states the reason

#### Scenario: Firmware 1019
- **WHEN** the device runs firmware 1019
- **THEN** every field is read-only, because the registry is keyed on firmware 1022

### Requirement: Field registry with static verification state
The field registry SHALL hold, for each field of a model and firmware version: report, offset, type, scale, range, aquasuite default, risk class, verification state and the volatile mask. The verification state SHALL be static registry data with four values: `code-defined`, `plausible-read` (a read on the fixtures is in range), `verified-write` and `failed` (a supervised Verification write during development passed or failed).

#### Scenario: Registry on the reference firmware
- **WHEN** a test loads the registry for firmware 1022 and decodes the fixtures in `tests/fixtures/leakshield/fw1022/`
- **THEN** every field is in range and has a risk class and a verification state

#### Scenario: Old fixtures
- **WHEN** a test decodes the fixtures in `tests/fixtures/leakshield/fw1019/`
- **THEN** the decode succeeds and every field is read-only

#### Scenario: No runtime verification
- **WHEN** the daemon runs
- **THEN** it changes no verification state and the API offers no Verification write

### Requirement: Write only Verified fields
A write of single settings SHALL change only Verified fields: a low-risk field at `plausible-read` or `verified-write`, and a Safety field at `verified-write`. It SHALL keep every other byte of the device settings unchanged. A Profile activation SHALL be the only exception: it writes the whole stored settings that the device produced on the same firmware.

#### Scenario: Safety field not verified by a write
- **WHEN** a client asks to write a Safety field at the state `plausible-read`
- **THEN** the daemon refuses the write and the device settings stay unchanged

#### Scenario: Unverified field
- **WHEN** a client asks to write a field that is not a Verified field
- **THEN** the daemon refuses the write and the device settings stay unchanged

### Requirement: Volatile mask
The registry SHALL hold a mask of the bytes that the device changes by itself. The read-back comparison and the external change detection SHALL both use it. The CRC bytes SHALL be expected to change on a write.

#### Scenario: Device changes a volatile byte
- **WHEN** the device changes a byte inside the volatile mask between two reads
- **THEN** the daemon reports no unexpected change

### Requirement: Re-read before write
Just before a write, the daemon SHALL read report 3 again and SHALL abort the write when it changed outside the volatile mask.

#### Scenario: Changed between read and write
- **WHEN** another tool changes a byte of report 3 after the first read and before the write
- **THEN** the daemon aborts the write and reports why

### Requirement: Backup before every write
Before each write, the daemon SHALL save a backup of reports 3, 5 and 8, keyed by the serial number of the Leakshield. Retention SHALL be the first backup per serial plus the last 20, deduplicated by hash.

#### Scenario: Backup exists
- **WHEN** the user changes a setting
- **THEN** a backup of the settings from before the change exists before the write starts

#### Scenario: Retention
- **WHEN** the 22nd distinct backup for a serial is saved
- **THEN** the first backup and the last 20 remain, and no backup duplicates another

### Requirement: Save settings after a write
After the last report 3 write, the daemon SHALL send one output report 2 save-settings command 4 seconds later. The write worker SHALL own the 4 second timer, so that more writes inside 4 seconds give one save-settings command. A failed save-settings command SHALL emit `WriteFailed`.

#### Scenario: Three writes in 4 seconds
- **WHEN** the daemon writes report 3 three times within 4 seconds
- **THEN** it sends one save-settings command 4 seconds after the last write

#### Scenario: Save-settings command fails
- **WHEN** the save-settings transfer fails
- **THEN** the daemon emits `WriteFailed` and offers the restore of the backup

### Requirement: One write at a time
The daemon SHALL run Leakshield setting writes in a separate worker with a queue of one.

#### Scenario: Queue full
- **WHEN** a write is queued and a client asks for another write
- **THEN** the daemon returns `Busy`

### Requirement: Read back and compare
After each write, the daemon SHALL read the settings back and check that only the expected bytes changed outside the volatile mask.

#### Scenario: Expected change only
- **WHEN** the readback differs from the backup only in the target field, the CRC bytes and the volatile mask
- **THEN** the write succeeds

#### Scenario: Unexpected change
- **WHEN** the readback differs in other bytes
- **THEN** the daemon reports an error and offers the restore of the backup

### Requirement: Restore a backup
The user SHALL be able to restore a backup of the device settings. A restore SHALL use the polkit action `critical`, which needs no admin password for a `wheel` member in the active local session. A restore SHALL write only report 3, until reports 5 and 8 are decoded.

#### Scenario: Restore with authorization
- **WHEN** an authorized caller restores a backup
- **THEN** report 3 of the device equals the backup and the daemon reads it back to confirm

#### Scenario: Reports 5 and 8 not decoded
- **WHEN** a backup holds reports 3, 5 and 8 and the user restores it
- **THEN** the daemon writes report 3 only

### Requirement: The Leakshield owns its settings
nicecooling SHALL NOT write the Leakshield settings at boot or when they drift from the active Profile. It SHALL write them only when the user changes a setting or activates a Profile.

#### Scenario: Daemon starts
- **WHEN** the daemon starts and the device settings differ from the active Profile
- **THEN** the daemon writes nothing to the Leakshield

### Requirement: Detect changes by other tools
The daemon SHALL read report 3 every 10 seconds and compare it with the last known value outside the volatile mask. A difference SHALL be an external change. After its own verified write, the daemon SHALL update the last known value. For an external change, nicecooling SHALL show the new value, keep it and send a notification. The notification SHALL say that the next Profile activation overwrites the value.

#### Scenario: Change on the device button
- **WHEN** the user changes a setting with the device button
- **THEN** within 10 seconds nicecooling shows the new value and notifies the user, and no Profile contains the change

#### Scenario: Own write
- **WHEN** the daemon completes a verified write
- **THEN** the next 10 second read reports no external change

### Requirement: Changes go into the active Profile
A setting that the user changes in nicecooling SHALL be saved into the active Profile.

#### Scenario: Change saved
- **WHEN** the user changes a Verified field while Profile "Quiet" is active
- **THEN** Profile "Quiet" holds the new value

### Requirement: Device clock
The user SHALL be able to read the clock of the Leakshield and set it from the PC clock at any offset. The daemon SHALL set the device clock to the PC time by itself only when the device time differs from the PC time by more than 60 seconds. This is the only Leakshield write that the daemon makes without a user action or a Profile activation.

#### Scenario: Set the clock
- **WHEN** the user sets the clock to the PC time
- **THEN** the device clock reads the PC time within one second

#### Scenario: Set the clock at a small offset
- **WHEN** the device time differs from the PC time by 5 seconds and the user sets the clock to the PC time
- **THEN** the device clock reads the PC time within one second

#### Scenario: Clock lost after a power failure
- **WHEN** the device time differs from the PC time by 3 hours
- **THEN** the daemon sets the device clock to the PC time

#### Scenario: Small drift
- **WHEN** the device time differs from the PC time by 20 seconds
- **THEN** the daemon does not write the device clock
