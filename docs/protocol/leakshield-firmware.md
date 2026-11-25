# LEAKSHIELD firmware 1022 image

ADR-0001 names the firmware image as a second source for registry fields that the aquasuite code leaves open. This note gives where the image is, its format, and what it can tell. The image and its decoded segments stay in `re/firmware/`, which git ignores. The code paths are item P11 of the private note that [leakshield.md](leakshield.md) names.

## Source

The image comes with the aquasuite X.96 package. aquasuite does not download it. The package holds a SQLite database, `Content/aquasuite_db.database` (4603904 bytes, also copied to `$APPDATA/aquasuite-data/`). Its table `firmware` has the columns `id`, `version`, `bootloader`, `hardware`, `device` and `data`. Two rows hold the LEAKSHIELD firmware:

| id | device | version | bootloader | hardware | Size of `data` |
|---|---|---|---|---|---|
| 26 | `leakshield_EN` | 1022 | 100 | 1000 | 146904 |
| 27 | `leakshield_DE` | 1022 | 100 | 1000 | 146948 |

aquasuite loads the row `leakshield_EN` when its user interface language is English, and `leakshield_DE` for any other language ([leakshield-firmware-update.md](leakshield-firmware-update.md)). The `DE` row is the same firmware with German display texts. The update service subscription and the activation code play no part in getting the image.

The extraction reads the database file with Python `sqlite3` in read-only mode. No aquasuite code runs.

## Files

| File in `re/firmware/` | SHA-256 |
|---|---|
| `leakshield_EN_fw1022_bl100_hw1000.bin` (database row, XML) | `afbcbefcdbb7c6a6d6e468aea4e0bc99e63068c8d66138be1ab15c0cf83d6e9d` |
| `leakshield_DE_fw1022_bl100_hw1000.bin` (database row, XML) | `f3b2da197f49fa8e3acd8641915d286b1b9b03ad0c39648520df19e5237aa3fd` |
| `leakshield_EN_fw1022_segment_0x04000.bin` (decoded segment, 109952 bytes) | `2a749675780c2aa9e6f2e376926301ca3551056afc34b0e14de0f56ce5d74c00` |
| `leakshield_DE_fw1022_segment_0x04000.bin` (decoded segment, 109984 bytes) | `1b1ac5177a1c38ead7d7985fa4fa929d66961ab3232b35b6087b617d52d539d9` |

## Format

- The database row is a UTF-8 XML document with a byte order mark: `FirmwareContainer`, then `firmware`, then one `Segment` with the child elements `adr_start` and `data` (base64).
- `adr_start` is 16384 (0x4000): the flash address of the application, after the bootloader.
- The decoded segment is encrypted. Its byte entropy is 7.997 bits per byte, and it has no readable text.
- The cipher has 8-byte blocks without chaining: 1511 8-byte blocks repeat at offsets that are multiples of 8, and only 559 at other offsets. A plain fill such as 0xFF gives the same cipher block each time.
- EN and DE differ in the first 8 bytes (a header block) and from offset 1824 on (the texts move the rest of the image).
- The GUI sends the segment unchanged to the bootloader: 1024-byte chunks, each one a Feature write of report 32 with the flash address, the length and a CRC ([leakshield-firmware-update.md](leakshield-firmware-update.md)). Thus the bootloader holds the key, and the host code has no decryption.

## What the image tells

Nothing about the registry yet. The image is encrypted, and the key is not in the aquasuite code. The fields that the code leaves open stay open:

- `signal_output_config` and `alarm_start_delay`: values not defined.
- `display_idle_delay`: unit not defined.
- Bit 0 of `shield_flags`: not defined.
- The device behavior without a Feed.
- The report 1 counters that commands 13 and 25000 clear.

A decryption needs the key from the bootloader or a plaintext attack on known blocks, for example the 0xFF fill. Both are outside the scope of ADR-0001. No firmware goes to the device from this work.
