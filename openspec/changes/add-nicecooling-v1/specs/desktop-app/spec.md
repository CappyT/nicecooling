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

### Requirement: Time of the fill level measurement
The Overview tab of a Leakshield SHALL show the time of the last fill level measurement as the time of the newest device event "fill level measurement finished" in `GetEvents`. It SHALL show no time while no such event exists, and SHALL NOT show the time of a sensor sample in its place.

#### Scenario: Measurement event
- **WHEN** the event list holds two finished measurements at 09:12 and 14:40
- **THEN** the Fill level row shows "measured 14:40"

#### Scenario: No measurement event
- **WHEN** the event list holds no finished measurement
- **THEN** the Fill level row shows its value and no time

### Requirement: Display page previews
The Display tab of a Leakshield SHALL show, for each of the 10 pages, the image that the device draws for it (`GetDisplayPreview`), in the page list and on the card of the selected page. The app SHALL draw each pixel as one square at a whole scale, with row 0 at the top and bit 7 of each byte at the left. It SHALL NOT draw the content of a page by itself, and SHALL say that the Leakshield shows the page for 30 s after a read.

#### Scenario: Page selected
- **WHEN** the user selects page 3
- **THEN** the app calls `GetDisplayPreview` for page 2 of the contract and shows the returned image in the list and on the page card

#### Scenario: Orientation of the image
- **WHEN** the returned rows start with the byte 0x80
- **THEN** the lit pixel is the top left pixel of the preview

### Requirement: Reads of a display page preview
The app SHALL read the image of a page when the user selects the page, when the Display tab opens and the selected page has no image yet, on "Refresh preview", and after each write that changes the page. It SHALL keep the last image of each page while it runs. It SHALL send no preview read to a read-only device, to a Leakshield that does not answer, or for a page without a template.

#### Scenario: Write of a page
- **WHEN** the user changes the decimals of a value of page 2
- **THEN** the app writes the field and then reads the image of page 2 again

#### Scenario: Read-only device
- **WHEN** the firmware of the Leakshield has no layout
- **THEN** "Refresh preview" is disabled with the reason and the app sends no call

### Requirement: Preview without an image
A preview without an image SHALL show the reason in its place and SHALL NOT be a dark box. The reasons are: not read yet, a read that runs, a page without a template, a page that the device does not show (`PageNotShown`), a read-only device, a Leakshield that does not answer, a daemon without the call, and a firmware without the command.

#### Scenario: Page not shown
- **WHEN** the daemon answers `PageNotShown`
- **THEN** the preview of the page shows "Not shown" and the page card shows the reason

#### Scenario: Daemon without the call
- **WHEN** the daemon has no `GetDisplayPreview`
- **THEN** the preview shows "Not available" and the page card says that the daemon cannot read the display

### Requirement: Display page template and options
The Display tab SHALL let the user set, for each page, the template (one of the 14 templates of the registry, or "No page", the value 255), the option "Show in standby" (bit 0 of `page_options`) and the option "Hide in normal operation" (bit 1). The template choice SHALL show a schematic of the layout of each template that the app draws.

#### Scenario: No page
- **WHEN** the user picks "No page" for page 5
- **THEN** the app writes `none` as the template of page 5

#### Scenario: Hide in normal operation
- **WHEN** the user turns on "Hide in normal operation" for a page with the options 1
- **THEN** the app writes the options 3 for that page

### Requirement: Slots of a display page
For the selected page the app SHALL show exactly the slots that its template uses, as (values, texts, icons, bars): logo (0, 0, 0, 0), chart (1, 0, 0, 0), values-1 (1, 1, 1, 0), values-2 (2, 2, 2, 0), values-4 (4, 4, 0, 0), values-3a (3, 1, 2, 0), values-3b (3, 1, 1, 0), time (0, 0, 0, 0), bars-2 (2, 2, 2, 2), bars-4a (4, 0, 2, 4), bars-4b (4, 1, 0, 4), value-and-bar (1, 1, 0, 1), value-and-time (1, 1, 1, 0), leakshield-state (0, 0, 0, 0). The logo template SHALL also show the logo.

#### Scenario: Slots of a template
- **WHEN** the selected page has the template bars-4a
- **THEN** the page card has 4 values with data source, decimals, minimum and maximum, 2 icons and no text

#### Scenario: Template without a slot
- **WHEN** the selected page has the template time
- **THEN** the page card has no value, no text and no icon, and says that the template has no settings

### Requirement: Values, texts and icons of a display page
A value SHALL have a data source (a device sensor, a Software sensor or none, written as the sensor index times 4) and 0 to 3 decimals. A bar SHALL have a minimum and a maximum from -32000 to 32000. A text SHALL be one of the 40 names of the device, or none, or an own text. An icon SHALL be one of the 16 icons of the device, or none.

#### Scenario: Limit out of range
- **WHEN** the user types 99999 as the maximum of a bar
- **THEN** the app writes 32000

#### Scenario: Icon of a slot
- **WHEN** the user picks icon 16 for icon 2 of page 3
- **THEN** the app writes 15 into the second entry of `page_icons` of page 3

### Requirement: Own text of a display page
An own text SHALL go with `SetDisplayName` into a page text (names 20 to 34): the page text of the slot when no other text slot in use shows it, else the first empty page text that no text slot in use shows, else the first page text that no text slot in use shows. The slot SHALL then select that name. The app SHALL NOT change a sensor name or a lighting name for an own text.

#### Scenario: Own text into a free page text
- **WHEN** the page texts 20 to 22 hold a text, no page shows a text, and the user saves the own text "CPU loop" for text 1 of page 2
- **THEN** the app calls `SetDisplayName` with the index 23 and the text, and writes 23 into the first entry of `page_names` of page 2

#### Scenario: All page texts in use
- **WHEN** text slots in use of other pages show all 15 page texts
- **THEN** "Own text" is disabled with the reason

### Requirement: Logo and icon import
The Display tab SHALL import a PNG file as the logo (128 x 64 pixels) or as an icon (23 x 23 pixels), with one import path for both. The app SHALL show the converted result before the write and SHALL write exactly that result with `SetDisplayLogo` or `SetDisplayIcon`. An imported icon SHALL replace the icon that the icon slot shows. The app SHALL refuse a file that is not a readable PNG image, with the reason.

#### Scenario: Not a PNG file
- **WHEN** the user picks a text file
- **THEN** the dialog says that the file is not a PNG image, and "Write the logo" is disabled

#### Scenario: Result before the write
- **WHEN** the user moves the threshold
- **THEN** the shown result changes, and no call goes to the daemon until the user presses the write button

### Requirement: Conversion of an imported picture
The app SHALL convert the picture to lit and dark pixels: a pixel is lit when its brightness is at or above a threshold that the user moves, 50 % at the start, and a transparent pixel is dark. The user SHALL be able to swap lit and dark. The app SHALL scale a file of another size to fit the target with its aspect ratio, in the middle on a dark ground, SHALL say so, and SHALL NOT refuse a file because of its size.

#### Scenario: File of the display size
- **WHEN** the user picks a PNG file of 128 x 64 pixels and the threshold is 50 %
- **THEN** each pixel with a brightness of 50 % or more is lit in the result, and the file is not scaled

#### Scenario: File of another size
- **WHEN** the user picks a white PNG file of 256 x 256 pixels as the logo
- **THEN** the result has a lit square of 64 x 64 pixels in the middle, and the dialog says that the file was scaled

### Requirement: Software sensor scale and sources in the app
The Sensors tab of a Leakshield SHALL show a scale and an offset for each Software sensor slot (`GetSoftwareSensorScales`) and SHALL write a change of one of them with `SetSoftwareSensorScale`, keeping the other. It SHALL show the value that the daemon sends: the source value times the scale plus the offset. The data source select SHALL list the sensors of `GetSoftwareSensorSources`, plus the source that the slot has. With a daemon that gives no list, it SHALL list every sensor.

#### Scenario: New scale
- **WHEN** slot 3 has scale 2 and offset -10.5 and the user types the scale 0.5
- **THEN** the app calls `SetSoftwareSensorScale` with slot 3, scale 0.5 and offset -10.5

#### Scenario: Sources of the daemon
- **WHEN** the daemon lists one sensor as a source
- **THEN** the select of an empty slot has that sensor only

### Requirement: Device state card
The System tab of a Leakshield SHALL show a card "Device state" with the maintenance value and the counters of `GetCounters`: the pump runtime, the pump runtime in short cycles, the pump revolutions, the fill valve cycles, the release valve cycles, the operating time and the power-ups. A time SHALL show in hours and minutes, or in minutes and seconds below one hour. The card SHALL have no control. With a daemon that gives no counters, each counter SHALL show "--" and the card SHALL say why.

#### Scenario: Counters of the device
- **WHEN** `GetCounters` gives a pump runtime of 11431 s
- **THEN** the row of the pump runtime shows "3 h 10 min"

#### Scenario: No counters
- **WHEN** the daemon has no `GetCounters`
- **THEN** each counter shows "--" and the card says that the daemon gave no counters

### Requirement: Event list reset and Refill pause in the app
The Status tab of a Leakshield SHALL have a "Reset" control of the event list. It SHALL ask before it sends `ResetEventLog`, and SHALL read the event list again after the call. The Overview tab SHALL show, while a Refill runs, one control "Pause or resume" that sends `ToggleRefillPause`. Both controls SHALL be disabled, with the reason, for a read-only device, and "Reset" also while the daemon or the Leakshield does not answer.

#### Scenario: Reset after the question
- **WHEN** the user presses "Reset" and then "Delete events"
- **THEN** the app calls `ResetEventLog` once and shows the empty event list

#### Scenario: Question cancelled
- **WHEN** the user presses "Reset" and then "Cancel"
- **THEN** the app sends no call

#### Scenario: Refill runs
- **WHEN** the running Action is the Refill
- **THEN** the action bar has "Pause or resume" and "Cancel"

### Requirement: Controls of a write that is not possible
The app SHALL warn and SHALL NOT refuse a choice of the user that the device accepts. A control whose write is not possible (a read-only device, a daemon or a Leakshield that does not answer, a field that is not a Verified field) SHALL be disabled and SHALL give the reason next to it or as its tooltip.

#### Scenario: Daemon offline
- **WHEN** the daemon is offline
- **THEN** "Change template", "Own text", "Change icon", "Change logo" and "Refresh preview" are disabled with the reason "The daemon is offline."

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
