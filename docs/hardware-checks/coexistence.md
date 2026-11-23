# Coexistence check with aquasuite

This procedure checks that the daemon and aquasuite share the Leakshield in turns. The device goes to the Windows VM, and the daemon raises `leakshield-unreachable`. A setting changes in aquasuite. The device goes back to the host, and the daemon reports an external change and writes nothing to the device. Each step has a pass line.

The daemon logs no Leakshield write: it prints a line only for a failure. So a USB capture on the host is the record of the writes.

## Safety

- The leak protection is off while the device is in the VM (`docs/hardware-checks/firmware-update.md`, Preconditions). The loop is closed and no maintenance runs. Keep the time in the VM short.
- The alarm response of `leakshield-unreachable` must be "notify". With "poweroff" the PC powers off during the check. Step 1 checks it.
- Change one low-risk setting in aquasuite: the display page time. Do not change a Safety setting or the Mode. Do not start a firmware update in this check.
- Do not use SPICE USB redirection. Use the libvirt `hostdev` of step 3 (`docs/hardware-checks/firmware-update.md`, step 3).
- Take the backup of step 1 first. Step 8 restores it.

## Preconditions

- Fedora 45 on the reference desktop. The daemon runs. The Leakshield is on its USB cable, `Connected` and `Writable` are true, and no Alarm is active.
- The libvirt VM runs aquasuite X.96. Its name is `win11-aquasuite`. The VM `win11` passes only a Bluetooth adapter through. The VM is shut down or running, and the device is not in it.
- `virsh`, `rg`, `lsusb` and `jq` are installed. The kernel module `usbmon` loads: `sudo modprobe usbmon`.
- The in-tree driver gives the hwmon device `leakshield`. The out-of-tree driver is not loaded.

## Shell variables

```bash
B=io.github.cappyt.NiceCooling1
R=/io/github/cappyt/NiceCooling1
LSID=leakshield_20417_31072
LS=$R/leakshield/$LSID
BC="busctl --allow-interactive-authorization=true"
VM=win11-aquasuite
BUS=$(lsusb -d 0c70:f014 | awk '{print $2}' | sed 's/^0*//')
hwof() { for d in /sys/class/hwmon/hwmon*; do [ "$(cat $d/name)" = "$1" ] && echo $d; done; }
```

The USB capture uses the text interface of `usbmon` (kernel documentation, `Documentation/usb/usbmon.rst`). Field 2 of a line is the time in microseconds. A line with `S Co:` is a control write to the device. A line with `S Bo:` is a bulk write. TODO(owner): the format comes from the kernel documentation and has not been run on this host.

The device file of step 3, `~/leakshield-hostdev.xml`:

```xml
<hostdev mode='subsystem' type='usb' managed='yes'>
  <source>
    <vendor id='0x0c70'/>
    <product id='0xf014'/>
  </source>
</hostdev>
```

## Steps

1. Take the backup and start the watchers. Run `S0=$($BC call $B $LS $B.Leakshield CreateBackup | cut -d'"' -f2); echo $S0; busctl call $B $R $B.Alarms GetResponses | rg -o '"leakshield-unreachable" "[a-z]+" [0-9]+'; busctl get-property $B $LS $B.Leakshield Connected; busctl get-property $B $R $B.Alarms Active`. In a second terminal run `stdbuf -oL busctl --system monitor $B | while IFS= read -r l; do printf '%s %s\n' "$(date +%s.%N)" "$l"; done | tee ~/coex-signals.txt`. In a third terminal run `sudo timeout 1200 cat /sys/kernel/debug/usb/usbmon/${BUS}u > ~/usbmon.txt`. Expected: the backup id has the form `yyyymmddThhmmssZ-<12 hex>`, the response reads `"notify" 0`, `Connected` is `b true`, and `Active` is empty. When the monitor prints its lines in batches, read the `Timestamp` field of each message instead of the first column.
2. Check that the VM is ready: `virsh list --all | rg $VM; virsh dumpxml $VM | rg -c hostdev`. If the VM is shut down: `virsh start $VM`, and wait until Windows runs. Expected: the VM is listed as `running`, and the `rg -c` command prints 0 or reports no match (the device is not in the VM).
3. Pass the device to the VM: `date +%s.%N; virsh attach-device $VM ~/leakshield-hostdev.xml --live`. Wait 20 s. Then `journalctl -k -o short-unix | rg 'USB disconnect' | tail -n 1; busctl get-property $B $LS $B.Leakshield Connected; busctl get-property $B $R $B.Alarms Active; journalctl -u nicecoolingd -n 10`. Expected: `virsh` prints `Device attached successfully`. The kernel log shows the `USB disconnect` of the Leakshield port. `Connected` is `b false`. `Active` holds `leakshield-unreachable` with the source `leakshield_20417_31072` and the response `notify`, raised less than 11 s after the `USB disconnect` line. The journal holds a line `the Leakshield at <path> is gone`. The monitor printed `AlarmRaised`. No poweroff is pending (the poweroff time reads 0).
4. In the VM, start aquasuite and let it find the Leakshield. Change the display page time on the display page, for example from 10 s to 12 s, and save the settings. TODO(owner): name the menu of aquasuite that holds the page time. Write down the field, the old value and the new value. Do not change another setting. Close aquasuite and wait 10 s: aquasuite sends its save-settings command 4 s after the write. Expected: aquasuite shows the device and the new value after a restart of the page, and `busctl get-property $B $R $B.Alarms Active` on the host still holds `leakshield-unreachable`. The PC does not power off.
5. Give the device back to the host: `N=$(wc -l < ~/usbmon.txt); echo $N; virsh detach-device $VM ~/leakshield-hostdev.xml --live; date +%s.%N`. Wait 30 s. Then `journalctl -k -o short-unix | rg 'idVendor=0c70, idProduct=f014' | tail -n 1; rg ExternalChange ~/coex-signals.txt; busctl get-property $B $LS $B.Leakshield Connected`. Expected: the kernel log shows `New USB device found` of the Leakshield (call its Unix time `T0`). The monitor printed `ExternalChange` with the field `display_page_time` less than 10 s after `T0`. `Connected` is `b true`. Write the time difference in the record. The app shows the notification "display_page_time changed outside nicecooling" when the app runs and "External change" is on in Settings.
6. Check that the daemon wrote nothing to the device after the reattach. Run `DEV2=$(lsusb -d 0c70:f014 | awk '{print $4}' | tr -d :); tail -n +$((N+1)) ~/usbmon.txt | rg -e " S Co:$BUS:$DEV2:0 s 21 09 " -e " S Io:$BUS:$DEV2:"; journalctl -u nicecoolingd --since "-10min" | rg -i 'not saved|did not answer|failed'`. Expected: the first `rg` prints no line, so the daemon sent no SET_REPORT and no interrupt write, and no write of report 3 (`0303`) or report 2. A line with the command `set_clock` (report 2, command 37000) shows only when the device clock differs from the PC clock by more than 60 s: write it in the record if it shows (design D12). The bulk writes `S Bo:$BUS:$DEV2:2` of the Feed are expected and are not a settings write. The journal command prints no line.
7. Check that the Alarm closes: wait until 30 s after `T0`, then `busctl get-property $B $R $B.Alarms Active; LK=$(hwof leakshield); cat $LK/fan2_input $LK/fan3_input`. Expected: `Active` is empty, the monitor printed `AlarmCleared`, and both `cat` calls print numbers. The Alarm closes after report 1 arrives and the Feed succeeds for 10 s in a row. Write the seconds from `T0` to `AlarmCleared`.
8. Restore the settings of the device: `$BC call $B $LS $B.Leakshield RestoreBackup s $S0`. Enter the administrator password when polkit asks. Wait 6 s. Then `busctl call $B $LS $B.Leakshield GetFields | rg -o '"display_page_time" "low" d [0-9.]+'; rg -c ExternalChange ~/coex-signals.txt`. Expected: the call returns with no error, the field has its old value again, and the number of `ExternalChange` signals did not grow (the daemon does not report its own write). Stop the three watchers (`Ctrl+C`) and shut down the VM if you started it: `virsh shutdown $VM`.

## Recovery

- The device does not come back on the host: `virsh detach-device $VM ~/leakshield-hostdev.xml --live`, then unplug and plug the USB cable. Watch `journalctl -kf` for `New USB device found`. Then `sudo systemctl restart nicecoolingd`.
- The device is in its bootloader (`bcdDevice` 2.00, a short report 1): follow `docs/hardware-checks/firmware-update.md`, section Bootloader state.
- The device holds the aquasuite value: `$BC call $B $LS $B.Leakshield RestoreBackup s $S0`.
- A poweroff is pending: cancel it in the app or with `$BC call $B $R $B.Alarms CancelPoweroff s <instance>`.

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
