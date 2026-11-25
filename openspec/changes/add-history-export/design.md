# Design

## Context

This design belongs to the change of `proposal.md`. `GLOSSARY.md` defines the project terms. The specs of this change hold the texts and the numbers: `specs/sensor-history/spec.md` and `specs/history-chart/spec.md`. This document gives the decisions and their reasons.

`add-nicecooling-v1` gives the project two processes. The root daemon `nicecoolingd` owns the hardware. The desktop app runs in the user session and owns the history (ADR-0005).

The current history, in `crates/nicecooling/src/history/` and `docs/storage.md`:

- The file is `$XDG_DATA_HOME/nicecooling/history.db`, SQLite in WAL mode, schema version 1, with the tables `meta`, `series`, `samples` and `minutes`.
- One service thread (`spawn_history` in `crates/nicecooling/src/services.rs`) records every sensor and every Channel: one transaction every 10 s, and a prune once per hour.
- The two periods are constants: 48 hours for the 1 s samples and 365 days for the 1 minute rows.
- The History screen, the Overview and the Leakshield Overview read the file with their own connections.
- The chart setup of the History screen lives in memory only (`HistoryUi`).

The owner decisions that bind this design:

1. The app records every sensor. A switch per sensor stops the recording, and the two periods become settings with fixed choices.
2. The export writes the shown range and the shown series to CSV: comma, decimal point, ISO 8601 time with the UTC offset, UTF-8, one header row "name (unit)", one row per time stamp, one column per series. Samples for a range inside the sample period, minute rows for older ranges, with the minimum and the maximum as an option.
3. No automatic export to a file.
4. Delete of a time range for the shown series or for all series, and of the whole history, each with a confirmation that names the number of rows.
5. The rich chart manager: saved setups, axis limits per unit, a color, a line width and a fill per series, notes on the chart, and the chart as an image.
6. The app records, never the daemon (ADR-0005).

The standing owner rules also hold: no action asks a password, the app warns and never refuses a choice of the user, the design has no defense against other local users, and nothing new runs in the root daemon.

## Goals / Non-Goals

**Goals:**

- A user takes the data of a chart out as a file that every tool reads without a question about the language.
- A user removes a bad test run from the history, and gets the disk space of a large delete back.
- A user chooses how long the data stay, and sees the disk cost of each choice before it applies.
- A user keeps a chart setup under a name and marks a time with a text.
- Every function works while the daemon is offline.
- The chart and the export show samples only where the store still holds them, also after a change of a period.

**Non-Goals:**

- Recording while no session runs the app. The history keeps its gaps (ADR-0005).
- Automatic export, a file that the app appends to, and a value file for other programs (owner decision 3).
- Sets of sensors with their own interval or their own period. The app records every sensor at 1 s.
- An import of a CSV file into the history.
- An undo of a delete.
- A sensor in the picker that the daemon no longer reports. Its rows stay reachable through a View, and they leave with the retention or with a delete for all series.
- A free position for a Note. A Note has a time and no value.
- A horizontal line at a value. The axis limits and the Alarm markers cover the need.
- A change of the Overview charts.

## Terms

`GLOSSARY.md` gets these entries with this change, in a new section "History". The specs and the interface use them.

| Term | Meaning |
|---|---|
| Series | The recorded values of one sensor or one Channel. |
| Sample | One value of a Series at one second. |
| Minute row | The mean, the minimum and the maximum of the Samples of one Series in one minute. |
| Sample period | The time that the Samples stay in the history: 24 hours, 48 hours, 7 days or 30 days. |
| Minute period | The time that the Minute rows stay in the history: 30 days, 90 days, 1 year, 2 years or 5 years. |
| View | A saved chart setup of the History screen: a name, the Series, the range length, the style of each Series and the axis limits per unit. It holds no data. |
| Note | A text that the user puts at a time of the history. Every chart that shows that time draws it. |

The glossary keeps Profile for a set of Curves, so a saved chart setup is a View and not a "chart profile". The entry View lists "chart profile", "preset" and "layout" under "Avoid". The entry Note lists "annotation" and "marker" under "Avoid", because a marker in the chart is an Alarm or a Warning. The interface text for a sensor of the list of H1 is "Not recorded".

## Decisions

### H1. The recording switch is a list of ids that the app does not record (sensor-history)

The app keeps the rule "record every sensor and every Channel". `settings.toml` gets the list `excluded` in a new table `[history]`. The writer skips a sample whose id is in the list. A sensor that appears later is recorded, because it is not in the list.

The switch is in one place: the dialog "Recorded sensors" of the Settings screen. It lists every sensor and every Channel with a search field, as the sensor picker does. The stored rows of a sensor stay when its switch goes off. The dialog says that the History screen deletes them.

Reason for a list of exclusions: the default must stay "everything", also for a sensor that a new device adds. A list of recorded ids would stop that.

Reason for the Settings screen: the history records Channels too, and the Sensors screen lists no Channel. One dialog covers both kinds.

Rejected: a button in each row of the Sensors screen. It covers no Channel, so a second place would be necessary.

Rejected: a delete of the stored rows when the switch goes off. A switch that deletes data needs a confirmation with a count, and the range delete of H8 does that already.

### H2. The two periods are fixed choices in the app settings (sensor-history)

`[history]` of `settings.toml` holds `sample_period` (`"24h"`, `"48h"`, `"7d"`, `"30d"`) and `minute_period` (`"30d"`, `"90d"`, `"1y"`, `"2y"`, `"5y"`). The defaults are the current constants: `"48h"` and `"1y"`. Every minute period is as long as the longest sample period or longer, so the two settings need no rule between them.

The loader reads each of the two values as text. A text that is not a choice gives the default of that key. The current loader gives the defaults for the whole file when one field does not parse, and a wrong period must not reset the language.

The Settings screen shows an estimate next to each choice. The numbers are 23 bytes per Sample (measured: 238 MB for 10 368 000 rows, `docs/app.md`) and 44 bytes per Minute row (computed from the column sizes; task 2.6 measures it). The number of Series is the number of sensors and Channels of the last daemon snapshot that are not in the list of H1.

| Choice | 60 Series |
|---|---|
| Samples, 24 hours | about 119 MB |
| Samples, 48 hours | about 238 MB |
| Samples, 7 days | about 835 MB |
| Samples, 30 days | about 3.6 GB |
| Minute rows, 30 days | about 114 MB |
| Minute rows, 90 days | about 342 MB |
| Minute rows, 1 year | about 1.4 GB |
| Minute rows, 2 years | about 2.8 GB |
| Minute rows, 5 years | about 6.9 GB |

An estimate above 2 GB has the Warning icon. The app applies every choice (owner rule: warn, never refuse).

Rejected: a free number of hours or days. The owner asked for a few fixed choices, and a free number needs range checks and its own texts.

Rejected: the choice "forever" for the minute period. A store with no upper size has no estimate to show.

### H3. A shorter period is a delete with a confirmation (sensor-history)

A change to a shorter period removes rows at the next prune. The app counts these rows first and shows the count in a confirmation, as for every other delete (owner decision 4). After the confirmation the app writes the setting and sends a prune request to the history service, which starts it within 2 s. A change that removes no row, and a change to a longer period, apply with no question. A longer period restores nothing: the store grows from that time on.

Rejected: apply the setting and let the hourly prune delete the rows. The user would lose up to 29 days of Samples with no count and no question.

### H4. A window draws Samples when the store still holds them (sensor-history)

The current rule compares the start of the window with the constant of 48 hours. With a setting, that rule fails after a change to a longer period: the window is inside the new period, and the Samples of that time left with the old period.

The new rule reads the data. The store holds the Samples of a range when no shown Series has, in the range, a Minute row that is complete before its first Sample of the range. The check costs two index reads per Series.

- A chart window draws Samples when it is 48 hours long or shorter and the store holds its Samples. Every other window draws Minute rows.
- The limit of 48 hours keeps a window at 172 800 points per Series or fewer, as now. A long sample period lets the user zoom into any 48 hours of the last 30 days at 1 s.
- The export uses the same check with no limit of 48 hours (H7).
- One window has one kind for all its Series. The gap logic and the hint of the screen depend on that.

`Resolution::of_window` and the two period constants leave `crates/nicecooling/src/history/`. The Overview charts call `load_series` and get the new rule with it.

Rejected: a stored time "Samples are complete from here" in the table `meta`. An older app version prunes at 48 hours and does not update that time, so the value is wrong after a downgrade and a second upgrade.

Rejected: Samples for every window inside the sample period. A window of 30 days would read 2 592 000 rows per Series for each redraw.

### H5. One writer: the history service thread (sensor-history, history-chart)

Every write to `history.db` runs in the service thread that records the Samples: the batch, the prune, the delete, the Notes and the compaction. The window sends a request on a channel and gets one reply with the result. The thread looks at the channel after each wait of `next_event`, so a request starts within 1 s.

Every read runs on a read-only connection in a background task of the window: the chart, the counts of a confirmation, the export and the Notes of a window. WAL mode lets these read while the thread writes.

The thread also owns the Samples that are not yet in the store. A delete must drop them (H8), and only the thread can.

The window runs one history operation at a time: an export, a delete, a prune after a change of a period or a compaction. The commands that start a second one are off with a text. Reason: an export holds a read position in the WAL file, and a compaction cannot give the space back while it does.

`HistoryDb::open` sets a busy timeout of 5 s. The service thread and a reader of the window both open the file at the start of the app, and the migration of H11 is a write.

Rejected: a second write connection in the window for the Notes and the deletes. Two writers need a retry rule for a busy database, and the Samples in memory would come back after a delete.

### H6. The CSV format has no setting (sensor-history)

The format is the one of owner decision 2. The spec fixes what the decision leaves open:

- No byte order mark. Each row ends with one line feed.
- Quotes as RFC 4180 has them: a field with a comma, a double quote or a line break is in double quotes.
- The header field of a Series is its name and its unit text in round brackets. The unit text is the one that the interface shows (`°C`, not `degC`). Two shown Series with the same name and unit get their ids in square brackets, so no two columns have the same header.
- The time is the local time with the offset that holds at that instant. A row in the hour that a clock change repeats is so still unique.
- The time of a Minute row is the start of its minute. The chart draws the same row in the middle of the minute.
- A number has 3 decimals at most and no trailing zero. The kernel gives a temperature and a voltage in thousandths, so 3 decimals keep these Samples exact. They also remove the binary rounding noise of the scale factor.
- A row exists for a time stamp when one exported Series or more has a value. A gap is a missing row.

The function that writes the time takes the offset of an instant from its caller. The app gives it the offset of the system time zone, and a test gives it a table.

Rejected: a byte order mark. Some spreadsheet programs need it to show `°` right, and many parsers then read the first header field as a different name. The file is for every tool.

Rejected: one row per value with the columns time, sensor and value. The owner chose one column per Series.

### H7. The export picks the data kind, shows the cost and streams to a temporary file (sensor-history)

The export dialog of the app shows the range, the number of Series, the data kind, the highest possible number of rows and an estimate of the size. The row number is the length of the range in seconds or in minutes plus one. The estimate is 26 bytes for the time field and the line end, plus 9 bytes per value column.

The kind follows owner decision 2 through the check of H4: Samples when the store holds them for the range, else Minute rows. When the store holds the Samples, the dialog also offers Minute rows. A range of 30 days at 1 s is 2 592 001 rows, and the user who wants an overview must be able to take 43 201 rows.

After "Export" the file dialog of the system asks for the file. A background task reads one cursor per Series, merges them by time and writes through a buffer to `<name>.part` in the same directory. It renames the file at the end. A cancel or an error removes the temporary file, so a file with the chosen name is always complete. The memory of an export does not grow with its size.

The app sets no upper size. It shows the Warning icon above 100 MB.

Rejected: an exact row count before the export. It reads every row of the range once more. The dialog says "up to", and the message at the end gives the exact number.

### H8. A range delete removes Samples and the Minute rows that overlap (sensor-history)

The range is the shown window. The scope is "Shown series" or "All series". The delete removes the Samples with a time in the range and each Minute row whose minute overlaps the range. The Minute row of a minute that is only partly in the range goes too, because its mean, minimum and maximum hold the Samples that the user deletes. The dialog says so.

The service thread runs the delete (H5). It first drops, from its memory, the Samples of the range that are not yet in the store and the minute sums that overlap the range. A range that ends at the present time then leaves nothing that a later batch writes back.

The confirmation counts on a read-only connection: one count per Series and table. The button "Delete" is off until the count is there, and when it is 0.

A delete removes no Note and no View. A Note is text of the user, not recorded data.

Rejected: keep the Minute rows at the two edges. The chart of 7 days would still show the peak of the deleted test run.

Rejected: compute the edge Minute rows again from the Samples that stay. The Samples exist only inside the sample period, so the result would depend on the age of the range.

### H9. The whole history is deleted in the Settings screen (sensor-history)

"Delete the whole history" empties `samples`, `minutes` and `series` in one transaction. The service thread also drops its Samples in memory and its table of Series ids, because the ids start again. The confirmation names the Samples, the Minute rows and the Series. It has the switch "Also delete the Notes", off by default. The Views stay: they are settings in another file.

The button is in the Settings screen, next to the periods and the size on disk, and not in the menu of the chart. A click next to "Export to CSV" must not empty the store.

Rejected: delete the three database files. The Overview keeps a connection open and would read the removed file until the app restarts.

### H10. A large delete compacts the file (sensor-history)

SQLite keeps the pages of deleted rows in the file and uses them for new rows. After a change from 30 days to 48 hours, about 3.3 GB would stay in the file with no use. So the service thread compacts the file (`VACUUM`, then a WAL checkpoint that truncates) after an operation that removed 1 000 000 rows or more. This is about 23 MB of Samples. The hourly prune of the steady state removes 216 000 Samples for 60 Series and never reaches the number.

The thread records nothing while it compacts, so the history has a gap of that length. Task 3.6 measures the time for the database of the size test and writes it to `docs/app.md`.

A compaction that fails changes no row. The app shows the error, and the next large delete tries again.

Rejected: a button "Compact". The user would have to know that a delete gives no space back.

Rejected: the incremental vacuum mode of SQLite. A database that exists needs one full `VACUUM` to get the mode, and every delete then pays for the page moves.

### H11. Schema version 2 adds the Notes, and a newer file is not written (sensor-history)

`HistoryDb::open` reads `schema_version` before it changes anything.

- A new file gets version 2.
- A version 1 file gets the table `notes`, its index and version 2 in one transaction. No row of `samples`, `minutes` or `series` changes.
- A version above 2 gives an error. The app writes nothing and records nothing, and the History screen shows the database error state with its own text. This is the rule that `docs/storage.md` has for the JSON files.

### H12. A View is a chart setup in its own file (history-chart)

The file is `$XDG_CONFIG_HOME/nicecooling/history-views.toml`. A View is a setting of the user, so it is in the configuration directory, and `history.db` can be removed without it. The format is TOML, which the app reads and writes already.

A View holds its id, its name, the range length, the Series in their order (sensor id, the name at the time of the save, color, line width, fill, hidden) and the fixed axis limits per unit. It holds no time position and no zoom: a View opens at the present time. The file also holds the id of the last loaded View and the counter of the ids.

The window writes the file with the atomic write of `docs/storage.md`. It writes at "Save view", "Save view as", "Rename", "Delete view" and when the loaded View changes.

A chart setup that is not saved stays in memory, as now.

Rejected: the Views in `settings.toml`. The loader of that file gives the defaults for the whole file when one value does not parse. A damaged View would reset the language and the notification switches.

Rejected: the Views in `history.db`. A user who removes the database to start again would lose the chart setups.

### H13. The style and the axis limits belong to the chart setup (history-chart)

Each Series of the chart has a color, a line width of 1 to 4 px and a fill switch. Each unit has an automatic axis or a fixed minimum and maximum. A View stores them with its Series. A second View can so show the same sensor in another color.

- The default color is the series color of the position, as now. A color is a token (`s1` to `s6`) or a custom `#RRGGBB`. A token follows the theme. A custom color is the same in both themes.
- `docs/ui/DESIGN.md` keeps red and amber for Alarms and Warnings. A custom color in that range gets a caption and applies (owner rule: warn, never refuse).
- The fill goes from the line to the lower edge of the plot at 10% opacity. The band of a Minute row stays at 20%, so both read on one chart.
- Fixed limits cut a line at the edge of the plot. The legend row says that values are outside, and its minimum and maximum still come from all values.

Rejected: one style per sensor for every chart. The style would need a place and a cleanup for a sensor that no View holds.

Rejected: a line width up to 8 px. The chart is 340 px high, and a line of 4 px is 1.2% of it already.

### H14. A Note is a row of the history database (history-chart)

A Note has a time and a text of 1 to 120 characters. It is a row of the table `notes` of `history.db`. Every chart that shows that time draws it, with every View and every set of Series.

Reason: a Note marks an event of the machine, for example "Fan curve changed". The event is true for every sensor. The row is in the file of the data that it comments, and a query by time finds the Notes of a window.

The user adds a Note with a secondary click in the chart or with the key N at the cursor. A dialog "Notes" lists all of them and moves the window to one. The limit is 500 Notes.

A range delete keeps the Notes (H8). The delete of the whole history keeps them unless the switch is on (H9).

Rejected: Notes as a part of a View. A Note would show in one chart only, and it would leave with its View.

Rejected: a Note with a time and a value at a free place of the plot. With two axes and several charts, a value has no single place.

### H15. The image is an SVG document that `resvg` turns into a PNG (history-chart)

A pure function builds one SVG document from the model of the screen, the palette and the size. `resvg` draws it into 1920 x 1080 px, and its pixel type writes the PNG. `gpui-pre` 0.3.12 builds `resvg` 0.46.0 with the features `text` and `system-fonts` already, so the direct dependency adds no crate to the lock file.

- The size is fixed at 1920 x 1080 px. The colors are those of the theme that the app shows.
- The image holds the title, the range, the charts and the legend. It has no cursor and no control.
- The text asks for the font list `Adwaita Sans, Inter, sans-serif`. The font database loads the fonts of the system, so the image does not fail without the interface font.
- The file write follows H7: a temporary file, then a rename.

Reason for a second drawing path: `gpui-pre` 0.3.12 reads the pixels of a window back only in its test feature, and its Linux platform crate has no such function. The SVG path also has no dependence on the size of the window, and a test can read the document as text.

Cost: the chart has two painters, the one of the screen and the one of the image. Task 9.2 compares them by hand.

Rejected: an SVG file as the output. The text of an SVG file depends on the fonts of the reader's machine.

Rejected: a choice of the size and of the theme in a dialog. One fixed size needs no dialog, and it can come later with no change of the format.

### H16. Places in the interface (sensor-history, history-chart)

- History toolbar: the View button after the sensor picker. It shows the name of the loaded View and opens a menu with the Views and the commands "Save view", "Save view as", "Rename view" and "Delete view".
- History toolbar: the menu button "Chart" at the end, with "Notes", "Axis limits", "Export to CSV", "Save as image" and "Delete data".
- Legend row: a click on the color mark opens the style popover. A click on the rest of the row hides or shows the Series, as now.
- Chart: a secondary click opens "Add note here". The key N does the same at the cursor. A click on the icon of a Note opens the Note.
- Settings: a new card "History" with the two periods and their estimates, "Recorded sensors", "Size on disk" and "Delete the whole history".

The toolbar follows the rule of `docs/ui/DESIGN.md`: one menu button holds the commands that are not navigation. Tasks 4.1 and 4.2 write the screen files, and the owner approves them before a screen task starts.

### H17. A View keeps a sensor that the daemon does not report (history-chart)

A device can be away for a day. A View that drops its sensor at the load would lose it for good. So the Series stays, with the name that the View stored, the tag "Sensor not found" and the rows that the store still holds. When a sensor with the same id returns, the Series is live again. The tag shows only while the daemon is connected, because an offline daemon reports no sensor at all.

Rejected: remove the Series at the load. Rejected: a question at each load. The user removes the Series from the chart with one click when the sensor is gone for good.

## Storage

`docs/storage.md` gets these changes with task 1.1, before the code that writes the files.

### `history.db`, schema version 2

```sql
CREATE TABLE notes (id INTEGER PRIMARY KEY, t INTEGER NOT NULL, text TEXT NOT NULL);  -- t: Unix s
CREATE INDEX notes_t ON notes (t);
-- meta: schema_version = 2
```

The tables `meta`, `series`, `samples` and `minutes` keep their columns.

### `settings.toml`, new table

```toml
[history]
sample_period = "48h"   # "24h", "48h", "7d" or "30d"
minute_period = "1y"    # "30d", "90d", "1y", "2y" or "5y"
excluded = []           # ids of the sensors and Channels that the app does not record
```

`schema_version` stays 1. A file without the table loads with these defaults.

### `history-views.toml`, new file

```toml
schema_version = 1
last_view = "view_1"    # absent when no View is loaded
next_id = 2

[[views]]
id = "view_1"
name = "Loop temperatures"
range = "24h"           # "1h", "24h", "48h", "7d", "30d" or "1y"

[[views.series]]
sensor = "leakshield_1_water_temp"
name = "Water temperature"   # the name at the time of the save
color = "s1"            # "s1" to "s6", or "#RRGGBB"
width = 2               # 1 to 4
fill = false
hidden = false

[[views.axes]]
unit = "degC"           # a unit with no entry has an automatic axis
min = 20.0
max = 45.0
```

The app is the only writer. The id of a View is `view_<n>` from `next_id`, and the app never uses an id again. The app does not read a file above 1 MiB.

## Limits

| Limit | Number | Reason |
|---|---|---|
| Sample period | 24 hours, 48 hours, 7 days, 30 days | H2 |
| Minute period | 30 days, 90 days, 1 year, 2 years, 5 years | H2 |
| Largest store, 60 Series | about 3.6 GB of Samples and 6.9 GB of Minute rows | The longest choices of H2. The app warns above 2 GB per period |
| Default store, 60 Series | about 238 MB of Samples and 1.4 GB of Minute rows | 48 hours and 1 year |
| Chart window with Samples | 48 hours, 172 800 points per Series | H4 |
| Rows of one export | 2 592 001 Samples (30 days), 525 601 Minute rows (1 year) | The longest window of each kind |
| Size of one export | No upper limit. Warning icon above 100 MB. 60 Series for 30 days at 1 s are about 1.5 GB | H7 |
| History operations at one time | 1 | H5 |
| Start of a request in the service thread | 1 s | H5 |
| Compaction | After 1 000 000 removed rows or more in one operation | H10 |
| Views | 32, with 32 Series each | The file stays near 200 KB |
| Name of a View | 1 to 48 characters | It fits the toolbar button |
| Views file that the app reads | 1 MiB | A larger file is not a file of the app |
| Notes | 500 | The list stays readable, and the table stays below 300 KB |
| Text of a Note | 1 to 120 characters | One line in the list |
| Line width | 1 to 4 px | H13 |
| Axis limit | -1 000 000 to 1 000 000 | Every unit of the app fits |
| Image | 1920 x 1080 px, 8 294 400 bytes before the PNG compression | H15 |
| Note text in the image | 40 characters, then "..." | The label stays inside its chart |

## Requirements of add-nicecooling-v1 that this change extends

`add-nicecooling-v1` is not archived, so this change has no delta against `openspec/specs/`. It extends this contract, and task 8.4 edits it:

| Capability | Change |
|---|---|
| `desktop-app` | The requirement "Sensor history": 48 hours and one year are the defaults of two settings, and a sensor of the list of H1 is not recorded. The scenario "Old samples" holds for the defaults. |

Files of the contract that change: `docs/storage.md`, `docs/app.md`, `docs/ui/screens/history.md`, `docs/ui/screens/settings.md`, `docs/ui/DESIGN.md`, `docs/ui/components.md`, `docs/i18n-terms.md` and `GLOSSARY.md`. `docs/api.md` does not change.

## Migration and rollback

- Forward: the first start of the new app brings `history.db` to version 2 (H11). `settings.toml` gets the table `[history]` at the first change of a setting. `history-views.toml` appears at the first saved View. No stored row is converted.
- Back: an older app opens a version 2 database. It does not read the version, it ignores the table `notes`, and it records as before. Its prune uses 48 hours and 365 days again, at its first run after the start. A user with a longer period loses the rows beyond these two times then. The older app ignores the table `[history]` and removes it at its next save of the settings. It never reads `history-views.toml`.
- Before a downgrade: export what must stay, or copy `history.db`. `docs/app.md` has these steps in a section "Downgrade".
- Forward again: the Notes and the Views are still there. The two periods and the list of H1 are the defaults when the older app saved its settings. The check of H4 makes the chart right for the rows that the older app removed.

Task 1.7 proves the two "back" statements with the old loader and the old schema text as test fixtures.

## Risks / Trade-offs

- [The estimate of 44 bytes per Minute row is computed, not measured] -> task 2.6 measures it with the size test and corrects the number in the code, in `docs/storage.md` and in the specs before the Settings card is built.
- [One year of Minute rows for 60 Series is about 1.4 GB, more than the 238 MB of the Samples] -> the Settings card shows both estimates and the size on disk, and the minute period has two shorter choices.
- [A compaction stops the recording for its length] -> it runs only after a delete of 1 000 000 rows or more, which the user started and confirmed. Task 3.6 measures the time.
- [A compaction needs free disk space of about the size of the rows that stay] -> a failed compaction changes no row and shows the error.
- [A delete cannot be undone] -> every delete has a confirmation with the exact count, the destructive button and the text "You cannot undo this."
- [A downgrade prunes at 48 hours and 365 days] -> the section "Downgrade" of `docs/app.md` says it and gives the steps.
- [The image has its own painter and can differ from the screen] -> the SVG tests check each part of the model, and the manual check of task 9.2 compares both.
- [The font of the image depends on the fonts of the system] -> the font list ends with `sans-serif`, and the tests do not compare text pixels.
- [A custom color can be red or amber, against the series rule of `docs/ui/DESIGN.md`] -> the caption warns, and the default colors stay the six tokens.
- [An export of 1.5 GB takes minutes] -> the dialog shows the size before the start, the export has a progress bar and "Cancel", and the recording goes on.
- [Another local user can read an exported file with open permissions] -> the file has the default permissions of the user, as every file that the user saves. No defense against other local users is a standing owner rule.

## Product choices that the owner can change

Each row is a choice that the owner decisions do not close. The specs hold the recommended option.

| Choice | In the specs | Alternative |
|---|---|---|
| Sample period choices | 24 hours, 48 hours, 7 days, 30 days | A shorter list without 30 days, which costs 3.6 GB for 60 Series |
| Minute period choices | 30 days, 90 days, 1 year, 2 years, 5 years | Add "forever", or stop at 2 years |
| Place of the recording switch | One dialog in the Settings screen, for sensors and Channels | A button in each row of the Sensors screen, which covers no Channel |
| Rows of a sensor whose switch goes off | They stay until a delete or the retention | Delete them with the switch, after a confirmation |
| Data kind choice in the export | The dialog also offers Minute rows when Samples exist | No choice: always the kind of owner decision 2 |
| Decimals in the CSV file | 3 at most, no trailing zero | The full stored number, up to 17 digits |
| Byte order mark and line end | None, and one line feed | A byte order mark and CR LF for spreadsheet programs on other systems |
| Two Series with the same name | The id in square brackets after the name | The same header twice |
| Edge minutes of a range delete | A Minute row that overlaps the range is deleted whole | Keep the Minute rows that are only partly in the range |
| Notes at the delete of the whole history | They stay, with a switch to delete them | Always delete them |
| Disk space after a delete | Automatic compaction after 1 000 000 removed rows | A button "Compact", or no compaction |
| Place of the style and the axis limits | In the View | One style per sensor for every chart |
| Colors of a Series | Six tokens or a custom `#RRGGBB`, with a caption for red and amber | The six tokens only |
| Line width | 1 to 4 px | Up to 8 px |
| What a Note is | A time and a text, in `history.db`, drawn in every chart | A Note of one View, or a text at a free place of the plot |
| Limits | 32 Views, 32 Series per View, 500 Notes, 120 characters | Other numbers |
| View at the start of the app | The last loaded View | An empty History screen, as now |
| Sensor of a View that is not found | It stays with a tag and its stored rows | Remove it at the load |
| Image | PNG, 1920 x 1080 px, the theme of the app | SVG, a choice of size, or always the light theme |
| Image content | Title, range, charts, legend, Notes, Alarm markers | The plot only |

## Open Questions

None. The three numbers that tasks measure (the bytes of a Minute row, the time of a compaction, the look of the image) change no approach.
