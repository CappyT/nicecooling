# Alarm check

This procedure checks the Alarms on the reference desktop: the pump stop, the cancel of a pending poweroff, `leakshield-unreachable`, and the forced poweroff when the shutdown stalls. Each step has a pass line.

The test Loop uses a spare fan Channel with its fan unplugged as the Pump channel, so no real pump is touched. The check powers off the desktop in step 7.

## Safety

- Step 7 powers off the PC. Save all work and close all programs before it.
- A pump stop sets every Channel to 100%, the Channels on their Hardware curve included. All fans run at full speed while the Alarm is active. The real pump runs at 100% too.
- The default alarm response of `pump-stop` is "poweroff after 30 s". Step 1 sets it to "notify" and step 9 sets it back. A step that needs a poweroff sets the delay itself.
- Step 6 starts a unit that stalls the shutdown. Step 9 removes it. Without step 9 every later shutdown of this PC waits for it.
- The test Loop stays in `state.json` and the fan stays unplugged during the reboot. The daemon would raise the pump stop again after the boot and power off the PC again. Step 7 disables the unit `nicecoolingd` before the poweroff. Step 9 enables it again.
- Keep the real Loop `loop_1` as it is. Do not touch the real Leakshield cable before step 5.

## Preconditions

- Fedora 45 on the reference desktop. The package files are installed (`docs/app.md`, Install). The unit `nicecoolingd` runs. The Loop `loop_1` exists.
- A spare fan Channel of `nct6798`. Its fan header has a fan that you can unplug. This procedure uses `nct6798_pwm4` (Chassis Fan 3) as an example. TODO(owner): name the Channel that is spare on the reference Board. Set `SPARE` and `SPARENUM` below to it.
- The journal is persistent: `ls /var/log/journal` lists a directory, and `journalctl --list-boots` lists more than one boot. If not: `sudo mkdir -p /var/log/journal && sudo systemctl restart systemd-journald`.
- A second terminal runs `busctl --system monitor io.github.cappyt.NiceCooling1`. It prints the signals of the Alarms interface.
- The logind inhibit delay has its default: `systemd-analyze cat-config systemd/logind.conf | rg InhibitDelayMaxSec` prints nothing, or `5`.

## Shell variables

```bash
B=io.github.cappyt.NiceCooling1
R=/io/github/cappyt/NiceCooling1
LSID=leakshield_20417_31072
BC="busctl --allow-interactive-authorization=true"
SPARE=nct6798_pwm4
SPARENUM=4
hwof() { for d in /sys/class/hwmon/hwmon*; do [ "$(cat $d/name)" = "$1" ] && echo $d; done; }
NCT=$(hwof nct6798)
```

`busctl get-property $B $R $B.Alarms Active` prints each Alarm as: instance id, event id, severity, source, response, raised time in microseconds, poweroff time in microseconds (0 when none), details.

## Stall unit

The unit holds the shutdown for 300 s. Without it, the shutdown ends in a few seconds and the forced poweroff of `poweroff.target` never shows. Write it in step 6 to `/etc/systemd/system/nicecooling-stall.service`:

```ini
[Unit]
Description=Stall the shutdown for the nicecooling alarm check

[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/usr/bin/true
ExecStop=/usr/bin/sleep 300
TimeoutStopSec=400
```

## Steps

1. Create the test Loop. Run `$BC call $B $R $B.Manager CreateLoop s "Alarm test"`. The reply holds the path of the Loop: use its last element as `TL` (for example `loop_2`). Then `$BC call $B $R/loop/$TL $B.Loop SetPumpChannels ao 1 $R/channel/$SPARE`, `$BC call $B $R $B.Alarms SetResponse ssu pump-stop notify 0` and `busctl call $B $R $B.Alarms GetResponses | rg -o '"pump-stop" "[a-z]+" [0-9]+'`. Write the old response down before the change (the default is `poweroff` 30). Make sure that the fan of the spare Channel is unplugged: `cat $NCT/fan${SPARENUM}_input`. Expected: `GetResponses` prints `"pump-stop" "notify" 0`, the Loop holds the spare Channel (`busctl get-property $B $R/loop/$TL $B.Loop PumpChannels`), and the tach reads 0.
2. Raise the pump stop. Run `t0=$(date +%s%6N); echo $t0; $BC call $B $R/channel/$SPARE $B.Channel SetFixedDuty d 50`, wait 8 s, then `busctl get-property $B $R $B.Alarms Active`. Expected: `Active` holds one `pump-stop` with the source of the test Loop and the response `notify`. The raised time minus `t0` is less than 6 000 000 microseconds. The monitor prints `AlarmRaised`. All fans run at 100%. Write the difference in the record.
3. Close the pump stop. Plug the fan into the spare header. Wait 15 s. Then `busctl get-property $B $R $B.Alarms Active`. Expected: `Active` holds no `pump-stop`, the monitor printed `AlarmCleared` after at least 10 s with a tach above 0 (the Alarm closes when every Pump channel reads above 0 rpm for 10 s), and the other Channels leave 100%.
4. Cancel a poweroff during the delay. Run `$BC call $B $R $B.Alarms SetResponse ssu pump-stop poweroff 120`, unplug the fan, and wait 10 s. Run `busctl get-property $B $R $B.Alarms Active` and note the instance id (`alarm_<start>_<n>`) as `INST` and the poweroff time. Within 60 s, run `$BC call $B $R $B.Alarms CancelPoweroff s $INST`. Wait until 130 s after the raised time. Expected: the call returns with no error, the monitor prints `PoweroffCancelled`, and the PC stays on (`uptime -s` is unchanged). The Alarm stays in `Active` and its poweroff time reads 0 or the entry shows no pending poweroff.
5. Raise `leakshield-unreachable`. First `$BC call $B $R $B.Alarms SetResponse ssu pump-stop notify 0` and plug the spare fan in, so that the pump stop is closed. Check `busctl call $B $R $B.Alarms GetResponses | rg -o '"leakshield-unreachable" "[a-z]+" [0-9]+'` prints `"notify" 0`. Unplug the USB cable of the Leakshield. After 20 s run `journalctl -k -o short-unix | rg 'USB disconnect' | tail -n 1` and `busctl get-property $B $R $B.Alarms Active`. Expected: `Active` holds `leakshield-unreachable` with the source `leakshield_20417_31072`. The raised time minus the Unix time of the `USB disconnect` line is less than 11 s. Plug the cable and write down the seconds from the plug to `AlarmCleared`. The Alarm closes after report 1 arrives and the Feed succeeds for 10 s in a row, so the time is 10 s or more.
6. Prepare the stalled shutdown. Write the unit of section Stall unit, then `sudo systemctl daemon-reload && sudo systemctl start nicecooling-stall.service`. Run `$BC call $B $R $B.Alarms SetResponse ssu pump-stop poweroff 60`. Save your work. Run `sudo systemctl disable nicecoolingd`. Plug the fan in, so that no Alarm is active. Expected: `systemctl is-active nicecooling-stall.service` prints `active`, `systemctl is-enabled nicecoolingd` prints `disabled`, `systemctl show -p JobTimeoutUSec -p JobTimeoutAction poweroff.target` prints `JobTimeoutUSec=55s` and `JobTimeoutAction=poweroff-force`, and `busctl get-property $B $R $B.Alarms Active` shows no Alarm.
7. Power off the PC with the Alarm. Run `date +%s%6N | tee ~/alarm-t0.txt; sync`. Unplug the fan. Wait. Within 15 s run `busctl get-property $B $R $B.Alarms Active | tee ~/alarm-active.txt; sync`. Write on paper the instance id, the raised time and the poweroff time (raised time + 60 s). Do nothing else. Expected: at the poweroff time the daemon calls logind, the shutdown stalls on `nicecooling-stall.service`, and the PC powers off by itself with no input. Write the wall clock time at which the screen and the fans go dark.
8. Read the journal of the next boot. Plug the fan in before the boot. After the boot run `journalctl -b -1 -o short-precise | rg -i 'logind|poweroff.target|powering|forcibly' | tail -n 20` and `journalctl -b -1 -o short-precise -n 1`. Compare with `~/alarm-active.txt`: the poweroff time is the end of the alarm delay. Expected: a line from `systemd-logind` shows the time when logind queued the job. A line about the job `poweroff.target` timing out shows 55 s after that time (within 2 s). Then the shutdown ends. Record: the end of the alarm delay, the time logind queued the job, the time of the timeout line, the time of the last line of the previous boot, and the total from the end of the alarm delay to the last line. The goal is 60 s. It is not a pass bound, because `systemd-shutdown` waits for the processes after SIGTERM and SIGKILL.
9. Clean up. Run `sudo systemctl stop nicecooling-stall.service; sudo rm /etc/systemd/system/nicecooling-stall.service; sudo systemctl daemon-reload; sudo systemctl enable --now nicecoolingd`. Then `$BC call $B $R/loop/$TL $B.Loop Delete`, `$BC call $B $R $B.Alarms SetResponse ssu pump-stop poweroff 30` (the old response of step 1) and `$BC call $B $R/channel/$SPARE $B.Channel ReleaseToHardware`. Expected: `systemctl is-active nicecoolingd` prints `active`, `ls /etc/systemd/system/nicecooling-stall.service` fails, `busctl get-property $B $R $B.Alarms Active` shows no Alarm, and `busctl call $B $R $B.Alarms GetResponses` shows the old responses. The spare fan header holds a fan again.

## Recovery

- A countdown runs that you do not want: `$BC call $B $R $B.Alarms CancelPoweroff s $INST`, or open the app and use "Cancel poweroff". After the logind call started, the PC powers off.
- The PC does not start the daemon after the boot: this is the intent of step 6. Run step 9.
- A stall unit is still active: `sudo systemctl stop nicecooling-stall.service`. The stop takes 300 s at most.
- The daemon fails: `sudo systemctl reset-failed nicecoolingd; sudo systemctl start nicecoolingd`. The stop of the daemon restored the BIOS snapshot, so the Channels are on their Hardware curves in the meantime.

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
