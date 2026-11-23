# Fan control check

This procedure checks the daemon on the reference Board: Curves on a Channel, the BIOS snapshot, and the restore after a stop, a kill, a freeze and a restart limit. Each step has a pass line. The check runs the daemon as root and writes `pwmN` attributes of the Channels. Do it with no load on the loop.

The owner keeps Channel 6 (`nct6798` `pwm6`, the pump) in manual mode at raw 255. The snapshot records that state, so a restore gives it back.

## Preconditions

- Fedora 45 on the reference Board. The unit and the binary are not packaged yet, so install them by hand in step 1.
- `jq` and `python3` are installed. The kernel driver `aquacomputer_d5next` may stay loaded: the daemon ignores its hwmon device.
- `/var/lib/nicecooling/` does not exist or is empty: `sudo systemctl stop nicecoolingd; sudo rm -rf /var/lib/nicecooling`.
- The pump safety limit: the check puts Channel 6 on a Curve with a Pump floor of 60%. Watch the water flow in `/sys/class/hwmon/hwmon*/fan2_input` of `asusec`. Stop the daemon with `sudo systemctl stop nicecoolingd` when the flow falls below 60% of its value in step 3.

## Check helper

Run this function in the shell of each step that says "compare with the snapshot". It prints one `DIFF` line for each attribute that differs from the snapshot, and nothing when all match. It skips `pwmN` of a Channel in an automatic mode, because the chip sets that value.

```bash
snapcheck() {
  sudo python3 - <<'PY'
import glob, json, os
snap = json.load(open("/var/lib/nicecooling/bios-snapshot.json"))
hw = next(os.path.dirname(p) for p in glob.glob("/sys/class/hwmon/hwmon*/name") if open(p).read().strip() == "nct6798")
for chip in snap["chips"]:
    for ch in chip["channels"]:
        n = ch["channel"]
        auto = int(dict(ch["attributes"])[f"pwm{n}_enable"]) >= 2
        for name, want in ch["attributes"]:
            if auto and name == f"pwm{n}":
                continue
            have = open(f"{hw}/{name}").read().strip()
            if have != want:
                print("DIFF", name, "want", want, "have", have)
PY
}
```

## Steps

1. Build and install by hand: `cargo build --release -p nicecoolingd`, then `sudo install -m 755 target/release/nicecoolingd /usr/libexec/nicecoolingd`, `sudo install -m 644 packaging/nicecoolingd.service /usr/lib/systemd/system/`, `sudo install -d /usr/share/nicecooling/boards /usr/share/nicecooling/calibration`, `sudo install -m 644 data/boards/*.toml /usr/share/nicecooling/boards/`, `sudo install -m 644 data/calibration/*.toml /usr/share/nicecooling/calibration/`, `sudo restorecon -R /usr/libexec/nicecoolingd /usr/share/nicecooling` and `sudo systemctl daemon-reload`.
2. Write the state files. `sudo install -d -m 700 /var/lib/nicecooling/profiles/profile_default`. In `/var/lib/nicecooling/state.json` write `{"schema_version":1,"loops":[{"id":"loop_1","name":"Loop","pump_channels":["nct6798_pwm6"],"flow_sensor":"asusec_fan2","calibration_table":"alphacool_17558","water_temperature":"asusec_temp4","leakshield":null,"pump_floor":60}]}`. In `/var/lib/nicecooling/profiles/profile_default/profile.json` write `{"schema_version":1,"id":"profile_default","name":"Default","channels":{"nct6798_pwm6":{"control":"curve","input":"asusec_temp4","points":[[20,60],[50,100]],"smoothing_s":null,"step_limit_pct_per_s":null,"fixed_duty":null}},"pump_floors":{},"slot":1}`. Expected: the Curve gives 60% at 20 C and 100% at 50 C.
3. Record the state before the start: `sudo cat /sys/class/hwmon/hwmon*/fan2_input` of `asusec` (flow), and `pwm6`, `pwm6_enable` of `nct6798`. Start the daemon: `sudo systemctl start nicecoolingd`. Expected: `systemctl is-active nicecoolingd` prints `active`, `/var/lib/nicecooling/bios-snapshot.json` exists, `jq '.chips[0].channels | length'` prints 7, and `/var/lib/nicecooling/lease.json` exists. The journal shows the warning `manual-at-snapshot nct6798_pwm6`.
4. Wait 30 s. Expected: Channel 6 follows the Curve. `pwm6` equals `round(duty * 255 / 100)` for the duty that the Curve gives at the `asusec` `temp4_input` value (60% at 20 C or below, 100% at 50 C or above), and `pwm6_enable` is 1 or 0 (0 only at raw 255). The other Channels do not change: `snapcheck` prints no `DIFF` for Channels 1 to 5 and 7.
5. `sudo systemctl stop nicecoolingd`. Expected: `snapcheck` prints nothing, and `/var/lib/nicecooling/lease.json` is gone. `pwm6` reads 255 and `pwm6_enable` reads 0, as before step 3.
6. Start the daemon, wait 10 s, and write `sha256sum /var/lib/nicecooling/bios-snapshot.json` down. Run `sudo kill -9 $(systemctl show -p MainPID --value nicecoolingd)`. Within 1 s run `snapcheck`. Expected: it prints nothing, because `ExecStopPost` ran before the restart (`RestartSec=2s`). Wait 5 s. Expected: the unit is `active` with a new `MainPID`, the checksum of the snapshot is the same, and `lease.json` exists again.
7. Run `sudo kill -STOP $(systemctl show -p MainPID --value nicecoolingd)` and note the time. Within 20 s run `snapcheck`. Expected: it prints nothing, the journal shows `Watchdog timeout` and the stop of the unit, and systemd starts the daemon again (`MainPID` changed, the unit is `active`).
8. Run `kill -9` of the `MainPID` six times, waiting until the unit is `active` between kills, all within 60 s: `for i in 1 2 3 4 5 6; do until [ "$(systemctl is-active nicecoolingd)" = active ]; do sleep 0.2; done; sudo kill -9 $(systemctl show -p MainPID --value nicecoolingd); sleep 0.5; done`. Expected: `systemctl show -p Result --value nicecoolingd` prints `start-limit-hit`, the unit stays `failed`, and `snapcheck` prints nothing.
9. Clean up: `sudo systemctl reset-failed nicecoolingd`. Expected: `pwm6` reads 255 and `pwm6_enable` reads 0. Remove the state if wanted: `sudo rm -rf /var/lib/nicecooling`.

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
