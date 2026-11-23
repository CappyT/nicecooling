# Leakshield firmware update

## Purpose

Defines how nicecooling updates the firmware of the Leakshield on Linux: where the image comes from, which images the daemon accepts, the flash through the bootloader, the recovery from the bootloader state, the D-Bus API and the update panel of the app.

## ADDED Requirements

### Requirement: Image source
The daemon SHALL read a firmware image only from an aquasuite package database that the caller names. The database is the file `Content/aquasuite_db.database`, table `firmware`. nicecooling SHALL ship no firmware and SHALL keep no copy of an image.

#### Scenario: No image kept
- **WHEN** an update with the simulated device ends
- **THEN** no file in the state directory contains the first 64 bytes of the segment

#### Scenario: Database path missing
- **WHEN** the caller names a path that does not exist or is not a database with a `firmware` table
- **THEN** the daemon refuses the call with `InvalidArgument` and touches no device

### Requirement: Read-only database access
The daemon SHALL open the database read-only. It SHALL NOT change the file or create a file next to it.

#### Scenario: Database unchanged
- **WHEN** the daemon lists or reads images from a database in a read-only directory
- **THEN** the size, the modification time and the SHA-256 of the file stay the same

### Requirement: Image selection
The daemon SHALL list every Leakshield image of the database with its language (English or German), version, hardware, bootloader and a status. The status is `ok`, `downgrade` or one refusal reason. The user picks one image, and the update call names it.

#### Scenario: Both languages listed
- **WHEN** the database holds `leakshield_EN` and `leakshield_DE` for the hardware and the bootloader of the device
- **THEN** the daemon lists both images with their versions

#### Scenario: No matching row
- **WHEN** no row of the database has the hardware and the bootloader of the device
- **THEN** every listed image has a refusal reason and an update call for one of them is refused

#### Scenario: Unknown image
- **WHEN** an update call names an image that the database does not hold
- **THEN** the daemon returns `NotFound`

### Requirement: Accepted images
The daemon SHALL accept an image only when its hardware and its bootloader equal those of the device, and either its version differs from the installed firmware version or its language differs from the display language of the device (report 1 byte 34). A refused image SHALL carry one reason identifier.

#### Scenario: Same version, same language
- **WHEN** the image version equals the installed firmware version and the image language equals the display language of the device
- **THEN** the daemon refuses it with the reason `same-version`

#### Scenario: Same version, other language
- **WHEN** the device runs 1022 with the English display language and the image is the German 1022 image
- **THEN** the daemon accepts it with the status `ok`, as aquasuite does

#### Scenario: Other hardware
- **WHEN** the image hardware differs from the device hardware
- **THEN** the daemon refuses it with the reason `hardware-mismatch`

#### Scenario: Other bootloader
- **WHEN** the image bootloader differs from the device bootloader
- **THEN** the daemon refuses it with the reason `bootloader-mismatch`

#### Scenario: Newer version
- **WHEN** the image hardware and bootloader match and its version is above the installed version
- **THEN** the daemon accepts it with the status `ok`

### Requirement: Downgrade confirmation
An image with a version below the installed firmware version SHALL need an extra confirmation in the call. Without it, the daemon SHALL refuse the call and touch no device.

#### Scenario: Downgrade without confirmation
- **WHEN** the image version is below the installed version and the call has no downgrade confirmation
- **THEN** the daemon refuses the call with the reason `downgrade` and sends nothing to the device

#### Scenario: Downgrade with confirmation
- **WHEN** the call has the downgrade confirmation and every other check passes
- **THEN** the daemon starts the update

### Requirement: Image container check
The daemon SHALL parse the image container and refuse an image that has no segment, a segment that is not valid base64, a segment that starts below 0x4000 (the bootloader area), or overlapping segments. It SHALL send the segment bytes to the bootloader unchanged and SHALL NOT decrypt them.

#### Scenario: Broken container
- **WHEN** the selected row holds a document that is not a firmware container
- **THEN** the daemon refuses the call with `InvalidArgument` and sends nothing to the device

#### Scenario: Segment in the bootloader area
- **WHEN** a segment starts at 0x3C00
- **THEN** the daemon refuses the call with `InvalidArgument` and sends nothing to the device

#### Scenario: Bytes unchanged
- **WHEN** the simulated bootloader receives all chunks
- **THEN** the concatenated chunk data equals the segment of the image, padded with 0xFF to a multiple of 1024 bytes

### Requirement: Start conditions
The daemon SHALL start an update only when no other update runs, no Leakshield write is queued, no Action runs, and the device is connected. A start that does not meet them SHALL fail before any traffic to the device.

#### Scenario: Update already running
- **WHEN** a client starts an update while another one runs
- **THEN** the daemon returns `Busy`

#### Scenario: Action running
- **WHEN** an Action runs on the Leakshield and a client starts an update
- **THEN** the daemon returns `Busy` and the Action continues

#### Scenario: Device not connected
- **WHEN** the Leakshield is not connected
- **THEN** the daemon returns `Disconnected`

### Requirement: Backup before the flash
Before the first traffic to the bootloader, the daemon SHALL save a backup of reports 3, 5 and 8. It SHALL NOT start the flash when the backup fails.

#### Scenario: Backup exists
- **WHEN** an update starts on a device that is not in the bootloader
- **THEN** a backup of the current reports exists before the device restarts

#### Scenario: Backup fails
- **WHEN** the daemon cannot save the backup
- **THEN** the update fails before the device restarts and the finish signal names the reason

### Requirement: Enter the bootloader
The daemon SHALL send the restart command with the parameter that selects the bootloader. It SHALL wait for the device to appear in the bootloader and SHALL fail the update when the device does not appear within the wait time.

#### Scenario: Device enters the bootloader
- **WHEN** the update sends the restart command to a device in normal operation
- **THEN** the simulated device re-enumerates in the bootloader and the update goes on

#### Scenario: Bootloader not found
- **WHEN** no device appears in the bootloader within the wait time
- **THEN** the update fails, sends no chunk and reports the failure

### Requirement: Chunked transfer
The daemon SHALL cut each segment into chunks of 1024 bytes in address order, pad the last chunk with 0xFF, and send each chunk as one feature report 32 with the flash address, the length and a CRC-16/USB. It SHALL wait for each write to finish before the next one.

#### Scenario: Chunk count
- **WHEN** the segment has 109952 bytes at address 0x4000
- **THEN** the daemon sends 108 chunks with the addresses 0x4000 to 0x1EC00, and the last chunk ends with 640 bytes of 0xFF

#### Scenario: Frame check
- **WHEN** the simulated bootloader receives a chunk
- **THEN** its CRC equals the CRC-16/USB of the bytes between the report ID and the CRC

### Requirement: Transfer failure
The daemon SHALL stop the transfer at the first chunk that fails after its retries. It SHALL NOT report success. It SHALL NOT send the plain restart command after a failure, so that the device stays in the bootloader for a resume.

#### Scenario: Chunk write fails
- **WHEN** the simulated bootloader drops chunk 40 on every try
- **THEN** the transfer stops, the finish signal reports `transfer-failed`, and no later chunk goes out

#### Scenario: Device lost during the transfer
- **WHEN** the simulated device disconnects during the transfer
- **THEN** the update ends with `device-lost` and the daemon offers a resume when the device returns in the bootloader

### Requirement: Resume from the bootloader
When the daemon finds the device in the bootloader, it SHALL offer an update that starts at chunk 0. It SHALL NOT send the restart-into-bootloader command in that case.

#### Scenario: Device found in the bootloader
- **WHEN** a client starts an update and the device is in the bootloader
- **THEN** the daemon sends chunk 0 first and no restart command before it

#### Scenario: Second flash recovers the device
- **WHEN** a transfer failed halfway and the client starts the update again
- **THEN** the simulated device ends in normal operation with the new firmware

### Requirement: Unknown identity in the bootloader
The daemon SHALL check the hardware and the bootloader version of the device before it sends a chunk. When the bootloader report 1 does not hold them, the daemon SHALL use the values that it saved when it entered the bootloader. When it has neither, the call SHALL need an extra confirmation.

#### Scenario: Values from the earlier record
- **WHEN** the daemon entered the bootloader itself and the bootloader report 1 has no hardware field
- **THEN** it checks the image against the saved values

#### Scenario: No values known
- **WHEN** the device is in the bootloader after an update that another tool started, and report 1 has no hardware field
- **THEN** the daemon refuses the call with the reason `target-unknown` unless the call carries the extra confirmation

### Requirement: Leave the bootloader and verify
After the last chunk the daemon SHALL send the plain restart command, wait for the device to return in normal operation, and check that its firmware version equals the image version. A device that returns with another version SHALL give the result `version-mismatch`.

#### Scenario: Update succeeds
- **WHEN** the simulated device returns with the image version
- **THEN** the finish signal reports `ok`

#### Scenario: Device does not return
- **WHEN** no device returns in normal operation within the wait time
- **THEN** the finish signal reports `device-lost`

### Requirement: Settings read back after the flash
After a successful flash the daemon SHALL read reports 3, 5 and 8 and compare them with the backup outside the volatile mask. It SHALL report a difference and SHALL NOT write the backup back on its own.

#### Scenario: Settings kept
- **WHEN** the reports after the flash equal the backup
- **THEN** the finish signal reports no difference

#### Scenario: Settings changed
- **WHEN** report 3 after the flash differs from the backup outside the volatile mask
- **THEN** the finish signal lists the difference and the backup stays available for a restore

### Requirement: Read-only after the flash
After the flash, every Leakshield setting SHALL stay read-only until the field registry covers the new firmware version. The daemon SHALL show the read-only reason `unknown-firmware` and SHALL send no Feed message in that time.

#### Scenario: Registry does not cover the new version
- **WHEN** the device returns with a firmware version that the registry does not cover
- **THEN** `Writable` is false, the reason is `unknown-firmware`, and `SetFields` and a Profile activation write nothing

#### Scenario: Registry covers the new version
- **WHEN** the registry covers the new firmware version
- **THEN** the device is writable with the layout of that version

### Requirement: Other operations during the update
While an update runs, the daemon SHALL refuse every other Leakshield write and Action, SHALL send no Feed message, and SHALL NOT report an external change.

#### Scenario: Write during the update
- **WHEN** a client calls `SetFields` while an update runs
- **THEN** the daemon returns `Busy`

#### Scenario: No external change
- **WHEN** report 3 reads differently after the device left the bootloader
- **THEN** the daemon raises no external change for the update itself

### Requirement: Unreachable Alarm during the update
While an update of a Leakshield runs, the daemon SHALL NOT raise the Alarm `leakshield-unreachable` for that Leakshield, for at most 3 minutes from the start of the update. The other Alarms SHALL stay active.

#### Scenario: Flash longer than 10 s
- **WHEN** the device stays in the bootloader for 40 s during an update and the response of `leakshield-unreachable` is poweroff
- **THEN** the Alarm does not rise and the PC stays on

#### Scenario: Update beyond the limit
- **WHEN** an update still runs 3 minutes after its start and report 1 is still missing
- **THEN** the Alarm `leakshield-unreachable` rises with its alarm response

#### Scenario: Pump stop during the update
- **WHEN** the Pump channel stops during an update
- **THEN** the pump-stop Alarm rises as usual

### Requirement: Progress and result signals
The daemon SHALL emit a progress signal for each step and for each chunk, and one finish signal with a result identifier. A signal SHALL carry identifiers and numbers, and no text.

#### Scenario: Progress during the transfer
- **WHEN** the transfer sends 108 chunks
- **THEN** a client sees progress signals whose done count rises from 0 to 108 and one finish signal

#### Scenario: Result identifiers
- **WHEN** an update ends
- **THEN** the finish signal carries one of `ok`, `transfer-failed`, `device-lost`, `version-mismatch`, `bootloader-not-found` or `backup-failed`

### Requirement: Update method authorization
The method that starts an update SHALL use the polkit action `io.github.cappyt.NiceCooling1.critical`. The method that lists images and their checks SHALL use the polkit action `io.github.cappyt.NiceCooling1.write`, because root opens a file that the caller names, and it SHALL NOT start an update.

#### Scenario: Polkit asks the admin password
- **WHEN** a wheel member in the active local session starts an update
- **THEN** polkit asks for the admin password

#### Scenario: Denied
- **WHEN** polkit denies the caller
- **THEN** the daemon returns `org.freedesktop.DBus.Error.AccessDenied` and sends nothing to the device

### Requirement: Contract of the update API
The update method, the list method, the signals, the property and the errors SHALL be in `crates/nicecooling-api/dbus/io.github.cappyt.NiceCooling1.xml` and in `docs/api.md`. The introspection of the daemon SHALL equal the XML.

#### Scenario: Introspection equals the contract
- **WHEN** a test normalizes the introspection of the test daemon
- **THEN** it equals the XML, and the update method has the annotation `critical`

### Requirement: Update panel
The Leakshield system screen SHALL have an update panel. It SHALL show the installed version and the state of the device, the image selection with the language, the checks and their refusal reasons, the warnings, the progress and the result.

#### Scenario: Panel states
- **WHEN** the device is in normal operation, in the bootloader, or offline
- **THEN** the panel shows the matching state and offers only the actions that the state allows

#### Scenario: Refused image
- **WHEN** the selected image has a refusal reason
- **THEN** the panel shows the reason and disables the button that starts the update

### Requirement: Warnings before the start
Before the start, the panel SHALL state that the leak protection is off while the device is in its bootloader, and that the user must not unplug the device or power off the PC. A downgrade SHALL show a second confirmation. The user SHALL confirm before the app calls the method.

#### Scenario: Confirmation needed
- **WHEN** the user presses the start button
- **THEN** the app shows the warning and calls the method only after the user confirms

#### Scenario: Downgrade confirmation
- **WHEN** the selected image is a downgrade
- **THEN** the panel asks for a separate confirmation and sends the downgrade confirmation with the call

### Requirement: Resume offer in the panel
When the device is in the bootloader, the panel SHALL say so and SHALL offer the resume with the same image selection.

#### Scenario: Device in the bootloader
- **WHEN** the daemon reports a device in the bootloader and no update runs
- **THEN** the panel shows the resume button and the text that the leak protection is off
