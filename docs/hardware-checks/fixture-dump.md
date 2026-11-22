# Leakshield fixture dump

This procedure reads reports 1, 3, 5 and 8 of the reference Leakshield into `tests/fixtures/leakshield/fw<firmware>/`. It only reads: input report 1 with `read()`, the feature reports with `HIDIOCGFEATURE`. Each step has a pass line.

## Steps

1. Find the hidraw node of USB interface 1 of the device `0c70:f014`: `/sys/bus/usb/devices/<port>/<port>:1.1/*/hidraw/`.
2. Give the user a temporary ACL on that node through pkexec: `setfacl -m u:<user>:rw /dev/hidrawN`. The ACL ends at the next reboot or re-enumeration.
3. Run the dump script with the node and `tests/fixtures/leakshield` as arguments. It names the folder from report 1 bytes 13-14.
4. Check the sizes: 837, 750, 1028 and 3091 bytes, report ID included.
5. Check the CRC of report 3 with `tools/re/decode_report3.py`.

## Record: 2026-11-23, serial 20417-31072, firmware 1022

| Step | Result |
|---|---|
| 1 | pass: `/dev/hidraw1` |
| 2 | pass. The owner keeps the ACL until the next reboot, for the next hardware checks |
| 3 | pass: `tests/fixtures/leakshield/fw1022/` |
| 4 | pass: 837, 750, 1028, 3091 |
| 5 | pass: CRC 0xECF7 match, `structure_id` 100. Report 3 is byte-identical to the fw1019 dump |
