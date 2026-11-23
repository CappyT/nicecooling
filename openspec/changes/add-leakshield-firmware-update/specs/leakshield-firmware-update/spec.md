# Leakshield firmware update

## Purpose

Defines how nicecooling updates the firmware of the Leakshield on Linux: where the image comes from, which images the daemon accepts, the flash through the bootloader, the recovery from the bootloader state, the D-Bus API and the update panel of the app.

## ADDED Requirements

### Requirement: Image source
The daemon SHALL read a firmware image only from an aquasuite package database that the caller passes as a file descriptor. The database is the file `Content/aquasuite_db.database`, table `firmware`. nicecooling SHALL ship no firmware and SHALL keep no copy of an image.

#### Scenario: No image kept
- **WHEN** an update with the simulated device ends
- **THEN** no file in the state directory contains the first 64 bytes of the segment

#### Scenario: Not a package database
- **WHEN** the descriptor is not a regular file, is above 16 MiB, or holds no `firmware` table
- **THEN** the daemon refuses the call with `InvalidArgument` and touches no device

### Requirement: Read-only database access
The daemon SHALL read the database only through the descriptor, into memory, with a time limit, and open it read-only with `trusted_schema` off and the defensive mode of SQLite. It SHALL parse one database at a time. It SHALL NOT open a path, change the file or create a file next to it.

#### Scenario: Database unchanged
- **WHEN** the daemon lists or reads images from a database
- **THEN** the size, the modification time and the SHA-256 of the file stay the same, and no other file appears in its directory

#### Scenario: Prepared schema
- **WHEN** `firmware` in the database is a view and not a table
- **THEN** the daemon refuses the call with `InvalidArgument`

#### Scenario: Second parse
- **WHEN** a client lists the images while another database parse runs
- **THEN** the daemon returns `Busy`

#### Scenario: Read blocks
- **WHEN** the read of the descriptor takes more than 10 s
- **THEN** the daemon refuses the call with `InvalidArgument` and touches no device

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
The daemon SHALL accept an image only when its hardware and its bootloader equal those of the device (see Unknown identity in the bootloader for an absent field), and either its version differs from the installed firmware version or its language differs from the display language of the device (report 1 byte 34). On a resume the version and language rule does not apply. A refused image SHALL carry one reason identifier.

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
An image with a version below the installed firmware version SHALL need an extra confirmation in the call. On a resume, the firmware version before the interrupted update, from the update record, counts as the installed version. Without the confirmation, the daemon SHALL refuse the call and touch no device.

#### Scenario: Downgrade without confirmation
- **WHEN** the image version is below the installed version and the call has no downgrade confirmation
- **THEN** the daemon refuses the call with the reason `downgrade` and sends nothing to the device

#### Scenario: Resume with a lower version
- **WHEN** the device is in the bootloader, the record holds firmware 1022 before the update, and the image version is 1020
- **THEN** the call needs the downgrade confirmation

#### Scenario: Resume with the earlier version
- **WHEN** the device is in the bootloader and the image version equals the firmware of the record before the update
- **THEN** the daemon accepts the image

#### Scenario: Downgrade with confirmation
- **WHEN** the call has the downgrade confirmation and every other check passes
- **THEN** the daemon starts the update

### Requirement: Image container check
The daemon SHALL parse the image container and refuse an image that has no segment, a segment that is not valid base64, a segment that starts below 0x4000 (the bootloader area), overlapping segments, or more than 1 MiB of data. It SHALL send the segment bytes to the bootloader unchanged and SHALL NOT decrypt them.

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
The daemon SHALL start an update only when no other update runs, no Action runs (by the mode of report 1), `leakshield-unreachable` is not active for the device, and the device is connected. The update SHALL take the lease of the device: a pending save-settings command goes out first, the device clears its settings flag, and a waiting Profile write is dropped. A start that fails these conditions SHALL fail before the restart command.

#### Scenario: Update already running
- **WHEN** a client starts an update while another one runs
- **THEN** the daemon returns `Busy`

#### Scenario: Action running
- **WHEN** an Action runs on the Leakshield and a client starts an update
- **THEN** the daemon returns `Busy` and the Action continues

#### Scenario: Device not connected
- **WHEN** the Leakshield is not connected
- **THEN** the daemon returns `Disconnected`

#### Scenario: Pending save-settings
- **WHEN** a client starts an update 1 s after a `SetFields`
- **THEN** the save-settings command goes out before the restart command

#### Scenario: Waiting Profile write
- **WHEN** a Profile write waits for its spacing and a client starts an update
- **THEN** the Profile write does not go out and its activation reports `leakshield-failed`

#### Scenario: Settings flag stays set
- **WHEN** report 1 byte 49 (`settings_dirty`) is still set 5 s after the save-settings command
- **THEN** the update fails with `backup-failed` and the device does not restart

#### Scenario: Profile request during the update
- **WHEN** a Profile activation for the Leakshield starts while the update runs
- **THEN** it reports `leakshield-failed` at once and writes nothing after the update

### Requirement: Backup before the flash
Before the first traffic to the bootloader, the daemon SHALL save a backup of reports 3, 5 and 8. It SHALL NOT start the flash when the backup fails. A resume from the bootloader SHALL reuse the backup of the update record.

#### Scenario: Backup exists
- **WHEN** an update starts on a device that is not in the bootloader
- **THEN** a backup of the current reports exists before the device restarts

#### Scenario: Backup fails
- **WHEN** the daemon cannot save the backup
- **THEN** the update fails before the device restarts and the finish signal names the reason

#### Scenario: Resume uses the earlier backup
- **WHEN** a resume starts on a device in the bootloader with an update record
- **THEN** the daemon takes no new backup and the read-back compares with the backup of the record

### Requirement: Enter the bootloader
The daemon SHALL send the restart command with the parameter that selects the bootloader. It SHALL wait for the device on HID interface 1 with a report 1 firmware from 1 to 99, and SHALL fail the update when the device does not appear within the wait time.

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
A chunk SHALL fail when its write returns an error until 300 ms after the first try, or when one write returns after more than 300 ms. The daemon SHALL stop the transfer at the first failed chunk and SHALL NOT report success. It SHALL NOT send the plain restart command after a failure, so that the device stays in the bootloader for a resume.

#### Scenario: Chunk write fails
- **WHEN** the simulated bootloader fails chunk 40 on every try
- **THEN** the transfer stops, the finish signal reports `transfer-failed`, and no later chunk goes out

#### Scenario: Slow chunk write
- **WHEN** the write of chunk 40 returns after 400 ms without an error
- **THEN** the transfer stops with `transfer-failed` and chunk 40 is not sent again

#### Scenario: Device lost during the transfer
- **WHEN** the simulated device disconnects during the transfer
- **THEN** the update ends with `device-lost` and the daemon offers a resume when the device returns in the bootloader

### Requirement: Resume from the bootloader
The daemon SHALL test each Leakshield at start and on each hot-plug event by its report 1. When it finds the device in the bootloader, it SHALL offer an update that starts at chunk 0, and SHALL NOT send the restart-into-bootloader command in that case. A resume SHALL write the update record before the first chunk.

#### Scenario: Device found in the bootloader
- **WHEN** a client starts an update and the device is in the bootloader
- **THEN** the daemon sends chunk 0 first and no restart command before it

#### Scenario: Daemon starts with the device in the bootloader
- **WHEN** the daemon starts while the device waits in the bootloader
- **THEN** the Leakshield object exists with `UpdateState` `interrupted`

#### Scenario: Record with the device in normal operation
- **WHEN** the daemon starts with an update record and the device in normal operation
- **THEN** it deletes the record and `UpdateState` is `idle`

#### Scenario: Second flash recovers the device
- **WHEN** a transfer failed halfway and the client starts the update again
- **THEN** the simulated device ends in normal operation with the new firmware

### Requirement: Unknown identity in the bootloader
The daemon SHALL check the hardware and the bootloader version of the device before it sends a chunk. In the bootloader it SHALL use the update record first. A nonzero field of the bootloader report 1 that differs from the record SHALL refuse the image. When a field has neither source, the call SHALL need an extra confirmation, and a field that is known SHALL still be checked.

#### Scenario: Values from the earlier record
- **WHEN** the daemon entered the bootloader itself and the bootloader report 1 has no hardware field
- **THEN** it checks the image against the values of the record

#### Scenario: Record and report differ
- **WHEN** the record holds hardware 1000 and the bootloader report 1 holds another nonzero hardware
- **THEN** the daemon refuses the image with the reason `target-conflict`, also with the extra confirmation

#### Scenario: No values known
- **WHEN** the device is in the bootloader after an update that another tool started, and report 1 has no hardware field
- **THEN** the daemon refuses the call with the reason `target-unknown` unless the call carries the extra confirmation

#### Scenario: Partial identity
- **WHEN** no record exists, the bootloader report 1 holds hardware 1000 and no bootloader field, the image hardware is 2000, and the call carries the extra confirmation
- **THEN** the daemon refuses the image with the reason `hardware-mismatch`

### Requirement: Leave the bootloader and verify
After the last chunk the daemon SHALL send the plain restart command and wait for the device in normal operation. It SHALL check that the firmware version equals the image version, from the identity fields of a report 1 of any length. It SHALL check that the state is plausible when the registry covers the new version: a listed mode and no system alarm bit.

#### Scenario: Update succeeds
- **WHEN** the simulated device returns with the image version
- **THEN** the finish signal reports `ok`

#### Scenario: Other version
- **WHEN** the simulated device returns with a version other than the image version
- **THEN** the finish signal reports `version-mismatch`

#### Scenario: Implausible state
- **WHEN** the simulated device returns with the image version and a system alarm bit set
- **THEN** the finish signal reports `implausible-state`

#### Scenario: Report 1 of another size
- **WHEN** the simulated device returns with the image version and a report 1 of 900 bytes
- **THEN** the finish signal reports `ok` and `leakshield-unreachable` does not rise

#### Scenario: Device does not return
- **WHEN** no device returns in normal operation within the wait time
- **THEN** the finish signal reports `device-lost`, and the update record stays until the device returns

### Requirement: Settings read back after the flash
After a successful flash the daemon SHALL read reports 3 and 8 and compare them with the backup outside the volatile mask. It SHALL report a difference, or a report that it cannot read, without a change of the result. It SHALL NOT write the backup back on its own.

#### Scenario: Settings kept
- **WHEN** the reports after the flash equal the backup
- **THEN** the finish signal reports no difference

#### Scenario: Settings changed
- **WHEN** report 3 after the flash differs from the backup outside the volatile mask
- **THEN** the finish signal reports `ok`, lists the difference, and the backup stays available for a restore

#### Scenario: Display image ignored
- **WHEN** report 5 after the flash differs from the backup and reports 3 and 8 do not
- **THEN** the finish signal reports no difference

### Requirement: Read-only after the flash
After the flash, every Leakshield setting SHALL stay read-only until the field registry covers the new firmware version. The daemon SHALL show the read-only reason `unknown-firmware` and SHALL send no Feed message in that time.

#### Scenario: Registry does not cover the new version
- **WHEN** the device returns with a firmware version that the registry does not cover
- **THEN** `Writable` is false, the reason is `unknown-firmware`, and `SetFields` and a Profile activation write nothing

#### Scenario: Registry covers the new version
- **WHEN** the registry covers the new firmware version
- **THEN** the device is writable with the layout of that version

### Requirement: Other operations during the update
While an update runs or is interrupted, no user other than the update SHALL reach the device. The daemon SHALL refuse every Leakshield method that reaches the device with `Busy`, SHALL send no Feed message, SHALL stop the report 1 poll, and SHALL NOT report an external change. The update methods and the backup list stay available.

#### Scenario: Write during the update
- **WHEN** a client calls `SetFields` while an update runs
- **THEN** the daemon returns `Busy`

#### Scenario: Read during the update
- **WHEN** a client calls `GetFields` or `CreateBackup` while an update runs
- **THEN** the daemon returns `Busy` and sends no feature read to the device

#### Scenario: Backup list while interrupted
- **WHEN** the update is interrupted and a client calls `ListBackups`
- **THEN** the daemon returns the list

#### Scenario: Mode change during the update
- **WHEN** a client calls `SetMode` while an update runs
- **THEN** the daemon returns `Busy` and sends no report 2 command

#### Scenario: No external change
- **WHEN** report 3 reads differently after the device left the bootloader
- **THEN** the daemon raises no external change for the update itself

### Requirement: Unreachable Alarm during the update
While an update of a Leakshield runs, the daemon SHALL NOT raise the Alarm `leakshield-unreachable` for that Leakshield. While the update is interrupted (no job runs, and an update record exists or the device is in the bootloader), the Alarm SHALL rise with the response `notify`, whatever response the user set. Its count SHALL restart when the update stops running. The other Alarms SHALL stay active.

#### Scenario: Flash longer than 10 s
- **WHEN** the device stays in the bootloader for 40 s during an update and the response of `leakshield-unreachable` is poweroff
- **THEN** the Alarm does not rise and the PC stays on

#### Scenario: Device left in the bootloader
- **WHEN** a transfer failed, the device waits in the bootloader, and the response of `leakshield-unreachable` is poweroff
- **THEN** the Alarm rises with the response `notify` and the PC stays on

#### Scenario: Device lost after the job
- **WHEN** the update ends with `device-lost`, the device stays absent, and the response of `leakshield-unreachable` is poweroff
- **THEN** the Alarm rises with the response `notify` and the PC stays on

#### Scenario: Good flash with poweroff
- **WHEN** the update ends with `ok` and the response of `leakshield-unreachable` is poweroff with delay 0
- **THEN** the Alarm does not rise

#### Scenario: Pump stop during the update
- **WHEN** the Pump channel stops during an update
- **THEN** the pump-stop Alarm rises as usual

### Requirement: Shutdown and sleep during the update
While an update runs, the daemon SHALL hold a logind inhibitor lock that blocks shutdown and sleep. It SHALL release the lock when the update ends. The lock SHALL NOT delay an Alarm poweroff of the daemon.

#### Scenario: Lock held
- **WHEN** the transfer runs
- **THEN** the test logind holds a `block` inhibitor for `shutdown:sleep` from the daemon

#### Scenario: Lock released
- **WHEN** the finish signal goes out
- **THEN** the daemon holds no inhibitor lock

#### Scenario: Alarm poweroff during the transfer
- **WHEN** the pump-stop Alarm with the response poweroff fires during the transfer
- **THEN** the daemon calls the poweroff without waiting for the update, and the update record stays

### Requirement: Daemon stop during the update
When the daemon stops while an update runs, it SHALL stop the update at once, send no further chunk and keep the update record. At the next start it SHALL find the device and offer the resume.

#### Scenario: Stop during the transfer
- **WHEN** the test daemon stops after chunk 40
- **THEN** no later chunk goes out and the update record stays

#### Scenario: Start after the stop
- **WHEN** the test daemon starts again with the simulated device in the bootloader
- **THEN** the Leakshield object has `UpdateState` `interrupted`

### Requirement: Progress and result signals
The daemon SHALL emit a progress signal for each step and for each chunk, and one finish signal with a result identifier. A signal SHALL carry identifiers and numbers, and no text.

#### Scenario: Progress during the transfer
- **WHEN** the transfer sends 108 chunks
- **THEN** a client sees progress signals whose done count rises from 0 to 108 and one finish signal

#### Scenario: Result identifiers
- **WHEN** an update ends
- **THEN** the finish signal carries one of `ok`, `transfer-failed`, `device-lost`, `version-mismatch`, `implausible-state`, `bootloader-not-found` or `backup-failed`

### Requirement: Update method authorization
The method that starts an update SHALL use the polkit action `io.github.cappyt.NiceCooling1.critical`. The method that lists images and their checks SHALL use the polkit action `io.github.cappyt.NiceCooling1.write`, and it SHALL NOT start an update.

#### Scenario: Polkit asks the admin password
- **WHEN** a wheel member in the active local session starts an update
- **THEN** polkit asks for the admin password

#### Scenario: List without a prompt
- **WHEN** a wheel member in the active local session lists the images
- **THEN** polkit allows the call without a prompt

#### Scenario: Denied
- **WHEN** polkit denies the caller
- **THEN** the daemon returns `org.freedesktop.DBus.Error.AccessDenied` and sends nothing to the device

### Requirement: Contract of the update API
The update method, the list method, the signals, the property and the errors SHALL be in `crates/nicecooling-api/dbus/io.github.cappyt.NiceCooling1.xml` and in `docs/api.md`. The introspection of the daemon SHALL equal the XML.

#### Scenario: Introspection equals the contract
- **WHEN** a test normalizes the introspection of the test daemon
- **THEN** it equals the XML, the update method has the annotation `critical`, and the list method has the annotation `write`

### Requirement: Update panel
The Leakshield system screen SHALL have an update panel. It SHALL show the installed version and the state of the device, the image selection with the language, the checks and their refusal reasons, the warnings, the progress and the result.

#### Scenario: Panel states
- **WHEN** the device is in normal operation, in the bootloader, or offline
- **THEN** the panel shows the matching state and offers only the actions that the state allows

#### Scenario: Refused image
- **WHEN** the selected image has a refusal reason
- **THEN** the panel shows the reason and disables the button that starts the update

### Requirement: Warnings before the start
Before the start, the panel SHALL state that the leak protection is off while the device is in its bootloader, that the user must not unplug the device or power off the PC, and that an Alarm poweroff stops the update. A downgrade and an unverified target SHALL each show a second confirmation. The user SHALL confirm before the app calls the method.

#### Scenario: Confirmation needed
- **WHEN** the user presses the start button
- **THEN** the app shows the warning and calls the method only after the user confirms

#### Scenario: Downgrade confirmation
- **WHEN** the selected image is a downgrade
- **THEN** the panel asks for a separate confirmation and sends the downgrade confirmation with the call

#### Scenario: Unverified target confirmation
- **WHEN** the selected image has the status `target-unknown`
- **THEN** the panel asks for a separate confirmation and sends the option `unverified_target` with the call

### Requirement: Resume offer in the panel
When the device is in the bootloader, the panel SHALL say so and SHALL offer the resume with the image selection.

#### Scenario: Device in the bootloader
- **WHEN** the daemon reports a device in the bootloader and no update runs
- **THEN** the panel shows the resume button and the text that the leak protection is off
