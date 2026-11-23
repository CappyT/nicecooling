# End-to-end check

This procedure runs the whole system on the reference desktop for one hour: the installed RPM, the Curves, the Feed message and a Profile switch. It passes when every step has a pass line and no Alarm rose. Each step has a pass line.

The check controls the real pump and the real fans, and it writes the Leakshield at each Profile switch. Read Safety first.

## Safety

- Do the check with no heavy load and with a person at the PC. The pump runs on a Curve with a Pump floor of 60%. Stop the daemon with `sudo systemctl stop nicecoolingd` when the water flow falls below 60% of its value in step 3. The stop restores the BIOS snapshot.
- A Profile switch writes the whole stored report 3 of the slot to the Leakshield, as stored (design D14). Run `docs/hardware-checks/profile-switch.md` before this check, so that the slots are known to hold the settings of the device.
- The default alarm responses stay as they are: a large leak and a pump stop power off the PC after 30 s. Save all work. Cancel a pending poweroff from the app (`docs/app.md`, Cancel a pending poweroff). An Alarm in this check fails it.

## Preconditions

- Fedora 45 on the reference desktop, with the Leakshield on its USB cable and the loop filled and leak-free.
- The RPM of task 9.1 is built and `docs/hardware-checks/package.md` passed. No file of a hand install exists (`docs/app.md`, section Remove).
- `/var/lib/nicecooling/` does not exist or is empty: `sudo systemctl stop nicecoolingd; sudo rm -rf /var/lib/nicecooling`.
- Every `pwmN_enable` of `nct6798` reads 5.
- The AppIndicator extension is on (`docs/app.md`, The AppIndicator extension).
- The function `snapcheck` of `docs/hardware-checks/fan-control.md`, section Check helper, is defined in the shell.
- `jq`, `xxd` and `rg` are installed. The in-tree driver gives the hwmon device `leakshield`. The out-of-tree driver is not loaded.
- A second terminal runs `busctl --system monitor io.github.cappyt.NiceCooling1 | tee ~/e2e-signals.txt` from step 5 to step 8.

## Shell variables

```bash
B=io.github.cappyt.NiceCooling1
R=/io/github/cappyt/NiceCooling1
LSID=leakshield_20417_31072
BC="busctl --allow-interactive-authorization=true"
RPM=$(ls $PWD/RPMS/*/nicecooling-*.rpm)
hwof() { for d in /sys/class/hwmon/hwmon*; do [ "$(cat $d/name)" = "$1" ] && echo $d; done; }
NCT=$(hwof nct6798)
HID=$(for h in /sys/class/hidraw/hidraw*; do p=$(readlink -f $h/device); echo "$p" | rg -q '0003:0C70:F014' && echo "$p" | rg -q ':1\.1/' && echo /dev/${h##*/}; done)
r1() { sudo dd if=$HID bs=1024 count=1 status=none | xxd -s "$1" -l "$2" -p; }
```

`r1 50 1` prints `active_profile` of report 1 as hex (`01` to `04`).

## Log script

Step 5 runs this script in a third terminal. It writes one line each minute for 60 minutes. Write it to `~/e2e-log.sh`.

```bash
#!/bin/bash
B=io.github.cappyt.NiceCooling1; R=/io/github/cappyt/NiceCooling1
hwof() { for d in /sys/class/hwmon/hwmon*; do [ "$(cat $d/name)" = "$1" ] && echo $d; done; }
NCT=$(hwof nct6798); LK=$(hwof leakshield); EC=$(hwof asusec)
for i in $(seq 0 60); do
  echo "min=$i $(date +%T) pwm6=$(cat $NCT/pwm6) tach6=$(cat $NCT/fan6_input) pwm1=$(cat $NCT/pwm1) water=$(busctl get-property $B $R/sensor/asusec_temp4 $B.Sensor Value) flow=$(busctl get-property $B $R/sensor/loop_1_flow $B.Sensor Value) feed2=$(cat $LK/fan2_input 2>&1) feed3=$(cat $LK/fan3_input 2>&1) alarms=$(busctl get-property $B $R $B.Alarms Active | cut -c1-40)"
  sleep 60
done
```

## Steps

1. Install and start: `sudo rpm -i $RPM; sudo systemctl enable --now nicecoolingd`, wait 10 s. Then `systemctl is-active nicecoolingd; busctl get-property $B $R $B.Manager Version; busctl get-property $B $R $B.Manager StateHealth; sudo ls /var/lib/nicecooling`. Expected: `rpm -i` exits 0, `active`, a version string, `s "ok"` or `s "empty"` for a new state, and `bios-snapshot.json` exists. `snapcheck` prints nothing: the daemon wrote no Channel yet.
2. Run the first-run flow of `docs/app.md` (First run) in the app: `nicecooling`. Choose the Pump channel `nct6798_pwm6`, the flow sensor `asusec_fan2` with the table `alphacool_17558`, the water temperature `asusec_temp4` and the Leakshield. Close the window and log out and in. Expected: `busctl get-property $B $R/loop/loop_1 $B.Loop Leakshield` prints the path of the Leakshield, `pgrep -a nicecooling` shows `nicecooling --background`, the tray icon is `nicecooling-symbolic`, and no Alarm shows.
3. Set the Curves with the app (Channels) or with `busctl`. The pump: `$BC call $B $R/channel/nct6798_pwm6 $B.Channel SetCurve 'oa(dd)dd' $R/sensor/asusec_temp4 2 20 60 50 100 5 5`. A case fan: `$BC call $B $R/channel/nct6798_pwm1 $B.Channel SetCurve 'oa(dd)dd' $R/sensor/k10temp_temp1 2 40 20 80 100 5 5`. Wait 60 s. Then `cat $NCT/pwm6 $NCT/pwm6_enable $NCT/pwm1 $NCT/pwm1_enable; busctl get-property $B $R/sensor/asusec_temp4 $B.Sensor Value; busctl get-property $B $R/sensor/k10temp_temp1 $B.Sensor Value; snapcheck`. Expected: `pwm6` equals `round(duty x 255 / 100)` for the duty of the pump Curve at the water temperature (60% at 20 C or below, 100% at 50 C or above). `pwm1` equals the same value for its Curve. `pwm6_enable` and `pwm1_enable` read 1. `snapcheck` lists a `DIFF` only for `pwm1`, `pwm1_enable`, `pwm6` and `pwm6_enable`. Values change by at most 5% each second, so wait until they settle.
4. Check the Feed: `LK=$(hwof leakshield); for i in 1 2 3; do echo "fan2 $(cat $LK/fan2_input) tach $(cat $NCT/fan6_input), fan3 $(cat $LK/fan3_input) dL/h, flow $(busctl get-property $B $R/sensor/loop_1_flow $B.Sensor Value)"; sleep 2; done; echo "pressure $(r1 285 2) pump_head_pressure $(r1 289 2) active_profile $(r1 50 1)"`. Expected: `fan2_input` equals the tach of `fan6_input`, `fan3_input` equals 10 x the flow of `loop_1_flow`, both within the change of one second. `pump_head_pressure` is not `0000`, and `active_profile` is `01`.
5. Create the Profile for the switch and start the hour. First `$BC call $B $R $B.Profiles CreateProfile so "E2E slot 2" $R/profile/profile_default`. The reply holds the path: use its last element as `PE`. Then `$BC call $B $R/profile/$PE $B.Profile SetSlot y 2`. Start the log: `date '+%F %T' | tee ~/e2e-start.txt; bash ~/e2e-log.sh | tee ~/e2e-log.txt` in the third terminal. Expected: `busctl get-property $B $R/profile/$PE $B.Profile Slot` prints `y 2`, and the log shows a first line with `alarms=a(ssssstta{sv}) 0`. The hour starts at the first line.
6. At minute 20, switch to the Profile: `$BC call $B $R $B.Profiles ActivateProfile o $R/profile/$PE`. Wait 10 s, then `busctl get-property $B $R $B.Profiles Active; busctl get-property $B $R $B.Profiles ActiveState; busctl get-property $B $R $B.Profiles ActivationCause; r1 50 1`. Expected: `Active` is the path of `$PE`, `ActiveState` is `s "applied"`, `ActivationCause` is `s "manual"`, `r1 50 1` prints `02`, the monitor prints `ProfileActivated`, and the Curves of step 3 still hold: `pwm6` is still on the pump Curve (the Profile was a copy of the Default Profile).
7. At minute 40, return to automatic: `$BC call $B $R $B.Profiles ReturnToAutomatic`. Wait 10 s, then run the same four reads as in step 6. Expected: `Active` is `profile_default`, `ActiveState` is `s "applied"`, `ActivationCause` is `s "default"`, `r1 50 1` prints `01`, and the Curves hold.
8. At minute 60 read the result. Run `rg -c 'AlarmRaised' ~/e2e-signals.txt; rg -c 'alarms=a\(ssssstta\{sv\}\) 0' ~/e2e-log.txt; rg -c 'feed2=[0-9]' ~/e2e-log.txt; journalctl -u nicecoolingd --since "$(cat ~/e2e-start.txt)" | rg -i 'warning|did not answer|not saved|gone|failed'; busctl get-property $B $R $B.Alarms Active; busctl get-property $B $R $B.Manager StateHealth`. Expected: the first command prints 0 or no line (no `AlarmRaised`), the second and the third print 61 (every line has an empty Alarm list and a Feed value), the journal command prints nothing, `Active` is empty, and `StateHealth` is `s "ok"`. The log shows `pwm6` between the Curve values for the water temperature.
9. Stop and restore: `sudo systemctl stop nicecoolingd; snapcheck; ls /var/lib/nicecooling/lease.json; cat $NCT/pwm6_enable $NCT/pwm1_enable`. Expected: `snapcheck` prints nothing, `lease.json` does not exist, and both `_enable` files read 5. Then start the daemon again: `sudo systemctl start nicecoolingd`, and `$BC call $B $R/profile/$PE $B.Profile Delete`.

## Recovery

- A Channel stays on a wrong value: `sudo systemctl stop nicecoolingd`. If it stays in manual mode: `echo 5 | sudo tee $NCT/pwmN_enable`.
- An Alarm shows: the check fails. Read `Active`, write down the instance, the event and the time, cancel the poweroff when one is pending (`$BC call $B $R $B.Alarms CancelPoweroff s <instance>`), and stop the check.
- The Leakshield holds a wrong slot: `$BC call $B $R $B.Profiles ReturnToAutomatic`. When the settings of the device are wrong, run `RestoreBackup` of the first backup (`docs/hardware-checks/leakshield.md`, Recovery).

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
