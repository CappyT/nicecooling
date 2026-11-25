# Graph responses check

This procedure checks the Graph responses on the reference desktop: one response of each of the 8 kinds, the cancel of a power response, the restart of the app, a second user and the Response pause. A real condition fires each response: a process that you start. Each step has a pass line.

## Safety

- Step 13 powers off the PC once. Save all work before step 13. Do step 13 last.
- Step 12 runs on a test VM, not on the reference desktop. The VM reboots 3 times. Do not run step 12 on the reference desktop.
- Steps 9 and 10 start a power countdown and cancel it. If a cancel fails, the PC reboots or suspends at the end of the countdown. Save all work before step 9.
- The second user of step 8 must log out before step 13. With two users logged in, logind refuses the poweroff.

## Preconditions

- Fedora 45 with GNOME on the reference desktop. The package files are installed (`docs/app.md`, Install). The app runs: `pgrep -a nicecooling` prints one process.
- `owner` is logged in and its session is the active session.
- The notifications of the shell are on (Settings, Notifications, Do Not Disturb off). The sound of the desktop is on.
- A mail account that you read, and an SMTP server that accepts a login from this PC.
- An MQTT broker, and `mosquitto_sub` on this PC. `docs/hardware-checks/mqtt.md` has the same broker settings.
- A second terminal for the commands below.

## Helper: the probe process

Each response fires while the process `ncprobe` runs. The process is a copy of `sleep`, so no other program has its name. Run this block once in the terminal:

```bash
cp /usr/bin/sleep /tmp/ncprobe
probe_start() { /tmp/ncprobe 3600 & }
probe_stop() { kill $(pgrep -x ncprobe); }
LOG=~/.local/share/nicecooling/response-log.json
rows() { jq '.rows | length' "$LOG"; }
```

`probe_start` starts the process. The condition of each Graph then holds. `probe_stop` ends the process. The condition then ends and the state of the response goes back to "not run". `rows` prints the number of rows of the response log of `owner`.

## How to build a response Graph

Each Graph of this check has the same shape. Do these actions on the Graphs screen:

- Press "New response". The editor opens with a response end node.
- Add a process node: type `process` in the search field of the node list and double click the result. Set its name to `ncprobe`.
- In the panel of the response end node, section "Inputs", select the process node for the pin `a`.
- Set the name of the Graph, the kind and the parameters that the step gives. Leave the hold time at 0 s, repeat off and the minimum interval at its default.
- Press "Apply". On the Graphs screen switch the Graph on.

## Steps

1. Set the accounts. Open Settings, group "Network". For MQTT set the host, the port, TLS on, the user name and the password of the broker, and the base topic `nicecooling`. For SMTP set the host, the port, the security, the user name, the password and the sender address. Expected: the MQTT state shows "Connected". Each password field shows a placeholder and never the password. Run `probe_start`, then `pgrep -x ncprobe`, then `probe_stop`: `pgrep` prints one number, and no response runs, because no Graph exists yet.
2. Notification. Build the Graph `Probe notification` with the kind notification, the title `Probe notification`, the text `{graph} fired at {time}` and the urgency normal. Run `probe_start`. Expected: within 3 s a desktop notification shows with the title "Probe notification" and the text "Probe notification fired at" and the local date and time. Run `rows`: it prints 1. Run `probe_stop`.
3. Sound. Build the Graph `Probe sound` with the kind sound and the sound name `dialog-warning`. Run `probe_start`. Expected: within 3 s the sound plays and a notification shows with the title "Probe sound". Run `probe_stop`.
4. Command. Build the Graph `Probe command` with the kind command, the path `/usr/bin/sh`, the first argument `-c`, the second argument `echo "$NICECOOLING_GRAPH_NAME" > /tmp/nc-graph-command.txt` and the time limit 30 s. Run `rm -f /tmp/nc-graph-command.txt` and `probe_start`. Wait 3 s and run `cat /tmp/nc-graph-command.txt`. Expected: it prints `Probe command`. Run `probe_stop`.
5. E-mail. Build the Graph `Probe email` with the kind e-mail, your own address as recipient, the subject `nicecooling {graph}` and the text `Fired at {time}`. Run `probe_start`. Expected: within 1 minute a message arrives at your address. Its subject is "nicecooling Probe email". Its text is "Fired at" and the local date and time. Run `probe_stop`.
6. MQTT message. In a third terminal run `mosquitto_sub -h <broker host> -p <port> --cafile <CA file> -u <user> -P <password> -t 'nicecooling/alerts/#' -F '%I %q %r %t %p'`. Build the Graph `Probe mqtt` with the kind MQTT message, the topic `alerts/probe` and an empty payload. Run `probe_start`. Expected: within 3 s the subscriber prints one line with QoS 1, retain flag 0, the topic `nicecooling/alerts/probe` and a payload of the form `{"graph_id":"graph_<n>","graph_name":"Probe mqtt","time":<number>}`. Run `probe_stop`. Close the third terminal with Ctrl+C.
7. Restart with the condition true. Run `probe_start`. Wait until the notification of `Probe notification` shows (step 2). Run `rows` and write the number down. Open the tray menu and press "Quit". Run `nicecooling --background &` and wait 20 s. Expected: no new notification shows. `rows` prints the same number as before the restart. The Graphs screen shows `Probe notification` as "fired". Run `probe_stop`.
8. Second user. Export the Graph `Probe notification`: on the Graphs screen press "Export" and save the file as `/tmp/probe-notification.json`. Run `chmod a+r /tmp/probe-notification.json`. Run `sudo useradd -m nctest` and `sudo passwd nctest`. In the top bar menu choose "Switch User" and log in as `nctest`. Wait until the app of `nctest` runs (`pgrep -u nctest -a nicecooling` prints one process). In the session of `nctest` open the Graphs screen, press "Import" and choose `/tmp/probe-notification.json`. Switch the imported Graph on. Switch back to the session of `owner` and unlock it. Run `probe_start`. Wait 10 s. Expected: the notification of `owner` shows. `sudo jq '.rows | length' /home/nctest/.local/share/nicecooling/response-log.json` prints 0 or reports that the file does not exist. The app of `nctest` runs no response, because its session is not active. Run `probe_stop`. Log `nctest` out, and then run `sudo userdel -r nctest`.
9. Cancel a reboot. Build the Graph `Probe reboot` with the kind reboot and the countdown 20 s. The editor shows the warning "This response can power off or restart the PC". Run `uptime -s` and write the time down. Run `probe_start`. Expected: within 3 s a notification of critical urgency shows with the name "Probe reboot", the seconds left and the button "Cancel". The tray menu and the window show the countdown. Press "Cancel" in the notification before the countdown ends. Wait 30 s. Expected: the PC does not reboot, and `uptime -s` prints the time that you wrote down. The response log in the editor of `Probe reboot` shows one row with the outcome `cancelled`. Run `probe_stop`. Switch the Graph off.
10. Cancel a suspend. Build the Graph `Probe suspend` with the kind suspend and the countdown 20 s. Run `probe_start`. Expected: the same notification as in step 9, with the name "Probe suspend". Press "Cancel" in the notification before the countdown ends. Wait 30 s. Expected: the screen stays on, the PC does not suspend and `uptime -s` prints the same time as in step 9. Run `probe_stop`. Switch the Graph off.
11. Read the response log. Open the editor of each Graph of steps 2 to 6, 9 and 10 in turn and read the response log in the response panel. Run `jq -c '.rows[] | [.graph_name, .kind, .outcome]' "$LOG"`. Expected: the editor of each Graph shows one row for each run that you made, and `jq` prints the same rows. `Probe notification` has 3 rows: one from step 2, one from step 7 and one from step 8. The restart in step 7 adds no row. The kinds `notification`, `sound`, `command`, `email` and `mqtt` have the outcome `ok`. `Probe reboot` and `Probe suspend` have the outcome `cancelled`.
12. The Response pause, on a test VM. The VM runs Fedora 45 with GNOME, the package files of `docs/app.md` and one user, and the app starts at login. Log in. Open the Graphs screen and press "New response". Add a constant node with the value 1. Set the kind reboot, the countdown 10 s, the hold time 0 s, repeat every 60 s and the minimum interval 0 s. In the section "Inputs" select the constant node for the pin `a`. Press "Apply" and switch the Graph on. A notification shows with the countdown, and the VM reboots after 10 s. This is the first reboot. The state of the Graph is "run" and the condition holds, so repeat makes the Graph run again after each start of the app. Log in again. The app starts, and 60 s after its start the second countdown runs and the VM reboots. Log in again. After 60 s the third countdown runs and the VM reboots. Log in the fourth time and open the Sensors screen. Wait 3 minutes. Expected: the banner "3 power responses in a row" shows on the Sensors screen with the button "Turn the Graph responses on". The VM does not reboot during the 3 minutes, and no countdown shows. Open the Graphs screen. The banner shows there as well. Press "Turn the Graph responses on". Expected: the banner goes away. Switch the Graph off before 60 s pass, and delete it. No countdown shows after that.
13. Poweroff. Log out every other user: `loginctl list-sessions` lists only the session of `owner`. Save all work. Build the Graph `Probe poweroff` with the kind poweroff and the countdown 20 s. Run `probe_start`. Press nothing. Expected: within 3 s a notification of critical urgency shows with the name "Probe poweroff", the seconds left and the button "Cancel". After the countdown the PC powers off. Start the PC and log in. Run `jq -c '.rows[-1] | [.graph_name, .kind, .outcome]' "$LOG"`. Expected: it prints `["Probe poweroff","poweroff","ok"]`. Then delete all Graphs of this check on the Graphs screen, and run `rm -f /tmp/ncprobe /tmp/nc-graph-command.txt /tmp/probe-notification.json`.

## Record

Date, GNOME Shell version:

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
| 11 | |
| 12 | |
| 13 | |
