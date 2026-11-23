# Package check

This procedure checks the RPM `nicecooling` on the reference Board. The first daemon start takes the BIOS snapshot before any Channel write. The removal stops the service first, and the Channels read their snapshot attributes after it. Each step has a pass line.

The check writes `pwmN` attributes of one Channel. Do it with no load on the loop.

## Preconditions

- Fedora 45 on the reference Board. The RPM of task 9.1 is built: `ls RPMS/*/nicecooling-*.rpm` lists one file. Set `RPM` to its path.
- No hand install: none of the files of `docs/app.md`, section Install, exist. If they do, run section Remove of `docs/app.md` first. `rpm -q nicecooling` prints `package nicecooling is not installed`.
- `/var/lib/nicecooling/` does not exist or is empty: `sudo systemctl stop nicecoolingd; sudo rm -rf /var/lib/nicecooling`.
- Every `pwmN_enable` of `nct6798` reads 5 (`docs/hardware-checks/fan-control.md`, Preconditions).
- `jq` is installed.
- The function `snapcheck` of `docs/hardware-checks/fan-control.md`, section Check helper, is defined in the shell. It prints one `DIFF` line for each attribute that differs from the snapshot file.
- The SELinux mode is enforcing: `getenforce` prints `Enforcing`.
- The Leakshield may stay on its cable. This check does not use it. The daemon then also reads it, and that is not a part of this check.

## Shell variables

```bash
B=io.github.cappyt.NiceCooling1
R=/io/github/cappyt/NiceCooling1
BC="busctl --allow-interactive-authorization=true"
RPM=$(ls $PWD/RPMS/*/nicecooling-*.rpm)
hwof() { for d in /sys/class/hwmon/hwmon*; do [ "$(cat $d/name)" = "$1" ] && echo $d; done; }
NCT=$(hwof nct6798)
```

## Steps

1. Record the state of the Channels before the install: `grep -r . $NCT/pwm* 2>/dev/null | sort > ~/pwm-before.txt; wc -l ~/pwm-before.txt`, and `rpm -qp --dump $RPM | rg ' 040700 ' `. Expected: write down the line count of the file, `pwm1_enable` to `pwm7_enable` all read 5, and the dump of the package holds the line of `/var/lib/nicecooling` with mode `040700` and no file below it.
2. Install the package: `sudo rpm -i $RPM`. Then `rpm -V nicecooling; stat -c '%a %U' /var/lib/nicecooling; ls /usr/libexec/nicecoolingd /usr/bin/nicecooling /usr/lib/systemd/system/nicecoolingd.service /usr/lib/systemd/system/poweroff.target.d/50-nicecooling.conf /usr/share/dbus-1/system.d/io.github.cappyt.NiceCooling1.conf /usr/share/polkit-1/actions/io.github.cappyt.NiceCooling1.policy /usr/share/polkit-1/rules.d/50-nicecooling.rules /etc/xdg/autostart/io.github.cappyt.NiceCooling.desktop; ls -Z /usr/libexec/nicecoolingd; ls /var/lib/nicecooling`. Expected: `rpm -i` exits 0, `rpm -V` prints nothing, `stat` prints `700 root`, every file of the list exists, the label of the daemon binary is the one that `restorecon -n -v /usr/libexec/nicecoolingd` keeps (no change line), and `/var/lib/nicecooling` is empty (the package owns the directory and no file in it). Compare `pwm1_enable` to `pwm7_enable` with `~/pwm-before.txt`: nothing changed. TODO(owner): the spec of task 9.1 does not say whether the package enables or starts the unit. Write down what `systemctl is-enabled nicecoolingd` and `systemctl is-active nicecoolingd` print here.
3. Start the daemon for the first time with no Channel under control: `sudo systemctl enable --now nicecoolingd`, wait 10 s. Then `systemctl is-active nicecoolingd; sudo ls /var/lib/nicecooling; sudo jq '.chips[0].channels | length' /var/lib/nicecooling/bios-snapshot.json; grep -r . $NCT/pwm* 2>/dev/null | sort | diff - ~/pwm-before.txt; sudo journalctl -b -g 'avc: +denied' | rg -i nicecooling`. Expected: `active`, the directory holds `bios-snapshot.json`, the snapshot holds 7 Channels, no `lease.json` exists, `diff` prints nothing (the daemon wrote no Channel), and the last command prints no line.
4. Check the snapshot against the Board: `sudo jq -r '.chips[0].channels[] | "\(.channel) \(.manual_at_snapshot)"' /var/lib/nicecooling/bios-snapshot.json; snapcheck`, then the checksum command of the next sentence. Expected: seven lines `N false` for N from 1 to 7, and `snapcheck` prints nothing. The snapshot holds the values of `~/pwm-before.txt`. Run `sudo sha256sum /var/lib/nicecooling/bios-snapshot.json | tee ~/snapshot-sha.txt` and keep the file.
5. Write a Channel for the first time: `t0=$(date +%s%6N); $BC call $B $R/channel/nct6798_pwm1 $B.Channel SetFixedDuty d 50`. Wait 5 s. Then `sudo jq '.created_us' /var/lib/nicecooling/lease.json; sudo jq '.taken_us' /var/lib/nicecooling/bios-snapshot.json; cat $NCT/pwm1 $NCT/pwm1_enable`. Expected: `lease.json` exists, `taken_us` of the snapshot is less than `created_us` of the lease, and the lease is less than the first write: `created_us` is greater than `t0`, so the lease comes before the first write. `pwm1` reads 128 (50% x 255 / 100 rounded) and `pwm1_enable` reads 1. The snapshot file is the same as in step 4: `sudo sha256sum /var/lib/nicecooling/bios-snapshot.json` equals `~/snapshot-sha.txt`.
6. Stop the service and look at the Channels: `sudo systemctl stop nicecoolingd; snapcheck; ls /var/lib/nicecooling/lease.json`. Expected: `snapcheck` prints nothing, `pwm1_enable` reads 5, and `ls` reports that `lease.json` does not exist.
7. Remove the package while the daemon controls a Channel. Start it again and write the Channel: `sudo systemctl start nicecoolingd`, wait 10 s, `$BC call $B $R/channel/nct6798_pwm1 $B.Channel SetFixedDuty d 50`, wait 5 s. Check `cat $NCT/pwm1_enable` reads 1. Save the list of the package files: `rpm -ql nicecooling > ~/nicecooling-files.txt`. Then `sudo rpm -e nicecooling`. Expected: `rpm -e` exits 0. The unit is stopped before the files go: `systemctl is-active nicecoolingd` prints `inactive` or `failed` and not `active`, and `pgrep -a nicecoolingd` prints nothing. The journal shows the stop of the unit before the removal: `journalctl -u nicecoolingd -n 10`.
8. Check the Channels and the files after the removal: `snapcheck; cat $NCT/pwm1_enable; for f in $(rg -v '^/var/lib/nicecooling$' ~/nicecooling-files.txt); do { [ -f "$f" ] || [ -L "$f" ]; } && echo "LEFT $f"; done; stat -c '%a' /var/lib/nicecooling; sudo ls /var/lib/nicecooling`. Expected: `snapcheck` prints nothing, `pwm1_enable` reads 5, no `LEFT` line shows, `stat` prints `700`, and the directory still holds `bios-snapshot.json`. The state directory stays after the removal (design, Migration Plan).
9. Install again and check that the old state loads: `sudo rpm -i $RPM; sudo systemctl enable --now nicecoolingd`, wait 10 s, then `systemctl is-active nicecoolingd; busctl get-property $B $R $B.Manager StateHealth; sudo sha256sum /var/lib/nicecooling/bios-snapshot.json; sudo jq .taken_us /var/lib/nicecooling/bios-snapshot.json`. Expected: `active`, `s "ok"`, and `snapcheck` prints nothing. Record the checksum: the snapshot is the same as in step 4 or it differs only by `taken_us`, because no lease exists after the stop of step 6 and the daemon then refreshes the snapshot (design D8).
10. Clean up: `sudo systemctl disable --now nicecoolingd; sudo rpm -e nicecooling`. Remove the state if wanted: `sudo rm -rf /var/lib/nicecooling`. Expected: `snapcheck` prints nothing before you remove the state, every `pwmN_enable` reads 5, and `rpm -q nicecooling` prints `package nicecooling is not installed`.

## Recovery

- A Channel stays in manual mode after a failed step: `echo 5 | sudo tee $NCT/pwmN_enable`, with the number of the Channel. When the snapshot file exists, `sudo /usr/libexec/nicecoolingd --restore-bios` restores all Channels from it and does nothing when no lease exists.
- A pump that runs too slow: `sudo systemctl stop nicecoolingd`. The stop restores the BIOS snapshot.
- `rpm -e` fails: `sudo systemctl stop nicecoolingd` first, then `sudo rpm -e nicecooling` again.

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
