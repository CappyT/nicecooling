# Dashboards

## Purpose

Defines the Dashboards of the desktop app: pages that the user builds from Tiles on a grid of 12 columns, next to the fixed Overview. It covers the Tile kinds, the Tile limits, what a Tile shows without a value, the Edit mode, the generator from a Loop, the navigation and the storage in the settings directory of the user.

## ADDED Requirements

### Requirement: Dashboard model
A Dashboard SHALL have a name of 1 to 40 characters and an ordered list of Tiles. The app SHALL keep the list in reading order: by row, then by column. This order SHALL be the order of the keyboard selection and of the accessible text.

#### Scenario: Reading order
- **WHEN** a Dashboard holds a Tile at column 6, row 0, a Tile at column 0, row 1 and a Tile at column 0, row 0
- **THEN** the list holds them in this order: column 0 row 0, column 6 row 0, column 0 row 1

#### Scenario: Name of 41 characters
- **WHEN** the user types a name of 41 characters in the rename input
- **THEN** the input keeps the first 40 characters

#### Scenario: Empty name
- **WHEN** the user clears the name of the Dashboard "Main loop" and presses Enter
- **THEN** the Dashboard keeps the name "Main loop"

### Requirement: Count limits
The app SHALL hold at most 16 Dashboards for one user. A Dashboard SHALL hold at most 48 Tiles, of which at most 12 are chart Tiles. At a count limit, the command that adds SHALL be off, and its tooltip SHALL give the reason. A change of the kind of a Tile SHALL obey the same count limits. No command SHALL store a file that the valid check of the loader refuses.

#### Scenario: 17th Dashboard
- **WHEN** 16 Dashboards exist
- **THEN** "New Dashboard" is off and its tooltip says "A user can have 16 Dashboards."

#### Scenario: 49th Tile
- **WHEN** a Dashboard holds 48 Tiles
- **THEN** "Add Tile" is off and its tooltip says "A Dashboard can have 48 Tiles."

#### Scenario: 13th chart Tile
- **WHEN** a Dashboard holds 12 chart Tiles and the user opens "Add Tile"
- **THEN** the row "Chart" is off and its tooltip says "A Dashboard can have 12 chart Tiles."

#### Scenario: Kind change at the chart limit
- **WHEN** a Dashboard holds 12 chart Tiles and the user opens the Tile settings of a value Tile
- **THEN** the kind "Chart" is off in the select "Kind" and its tooltip says "A Dashboard can have 12 chart Tiles."

#### Scenario: Every command keeps the file valid
- **WHEN** the user adds, moves, resizes and removes Tiles, changes their settings, and adds, renames, moves and deletes Dashboards
- **THEN** the valid check of the loader accepts the Dashboards after each command

### Requirement: Grid
A Dashboard SHALL have a grid of 12 columns and at most 64 rows. A Tile SHALL have a column of 0 to 11, a row of 0 to 63, a column span of 1 to 12 and a row span of 1 to 8. The column plus the column span SHALL be 12 or less. The row plus the row span SHALL be 64 or less. No two Tiles of a Dashboard SHALL cover the same cell. A Dashboard SHALL have no layers and no pixel positions.

#### Scenario: Right edge
- **WHEN** a Tile is at column 10 with a column span of 2 and the user makes it wider by one column
- **THEN** the Tile keeps the column span of 2

#### Scenario: Occupied cell
- **WHEN** a Tile covers the columns 0 to 2 of row 0 and the user moves another Tile to column 2, row 0
- **THEN** the other Tile stays at its place

### Requirement: Grid geometry
The grid SHALL fill the content width less 24 px of padding at each side. The gap between two columns and between two rows SHALL be 12 px. A row SHALL be 60 px high at every window size. The column width SHALL follow the window. The layout SHALL be the same from a window of 1100x700 up. A Dashboard SHALL scroll vertically and never horizontally.

#### Scenario: Minimum window
- **WHEN** the window is 1100 px wide
- **THEN** the grid is 832 px wide, a column is 58.3 px wide, and a Tile of 3 columns and 3 rows is 199 px wide and 204 px high

#### Scenario: Wide window
- **WHEN** the window is 1920 px wide
- **THEN** a column is 126.7 px wide, and a Tile of 3 columns and 3 rows is 404 px wide and 204 px high

#### Scenario: Rows at the minimum window
- **WHEN** the window is 1100x700, no banner shows and the Dashboard is not in Edit mode
- **THEN** 8 rows show without a scroll

### Requirement: Tile kinds and sizes
A Tile SHALL have one kind: value, gauge, bar, chart or text. Each kind SHALL have a minimum size and a default size, in columns by rows: value 2x1 and 3x1, gauge 2x2 and 3x3, bar 2x1 and 4x1, chart 3x2 and 6x3, text 1x1 and 4x1. No command SHALL make a Tile smaller than the minimum size of its kind.

#### Scenario: Default size
- **WHEN** the user adds a chart Tile
- **THEN** the Tile has 6 columns and 3 rows

#### Scenario: Minimum size
- **WHEN** a gauge Tile has 2 columns and 2 rows and the user presses Shift+Left
- **THEN** the Tile keeps 2 columns

#### Scenario: Kind change to a larger minimum
- **WHEN** a value Tile of 2x1 at column 0, row 0 becomes a chart Tile, and the cells of columns 0 to 2 of rows 0 and 1 are free or its own
- **THEN** the Tile has 3 columns and 2 rows at column 0, row 0

#### Scenario: Kind change with no room
- **WHEN** a value Tile of 2x1 becomes a chart Tile and another Tile covers the cell below it
- **THEN** the Tile moves to the first free place for 3 columns and 2 rows

#### Scenario: Kind change with no free place
- **WHEN** a value Tile becomes a chart Tile and no place of the 64 rows is free for 3 columns and 2 rows
- **THEN** the Tile settings say "This Dashboard has no free place for this Tile." and "Save" is off

### Requirement: Minimum sizes at the minimum window
At a window of 1100x700, a Tile of each kind at its minimum size SHALL show its value and its unit without a cut. A title that does not fit SHALL truncate with an ellipsis. The value SHALL keep its width.

#### Scenario: Value Tile of 2x1
- **WHEN** a value Tile of 2 columns and 1 row shows "2 840 rpm" in a window of 1100x700, in English, Italian, German and French
- **THEN** the number and the unit are inside the Tile and no part of them is cut

#### Scenario: Gauge Tile of 2x2
- **WHEN** a gauge Tile of 2 columns and 2 rows shows "450 mbar" in a window of 1100x700
- **THEN** the number is inside the ring and the ring is inside the Tile

#### Scenario: Long title
- **WHEN** a value Tile of 2x1 has the title "Temperatura acqua del radiatore superiore"
- **THEN** the title ends with an ellipsis and the value shows in full

### Requirement: Sensor of a Tile
A value, gauge, bar or chart Tile SHALL show one sensor of the sensor list of the daemon, of any origin and any unit. The Tile SHALL store the id of the sensor. A text Tile SHALL have no sensor. A Tile SHALL NOT take the Duty of a Channel as its source.

#### Scenario: Derived sensor
- **WHEN** the user picks a Derived sensor for a value Tile
- **THEN** the Tile shows the value of the Derived sensor

#### Scenario: Calibrated flow
- **WHEN** the user picks the sensor `loop_1_flow` with the value 115.2 and 0 decimals
- **THEN** the Tile shows "115 L/h"

#### Scenario: Sensor of another origin
- **WHEN** the sensor list of the daemon holds a sensor of an origin that this version of the app does not know
- **THEN** the sensor picker lists it and a Tile shows its value

### Requirement: Tile title
A Tile with a sensor SHALL show a title: its own title of 1 to 40 characters, or the name of its sensor when the own title is empty. A title that comes from the sensor SHALL follow a rename of the sensor.

#### Scenario: Sensor name as title
- **WHEN** a Tile has no own title and its sensor has the name "Water temperature"
- **THEN** the Tile shows the title "Water temperature"

#### Scenario: Rename of the sensor
- **WHEN** the user renames that sensor to "Loop water" in the Sensors screen
- **THEN** the Tile shows the title "Loop water"

#### Scenario: Own title
- **WHEN** a Tile has the own title "Radiator out" and the user renames its sensor
- **THEN** the Tile shows the title "Radiator out"

### Requirement: Value format
A Tile SHALL show its value with 0 to 3 decimals, as the Tile sets them, and with the unit text of its sensor. The number SHALL follow the number format of the app: a space groups the thousands, and Italian, German and French use a decimal comma. The default decimals of a new Tile SHALL be 1 for the units `degC`, `V`, `A`, `W` and `mbar/min`, and 0 for every other unit.

#### Scenario: One decimal
- **WHEN** a Tile with 1 decimal shows a sensor in `degC` with the value 31.4
- **THEN** the Tile shows "31.4 °C"

#### Scenario: Two decimals
- **WHEN** the same Tile has 2 decimals
- **THEN** the Tile shows "31.40 °C"

#### Scenario: Thousands
- **WHEN** a Tile with 0 decimals shows a sensor in `rpm` with the value 2840
- **THEN** the Tile shows "2 840 rpm"

#### Scenario: Italian
- **WHEN** the language is Italian and a Tile with 1 decimal shows 31.4 in `degC`
- **THEN** the Tile shows "31,4 °C"

### Requirement: Value Tile
A value Tile SHALL show its title, the value of its sensor and the unit. The value and the unit SHALL be separate text runs. A value Tile with 2 rows or more SHALL show the number in the numeral text style. A value Tile with 1 row SHALL show it in the value text style.

#### Scenario: One row
- **WHEN** a value Tile of 3x1 shows the sensor "Flow" with 115.2 L/h and 0 decimals
- **THEN** it shows the title "Flow" and "115" in the value text style with the unit "L/h"

#### Scenario: Two rows
- **WHEN** the user makes the Tile 2 rows high
- **THEN** the number shows in the numeral text style

### Requirement: Gauge Tile
A gauge Tile SHALL show a ring arc of 270 degrees with its opening at the bottom, the value and the unit in the ring, and its title. The filled part of the ring SHALL be the fraction (value - minimum) / (maximum - minimum), held between 0 and 1. The number SHALL always be the real value. A gauge Tile with 3 columns and 3 rows or more SHALL show the number in the numeral text style. A smaller gauge Tile SHALL show it in the value text style.

#### Scenario: Small gauge
- **WHEN** a gauge Tile has 2 columns and 2 rows
- **THEN** the number shows in the value text style

#### Scenario: Value in the range
- **WHEN** a gauge Tile with the minimum 0 and the maximum 450 shows 298 mbar
- **THEN** the ring is filled to 66% of its length and the number is "298"

#### Scenario: Value above the maximum
- **WHEN** the same Tile shows 500 mbar
- **THEN** the ring is filled to 100% and the number is "500"

#### Scenario: Value below the minimum
- **WHEN** a gauge Tile with the minimum 20 and the maximum 60 shows 18.5 °C
- **THEN** the ring has no filled part and the number is "18.5"

### Requirement: Bar Tile
A bar Tile SHALL show a horizontal bar on a track, its title, the value and the unit. The filled part of the track SHALL be the same fraction as for the gauge Tile, held between 0 and 1.

#### Scenario: Fill level
- **WHEN** a bar Tile with the minimum 0 and the maximum 100 shows a fill level of 68 %
- **THEN** the bar covers 68% of the track and the Tile shows "68 %"

#### Scenario: Value above the maximum
- **WHEN** a bar Tile with the minimum 0 and the maximum 3000 shows 3200 rpm
- **THEN** the bar covers the whole track and the Tile shows "3 200 rpm"

### Requirement: Range of a gauge and a bar
A gauge Tile and a bar Tile SHALL have a minimum and a maximum. The maximum SHALL be above the minimum. A new Tile SHALL take its range from the unit of its sensor: 0 to 100 for `%` and `degC`, 0 to 3000 for `rpm`, 0 to 300 for `l/h`, 0 to 450 for `mbar`, and 0 to 100 for every other unit. In the Tile settings of a new Tile, each pick of a sensor SHALL set this range, until the user edits the minimum or the maximum. In the Tile settings of a stored Tile, a pick of a sensor SHALL keep the range.

#### Scenario: Default range of a speed
- **WHEN** the user adds a gauge Tile and picks a sensor in `rpm`
- **THEN** the minimum is 0 and the maximum is 3000

#### Scenario: Second pick in a new Tile
- **WHEN** the user adds a gauge Tile, picks a sensor in `rpm` and then picks a sensor in `l/h`, with no edit of the range
- **THEN** the minimum is 0 and the maximum is 300

#### Scenario: Maximum below the minimum
- **WHEN** the user sets the minimum 30 and the maximum 20 in the Tile settings
- **THEN** "Save" is off and the dialog says "The maximum must be above the minimum."

#### Scenario: Sensor change keeps the range
- **WHEN** a gauge Tile has the range 20 to 60 and the user picks another sensor
- **THEN** the range stays 20 to 60

### Requirement: Chart Tile
A chart Tile SHALL show a line of the values of its sensor over the last N minutes, its title and the newest live value. N SHALL be a whole number of 1 to 1440. The Tile settings SHALL offer 5 minutes, 15 minutes, 1 hour, 6 hours and 24 hours. A new chart Tile SHALL have 60 minutes. The x axis SHALL always cover N minutes and SHALL end at the present time. A chart Tile with 4 columns and 3 rows or more SHALL show the value labels of its axis. A smaller chart Tile SHALL show no axis label.

#### Scenario: Time span choices
- **WHEN** the user opens the select "Time span"
- **THEN** it lists "5 min", "15 min", "1 h", "6 h" and "24 h"

#### Scenario: Short history
- **WHEN** a chart Tile has 60 minutes and the history store holds only the last 10 minutes of the sensor
- **THEN** the line covers the last 10 minutes, and the 50 minutes before them are empty

#### Scenario: No data
- **WHEN** the history store holds no sample of the sensor in the time span
- **THEN** the plot shows "No data in this time span."

#### Scenario: Small chart
- **WHEN** a chart Tile has 3 columns and 2 rows
- **THEN** the plot shows the line and the limit lines and no axis label

### Requirement: Chart data from the history store
A chart Tile SHALL read its line from the history store of the app. It SHALL show the stored values when the Dashboard opens, and it SHALL NOT wait for new samples. A time with no stored value SHALL be a break in the line. A read that fails SHALL keep the line of the last good read. A chart Tile with no good read SHALL show "The history cannot be read." after a read that fails.

#### Scenario: Data at once
- **WHEN** the history store holds 60 minutes of a sensor and the user opens a Dashboard with a chart Tile of 60 minutes on it
- **THEN** the line covers 60 minutes after the first read, within 1 s

#### Scenario: Gap
- **WHEN** the app did not run from 21:10 to 21:15 and a chart Tile shows 21:00 to 22:00
- **THEN** the line has a break from 21:10 to 21:15

#### Scenario: History file with an error
- **WHEN** the history store cannot be read at the first read of a chart Tile
- **THEN** the chart Tile shows "The history cannot be read." and the other Tiles show their values

#### Scenario: One failed read
- **WHEN** a chart Tile shows its line and the next read of the history store fails
- **THEN** the chart Tile still shows the line of the last read

### Requirement: Chart read rate
A chart Tile SHALL read the history store outside the drawing of the window. It SHALL read again at an interval of N / 4 seconds, rounded down to a whole second, where N is its time span in minutes. The interval SHALL not be below 10 s and not above 60 s. Only the Dashboard that the window shows SHALL read. The screen SHALL run one read task at a time.

#### Scenario: Interval for 5 minutes
- **WHEN** a chart Tile has 5 minutes
- **THEN** it reads again every 10 s

#### Scenario: Interval for 1 hour
- **WHEN** a chart Tile has 60 minutes
- **THEN** it reads again every 15 s

#### Scenario: Interval for 24 hours
- **WHEN** a chart Tile has 1440 minutes
- **THEN** it reads again every 60 s

#### Scenario: Dashboard behind another tab
- **WHEN** the Dashboard "GPU" has a chart Tile and the window shows the Dashboard "Main loop"
- **THEN** the app makes no read for the chart Tile of "GPU"

#### Scenario: Two Tiles on one sensor
- **WHEN** two chart Tiles show the same sensor with the same time span
- **THEN** the app makes one read per interval for both

### Requirement: Points of a chart line
The line of a chart Tile SHALL have one point per time bucket that holds a value: the mean of the values of the bucket. A bucket SHALL be N / 10 seconds wide, rounded up to a whole second, and SHALL start at a multiple of its width in Unix time. So a line has 601 points at most.

#### Scenario: Many samples
- **WHEN** the history store holds 86 400 samples of the last 24 hours with no gap
- **THEN** the line has 601 points or fewer and no break

#### Scenario: Many breaks
- **WHEN** the sensor of a chart Tile of 24 hours lost its value for 5 s once per minute
- **THEN** the line has 601 points or fewer

#### Scenario: Bucket width
- **WHEN** a chart Tile has 60 minutes
- **THEN** a bucket is 6 s wide, and a sample of 21:00:07 is in the bucket that starts at 21:00:06

### Requirement: Breaks of a chart line
A break of the line SHALL be where the stored rows have a gap that is wider than one bucket. The stored rows have a gap where two 1 s samples are more than 10 s apart, where two 1 minute rows are more than 150 s apart, and at a row with no value.

#### Scenario: Dropout in a short chart
- **WHEN** the sensor of a chart Tile of 5 minutes had no value for 5 s
- **THEN** the line has a break of these 5 s

#### Scenario: Dropout in a long chart
- **WHEN** the sensor of a chart Tile of 24 hours had no value for 5 s
- **THEN** the line has no break there, because a bucket is 144 s wide

#### Scenario: Minute rows
- **WHEN** the history store gives 1 minute rows for a chart Tile of 6 hours, with a bucket of 36 s
- **THEN** the rows that are 60 s apart are one unbroken line

### Requirement: Text Tile
A text Tile SHALL show a text of 1 to 200 characters in one of two styles: heading or note. A heading SHALL be one line that truncates with an ellipsis. A note SHALL wrap, and a note that does not fit SHALL end with an ellipsis. A heading SHALL have no card ground. A text Tile SHALL have no sensor, no value and no Tile limit.

#### Scenario: Heading
- **WHEN** a text Tile of 12x1 has the style heading and the text "Radiators"
- **THEN** it shows "Radiators" on one line in bold

#### Scenario: Note
- **WHEN** a text Tile of 4x2 has the style note and a text of 180 characters
- **THEN** the text wraps inside the Tile

### Requirement: Tile limits
A value, gauge, bar or chart Tile SHALL have two optional Tile limits: a warning limit and an alarm limit. Each SHALL have a direction, "above" or "below", and a value. A value is over an "above" limit when it is greater than the limit value, and over a "below" limit when it is less. A value equal to the limit value is not over it. When a value is over both limits, the alarm limit SHALL win.

#### Scenario: Over the warning limit
- **WHEN** a Tile has the warning limit above 35 and the alarm limit above 40, and its value is 36.2
- **THEN** the Tile is in the warning state

#### Scenario: Over both limits
- **WHEN** the value of that Tile is 41
- **THEN** the Tile is in the alarm state

#### Scenario: Equal to the limit
- **WHEN** the value of that Tile is 35.0
- **THEN** the Tile is in no limit state

#### Scenario: Below limit
- **WHEN** a Tile has the alarm limit below 250 and its value is 142
- **THEN** the Tile is in the alarm state

#### Scenario: No limit set
- **WHEN** a Tile has no Tile limit and its value is 95 °C
- **THEN** the Tile is in no limit state

### Requirement: Look of a limit state
A Tile in the warning state SHALL have the Warning tint of the theme as its ground and the Warning icon before its title. A Tile in the alarm state SHALL have the Alarm tint and the Alarm icon. The number, the filled part of a ring and the bar SHALL take the Warning or the Alarm color. The line of a chart SHALL keep its series color. The title, the unit and the caption SHALL have the normal text color, not the dim one. A Tile SHALL NOT use the solid Alarm field.

#### Scenario: Warning state
- **WHEN** a value Tile is in the warning state
- **THEN** its ground is the Warning tint, the triangle icon shows before the title, the number has the Warning text color, and the unit has the normal text color

#### Scenario: Alarm state of a gauge
- **WHEN** a gauge Tile is in the alarm state
- **THEN** its ground is the Alarm tint, the octagon icon shows before the title, and the filled part of the ring has the Alarm color

#### Scenario: Dark theme
- **WHEN** the system theme is dark and a Tile is in the alarm state
- **THEN** the Tile uses the dark values of the Alarm tokens

### Requirement: Text of a limit state
The state of a Tile limit SHALL never be a color alone. A Tile in a limit state SHALL give the limit as a text, with the decimals of the Tile and the unit: in its tooltip and in its accessible text always, and as a caption on the Tile when the Tile has 2 rows or more.

#### Scenario: Caption on a tall Tile
- **WHEN** a value Tile of 3x2 with 1 decimal is over its warning limit above 35 on a sensor in `degC`
- **THEN** the Tile shows the caption "above 35.0 °C"

#### Scenario: Tile of one row
- **WHEN** a value Tile of 3x1 is over that limit
- **THEN** the Tile shows the icon and no caption, and its tooltip holds "Warning limit: above 35.0 °C"

#### Scenario: Accessible text
- **WHEN** a Tile "Water temperature" shows 36.2 °C and is over that limit
- **THEN** its accessible text is "Water temperature 36.2 °C. Warning limit: above 35.0 °C."

### Requirement: A Tile limit is not an Alarm
A Tile limit SHALL change the look of its Tile only. It SHALL NOT raise an Alarm or a Warning, SHALL NOT run an alarm response, SHALL NOT show a notification, SHALL NOT open the window and SHALL NOT change the tray icon, the status bar, the sidebar or the list of active Alarms. The Tile settings SHALL say this.

#### Scenario: Value over the alarm limit
- **WHEN** no Alarm of the daemon is active and the value of a Tile goes over its alarm limit
- **THEN** the status bar says "No active Alarms", the tray icon is in its normal state and no notification shows

#### Scenario: No call of the daemon
- **WHEN** the value of a Tile goes over its alarm limit
- **THEN** the app makes no call of the daemon

#### Scenario: Text in the settings
- **WHEN** the user opens the Tile settings of a value Tile
- **THEN** the dialog shows "A limit colors this Tile only. It raises no Alarm and runs no alarm response."

### Requirement: Limit lines of a chart
A chart Tile SHALL draw each Tile limit that it has as a dashed horizontal line at the limit value, in the Warning or the Alarm text color, with the value as a label. The value axis SHALL cover the limit values. The state of the Tile SHALL come from the newest live value.

#### Scenario: Two lines
- **WHEN** a chart Tile has the warning limit above 35 and the alarm limit above 40, and its values are between 30 and 33
- **THEN** the plot shows two dashed lines at 35 and at 40, and the value axis reaches 40

#### Scenario: State from the live value
- **WHEN** the line of that Tile passed 36 ten minutes ago and the newest live value is 33
- **THEN** the Tile is in no limit state

### Requirement: Sensor without a value
A Tile whose sensor has no value SHALL show "--" without a unit and the icon of a missing value. A gauge and a bar SHALL have no filled part. The line of a chart SHALL end where the value stopped. The Tile SHALL be in no limit state. The other Tiles SHALL stay live.

#### Scenario: Lost sensor
- **WHEN** the daemon is online and the water temperature sensor has no value
- **THEN** its value Tile shows "--" and the dashed-circle icon, and a Tile on the pump speed shows "2 840 rpm"

#### Scenario: Gauge
- **WHEN** the sensor of a gauge Tile has no value
- **THEN** the ring shows its track only and "--" in its centre

#### Scenario: No limit state
- **WHEN** a Tile has the alarm limit below 250 and its sensor has no value
- **THEN** the Tile is in no limit state

### Requirement: Sensor that no longer exists
When the sensor list that the daemon gave does not hold the sensor of a Tile, the Tile SHALL show "Sensor not found" and the stored sensor id, and no value, ring, bar or line. The Tile SHALL keep the sensor id. When a later sensor list holds the sensor, the Tile SHALL show its value again. The app SHALL decide this only against a sensor list that it read in this run.

#### Scenario: Removed Derived sensor
- **WHEN** the user deletes the Derived sensor `derived_2` and a Tile uses it
- **THEN** the Tile shows "Sensor not found" and `derived_2`

#### Scenario: Sensor returns
- **WHEN** a later sensor list of the daemon holds `derived_2` again
- **THEN** the Tile shows the value of `derived_2`

#### Scenario: Own title stays
- **WHEN** that Tile has the own title "Delta T"
- **THEN** it shows the title "Delta T" over "Sensor not found"

#### Scenario: New sensor in Edit mode
- **WHEN** the user opens the Tile settings of that Tile and picks the sensor `nct6798_temp1`
- **THEN** the Tile shows the value of `nct6798_temp1`

### Requirement: Daemon offline or down
While the daemon is offline or down, a Tile SHALL show the last value that the app got, in the dim text color, with its unit. The Tile SHALL be in no limit state. One line above the grid SHALL give the time of the last values. A chart Tile SHALL still draw the stored history. Before the first answer of the daemon in a run, a Tile SHALL show "--".

#### Scenario: Offline after values
- **WHEN** the daemon goes offline at 21:40:12 and the last water temperature was 31.4 °C
- **THEN** the Tile shows "31.4 °C" in the dim text color, and the line above the grid says "Last values 21:40:12"

#### Scenario: Offline with a limit
- **WHEN** the last value of a Tile was over its alarm limit and the daemon goes offline
- **THEN** the Tile has no Alarm tint and no Alarm icon

#### Scenario: Never online
- **WHEN** the app starts and the daemon never answers
- **THEN** each Tile with a sensor shows "--", and no Tile shows "Sensor not found"

#### Scenario: Chart while offline
- **WHEN** the daemon is offline and the history store holds the last hour
- **THEN** a chart Tile of 60 minutes draws its line up to the last stored sample

### Requirement: Loading
While the app connects to the daemon at its start, the shell SHALL show "Connecting to the daemon" in place of the Dashboards screen, for at most 2 s, as for every screen. Then the offline state applies.

#### Scenario: Start
- **WHEN** the user opens Dashboards 0.5 s after the start of the app and the daemon did not answer yet
- **THEN** the window shows "Connecting to the daemon" and no Tile

#### Scenario: After 2 s
- **WHEN** the daemon does not answer in 2 s
- **THEN** the Tiles show "--" and the offline banner shows

### Requirement: Edit mode without the daemon
Every command that changes a Dashboard SHALL work while the daemon is offline or down. The grid SHALL then keep its resize handle and its drag. The sensor picker SHALL then list the sensors of the last sensor list that the app got.

#### Scenario: Add a Tile offline
- **WHEN** the daemon is offline and the user adds a value Tile on a sensor of the last sensor list
- **THEN** the Tile is added and stored

#### Scenario: Resize offline
- **WHEN** the daemon is offline and Edit mode is on
- **THEN** the selected Tile has its resize handle, and a drag of the handle changes the size

#### Scenario: No sensor list
- **WHEN** the daemon never answered in this run and the user opens the sensor picker
- **THEN** the picker is empty and the user can still add a text Tile

### Requirement: Navigation
The sidebar SHALL have one entry "Dashboards", directly after "Overview". The Dashboards screen SHALL have a tab strip with one tab per Dashboard, in the stored order. Tabs that do not fit SHALL go into a "More" menu. At the start of the app the first Dashboard SHALL be the selected one. While the app runs, the app SHALL keep the selected Dashboard, also while the window is closed.

#### Scenario: Sidebar order
- **WHEN** the app shows its sidebar
- **THEN** the first entries are "Overview", "Dashboards", "Loops", "Channels" and "Sensors"

#### Scenario: Shortcut
- **WHEN** the user presses Alt+2
- **THEN** the Dashboards screen opens

#### Scenario: Three Dashboards
- **WHEN** the user has the Dashboards "Main loop", "GPU" and "Fans"
- **THEN** the tab strip shows the three names in this order and "Main loop" is selected at the start of the app

#### Scenario: Sixteen Dashboards at the minimum window
- **WHEN** the user has 16 Dashboards with names of 12 characters and the window is 1100 px wide
- **THEN** the tabs that do not fit are in the "More" menu, and the strip does not scroll

#### Scenario: Return to the screen
- **WHEN** the user selects "GPU", opens Sensors and opens Dashboards again
- **THEN** "GPU" is selected

#### Scenario: Window closed and opened
- **WHEN** the user selects "GPU", closes the window to the tray and opens it again
- **THEN** the Dashboards screen shows "GPU"

### Requirement: The Overview stays
This change SHALL NOT change the Overview. The Overview SHALL stay the first sidebar entry and the screen that the app shows at its start. On an Alarm the window SHALL open and show the Overview, also while a Dashboard shows and also in Edit mode.

#### Scenario: Alarm while a Dashboard shows
- **WHEN** a Dashboard shows and the daemon raises an Alarm
- **THEN** the window shows the Overview

#### Scenario: Alarm in Edit mode
- **WHEN** the user moved a Tile in Edit mode and the daemon raises an Alarm
- **THEN** the window shows the Overview, Edit mode ends, and the Tile is at its new place when the user opens the Dashboard again

#### Scenario: Alarm with the Tile settings open
- **WHEN** the user changed the decimals in the Tile settings from 1 to 3, did not press "Save", and the daemon raises an Alarm
- **THEN** the window shows the Overview, the dialog is closed, and the Tile still has 1 decimal

#### Scenario: Banner on a Dashboard
- **WHEN** a poweroff countdown runs and the user opens a Dashboard
- **THEN** the poweroff banner shows above the tab strip

### Requirement: Empty states
With no Dashboard, the screen SHALL show one card with the title "No Dashboard yet", a text that says what a Dashboard is, and the command "New Dashboard". A Dashboard with no Tile SHALL show "No Tile yet" and the command "Add Tile", which turns Edit mode on.

#### Scenario: First use
- **WHEN** the user opens Dashboards and no Dashboard exists
- **THEN** the card "No Dashboard yet" shows with the text "A Dashboard is a page that you build from Tiles: values, gauges, bars, charts and texts."

#### Scenario: Empty Dashboard
- **WHEN** the user presses "Add Tile" on a Dashboard with no Tile
- **THEN** Edit mode is on and the menu of the Tile kinds is open

### Requirement: New Dashboard
"New Dashboard" SHALL open a menu with "Empty Dashboard" and one row per Loop, "From Loop '<name>'". "Empty Dashboard" SHALL add a Dashboard with the name "Dashboard <n>", where n is the lowest number of 1 or more that no Dashboard name of this form uses. It SHALL add it after the last tab, select it and turn Edit mode on. A row of a Loop SHALL add the Dashboard of the generator after the last tab and select it, and SHALL keep Edit mode as it is.

#### Scenario: First empty Dashboard
- **WHEN** no Dashboard exists and the user picks "Empty Dashboard"
- **THEN** the Dashboard "Dashboard 1" exists, is selected and is in Edit mode

#### Scenario: Next number
- **WHEN** "Dashboard 1" and "Dashboard 3" exist and the user picks "Empty Dashboard"
- **THEN** the new Dashboard has the name "Dashboard 2"

#### Scenario: Two Loops
- **WHEN** the Loops "Main loop" and "GPU loop" exist and the user opens "New Dashboard"
- **THEN** the menu has the rows "Empty Dashboard", "From Loop 'Main loop'" and "From Loop 'GPU loop'"

### Requirement: Dashboard commands
In Edit mode a menu SHALL have the commands "Rename", "Move left", "Move right" and "Delete Dashboard" for the selected Dashboard. "Move left" SHALL be off for the first tab and "Move right" for the last. "Delete Dashboard" SHALL ask before it deletes, and SHALL then select the tab at the same position, or the last tab.

#### Scenario: Move right
- **WHEN** the tabs are "Main loop", "GPU", "Fans" and the user picks "Move right" on "Main loop"
- **THEN** the tabs are "GPU", "Main loop", "Fans" and "Main loop" stays selected

#### Scenario: Delete asks
- **WHEN** the user picks "Delete Dashboard" on "Main loop" with 9 Tiles
- **THEN** a dialog asks "Delete the Dashboard 'Main loop'? Its 9 Tiles are removed." with the buttons "Cancel" and "Delete Dashboard"

#### Scenario: Cancel
- **WHEN** the user presses "Cancel" in that dialog
- **THEN** the Dashboard "Main loop" still has its 9 Tiles

#### Scenario: Delete the last tab
- **WHEN** the user deletes the last of three tabs
- **THEN** the second tab is selected

### Requirement: Edit mode
A Dashboard SHALL have an Edit mode. The button "Edit" SHALL turn it on. In Edit mode the screen SHALL show the cells of the grid, a toolbar with "Add Tile", "Undo", "Redo", the Dashboard menu and "Done", and a resize handle on the selected Tile. "Done", Escape with no open dialog or menu, and a change to another screen SHALL end Edit mode. Outside Edit mode, no command SHALL change a Tile.

#### Scenario: Turn on
- **WHEN** the user presses "Edit"
- **THEN** the toolbar and the grid cells show, and the first Tile is selected

#### Scenario: Drag in view mode
- **WHEN** Edit mode is off and the user drags a Tile by 200 px
- **THEN** the Tile stays at its place

#### Scenario: Escape
- **WHEN** Edit mode is on, no dialog and no menu is open, and the user presses Escape
- **THEN** Edit mode is off

#### Scenario: Other tab
- **WHEN** Edit mode is on and the user selects the tab of another Dashboard
- **THEN** Edit mode stays on for the other Dashboard

#### Scenario: Rows at the minimum window in Edit mode
- **WHEN** the window is 1100x700, no banner shows and Edit mode is on
- **THEN** 7 rows show without a scroll

### Requirement: Rows of the grid in Edit mode
In Edit mode the grid SHALL show the cells of every row down to 8 rows below the last Tile, and not below row 63. A Dashboard with no Tile SHALL show 8 rows of cells.

#### Scenario: Rows below the last Tile
- **WHEN** Edit mode is on and the lowest Tile of the Dashboard ends at row 2
- **THEN** the grid shows the cells of the rows 0 to 10

#### Scenario: Last rows of the grid
- **WHEN** Edit mode is on and the lowest Tile ends at row 60
- **THEN** the grid shows the cells of the rows 0 to 63

### Requirement: Changes are stored at once
The app SHALL store each change of a Dashboard within 2 s after its command ends: after the release of a drag, after a key command, and after "Save" of a dialog. The app SHALL write the file at most once per second, with the newest state. The app SHALL write a change that waits before it quits. Edit mode SHALL have no step that stores and no step that discards.

#### Scenario: Drag
- **WHEN** the user releases a Tile at a new place
- **THEN** the file of the Dashboards holds the new place within 2 s

#### Scenario: Quit in Edit mode
- **WHEN** the user moved three Tiles in Edit mode and picks "Quit" at once, without "Done"
- **THEN** the three Tiles are at their new places after the next start

#### Scenario: Held key
- **WHEN** the user holds Alt+Down for 1 s and the key repeats 30 times
- **THEN** the app writes the file 2 times or fewer in that second, and the file then holds the last place of the Tile

### Requirement: Undo
Edit mode SHALL keep the last 50 changes of its session. "Undo" SHALL take the newest change back, and "Redo" SHALL make it again. One drag, one key command and one "Save" of a dialog SHALL each be one change. An undo SHALL show the Dashboard that it changes. The end of Edit mode and "New Dashboard" SHALL clear the list.

#### Scenario: Undo a remove
- **WHEN** the user removes a gauge Tile and presses Ctrl+Z
- **THEN** the gauge Tile is back at its place with its settings

#### Scenario: Redo
- **WHEN** the user then presses Ctrl+Shift+Z
- **THEN** the gauge Tile is removed again

#### Scenario: 51 changes
- **WHEN** the user makes 51 changes and presses Ctrl+Z 51 times
- **THEN** the first change stays and the 50 newer changes are taken back

#### Scenario: Undo of a deleted Dashboard
- **WHEN** the user deletes the Dashboard "GPU" in Edit mode and presses Ctrl+Z
- **THEN** the Dashboard "GPU" is back at its tab position and is selected

#### Scenario: After Done
- **WHEN** the user presses "Done" and "Edit" again
- **THEN** "Undo" is off

#### Scenario: After New Dashboard
- **WHEN** the user moves a Tile in Edit mode and then picks "Empty Dashboard"
- **THEN** "Undo" is off

### Requirement: Add a Tile
"Add Tile" SHALL open a menu of the five kinds and then the Tile settings. "Save" SHALL add the Tile with the default size of its kind at the first free place: the place with the lowest row, and in that row the lowest column, where every cell of the Tile is free. "Cancel" SHALL add no Tile. The new Tile SHALL be the selected one.

#### Scenario: Next to a gauge
- **WHEN** a Dashboard holds one gauge Tile of 3x3 at column 0, row 0 and the user adds a value Tile
- **THEN** the value Tile is at column 3, row 0 with 3 columns and 1 row

#### Scenario: Row is full
- **WHEN** row 0 holds Tiles on the columns 0 to 8 and the user adds a chart Tile of 6x3
- **THEN** the chart Tile is at column 0 of the first row where 6 columns of 3 rows are free

#### Scenario: Cancel
- **WHEN** the user picks "Gauge" and presses "Cancel" in the Tile settings
- **THEN** the Dashboard has no new Tile

#### Scenario: No free place
- **WHEN** no place of the 64 rows is free for the default size
- **THEN** the Tile settings say "This Dashboard has no free place for this Tile." and "Save" is off

### Requirement: Remove a Tile
In Edit mode the Delete key and the command "Remove" in the Tile settings SHALL remove the selected Tile without a question. The other Tiles SHALL keep their places. The next Tile in reading order SHALL become the selected one, or the previous Tile when the removed Tile was the last one.

#### Scenario: Remove
- **WHEN** a Dashboard holds Tiles at column 0, 3 and 6 of row 0 and the user removes the Tile at column 3
- **THEN** the Tiles at column 0 and column 6 stay, and the Tile at column 6 is selected

#### Scenario: No compaction
- **WHEN** the user removes every Tile of row 0 and row 1 holds Tiles
- **THEN** the Tiles of row 1 stay in row 1

### Requirement: Move a Tile
In Edit mode a drag of a Tile SHALL move it by the distance of the drag: the new place is the old place plus that distance, at the nearest cell. The move SHALL be made only when every cell of the Tile at the new place is in the grid and free. Otherwise the Tile SHALL go back. During the drag, an outline SHALL show the target cells, in the accent color when the move is possible and dashed when it is not.

#### Scenario: Drag by 150 px
- **WHEN** the window is 1100 px wide and the user drags a Tile at column 0 by 150 px to the right, onto free cells
- **THEN** the Tile is at column 2

#### Scenario: Drag down
- **WHEN** the user drags a Tile at row 0 by 80 px down, onto free cells
- **THEN** the Tile is at row 1

#### Scenario: Occupied target
- **WHEN** the user releases a Tile over cells that another Tile covers
- **THEN** the Tile goes back to its place and no change is stored

#### Scenario: Escape during a drag
- **WHEN** the user presses Escape during a drag
- **THEN** the Tile goes back to its place and Edit mode stays on

### Requirement: Resize a Tile
In Edit mode a drag of the handle at the lower right corner of the selected Tile SHALL change its column span and its row span by the distance of the drag, to the nearest cell. The Tile SHALL not get smaller than the minimum size of its kind, wider than 12 columns less its column, higher than 8 rows, or lower than row 63. It SHALL not grow over a cell that another Tile covers: the spans then stop at the last free size.

#### Scenario: Grow by two columns
- **WHEN** the window is 1100 px wide and the user drags the handle of a Tile of 3x1 by 140 px to the right, over free cells
- **THEN** the Tile has 5 columns

#### Scenario: Neighbor in the way
- **WHEN** a Tile of 3x1 at column 0 has a neighbor at column 4 and the user drags its handle 300 px to the right
- **THEN** the Tile has 4 columns

#### Scenario: Maximum height
- **WHEN** the user drags the handle of a chart Tile 900 px down, over free cells
- **THEN** the Tile has 8 rows

### Requirement: Keyboard selection in the grid
The grid SHALL be one focus stop. Left and Right SHALL select the previous and the next Tile in reading order. Up and Down SHALL select the nearest Tile above and below that shares a column with the selected Tile. Tab SHALL leave the grid. Outside Edit mode, Enter on a Tile with a sensor SHALL open History with that sensor selected. The selected Tile SHALL have the focus ring. The grid SHALL scroll so that the selected Tile shows.

#### Scenario: Selected Tile out of view
- **WHEN** the window is 1100x700, the Tile at row 0 is selected and the user presses Down until a Tile at row 12 is selected
- **THEN** the grid scrolls and the Tile at row 12 shows

#### Scenario: Next Tile
- **WHEN** the Tile at column 0, row 0 is selected and the user presses Right
- **THEN** the next Tile in reading order is selected and has the focus ring

#### Scenario: Tile below
- **WHEN** a Tile on the columns 3 to 5 of row 0 is selected, row 1 holds a Tile on the columns 0 to 2 and a Tile on the columns 4 to 9, and the user presses Down
- **THEN** the Tile on the columns 4 to 9 is selected

#### Scenario: Enter outside Edit mode
- **WHEN** Edit mode is off, a Tile on "Water temperature" is selected and the user presses Enter
- **THEN** History opens with "Water temperature" selected

#### Scenario: Tab
- **WHEN** the grid has the focus and the user presses Tab
- **THEN** the focus leaves the grid

### Requirement: Ends and ties of the keyboard selection
Left at the first Tile and Right at the last Tile SHALL change nothing. Up and Down SHALL change nothing when no Tile above or below shares a column. When two Tiles above or below are at the same distance, Up and Down SHALL select the one with the lowest column. With no selected Tile, an arrow key SHALL select the first Tile.

#### Scenario: Last Tile
- **WHEN** the last Tile in reading order is selected and the user presses Right
- **THEN** the same Tile is selected

#### Scenario: Two Tiles below
- **WHEN** a Tile on the columns 0 to 5 of row 0 is selected, row 1 holds a Tile on the columns 0 to 2 and a Tile on the columns 3 to 5, and the user presses Down
- **THEN** the Tile on the columns 0 to 2 is selected

### Requirement: Keyboard move of a Tile
In Edit mode, Alt with an arrow key SHALL move the selected Tile to the next free place in that direction. This is the place one cell further when every cell of the Tile there is free. Otherwise it is the first place further in that direction where every cell of the Tile is free, over the Tiles in the way. With no such place, the Tile SHALL stay.

#### Scenario: Move right
- **WHEN** a Tile is at column 0 and the cell at its right is free, and the user presses Alt+Right
- **THEN** the Tile is at column 1

#### Scenario: Move over a neighbor
- **WHEN** a Tile of 3x1 at column 0 has a neighbor of 3x1 at column 3, the columns 6 to 11 of the row are free, and the user presses Alt+Right
- **THEN** the Tile is at column 6

#### Scenario: Move with no free place
- **WHEN** a Tile of 3x1 at column 0 has neighbors on the columns 3 to 11 of its row and the user presses Alt+Right
- **THEN** the Tile stays at column 0

#### Scenario: Move below a wide Tile
- **WHEN** a Tile of 3x1 at row 0 has a Tile of 12x1 at row 1, row 2 is free, and the user presses Alt+Down
- **THEN** the Tile is at row 2

### Requirement: Keyboard commands of Edit mode
In Edit mode, on the selected Tile: Shift+Right and Shift+Down SHALL make it one cell larger, and Shift+Left and Shift+Up one cell smaller. Enter SHALL open the Tile settings. Delete SHALL remove it. Insert SHALL open "Add Tile". Ctrl+Z SHALL undo and Ctrl+Shift+Z SHALL redo. A resize that the grid rules do not allow SHALL change nothing.

#### Scenario: Grow
- **WHEN** a Tile of 3x1 is selected, the cells below it are free and the user presses Shift+Down
- **THEN** the Tile has 2 rows

#### Scenario: Every command without the mouse
- **WHEN** the user turns Edit mode on with Tab and Enter on "Edit", adds a Tile with Insert, moves it with Alt+Down, makes it larger with Shift+Right, opens its settings with Enter and removes it with Delete
- **THEN** each step works and the mouse is not used

### Requirement: Tile settings
In Edit mode a double click or Enter on a Tile SHALL open the Tile settings: a dialog with "Kind", "Sensor", "Title", "Decimals", "Minimum" and "Maximum" (gauge and bar), "Time span" (chart), "Warning limit" and "Alarm limit", or, for a text Tile, "Kind", "Text" and "Style". "Save" SHALL be one change. "Cancel" and Escape SHALL change nothing. "Save" SHALL be off, with the reason, while a Tile with a sensor has no sensor or a text Tile has no text.

#### Scenario: Fields of a gauge
- **WHEN** the user opens the settings of a gauge Tile
- **THEN** the dialog has "Minimum" and "Maximum" and has no "Time span"

#### Scenario: No sensor
- **WHEN** the user adds a value Tile and picks no sensor
- **THEN** "Save" is off and the dialog says "Choose a sensor."

#### Scenario: Number that does not parse
- **WHEN** the user types "abc" in "Maximum"
- **THEN** "Save" is off and the dialog says "Enter a number."

#### Scenario: Limit off
- **WHEN** the switch of "Warning limit" is off
- **THEN** its direction and its value are off, and the Tile has no warning limit after "Save"

#### Scenario: Limit with a unit
- **WHEN** the sensor is in `degC` and the user turns "Alarm limit" on
- **THEN** the value input shows the unit "°C"

#### Scenario: Escape
- **WHEN** the user changes the decimals from 1 to 3 and presses Escape
- **THEN** the Tile still has 1 decimal and Edit mode stays on

### Requirement: Open the sensor from a Tile
Outside Edit mode, a click on a Tile with a sensor SHALL open History with that sensor selected. A hover SHALL show a tooltip with the full name of the sensor, the value, the Tile limits and, for a live value, the age of the value. A text Tile SHALL take no click.

#### Scenario: Click
- **WHEN** the user clicks a gauge Tile on the sensor "Pressure"
- **THEN** History opens with "Pressure" selected

#### Scenario: Tooltip
- **WHEN** the pointer is over a Tile on "Water temperature" with the value 31.4 °C that arrived 1 s ago and the warning limit above 35
- **THEN** the tooltip holds "Water temperature", "31.4 °C", "Warning limit: above 35.0 °C" and "1 s ago"

### Requirement: Generator from a Loop
The command "Create a Dashboard from this Loop" SHALL add one Dashboard for one Loop and select it. It SHALL be in the Loops screen for the selected Loop, and in the menu of "New Dashboard" as "From Loop '<name>'". The Dashboard SHALL have the name of the Loop. When a Dashboard has that name, the new name SHALL be the Loop name, a space and the lowest number from 2 up that makes it free. The Loop name SHALL be cut so that the whole name has 40 characters at most.

#### Scenario: First Dashboard of a Loop
- **WHEN** the user picks "Create a Dashboard from this Loop" on the Loop "Main loop" and no Dashboard has that name
- **THEN** a Dashboard "Main loop" exists and the Dashboards screen shows it, not in Edit mode

#### Scenario: Second Dashboard of a Loop
- **WHEN** the user picks the command again
- **THEN** a second Dashboard exists with the name "Main loop 2"

#### Scenario: Long Loop name
- **WHEN** a Loop has a name of 45 characters and a Dashboard has the first 40 characters of it as its name
- **THEN** the new Dashboard has the first 38 characters of the Loop name, a space and "2" as its name

#### Scenario: At the limit
- **WHEN** 16 Dashboards exist
- **THEN** the command is off in the Loops screen and its tooltip says "A user can have 16 Dashboards."

### Requirement: Tiles of a generated Dashboard
The generator SHALL make these Tiles, in this order, each only when the Loop has the part and the sensor list holds its sensor: a gauge 3x3 of the Leakshield pressure with the range 0 to 450; a bar 3x1 of the fill level with the range 0 to 100; a value 3x1 of the calibrated flow; a chart 6x2 of the water temperature with 60 minutes; a value 3x1 of the speed of each Pump channel of the Loop; a value 3x1 of the speed of each fan Channel of the Loop, 8 at most. It SHALL set no Tile limit.

#### Scenario: Loop with every part
- **WHEN** the Loop "Main loop" has a Leakshield, a calibrated flow, a water temperature sensor and the Pump channel "Waterpump" with a speed sensor, and no fan Channel has a speed sensor
- **THEN** the Dashboard has 5 Tiles: a gauge on the pressure, a bar on the fill level, a value on the flow, a chart on the water temperature and a value on the pump speed

#### Scenario: No limit
- **WHEN** the generator makes a Dashboard
- **THEN** no Tile of it has a warning limit or an alarm limit

#### Scenario: Pump without a speed sensor
- **WHEN** a Pump channel of the Loop has no speed sensor
- **THEN** the Dashboard has no Tile for that Pump channel

### Requirement: Titles of the generated Tiles
A generated Tile of a Channel SHALL have the name of the Channel as its own title, cut to 40 characters. The other generated Tiles SHALL have no own title.

#### Scenario: Titles of the Channels
- **WHEN** the generator makes the Tile of the Pump channel "Waterpump"
- **THEN** the Tile has the own title "Waterpump", and the other four Tiles have no own title

#### Scenario: Long Channel name
- **WHEN** a Pump channel has a name of 64 characters
- **THEN** the own title of its Tile is the first 40 characters of the name

### Requirement: Places of the generated Tiles
The generator SHALL put each Tile, in the order of the list, at the first free place, by the rule of "Add a Tile".

#### Scenario: Loop with a Leakshield
- **WHEN** the generator makes the 5 Tiles of "Main loop" and two fan Tiles
- **THEN** the gauge is at column 0, row 0, the bar at column 3, row 0, the flow at column 6, row 0, the chart at column 3, row 1, the pump speed at column 9, row 0, and the fans at column 9, row 1 and column 9, row 2

#### Scenario: Three more fans
- **WHEN** the Loop has five fan Channels with a speed sensor
- **THEN** the third, fourth and fifth fan Tiles are at column 0, column 3 and column 6 of row 3

### Requirement: Generator for a Loop without a Leakshield
For a Loop with no Leakshield the generator SHALL make no pressure Tile and no fill level Tile, and SHALL make the other Tiles. It SHALL NOT refuse the command. For a Loop with no part that has a sensor, it SHALL make a Dashboard with no Tile.

#### Scenario: No Leakshield
- **WHEN** the Loop "GPU loop" has a calibrated flow, a water temperature sensor and one Pump channel with a speed sensor, and no Leakshield and no fan
- **THEN** the Dashboard has the flow at column 0, row 0, the chart at column 3, row 0 with 6 columns and 2 rows, and the pump speed at column 9, row 0

#### Scenario: Loop with no sensor
- **WHEN** a Loop has no Leakshield, no flow sensor, no water temperature sensor and no Pump channel
- **THEN** the Dashboard exists and shows "No Tile yet"

### Requirement: Fan Channels of a Loop
The fan Channels of a Loop SHALL be the Channels that are not a Pump channel of any Loop, that have the control kind Curve, and whose Curve has as its Input the water temperature sensor of the Loop, or a Derived sensor that has this sensor as a direct source. The Curve SHALL be the one that the daemon reports for the Channel: the Curve of the active Profile. A Curve whose Input is a Graph sensor SHALL NOT match. The order SHALL be the order of the Channels screen.

#### Scenario: Fans on the water temperature
- **WHEN** "Chassis Fan 1" and "Chassis Fan 2" have a Curve with the Input "Water temperature" of the Loop, and "CPU Fan" has a Curve with the Input "CPU temperature"
- **THEN** the fan Channels of the Loop are "Chassis Fan 1" and "Chassis Fan 2"

#### Scenario: Derived Input
- **WHEN** "Chassis Fan 3" has a Curve whose Input is a Derived sensor "Delta T" with the water temperature of the Loop as a source
- **THEN** "Chassis Fan 3" is a fan Channel of the Loop

#### Scenario: Fixed Duty with an old Input
- **WHEN** "Chassis Fan 4" has the control kind Fixed, and its Curve still has the Input "Water temperature" of the Loop
- **THEN** "Chassis Fan 4" is not a fan Channel of the Loop by the Curve rule

#### Scenario: Ten fans
- **WHEN** the Loop has ten fan Channels with a speed sensor
- **THEN** the Dashboard has Tiles for the first eight

### Requirement: Fan Channels when no Curve matches
When no Channel matches the rule of "Fan Channels of a Loop", the fan Channels of the Loop SHALL be all Channels that are not a Pump channel of any Loop, in the order of the Channels screen.

#### Scenario: No Curve on the water temperature
- **WHEN** every Channel runs on its Hardware curve
- **THEN** the fan Channels of the Loop are all Channels that are not a Pump channel

#### Scenario: Pump channel of another Loop
- **WHEN** every Channel runs on its Hardware curve and "Pump 2" is a Pump channel of the Loop "GPU loop"
- **THEN** "Pump 2" is not a fan Channel of the Loop "Main loop"

### Requirement: A generated Dashboard is a normal Dashboard
A generated Dashboard SHALL have no link to its Loop. A later change of the Loop SHALL NOT change it. The user SHALL edit it as any Dashboard.

#### Scenario: Loop renamed
- **WHEN** the user renames the Loop "Main loop" to "CPU loop" after the generator ran
- **THEN** the Dashboard keeps the name "Main loop" and its Tiles

#### Scenario: Leakshield removed from the Loop
- **WHEN** the user sets the Leakshield of the Loop to "None"
- **THEN** the pressure Tile stays and shows the pressure of the Leakshield

### Requirement: Storage file
The app SHALL store the Dashboards of a user in `$XDG_CONFIG_HOME/nicecooling/dashboards.toml`, with a top-level `schema_version` of 1 and the fields that `docs/storage.md` defines. The app SHALL be the only writer. It SHALL write the file with the atomic write of `docs/storage.md`. The daemon SHALL never read the file.

#### Scenario: Restart
- **WHEN** the user builds a Dashboard with 5 Tiles and restarts the app
- **THEN** the Dashboard shows the same 5 Tiles with the same places and settings

#### Scenario: Round trip of every field
- **WHEN** the app stores a Dashboard that has one Tile of each kind, with both Tile limits on one of them, and loads the file
- **THEN** the loaded Dashboard is equal to the stored one

#### Scenario: Fields left out
- **WHEN** a gauge Tile in the file has no `title`, no `decimals`, no `min`, no `max` and no Tile limit
- **THEN** the Tile loads with an empty own title, 0 decimals, the range 0 to 100 and no Tile limit

#### Scenario: Fields that the app writes
- **WHEN** the app stores a chart Tile with an empty own title and no Tile limit
- **THEN** the file holds `title = ""` for the Tile and holds no `warning`, no `alarm`, no `min` and no `max` for it

### Requirement: Dashboards of one user
Each user SHALL have their own Dashboards. The Dashboards of one user SHALL NOT show for another user. The app SHALL NOT protect the file against another local user beyond the file mode of the settings directory.

#### Scenario: Second user
- **WHEN** the user `alice` builds a Dashboard and the user `bob` starts the app
- **THEN** `bob` sees "No Dashboard yet"

### Requirement: Missing file
With no file, the app SHALL show no Dashboard. It SHALL create the file, and the directory when needed, at the first change.

#### Scenario: First start
- **WHEN** the app starts and `dashboards.toml` does not exist
- **THEN** the Dashboards screen shows "No Dashboard yet" and the app writes no file

#### Scenario: First Dashboard
- **WHEN** the user then picks "Empty Dashboard"
- **THEN** the file exists and holds one Dashboard with the name "Dashboard 1"

### Requirement: File that is not valid
A file SHALL be valid when it parses and every Dashboard and Tile in it obeys the count limits, the grid rules, the sizes and the ranges of this spec. The app SHALL NOT show and SHALL NOT write a file that is not valid. It SHALL show its path and the first fault, with "Try again" and "Start with no Dashboard". A file whose Tiles are not in reading order, or that holds a key that the app does not know, SHALL be valid.

#### Scenario: Parse error
- **WHEN** the file holds text that is not TOML
- **THEN** the screen says "The Dashboards file cannot be read.", shows the path, and has no tab and no "New Dashboard"

#### Scenario: Two Tiles on one cell
- **WHEN** the second Tile of the first Dashboard in the file covers a cell of its first Tile
- **THEN** the screen names the fault "Dashboard 1, Tile 2: it covers a cell of Tile 1"

#### Scenario: Try again
- **WHEN** the user repairs the file in an editor and presses "Try again"
- **THEN** the Dashboards show

#### Scenario: File stays
- **WHEN** the file is not valid and the app runs for one hour
- **THEN** the file has the same bytes

#### Scenario: Tiles out of reading order
- **WHEN** the file lists a Tile at row 1 before a Tile at row 0
- **THEN** the file is valid, and the app holds the Tile of row 0 first

#### Scenario: Tile without a sensor field
- **WHEN** the third Tile of the first Dashboard has the kind `value` and no `sensor`
- **THEN** the screen names the fault "Dashboard 1, Tile 3: it has no sensor"

### Requirement: Start with no Dashboard
For a file that is not valid, "Start with no Dashboard" SHALL rename the file to `dashboards.toml.bad` and start with no Dashboard. The rename SHALL replace an older `dashboards.toml.bad`.

#### Scenario: Start with no Dashboard
- **WHEN** the user presses "Start with no Dashboard"
- **THEN** `dashboards.toml.bad` holds the old content, `dashboards.toml` does not exist, and the screen shows "No Dashboard yet"

#### Scenario: Second use
- **WHEN** `dashboards.toml.bad` exists and the user presses "Start with no Dashboard" for another file that is not valid
- **THEN** `dashboards.toml.bad` holds the content of that other file

### Requirement: File of a newer version
The app SHALL read `schema_version` before the rest of the file. With a `schema_version` above 1, the app SHALL NOT show and SHALL NOT write the file. The screen SHALL say that the file comes from a newer version, with its path. It SHALL have no command that changes a Dashboard and no "Start with no Dashboard". A later version that changes a count limit or a minimum size of this spec SHALL raise `schema_version`.

#### Scenario: Version 2
- **WHEN** the file has `schema_version = 2` and a Tile kind that this version does not know
- **THEN** the screen says "The Dashboards file comes from a newer nicecooling. This version does not change it." and the file keeps its bytes

#### Scenario: Generator with a newer file
- **WHEN** the file has `schema_version = 2`
- **THEN** "Create a Dashboard from this Loop" is off in the Loops screen, and its tooltip gives the reason

### Requirement: Failed write
When the write of the file fails, the app SHALL keep the change in memory, SHALL show the error on the Dashboards screen with the command "Try again", and SHALL write again at the next change and at "Try again".

#### Scenario: Directory not writable
- **WHEN** the settings directory is read-only and the user moves a Tile
- **THEN** the Tile shows at its new place and the screen says "The Dashboards cannot be stored:" with the error text

#### Scenario: Next change
- **WHEN** the directory is writable again and the user moves another Tile
- **THEN** the file holds both moves and the error text is gone

#### Scenario: Try again
- **WHEN** the directory is writable again and the user presses "Try again"
- **THEN** the file holds the move and the error text is gone

### Requirement: The settings file does not change
This change SHALL add no key to `settings.toml` and SHALL NOT raise its `schema_version`. A change of the app settings SHALL NOT write `dashboards.toml`. An app version without Dashboards then reads and writes `settings.toml` as before and never opens `dashboards.toml`.

#### Scenario: Settings file
- **WHEN** the user creates a Dashboard, adds Tiles and deletes the Dashboard
- **THEN** `settings.toml` has the same bytes as before

#### Scenario: Settings change
- **WHEN** the user hides a sensor in the Sensors screen
- **THEN** `dashboards.toml` has the same bytes as before

### Requirement: Nothing in the daemon
Dashboards SHALL run in the app only. A command of a Dashboard SHALL make no call of the daemon and SHALL ask no password. This change SHALL add no member to the D-Bus API and no code to the daemon.

#### Scenario: Edit as a user outside wheel
- **WHEN** a user outside the group `wheel` adds, moves and removes Tiles
- **THEN** each command works and no polkit dialog shows

#### Scenario: Calls of the daemon
- **WHEN** the user creates a Dashboard from a Loop and edits it
- **THEN** the daemon gets no method call that it does not get on the Overview

### Requirement: Languages
Every text of the Dashboards screen, the Tile settings and the generator command SHALL exist in English, Italian, German and French, with the terms of `docs/i18n-terms.md` for Dashboard and Tile.

#### Scenario: Italian
- **WHEN** the language is Italian
- **THEN** the sidebar entry is "Dashboard" and the command "Add Tile" is "Aggiungi riquadro"

#### Scenario: German
- **WHEN** the language is German
- **THEN** the command "Add Tile" is "Kachel hinzufügen"
