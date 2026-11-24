# Leakshield check

This procedure checks the daemon against the reference Leakshield (serial 20417-31072, firmware 1022). It checks the read of all settings, the Feed, the pressure, the backups, the restore and the alarm reset. It also records what the device does when the Feed stops. Each step has a pass line.

The check writes the device: `SetFields`, `RestoreBackup`, `ResetAlarms`. It also stops the Feed on purpose. Read Safety first.

## Safety

- The loop is filled and leak-free. The pump runs at its normal duty. The PC is idle. A person sits at the PC for the whole check and can stop the pump.
- The check changes one low-risk field only: `display_page_time`. It never writes a Safety field. A Safety field is read-only until its Verification write passes (`docs/hardware-checks/verification-writes.md`).
- Take the backup of step 7 before any write. Step 10 restores it. If a step leaves the device in an unknown state, run Recovery.
- Step 6 stops the Feed. The Leakshield then computes its pressure with no data from the daemon. The code does not say what it does (`docs/protocol/leakshield.md`, Behavior when the Feed stops). Keep the step short and stay at the PC. `systemctl start nicecoolingd` ends it.
- Step 6 keeps the Mode of `default_mode` (owner decision): RELEASE on the reference device. `docs/protocol/leakshield.md`, Behavior when the Feed stops, has the result.
- Steps 11 to 14 use only bit 0 of `alarm_bits` (the fill level warning). Its Device response in the fw1022 dump is "none" and its alarm response is "notify". A large leak or system bit can power off the PC (alarm response "poweroff", Device response "power switch long"). Do not use those bits.

## Preconditions

- Fedora 45 on the reference desktop. The unit `nicecoolingd` runs and the Leakshield is on its USB cable. The daemon and the app are installed (`docs/app.md`, Install).
- The Loop `loop_1` exists. Its Pump channel is `nct6798_pwm6`, its flow sensor is `asusec_fan2` with the table `alphacool_17558`, and its Leakshield is `leakshield_20417_31072`.
- The out-of-tree driver is not loaded: `modinfo -n aquacomputer_d5next` prints a path under `kernel/drivers/hwmon/`, and `dkms status` lists no `aquacomputer`. The in-tree driver gives the hwmon device `leakshield`.
- `jq`, `xxd` and `rg` are installed.
- A second terminal runs `sudo busctl --system monitor io.github.cappyt.NiceCooling1` for the whole check: a monitor of the system bus needs root. It prints `ExternalChange` and `WriteFailed`.

## Shell variables

Run these in each terminal that runs a step. The functions read report 1 straight from the hidraw node of interface 1. Report 1 is an input report, so the daemon and this shell each get their own copy. The offsets are those of `docs/protocol/leakshield.md`. These functions have not been run on the device yet.

```bash
B=io.github.cappyt.NiceCooling1
R=/io/github/cappyt/NiceCooling1
LSID=leakshield_20417_31072
LS=$R/leakshield/$LSID
BC="busctl --allow-interactive-authorization=true"
hwof() { for d in /sys/class/hwmon/hwmon*; do [ "$(cat $d/name)" = "$1" ] && echo $d; done; }
HID=$(for h in /sys/class/hidraw/hidraw*; do p=$(readlink -f $h/device); echo "$p" | rg -q '0003:0C70:F014' && echo "$p" | rg -q ':1\.1/' && echo /dev/${h##*/}; done)
r1() { sudo dd if=$HID bs=1024 count=1 status=none | xxd -s "$1" -l "$2" -p; }
s16() { local v=$((0x$1)); echo $((v >= 32768 ? v - 65536 : v)); }
```

`r1 285 2` prints the pressure (i16, 0.1 mbar), and `s16 $(r1 285 2)` prints it as a signed number. The offsets count the report ID byte as offset 0, as the kernel driver does. The device sends report 1 every 500 ms. `r1 765 4` prints `alarm_bits` and `r1 769 4` prints `alarm_bits_latched`. `HID` must hold one path: check `echo $HID`.

## Steps

1. Note the start time with `date '+%F %T' > ~/leakshield-check-start.txt`. Check the device state: `systemctl is-active nicecoolingd`, then `for p in Serial Firmware Structure Connected Writable ReadOnlyReason Mode RunningAction; do busctl get-property $B $LS $B.Leakshield $p; done`. Expected: `active`, `s "20417-31072"`, `q 1022`, `q 100`, `b true`, `b true`, `s ""`, the Mode of `default_mode` and `s ""`. The reference device has `default_mode` RELEASE, so it prints `s "release"`. Stop the check when `Writable` is false.
2. Check that the daemon reads all settings: `busctl call $B $LS $B.Leakshield GetFields | awk '{print $2}'`, and `rg -c '^\[\[field\]\]' crates/nicecooling-protocol/registry/leakshield.toml`. Expected: both commands print 94. No call fails.
3. Compare the kernel driver, the daemon and the device on report 1: `HW=$(hwof leakshield); for i in 1 2 3 4 5; do echo "kernel $(cat $HW/fan1_input) ubar, daemon $(busctl get-property $B $R/sensor/${LSID}_pressure $B.Sensor Value), device $(s16 $(r1 285 2)) x 0.1 mbar"; sleep 2; done`. Expected: in each line the three values agree: kernel ubar / 1000 = daemon mbar = device value x 0.1, with at most one count (0.1 mbar) of difference because the reads are 1 s apart.
4. Check the Feed while the daemon runs: `sudo systemctl start nicecoolingd`, wait 10 s, then `HW=$(hwof leakshield); NCT=$(hwof nct6798); for i in 1 2 3 4 5; do echo "fan2 $(cat $HW/fan2_input) tach $(cat $NCT/fan6_input), fan3 $(cat $HW/fan3_input) dL/h, flow $(busctl get-property $B $R/sensor/loop_1_flow $B.Sensor Value)"; sleep 2; done`. Expected: `fan2_input` equals the `fan6_input` of `nct6798` (rpm), and `fan3_input` equals 10 x the flow of `loop_1_flow` (L/h), each within the change of one second. Both are numbers, not ENODATA.
5. Record the pressure with the Feed: `echo "pressure $(r1 285 2) pump_head_pressure $(r1 289 2) sensor4 $(r1 101 2) sensor5 $(r1 111 2)"`. Expected: `sensor4` equals the rpm of step 4 in hex, `sensor5` equals 10 x the flow in hex, and `pump_head_pressure` is not `0000`. Write the four values down. The Leakshield computes the pressure from the Feed when `pump_head_pressure` changes in step 6.
6. Stop the Feed and record what the device holds. Run `date +%s > ~/feed-stop-start.txt; sudo systemctl stop nicecoolingd; HW=$(hwof leakshield); for i in $(seq 1 300); do echo "$(date +%s) fan2=$(cat $HW/fan2_input 2>&1) fan3=$(cat $HW/fan3_input 2>&1) s4=$(r1 101 2) s5=$(r1 111 2) head=$(r1 289 2)"; sleep 2; done | tee ~/feed-stop.log`. This takes 10 min. Then `sudo systemctl start nicecoolingd`. Expected: the log shows for sensors 4 and 5 the value right after the stop, the value at the end, and the second where it changes (or "unchanged for 10 min"). The `head=` column shows `pump_head_pressure` with no Feed. After the start, `fan2_input` shows a number again within 10 s. Write the three facts in the record: the held value, the duration, the end value.
7. Take the first backup: `A=$($BC call $B $LS $B.Leakshield CreateBackup | cut -d'"' -f2); echo $A; sudo ls /var/lib/nicecooling/backups/$LSID/$A; sudo jq -r .reason /var/lib/nicecooling/backups/$LSID/$A/meta.json`. Expected: the id has the form `yyyymmddThhmmssZ-<12 hex>`. The directory holds `meta.json`, `report3.bin`, `report5.bin` and `report8.bin`. The reason is `manual`, or `first` when no backup of this serial existed. A device that equals an older backup returns the id of that backup.
8. Read the field, then write it with a value one second higher. Run `busctl call $B $LS $B.Leakshield GetFields | rg -o '"display_page_time" "low" d [0-9.]+'`. For the fw1022 dump it prints `d 10`. Then run `$BC call $B $LS $B.Leakshield SetFields 'a{sv}' 1 display_page_time d 11` and wait 6 s. Expected: the call returns with no error, the monitor prints no `WriteFailed`, and the same `GetFields` command now prints `d 11`. The daemon sent save-settings 4 s after the write.
9. Take the second backup and compare: `C=$($BC call $B $LS $B.Leakshield CreateBackup | cut -d'"' -f2); D=/var/lib/nicecooling/backups/$LSID; sudo cmp -l $D/$A/report3.bin $D/$C/report3.bin; sudo cmp $D/$A/report8.bin $D/$C/report8.bin; sudo cmp $D/$A/report5.bin $D/$C/report5.bin`. Expected: `cmp -l` lists only byte 65 (the low byte of `display_page_time`, offset 64 counted from 0) and the CRC bytes 749 and 750 (offsets 748 and 749), and a CRC byte appears only when its value changes. For the dump value the line of byte 65 reads `144 156` (octal 0x64 and 0x6E). `cmp` of `report8.bin` prints nothing. `report5.bin` is the screenshot of the display and can differ: record whether it does. Any other byte in `report3.bin` is a volatile byte: write its offset in the record.
10. Restore the first backup: `$BC call $B $LS $B.Leakshield RestoreBackup s $A`. polkit asks no password for a `wheel` member in the active session. Wait 6 s. Then `E=$($BC call $B $LS $B.Leakshield CreateBackup | cut -d'"' -f2); sudo cmp $D/$A/report3.bin $D/$E/report3.bin; busctl call $B $LS $B.Leakshield GetFields | rg -o '"display_page_time" "low" d [0-9.]+'`. Expected: the restore returns with no error. `cmp` prints nothing, so the device holds `report3.bin` of the first backup again, and `GetFields` prints `d 10` (the value of step 8 before the write).
11. Check the alarm settings, then set one current bit. Run `busctl call $B $LS $B.Leakshield GetFields | rg -o '"fill_level_warning_response_action" "safety" s [a-z-]+'` and `busctl call $B $R $B.Alarms GetResponses | rg -o '"leakshield.fill-level-low-warning" "[a-z]+" [0-9]+'`. Expected: the Device response is `none` and the alarm response is `notify`. Stop when it is not. Then cause bit 0 of `alarm_bits` and keep the cause. TODO(owner): no document names a cause that sets one current bit without a write of a Safety field. After the Verification write of task 5.13 passes for `fill_level_warning_threshold`, the owner can write a threshold above the current fill level with `SetFields`. Read both words with `echo "bits $(r1 765 4) latched $(r1 769 4)"` and `busctl get-property $B $R $B.Alarms Active`. Expected: bit 0 is set in both words (`00000001`), and `Active` holds `leakshield.fill-level-low-warning` with `bits` and `latched_bits` set.
12. Send the reset with the cause present: `$BC call $B $LS $B.Leakshield ResetAlarms`, wait 3 s, then `echo "bits $(r1 765 4) latched $(r1 769 4)"`. Expected: the call returns with no error and `alarm_bits` still reads `00000001`. Record `alarm_bits_latched`. When it reads `00000000`, the command clears all latched bits. When it reads `00000001`, the command clears only the bits that are clear in `alarm_bits`. The device may latch the bit again at once: record that too.
13. Remove the cause of the bit. Wait 5 s, then `echo "bits $(r1 765 4) latched $(r1 769 4)"` and `busctl get-property $B $R $B.Alarms Active`. Expected: `alarm_bits` reads `00000000` and `alarm_bits_latched` reads `00000001`. The Alarm instance stays in `Active` with `bits` 0 and `latched_bits` 1, because a latched bit keeps it open.
14. Send the reset with the latched bit set and `alarm_bits` clear: `$BC call $B $LS $B.Leakshield ResetAlarms`, wait 3 s, then `echo "bits $(r1 765 4) latched $(r1 769 4)"` and `busctl get-property $B $R $B.Alarms Active`. Expected: both words read `00000000`, and `Active` holds no `leakshield.` instance after the next report 1 (within 3 s).
15. Check the end state: `for p in Connected Writable Mode RunningAction; do busctl get-property $B $LS $B.Leakshield $p; done; busctl get-property $B $R $B.Alarms Active; journalctl -u nicecoolingd --since "$(cat ~/leakshield-check-start.txt)" | rg -i 'not saved|did not answer|gone|warning'`. Expected: `b true`, `b true`, the Mode of step 1, `s ""`, an empty `Active` array, and no line from `rg`.

## Recovery

- The device holds a wrong setting: `$BC call $B $LS $B.Leakshield RestoreBackup s $A`, then compare as in step 10.
- The daemon is stopped: `sudo systemctl start nicecoolingd`. Its stop restores the BIOS snapshot, so the fans are on their Hardware curves until it runs again.
- The Leakshield does not answer: unplug and plug the USB cable. A device in its bootloader needs `docs/hardware-checks/firmware-update.md`.
- A step 11 cause is still active: remove the cause, then run `ResetAlarms` as in step 14.

## Record

| Step | Result |
|---|---|
| 1 | |
| 2 | |
| 3 | |
| 4 | |
| 5 | |
| 6 | |
| 7 | |
| 8 | |
| 9 | |
| 10 | |
| 11 | |
| 12 | |
| 13 | |
| 14 | |
| 15 | |
