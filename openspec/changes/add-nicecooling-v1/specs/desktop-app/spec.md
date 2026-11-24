# Desktop app

## Purpose

Defines the desktop app: the session agent that shows the tray icon, the notifications and the sensor history, and the windows for Curves, Leakshield settings and Profiles.

## ADDED Requirements

### Requirement: Start in the background
The app SHALL start at login without a window.

#### Scenario: Login
- **WHEN** the user logs in
- **THEN** the app runs with no window open and shows its tray icon

### Requirement: Tray icon
The app SHALL show a tray icon with an entry that opens the main window. While a poweroff countdown runs, the tray menu SHALL have the entry "Cancel poweroff". On GNOME the tray icon SHALL need the AppIndicator extension.

#### Scenario: Open from tray
- **WHEN** the user clicks the tray icon menu entry to open the window
- **THEN** the main window opens

#### Scenario: Cancel from the tray
- **WHEN** a countdown runs and the user selects "Cancel poweroff" in the tray menu
- **THEN** the app cancels the pending poweroff

### Requirement: Daemon unreachable
When the app cannot reach the daemon, it SHALL show that state in the window and in the tray icon, and it SHALL disable every control that writes.

#### Scenario: Daemon stopped
- **WHEN** the daemon stops while the app runs
- **THEN** the tray icon shows the offline state, the window shows a banner and every write control is disabled

#### Scenario: Restart limit reached
- **WHEN** systemd reaches the restart limit of the daemon
- **THEN** the window shows "daemon down, Board on BIOS curves"

### Requirement: Alarm notifications
The app SHALL be the only process that shows Alarm notifications. It SHALL show them with critical urgency. When a poweroff is pending, the notification SHALL have a cancel button if the notification server has the `actions` capability. The window SHALL also have a "Cancel poweroff" control.

#### Scenario: Cancel from notification
- **WHEN** the user presses cancel during the delay
- **THEN** the app cancels the pending poweroff

#### Scenario: Server without actions
- **WHEN** the notification server has no `actions` capability and a countdown runs
- **THEN** the notification has no button, and the tray menu and the window offer "Cancel poweroff"

### Requirement: Windowless session agent
The app SHALL run as a windowless GPUI process with a StatusNotifierItem tray icon and notification actions.

#### Scenario: Idle in the background
- **WHEN** the app runs with no window
- **THEN** its tray icon responds and the app has no open window

### Requirement: App settings
The app SHALL keep its language, tray and notification switches and dismissed hints in a settings file of the user. "Start at login" off SHALL disable the system autostart entry for that user.

#### Scenario: Start at login off
- **WHEN** the user turns "Start at login" off
- **THEN** the app writes an autostart entry with `Hidden=true` in the user directory, and the next login starts no app

#### Scenario: Settings kept
- **WHEN** the user turns a notification switch off and restarts the app
- **THEN** the switch is still off

### Requirement: Sensor history
The app SHALL record sensor samples at 1 second for 48 hours, then 1 minute aggregates for one year. It SHALL store them in the data directory of the user.

#### Scenario: Old samples
- **WHEN** a sample is 3 days old
- **THEN** only its 1 minute aggregate remains

#### Scenario: Gap
- **WHEN** no user session runs the app
- **THEN** the history has a gap for that time

### Requirement: Curve editor
The app SHALL let the user edit the points of a Curve, choose its Input and see the live Input and Duty.

#### Scenario: Edit a point
- **WHEN** the user drags a point
- **THEN** the app sends the new Curve to the daemon

### Requirement: Leakshield pages
The app SHALL show pages for every Leakshield setting group. A field that is not a Verified field SHALL show as read-only with the reason. The app SHALL NOT offer a write control for a Verification write.

#### Scenario: Read-only reason
- **WHEN** the firmware version has no layout
- **THEN** each field is read-only and the page says why

### Requirement: Pump runtime per day
The Overview tab of a Leakshield SHALL chart the pump runtime of the last 14 local days: for each day, the sum of the durations of the refills of the daemon history (`GetRefillHistory`), at most 1000 s. A day above 30 s SHALL show as concerning and a day above 60 s as critical. The limit line SHALL come from the warning limit field of the device, and the chart SHALL show no limit line while the device gives none.

#### Scenario: Days of a refill
- **WHEN** the history holds two refills of 12 s and 8 s on the same local day
- **THEN** the bar of that day reads 20 s

#### Scenario: High day
- **WHEN** a day sums to 61 s
- **THEN** its bar is critical

#### Scenario: Limit from the device
- **WHEN** the warning limit field of the device holds 5
- **THEN** the dashed line is at 5 s per day

### Requirement: Profile and rule management
The app SHALL let the user create, edit, activate and revoke Profiles, set the Leakshield slot of a Profile, edit Rule groups, and set Immediate switching. The app SHALL NOT offer to delete the Default Profile.

#### Scenario: Activate by hand
- **WHEN** the user activates a Profile in the app
- **THEN** the daemon holds a manual activation

#### Scenario: Leakshield part failed
- **WHEN** the Leakshield part of a Profile switch fails with a write error
- **THEN** the app shows "Curves applied, Leakshield failed" and offers the restore

### Requirement: Languages
The app SHALL offer English and Italian.

#### Scenario: Italian system
- **WHEN** the system language is Italian
- **THEN** the app shows Italian text

### Requirement: Authorization prompts
The app SHALL NOT ask for a password and SHALL NOT mark a control with a lock icon for authorization; the read-only lock of a field that is not Verified stays. A user outside the group `wheel` gets the system polkit dialog. The app SHALL report a denial. The app SHALL warn before an action that lowers the protection and SHALL NOT refuse the choice of the user.

#### Scenario: Wheel member sets RELEASE
- **WHEN** a wheel member in the active local session confirms Mode RELEASE
- **THEN** the app sends the change and no password dialog shows

#### Scenario: Denied
- **WHEN** a user outside the group `wheel` cancels the polkit dialog of an action
- **THEN** the action does not run and the app says it was denied

### Requirement: Screen data from the contract
The app SHALL take the BIOS snapshot, the fan sensor of a Channel, the Curve readout, the Feed state and values, the Alarm history, the Profile dwell, the failed write backup and the resolved alarm responses from the daemon, and SHALL NOT compute again what the daemon decides.

#### Scenario: Alarm history after an app restart
- **WHEN** the app starts after an Alarm rose and cleared while it was closed
- **THEN** the Alarms history table and the History markers show that instance

#### Scenario: Restore offer after an app restart
- **WHEN** the app starts after a failed Profile write
- **THEN** the Profiles screen offers the restore of the backup of `FailedWriteBackup`

#### Scenario: Diff before an activation
- **WHEN** the user activates a Profile whose slot differs from `ActiveSlot`
- **THEN** the app shows the fields that differ between the stored settings of that slot and the device before it activates
