# nicecooling

nicecooling controls the fans and the pump of a water-cooled Linux desktop, and it monitors and configures the Leakshield. The system service `nicecoolingd` does the control. The app `nicecooling` shows a window, a tray icon and notifications.

Target: Fedora on x86_64, with GNOME.

## Install

Before the first start, update the Leakshield to firmware 1022 with aquasuite. The daemon reads a Leakshield with another firmware, and it does not write it (`docs/protocol/leakshield.md`).

1. Build the RPM in a container: `packaging/container/build-rpm.sh`. It needs `podman` and network. The RPM is `target/rpm/RPMS/x86_64/nicecooling-<version>.rpm`.
2. Install it: `sudo dnf install ./target/rpm/RPMS/x86_64/nicecooling-*.rpm`.
3. Start the daemon and start it at boot: `sudo systemctl enable --now nicecoolingd`.
4. Check it: `systemctl is-active nicecoolingd` prints `active`.
5. Start the app: `nicecooling`. `docs/app.md` explains the first run and the tray icon.

The first start of the daemon takes the BIOS snapshot: the `pwm` attributes of each Channel as the BIOS left them. The snapshot is `/var/lib/nicecooling/bios-snapshot.json`. The daemon saves it before the first Channel write.

The package installs the daemon, the app, the systemd unit, the `poweroff.target` drop-in, the D-Bus policy, the polkit action and rule, the desktop entries, the icons and the Board files. The RPM owns each of them.

## Hardware watchdog (optional)

When the kernel hangs, the Channels stay at the last duty. A hardware watchdog resets the machine in that case. systemd feeds the watchdog when `RuntimeWatchdogSec` is set in the `[Manager]` section of `/etc/systemd/system.conf`. Fedora does not ship that file. Create it, or use a drop-in, which systemd recommends:

```sh
sudo mkdir -p /etc/systemd/system.conf.d
printf '[Manager]\nRuntimeWatchdogSec=30s\n' | sudo tee /etc/systemd/system.conf.d/50-watchdog.conf
sudo systemctl daemon-reload
systemctl show -p RuntimeWatchdogUSec
```

The last command prints `RuntimeWatchdogUSec=30s`. The setting belongs to the host: the package does not set it. The mainboard must have a watchdog device (`/dev/watchdog0`).

## Rollback

The Migration Plan in [`openspec/changes/add-nicecooling-v1/design.md`](openspec/changes/add-nicecooling-v1/design.md) (section "Migration Plan") defines the steps. In short:

1. Stop the service: `sudo systemctl stop nicecoolingd`. This restores the BIOS snapshot: each Channel goes back to its snapshot attributes.
2. If wanted, restore the first Leakshield backup of the device with `RestoreBackup` (`docs/api.md`, section "Leakshield backups"). The backups are in `/var/lib/nicecooling/backups/`.
3. Remove the package: `sudo dnf remove nicecooling`. The RPM owns each file it installs, so the removal deletes them.

Files that remain after the removal:

- `/var/lib/nicecooling/`: the daemon state, the BIOS snapshot and the Leakshield backups. Delete it by hand to remove it.
- `~/.local/share/nicecooling/` of each user: the app history.
- `~/.config/nicecooling/` and `~/.config/autostart/io.github.cappyt.NiceCooling.desktop` of each user: the app settings and the "Start at login" override.
