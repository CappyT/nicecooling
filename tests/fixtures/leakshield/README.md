# Leakshield fixtures

Raw HID reports of the reference Leakshield (USB `0c70:f014`), one folder per firmware version.

- `report1.bin`: input report 1 (sensors), as read from `/dev/hidraw`.
- `report3.bin`, `report5.bin`, `report8.bin`: feature reports read with `HIDIOCGFEATURE`.

Each file starts with the report ID byte, so it is one byte longer than the report size in the HID descriptor (836, 749, 1027 and 3090 bytes). Reports 3 and 8 end with a CRC-16/USB of bytes 1 to n-2, stored big-endian. Report 5 has no such CRC.

The dumps are reads only: no write to the device produced them.
