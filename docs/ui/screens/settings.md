# Settings

Comp: `../comps/settings-network.html` shows the card "Network" (1280x800; also 1100x700 and Italian in `../comps/shots/`). The other cards have no comp. The screen is a settings form: one card per group, one row per setting, as in `../comps/leakshield-wizard.html`.

## Layout

| Card | Rows |
|---|---|
| Language | "Language": select with "System", "English", "Italiano", "Deutsch", "Français". Each language name is in its own language |
| Start and tray | "Start at login": switch. "Tray icon": switch. The hint row for the AppIndicator extension, only when no tray host exists |
| Notifications | "Warnings": switch. "Profile switched by a rule": switch, off by default. "External change on a Leakshield": switch. Text row: "Alarm notifications are always on." |
| Network | The MQTT broker and the SMTP account of the app: see "Network" |
| Daemon | "Connection": "Connected", "Offline" or "Down", with "Try again". "Daemon version" and "App version". "Board": the Board name and the Board file in use. "State directory": the path `/var/lib/nicecooling/`, read-only text, no control. There is no configuration file |
| About | App name, version, license, the link to the project page, and the note about Aqua Computer below |

## Network

One card with two halves side by side: "MQTT broker" at the left, "SMTP account" at the right. Each half is a list with one row per setting: the name at the left, the control at the right, 36 px high. The app holds these settings in `settings.toml` and the two passwords in `secrets.toml`. The daemon has no part in them.

| Half | Row | Control |
|---|---|---|
| MQTT broker | "Connection" | Text, no control: "Off", "Connecting", "Connected" with the green dot, or "Failed" with the octagon and the last error text as caption |
| MQTT broker | "Host" | Input. Empty means that MQTT is off |
| MQTT broker | "Port" | Number input. Empty gives 1883, or 8883 with TLS. The default shows as dim text in the empty input |
| MQTT broker | "TLS" | Switch |
| MQTT broker | "CA file" | A path and the button "Choose", which opens the file dialog of the system. "None" without a file. It adds to the system certificates |
| MQTT broker | "User name" | Input |
| MQTT broker | "Password" | Password input |
| MQTT broker | "Client id" | Input. Empty gives "nicecooling-" and the host name |
| MQTT broker | "Base topic" | Input. Empty gives "nicecooling" |
| MQTT broker | "Value interval" | Number input in seconds, 1 to 3600. Default 10 |
| SMTP account | "Host" | Input. Empty means that no SMTP account is set |
| SMTP account | "Security" | Select: "STARTTLS", "TLS", "None" |
| SMTP account | "Port" | Number input. Empty gives 587, 465 or 25 for the three securities |
| SMTP account | "User name" | Input |
| SMTP account | "Password" | Password input |
| SMTP account | "Sender" | Input: the address in the "From" line |

- A password input never shows the stored password. With a password set it shows eight dots as a placeholder. Typing replaces the password. An empty input that the user confirms clears it. There is no button that shows the password.
- A field applies when it loses the focus or on Enter. Escape keeps the earlier value. A change of a broker setting closes the connection and opens a new one.
- A value that the app refuses does not apply. The field keeps the focus and shows the reason under it: "The interval is 1 to 3600 s.", "A base topic has no wildcard and no empty level."
- The two warnings are rows on `warning-tint` with the Warning icon, at the end of their half. A warning does not turn a field off.
- Caption under the card: "The app publishes the sensor values and the Alarm events while it runs. It reads nothing from the broker."

| Warning | When | Text |
|---|---|---|
| Clear-text password | "TLS" is off and a broker password is set | "The password goes over the network in clear text" |
| Login without encryption | "Security" is "None" and a user name is set | "The login is not encrypted. The user name and the password go over the network in clear text." |

## States

| State | What the user sees |
|---|---|
| Empty | Not used |
| Loading | Row frames with grey bars for the daemon rows only |
| Daemon offline | "Connection: Offline" with the offline icon and the text "Start the daemon: systemctl start nicecoolingd". The Language, notification and Network settings still work: the app holds them |
| Daemon down | "Connection: Down" with the octagon in the Alarm color and the text "systemd stopped the restarts of the daemon. nicecooling protection is off. The Board runs its Hardware curves from the BIOS snapshot." Then the two commands: `systemctl status nicecoolingd` and `systemctl restart nicecoolingd` |
| Version mismatch | Warning row in the Daemon card: "The daemon is version 1.1 and the app is version 1.0. Update both to the same version." |
| Tray not visible | Hint row with the Warning icon: "The tray icon is not visible. On GNOME, turn on the AppIndicator extension." Link "How to turn it on" |
| No Board file | "Board: ASRock X570 Taichi. No Board file: Channels and sensors have the names of the kernel." |
| Language changed | The app changes at once, without a restart |
| MQTT off | "Connection: Off" in `fg-dim`. The app opens no connection while "Host" is empty |
| MQTT connecting | "Connection: Connecting" with a spinner |
| MQTT connected | "Connection: Connected" with the green dot. Caption: the host and the port |
| MQTT failed | "Connection: Failed" with the octagon in the Alarm color. Caption: the last error text ("The broker refused the login."). The app tries again by itself |
| Response pause | Banner 3 of the shell, as on every screen |
| Alarm | Not used |

## Interactions

- Each switch applies at once. The app writes the switches to `settings.toml` and "Start at login" to the autostart override (`docs/storage.md`).
- "Quit" is in the primary menu, not here.

## Copy

| Place | Text |
|---|---|
| Start at login | "The app starts without a window and shows its tray icon." |
| Tray icon off | "Without the tray icon, open the window from the app list." |
| About note | "nicecooling is not a product of Aqua Computer. LEAKSHIELD and aquasuite are names of Aqua Computer GmbH & Co. KG." |
| Firmware | "nicecooling does not update the firmware of any device." |
| Network | "MQTT broker", "SMTP account", "Connection", "Host", "Port", "TLS", "CA file", "Choose", "None", "User name", "Password", "Client id", "Base topic", "Value interval", "Security", "Sender" |
