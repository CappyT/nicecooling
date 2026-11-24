# Profile switch check

This procedure checks the Profile switch on the reference Leakshield. It counts the writes that the daemon sends to the device with a USB capture on the host. It checks the slot rule, the save-settings command 4 s after a write, the rate limit and the missing read-back. Each step has a pass line.

The daemon logs no Leakshield write: it prints a line only for a failure. So the journal cannot show a write or a read-back. The USB capture is the only record of them.

## Safety

- A switch to another slot writes the whole stored report 3 of that slot to the Leakshield, as stored. It also writes while an Action runs and while the Mode is RELEASE (design D14). The loop is filled and leak-free and no Action runs. The Mode belongs to the Profile (design D14): the device and the four slots are in RELEASE, so a switch does not change the Mode and steps 1, 4 and 10 compare equal reports. Step 11 checks the Mode of a Profile.
- Each slot must hold a copy of the device settings. Step 1 checks it. A slot that holds other settings (for example Safety fields that you changed) writes them to the device at the switch.
- Take the backup of step 1 first. Recovery restores it.
- The check ends on the Default Profile (slot 1).

## Preconditions

- Fedora 45 on the reference desktop. The daemon runs. The Leakshield is on its USB cable and `Writable` is true.
- The first start created `profile_default` (slot 1), `profile_2` (slot 2), `profile_3` (slot 3) and `profile_4` (slot 4). The Default Profile is active. A state written by hand has only `profile_default`: create the three Profiles with `CreateProfile` and `SetSlot`, and use their object names in place of `profile_2` to `profile_4`.
- The app does not run (`pkill -x nicecooling`): when `ActiveSlot` changes it calls `GetFields`, and these reads of report 3 come within 1 s of a write, so step 9 cannot tell them from a read-back.
- `rg`, `xxd` and `lsusb` are installed. The kernel module `usbmon` loads: `sudo modprobe usbmon`. debugfs is mounted: `ls /sys/kernel/debug/usb/usbmon` lists files.
- No other program uses the Leakshield: aquasuite does not run, and the VM does not hold the device.
- A second terminal runs `busctl --system monitor io.github.cappyt.NiceCooling1` for the whole check. It prints `ProfileActivated`.

## Shell variables

```bash
B=io.github.cappyt.NiceCooling1
R=/io/github/cappyt/NiceCooling1
LSID=leakshield_20417_31072
LS=$R/leakshield/$LSID
BC="busctl --allow-interactive-authorization=true"
D=/var/lib/nicecooling
HID=$(for h in /sys/class/hidraw/hidraw*; do p=$(readlink -f $h/device); echo "$p" | rg -q '0003:0C70:F014' && echo "$p" | rg -q ':1\.1/' && echo /dev/${h##*/}; done)
r1() { sudo dd if=$HID bs=1024 count=1 status=none | xxd -s "$1" -l "$2" -p; }
BUS=$(lsusb -d 0c70:f014 | awk '{print $2}' | sed 's/^0*//')
DEV=$(lsusb -d 0c70:f014 | awk '{print $4}' | tr -d : )
SETF=" S Co:$BUS:$DEV:0 s 21 09 0303 "
GETF=" S Ci:$BUS:$DEV:0 s a1 01 0303 "
SAVE="= 02000000 02000000 0034c6"
```

The USB capture reads the text interface of `usbmon` (kernel documentation, `Documentation/usb/usbmon.rst`). Field 2 of a line is the time in microseconds. A control write of report 3 is a SET_REPORT: type `21 09`, value `0303` (feature report, id 3), length `02ee` (750). A read of report 3 is a GET_REPORT: `a1 01 0303`. The save-settings command is the report 2 payload `02 00000002 00000000 34C6`. The HID interface has no interrupt OUT endpoint, so the kernel sends it as a control write: `S Co:1:013:0 s 21 09 0202 0001 000b 11 = 02000000 02000000 0034c6`. `$SAVE` matches it. The filters match the capture of the reference host (2026-11-24, device 013): `S Co:1:013:0 s 21 09 0303 0001 02ee 750` for a write of report 3 and `S Ci:1:013:0 s a1 01 0303 0001 02ee 750` for a read.

Report 1 `active_profile` is at offset 50: `r1 50 1` prints `01` to `04`.

Each step reads only the capture lines after its action: `N=$(wc -l < ~/usbmon.txt)` before the action, then `tail -n +$((N+1)) ~/usbmon.txt`.

## Steps

1. Take the first backup and check the slots. Run `S0=$($BC call $B $LS $B.Leakshield CreateBackup | cut -d'"' -f2); echo $S0`. Then for each of the slots 2, 3 and 4: `sudo cmp -l $D/leakshield/$LSID/slot-1/report3.bin $D/leakshield/$LSID/slot-N/report3.bin`. Then `for p in profile_default profile_2 profile_3 profile_4; do busctl get-property $B $R/profile/$p $B.Profile Slot; done; r1 50 1`. Expected: each `cmp -l` lists only the byte 748 (the device profile number, offset 747) and the CRC bytes 749 and 750, so the slots hold the settings of the device. The slots read `y 1`, `y 2`, `y 3` and `y 4`, and `r1 50 1` prints `01`.
2. Create the third Profile that shares slot 1, and start the capture. Run `$BC call $B $R $B.Profiles CreateProfile so "Slot one copy" $R/profile/profile_default`. The reply holds the path: use its last element as `PC`. In a third terminal run `sudo modprobe usbmon; sudo timeout 900 cat /sys/kernel/debug/usb/usbmon/${BUS}u > ~/usbmon.txt`. Wait 12 s, then `rg -c "$GETF" ~/usbmon.txt` and `rg "$GETF" ~/usbmon.txt | head -n 2`. Expected: `busctl get-property $B $R/profile/$PC $B.Profile Slot` prints `y 1`. The capture holds one `GET_REPORT` of report 3 about every 10 s (the external change poll of the daemon), so the count is 1 or more and the two lines show the filter works. Look at them and correct `GETF`, `SETF` and `SAVE` if needed.
3. Switch to the Profile with the other slot (`profile_2`, slot 2): `N=$(wc -l < ~/usbmon.txt); $BC call $B $R $B.Profiles ActivateProfile o $R/profile/profile_2; sleep 8; tail -n +$((N+1)) ~/usbmon.txt | rg -e "$SETF" -e "$SAVE"; r1 50 1`. Expected: the capture lists exactly one write of report 3 (`$SETF`, length 750) and exactly one save-settings line. The time of the save-settings line (field 2) is 4 s after the write, with a difference of less than 0.2 s. `r1 50 1` prints `02`. The monitor prints `ProfileActivated`, and `busctl get-property $B $R $B.Profiles ActiveState` prints `s "applied"`.
4. Check the content of the write. Run `S1=$($BC call $B $LS $B.Leakshield CreateBackup | cut -d'"' -f2); sudo cmp -l $D/backups/$LSID/$S0/report3.bin $D/backups/$LSID/$S1/report3.bin`. Expected: `cmp -l` lists only byte 748 (the octal values `1 2`: slot 1 became slot 2) and the CRC bytes 749 and 750. The device holds the stored report 3 of slot 2: it equals the first report of the device but for the profile number.
5. Activate the same Profile again: `N=$(wc -l < ~/usbmon.txt); $BC call $B $R $B.Profiles ActivateProfile o $R/profile/profile_2; sleep 10; tail -n +$((N+1)) ~/usbmon.txt | rg -e "$SETF" -e "$SAVE"; r1 50 1`. Expected: the `rg` command prints no line, so the daemon wrote nothing. `r1 50 1` prints `02`.
6. Switch back to the Default Profile (slot 1): `N=$(wc -l < ~/usbmon.txt); $BC call $B $R $B.Profiles ActivateProfile o $R/profile/profile_default; sleep 8; tail -n +$((N+1)) ~/usbmon.txt | rg -e "$SETF" -e "$SAVE"; r1 50 1`. Expected: one write of report 3, one save-settings line 4 s later (less than 0.2 s of difference), and `r1 50 1` prints `01`.
7. Switch between the two Profiles with the same slot, and back: `for p in $PC profile_default; do N=$(wc -l < ~/usbmon.txt); $BC call $B $R $B.Profiles ActivateProfile o $R/profile/$p; sleep 10; tail -n +$((N+1)) ~/usbmon.txt | rg -e "$SETF" -e "$SAVE"; busctl get-property $B $R $B.Profiles Active; busctl get-property $B $R $B.Profiles ActiveState; r1 50 1; done`. Expected: the `rg` command prints no line in both rounds. `Active` is the path of `$PC` in the first round and of `profile_default` in the second, `ActiveState` is `s "applied"`, and `r1 50 1` prints `01` each time.
8. Check the rate limit. Wait 10 s so that the last profile write is more than 2 s old. Then run the three calls in one command: `N=$(wc -l < ~/usbmon.txt); for p in profile_2 profile_3 profile_4; do $BC call $B $R $B.Profiles ActivateProfile o $R/profile/$p; done; sleep 10; tail -n +$((N+1)) ~/usbmon.txt | rg -e "$SETF" -e "$SAVE"; r1 50 1`. The first call writes at once. The two others come within 2 s of that write. Expected: the capture lists exactly two writes of report 3 and one save-settings line. The second write comes 1.5 s after the first one (less than 0.2 s of difference), so only the newest request (slot 4) was written and the request for slot 3 was dropped. The save-settings line comes 4 s after the second write. `r1 50 1` prints `04`.
9. Check that no write has a read-back. Run `awk -v s="$SETF" -v g="$GETF" 'index($0,s){w=$2} index($0,g){ if (w && $2-w < 1000000) print "READBACK", $2-w }' ~/usbmon.txt`. Then `journalctl -u nicecoolingd --since "-30min" | rg -i 'not saved|did not answer|failed'`. Expected: the `awk` command prints no line: no read of report 3 starts less than 1 s after a write. A line that shows up can be the 10 s poll: repeat step 8 once and compare before you fail the step. The journal command prints no line, because the daemon logs nothing for a successful write and nothing for a read-back.
10. Return to the Default Profile and compare with the first backup. Run `N=$(wc -l < ~/usbmon.txt); $BC call $B $R $B.Profiles ReturnToAutomatic; sleep 8; tail -n +$((N+1)) ~/usbmon.txt | rg -e "$SETF" -e "$SAVE"; r1 50 1; S2=$($BC call $B $LS $B.Leakshield CreateBackup | cut -d'"' -f2); sudo cmp $D/backups/$LSID/$S0/report3.bin $D/backups/$LSID/$S2/report3.bin`. Expected: one write of report 3 and one save-settings line 4 s later, `r1 50 1` prints `01`, and `cmp` prints nothing. Stop the capture (`Ctrl+C` in the third terminal). Delete the third Profile: `$BC call $B $R/profile/$PC $B.Profile Delete`.
11. Check the Mode of a Profile. Run `$BC call $B $LS $B.Leakshield SetMode s shield` (polkit asks no password) and wait until `busctl get-property $B $LS $B.Leakshield Mode` prints `s "shield"` and `RunningAction` prints `s ""`. Then `for n in 1 2 3 4; do sudo xxd -s 724 -l 1 -p $D/leakshield/$LSID/slot-$n/report3.bin; done`. Switch to `profile_2`, wait 8 s and read `Mode`. Switch to `profile_default`, wait until `Mode` prints `s "shield"` and no Action runs, and read the four slots again. Run `$BC call $B $LS $B.Leakshield SetMode s release`, wait until `Mode` prints `s "release"`, and read slot 1 again. Last, `S3=$($BC call $B $LS $B.Leakshield CreateBackup | cut -d'"' -f2); sudo cmp $D/backups/$LSID/$S0/report3.bin $D/backups/$LSID/$S3/report3.bin`. Expected: after SetMode shield the slots read `01 03 03 03`. Slot 2 gives `s "release"`. The switch back runs `measure-fill-level` and gives `s "shield"`, and the slots still read `01 03 03 03`. After SetMode release slot 1 reads `03`. `cmp` prints nothing.

## Recovery

- The device holds a wrong slot or wrong settings: `$BC call $B $LS $B.Leakshield RestoreBackup s $S0` (polkit asks no password), then check `r1 50 1`. The restore writes report 3 of the backup, which has the profile number 1, and report 8 of the backup when the device holds another one.
- A Profile stays active that you did not want: `$BC call $B $R $B.Profiles ReturnToAutomatic`.
- The capture grows: it is limited to 900 s by `timeout`. Delete `~/usbmon.txt` after the record.
- The Leakshield does not answer: unplug and plug the USB cable, then `sudo systemctl restart nicecoolingd`.

## Record

| Step | Result |
|---|---|
| 1 | 2026-11-24, pass: slots 2-4 differ from slot 1 only at byte 748 and the CRC, slot 1 equals S0, `01`. |
| 2 | 2026-11-24, pass: the filters match the capture of device 013. |
| 3 | 2026-11-24, pass with a deviation: one write and one save-settings, `02`. The save came 4.286 s after the write (expected 4 s +- 0.2). It was the first write after a restart of the daemon. Cause not measured; accepted by the owner. |
| 4 | 2026-11-24, pass: only byte 748 and the CRC. |
| 5 | 2026-11-24, pass: no write, `02`. |
| 6 | 2026-11-24, pass: one write, save 4.016 s later, `01`. |
| 7 | 2026-11-24, pass: no write in both rounds, `applied`, `01`. |
| 8 | 2026-11-24, pass: writes at 0 s and 1.501 s, save 4.016 s after the second, `04`. |
| 9 | 2026-11-24, pass: one read 0.181 s after a write, at 80.001 s on the grid of the 10 s poll. No journal line. |
| 10 | 2026-11-24, pass: one write, save 4.026 s later, `01`, S2 equals S0. |
| 11 | 2026-11-24, pass: SHIELD after 27 s, slots `01 03 03 03`, slot 2 gives RELEASE, back on slot 1 SHIELD after 18 s, release gives slot 1 `03`, S3 equals S0. |
