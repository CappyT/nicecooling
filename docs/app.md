# nicecooling app

The app is the user part of nicecooling. It shows the state of the daemon, controls it, and keeps a tray icon and desktop notifications. The daemon `nicecoolingd` does the control and the alarm response. The app only reads and sends commands. When the app does not run, the daemon still runs.

The app runs as the user, in the graphical session. It talks to the daemon on the system bus. It needs no root.

## Install

There is no package yet: the RPM comes with section 10 of the OpenSpec change. Until then, install by hand from the repository. Run the commands in the repository root, on Fedora with GNOME.

1. Build: `cargo build --release -p nicecoolingd -p nicecooling`.
2. Install the daemon and its files:
   - `sudo install -m 755 target/release/nicecoolingd /usr/libexec/nicecoolingd`
   - `sudo install -m 644 packaging/nicecoolingd.service /usr/lib/systemd/system/`
   - `sudo install -m 644 packaging/dbus/io.github.cappyt.NiceCooling1.conf /usr/share/dbus-1/system.d/`
   - `sudo install -m 644 packaging/polkit/io.github.cappyt.NiceCooling1.policy /usr/share/polkit-1/actions/`
   - `sudo install -m 644 packaging/polkit/50-nicecooling.rules /usr/share/polkit-1/rules.d/`
   - `sudo install -d /usr/lib/systemd/system/poweroff.target.d /usr/share/nicecooling/boards /usr/share/nicecooling/calibration`
   - `sudo install -m 644 packaging/poweroff.target.d/50-nicecooling.conf /usr/lib/systemd/system/poweroff.target.d/`
   - `sudo install -m 644 data/boards/*.toml /usr/share/nicecooling/boards/`
   - `sudo install -m 644 data/calibration/*.toml /usr/share/nicecooling/calibration/`
3. Install the app and its files:
   - `sudo install -m 755 target/release/nicecooling /usr/bin/nicecooling`
   - `sudo install -m 644 packaging/io.github.cappyt.NiceCooling.desktop /usr/share/applications/`
   - `sudo install -m 644 packaging/io.github.cappyt.NiceCooling.autostart.desktop /etc/xdg/autostart/io.github.cappyt.NiceCooling.desktop`
   - `sudo install -D -m 644 docs/ui/icons/nicecooling.svg /usr/share/icons/hicolor/scalable/apps/io.github.cappyt.NiceCooling.svg`
   - `sudo install -d /usr/share/icons/hicolor/symbolic/apps`
   - `sudo install -m 644 docs/ui/icons/nicecooling-*symbolic.svg /usr/share/icons/hicolor/symbolic/apps/`
   - `sudo gtk-update-icon-cache -f /usr/share/icons/hicolor`
4. Label the files and start the daemon: `sudo restorecon -R /usr/bin/nicecooling /usr/libexec/nicecoolingd /usr/share/nicecooling`, `sudo systemctl daemon-reload`, `sudo systemctl reload dbus-broker`, `sudo systemctl enable --now nicecoolingd`.
5. Check the daemon: `systemctl is-active nicecoolingd` prints `active`.

The autostart entry has the name `io.github.cappyt.NiceCooling.desktop` in `/etc/xdg/autostart/`. The setting "Start at login" uses the same name (see Start at login).

## The AppIndicator extension

The tray icon is a StatusNotifierItem. GNOME Shell does not show it without the AppIndicator extension.

1. Install it: `sudo dnf install gnome-shell-extension-appindicator`.
2. Log out and log in again. On Wayland, the shell loads a new extension only at login.
3. Turn it on: `gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com`.
4. Check: `gnome-extensions list --enabled | grep appindicator` prints the identifier.

The extension is a setting of each user. Turn it on in each account that runs the app.

Without a StatusNotifier host, the app does the following:

- It opens its window at start, also with `--background`.
- Settings shows the hint "The tray icon is not visible. On GNOME, turn on the AppIndicator extension."
- "Do not show this again" on the hint stores the hint id `appindicator` in `settings.toml`.

## Start the app

- `nicecooling` opens the window.
- `nicecooling --background` opens no window. The tray icon and the notifications run.
- The entry "NiceCooling" in the app list runs `nicecooling`.
- Closing the window does not stop the app. Only "Quit" in the tray menu or in the primary menu stops it.

## First run

The first-run flow opens in the window when the daemon has no Loop. Its screen definition is `docs/ui/screens/first-run.md`.

1. Start the window: `nicecooling`. The Overview shows "Set up a Loop".
2. Step "Board": the app shows the Board, the Board file in use, and the Channels.
3. Step "Loop": choose the Pump channel, the flow sensor with its calibration table, the water temperature and the Leakshield. The step needs a Pump channel.
4. Step "BIOS snapshot": the app shows the snapshot that the daemon took, with the Hardware curves.
5. Step "Leakshield": it shows only with a Leakshield. The app does not write the Leakshield in this flow.
6. Step "Done": the Channels still run their Hardware curves. Add a Curve in Channels.

"Skip setup" closes the flow. The flow does not write the Leakshield.

When the daemon is offline, the flow shows "nicecooling needs its daemon. Start it: systemctl start nicecoolingd".

## Tray icon

| State | Icon |
|---|---|
| Normal | `nicecooling-symbolic` |
| Warning | `nicecooling-warning-symbolic` |
| Alarm, or the daemon is down | `nicecooling-alarm-symbolic` |
| The app cannot reach the daemon | `nicecooling-offline-symbolic` |

The theme must hold these four icons: step 3 of Install puts them in `hicolor`. The menu has the entries of `docs/ui/screens/tray-and-notifications.md`: the countdown, one line for each Loop, the Profile submenu, "Open nicecooling" and "Quit". A click on the icon opens the window. "Quit" stops the app. The daemon continues.

## Cancel a pending poweroff

An Alarm with the response "Poweroff after a delay" starts a countdown in the daemon. Four places cancel it:

- The "Cancel poweroff" button of the Alarm notification. It works without the window.
- The first entry of the tray menu, "Cancel poweroff (0:21)".
- The "Cancel poweroff" button of the poweroff banner and of the countdown dialog in the window.
- The "Cancel poweroff" button in the row of the Alarm on the Alarms screen.

All four call `CancelPoweroff` of the daemon (`docs/api.md`). The Alarm stays active. The notification then shows "Poweroff cancelled".

The polkit action `io.github.cappyt.NiceCooling1.cancel-poweroff` allows a member of `wheel` in a local session, active or not. Another user must give the administrator password (`docs/api.md`, Authorization). The app of a second session of a `wheel` member cancels the poweroff too.

## Settings and files

| File | Content |
|---|---|
| `~/.config/nicecooling/settings.toml` | The app settings: language, tray, notification choices, closed hints, hidden sensors. The app is the only writer. A missing file gives the defaults. |
| `~/.config/autostart/io.github.cappyt.NiceCooling.desktop` | The "Start at login" override. It exists only when the setting is off. |
| `~/.local/share/nicecooling/history.db` | The history store. |

`$XDG_CONFIG_HOME` and `$XDG_DATA_HOME` replace `~/.config` and `~/.local/share` when set. `docs/storage.md` defines the formats.

### Start at login

The setting is on by default, because of the system entry in `/etc/xdg/autostart/`. The switch in Settings changes it:

- Off: the app writes `~/.config/autostart/io.github.cappyt.NiceCooling.desktop` with `Hidden=true`. The session then ignores the system entry.
- On: the app removes that file.

## History

The app writes the history of the sensors while it runs, also with `--background`. It opens no history file when it does not run, so the history has a gap for that time.

- One transaction every 10 s writes the new `samples` rows and the `minutes` row of each complete minute.
- The hourly prune deletes `samples` older than 48 hours and `minutes` older than 365 days.
- The Overview and the History screen read the file.

### Measured size

The test `history_size_48h_60_sensors` of `crates/nicecooling/tests/history.rs` simulates 48 h of 60 sensors at 1 Hz with a simulated clock: 10 368 000 `samples` rows. It sums `history.db`, `history.db-wal` and `history.db-shm`. The limit of the test is 300 MB.

Measured size: 238 MB with the three files summed. This is under the limit of 300 MB.

Run it on request. The test is `#[ignore]`, and it writes the database in the temporary directory:

`cargo test --release -p nicecooling --test history history_size_48h_60_sensors -- --ignored --nocapture`

## Remove

- `sudo systemctl disable --now nicecoolingd`. The daemon restores the BIOS snapshot at stop.
- Delete the files that Install put in place.
- Delete `~/.config/nicecooling/`, `~/.local/share/nicecooling/` and the autostart override of each user, if wanted.
