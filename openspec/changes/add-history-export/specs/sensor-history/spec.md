# Sensor history

## Purpose

Defines the history store of the app. It covers which sensors the app records, how long the 1 s samples and the 1 minute rows stay, and how a chart or an export chooses between them. It also covers the CSV export, the delete of a range and of the whole history, the compaction of the database file and the schema version rule. The app does all of this in the user session. The daemon does none of it.

## ADDED Requirements

### Requirement: Recorded sensors
The app SHALL record one sample per second for every sensor and every Channel that the daemon reports, except the ids in the list of not recorded sensors. The list SHALL be empty by default. A sensor that the daemon reports for the first time SHALL be recorded.

#### Scenario: Every sensor by default
- **WHEN** the daemon reports 48 sensors and 14 Channels and the list is empty
- **THEN** the store gets 62 samples for each second

#### Scenario: New sensor
- **WHEN** a Derived sensor `derived_3` is created at 10:00:00
- **THEN** the store holds samples of `derived_3` from its first value on

### Requirement: Recording switch
The Settings screen SHALL have one switch for each sensor and each Channel in the dialog "Recorded sensors". A switch that is off SHALL put the id in the list of not recorded sensors. The app SHALL then store no sample and no minute row of that sensor for a later time. The rows that the store holds SHALL stay. The list SHALL survive a restart of the app.

#### Scenario: Switch off
- **WHEN** the user turns the switch of `gpu_temp` off at 10:00:30
- **THEN** the store gets no sample of `gpu_temp` with a time after 10:00:31, and the rows before that time stay

#### Scenario: Minute of the change
- **WHEN** the switch of `gpu_temp` goes off at 10:00:30
- **THEN** the minute row of 10:00 holds 31 samples or fewer, and no minute row of 10:01 exists

#### Scenario: Switch on again
- **WHEN** the user turns the switch on at 10:20:00
- **THEN** the store gets samples from the next second, and the chart shows a gap from 10:00:31 to 10:20:00

#### Scenario: Restart
- **WHEN** the switch of `gpu_temp` is off and the app restarts
- **THEN** the switch is still off and the store gets no sample of `gpu_temp`

### Requirement: Not recorded sensor in the chart
The sensor picker of the History screen and the legend SHALL mark a sensor of the list with the text "Not recorded". The chart SHALL still draw the stored rows of that sensor. When every shown series is in the list, the label of a gap SHALL be "No data: not recorded".

#### Scenario: Old rows of a sensor that is not recorded
- **WHEN** `gpu_temp` was recorded until 10:00:30 and the chart shows 09:30 to 10:30
- **THEN** the chart draws the line until 10:00:30 and the legend row has the tag "Not recorded"

#### Scenario: Gap label
- **WHEN** the chart shows only `gpu_temp`, which is not recorded
- **THEN** the gap after 10:00:30 has the label "No data: not recorded"

#### Scenario: Mixed series
- **WHEN** the chart shows `gpu_temp` and a recorded sensor
- **THEN** the gaps come from the recorded sensor and have the label "No data: the app did not run"

### Requirement: Retention periods
The app SHALL have two retention settings. The sample period SHALL be 24 hours, 48 hours, 7 days or 30 days, with the default 48 hours. The minute period SHALL be 30 days, 90 days, 1 year, 2 years or 5 years, with the default 1 year. Both settings SHALL survive a restart of the app. A stored value that is not one of the choices SHALL read as the default of that setting, and the other settings SHALL keep their values.

#### Scenario: Defaults
- **WHEN** the settings file has no history table
- **THEN** the sample period is 48 hours and the minute period is 1 year

#### Scenario: Stored choice
- **WHEN** the user sets the sample period to 7 days and the minute period to 2 years and restarts the app
- **THEN** the Settings screen shows 7 days and 2 years

#### Scenario: Unknown value
- **WHEN** the settings file has the sample period `"3d"` and the language `"it"`
- **THEN** the sample period is 48 hours and the language is Italian

### Requirement: Prune
At its start and then once per hour, the app SHALL delete each sample that is older than the sample period and each minute row that is older than the minute period.

#### Scenario: Sample older than the period
- **WHEN** the sample period is 24 hours and a sample is 25 hours old at the prune
- **THEN** the prune deletes the sample and its minute row stays

#### Scenario: Sample inside the period
- **WHEN** the sample period is 7 days and a sample is 6 days old at the prune
- **THEN** the sample stays

#### Scenario: Minute row older than the period
- **WHEN** the minute period is 90 days and a minute row is 91 days old at the prune
- **THEN** the prune deletes the minute row

### Requirement: Change of a retention period
A change to a shorter period that deletes one row or more SHALL show a confirmation that names the number of rows. After the confirmation, the app SHALL store the setting and SHALL start the delete of these rows within 2 s. Without the confirmation, the setting SHALL keep its old value. A change that deletes no row SHALL apply with no question.

#### Scenario: Shorter sample period
- **WHEN** the user changes the sample period from 48 hours to 24 hours and the store holds 5 184 000 samples older than 24 hours
- **THEN** the dialog reads "This deletes 5 184 000 samples older than 24 hours." and has the buttons "Cancel" and "Delete samples"

#### Scenario: Confirmed
- **WHEN** the user presses "Delete samples"
- **THEN** the setting is 24 hours, the delete starts within 2 s, and after it the store holds no sample older than 24 hours

#### Scenario: Cancelled
- **WHEN** the user presses "Cancel"
- **THEN** the setting is still 48 hours and no row is deleted

#### Scenario: Longer period
- **WHEN** the user changes the sample period from 48 hours to 7 days
- **THEN** the setting applies with no question, and the store holds the samples of the last 48 hours as before

### Requirement: Size on disk
The Settings screen SHALL show the size of the history database on disk. It SHALL show an estimate for each period. The estimate of the samples SHALL be the number of recorded series times the seconds of the sample period times 23 bytes. The estimate of the minute rows SHALL be the number of recorded series times the minutes of the minute period times 44 bytes. An estimate above 2 GB SHALL have the Warning icon. The app SHALL NOT refuse a choice because of its size.

#### Scenario: Default periods
- **WHEN** 60 series are recorded with 48 hours and 1 year
- **THEN** the estimates read "about 238 MB" and "about 1.4 GB" with no Warning icon

#### Scenario: Large choice
- **WHEN** 60 series are recorded and the user picks the sample period 30 days
- **THEN** the estimate reads "about 3.6 GB" with the Warning icon, and the choice applies

### Requirement: Samples or minute rows in a chart
The store holds the samples of a time range when no shown series has, in that range, a minute row that is complete before the first sample of that series in the range. A chart window SHALL draw the 1 s samples when it is 48 hours long or shorter and the store holds its samples. Every other window SHALL draw the 1 minute rows. All series of one window SHALL use the same kind.

#### Scenario: Window inside a long sample period
- **WHEN** the sample period is 7 days for 10 days, and the window is the 24 hours that end 3 days ago
- **THEN** the chart draws the 1 s samples

#### Scenario: Window longer than 48 hours
- **WHEN** the sample period is 7 days and the window is 7 days long
- **THEN** the chart draws the 1 minute rows

#### Scenario: Period made longer one day ago
- **WHEN** the sample period changed from 48 hours to 7 days one day ago, and the window is the 24 hours that end 4 days ago
- **THEN** the chart draws the 1 minute rows, because the store holds no samples of that window

#### Scenario: First hour after the install
- **WHEN** the first sample of the store is 30 minutes old and the window is the last hour
- **THEN** the chart draws the 1 s samples of the 30 minutes

### Requirement: Export scope
The History screen SHALL export the shown range and the shown series to one CSV file. The shown series are the selected series that the legend does not hide, in the order of the legend. The range SHALL end at the present time when the window ends later. The export SHALL be off, with the reason, when no series is shown.

#### Scenario: Hidden series
- **WHEN** four sensors are selected and the legend hides one
- **THEN** the file has the time column and three value columns

#### Scenario: Zoomed window
- **WHEN** the window is zoomed to 14:00:00 until 14:10:00 of one day
- **THEN** the file holds only rows with a time from 14:00:00 to 14:10:00

#### Scenario: No series
- **WHEN** no sensor is selected
- **THEN** "Export to CSV" is off with the text "Select sensors to export."

### Requirement: Export data kind
The export SHALL write the 1 s samples when the store holds the samples of the range, and the 1 minute rows for every other range. One file SHALL hold one kind. When the store holds the samples, the dialog SHALL also offer the 1 minute rows. The option "Minimum and maximum" SHALL exist for the 1 minute rows only and SHALL be off by default.

#### Scenario: Range inside the sample period
- **WHEN** the sample period is 48 hours and the range is the last 24 hours
- **THEN** the dialog shows "1 s samples" as the choice, and the file holds one row per second with data

#### Scenario: Older range
- **WHEN** the sample period is 48 hours and the range is the last 7 days
- **THEN** the dialog shows "1 minute rows" with no other choice, and the file holds the mean of each minute

#### Scenario: Minute rows by choice
- **WHEN** the sample period is 30 days, the range is the last 7 days and the user picks "1 minute rows"
- **THEN** the file holds 10 081 rows or fewer

### Requirement: CSV file format
The file SHALL be UTF-8 text with no byte order mark. Each row SHALL end with one line feed. The separator SHALL be the comma. A field that holds a comma, a double quote or a line break SHALL be in double quotes, with each double quote written twice. The file SHALL have one header row and then the data rows, the oldest first. The file SHALL NOT depend on the language of the app or of the system.

#### Scenario: Plain file
- **WHEN** the user exports two series of 10 seconds
- **THEN** the file has 11 lines, each line has 2 commas, and the first 3 bytes are not a byte order mark

#### Scenario: Name with a comma
- **WHEN** a sensor has the name `Radiator, top`
- **THEN** its header field is `"Radiator, top (°C)"`

#### Scenario: German system
- **WHEN** the system language is German and a value is 31.4
- **THEN** the field is `31.4`

### Requirement: CSV header
The first header field SHALL be `time`. The field of a series SHALL be its name, a space and its unit text in round brackets, for example `Water temperature (°C)`. A series without a unit SHALL have its name only. When two shown series have the same name and unit, each of them SHALL have its id in square brackets after the name.

#### Scenario: Two series
- **WHEN** the shown series are "Water temperature" in °C and "Waterpump" in %
- **THEN** the header row is `time,Water temperature (°C),Waterpump (%)`

#### Scenario: Same name twice
- **WHEN** two sensors with the ids `nct6798_temp1` and `nct6798_temp2` both have the name "Temperature" and the unit °C
- **THEN** the header fields are `Temperature [nct6798_temp1] (°C)` and `Temperature [nct6798_temp2] (°C)`

### Requirement: CSV time column
The time field SHALL be the local time of the system at that instant in the form `YYYY-MM-DDTHH:MM:SS+HH:MM`, with the UTC offset that holds at that instant. The time of a 1 minute row SHALL be the start of its minute.

#### Scenario: Winter time in Rome
- **WHEN** the system time zone is Europe/Rome and a sample has the Unix time 1795623185
- **THEN** its time field is `2026-11-25T17:13:05+01:00`

#### Scenario: Hour that occurs twice
- **WHEN** the time zone is Europe/Rome and two samples have the Unix times 1792888200 and 1792891800
- **THEN** their time fields are `2026-10-25T02:30:00+02:00` and `2026-10-25T02:30:00+01:00`

#### Scenario: Minute row
- **WHEN** a minute row covers 17:13:00 to 17:13:59
- **THEN** its time field ends with `T17:13:00+01:00`

### Requirement: CSV rows and values
The file SHALL have one row for each time stamp at which one exported series or more has a value. A series with no value at that time SHALL have an empty field. A number SHALL have a decimal point, 3 decimals at most, no trailing zero, no digit grouping and no exponent.

#### Scenario: One series has a gap
- **WHEN** series A has values at 17:13:05 and 17:13:06 and series B only at 17:13:05
- **THEN** the rows are `...T17:13:05+01:00,31.4,52` and `...T17:13:06+01:00,31.5,`

#### Scenario: No value in any series
- **WHEN** no exported series has a value from 17:20:00 to 17:25:00
- **THEN** the file has no row for that time

#### Scenario: Rounding
- **WHEN** the mean of a minute is 31.416666
- **THEN** the field is `31.417`

#### Scenario: Whole number
- **WHEN** a speed sample is 2840.0
- **THEN** the field is `2840`

### Requirement: Minimum and maximum columns
With the option "Minimum and maximum", each series SHALL have three columns: the mean with the header of the series, then the minimum with ` min` after the name, then the maximum with ` max` after the name.

#### Scenario: One series with the option
- **WHEN** the user exports "Water temperature" in °C as 1 minute rows with the option on
- **THEN** the header row is `time,Water temperature (°C),Water temperature min (°C),Water temperature max (°C)`

#### Scenario: Values of a minute
- **WHEN** the minute of 17:13 has the mean 31.4, the minimum 30.9 and the maximum 31.8
- **THEN** the row is `2026-11-25T17:13:00+01:00,31.4,30.9,31.8`

### Requirement: Export dialog
Before the file choice, the app SHALL show the range, the number of series, the data kind, the highest possible number of rows and an estimate of the file size. The estimate SHALL be the rows times the sum of 26 bytes and 9 bytes for each value column. An estimate above 100 MB SHALL have the Warning icon. The app SHALL NOT refuse an export because of its size.

#### Scenario: One day of four series
- **WHEN** the range is 24 hours, four series are shown and the kind is 1 s samples
- **THEN** the dialog reads "Up to 86 401 rows, about 5.4 MB"

#### Scenario: Large export
- **WHEN** the sample period is 30 days, the range is 30 days and 60 series are shown as 1 s samples
- **THEN** the dialog reads "Up to 2 592 001 rows, about 1.5 GB" with the Warning icon, and "Export" stays on

### Requirement: Export file
The app SHALL ask for the file with the file dialog of the system and propose the name `nicecooling-history-<start>-<end>.csv`, with both times as `YYYYMMDDTHHMMSS` in local time. It SHALL write a temporary file in the same directory and give it the chosen name when the export is complete. It SHALL show the progress and a "Cancel" button. The app SHALL keep recording during an export.

#### Scenario: Proposed name
- **WHEN** the range is 2026-11-24 17:13:05 to 2026-11-25 17:13:05 local time
- **THEN** the proposed name is `nicecooling-history-20261124T171305-20261125T171305.csv`

#### Scenario: Complete export
- **WHEN** an export of 86 400 rows ends
- **THEN** the file has the chosen name, and the app shows "Exported 86 400 rows" with the path

#### Scenario: Cancel
- **WHEN** the user presses "Cancel" at 40%
- **THEN** no file with the chosen name exists and the temporary file is removed

#### Scenario: Recording goes on
- **WHEN** an export runs for 30 s
- **THEN** the store gets the samples of these 30 s

### Requirement: Export failure
When the app cannot write the file, it SHALL remove the temporary file, show the error of the system with the path, and leave the history unchanged. A file that existed under the chosen name SHALL keep its old content.

#### Scenario: Directory without write access
- **WHEN** the user picks a file in a directory that the user cannot write
- **THEN** the app shows "The file cannot be written" with the path and the error text, and no file is created

#### Scenario: Disk full during the export
- **WHEN** the disk is full after 10 000 rows
- **THEN** the temporary file is removed and a file that existed under the chosen name is unchanged

### Requirement: Delete of a range
The History screen SHALL delete the rows of the shown range for the shown series or for all series, as the user picks. It SHALL delete the samples with a time in the range and each minute row whose minute overlaps the range. It SHALL also drop the samples of the range that are not yet in the store. The app SHALL keep recording. A delete SHALL NOT delete a Note or a View.

#### Scenario: Shown series
- **WHEN** the range is 14:00:00 to 14:10:00, the scope is the three shown series and the user confirms
- **THEN** the store holds no sample of these three series from 14:00:00 to 14:10:00, and the rows of the other series stay

#### Scenario: Minute at the edge
- **WHEN** the range is 14:00:30 to 14:10:30
- **THEN** the minute rows of 14:00 to 14:10 of the scope are deleted, and the minute rows of 13:59 and 14:11 stay

#### Scenario: Range that ends now
- **WHEN** the range ends at the present time and the user confirms at 15:00:00
- **THEN** no sample of the scope with a time up to 15:00:00 appears in the store later, and the store gets the samples after 15:00:00

#### Scenario: All series with no sensor selected
- **WHEN** no sensor is selected
- **THEN** the scope "Shown series" is off with the reason, and the scope "All series" works

### Requirement: Confirmation of a range delete
Before a range delete, the app SHALL show a confirmation that names the exact number of samples, the exact number of minute rows, the number of series and the range. The dialog SHALL say that the delete cannot be undone. The button SHALL be the destructive button "Delete". It SHALL be off while the app counts and when the count is 0.

#### Scenario: Count
- **WHEN** the range is 14:00:00 to 14:09:59 and the scope holds 1 800 samples and 30 minute rows of 3 series in it
- **THEN** the dialog reads "Delete 1 800 samples and 30 minute rows of 3 series from 14:00:00 to 14:09:59? You cannot undo this."

#### Scenario: Scope change
- **WHEN** the user changes the scope from "Shown series" to "All series"
- **THEN** the numbers change to the count of all series before "Delete" is on again

#### Scenario: Nothing to delete
- **WHEN** the scope holds no row in the range
- **THEN** the dialog reads "No rows in this range." and "Delete" is off

#### Scenario: Cancel
- **WHEN** the user presses "Cancel"
- **THEN** no row is deleted

### Requirement: Delete of the whole history
The Settings screen SHALL delete every sample, every minute row and every series entry of the store. The confirmation SHALL name the exact number of samples, of minute rows and of series, and SHALL say that the delete cannot be undone. It SHALL have the switch "Also delete the Notes" with the number of Notes, off by default. The Views SHALL stay. The app SHALL keep recording.

#### Scenario: Confirmation text
- **WHEN** the store holds 10 368 000 samples and 525 600 minute rows of 62 series, and 12 Notes
- **THEN** the dialog reads "Delete 10 368 000 samples and 525 600 minute rows of 62 series? You cannot undo this." and the switch reads "Also delete the 12 Notes"

#### Scenario: Confirmed with the switch off
- **WHEN** the user presses "Delete history"
- **THEN** the store holds no sample and no minute row from before that time, the 12 Notes and the Views stay, and the store gets the next samples

#### Scenario: Switch on
- **WHEN** the user turns the switch on and presses "Delete history"
- **THEN** the store holds no Note

### Requirement: Compaction
The app SHALL compact the database file after a delete, a prune or a change of a period that removed 1 000 000 rows or more in one operation. The file SHALL then give the free space back to the disk. A smaller delete SHALL leave the file size as it is. When the compaction fails, the app SHALL show the error, and the deleted rows SHALL stay deleted.

#### Scenario: Whole history
- **WHEN** the user deletes a history of 10 368 000 samples in a file of 238 MB
- **THEN** after the compaction the database files are smaller than 1 MB in sum

#### Scenario: Small range delete
- **WHEN** a range delete removes 1 800 samples
- **THEN** the app does not compact the file

#### Scenario: Hourly prune
- **WHEN** the hourly prune removes 216 000 samples of 60 series
- **THEN** the app does not compact the file

#### Scenario: Gap of a compaction
- **WHEN** a compaction runs from 15:00:00 to 15:00:20
- **THEN** the history has a gap of 20 s or less at that time, and the store gets the samples after 15:00:20

#### Scenario: No space for the compaction
- **WHEN** the disk has no free space for the compaction
- **THEN** the app shows "The file was not compacted" with the error text, and the rows stay deleted

### Requirement: One history operation at a time
While an export, a delete, a prune after a change of a period or a compaction runs, the app SHALL start no second one of these. The commands that start one SHALL be off with the text "A history operation is running." The chart SHALL still read the store, and Views and Notes SHALL still work.

#### Scenario: Delete during an export
- **WHEN** an export runs and the user opens the menu of the History screen
- **THEN** "Delete data" and "Export to CSV" are off with the text "A history operation is running."

#### Scenario: Chart during a compaction
- **WHEN** a compaction runs and the user changes the range to 7 days
- **THEN** the chart draws the 7 days

### Requirement: Schema version of the database
The app SHALL bring a database of schema version 1 to version 2 at the first open and SHALL keep every row. The app SHALL NOT write a database with a schema version above 2. It SHALL then record nothing, and the History screen SHALL show the database error state with the text "The history database is from a newer version of nicecooling." and the path.

#### Scenario: Version 1 database
- **WHEN** the app opens a version 1 database with 1 000 samples
- **THEN** the schema version is 2, the 1 000 samples are in the store and the Notes list is empty

#### Scenario: Newer database
- **WHEN** the app opens a database with the schema version 3
- **THEN** the file does not change, the app records no sample, and the History screen shows the text with the path

### Requirement: No daemon and no password
Recording switch, retention settings, export, delete and compaction SHALL work in the app alone. They SHALL make no call to the daemon, SHALL ask no password and SHALL work while the daemon is offline.

#### Scenario: Daemon offline
- **WHEN** the daemon is offline and the user exports the last 24 hours
- **THEN** the export writes the file

#### Scenario: Delete without a password
- **WHEN** a user outside the group `wheel` deletes a range
- **THEN** the delete runs with no password dialog
