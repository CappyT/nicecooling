# Design

## Context

See `proposal.md` for the motivation and `specs/leakshield-firmware-update/spec.md` for the requirements. This change builds on `add-nicecooling-v1` and reads its contracts: `design.md` (D3, D6, D11, D12, D13, D17), `docs/api.md`, `crates/nicecooling-api/dbus/io.github.cappyt.NiceCooling1.xml` and `docs/storage.md`. Terms follow `GLOSSARY.md`.

The protocol comes from `docs/re/firmware-update.md` (aquasuite X.96). Each statement there has a confidence:

- **code**: the decompiled aquasuite code defines it. This covers the sequence, the two restart commands, the 1024-byte chunks, the report 32 layout, the CRC, the 50 ms and 300 ms timings, and the restart from chunk 0 when the device is already in the bootloader.
- **inferred**: the bootloader decrypts the image (the host has no key), an incomplete image leaves the device in the bootloader, and the settings survive a flash.
- **unknown**: the length of feature report 32 in the bootloader, the answer of the bootloader to each chunk, the time to enter and to leave the bootloader, the meaning of frame bytes 1-7, and the content of the bootloader report 1.

State of the code today:

- `sources/hotplug.rs` recognizes the bootloader (`bcdDevice` 2.00 and two interfaces). The daemon never opens such a device.
- The write worker, the Feed sender, the external change detector and the clock sync belong to one attached Leakshield (`crates/nicecoolingd/src/leakshield/`).
- `crates/nicecooling-protocol/src/sim.rs` simulates a Leakshield in normal operation. It has no bootloader.
- The package database has the columns `id`, `version`, `bootloader`, `hardware`, `device` and `data` (`docs/re/firmware.md`). The reference rows are `leakshield_EN` and `leakshield_DE`: firmware 1022, bootloader 100, hardware 1000. The reference device runs firmware 1022.

## Goals / Non-Goals

**Goals:**

- A user flashes a Leakshield from the app, with no VM, a backup before the flash and a settings check after it.
- A flash that stops halfway leaves a way back: a second flash from the bootloader recovers the device.
- Every automated test runs on a simulated bootloader.

**Non-Goals:**

- Decrypt, patch or build an image. The bootloader holds the key.
- Flash from a loose image file (aquasuite also takes a `.xml` file), and firmware of other devices.
- Add registry data for a new firmware version. That needs a device dump and a separate change.
- Cancel a running transfer. A stopped transfer is the same state as a failed one: the device waits in the bootloader.

## Decisions

### D1. Where the code lives

| Crate | Content |
|---|---|
| `nicecooling-protocol` | `src/firmware/image.rs` (container), `src/firmware/frame.rs` (chunks and report 32), `src/firmware/bootloader.rs` (bootloader state from report 1, the restart commands), `src/sim.rs` (simulated bootloader). No SQLite. |
| `nicecoolingd` | `src/firmware/package.rs` (database), `src/firmware/checks.rs` (acceptance), `src/firmware/flash.rs` (state machine), `src/firmware/record.rs` (update record), `src/dbus/leakshield.rs` (methods, signals). |
| `nicecooling-api` | The XML, the error name and the identifiers. |
| `nicecooling` | `src/screens/leakshield/system.rs` and its view: the update panel. |

Rejected: SQLite in the protocol crate. Every tool of the workspace would link it.

### D2. The daemon reads the database

The caller names a path. The daemon opens it with `rusqlite` read-only and with `immutable=1`, so SQLite creates no `-wal`, `-shm` or journal file next to the package file. First the daemon checks that the path is a regular file, because a FIFO or a device node would block it.

- The daemon runs as root and parses a file that a user chose. The start method is `critical` for this reason. The list method is class `write` for the same reason.
- `nicecoolingd` takes `rusqlite` as a direct dependency, at the version of the workspace.
- The list query reads `id`, `version`, `bootloader`, `hardware` and `device` of the rows whose `device` is `leakshield_EN` or `leakshield_DE`. The `data` column is read for the chosen row only.
- The image stays in memory during the update. The daemon writes no copy (`spec.md`, Image source). A resume after a daemon restart needs the path again.

Rejected: the app reads the database and sends the bytes over D-Bus. The owner decided that the daemon reads it.

### D3. Image container

The `data` of a row is a UTF-8 XML document with a byte order mark: `FirmwareContainer`, `firmware`, and `Segment` elements with `data` (base64) and `adr_start` (decimal). `image.rs` parses it with `roxmltree` and `base64`. Both crates are in `Cargo.lock` already as transitive dependencies. Task 2.1 pins their versions.

The parser refuses an image when:

- the document has no segment, or a segment has bad base64 or an `adr_start` that is not a number;
- a segment starts below 0x4000. That is the area of the bootloader. A segment must never overwrite it;
- two segments overlap, or the data of all segments is above 1 MiB.

The upper flash bound is unknown (OQ2). The check does not use one.

### D4. Acceptance checks

`checks.rs` is a pure function of the device identity, the image row and the options. The device identity is report 1 `hardware_revision` (offset 7), `bootloader_version` (offset 11) and `firmware_version` (offsets 13-14). The first failing check gives the status:

| Order | Check | Status |
|---|---|---|
| 1 | image hardware equals device hardware | else `hardware-mismatch` |
| 2 | image bootloader equals device bootloader | else `bootloader-mismatch` |
| 3 | image version differs from firmware, or image language differs from the display language (report 1 byte 34) | else `same-version` |
| 4 | image version above firmware | else `downgrade` (needs the option `downgrade`) |
| 5 | none of the above | `ok` |

For a device in the bootloader, the firmware field is the version of the bootloader (1 to 99). Checks 3 and 4 do not apply. Checks 1 and 2 use the identity of the update record (D8) or, when the bootloader report 1 has the fields, its own values. Without both sources the status is `target-unknown`, and the option `unverified_target` lifts it.

### D5. Chunks and the report 32 frame

`frame.rs` holds the constants and the builder.

| Item | Value | Source |
|---|---|---|
| Chunk size | 1024 bytes | code |
| Padding of the last chunk | 0xFF | code |
| Chunk address | `adr_start + n x 1024` | code |
| Frame, after the report ID | `00 00 00`, u32 `2`, u32 address, u32 `1024`, 1024 data bytes, CRC-16/USB u16 big-endian | code |
| CRC range | the bytes after the report ID and before the CRC | code |
| Buffer length | 1042 bytes with the report ID, 1041 after it | code |
| Buffer length in the bootloader | the HID descriptor decides: a longer one pads with zeros, a shorter one cuts | unknown, task 1.5 |

The builder takes the feature report length as a parameter. The default is 1042 until the capture shows the descriptor of the bootloader. The CRC function is the one of `report2.rs` (`crc.rs`).

### D6. The flash state machine

`flash.rs` runs one job in a blocking task, outside the tick (v1 D6). The steps and their first timings follow aquasuite (`docs/re/firmware-update.md`). Task 1.5 replaces a timing when the capture shows another value.

| Step id | Action | Timing |
|---|---|---|
| `backup` | Save the backup. Write the update record. Check the options. | |
| `enter-bootloader` | Delay 5 ms, send report 2 command 1 with parameter 1. Skip when the device is in the bootloader. | wait 4 s |
| `wait-bootloader` | Poll for the device in the bootloader. | every 250 ms, at most 8 s |
| `transfer` | For each chunk: write the frame, retry the same write on an error, delay 50 ms. | 300 ms per chunk |
| `leave-bootloader` | Delay 250 ms, send report 2 command 1 with parameter 0. | |
| `wait-device` | Wait for the device in normal operation with a firmware version of 100 or more. | at most 15 s (unknown, task 1.5) |
| `read-back` | Compare the version, read reports 3, 5 and 8, compare them with the backup. | |

- A chunk error is a failed `set_feature`. The bootloader gives no reply (code), so a write that returns without error counts as accepted.
- After a chunk that stays failed for 300 ms, the job stops, emits `transfer-failed` and does not send the leave command. aquasuite sends it. The device stays in the bootloader, ready for a second flash from nicecooling or from aquasuite in the VM: this is safer than a start of an incomplete image.
- `bootloader-not-found`, `device-lost` and `backup-failed` end the job at their step. After `bootloader-not-found` the device stays where it is.
- The result `version-mismatch` means the device returned in normal operation with a version other than the image version.

### D7. The quiesce of the attached Leakshield

While `UpdateState` is `running`, the attachment of the device does this:

- The write worker, `SetMode`, `StartAction`, `SyncClock`, the backup methods that write, and the reset methods return `Busy`. A Profile activation reports `leakshield-failed`.
- The Feed sender skips the Loop of the device. The clock sync and the external change detector are off.
- A hot-plug disconnect or connect with the USB path of the device keeps the object. The property `Connected` follows the device and the object stays on the bus.
- After a result `ok`, the detector drops its last known report 3 and takes the next read as the new value. The update raises no external change.

A flash takes more than 10 s without report 1, and a response `poweroff` on the Alarm `leakshield-unreachable` would power off the PC during the flash. So the alarm stage does not raise that Alarm for the Leakshield under update, for at most 3 minutes from the start. The other Alarms stay active.

### D8. The update record and the device in the bootloader

`/var/lib/nicecooling/leakshield/<leakshield id>/firmware-update.json` holds the serial, the USB path, the hardware, the bootloader version, the firmware before the update, the backup id and the start time. The daemon writes it atomically before the restart command and deletes it when the device returns in normal operation. It has no last-known-good copy, like `lease.json`. Task 3.3 adds it to `docs/storage.md`.

When the hot-plug event shows a device in the bootloader, the daemon opens its HID interface and reads the short report 1. It pads the report with zeros as aquasuite does. A hardware or bootloader field of 0 counts as absent. The object of the device uses the serial of the record. Without a record and without a serial in the report, the object id is open (OQ1).

The device in the bootloader reports `UpdateState` `bootloader`. The properties `Firmware`, `HardwareRevision` and `BootloaderVersion` show what the report holds, and the settings are read-only with the reason `unknown-firmware`.

### D9. After the flash

The device returns with the attach path of v1. The registry decides:

- Firmware covered by the registry: the device is writable with the layout of that version.
- Firmware not covered: `Writable` is false with `unknown-firmware`, the daemon sends no Feed message and raises the existing Warning `leakshield-unknown-firmware`. A Profile activation fails `leakshield-failed`.

The read-back compares reports 3, 5 and 8 with the backup. It uses the volatile mask of the firmware before the update, because the registry may not know the new one. The result lists the report names that differ. The daemon never writes the backup back on its own. The user restores it with `RestoreBackup`, which refuses a backup of another firmware version (`docs/api.md`).

The backup uses the reason `before-write`. A new reason would change the `ListBackups` contract, and the retention rule already covers it.

### D10. D-Bus contract additions

The task 5.1 writes these into the XML and `docs/api.md`. The owner reviews them before task 5.2.

| Item | Definition |
|---|---|
| Property `UpdateState` (`s`) | `idle`, `running` or `bootloader`. |
| Properties `HardwareRevision`, `BootloaderVersion` (`q`) | Report 1 offsets 7 and 11. 0 when absent. |
| Method `ListFirmwareImages(database s) -> a(xsqqqs)` | Image id, language `en` or `de`, version, hardware, bootloader, status. Class `write`: root opens a file that the caller names. |
| Method `UpdateFirmware(database s, image x, options a{sv})` | Class `critical`. Returns when the checks passed and the job started. Option keys: `downgrade` (`b`), `unverified_target` (`b`). |
| Signal `FirmwareUpdateProgress(step s, done u, total u)` | Step ids of D6. During `transfer`, `done` counts chunks and `total` is the chunk count. |
| Signal `FirmwareUpdateFinished(result s, details a{sv})` | Result ids of the spec. Details: `firmware` (`q`), `backup` (`s`), `differences` (`as`). |
| Error `ImageRefused` | The message holds the status identifier of D4. |

Other failures reuse `InvalidArgument`, `NotFound`, `Busy` and `Disconnected`. The method carries the annotation `io.github.cappyt.NiceCooling1.Authorization`, and the map test of v1 task 4.4 finds it.

### D11. The update panel

The panel is a new card of the Leakshield system screen. `docs/ui/screens/leakshield-system.md` gets the card, its states and its copy (task 6.1). The owner approves the text before task 6.2, as in v1 task 8.1.

- Flow: choose the database file, the app calls `ListFirmwareImages`, the user picks an image by language, the app shows the warnings and the confirmation, then calls `UpdateFirmware` with the `critical` prompt of polkit.
- Warnings: the leak protection is off while the device is in its bootloader, and the user must not unplug the device or power off the PC. A downgrade has a second confirmation.
- States: device in normal operation, device in the bootloader (resume), offline, daemon offline, running (a progress bar with the step and the chunk count), result (ok, each failure, the list of differences).
- The panel calls only the methods of D10. It reads no database and holds no image.

### D12. The simulated bootloader

`sim.rs` gets a mode: `Application`, `Bootloader` and `Rebooting`. A test builds it with a configuration: the feature report length, the time to enter and to leave, the behavior for each chunk (accept, drop, stall) and the image that it expects.

- Report 2 command 1 with parameter 1 in the application moves the device to `Bootloader` after the enter time. The sim then sends the same events as the real device: a disconnect and a connect with `bcdDevice` 2.00, two interfaces and a short report 1 with firmware 1.
- A feature write of report 32 with a wrong report length or CRC fails. A good frame is stored at its address.
- Command 1 with parameter 0 moves the device to `Application` with the new firmware version when the stored chunks equal the expected image. Otherwise the device stays in the bootloader (inferred behavior).
- Tests drive the clock of the daemon, so the 4 s and 8 s waits cost no time.

Hard rule: no test and no tool of this change opens the real device. Tasks 2.5 and 8.1 check it with `rg`. The tool `tools/nicecooling-verify` never calls the firmware module.

### D13. The manual checks

Two MANUAL tasks keep the real device safe.

- The capture (section 1). The owner runs aquasuite in the VM and records the host bus with `usbmon`. Run 1: aquasuite in German with the device in English flashes the DE image. Run 2: aquasuite in English restores the EN image. The summary tool reads the capture files. It never opens the device. The results settle the unknowns before the write path is final. The tasks that depend on them come after the gate 1.6.
- The first flash from nicecooling (section 7). The owner sits at the desk. aquasuite in the VM is the recovery plan: the procedure of `docs/hardware-checks/firmware-update.md` recovers a device in the bootloader.

## Risks / Trade-offs

- [The device stays in the bootloader and the leak protection is off] -> The warning before the start. The resume offer in the panel. aquasuite in the VM as recovery. The loop stays closed during the check.
- [A wrong image bricks the device] -> Hardware and bootloader checks, the 0x4000 lower bound, and no chunk before the checks pass. The bootloader decrypts, so a bad image fails there.
- [The unknown bootloader answers hide a failed chunk] -> The read-back after the restart is the proof: the device must return with the image version. The capture (section 1) may show a status per chunk.
- [The frame length differs in the bootloader] -> A parameter, a task after the capture, and a sim test for both lengths.
- [The daemon crashes during the transfer] -> The device waits in the bootloader. The next start finds it, and the panel offers the resume.
- [Root parses a file that a user chose] -> Read-only and `immutable=1` open, a regular-file check, class `critical` for the start. The list method is class `write`.
- [The user flashes an image for the wrong language] -> The panel shows the language of each image next to the display language of the device (report 1 byte 34). The user can flash again.
- [The kernel driver reads report 1 of the device in the bootloader] -> The driver only reads. The first flash check watches the kernel log.
- [The new firmware is not in the registry] -> The settings stay read-only and the Feed stops, with a Warning. The registry gets the version in a later change.

## Migration Plan

Nothing to migrate. The change adds a method, two signals, three properties and one error to the `Leakshield` interface, and one stored file that exists only during an update. Rollback: remove the RPM. A device in the bootloader after a rollback needs aquasuite in the VM.

## Open Questions

- OQ1. **Object id without serial.** If the bootloader report 1 has no serial and no record exists, the object has no id derived from the serial. A fixed id such as `leakshield_bootloader` is possible when one device is present. The capture shows whether the serial is there.
- OQ2. **Upper flash bound.** The image check has a lower bound only. A bound needs the flash size of the hardware, which no source gives.
