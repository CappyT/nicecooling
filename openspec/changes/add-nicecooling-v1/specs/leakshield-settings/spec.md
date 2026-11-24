# Leakshield settings

## Purpose

Defines how nicecooling reads, decodes, writes, backs up and restores the settings of the Leakshield, how the field registry holds the verification state, and how it handles the device clock and changes made by other tools.

## ADDED Requirements

### Requirement: Read and show every setting
nicecooling SHALL read the Leakshield settings from the device and show them. The settings SHALL cover Modes, Actions, every setting of the aquasuite wizard, alarms, display pages, Software sensors, RGBpx lighting and the clock.

#### Scenario: Settings shown
- **WHEN** a Leakshield is connected
- **THEN** the user can read each of those settings, writable or not

### Requirement: Start speed at or below the maximum speed
The wizard SHALL keep the start speed of the pump at or below its maximum speed. After a preset and after an edit of either speed, the wizard SHALL cut a higher start speed to the maximum speed.

#### Scenario: Preset with a higher start speed
- **WHEN** the user selects a pump preset whose table has the start speed 5500 rpm and the maximum speed 4850 rpm
- **THEN** the pending changes hold the start speed 4850 rpm

### Requirement: A wizard switch changes one bit
A switch of the wizard changes one bit of a flag field. "Apply" SHALL write that bit onto the value that the device holds at that time, and SHALL leave the other bits of the field as the device holds them.

#### Scenario: Another screen changed the same field
- **WHEN** the user turns adaptive pressure off in the wizard, then another screen sets a different bit of the same field, then the user selects "Apply"
- **THEN** the write holds the adaptive bit clear and the bit of the other screen set

### Requirement: Presets write their numbers
A pump preset SHALL write the maximum speed, the start speed and the maximum pressure of the pump type into the pending changes. A reservoir preset SHALL write the total volume and the two air volumes of the reservoir type. With a preset these numbers SHALL be read-only in the wizard. The choice "Custom" SHALL keep the numbers and open them for an edit. `docs/ui/screens/leakshield-settings.md` has the tables of the presets.

#### Scenario: Pump preset
- **WHEN** the user selects the pump type DDC-1T
- **THEN** the pending changes hold the maximum speed 3900 rpm, the start speed 3900 rpm and the maximum pressure 350 mbar

#### Scenario: Reservoir preset
- **WHEN** the user selects the reservoir type aqualis 450
- **THEN** the pending changes hold the total volume 450 ml and the air volumes 70 ml and 350 ml

#### Scenario: Custom
- **WHEN** the user selects "Custom" after a preset
- **THEN** the numbers keep their values and accept an edit

### Requirement: More than one pump
The Pump step SHALL have the option "More than one pump" on `pump_flags` bit 1. With the option on, the wizard SHALL count twice the maximum pressure of the pump and SHALL show a warning under the option.

#### Scenario: Option turned on
- **WHEN** the user turns the option on for a pump of 375 mbar with the recommended values selected
- **THEN** the write sets bit 1 of `pump_flags` and the recommended minimum is 430 mbar

### Requirement: Warning for a pump above 430 mbar
When the maximum pressure of the pump, doubled with more than one pump, is above 430 mbar, the Pump, Pressure and Mode steps SHALL show a warning that the Leakshield cannot fully protect the Loop at full pump speed. The Summary SHALL show it while a change of the pump is pending. The warning SHALL NOT disable "Apply".

#### Scenario: Pump of 431 mbar
- **WHEN** the maximum pressure of the pump is 431 mbar
- **THEN** the Pressure step and the Mode step show the warning and "Apply" stays enabled

#### Scenario: Pump of 430 mbar
- **WHEN** the maximum pressure of the pump is 430 mbar with one pump
- **THEN** no step shows the warning

### Requirement: Speed and pressure helper
The Pump step SHALL show, read-only, the pressure of the pump at the current speed of the Pump channel of the Loop and the speed at which the pump makes 450 mbar. The pressure of the pump SHALL be the maximum pressure times the square of speed over maximum speed.

#### Scenario: D5 at 2840 rpm
- **WHEN** the pump has 4800 rpm and 375 mbar and the Pump channel runs at 2840 rpm
- **THEN** the helper shows about 131 mbar

### Requirement: Loop step with the height difference
The wizard SHALL have a step "Loop" after the Pressure step with the height difference of the Loop, `loop_height`, in whole cm from 0 to 200. The wizard SHALL correct a value outside this range. Above 100 cm the step SHALL warn that the recommended pressure values can be too low. The warning SHALL NOT disable "Apply".

#### Scenario: Height of 101 cm
- **WHEN** the user sets the height difference to 101 cm
- **THEN** the step shows the warning and "Apply" stays enabled

#### Scenario: Height out of range
- **WHEN** the user sets the height difference to 250 cm
- **THEN** the pending change holds 200 cm

### Requirement: Ranges of the Standby step
The wizard SHALL correct the assumed pump speed to 1000 to 10000 rpm and the assumed flow to 30 to 500 l/h. It SHALL keep the standby minimum pressure in 30 mbar to the minimum pressure of the control band. The standby minimum pressure SHALL accept an edit only while the option for a lower vacuum in standby, `shield_flags` bit 5, is on.

#### Scenario: Speed out of range
- **WHEN** the user sets the assumed pump speed to 500 rpm
- **THEN** the pending change holds 1000 rpm and the wizard shows no error

#### Scenario: Standby minimum above the minimum
- **WHEN** the minimum pressure is 250 mbar and the user sets the standby minimum to 400 mbar
- **THEN** the pending change holds 250 mbar

#### Scenario: Minimum lowered under the standby minimum
- **WHEN** the standby minimum is 250 mbar and the user sets the minimum pressure to 200 mbar
- **THEN** the standby minimum is 200 mbar

#### Scenario: Option off
- **WHEN** bit 5 of `shield_flags` is clear
- **THEN** the standby minimum takes no edit and its caption names the option to turn on

### Requirement: SHIELD pressure values are a control band
The wizard SHALL present the minimum, the target and the maximum pressure of SHIELD as a control band and not as alarm limits. Each value SHALL have a caption with what the Leakshield does there: it starts its pump below the minimum, it builds the vacuum up to the target, and it opens its vent valve above the maximum.

#### Scenario: Captions
- **WHEN** the user opens the Pressure step
- **THEN** each of the three values has a caption with the action of the Leakshield at that value

### Requirement: Recommended values or own values
The user SHALL choose between the recommended pressure values and own values. The choice SHALL be `shield_flags` bit 4: set for the recommended values, clear for own values. The three values SHALL be read-only with the recommended values.

#### Scenario: Own values
- **WHEN** the user selects own values in the wizard and applies the change
- **THEN** the write clears bit 4 of `shield_flags` and the three values accept an edit

#### Scenario: Recommended values
- **WHEN** bit 4 of `shield_flags` is set
- **THEN** the three values are read-only and the wizard says that they are the recommended values

### Requirement: Recommended pressure values
The recommended values SHALL come from the maximum pressure of the pump, doubled with more than one pump: the minimum is that pressure cut to 430 mbar, the target is 50 mbar above that pressure, the maximum is 40 mbar above the target, and the standby minimum is that pressure and at most 80 mbar. The limits of the pressure values SHALL apply to them.

#### Scenario: D5 preset
- **WHEN** the maximum pressure of the pump is 375 mbar with one pump
- **THEN** the recommended values are minimum 375, target 425, maximum 465 and standby minimum 80 mbar

#### Scenario: Two pumps
- **WHEN** the maximum pressure of the pump is 375 mbar with more than one pump
- **THEN** the recommended values are minimum 430, target 450 and maximum 490 mbar

### Requirement: Recommended values follow the pump
The wizard SHALL put the recommended values into the pending changes when the user selects the recommended values, and with each change of the pump type, of the maximum pressure of the pump or of the option for more than one pump while the recommended values are selected.

#### Scenario: Pump pressure changed with the recommended values
- **WHEN** the recommended values are selected and the user sets the maximum pressure of the pump to 400 mbar
- **THEN** the pending changes hold the minimum 400 mbar, the target 450 mbar and the maximum 490 mbar

#### Scenario: Pump pressure changed with own values
- **WHEN** own values are selected and the user changes the maximum pressure of the pump
- **THEN** the pressure values do not change

### Requirement: Button for the recommended values
With own values, a button SHALL put the recommended values into the pending changes and keep the choice. When the recommended values are selected and the Leakshield holds other values, the wizard SHALL say so and offer the same button.

#### Scenario: Button with own values
- **WHEN** own values are selected and the user selects the button for the recommended values
- **THEN** the pending changes hold the recommended values and bit 4 of `shield_flags` stays clear

#### Scenario: Leakshield holds other values
- **WHEN** bit 4 of `shield_flags` is set and the Leakshield holds 250, 300 and 350 mbar for a pump of 375 mbar
- **THEN** the wizard shows the three values that the Leakshield holds, a line with the recommended values and the button

### Requirement: Adaptive pressure is its own switch
Adaptive pressure SHALL be a switch on `shield_flags` bit 3. It SHALL NOT lock or dim the three pressure values, because the Leakshield keeps the stored distance between minimum and target and the stored maximum while it lowers the minimum and the target with the pump speed.

#### Scenario: Adaptive pressure on
- **WHEN** adaptive pressure is on
- **THEN** the three values keep their look and stay editable with own values

### Requirement: Limits of the pressure values
The wizard SHALL keep the minimum in 30 to 430 mbar, the target in 50 to 450 mbar and the maximum in 80 to 500 mbar, the target at least 20 mbar above the minimum and the maximum at least 30 mbar above the target. An edit SHALL be corrected to these limits and SHALL move the neighbor values that are too close. The wizard SHALL show no error for a corrected value.

#### Scenario: Minimum raised
- **WHEN** the values are 250, 300 and 350 mbar and the user sets the minimum to 310 mbar
- **THEN** the values are 310, 330 and 360 mbar

#### Scenario: Maximum lowered
- **WHEN** the values are 250, 300 and 350 mbar and the user sets the maximum to 290 mbar
- **THEN** the values are 240, 260 and 290 mbar

#### Scenario: Value out of range
- **WHEN** the user sets the minimum to 999 mbar
- **THEN** the values are 430, 450 and 480 mbar and the wizard shows no error

#### Scenario: Value at a limit
- **WHEN** the target is 20 mbar above the minimum
- **THEN** an information line under the target says that it is as close to the minimum as the Leakshield accepts

### Requirement: Stored fields of the pressure values
The wizard SHALL store the minimum in `shield_pressure_low`, target minus minimum in `shield_pressure_working_range` and maximum minus target in `shield_pressure_max_range`.

#### Scenario: Target edited
- **WHEN** the values are 250, 300 and 350 mbar and the user sets the target to 320 mbar
- **THEN** the pending changes hold a working range of 70 mbar and a maximum range of 30 mbar, and no change of the minimum

### Requirement: Band diagram true to scale
The Pressure step SHALL show a band diagram on one linear mbar axis. Its solid lines and its zones SHALL be the band that the Leakshield uses now: the lower limit, the setpoint and the upper limit that the Leakshield reports. Each line SHALL carry the live value, and the stored value under it when the two differ. The marker of the live pressure SHALL be at its place on the axis.

#### Scenario: Adaptive pressure lowers the limits
- **WHEN** the stored values are 375, 425 and 465 mbar, the Leakshield reports the limits 172.8, 222.8 and 465 mbar and the pressure is 389 mbar
- **THEN** the solid lines are at 465, 222.8 and 172.8 mbar, the target and the minimum show the stored 425 and 375 mbar under their values, and the marker is between the target line and the maximum line

### Requirement: Draft band before Apply
When the values of the wizard differ from the values that the Leakshield holds, the diagram SHALL draw them as three dashed lines with their new values. It SHALL draw no dashed line otherwise.

#### Scenario: Edited maximum
- **WHEN** the Leakshield holds 375, 425 and 465 mbar and the user sets the maximum to 490 mbar
- **THEN** the diagram has dashed lines at 490, 425 and 375 mbar and the solid lines stay

#### Scenario: No change
- **WHEN** the pending changes hold no pressure value
- **THEN** the diagram has no dashed line

### Requirement: Diagram without a live band
While the Leakshield does not answer, is in RELEASE or in MONITOR, runs an Action, or reports limits that are not in order, the diagram SHALL draw the stored band alone, without a marker, and a caption SHALL say that the Leakshield is not regulating in SHIELD now.

#### Scenario: RELEASE
- **WHEN** the Mode is RELEASE
- **THEN** the solid lines are the stored values, the diagram has no marker and the caption says that the Leakshield is not regulating in SHIELD now

#### Scenario: Action
- **WHEN** an Action runs and the three limits read 300 mbar
- **THEN** the solid lines are the stored values and the diagram has no marker

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
nicecooling SHALL NOT write the Leakshield settings at boot or when they drift from the active Profile (the device clock is not a setting: see Device clock). It SHALL write them only when the user changes a setting or activates a Profile.

#### Scenario: Daemon starts
- **WHEN** the daemon starts and the device settings differ from the active Profile
- **THEN** the daemon writes no setting and no report 3 to the Leakshield

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
The user SHALL be able to read the clock of the Leakshield and set it from the PC clock at any offset. The daemon SHALL set the device clock to the PC time by itself, at the attach and every 30 minutes, and when the clocks differ by more than 60 seconds. These sets and the alarm suppression at sleep are its only Leakshield writes without a user action or a Profile activation.

#### Scenario: Set the clock
- **WHEN** the user sets the clock to the PC time
- **THEN** the device clock reads the PC time within one second

#### Scenario: Set the clock at a small offset
- **WHEN** the device time differs from the PC time by 5 seconds and the user sets the clock to the PC time
- **THEN** the device clock reads the PC time within one second

#### Scenario: Clock lost after a power failure
- **WHEN** the device time differs from the PC time by 3 hours
- **THEN** the daemon sets the device clock to the PC time

#### Scenario: Attach
- **WHEN** the daemon attaches a Leakshield, also after a replug
- **THEN** it sets the device clock to the PC time at once

#### Scenario: Period
- **WHEN** 30 minutes pass since the last clock set
- **THEN** the daemon sets the device clock to the PC time

#### Scenario: Small drift
- **WHEN** the device time differs from the PC time by 20 seconds, less than 30 minutes after the last set
- **THEN** the daemon does not write the device clock

### Requirement: Display page preview from the device
The daemon SHALL give the image that the device shows for a display page, as the device draws it. It SHALL send `show_page` for the page, wait 200 ms, read report 5 and return the 128 x 64 image only when report 5 names the page. It SHALL fail when the device shows another page. It SHALL send nothing to a device that is read-only. The call SHALL need no authorization, because `show_page` stores no setting.

#### Scenario: Preview of a page
- **WHEN** the app asks for the preview of page 3
- **THEN** the daemon sends `show_page` with the page 3 for 30 s and returns the image of report 5

#### Scenario: Another page on the display
- **WHEN** the device shows page 7 after the daemon sent `show_page` for page 2
- **THEN** the call fails and returns no image

#### Scenario: Read-only device
- **WHEN** the firmware of the device is not in the registry
- **THEN** the call fails and the daemon sends no command

### Requirement: Display assets of report 8
The daemon SHALL read the 40 names, the 16 icons and the logo of report 8 and SHALL give them to every user. It SHALL write one name (at most 23 bytes of Windows-1252 text), one icon (23 x 23 pixels, black and white) or the logo (128 x 64 pixels, black and white) on request, in the write class. It SHALL refuse a read-only device.

#### Scenario: Name written
- **WHEN** the user writes the text "Pump" as name 21
- **THEN** report 8 on the device holds "Pump" as name 21 and the other names, the icons and the logo are as before

#### Scenario: Value does not fit
- **WHEN** the user writes a name of 24 bytes or an icon of 22 x 23 pixels
- **THEN** the call fails and the daemon sends nothing to the device

### Requirement: Write path of report 8
A write of a display asset SHALL run in this order: the checks of the value, a backup of reports 3, 5 and 8, a read of report 8, one write of the whole report with a new CRC that changes only the bytes of the asset, a delay of 250 ms and a read-back. The write SHALL fail with an error that names the backup when the read-back differs from the written report.

#### Scenario: Backup before the write
- **WHEN** the user writes a name
- **THEN** a backup that holds report 8 as it was before the write exists

#### Scenario: Read-back differs
- **WHEN** the device stores another byte than the written one
- **THEN** the write fails with the backup id
