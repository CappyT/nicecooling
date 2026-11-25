# History chart

## Purpose

Defines the chart manager of the History screen. A View is a saved chart setup, with the style of each series and the axis limits per unit. A Note is a text at a time of the history. The capability also covers the image of the chart. The app keeps all of it in the files of the user.

## ADDED Requirements

### Requirement: View content
A View SHALL hold a name, the series in their order, the range length, the style and the hidden state of each series, and the axis limits per unit. The range length SHALL be one of 1 hour, 24 hours, 48 hours, 7 days, 30 days and 1 year. A View SHALL hold no time position, no zoom and no data. A View SHALL survive a restart of the app.

#### Scenario: Saved setup
- **WHEN** the chart shows 4 series with the range 24 hours, one series in a custom color, and the user saves it as "Loop temperatures"
- **THEN** the View holds the 4 sensor ids in the legend order, the range 24 hours and the color of each series

#### Scenario: Zoom is not stored
- **WHEN** the user zooms to 10 minutes of yesterday and saves the View
- **THEN** the View holds the range length of the toolbar and no zoom

#### Scenario: Restart
- **WHEN** the user saves a View and restarts the app
- **THEN** the list of Views shows it

### Requirement: Save a View
"Save view as" SHALL ask for a name of 1 to 48 characters. The app SHALL remove spaces at both ends. A name that another View has, compared without case, SHALL turn "Save" off with the reason. The app SHALL hold 32 Views at most and 32 series in a View at most, and SHALL give the reason when a limit stops a save.

#### Scenario: New View
- **WHEN** the user enters " Loop temperatures " and presses "Save"
- **THEN** a View with the name "Loop temperatures" exists and is the loaded View

#### Scenario: Name in use
- **WHEN** a View "Loop temperatures" exists and the user enters "loop Temperatures"
- **THEN** "Save" is off and the text reads "A View with this name exists."

#### Scenario: Thirty-third View
- **WHEN** 32 Views exist
- **THEN** "Save view as" is off with the text "32 Views exist. Delete a View to save a new one."

#### Scenario: Too many series
- **WHEN** the chart shows 33 series
- **THEN** "Save view as" is off with the text "A View holds 32 series at most."

### Requirement: Load a View
A click on a View in the list SHALL replace the series, the styles, the axis limits and the range length of the chart with those of the View. The window SHALL then follow the present time with no zoom. The loaded View SHALL show its name in the toolbar.

#### Scenario: Load
- **WHEN** the chart shows 2 series zoomed to a time of last week, and the user loads "Loop temperatures" with 4 series and the range 24 hours
- **THEN** the chart shows the 4 series over the last 24 hours and the toolbar shows "Loop temperatures"

#### Scenario: Fixed axis of the View
- **WHEN** the View has the limits 20 to 45 for °C
- **THEN** the °C axis of the chart goes from 20 to 45

### Requirement: Change, rename and delete a View
When the chart setup differs from the loaded View, the toolbar SHALL mark the View as changed, and "Save view" SHALL store the setup in that View. "Rename" SHALL follow the name rules of a new View. "Delete view" SHALL ask a confirmation that names the View. The delete SHALL leave the chart as it is and SHALL delete no history row.

#### Scenario: Changed mark
- **WHEN** the user adds a fifth series to the loaded View "Loop temperatures"
- **THEN** the toolbar shows "Loop temperatures (changed)" and "Save view" is on

#### Scenario: Save over the View
- **WHEN** the user presses "Save view"
- **THEN** the View holds 5 series and the mark is gone

#### Scenario: Delete
- **WHEN** the user picks "Delete view" for "Loop temperatures" and confirms the dialog "Delete the View Loop temperatures?"
- **THEN** the list has no such View, the chart still shows its series, and the store holds the same rows

### Requirement: View at the start
When the History screen opens for the first time after the start of the app, it SHALL load the View that was loaded last, when that View still exists. Without such a View, the screen SHALL open with no sensor selected. A chart setup that was not saved SHALL NOT survive a restart.

#### Scenario: Last View
- **WHEN** "Loop temperatures" was the loaded View and the app restarts
- **THEN** the History screen opens with "Loop temperatures"

#### Scenario: No View
- **WHEN** no View exists
- **THEN** the screen shows "Select sensors to chart." with the picker open

#### Scenario: Sensor opened from the Sensors screen
- **WHEN** the user presses Enter on a row of the Sensors screen
- **THEN** History opens with that sensor only, and no View is loaded

### Requirement: Sensor of a View that is not found
A series whose sensor the daemon does not report SHALL stay in the View and in the chart. While the daemon is connected, its legend row SHALL show the name that the View stored and the tag "Sensor not found". The chart SHALL draw the rows that the store still holds for it. The series SHALL be live again when a sensor with the same id returns.

#### Scenario: Device unplugged
- **WHEN** a View holds `gpu_temp` with the stored name "GPU temperature" and the daemon reports no such sensor
- **THEN** the legend row reads "GPU temperature" with the tag "Sensor not found", and the chart draws the rows of `gpu_temp` up to its last sample

#### Scenario: Save keeps the series
- **WHEN** the user saves the View while `gpu_temp` is not found
- **THEN** the View still holds `gpu_temp`

#### Scenario: Sensor returns
- **WHEN** the daemon reports `gpu_temp` again
- **THEN** the tag is gone and the line continues after a gap

#### Scenario: Daemon offline
- **WHEN** the daemon is offline
- **THEN** no legend row shows the tag "Sensor not found"

### Requirement: Views file that cannot be read
When the app cannot read the Views file, the History screen SHALL show "The saved Views cannot be read." with the path, and the chart SHALL work with no View. The app SHALL leave the file unchanged until the user saves a View. It SHALL then keep the old content in a copy with the suffix `.bad`. The app SHALL NOT write a Views file with a higher schema version than it knows.

#### Scenario: Damaged file
- **WHEN** the Views file is not valid text of its format
- **THEN** the list of Views is empty, the screen shows the text with the path, and the file is unchanged

#### Scenario: Save after a damaged file
- **WHEN** the user then saves a View
- **THEN** a file with the suffix `.bad` holds the old content and the Views file holds one View

#### Scenario: Newer file
- **WHEN** the Views file has the schema version 2
- **THEN** "Save view as" is off with the text "The Views file is from a newer version of nicecooling."

### Requirement: Series style
Each series of the chart SHALL have a color, a line width and a fill switch. The color SHALL be one of the six series colors or a custom color `#RRGGBB`. The line width SHALL be 1, 2, 3 or 4 px, with the default 2 px. With the fill on, the chart SHALL fill the area between the line and the lower edge of the plot in the series color at 10% opacity. The fill SHALL be off by default. A change SHALL show in the chart at once.

#### Scenario: Default style
- **WHEN** the user selects a third sensor
- **THEN** its series has the third series color, 2 px and no fill

#### Scenario: Width and fill
- **WHEN** the user sets 4 px and turns the fill on for "Water temperature"
- **THEN** its line is 4 px wide and the area under it has its color at 10% opacity

#### Scenario: Series color in both themes
- **WHEN** a series has the second series color and the system theme changes to dark
- **THEN** the line has the dark value of the second series color

#### Scenario: Custom color in both themes
- **WHEN** a series has the color `#3A944A` and the system theme changes to dark
- **THEN** the line still has `#3A944A`

### Requirement: Custom color input
The style popover SHALL take a custom color as `#` and six hexadecimal digits. Any other text SHALL leave the color as it is and show "Enter a color as #RRGGBB." A color with a hue from 345° through 0° to 55° and a saturation of 40% or more SHALL show the caption "Red and amber mark Alarms and Warnings." The app SHALL apply that color.

#### Scenario: Valid color
- **WHEN** the user enters `#2190A4`
- **THEN** the series has that color and no caption shows

#### Scenario: Not a color
- **WHEN** the user enters `blue`
- **THEN** the color stays and the text "Enter a color as #RRGGBB." shows

#### Scenario: Red
- **WHEN** the user enters `#C01C28`
- **THEN** the series is red and the caption "Red and amber mark Alarms and Warnings." shows

### Requirement: Axis limits per unit
Each unit of the chart SHALL have an automatic axis by default. The user SHALL be able to set a fixed minimum and maximum for a unit, each from -1 000 000 to 1 000 000. A minimum that is not below the maximum SHALL leave the limits as they are and show the reason. The limits of a unit SHALL apply to every chart of the screen that has that unit.

#### Scenario: Fixed limits
- **WHEN** the user sets the minimum 20 and the maximum 45 for °C
- **THEN** the °C axis goes from 20 to 45, and a value of 32.5 is in the middle of the plot height

#### Scenario: Back to automatic
- **WHEN** the user turns "Automatic" on for °C
- **THEN** the axis follows the values of the window again

#### Scenario: Minimum above maximum
- **WHEN** the user enters the minimum 50 while the maximum is 45
- **THEN** the limits stay and the text "The minimum must be below the maximum." shows

#### Scenario: Out of range
- **WHEN** the user enters the maximum 2 000 000
- **THEN** the limits stay and the text "Enter a number from -1 000 000 to 1 000 000." shows

### Requirement: Values outside fixed limits
With fixed limits, the chart SHALL draw a line only inside the plot and SHALL cut it at the upper or the lower edge. The legend row of a series with one value or more outside the limits in the window SHALL show the caption "Values outside the axis limits". The minimum and the maximum of the legend SHALL still come from all values of the window.

#### Scenario: Value above the maximum
- **WHEN** the limits of °C are 20 to 45 and a series reaches 47.2
- **THEN** the line ends at the upper edge of the plot, the legend row shows the caption, and its maximum reads 47.2

#### Scenario: All values inside
- **WHEN** every value of the window is from 20 to 45
- **THEN** no legend row shows the caption

### Requirement: Add a Note
A Note SHALL hold a time and a text of 1 to 120 characters. A secondary click in the chart SHALL offer "Add note here" for the time under the pointer, and the key N SHALL do the same for the time of the cursor. The dialog SHALL show the time and take the text. The app SHALL hold 500 Notes at most and SHALL give the reason at the limit.

#### Scenario: Note from a click
- **WHEN** the user makes a secondary click at 14:03:20, picks "Add note here" and enters "Fan curve changed"
- **THEN** a Note with the time 14:03:20 and that text exists

#### Scenario: Empty text
- **WHEN** the text field is empty
- **THEN** "Save" is off

#### Scenario: Text of 121 characters
- **WHEN** the user pastes 121 characters
- **THEN** the field holds the first 120 characters

#### Scenario: Limit
- **WHEN** 500 Notes exist
- **THEN** "Add note here" is off with the text "500 Notes exist. Delete a Note to add a new one."

### Requirement: Notes in the chart
Every chart of the History screen SHALL draw each Note of the window as a vertical dashed line at its time, with the pencil icon at the lower end of the plot. The Note SHALL NOT depend on the shown series or on the loaded View. A pointer over the icon SHALL show the text and the time.

#### Scenario: Note in the window
- **WHEN** a Note has the time 14:03:20 and the window is 14:00 to 15:00
- **THEN** each chart shows a dashed line with the pencil icon at 14:03:20

#### Scenario: Note outside the window
- **WHEN** the window is 16:00 to 17:00
- **THEN** no chart shows that Note

#### Scenario: Other View
- **WHEN** the user loads another View whose window holds 14:03:20
- **THEN** the chart shows the Note

### Requirement: Edit and delete a Note
A click on the icon of a Note SHALL open a dialog with its time and its text. "Save" SHALL store a changed text of 1 to 120 characters. "Delete note" SHALL remove the Note at once. The time of a Note SHALL NOT change.

#### Scenario: Edit
- **WHEN** the user changes the text to "Fan curve changed, 40% floor" and presses "Save"
- **THEN** the pointer over the icon shows the new text

#### Scenario: Delete
- **WHEN** the user presses "Delete note"
- **THEN** the chart shows no line at 14:03:20 and the Notes list has one row fewer

### Requirement: Notes list
The History screen SHALL have a dialog "Notes" with every Note, the newest first: its time, its text, "Show" and "Delete". "Show" SHALL close the dialog and move the window, with the range length of the toolbar and no zoom, so that the Note is in its middle. When that window would end after the present time, it SHALL follow the present time.

#### Scenario: Show an old Note
- **WHEN** the range is 24 hours and the user presses "Show" for a Note at 2026-11-10 14:00:00
- **THEN** the window is 2026-11-10 02:00:00 to 2026-11-11 02:00:00

#### Scenario: Show a Note of the last hour
- **WHEN** the range is 24 hours and the Note is 30 minutes old
- **THEN** the window is the last 24 hours and "Now" is active

#### Scenario: Empty list
- **WHEN** no Note exists
- **THEN** the dialog reads "No Notes. Make a secondary click in the chart to add one."

### Requirement: Image of the chart
"Save as image" SHALL write a PNG file of 1920 x 1080 px in the colors of the theme that the app shows. The image SHALL hold a title, the range as text, each chart with its axes, lines, fills, bands, gaps, Alarm markers and Notes, and the legend with the name, the unit, the minimum and the maximum of each shown series. The title SHALL be the name of the loaded View, or "History" without one. The image SHALL hold no cursor and no control.

#### Scenario: Image of a View
- **WHEN** the user saves the image of the View "Loop temperatures" with 4 shown series in the dark theme
- **THEN** the file is a PNG of 1920 x 1080 px with a dark ground, the title "Loop temperatures" and 4 legend rows

#### Scenario: Hidden series
- **WHEN** the legend hides one of 4 series
- **THEN** the image has 3 lines and 3 legend rows

#### Scenario: Note text
- **WHEN** the window holds a Note with a text of 60 characters
- **THEN** the image shows the first 40 characters of the text and "..." next to the line of the Note

#### Scenario: No chart
- **WHEN** the screen shows "No data in this range."
- **THEN** "Save as image" is off with the text "No chart to save."

### Requirement: Image file
The app SHALL ask for the file with the file dialog of the system and propose the name `nicecooling-history-<start>-<end>.png`, with both times as `YYYYMMDDTHHMMSS` in local time. It SHALL write a temporary file in the same directory and give it the chosen name when the image is complete. When it cannot write the file, it SHALL remove the temporary file and show the error of the system with the path.

#### Scenario: Proposed name
- **WHEN** the window is 2026-11-24 17:13:05 to 2026-11-25 17:13:05 local time
- **THEN** the proposed name is `nicecooling-history-20261124T171305-20261125T171305.png`

#### Scenario: Saved
- **WHEN** the image is complete
- **THEN** the app shows "Image saved" with the path

#### Scenario: Directory without write access
- **WHEN** the user picks a directory that the user cannot write
- **THEN** the app shows "The file cannot be written" with the path and the error text

### Requirement: Image text font
The text of the image SHALL use the interface font of the app when the system has it. Without it, the text SHALL use the default sans-serif font of the system. An image SHALL NOT fail because a font is absent.

#### Scenario: Interface font installed
- **WHEN** the system has Adwaita Sans
- **THEN** the title and the labels of the image are in Adwaita Sans

#### Scenario: Interface font absent
- **WHEN** the system has no Adwaita Sans and no Inter
- **THEN** the app writes the image with the default sans-serif font

### Requirement: Chart manager without the daemon
Views, series styles, axis limits, Notes and the image SHALL work in the app alone. They SHALL make no call to the daemon, SHALL ask no password and SHALL work while the daemon is offline.

#### Scenario: Daemon offline
- **WHEN** the daemon is offline and the user loads a View and adds a Note
- **THEN** the chart shows the stored rows of the View and the new Note
