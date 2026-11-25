# nicecooling app

The app is the user part of nicecooling. It shows the state of the daemon, controls it, and keeps a tray icon and desktop notifications. The daemon `nicecoolingd` does the control and the alarm response. The app only reads and sends commands. When the app does not run, the daemon still runs.

The app runs as the user, in the graphical session. It talks to the daemon on the system bus. It needs no root.

## Install

The RPM installs the files below, and `README.md` gives its install and rollback steps. To install by hand instead, run the commands in the repository root, on Fedora with GNOME.

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

At login the app can start before the StatusNotifier host of the desktop. The app waits for the host, and the tray icon shows when the host comes. Without a host after 30 s, the app does the following:

- It opens its window, also with `--background`.
- Settings shows the hint "The tray icon is not visible. On GNOME, turn on the AppIndicator extension."
- "Do not show this again" on the hint stores the hint id `appindicator` in `settings.toml`.

## Start the app

- `nicecooling` opens the window.
- `nicecooling --background` opens no window. The tray icon and the notifications run.
- The entry "NiceCooling" in the app list runs `nicecooling`.
- One instance runs for each user session. It holds the name `io.github.cappyt.NiceCooling` on the session bus. A second `nicecooling` shows the window of the first and exits with status 0. A second `nicecooling --background` exits with status 0 and asks nothing. Without a session bus, the app starts as usual.
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

The polkit action `io.github.cappyt.NiceCooling1.cancel-poweroff` allows a member of `wheel` in a local session, active or not. A user outside `wheel` must give the administrator password (`docs/api.md`, Authorization). The app of a second session of a `wheel` member cancels the poweroff too.

## Graph service

The Graph service evaluates the Graphs (`docs/graphs.md`). It runs on its own thread, in the one app instance of the user session. It follows the events of the daemon and takes the commands of the window, the tray and the notifications through one channel. The channel holds 256 commands. A command that comes while the channel is full is refused, and the refusal text goes to the shared view. The stop message has its own path and is never refused.

- A pass runs once per second. It reads the sensor values and the Alarms from the last events of the daemon, and it scans `/proc` for the process nodes only when a Graph has one. A pass that is late does not run again to catch up.
- A Graph that panics in a pass is stored as disabled, and a notification names it. A panic outside the evaluation of a Graph ends the service. The window shows the error and a notification says that the service stopped.
- The service writes its state for the screens to one shared view: the rows of the Graphs, the countdown, the Response pause, the state of the values call, the response log and the refusals (`docs/ui/screens/graphs.md`).
- "Try" of the Graph editor runs a draft with its own state. It sends no value to the daemon and starts no response.
- `docs/storage.md` defines the files. The app is the only writer and the daemon never reads them.

### Links to the daemon

- At each pass the service sends the value of each Graph sensor with `SetGraphSensorValues` (`docs/api.md`, Graph sensors). A disabled Graph and a value that is not finite go as the no-value entry. The call waits at most 2 s.
- The call has no interactive flag. When polkit refuses it, the call stops for 60 s and the shared view holds the state "no access". The app never opens a password dialog for it.
- Apply and Enable make the Graph sensor with `CreateGraphSensor` when the Graph has none. Apply renames it with `SetDefinition` when the name or the unit differs. A Graph that loses its sensor end node deletes its sensor, and a delete of a Graph deletes its sensor first. These calls can open the polkit prompt. A refusal of the daemon, for example `InUse` or the limit of 64, keeps the Graph as it was and shows the reason.
- At each new snapshot of the daemon, the service makes the Graph sensor of each Graph that holds an id the daemon does not list. A Graph sensor that no Graph holds stays on the daemon. It can belong to the app of another user, so the app leaves it and lists it in the shared view.

### Active session

Only the app of the active session of the seat sends Graph sensor values and runs Graph responses. The app finds its session with `XDG_SESSION_ID` and `GetSession` of `org.freedesktop.login1.Manager`, reads the property `Active` of `org.freedesktop.login1.Session` and follows its `PropertiesChanged` signal. Without an id, or when logind does not know it, the alias `auto` of logind stands in.

- Until logind answers, the session counts as not active.
- A desktop with no `Active` property, or an app with no system bus, counts as active.
- The Graphs are evaluated in every session. An app of a session that is not active sends no value, steps no response state machine and changes no run state. The countdown of a power response that ends in such a session ends as `cancelled`. "Run once" shows a refusal.
- The Graph sensor of a user stays on the daemon while the session is away. The daemon counts a value older than 3 s as no value, so the Inputs of that sensor go to Lost Input.

### Responses

A response end node fires when its state machine says so (`docs/graphs.md`, Response state machine). The run gets an id `run_<time_us>_<n>`, and each run writes one row to `response-log.json` with its outcome.

| Kind | What the app does | Outcome |
|---|---|---|
| `notification` | Shows a desktop notification. A new run of the same Graph replaces its last notification. | `ok`, or `failed` when the notification thread is gone |
| `sound` | Shows a notification with the Graph name as title and the sound as its hint. | as `notification` |
| `command` | Starts the program and its arguments with no shell and no placeholder. The program gets the environment of the app and the variables of `docs/graphs.md` (Placeholders), and the three streams are closed. The program leads its own process group. | `ok` for exit code 0, `failed` for another exit or a start error, `timeout` after the time limit (the app kills the group) |
| `poweroff`, `reboot`, `suspend` | Counts down, then calls logind. | `ok`, `failed` or `cancelled` |
| `mqtt` | Sends one message through the MQTT publisher. The topic is literal, and the payload template is expanded. | `ok` at the acknowledgment of the broker, `timeout` after 30 s, `failed`, or `not-configured` without a broker host |
| `email` | Sends one plain text message in its own task, with a limit of 30 s. The subject and the text are expanded. | `ok`, `failed`, `timeout`, or `not-configured` without an SMTP account |

One response end node has at most one run without an outcome. A run that starts while the node still has one gets the outcome `busy`.

### Power responses and logind

- A power response counts down for `countdown_s` seconds, limited to 10 to 3600. One countdown runs at a time. A second power response during it gets `busy`.
- The countdown shows as a critical notification with the button "Cancel", as an entry of the tray menu and in the shared view. Each shows the seconds left. The button and the tray entry send the command `CancelCountdown`. A cancel gives the count back to the Response pause and ends the run as `cancelled`. A cancel after the power call changes nothing.
- While the daemon reports the poweroff countdown of an Alarm, no Graph power response starts, and a running Graph countdown is cancelled. The Alarm wins. "Run once" of a power response shows a refusal in that time.
- At the end of the countdown the service writes `graph-state.json`, then calls logind on the system bus: `PowerOff`, `Reboot` or `Suspend` of `org.freedesktop.login1.Manager`, with the argument `interactive` false. logind then asks for no password. A state file that cannot be written means no call, because after a restart the response could run again. The call waits at most 15 s.
- A refusal, an error or a timeout of logind gives the outcome `failed` and a critical notification with the reason.

### Response pause

Three poweroff or reboot runs in a row start the Response pause. The pause protects against a loop of reboots that a Graph causes.

- A run counts when it starts, not when its countdown ends. A suspend does not count. "Run once" counts.
- The count goes back to 0 after 60 minutes of app run time without a counted run. The app run time adds up over restarts, and `graph-state.json` holds it.
- A run that the user cancels, or that stops with the app before the power call, gives its count back.
- While the pause is on, no response runs by itself. The state machines keep their state, so a condition that stays true runs nothing when the pause ends.
- The pause does not end by itself. The command `ResponsesOn` (the button "Turn the Graph responses on" of `docs/ui/screens/app-shell.md`) ends it and sets the count to 0.
- "Run once" runs while the pause is on.

### Stop path

"Quit" in the tray or in the primary menu, and SIGTERM (the end of the session), send the same command. The app asks the Graph service to stop and waits at most 2 s. The service cancels a running countdown, so it makes no power call after the stop. It then writes `graph-state.json` whatever the time since the last write, and answers. The app logs a line when the service does not answer. Then the app ends. The state file is also written at most every 30 s while a value or a run state changes, and before a power call.

### MQTT and SMTP clients

- The MQTT client only publishes. It runs as one task on the Tokio runtime of the app and never blocks a caller: a broker that is down costs nothing. It stays off while `host` of `[mqtt]` is empty. It connects with the keep-alive of 30 s, a clean session and the Last Will `offline` on `<base>/status`. After the connect it publishes `online` on that topic and the values of all sensors at once. After a lost connection it waits 1 s, then doubles the wait to 60 s. A connection that lasts 30 s or more resets the wait.
- The topics and the messages are in `docs/graphs.md` (MQTT topics). A sensor that the user hid on the Sensors screen has no message. The queue keeps the latest value of each sensor topic, and at most 256 messages for events and responses, across reconnects. At the limit it drops the oldest message and logs a line.
- The service reads `[mqtt]` and the password at the start of the app. A change in the settings file reaches the client at the next start.
- The SMTP sender runs in its own task with a limit of 30 s, so a slow server never delays a pass. The account is in `docs/graphs.md` (SMTP account).
- The password of each client is in `secrets.toml` (mode 0600). The app never shows it, never writes it to a log and never puts it in an export file.

## Settings and files

| File | Content |
|---|---|
| `~/.config/nicecooling/settings.toml` | The app settings: language, tray, notification choices, closed hints, hidden sensors, and the tables `[mqtt]` and `[smtp]`. The app is the only writer. A missing file gives the defaults. |
| `~/.config/nicecooling/graphs.json` | All Graphs. |
| `~/.config/nicecooling/secrets.toml` | The MQTT and SMTP passwords, mode 0600. |
| `~/.local/share/nicecooling/graph-state.json` | The stored values, the run state of the response end nodes and the Response pause. |
| `~/.local/share/nicecooling/response-log.json` | The newest 500 response runs. |
| `~/.config/autostart/io.github.cappyt.NiceCooling.desktop` | The "Start at login" override. It exists only when the setting is off. |
| `~/.local/share/nicecooling/history.db` | The history store. |

`$XDG_CONFIG_HOME` and `$XDG_DATA_HOME` replace `~/.config` and `~/.local/share` when set. `docs/storage.md` defines the formats.

### Language

The app has English, Italian, German and French. The setting "Language" in Settings has "System" and one entry per language, each in its own language. With "System", the app reads the first non-empty value of `LC_ALL`, `LC_MESSAGES` and `LANG`: a value that starts with `it`, `de` or `fr` gives that language, and any other value gives English. `docs/i18n-terms.md` lists the terms of the app text in each language.

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
- Delete `~/.config/nicecooling/`, `~/.local/share/nicecooling/` and the autostart override of each user, if wanted. The Graph sensors stay on the daemon until each is deleted (`docs/api.md`, Graph sensors).
