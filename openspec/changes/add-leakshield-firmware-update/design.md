# Design

## Context

The Leakshield is the leak protection unit of Aqua Computer (USB `0c70:f014`). Its firmware updates come only from aquasuite, the Windows software of the vendor. On the reference desktop aquasuite runs in the Windows VM `win11-aquasuite` with USB passthrough. Thus each update needs the VM. This change lets the nicecooling daemon flash the image of the aquasuite package, started from the app.

`proposal.md` gives the motivation and `specs/leakshield-firmware-update/spec.md` the requirements. The change builds on the change `add-nicecooling-v1`. It reads its contracts: `openspec/changes/add-nicecooling-v1/design.md`, `docs/api.md`, `crates/nicecooling-api/dbus/io.github.cappyt.NiceCooling1.xml` and `docs/storage.md`. A decision of that design is written with the prefix `v1-`, for example v1-D6. The decisions of this file have no prefix. Terms follow `GLOSSARY.md`.

"The owner" is the owner of the repository, who decides scope and risk. The owner decisions of this design are in [issue #70](https://github.com/CappyT/nicecooling/issues/70), in the comments of the spec review. The text marks them "owner decision".

The protocol comes from `docs/re/firmware-update.md` (aquasuite X.96). Each statement there has a confidence:

- **code**: the decompiled aquasuite code defines it. This covers the step sequence, the two restart commands, the 1024-byte chunks, the report 32 layout, the CRC, the waits of the steps, the 300 ms chunk limit, the bootloader test (report 1 firmware 1 to 99 on HID interface 1), and the start from chunk 0 for a device in the bootloader. aquasuite reads no answer from the bootloader.
- **inferred**: an incomplete image leaves the device in the bootloader, and the settings survive a flash.
- **unknown**: the feature report 32 length of the bootloader, whether the bootloader signals a bad chunk, the time to leave the bootloader, the meaning of frame bytes 1-7, the content of the bootloader report 1, and whether the bootloader checks the image.

State of the code today:

- `sources/hotplug.rs` marks a device as "in the bootloader" by `bcdDevice` 2.00 and two interfaces. `fleet.rs` skips such a device at start and on hot-plug. No code opens it.
- `UsbBus::open` (`sources/usb.rs`) opens the HID interface and the bulk interface. `sources/leakshield_hid.rs` finds the hidraw node on HID interface 1, as aquasuite does. `Report1::decode` takes only a report of 837 bytes.
- `DeviceIo` (`leakshield/mod.rs`) shares one `Link` between its clones: the worker, the Feed sender, the detector and the D-Bus methods. A swap of the link reaches all of them. The transport `Gone` fails every call.
- The write worker (`leakshield/worker.rs`) runs one job at a time as one blocking call. It also holds the save-settings deadline. A Profile request takes no permit and waits in the same queue.
- `alarms/logind.rs` powers off for an Alarm with `SD_LOGIND_SKIP_INHIBITORS`. `alarms/response.rs` resolves the response of an Alarm by its event name.
- `crates/nicecooling-protocol/src/sim.rs` simulates a Leakshield in normal operation. `crates/nicecoolingd/tests/support/sim_usb.rs` plugs and unplugs it.
- The package database `Content/aquasuite_db.database` (4603904 bytes) has the table `firmware` with the columns `id`, `version`, `bootloader`, `hardware`, `device` and `data` (`docs/re/firmware.md`). The rows `leakshield_EN` and `leakshield_DE` hold firmware 1022, bootloader 100 and hardware 1000. The report 1 fixture of the reference device (`tests/fixtures/leakshield/fw1022/report1.bin`) holds hardware 1000 at offset 7, bootloader 100 at offset 11 and firmware 1022 at offset 13.

## Goals / Non-Goals

**Goals:**

- A user flashes a Leakshield from the app, with a backup before the flash and a check after it. A normal flash needs no VM.
- A flash that stops halfway leaves a way back: a second flash from the bootloader recovers the device.
- Every automated test runs on a simulated bootloader.

**Non-Goals:**

- Decrypt, patch or build an image. The bootloader holds the key.
- Flash from a loose image file (aquasuite also takes a `.xml` file), and flash other devices.
- Add registry data for a new firmware version. That needs a device dump and a separate change.
- Cancel a running transfer. A stopped transfer is the same state as a failed one: the device waits in the bootloader.

## Decisions

### D1. Where the code lives

| Crate | Content |
|---|---|
| `nicecooling-protocol` | `src/firmware/image.rs` (container), `src/firmware/frame.rs` (chunks and report 32), `src/firmware/bootloader.rs` (the bootloader test and `Identity` on report 1), `src/device.rs` (`DeviceError::Busy`), `src/sim.rs` (the bootloader mode of the simulated device), `src/lib.rs`. No SQLite. |
| `nicecooling-core` | `src/tick/mod.rs` (`TickContext`), `src/tick/stage_alarms.rs`, `src/alarms/unreachable.rs` and `src/alarms/mod.rs` (the response of an Alarm instance) (D7). |
| `nicecoolingd` | `src/firmware/` (`package.rs`, `checks.rs`, `flash.rs`, `record.rs`), `src/leakshield/mod.rs` (the device lease), `worker.rs`, `attached.rs`, `feed.rs`, `clock.rs`, `external.rs`, `src/fleet.rs` (the bootloader attach), `src/sources/usb.rs` (a HID-only open), `hotplug.rs`, `leakshield.rs` (the poll), `src/value_table.rs` (the update state for the tick), `src/dbus/stage.rs`, `src/alarms/poweroff.rs` and `response.rs` (the forced response), `src/alarms/logind.rs` (the inhibitor), `src/profiles/activate.rs`, `src/dbus/leakshield.rs`. |
| `nicecooling-api` | The XML, the error name, the identifiers and `src/authz_map.rs`. |
| `nicecooling` | `src/screens/leakshield/system.rs` and its view, `src/command.rs`, `src/daemon_client.rs`: the update panel. |

The code reuses `Report1::decode`, the `restart` command of the registry with `Report2`, `crc.rs`, `take_backup` (`leakshield/write.rs`), `Report3::differing_offsets`, `Attached::reattach` and `UsbAttach::open`. The re-enumeration of the simulated device goes through `tests/support/sim_usb.rs`, because `UsbEvent` is a type of `nicecoolingd`.

Rejected: SQLite in the protocol crate. Every tool of the workspace links that crate.

### D2. The app passes a file descriptor

The user picks the database file in the app. The app opens it as the user and passes the file descriptor (D-Bus type `h`) (owner decision). The daemon reads the file through the descriptor:

1. `fstat` the descriptor. Refuse it with `InvalidArgument` when it is not a regular file or when it is above 16 MiB.
2. Read the file into memory on a blocking thread, at most 16 MiB. A read that takes more than 10 s fails with `InvalidArgument`. A descriptor on a FUSE mount can block the read.
3. Open it with SQLite from memory, read-only: `Connection::deserialize_read_exact` of `rusqlite` with the feature `serialize`. Set `SQLITE_DBCONFIG_TRUSTED_SCHEMA` off and `SQLITE_DBCONFIG_DEFENSIVE` on.
4. Refuse the file when `firmware` is not a table in `sqlite_schema`.

- The daemon parses one database at a time. A second call while a parse runs returns `Busy`. A read that blocks holds that slot until its thread ends.
- The list query reads `id`, `version`, `bootloader`, `hardware` and `device` of the rows whose `device` is `leakshield_EN` or `leakshield_DE`. It reads `data` only for the chosen row, only when `length(data)` is at most 4 MiB, as bytes of a BLOB or a TEXT value.
- The daemon never opens a path. Thus the file and its directory stay unchanged.
- The image stays in memory during the update. The daemon writes no copy (`spec.md`, Image source). A resume after a daemon restart needs the file again.
- Both methods are class `write` or higher (D10). Thus only a local, active wheel member makes the daemon read a file.

Rejected:

- A path that the daemon opens. There is a race between the file check and the open: the user can put a FIFO in place of the file and block the daemon. Root also opens a path that a user chose, and root cannot read a FUSE mount of the user (gvfs, sshfs).
- The app reads the database and sends the image with its version, hardware and bootloader. The daemon then takes from an unprivileged client the values that decide whether the image fits the device. With the descriptor, the daemon reads these values and the image from the same bytes.

### D3. Image container

The `data` of a row is a UTF-8 XML document with a byte order mark: the root `FirmwareContainer` holds `firmware`, which holds the `Segment` elements. Each `Segment` has the child elements `adr_start` (decimal) and `data` (base64). `image.rs` parses it with `roxmltree` 0.21 and `base64` 0.22, from `[workspace.dependencies]`. `nicecooling-api` already uses `roxmltree` 0.21, and `Cargo.lock` holds `base64` 0.22.1.

`image::parse(bytes: &[u8]) -> Result<Image, ImageError>` gives `Image { segments: Vec<Segment> }` with `Segment { adr_start: u32, data: Vec<u8> }`. It refuses an image when:

- the bytes are not XML, or the root element is not `FirmwareContainer`;
- the document has no segment, or a segment has bad base64 or an `adr_start` that is not a u32;
- a segment starts below 0x4000. That is the area of the bootloader;
- the end of a segment, rounded up to a whole chunk, does not fit in a u32;
- two segments overlap;
- the decoded data of all segments is above 1 MiB. No source gives the flash size (OQ2). The bound is about 9 times the reference segment of 109952 bytes.

Each segment is cut into chunks from its own `adr_start`, and the last chunk of each segment is padded (D5). The chunk count of the job is the sum over the segments. The Leakshield image has one segment.

### D4. Acceptance checks

`checks::check(target: &Target, image: &ImageRow, options: Options) -> Status` is a pure function.

- `Target` holds `hardware: Option<u16>`, `bootloader: Option<u16>`, `firmware: Option<u16>` and `language: Option<Language>`, with the source of each field. It merges the update record and `bootloader::Identity`, the report 1 fields that the protocol crate reads (D8).
- In normal operation the fields come from report 1: hardware at offset 7, bootloader at offset 11 (the version of the bootloader, as the firmware reports it), firmware at offsets 13-14, and the display language at byte 34 (0 English, 1 German, any other value absent).
- In the bootloader, firmware and language are absent, because the field at offsets 13-14 holds 1 to 99.

The bootloader identity comes first:

| Order | Rule | Status |
|---|---|---|
| 1 | The record (D8) has a hardware or bootloader value, and the bootloader report 1 has a nonzero value for the same field that differs | `target-conflict`, no option lifts it |
| 2 | Hardware or bootloader is absent from the record and from the report | `target-unknown`, the option `unverified_target` lifts it (owner decision) |

The record is the first source of a field, the bootloader report 1 the second. With `unverified_target`, the check of an absent field is skipped. A present field is still checked.

Then the first failing check gives the status:

| Order | Check | Status when it fails |
|---|---|---|
| 1 | image hardware equals device hardware | `hardware-mismatch` |
| 2 | image bootloader equals device bootloader | `bootloader-mismatch` |
| 3 | image version differs from the firmware, or image language differs from the display language | `same-version` |
| 4 | image version is not below the firmware | `downgrade`, the option `downgrade` lifts it |
| 5 | all checks pass | `ok` |

Thus the German 1022 image on a device with firmware 1022 in English passes check 3 and check 4, and its status is `ok`. Check 3 compares only the version when the language is absent.

On a resume (device in the bootloader), check 3 does not apply: a flash of the version from before is a valid recovery. Check 4 compares the image with `firmware_before` of the record, so a lower version needs the option `downgrade` (owner decision). Without a record, neither check applies.

Rejected: the aquasuite rule. aquasuite compares neither the hardware nor the bootloader of the image with the device. The check costs one comparison, and the database has the values.

### D5. Chunks and the report 32 frame

`frame.rs` holds the constants and the builder.

| Item | Value | Source |
|---|---|---|
| Chunk size | 1024 bytes | code |
| Padding of the last chunk of a segment | 0xFF | code |
| Chunk address | `adr_start + n x 1024` | code |
| Frame, after the report ID | `00 00 00`, u32 `2`, u32 address, u32 `1024`, 1024 data bytes, CRC-16/USB u16 big-endian | code |
| CRC range | the bytes after the report ID and before the CRC | code |
| Buffer length | 1042 bytes with the report ID | code; task 1.5 measures it for the bootloader |

The Linux hidraw driver sends the buffer length that the caller passes. When the capture shows a longer feature report 32 in the bootloader, the builder pads the frame with zeros after the CRC to that length. A shorter feature report 32 stops gate 1.6: the frame does not fit, and the owner decides. The value is one constant. The CRC function is the one of `crc.rs`.

Rejected: a parser of the HID descriptor at runtime. The capture gives the length for bootloader 100, and check 2 of D4 refuses an image for another bootloader. With `unverified_target` the bootloader version is not known, so the frame length is not proven. The second confirmation of the panel says so.

### D6. The flash job

The flash is a task of its own, `firmware/flash.rs`. It holds the lease of the device (D7) from the start to the finish signal.

The lease:

- The job takes it from the write worker. The worker first sends a pending save-settings command, then reads report 1 until `settings_dirty` (byte 49) is 0, at most 5 s. A flag that stays set fails the start with `backup-failed`.
- The worker drops a waiting Profile request. While the lease is held, the worker fails each new `Profile` message at once, and the activation reports `leakshield-failed`. It ignores a `CancelProfile` message, which has no reply.
- While the lease is held, the device link of every other user is a transport that fails with `DeviceError::Busy` (D7). Only the job holds the real transport.

The job:

- Each HID call runs in `spawn_blocking`. The chunk time is measured with `std::time::Instant` around the call. The waits between the calls are tokio sleeps. Tokio 1.54 does not auto-advance the paused test clock while a blocking task runs, so the paused clock drives the waits and a slow simulated call takes real time.
- The job never reads `UsbEvent`. The `Attacher` opens and classifies every connect (D8) and reattaches the transport to the `Attached`. The job waits on the `Info` watch of the `Attached` and then takes the new transport under its lease.
- The job holds a logind inhibitor lock (`Inhibit("shutdown:sleep", "nicecooling", "Leakshield firmware update", "block")`) from its first step to the finish signal. The lock blocks the shutdown and sleep requests of the user and the system. The Alarm poweroff of the daemon skips it: an Alarm takes priority over the update, and the device then waits in the bootloader (owner decision).
- The start is refused with `Busy` while an Action runs, or while `leakshield-unreachable` is active for the Leakshield. The job reads the running Action from the mode of report 1, with `leakshield/action.rs`.

The steps follow aquasuite (`docs/re/firmware-update.md`, Sequence). Task 1.6 decides a change of a timing after the capture.

| Step id | Action | Source |
|---|---|---|
| `backup` | Take the inhibitor lock. Take the lease. Save the backup. Write the update record. | design |
| `enter-bootloader` | Wait 5 ms. Send the restart command with parameter 1. Wait 4 s. Skip this step when the device is in the bootloader. | code |
| `wait-bootloader` | Wait for the `Attached` in state bootloader (D8). At most 8 s. | code |
| `settle` | Wait 2 s. | code |
| `transfer` | For each chunk: write the frame, then wait 50 ms. | code |
| `leave-bootloader` | Wait 250 ms. Send the restart command with parameter 0. Wait 1 s. | code |
| `wait-device` | Wait for the `Attached` in normal operation: report 1 firmware 100 or more. At most 15 s. | design; aquasuite does not wait |
| `read-back` | Compare the firmware version. Check the plausibility. Compare reports 3 and 8 with the backup (D9). | design |

Chunk failure:

- The chunk write is a `HIDIOCSFEATURE` call. The kernel can block it for about 5 s (`USB_CTRL_SET_TIMEOUT` of usbhid), and user space cannot cancel it.
- A call that returns an error at once is sent again, until 300 ms after the first try (code).
- A call that returns after more than 300 ms fails the chunk, also without an error. The job does not send it again.
- After a failed chunk, the job stops with `transfer-failed` and does not send the restart command with parameter 0. The device stays in the bootloader (owner decision). aquasuite sends that command, but a start of an incomplete image has no known outcome.

The worst-case time for the reference image is the sum of the bounds: the dirty wait 5 s, the backup and the record about 1 s, 4 s + 8 s + 2 s, 108 x (50 ms + 300 ms), 1.25 s, 15 s and the read-back about 1 s. That is about 75 s. A slow chunk call adds at most about 5 s, and then the job stops. Each step has its own bound, so the job always ends.

Other results:

- `backup-failed`, `bootloader-not-found` and `device-lost` end the job at their step. After `bootloader-not-found` the device stays where it is. A job that ends with the device in normal operation deletes the record (D8).
- `version-mismatch`: the device returns in normal operation with a version other than the image version.
- `implausible-state`: the version is right, but the plausibility check fails. The check needs a registry layout for the new firmware. With one, the mode must be a listed mode and no system alarm bit (bits 7-10) may be set. Without one, the check covers only the version.

A resume (device in the bootloader) takes no new backup, because the bootloader cannot give one. It reuses the backup id of the update record. Without a record there is no backup, and the read-back checks only the version and the plausibility. A resume writes the record before the first chunk. A field that it does not know stays absent.

When the daemon stops during the job (crash, `systemctl stop`, a package upgrade, an Alarm poweroff), the job stops at once (owner decision). The record stays, and the device waits in the bootloader. At the next start the daemon finds it (D8) and offers the resume.

Rejected: finish the transfer before the daemon exits. It fits in the stop timeout of systemd, but a stop would then wait up to about 40 s, also for an Alarm poweroff.

### D7. The device during the update, and the Alarms

`UpdateState` has three values:

- `idle`: no job runs and no update is open.
- `running`: the job holds the lease.
- `interrupted`: no job runs, and an update record exists or the device is in the bootloader. The device can be in the bootloader or absent.

While `UpdateState` is not `idle`:

- The device link of every user other than the job is the busy transport. Thus no command and no read reaches the device from the worker, `send_late` (`SetMode`, `StartAction`, `SyncClock`, the reset methods), the Feed sender, the detector, `GetFields`, `GetEvents`, `CreateBackup`, `GetDeviceResponseProposals` or the report 1 poll. aquasuite also stops its periodic reads before the transfer.
- A D-Bus method that gets the busy error returns `Busy`, `GetDeviceResponseProposals` included. A disconnect during the update keeps the busy link, so these methods return `Busy`, not `Disconnected`. The Feed sender, the poll and the detector treat it as a skip, not as a disconnect.
- `ListFirmwareImages`, `UpdateFirmware` (the resume), `ListBackups` and `GetBackup` stay available. They do not use the busy link.
- A Profile activation reports `leakshield-failed`.
- A hot-plug disconnect or connect with the USB path of the device keeps the object. The property `Connected` follows the device.
- At each return to normal operation, the detector drops its last known report 3 and takes the next read as the new value. The update raises no external change.
- The bootloader report 1 never enters `LeakshieldStatus`. Thus the alarm stage decodes no alarm bits from it, and the Alarms of the leak groups keep the state they had.

The update state of each Leakshield object goes into the value table (`value_table.rs`), with its other state. `TickContext` gives it to the alarm stage.

- `running`: the alarm stage does not raise `leakshield-unreachable` for that Leakshield (owner decision). The job ends within the bounds of D6.
- `interrupted`: `leakshield-unreachable` rises after its normal delay, with the response `notify`, whatever response the user set (owner decision). A poweroff at each boot would stop the user before a resume. The stage stores the response on the instance in `ActiveAlarms`, `dbus/stage.rs` puts it into the poweroff event, and `alarms/poweroff.rs` uses it before the configured one of `alarms/response.rs`.
- When the state leaves `running`, the unreachable rule restarts its count for that Leakshield. Thus the old report 1 time raises no Alarm after a good flash, and the delay of `interrupted` counts from the end of the job.
- A job does not start while `leakshield-unreachable` is active for the Leakshield (D6). Thus no instance of it with a pending poweroff exists while the job runs.
- The other Alarms, for example the pump-stop Alarm, stay active in both states. Their poweroff takes priority over the update (D6).

Rejected:

- A rule in each user of the device. A list of users misses some: the first draft missed `GetFields`, `CreateBackup` and the report 1 poll. The lease is one gate for all of them.
- A fixed hold of 3 minutes from the start. It has no basis in the step times, and it ends while the device can still be in the bootloader.
- A new Alarm `leakshield-bootloader`. The forced `notify` keeps the Alarm list and its configuration as they are (owner decision).

### D8. The bootloader test and the update record

The `Attacher` (`fleet.rs`) tests each device as aquasuite does (code). It opens HID interface 1 of every `0c70:f014` device and reads report 1, which the device sends by itself. Firmware 1 to 99 means the bootloader.

- The bootloader has no bulk interface, so `UsbBus` gets a HID-only open. `sim_usb.rs` gets it too.
- `DeviceIo` takes a report 1 of any length. It pads a short one with zeros and reads only the identity fields (offsets 1-14 and 34). A hardware or bootloader field of 0, and the serial 0-0, count as absent. Only a report of 837 bytes goes to `Report1::decode`.
- The `Attached` keeps the identity and the time of each report 1 of any length. `Info.firmware`, `Connected` and the report 1 time of the unreachable rule follow it.
- The test runs at the start (`open_present`) and on each hot-plug event. A device in the bootloader is no longer skipped.
- While `UpdateState` is not `idle`, the `Attacher` classifies a connect with a `DeviceIo` of its own and updates `Info` from that read. It hands the new transport to the lease holder, or keeps it for a resume. `detach` and `reattach` leave the busy link of the other users in place.
- `bcdDevice` 2.00 and two interfaces stay a log hint. They come from one measurement of one device.

The update record is `/var/lib/nicecooling/leakshield/<id>/firmware-update.json`, where `<id>` is the object id of the Leakshield. Its fields:

```json
{"schema_version": 1, "serial": "s", "usb_path": "s", "hardware": 1000, "bootloader": 100,
 "firmware_before": 1022, "backup": "s", "started_us": 0,
 "image": {"id": 0, "version": 1022, "language": "en"}}
```

- `serial`, `hardware`, `bootloader`, `firmware_before` and `backup` can be null after a resume without a record.
- The daemon writes it atomically before the restart command.
- Each classification in normal operation while no job runs, by the `Attacher` at start or on hot-plug, deletes the record and sets `idle`. The job deletes it when it ends with the device in normal operation. Thus `interrupted` lasts only while the device is in the bootloader or absent.
- At start the daemon reads every `leakshield/*/firmware-update.json`. It matches a device in the bootloader by the USB path, else by a serial in the bootloader report 1.
- It has no last-known-good copy. A damaged record, or one with a higher `schema_version`, counts as absent and is logged. The device then needs the option `unverified_target`.
- Task 3.3 adds it to `docs/storage.md`.

The object of a device in the bootloader takes its id from the serial of the record, else from a serial in the bootloader report 1. Without both, the id is open (OQ1). The property `Firmware` shows the report value, and the settings are read-only with the reason `unknown-firmware`.

### D9. After the flash

The device returns with the attach path of v1. The registry decides:

- Firmware covered by the registry: the device is writable with the layout of that version.
- Firmware not covered: `Writable` is false with `unknown-firmware`, the daemon sends no Feed message and raises the existing Warning `leakshield-unknown-firmware`. A Profile activation fails with `leakshield-failed`.

The `wait-device` step and the version check read only the identity fields of report 1 (D8). Thus a new firmware with another report 1 size still ends the job with the right result, and the device stays reachable.

The read-back compares reports 3 and 8 with the backup. Report 5 holds the page of the display and a 128 x 64 image of the screen, so it changes on its own. The backup still holds reports 3, 5 and 8 for `RestoreBackup`.

- The comparison uses the volatile mask of the firmware before the update, because the registry may not know the new one.
- The result lists the names of the reports that differ, and the names of the reports that the daemon cannot read or decode. A difference does not change the result `ok`: the list goes into the `differences` detail.
- The daemon never writes the backup back on its own. The user restores it with `RestoreBackup`, which refuses a backup of another firmware version (`docs/api.md`).

The backup uses the reason `before-write`. A new reason would change the `ListBackups` contract, and the retention rule already covers it.

### D10. D-Bus contract additions

Task 5.1 writes these items into the XML and `docs/api.md`. The owner reviews them before task 5.2.

| Item | Definition |
|---|---|
| Property `UpdateState` (`s`) | `idle`, `running` or `interrupted` (D7). |
| Method `ListFirmwareImages(database h) -> a(xsqqqs)` | Image id, language `en` or `de`, version, hardware, bootloader, status. Class `write` (owner decision). |
| Method `UpdateFirmware(database h, image x, options a{sv})` | Class `critical`. It returns when the checks pass and the job starts. Option keys: `downgrade` (`b`), `unverified_target` (`b`). |
| Signal `FirmwareUpdateProgress(step s, done u, total u)` | The step ids of D6. During `transfer`, `done` counts the chunks and `total` is the chunk count. |
| Signal `FirmwareUpdateFinished(result s, details a{sv})` | Result ids: `ok`, `transfer-failed`, `device-lost`, `version-mismatch`, `implausible-state`, `bootloader-not-found`, `backup-failed`. Details: `firmware` (`q`), `backup` (`s`), `differences` (`as`). |
| Error `ImageRefused` | The message holds the status of D4: `same-version`, `downgrade`, `hardware-mismatch`, `bootloader-mismatch`, `target-unknown` or `target-conflict`. |

`UpdateFirmware` checks in this order: polkit, the descriptor and the database (`InvalidArgument`, or `Busy` while another parse runs), `Busy` when an update runs, `Disconnected` when the device is not connected, `Busy` when an Action runs or `leakshield-unreachable` is active, the image id (`NotFound`), the container (`InvalidArgument`), the checks of D4 (`ImageRefused`), then the start. `ListFirmwareImages` returns `Disconnected` when the device is not connected.

`docs/api.md` adds: the update, the parse slot and the active unreachable Alarm to the meaning of `Busy`; the `Busy` of every method that reaches the device while `UpdateState` is not `idle`; and an exception to the rule that the response of an instance is the response of its event: `leakshield-unreachable` takes `notify` while the update is interrupted. Both methods carry the annotation `io.github.cappyt.NiceCooling1.Authorization`. The map test of v1 task 4.4 finds them, and the test of the critical methods in `authz_map.rs` gets `UpdateFirmware`.

### D11. The update panel

The panel is a new card of the Leakshield system screen. `docs/ui/screens/leakshield-system.md` gets the card, its states and its copy (task 6.1). The owner approves the text before task 6.2, as in v1 task 8.1.

- Flow: the user picks the database file, the app opens it and calls `ListFirmwareImages` with the descriptor, the user picks an image by language, the app shows the warnings and the confirmation, then calls `UpdateFirmware` with a new descriptor and the `critical` prompt of polkit.
- Warnings: the leak protection is off while the device is in its bootloader, the user must not unplug the device or power off the PC, and an Alarm poweroff stops the update. A downgrade and `unverified_target` each have a second confirmation. The second one for `unverified_target` says that the daemon cannot check the device.
- States: device in normal operation, update interrupted with the device in the bootloader (resume) or absent, offline, daemon offline, running (a progress bar with the step and the chunk count), result (ok, each failure, the list of differences).
- The panel calls only the methods of D10. It holds no image. The app passes the descriptor as `zbus::zvariant::OwnedFd` through `daemon_client.rs`, and subscribes to the two signals there.

### D12. The simulated bootloader

`sim.rs` gets a mode: `Application` or `Bootloader`. A test builds it with `SimDevice::fw1022().with_bootloader(BootloaderConfig { enter, leave, chunks, image, new_firmware })`:

- `enter` and `leave`: the time to enter and to leave the bootloader.
- `chunks`: a `ChunkBehavior` for each chunk index: `Accept`, `FailFast`, `Slow(Duration)` (the call sleeps in real time, then succeeds) or `Drop` (the call succeeds and the chunk is lost). The default is `Accept`.
- `image`: the chunks that it expects. `new_firmware`: the version it reports after a complete image.

Behavior:

- Report 2 command 1 with parameter 1 in `Application` moves the device to `Bootloader` after `enter`. The test support (`tests/support/sim_usb.rs`) then sends a disconnect and a connect. The bootloader report 1 is a placeholder of 64 bytes until gate 1.6: byte 0 is the report ID 0x01, byte 2 is 0x01, bytes 13-14 are 0x0001, all other bytes are 0.
- A feature write of report 32 with a wrong length or CRC fails. A good frame is stored at its address.
- Command 1 with parameter 0 moves the device to `Application` with `new_firmware` when the stored chunks equal the expected image. Otherwise the device stays in the bootloader (inferred behavior).
- Tests use the paused tokio clock, so the 4 s and 8 s waits cost no time.

Hard rule: no test and no tool of this change opens the real device. Tasks 2.5 and 8.1 check it with `rg`. The tool `tools/nicecooling-verify` never calls the firmware module.

### D13. The manual checks

Two MANUAL tasks touch the real device.

- The capture (section 1). The owner runs aquasuite in the VM and records the host bus with `usbmon`. Run 1: aquasuite in German with the device in English flashes the DE image. Run 2: aquasuite in English restores the EN image. The tool `tools/re/usbmon-summary.py` reads the capture files and never opens the device. The tasks that depend on the results come after gate 1.6.
- The first flash from nicecooling (section 7). The owner sits at the desk. aquasuite in the VM is the recovery plan: the procedure of `docs/hardware-checks/firmware-update.md` recovers a device in the bootloader.

The capture settles: the feature report 32 length of the bootloader, the status of each SET_REPORT (ACK, STALL or timeout), the time to enter and to leave the bootloader, the interfaces of the bootloader, and the content of the bootloader report 1. It does not settle the answer of the bootloader to a bad frame, or whether it checks the image (OQ3). No bad frame goes to the device (owner decision).

## Risks / Trade-offs

- [The device stays in the bootloader and the leak protection is off] -> The warning before the start. The resume offer in the panel. `leakshield-unreachable` rises as `notify` (D7). aquasuite in the VM as recovery.
- [A wrong image damages the device] -> The hardware and bootloader checks against the values of the database, which equal the report 1 fixture of the reference device. The 0x4000 lower bound. No chunk goes out before the checks pass. Whether the bootloader checks the image is unknown (OQ3).
- [A chunk is dropped without an error] -> The read-back checks the version, the plausibility and the settings. A dropped chunk in a part that these checks do not reach stays undetected. The capture shows whether the bootloader answers each SET_REPORT with ACK.
- [The daemon stops during the transfer] -> The job stops at once, and the record stays. The start path finds the device in the bootloader (D8), and the panel offers the resume.
- [An Alarm poweroff during the flash] -> The poweroff takes priority (owner decision). The device waits in the bootloader, and the panel offers the resume after the boot. The warning before the start says so.
- [A user or system shutdown or a suspend during the flash] -> The inhibitor lock blocks both. A forced power off leaves the device in the bootloader, and the resume recovers it.
- [The device takes more than 15 s to return] -> The result is `device-lost`, the record stays and `UpdateState` is `interrupted`. The normal attach path finds the device when it returns, and the result text says that it can still return.
- [The new firmware changes the size of report 1] -> The job ends with the right result, and the device stays connected and reachable (D8, D9). `Report1::decode` takes only 837 bytes, so the device shows no values until the registry covers the new version.
- [Root parses a file that a user chose] -> The descriptor, the class `write`, the size bounds, the read time limit, one parse at a time, the defensive SQLite open and the table check (D2).
- [The user flashes an image for the wrong language] -> The panel shows the language of each image next to the display language of the device (report 1 byte 34). The user can flash again.
- [The kernel HID driver binds to the bootloader interface] -> The first flash check (section 7) watches the kernel log for errors.
- [The new firmware is not in the registry] -> The settings stay read-only and the Feed stops, with a Warning. The registry gets the version in a later change.

## Migration Plan

Nothing to migrate. The change adds two methods, two signals, one property and one error to the `Leakshield` interface, and one stored file that exists only during an update. Rollback: remove the RPM. A device in the bootloader after a rollback needs aquasuite in the VM.

## Open Questions

- OQ1. **Object id without serial.** If the bootloader report 1 has no serial and no record exists, the object has no id from the serial. A fixed id such as `leakshield_bootloader` is possible when one device is present. The capture shows whether the serial is there.
- OQ2. **Upper address bound.** The image check has the 0x4000 lower bound, a 1 MiB size bound and the u32 end check. An upper address bound needs the flash size of the hardware, which no source gives.
- OQ3. **Image check in the bootloader.** No source shows how the bootloader answers a bad CRC or a wrong address, or whether it checks the whole image. No bad frame goes to the real device to test it (owner decision).
