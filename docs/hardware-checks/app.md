# App check

This procedure checks the app on the reference desktop: the background start, the tray icon, the Alarm notification and the places that cancel a pending poweroff. It also checks the steps of `docs/app.md` on a new user account. Each step has a pass line.

The check raises a real Alarm with a poweroff countdown. When no cancel works, the PC powers off at the end of the countdown. Save all work before step 4.

## Preconditions

- Fedora 45 with GNOME on the reference desktop, with the Leakshield on its USB cable.
- `owner` is a `wheel` member.
- The AppIndicator extension is installed (`docs/app.md`, The AppIndicator extension). It is on for `owner`.
- The notifications of the shell are on (Settings, Notifications, Do Not Disturb off).
- A second terminal for `busctl`. It stays open in the session of `owner` during steps 4 to 7.
- The unit `nicecoolingd` runs. The Loop exists (`docs/app.md`, First run).

## Helper: raise the Alarm

The Alarm `leakshield-unreachable` rises when the daemon gets no data from the Leakshield for 10 s. Its response is "Poweroff after a delay". A cancel leaves the Alarm active, and the same Alarm does not start a new countdown until it closes. So each step that needs a countdown starts with these actions:

1. Plug the USB cable of the Leakshield. Wait until the Overview shows the Leakshield and no Alarm is active.
2. Unplug the USB cable. After 10 s the Alarm rises and the countdown starts.

Plug the cable again at the end of each step. The Alarm then closes.

## Steps

1. Install by `docs/app.md`, section Install. Reboot or log out. Expected: `systemctl is-active nicecoolingd` prints `active`, and `ls /etc/xdg/autostart/io.github.cappyt.NiceCooling.desktop` finds the file.
2. Log in as `owner`. Wait 20 s. Do not start the app by hand. Run `pgrep -a nicecooling` in a terminal. Expected: it prints one process with `nicecooling --background`. The Overview and `Alt+Tab` show no window of the app. The top bar shows the tray icon `nicecooling-symbolic`. Open its menu. It has "Open nicecooling" and "Quit".
3. Set the Alarm response: open the window from the tray menu, go to Alarms, "Alarm responses", row "Leakshield unreachable". Select "Poweroff after a delay" and set "Delay" to 120. Expected: the feedback "Applied" shows. In the second terminal run `busctl --system monitor io.github.cappyt.NiceCooling1`. It stays open and prints the signals of the next steps. Close the window.
4. Raise the Alarm (Helper). Expected: within 15 s a notification "Leakshield unreachable" shows with the buttons "Cancel poweroff" and "Open nicecooling". It stays on screen or in the notification list. The tray icon changes to `nicecooling-alarm-symbolic`. The first tray menu entry reads "Cancel poweroff (m:ss)" and counts down. When the notification shows no button, expand it first.
5. Click "Cancel poweroff" in the notification. Expected: the notification changes to "Poweroff cancelled". The tray entry "Cancel poweroff" is gone. The `busctl monitor` output shows the signal `PoweroffCancelled`. The Alarm stays active. Plug the cable.
6. Raise the Alarm again (Helper). Open the tray menu and click "Cancel poweroff (m:ss)". Expected: the same result as step 5 (the tray entry is gone, `PoweroffCancelled` shows). Plug the cable.
7. Raise the Alarm again (Helper). Click "Open nicecooling" in the notification, or the tray entry "Open nicecooling". Expected: the window opens, the poweroff banner shows on the top with the countdown, and the countdown dialog opens with the focus on "Cancel poweroff". Click "Hide", then click "Cancel poweroff" in the banner. Expected: the banner and the dialog go away and `PoweroffCancelled` shows. Plug the cable. Repeat with the button "Cancel poweroff" in the Alarm row of the Alarms screen: raise the Alarm, open Alarms, click the button. Expected: the same result. Plug the cable. Close the window. Expected: `pgrep -a nicecooling` still lists the process.
8. Check a new user account with only `docs/app.md`. Remove the old state with the section Remove of `docs/app.md`, and the settings of `owner`. Create the account: `sudo useradd -m -G wheel nctest` and `sudo passwd nctest`. Log in as `nctest`. Follow `docs/app.md` from the top, with no other document. Expected: Install, The AppIndicator extension, Start the app and First run all work as written. After the next login of `nctest`, the app runs with no window and the tray icon shows (step 2). Write each place where the text was wrong or unclear in the record.
9. Clean up. Plug the Leakshield cable. As `owner`, open Alarms, "Alarm responses", and click "Reset" on the row "Leakshield unreachable". Expected: the response is the default again. Close the `busctl` terminal. Remove the account if wanted: `sudo userdel -r nctest`.

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
