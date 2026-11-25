# LEAKSHIELD firmware update protocol

This note gives the protocol that aquasuite X.96 uses to write a firmware image to the LEAKSHIELD. The image format is in [leakshield-firmware.md](leakshield-firmware.md). The code that defines each statement is item P11 of the private note that [leakshield.md](leakshield.md) names. Each statement has a confidence:

- **code**: the decompiled code defines it.
- **inferred**: a conclusion from the code or from a measurement, not stated by the code.
- **unknown**: open. The last section gives a check that can settle it.

The X.75 service library has the same chunk builder and the same report 2 commands. The X.75 GUI was not read, so the X.75 update sequence is inferred to be the same.

## Sequence

aquasuite runs the update in a background worker of the GUI. The worker does these steps in this order (code).

| Step | Action | Timing |
|---|---|---|
| 1 | If the device is not in the bootloader: queue a 5 ms delay and the report 2 command "restart into the bootloader" on the normal device queue. | then wait 4 s |
| 2 | Wait for the bootloader: poll for "device online and in the bootloader". | every 250 ms, at most 8 s. On timeout: wait 2 s, then fail |
| 3 | Stop all running transfers of the device. Stop the periodic report reads, clear the job queue. | then wait 2 s |
| 4 | Queue one job group with all chunks of all segments: a report 32 write, then a 50 ms delay, for each chunk. | 300 ms timeout for each write |
| 5 | Poll the job group: progress is the number of finished jobs divided by the number of all jobs. Stop at the first failed job. | every 100 ms |
| 6 | Clear the job queue (again after a failure). | 2 s for each clear |
| 7 | Queue a new job group: a 250 ms delay, then the report 2 command "restart". | |
| 8 | Wait 1 s, close the HID device, wait 2 s. Without a failure, set the progress to 100. | |

There is no erase command, no start command, no read-back and no verify command (code). The host sends only report 2 output reports and report 32 feature writes. It reads nothing from the bootloader, except the report 1 that the device sends by itself.

## Enter the bootloader

The host sends report 2 command 1 (`restart`) with parameter 1. Report 2 has the layout in `docs/protocol/leakshield.md`: report ID, command u32 big-endian, parameter u32 big-endian, CRC-16/USB big-endian over the 8 bytes (code).

| Purpose | Parameter | Bytes |
|---|---|---|
| Restart into the bootloader | 1 | `02 00000001 00000001 F443` |
| Restart (end of the update) | 0 | `02 00000001 00000000 3482` |

The CRC values are computed with CRC-16/USB. The same function gives `34C6` for the save-settings command, which `docs/protocol/leakshield.md` gives. aquasuite puts a 5 ms delay before the command and uses a 500 ms timeout for the output report (code).

## Detect the bootloader

- aquasuite finds the device by the USB product ID 0xF014 and HID interface 1, in the application and in the bootloader (code). It does not read `bcdDevice` or the interface count. The bootloader keeps the vendor and product ID (inferred from the hardware check, `docs/hardware-checks/firmware-update.md`).
- The device is in the bootloader when the firmware version of report 1 (bytes 13-14, big-endian) is from 1 to 99 (code). The bootloader sends a report 1 that is shorter than the application report 1. aquasuite pads it with zeros to the full size before it reads the fields (code).
- The reference device in the bootloader sends firmware 1, `bcdDevice` 2.00 and two USB interfaces (measured, hardware check).

## Transfer

### Chunks

- Each segment of the image (`adr_start` and `data`, `leakshield-firmware.md`) is cut into chunks of 1024 bytes (code). The LEAKSHIELD image has one segment at 0x4000.
- The host fills each chunk with 0xFF before it copies the data. Thus the last chunk is padded with 0xFF to 1024 bytes (code).
- The address of chunk n is `adr_start + n x 1024`. The length field is always 1024, also for the last chunk (code).
- The host sends the chunks in address order, one at a time. The data stays encrypted: the host does not change the bytes (code).
- The EN segment (109952 bytes) gives 108 chunks, from 0x4000 to 0x1EC00. The last chunk holds 384 data bytes and 640 bytes of 0xFF. The DE segment (109984 bytes) also gives 108 chunks (computed).

### Report 32 layout

Each chunk is one HID Feature write (SET_REPORT, report type Feature) of report ID 32 (0x20). The table gives the buffer that the host passes to the HID driver (code).

| Offset | Size | Content |
|---|---|---|
| 0 | 1 | Report ID 0x20 |
| 1 | 3 | 0x00 0x00 0x00 |
| 4 | 4 | Constant 2, u32 big-endian. Meaning unknown, inferred to be a "write flash" code |
| 8 | 4 | Flash address of the chunk, u32 big-endian |
| 12 | 4 | Length 1024 (0x00000400), u32 big-endian |
| 16 | 1024 | Chunk data |
| 1040 | 2 | CRC, u16 big-endian |

- CRC algorithm: CRC-16/USB (polynomial 0x8005, reflected input and output, initial value 0xFFFF, final XOR 0xFFFF). The settings report and report 2 use the same function (code).
- CRC range: bytes 1-1039, that is, all bytes after the report ID and before the CRC (code).
- The frame is 1041 bytes after the report ID. The Windows HID call sends a buffer of the feature report length that the HID descriptor declares. A longer descriptor length pads the frame with zeros after the CRC, a shorter one cuts it (code). The bootloader descriptor length is unknown.

Example, start of the first chunk: `20 000000 00000002 00004000 00000400`, then the first 1024 bytes of the segment, then the CRC.

### Timing

- A 50 ms delay follows each chunk write (code).
- One chunk write has a timeout of 300 ms (code).
- The 108 chunks take at least 108 x 50 ms = 5.4 s, plus the USB transfer time (computed).
- The full sequence takes about 4 s + up to 8 s + 2 s + transfer + 2 s + 1 s + 2 s (code).

### End

After the last chunk the host waits 250 ms and sends report 2 command 1 with parameter 0 (code). It sends no finish or verify command. The device then leaves the bootloader and starts the application (measured: the device came back with firmware 1022, `bcdDevice` 1.00 and three interfaces, hardware check).

## Checks before a flash

### Version gate

| Check | X.96 | X.75 |
|---|---|---|
| Firmware that aquasuite accepts for normal operation | 1022 only | 1020 only |
| Update possible (HID open) | device firmware at most 1022 | device firmware at most 1020, and not the server variant |
| Image version in the package database | 1022 | 1020 |

These values are code, except the database rows, which are read from the package. A device firmware above the accepted one marks the update as "firmware too new" and blocks it (code).

The System page button of the GUI works like this (code):

- Device firmware 5000 or more: the button asks for an image file (`.xml`).
- Ctrl and Alt held: the button asks for an image file. If the user cancels the dialog, the button flashes the package image without the version gate.
- Device firmware above 1022: nothing happens.
- Device firmware other than 1022, including the bootloader (1-99): the button flashes the package image.
- Device firmware 1022: the button flashes only when the display language of the device differs from the image language. It is then a language change.

The main window also builds an update list from each device whose firmware is not the accepted one (code).

### Hardware and bootloader IDs

The host does not compare hardware or bootloader IDs of the image with the device (code). The database rows hold bootloader 100 and hardware 1000, but the lookup reads only the version and the data. The image container holds only segments. The image version is not compared with the device firmware either. Other device types of aquasuite check the hardware revision or the bootloader version; the LEAKSHIELD path does not.

### EN or DE

aquasuite picks the image by its own user interface language, not by the device (code). Language ID 1 selects `leakshield_EN`, any other ID selects `leakshield_DE`. In the automatic language mode, aquasuite sets ID 1 when the system culture is not the one it compares with; the compared value is encrypted, inferred to be German. The device display language is report 1 byte 34 (0 English, 1 German).

## Error handling

- Chunk failure: a failure is only a failed HID Feature write. The host reads no acknowledgement from the bootloader (code). When the write call fails, the transport tries the same job again at each pass of its job loop until 300 ms after the job start. Then it marks the job as failed (code). How the bootloader signals a bad CRC (a stalled control transfer or a silent drop) is unknown.
- The job group itself does not stop at a failed job. The worker sees the failure at its next 100 ms poll and clears the queue (code). Thus a few more chunks can go out after a failure (inferred).
- There is no retry of the full transfer and no resume (code).
- After a chunk failure, the worker still sends the restart command, and it ends without its error flag. The GUI then shows the update as finished (code). The device stays in the bootloader when its image is incomplete (inferred from the hardware check).
- Device already in the bootloader: the worker skips the restart command, the wait passes at once, and the transfer starts again from the first chunk at `adr_start` (code). The hardware check confirms that a second full flash recovers the device.
- Bootloader not found within 4 s + 8 s: the worker fails with its error flag set and sends nothing to the bootloader (code).
- Re-enumeration after the flash: the worker does not wait for it. It closes its HID device and ends. The normal device discovery of aquasuite finds the device again (inferred).

## After the flash

- The worker reads no report and writes no settings. It does not restore a backup and does not load the factory defaults (code).
- The update page has a button that restarts aquasuite (code).
- The settings and the display assets survive the flash (inferred). The report 3 and report 8 dumps in `tests/fixtures/leakshield/fw1019/` and `fw1022/` are byte-identical across the 1019 to 1022 update.

## Open points and checks

All open points can be settled by one supervised update of the reference device in the VM (procedure in `docs/hardware-checks/firmware-update.md`), with a `usbmon` capture on the host bus of the device. A capture only observes the traffic of aquasuite. With firmware 1022 on the device, a language change (aquasuite in German, device in English) makes aquasuite flash the DE image. A second update with aquasuite in English restores the EN image.

| Open point | What the capture shows |
|---|---|
| Feature report 32 length of the bootloader | The HID report descriptor of the bootloader in the enumeration, and the `wLength` of each SET_REPORT |
| Bootloader answer to each chunk | The status of each control transfer: ACK, STALL or timeout |
| Time to enter and to leave the bootloader | Time from the restart command to the new enumeration, and from the last command to the application enumeration |
| Bytes 1-7 of the frame | Only the bootloader code gives their meaning. A capture confirms that aquasuite sends `000000 00000002` |
| Bootloader answer to a bad CRC or a wrong address | Not covered by a passive capture. It needs a deliberate bad frame: an owner decision |
| Bootloader report 1 content | The full report 1 in the bootloader: serial, hardware and bootloader version fields |
