# Sensor history

## Purpose

Defines the history store of the app. It covers which sensors the app records and how long the Samples and the Minute rows stay. It covers how a chart or an export chooses between them. It also covers the CSV export, the delete of a range and of the whole history, the compaction of the database file and the schema version rule. The app does all of this in the user session. The daemon does none of it.

## ADDED Requirements

### Requirement: Recorded sensors
The app SHALL record one Sample per second for every sensor and every Channel that the daemon reports. It SHALL NOT record the ids in the list of not recorded sensors. The list SHALL be empty by default. A sensor that the daemon reports for the first time SHALL be recorded.

#### Scenario: Every sensor by default
- **WHEN** the daemon reports 48 sensors and 14 Channels and the list is empty
- **THEN** the store gets 62 Samples for each second

#### Scenario: New sensor
- **WHEN** a Derived sensor `derived_3` is created at 10:00:00
- **THEN** the store holds Samples of `derived_3` from its first value on

### Requirement: Recording switch
The Settings screen SHALL have one switch for each sensor and each Channel in the dialog "Recorded sensors". A switch that is off SHALL put the id in the list of not recorded sensors. The app SHALL then store no Sample and no Minute row of that sensor for a later time. The rows that the store holds SHALL stay. The list SHALL survive a restart of the app.

#### Scenario: Switch off
- **WHEN** the user turns the switch of `gpu_temp` off at 10:00:30
- **THEN** the store gets no Sample of `gpu_temp` with a time of 10:00:31 or later, and the rows before that time stay

#### Scenario: Minute of the change
- **WHEN** the switch of `gpu_temp` goes off at 10:00:30
- **THEN** the Minute row of 10:00 holds 31 Samples or fewer, and no Minute row of 10:01 exists

#### Scenario: Switch on again
- **WHEN** the user turns the switch on at 10:20:00
- **THEN** the store gets Samples from the next second, and the chart shows a gap from 10:00:31 to 10:20:00

#### Scenario: Restart
- **WHEN** the switch of `gpu_temp` is off and the app restarts
- **THEN** the switch is still off and the store gets no Sample of `gpu_temp`

### Requirement: Rows of the dialog "Recorded sensors"
The dialog SHALL list the sensors and the Channels of the last snapshot, which is the last full state that the app read from the daemon. It SHALL also list each id of the list of not recorded sensors that the snapshot does not hold, with the id as the name. The dialog SHALL work while the daemon is offline. The row "Recorded sensors" of the card "History" SHALL read "<n> of <m> recorded".

#### Scenario: Row of the card
- **WHEN** the snapshot holds 62 sensors and Channels and one of them is in the list
- **THEN** the row reads "61 of 62 recorded"

#### Scenario: Sensor of the list that the daemon does not report
- **WHEN** `gpu_temp` is in the list and the daemon reports no such sensor
- **THEN** the dialog has a row `gpu_temp` with its switch off, and the user can turn it on

#### Scenario: Daemon offline after a connection
- **WHEN** the daemon reported 62 sensors and Channels and then went offline
- **THEN** the dialog lists the 62 rows and the switches work

#### Scenario: App started with the daemon offline
- **WHEN** the app started with the daemon offline and the list holds `gpu_temp`
- **THEN** the dialog has the row `gpu_temp` only and the text "The daemon is offline. The other sensors show when it connects.", and the row of the card reads "1 not recorded"

### Requirement: Not recorded sensor in the chart
The sensor picker of the History screen and the legend SHALL mark a sensor of the list with the tag "Not recorded". The chart SHALL still draw the stored rows of that sensor. When every shown Series is in the list, the label of a gap SHALL be "No data: not recorded".

#### Scenario: Old rows of a sensor that is not recorded
- **WHEN** `gpu_temp` was recorded until 10:00:30 and the chart shows 09:30 to 10:30
- **THEN** the chart draws the line until 10:00:30 and the legend row has the tag "Not recorded"

#### Scenario: Gap label
- **WHEN** the chart shows only `gpu_temp`, which is not recorded
- **THEN** the gap after 10:00:30 has the label "No data: not recorded"

#### Scenario: Mixed Series
- **WHEN** the chart shows `gpu_temp` and a recorded sensor
- **THEN** the gaps come from the recorded sensor and have the label "No data: the app did not run"

### Requirement: Sample period and Minute period
The app SHALL have two settings. The Sample period SHALL be 24 hours, 48 hours, 7 days or 30 days, with the default 48 hours. The Minute period SHALL be 30 days, 90 days, 1 year, 2 years or 5 years, with the default 1 year. A year SHALL be 365 days. Both settings SHALL survive a restart of the app. A stored value that is not one of the choices SHALL read as the default of that setting, and the other settings SHALL keep their values.

#### Scenario: Defaults
- **WHEN** no period is stored
- **THEN** the Sample period is 48 hours and the Minute period is 1 year

#### Scenario: Stored choice
- **WHEN** the user sets the Sample period to 7 days and the Minute period to 2 years and restarts the app
- **THEN** the Settings screen shows 7 days and 2 years

#### Scenario: Unknown value
- **WHEN** the stored Sample period is `"3d"` and the stored language is `"it"`
- **THEN** the Sample period is 48 hours and the language is Italian

### Requirement: Prune
At its start and then once per hour, the app SHALL delete each Sample that is older than the Sample period. It SHALL also delete each Minute row that is older than the Minute period.

#### Scenario: Sample older than the period
- **WHEN** the Sample period is 24 hours and a Sample is 25 hours old at the prune
- **THEN** the prune deletes the Sample and its Minute row stays

#### Scenario: Sample inside the period
- **WHEN** the Sample period is 7 days and a Sample is 6 days old at the prune
- **THEN** the Sample stays

#### Scenario: Minute row older than the period
- **WHEN** the Minute period is 90 days and a Minute row is 91 days old at the prune
- **THEN** the prune deletes the Minute row

#### Scenario: Minute period of 2 years
- **WHEN** the Minute period is 2 years and a Minute row is 731 days old at the prune
- **THEN** the prune deletes the Minute row

### Requirement: Change of a period
A change to a shorter period that deletes one row or more SHALL show a confirmation that names the number of rows. After the confirmation, the app SHALL store the setting and SHALL start the delete of these rows within 2 s. Without the confirmation, the setting SHALL keep its old value. A change that deletes no row SHALL apply with no question.

#### Scenario: Shorter Sample period
- **WHEN** the user changes the Sample period from 48 hours to 24 hours and the store holds 5 184 000 Samples older than 24 hours
- **THEN** the dialog reads "This deletes 5 184 000 samples older than 24 hours." and has the buttons "Cancel" and "Delete samples"

#### Scenario: Confirmed
- **WHEN** the user presses "Delete samples"
- **THEN** the setting is 24 hours, the delete starts within 2 s, and after it the store holds no Sample older than 24 hours

#### Scenario: Cancelled
- **WHEN** the user presses "Cancel"
- **THEN** the setting is still 48 hours and no row is deleted

#### Scenario: Longer period
- **WHEN** the user changes the Sample period from 48 hours to 7 days
- **THEN** the setting applies with no question, and the store holds the Samples of the last 48 hours as before

### Requirement: Size on disk
The Settings screen SHALL show the size of the store on disk. The size SHALL be the sum of `history.db`, `history.db-wal` and `history.db-shm`. The app SHALL read it when the Settings screen opens and after each history operation.

#### Scenario: Size row
- **WHEN** the three files are 230 MB, 8 MB and 32 KB
- **THEN** the row "Size on disk" reads "238 MB"

#### Scenario: After a delete
- **WHEN** a delete of the whole history and its compaction end while the Settings screen is open
- **THEN** the row shows the new size with no action of the user

### Requirement: Size text
A size text SHALL use the base 1000. It SHALL use MB below 1 000 MB and GB from there. It SHALL have one decimal below 10 and no decimal from 10 on, in the number form of the language of the app.

#### Scenario: Megabytes
- **WHEN** a size is 238 464 000 bytes
- **THEN** the text is "238 MB"

#### Scenario: Gigabytes
- **WHEN** a size is 1 387 584 000 bytes
- **THEN** the text is "1.4 GB"

#### Scenario: Italian
- **WHEN** the language of the app is Italian and a size is 5 356 862 bytes
- **THEN** the text is "5,4 MB"

### Requirement: Estimate of a period
The Settings screen SHALL show an estimate next to each period. The estimate of the Samples SHALL be the number of recorded Series times the seconds of the Sample period times 23 bytes. The estimate of the Minute rows SHALL be the number of recorded Series times the minutes of the Minute period times 44 bytes. An estimate above 2 GB SHALL have the Warning icon. The app SHALL NOT refuse a choice because of its size.

#### Scenario: Default periods
- **WHEN** 60 Series are recorded with 48 hours and 1 year
- **THEN** the estimates read "about 238 MB" and "about 1.4 GB" with no Warning icon

#### Scenario: Large choice
- **WHEN** 60 Series are recorded and the user picks the Sample period 30 days
- **THEN** the estimate reads "about 3.6 GB" with the Warning icon, and the choice applies

#### Scenario: No snapshot
- **WHEN** the app started with the daemon offline
- **THEN** both estimates read `--`

### Requirement: Stored Samples of a range
The app SHALL decide from the stored rows if the store holds the Samples of a range for a set of Series. The Samples of a Series are gone in two cases. In the first case, the first Minute row of the range ends before the first Sample of the range. In the second case, the range holds Minute rows of the Series and no Sample. The store holds the Samples of the range when no Series of the set is in one of these cases.

#### Scenario: Period made longer one day ago
- **WHEN** the Sample period changed from 48 hours to 7 days one day ago, and the range is the 24 hours that end 4 days ago
- **THEN** the store does not hold the Samples of the range

#### Scenario: First hour after the install
- **WHEN** the first Sample of the store is 30 minutes old and the range is the last hour
- **THEN** the store holds the Samples of the range

#### Scenario: One Series with Minute rows only
- **WHEN** a range holds Samples of Series A, and Minute rows and no Sample of Series B
- **THEN** the store does not hold the Samples of the range for the set of A and B

### Requirement: Samples or Minute rows in a chart
A window SHALL draw the Samples when it is 48 hours long or shorter and the store holds its Samples for every Series of the chart, hidden or not. Every other window SHALL draw the Minute rows. All Series of one window SHALL use the same kind.

#### Scenario: Window inside a long Sample period
- **WHEN** the Sample period has been 7 days for 10 days, and the window is the 24 hours that end 3 days ago
- **THEN** the chart draws the Samples

#### Scenario: Window longer than 48 hours
- **WHEN** the Sample period is 7 days and the window is 7 days long
- **THEN** the chart draws the Minute rows

#### Scenario: Window whose Samples are gone
- **WHEN** the Sample period changed from 48 hours to 7 days one day ago, and the window is the 24 hours that end 4 days ago
- **THEN** the chart draws the Minute rows

#### Scenario: Hidden Series
- **WHEN** the store holds the Samples of the window for three Series and not for a fourth, and the user hides the fourth in the legend
- **THEN** the chart still draws the Minute rows

### Requirement: Export scope
The History screen SHALL export the shown range and the shown Series to one CSV file. The shown Series are the selected Series that the legend does not hide, in the order of the legend. The delete scope "Shown series" SHALL use the same Series. The range SHALL end at the present time when the window ends later. The export SHALL be off, with the reason, when no Series is shown.

#### Scenario: Hidden Series
- **WHEN** four sensors are selected and the legend hides one
- **THEN** the file has the time column and three value columns

#### Scenario: Zoomed window
- **WHEN** the window is zoomed to 14:00:00 until 14:10:00 of one day
- **THEN** the file holds only rows with a time from 14:00:00 to 14:10:00

#### Scenario: No Series
- **WHEN** no sensor is selected
- **THEN** "Export to CSV" is off with the text "Select sensors to export."

### Requirement: Export data kind
The export SHALL write the Samples when the store holds the Samples of the range for the shown Series. It SHALL write the Minute rows for every other range. One file SHALL hold one kind. When the store holds the Samples, the dialog SHALL also offer the Minute rows. The option "Minimum and maximum" SHALL exist for the Minute rows only and SHALL be off by default.

#### Scenario: Range inside the Sample period
- **WHEN** the Sample period is 48 hours and the range is the last 24 hours
- **THEN** the dialog shows "1 s samples" as the selected choice, and the file holds one row per second with data

#### Scenario: Older range
- **WHEN** the Sample period is 48 hours and the range is the last 7 days
- **THEN** the dialog shows "1 minute rows" with no other choice, and the file holds the mean of each minute

#### Scenario: Minute rows by choice
- **WHEN** the Sample period is 30 days, the range is the last 7 days and the user picks "1 minute rows"
- **THEN** the file holds 10 081 rows or fewer

### Requirement: CSV file format
The file SHALL be UTF-8 text with no byte order mark. Each row SHALL end with one line feed. The separator SHALL be the comma. A field that holds a comma, a double quote or a line break SHALL be in double quotes, with each double quote written twice. The file SHALL have one header row and then the data rows, the oldest first. The file SHALL NOT depend on the language of the app or of the system.

#### Scenario: Plain file
- **WHEN** the user exports two Series of 10 seconds
- **THEN** the file has 11 lines, each line has 2 commas, and the first 3 bytes are not a byte order mark

#### Scenario: Name with a comma
- **WHEN** a sensor has the name `Radiator, top`
- **THEN** its header field is `"Radiator, top (°C)"`

#### Scenario: German system
- **WHEN** the system language is German and a value is 31.4
- **THEN** the field is `31.4`

### Requirement: CSV header
The first header field SHALL be `time`. The field of a Series SHALL be its name, a space and its unit text in round brackets, for example `Water temperature (°C)`. A Series without a unit SHALL have its name only. When two shown Series have the same name and unit, each of them SHALL have its id in square brackets after the name.

#### Scenario: Two Series
- **WHEN** the shown Series are "Water temperature" in °C and "Waterpump" in %
- **THEN** the header row is `time,Water temperature (°C),Waterpump (%)`

#### Scenario: Same name twice
- **WHEN** two sensors with the ids `nct6798_temp1` and `nct6798_temp2` both have the name "Temperature" and the unit °C
- **THEN** the header fields are `Temperature [nct6798_temp1] (°C)` and `Temperature [nct6798_temp2] (°C)`

### Requirement: CSV time column
The time field SHALL be the local time of the system at the time of the row. Its form SHALL be `YYYY-MM-DDTHH:MM:SS+HH:MM` or `YYYY-MM-DDTHH:MM:SS-HH:MM`, with the UTC offset at that time. The time of a Minute row SHALL be the start of its minute.

#### Scenario: Winter time in Rome
- **WHEN** the system time zone is Europe/Rome and a Sample has the Unix time 1795623185
- **THEN** its time field is `2026-11-25T17:13:05+01:00`

#### Scenario: Hour that occurs twice
- **WHEN** the time zone is Europe/Rome and two Samples have the Unix times 1792888200 and 1792891800
- **THEN** their time fields are `2026-10-25T02:30:00+02:00` and `2026-10-25T02:30:00+01:00`

#### Scenario: Minute row
- **WHEN** a Minute row is for 17:13:00 to 17:13:59
- **THEN** its time field ends with `T17:13:00+01:00`

### Requirement: CSV rows and values
The file SHALL have one row for each time stamp at which one exported Series or more has a value. A Series with no value at that time SHALL have an empty field. A number SHALL use the point as the decimal separator. It SHALL have 3 decimals at most, no trailing zero, no digit grouping and no exponent. A whole number SHALL have no decimal point.

#### Scenario: One Series has a gap
- **WHEN** Series A has values at 17:13:05 and 17:13:06 and Series B only at 17:13:05
- **THEN** the rows are `...T17:13:05+01:00,31.4,52` and `...T17:13:06+01:00,31.5,`

#### Scenario: No value in any Series
- **WHEN** no exported Series has a value from 17:20:00 to 17:25:00
- **THEN** the file has no row for that time

#### Scenario: Rounding
- **WHEN** the mean of a minute is 31.416666
- **THEN** the field is `31.417`

#### Scenario: Whole number
- **WHEN** a speed Sample is 2840.0
- **THEN** the field is `2840`

### Requirement: Minimum and maximum columns
With the option "Minimum and maximum", each Series SHALL have three columns. The first SHALL be the mean with the header of the Series. The second SHALL be the minimum, with ` min` after the name. The third SHALL be the maximum, with ` max` after the name.

#### Scenario: One Series with the option
- **WHEN** the user exports "Water temperature" in °C as Minute rows with the option on
- **THEN** the header row is `time,Water temperature (°C),Water temperature min (°C),Water temperature max (°C)`

#### Scenario: Same name twice with the option
- **WHEN** two Series have the name "Temperature" and the unit °C, and the option is on
- **THEN** the minimum header of the first is `Temperature [nct6798_temp1] min (°C)`

#### Scenario: Values of a minute
- **WHEN** the minute of 17:13 has the mean 31.4, the minimum 30.9 and the maximum 31.8
- **THEN** the row is `2026-11-25T17:13:00+01:00,31.4,30.9,31.8`

### Requirement: Export dialog
Before the file choice, the app SHALL show the range, the number of Series, the data kind, the highest possible number of rows and an estimate of the file size. The estimate SHALL be the rows times the sum of 26 bytes and 9 bytes for each value column. An estimate above 100 MB SHALL have the Warning icon. The app SHALL NOT refuse an export because of its size.

#### Scenario: One day of four Series
- **WHEN** the range is 24 hours, four Series are shown and the kind is Samples
- **THEN** the dialog reads "Up to 86 401 rows, about 5.4 MB"

#### Scenario: Large export
- **WHEN** the Sample period is 30 days, the range is 30 days and 60 Series are shown as Samples
- **THEN** the dialog reads "Up to 2 592 001 rows, about 1.5 GB" with the Warning icon, and "Export" stays on

### Requirement: Export file
The app SHALL ask for the file with the file dialog of the system. It SHALL propose the name `nicecooling-history-<start>-<end>.csv`, with both times as `YYYYMMDDTHHMMSS` in local time. It SHALL write a temporary file in the same directory and give it the chosen name when the export is complete. It SHALL show the progress and a "Cancel" button. The app SHALL keep recording during an export.

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
- **THEN** the store gets the Samples of these 30 s

### Requirement: Export failure
When the app cannot write the file, it SHALL remove the temporary file, show the error of the system with the path, and leave the store unchanged. A file that existed under the chosen name SHALL keep its old content.

#### Scenario: Directory without write access
- **WHEN** the user picks a file in a directory that the user cannot write
- **THEN** the app shows "The file cannot be written" with the path and the error text, and no file is created

#### Scenario: Disk full during the export
- **WHEN** the disk is full after 10 000 rows
- **THEN** the temporary file is removed and a file that existed under the chosen name is unchanged

### Requirement: Delete of a range
The History screen SHALL delete the rows of the shown range for the shown Series or for all Series, as the user picks. It SHALL delete the Samples with a time in the range and each Minute row whose minute overlaps the range. It SHALL also drop the Samples of the range that are not yet in the store. The app SHALL keep recording. A delete SHALL NOT delete a Note or a View.

#### Scenario: Shown Series
- **WHEN** the range is 14:00:00 to 14:10:00, the scope is the three shown Series and the user confirms
- **THEN** the store holds no Sample of these three Series from 14:00:00 to 14:10:00, and the rows of the other Series stay

#### Scenario: Minute at the edge
- **WHEN** the range is 14:00:30 to 14:10:30
- **THEN** the Minute rows of 14:00 to 14:10 of the scope are deleted, and the Minute rows of 13:59 and 14:11 stay

#### Scenario: Range that ends now
- **WHEN** the range ends at the present time and the user confirms at 15:00:00
- **THEN** no Sample of the scope with a time up to 15:00:00 appears in the store later, and the store gets the Samples after 15:00:00. The Minute row of 15:00 then holds only the Samples after 15:00:00

#### Scenario: All Series with no sensor selected
- **WHEN** no sensor is selected
- **THEN** the scope "Shown series" is off with the reason, and the scope "All series" works

### Requirement: Confirmation of a range delete
Before a range delete, the app SHALL show a confirmation that names the exact number of Samples, the exact number of Minute rows, the number of Series and the range. The dialog SHALL say that the delete cannot be undone. The button SHALL be the destructive button "Delete". It SHALL be off while the app counts and when the count is 0.

#### Scenario: Count
- **WHEN** the range is 14:00:00 to 14:09:59 and the scope holds 1 800 Samples and 30 Minute rows of 3 Series in it
- **THEN** the dialog reads "Delete 1 800 samples and 30 minute rows of 3 series from 14:00:00 to 14:09:59? You cannot undo this."

#### Scenario: Scope change
- **WHEN** the user changes the scope from "Shown series" to "All series"
- **THEN** the numbers change to the count of all Series before "Delete" is on again

#### Scenario: Nothing to delete
- **WHEN** the scope holds no row in the range
- **THEN** the dialog reads "No rows in this range." and "Delete" is off

#### Scenario: Cancel
- **WHEN** the user presses "Cancel"
- **THEN** no row is deleted

### Requirement: Delete of the whole history
The Settings screen SHALL delete every Sample and every Minute row of the store. The confirmation SHALL name the exact number of Samples, of Minute rows and of Series with rows, and SHALL say that the delete cannot be undone. It SHALL have the switch "Also delete the <n> Notes", with the number of Notes, off by default. The Views SHALL stay. The app SHALL keep recording.

#### Scenario: Confirmation text
- **WHEN** the store holds 10 368 000 Samples and 525 600 Minute rows of 62 Series, and 12 Notes
- **THEN** the dialog reads "Delete 10 368 000 samples and 525 600 minute rows of 62 series? You cannot undo this." and the switch reads "Also delete the 12 Notes"

#### Scenario: Confirmed with the switch off
- **WHEN** the user presses "Delete history"
- **THEN** the store holds no Sample and no Minute row from before that time, the 12 Notes and the Views stay, and the store gets the next Samples

#### Scenario: Switch on
- **WHEN** the user turns the switch on and presses "Delete history"
- **THEN** the store holds no Note

### Requirement: Compaction
The app SHALL compact the database file after one confirmed operation that removes 1 000 000 rows or more. Such an operation is a range delete, a delete of the whole history or a change to a shorter period. The file SHALL then give the free space back to the disk. A smaller delete SHALL leave the file size as it is. The app SHALL NOT compact the file after a prune at its start or after an hourly prune.

#### Scenario: Whole history
- **WHEN** the user deletes a history of 10 368 000 Samples in a file of 238 MB
- **THEN** after the compaction the database files are smaller than 1 MB in sum

#### Scenario: Small range delete
- **WHEN** a range delete removes 1 800 Samples from a file that had no free pages
- **THEN** the app does not compact the file

#### Scenario: Hourly prune
- **WHEN** the hourly prune removes 216 000 Samples of 60 Series
- **THEN** the app does not compact the file

#### Scenario: Prune at the start after a weekend
- **WHEN** the app starts after 60 hours and the prune at the start removes 10 368 000 Samples
- **THEN** the app does not compact the file

### Requirement: Compaction that does not run or fails
Before a compaction, the app SHALL read the free disk space. With less than twice the size of the pages in use, which hold the rows that stay, it SHALL NOT compact. When a compaction does not run or fails, the app SHALL show "The file was not compacted" with the reason, and the deleted rows SHALL stay deleted.

#### Scenario: No space for the compaction
- **WHEN** the pages in use are 2 GB after a delete and the disk has 3 GB of free space
- **THEN** the app does not compact, shows "The file was not compacted" with the reason, and the rows stay deleted

#### Scenario: Compaction fails
- **WHEN** the compaction gives an error of the database
- **THEN** the app shows "The file was not compacted" with the error text, and the rows stay deleted

### Requirement: Samples during a long operation
While a delete, a prune or a compaction runs, the Samples that the daemon sends SHALL wait in the app. When the operation ends, the app SHALL store them with their own times, and their Minute rows SHALL hold them. The app holds 1024 daemon events. When more than 1024 events arrive during one operation, the app SHALL lose only the oldest of them.

#### Scenario: Compaction of 20 s
- **WHEN** a compaction runs from 15:00:00 to 15:00:20 and the daemon sends one Sample per second for each sensor
- **THEN** after the compaction the store holds the Samples of 15:00:00 to 15:00:20, and the Minute row of 15:00 holds 60 Samples

#### Scenario: More than 1024 events during one operation
- **WHEN** a delete runs for 30 minutes and the daemon sends one event per second
- **THEN** the store holds the Samples of the last 1024 s of the delete with their Minute rows, and the chart shows a gap before them

### Requirement: One history operation at a time
While an export, a delete, a prune after a change of a period or a compaction runs, the app SHALL start no second one of these. The commands that start one SHALL be off with the text "A history operation is running." The commands that add, change or delete a Note SHALL be off with the same text. The chart SHALL still read the store and draw the Notes, and Views SHALL still work.

#### Scenario: Delete during an export
- **WHEN** an export runs and the user opens the menu "Chart"
- **THEN** "Delete data" and "Export to CSV" are off with the text "A history operation is running."

#### Scenario: Chart during a compaction
- **WHEN** a compaction runs and the user changes the range length to 7 days
- **THEN** the chart draws the 7 days

#### Scenario: Note during a delete
- **WHEN** a range delete runs and the user makes a secondary click in the chart
- **THEN** "Add note here" is off with the text "A history operation is running."

### Requirement: Schema version of the database
The service thread, which records the Samples, SHALL bring a database of schema version 1 to version 2 when it opens the file, and SHALL keep every row. A read of a version 1 database SHALL NOT change the file. The app SHALL NOT write a database with a schema version above 2. It SHALL then record nothing and draw no chart from that file. The History screen SHALL then show the database error state with the text "The history database is from a newer version of nicecooling." and the path.

#### Scenario: Version 1 database
- **WHEN** the app opens a version 1 database with 1 000 Samples
- **THEN** the schema version is 2, the 1 000 Samples are in the store and the Notes list is empty

#### Scenario: Chart read before the service thread opens the file
- **WHEN** the History screen reads a version 1 database before the service thread opens it
- **THEN** the chart draws the stored rows, shows no Note, and the file does not change

#### Scenario: Newer database
- **WHEN** the app opens a database with the schema version 3
- **THEN** the file does not change, the app records no Sample, and the History screen shows the text with the path

### Requirement: No daemon and no password
Recording switch, period settings, export, delete and compaction SHALL work in the app alone. They SHALL make no call to the daemon, SHALL ask no password and SHALL work while the daemon is offline.

#### Scenario: Daemon offline
- **WHEN** the daemon is offline and the user exports the last 24 hours
- **THEN** the export writes the file

#### Scenario: Delete without a password
- **WHEN** a user outside the group `wheel` deletes a range
- **THEN** the delete runs with no password dialog
