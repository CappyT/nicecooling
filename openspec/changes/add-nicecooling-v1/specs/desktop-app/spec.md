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

### Requirement: Profile and rule management
The app SHALL let the user create, edit, activate and revoke Profiles and their rules, and set Immediate switching. The app SHALL NOT offer to delete the Default Profile.

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
When an action needs authentication, the app SHALL show the polkit prompt and SHALL report a denial.

#### Scenario: Denied
- **WHEN** the user cancels the prompt of a critical action
- **THEN** the action does not run and the app says it was denied
