# Design

## Context

This design belongs to the change of `proposal.md`. The specs of this change hold the texts and the numbers: `specs/sensor-history/spec.md` and `specs/history-chart/spec.md`. This document gives the decisions and their reasons. `tasks.md` holds the work steps. A task number with no change name points to it.

`GLOSSARY.md` defines the project terms. This design uses these:

- A Channel is one fan or pump output of the Board, with its speed reading and its Duty.
- An Alarm and a Warning are events that the daemon reports. The chart marks their times.
- A Derived sensor is a sensor that the daemon computes from other sensors.
- The Leakshield is the pressure device on the reservoir. It has its own Overview screen.

`add-nicecooling-v1` gives the project two processes. The root daemon `nicecoolingd` owns the hardware. The desktop app runs in the user session and owns the history (ADR-0005).

The current history, in `crates/nicecooling/src/history/` and `docs/storage.md`:

- The file is `$XDG_DATA_HOME/nicecooling/history.db`, SQLite in WAL mode, schema version 1, with the tables `meta`, `series`, `samples` and `minutes`.
- The daemon sends one sensor event per second. One event holds the time and the values of all sensors and all Channels. For a Channel, the app records the Duty in percent.
- The app passes each daemon event to the app window and to each service thread through one event channel of 1024 events. Each reader has its own position, so a slow thread does not delay the screens. A reader that is more than 1024 events late loses the oldest events.
- One service thread (`spawn_history` in `crates/nicecooling/src/services.rs`) records every sensor and every Channel. It keeps the Samples of up to 10 s and the sums of the minutes in progress in memory.
- The service thread writes one transaction every 10 s, with the Minute row of each minute that the clock has passed. It prunes once per hour.
- The two periods are constants: 48 hours for the Samples and 365 days for the Minute rows.
- The History screen, the Overview, the Leakshield Overview and the sparkline of the Sensors screen read the file with their own connections.
- The chart setup of the History screen is in memory only (`HistoryUi`).
- Nothing stops a second app process of the same user, for example in a second session. Both processes then record the same Samples to the same file.

The owner decisions that bind this design are in [issue #89](https://github.com/CappyT/nicecooling/issues/89), "Owner decisions of 2026-11-25":

1. The app records every sensor, as now, with a switch per sensor that stops its recording, and with settings for the two periods. The Sample period choices are 24 hours, 48 hours, 7 days and 30 days. The Minute period choices are 30 days, 90 days, 1 year, 2 years and 5 years. The defaults stay 48 hours and 1 year.
2. The export writes the shown range and the shown Series to CSV: comma, decimal point, ISO 8601 time with the UTC offset, UTF-8, one header row.
3. No automatic export to a file.
4. Delete of a time range, for the shown Series or for all Series, and of the whole history. Each delete has a confirmation that names the number of rows.
5. The rich chart manager: saved Views, axis limits per unit, a color, a line width and a fill per Series, Notes on the chart, and "Save as image".
6. "Save as image" gives one PNG of 1920 x 1080 px in the theme that the app shows, with the title, the range, the charts, the legend, the Notes and the Alarm markers. It has no dialog of options.
7. The app records, never the daemon (ADR-0005).

The standing owner rules of ADR-0007 also hold. No action asks a password. The app warns and never refuses a valid choice of the user. The design has no defense against other local users. Nothing new runs in the root daemon.

## Goals / Non-Goals

**Goals:**

- A user exports the data of a chart to a file with one fixed format. The format does not depend on a language setting.
- A user removes a bad test run from the store, and gets the disk space of a large delete back.
- A user chooses how long the data stay, and sees the disk cost of each choice before it applies.
- A user keeps a chart setup under a name and marks a time with a text.
- Every function works while the daemon is offline.
- The chart and the export show Samples only where the store still holds them, also after a change of a period.

**Non-Goals:**

- Recording while no session runs the app. The history keeps its gaps (ADR-0005).
- Automatic export, a file that the app appends to, and a value file for other programs (owner decision 3).
- Sets of sensors with their own interval or their own period. The app records every sensor at 1 s.
- An import of a CSV file into the store.
- An undo of a delete.
- A sensor in the picker that the daemon no longer reports. A View still shows its rows. The prune or a delete for all Series removes them.
- A free position for a Note. A Note has a time and no value.
- A horizontal line at a value. The axis limits and the Alarm markers show these levels.
- A change of the Overview charts, other than the way that they read the store: the reader open (H5) and the data kind of H4.

## Terms

`GLOSSARY.md` gets these entries with this change, in a new section "History". The specs and the interface use them.

| Term | Meaning |
|---|---|
| Series | The recorded values of one sensor or one Channel. |
| Sample | One value of a Series at one second. |
| Minute row | The mean, the minimum and the maximum of the Samples of one Series in one minute. |
| Sample period | The time that the Samples stay in the store: 24 hours, 48 hours, 7 days or 30 days. |
| Minute period | The time that the Minute rows stay in the store: 30 days, 90 days, 1 year, 2 years or 5 years. |
| View | A saved chart setup of the History screen: a name, the Series, the range length, the style of each Series and the axis limits per unit. It holds no data. |
| Note | A text that the user puts at a time of the history. Every chart of the History screen that shows that time draws it. |

These words have one meaning in this change:

- The store is the recorded data. `history.db` is its file.
- The shown Series are the Series of the chart that the legend does not hide. The export and the delete scope "Shown series" use them.
- A window is the time range that a chart shows. The app window is the window of the app with its screens.
- The event channel carries the daemon events. The request channel carries the requests of the app window to the service thread. Neither is a Channel.
- The service thread is the thread that records the Samples.
- A snapshot is the full state that the app reads from the daemon at each connect: the sensors, the Channels, their names and their units. The app keeps the last snapshot while the daemon is offline.
- The list of not recorded sensors holds the ids that the app does not record. Its key in the settings file is `excluded`.
- A series color is one of the six colors `s1` to `s6` of `docs/ui/DESIGN.md`.

The glossary keeps Profile for a set of Curves, so a saved chart setup is a View and not a "chart profile". The entry View lists "chart profile", "preset" and "layout" under "Avoid". The entry Note lists "annotation" and "marker" under "Avoid", because a marker in the chart is an Alarm or a Warning. The interface text for a sensor of the list of not recorded sensors is "Not recorded".

## Decisions

### H1. The recording switch is a list of ids that the app does not record (sensor-history)

The app keeps the rule "record every sensor and every Channel". `settings.toml` gets the list `excluded` in a new table `[history]`: the list of not recorded sensors. The service thread skips a value whose id is in the list. A sensor that appears later is recorded, because it is not in the list. A change of the list applies from the next event that the service thread reads.

The switch is in one place: the dialog "Recorded sensors" of the Settings screen. It lists every sensor and every Channel of the snapshot with a search field, as the sensor picker does. It also lists each id of the list that the snapshot does not hold. Such a row has the id as its name. The user can then turn the recording on again while the device is away.

The app keeps the last snapshot while the daemon is offline, so the dialog works then. An app that started with the daemon offline has no snapshot. The dialog then lists the ids of the list only and says that the daemon is offline.

The stored rows of a sensor stay when its switch goes off. The dialog says that the user can delete them in the History screen. The row "Recorded sensors" of the card shows the two numbers, for example "61 of 62 recorded".

Only the History screen names a sensor that is not recorded. The charts of the Overview and of the Leakshield Overview draw the stored rows of such a sensor and then nothing, with no label.

Reason for a list of not recorded ids: the default must stay "everything", also for a sensor that a new device adds. A list of recorded ids would stop that.

Reason for the Settings screen: the history records Channels too, and the Sensors screen lists no Channel. One dialog shows both kinds.

Rejected: a button in each row of the Sensors screen. It shows no Channel, so a second place would be necessary.

Rejected: the list of hidden sensors of the Sensors screen (`[sensors] hidden`) as the list of not recorded sensors. A user hides a sensor to shorten a list and can still want its history. That list also holds no Channel.

Rejected: a delete of the stored rows when the switch goes off. A switch that deletes data needs a confirmation with a count, and the range delete of H8 does that already.

### H2. The two periods are fixed choices in the app settings (sensor-history)

`[history]` of `settings.toml` holds `sample_period` (`"24h"`, `"48h"`, `"7d"`, `"30d"`) and `minute_period` (`"30d"`, `"90d"`, `"1y"`, `"2y"`, `"5y"`). The defaults are the current constants: `"48h"` and `"1y"`. A year is 365 days: `"1y"` is 365 days, `"2y"` is 730 days and `"5y"` is 1 825 days. Every Minute period is as long as the longest Sample period or longer, so the two settings need no rule between them.

The loader reads each of the two values as text. A text that is not a choice gives the default of that key. Reason: the current loader gives the defaults for the whole file when one field does not parse. A wrong period must not reset the language.

The Settings screen shows an estimate next to each choice. The numbers are 23 bytes per Sample and 44 bytes per Minute row.

- A Sample is a cell of 20 bytes in its table and takes 23 bytes on disk. The size test measured 238 MB for 10 368 000 Samples (`docs/app.md`).
- A Minute row is a cell of 40 bytes: a record of 37 bytes, 1 byte for its size and 2 bytes for its pointer. The page overhead of the Samples gives 44 to 46 bytes on disk. Task 2.6 measures the number.
- A Series of whole numbers takes less: SQLite stores a whole number in 1 or 2 bytes, so a Duty or a speed needs fewer bytes than these two numbers.

The number of Series is the number of sensors and Channels of the snapshot that are not in the list of not recorded sensors. With no snapshot, the estimates show `--`.

A size text has the base 1000. It uses MB below 1 000 MB and GB from there, with one decimal below 10 and no decimal from 10 on. The row "Size on disk" shows the sum of `history.db` and its two WAL files. The app reads it when the Settings screen opens and after each history operation.

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

An estimate above 2 GB has the Warning icon. The app applies every choice (ADR-0007: warn, never refuse).

Rejected: a free number of hours or days. Owner decision 1 fixes the choices, and a free number needs range checks and its own texts.

Rejected: the choice "forever" for the Minute period. A store with no upper size has no estimate to show.

### H3. A shorter period is a delete with a confirmation (sensor-history)

A change to a shorter period removes rows at the next prune. The app counts these rows first and shows the count in a confirmation, as for every other delete (owner decision 4). The dialog shows a counting state until the count is there.

After the confirmation the app writes the setting and then sends a prune request to the service thread. The service thread looks at its requests once per second (H5), so the delete starts within 2 s. The request carries no period: the service thread reads the stored periods when it handles the request.

A change that removes no row, and a change to a longer period, apply with no question. A longer period restores nothing: the store grows from that time on.

Rejected: apply the setting and let the hourly prune delete the rows. The user would lose up to 29 days of Samples with no count and no question.

### H4. A window draws Samples when the store still holds them (sensor-history)

The current rule compares the start of the window with the constant of 48 hours. With a setting, that rule fails after a change to a longer period. The window is then inside the new period, and the Samples of that time left with the old period.

The new rule reads the data. The check is its own function. It takes a range and a set of Series and gives the data kind. `load_series` reads one Series and takes the kind from its caller. For each Series it reads the first Sample of the range and the first Minute row of the range: two index reads. The Samples of a Series are gone in two cases:

- The first Minute row of the range ends before the first Sample of the range.
- The range holds Minute rows of the Series and no Sample.

The store holds the Samples of a range when no Series of the set is in one of these cases.

- The chart gives the check all its Series, hidden or not. A click on a legend row then never changes the data kind.
- A window draws Samples when it is 48 hours long or shorter and the store holds its Samples. Every other window draws Minute rows.
- The limit of 48 hours keeps a window at 172 800 points per Series or fewer, as now. A long Sample period lets the user zoom into any 48 hours of the last 30 days at 1 s.
- The export gives the check the shown Series and has no limit of 48 hours (H7).
- One window has one kind for all its Series. Two parts of the screen depend on the kind. The first is the largest distance between two points that is not a gap: 10 s for Samples, 150 s for Minute rows. The second is the hint that the chart shows Minute rows.

`Resolution::of_window` and the two period constants are removed from `crates/nicecooling/src/history/`. The History screen calls the check once for all its Series. An Overview chart calls it for its one Series.

Rejected: a stored time "Samples are complete from here" in the table `meta`. An older app version prunes at 48 hours and does not update that time. The value is then wrong after a downgrade and a second upgrade.

Rejected: Samples for every window inside the Sample period. A window of 30 days would read 2 592 000 rows per Series for each redraw.

### H5. One writer: the service thread (sensor-history, history-chart)

Every write to `history.db` runs in the service thread: the batch, the prune, the delete, the Notes and the compaction. The app window sends a request on the request channel and gets one reply with the result. The service thread looks at the request channel after each wait of `next_event`. So a request starts within 1 s when the thread is not in a write.

`HistoryDb` has two open functions:

- The writer open creates the file, checks the schema version and migrates (H11). Only the service thread calls it.
- The reader open is read-only. It changes no schema and no row, and it gives the error of H11 for a version above 2. A file that does not exist reads as an empty store. A version 1 file reads as a store with no Notes.

Every read uses the reader open: the chart, the two Overview charts, the sparkline of the Sensors screen, the counts of a confirmation, the export and the Notes of a window. WAL mode lets these read while the service thread writes.

Reason for the two open functions: the current `HistoryDb::open` runs the schema statements at each open, and every chart read calls it. With one open function, a reader would migrate the file, against the rule of one writer.

The service thread gets the history settings from `SettingsStore`. `SettingsStore` shares the list of not recorded sensors and the two periods behind a mutex, as it shares the notification settings. The service thread reads them at each step: the list before it records, and the periods before a prune.

The service thread also owns the Samples that are not yet in the store. A delete must drop them (H8), and only the service thread can.

The app runs one history operation at a time. A history operation is an export, a delete, a prune after a change of a period or a compaction. The commands that start a second one are off with a text. The commands that add, change or delete a Note are off with the same text. Reason for the Notes: the service thread cannot write a Note while it deletes or compacts. An export does not stop a Note, but one state "operation running" for all operations is simpler than one state per kind. Reason for the rule: an export holds a read position in the WAL file, and a compaction cannot give the space back while it does.

Both open functions set a busy timeout of 5 s. Reason: a second app process of the same user writes the same file, and the checkpoint of a compaction waits for a reader.

"One writer" is a rule inside one app process. This design gives a second process of the same user no new link:

- A delete in one process does not reach the memory of the other process. The other process can write back the Samples of its last 10 s and the Minute row of its minute in progress.
- A change of a period or of the list of not recorded sensors reaches the other process at its next start. Until then, each process prunes with the periods that it loaded.
- `docs/app.md` says this.

Rejected: a second write connection in the app window for the Notes and the deletes. Two writers need a retry rule for a busy database, and the Samples in memory would come back after a delete.

### H6. The CSV format has no setting (sensor-history)

The format is the one of owner decision 2. The spec fixes what the decision leaves open:

- One row per time stamp and one column per Series.
- No byte order mark. Each row ends with one line feed.
- Quotes as RFC 4180 has them: a field with a comma, a double quote or a line break is in double quotes.
- The header field of a Series is the name that the legend shows and its unit text in round brackets. The unit text is the one that the interface shows (`°C`, not `degC`). Two shown Series with the same name and unit get their ids in square brackets, so no two columns have the same header.
- The time is the local time with the UTC offset of that time. The offset keeps each row unique in the hour that a clock change repeats.
- The time of a Minute row is the start of its minute. The chart draws the same row in the middle of the minute.
- A number has 3 decimals at most and no trailing zero. The kernel gives a temperature and a voltage in thousandths, so 3 decimals keep these Samples exact. The store holds each value as a floating point number after the division by 1000. Three decimals also remove the rounding error of that division.
- A row exists for a time stamp when one exported Series or more has a value. A gap is a missing row.
- With the option "Minimum and maximum", each Series has three columns: the mean with the header of the Series, then the minimum and the maximum. Their headers have ` min` and ` max` after the name.

The function that writes the time takes the offset of a time from its caller. The app gives it the offset of the system time zone, and a test gives it a table.

Rejected: a byte order mark. Some spreadsheet programs need it to show `°` right, and many parsers then read the first header field as a different name.

Rejected: one row per value with the columns time, sensor and value. It multiplies the rows by the number of Series, and a chart of the file then needs a pivot step.

### H7. The export picks the data kind, shows the cost and streams to a temporary file (sensor-history)

The export dialog of the app shows the range, the number of Series, the data kind, the highest possible number of rows and an estimate of the size. The highest number of rows is the length of the range in seconds or in minutes, plus one. The estimate is this number times the bytes of a row: 26 bytes for the time field and the line end, plus 9 bytes per value column. A Series is one value column, or three with the minimum and the maximum.

The kind comes from the check of H4: Samples when the store holds them for the range, else Minute rows. When the store holds the Samples, the dialog also offers Minute rows. A range of 30 days at 1 s is 2 592 001 rows, and a user who wants an overview can take 43 201 rows.

After "Export" the file dialog of the system asks for the file. A background task reads one cursor per Series, merges them by time and writes through a buffer to `<name>.part` in the same directory. It renames the file at the end. A cancel or an error removes the temporary file, so a file with the chosen name is always complete. The memory of an export does not grow with its size.

The app sets no upper size. It shows the Warning icon above 100 MB.

Rejected: an exact row count before the export. It reads every row of the range once more. The dialog says "up to", and the message at the end gives the exact number.

Rejected: the suffix `.tmp` and the atomic write function of the app settings. That function takes the whole content in memory, and `.part` is the usual name of a download that is not complete.

### H8. A range delete removes Samples and the Minute rows that overlap (sensor-history)

The range is the shown range. The scope is "Shown series" or "All series". The delete removes the Samples with a time in the range and each Minute row whose minute overlaps the range. The Minute row of a minute that is only partly in the range is deleted too. Its mean, minimum and maximum hold the Samples that the user deletes. The dialog says so.

The service thread runs the delete (H5). It first drops, from its memory, the Samples of the range that are not yet in the store and the minute sums that overlap the range.

It also keeps the range and the scope. It drops each later Sample of the scope with a time in the range. Reason: events of the range can still wait in the event channel (H18). It forgets the range when a Sample with a later time arrives. A range that ends at the present time then leaves nothing that a later batch writes back.

The minute in progress at the end of the range gets a new Minute row from the Samples after the range. That row holds only Samples that the user did not delete.

The confirmation counts with the reader open: one count per Series and table. The button "Delete" is off until the count is there, and when it is 0.

A delete removes no Note and no View.

Rejected: keep the Minute rows at the two edges. The chart of 7 days would still show the peak of the deleted test run.

Rejected: compute the edge Minute rows again from the Samples that stay. The Samples exist only inside the Sample period, so the result would depend on the age of the range.

### H9. The whole history is deleted in the Settings screen (sensor-history)

"Delete the whole history" empties `samples` and `minutes`. The table `series` keeps its rows, so a Series keeps its id. The service thread also drops its Samples in memory and its minute sums. It drops each Sample that arrives after the delete and has a time before it, as for a range delete (H8).

The confirmation names the number of Samples, of Minute rows and of Series that have rows. It has the switch "Also delete the <n> Notes", off by default. The Views stay: they are settings in another file.

The button is in the Settings screen, next to the periods and the size on disk, and not in the menu "Chart". A click next to "Export to CSV" must not empty the store.

Rejected: empty the table `series` too. The ids would start again, and a second app process would write its Samples under the ids that it loaded, which then belong to other sensors.

Rejected: delete the three database files. The Overview keeps a connection open and would read the removed file until the app restarts.

### H10. A large delete that the user confirmed compacts the file (sensor-history)

SQLite keeps the pages of deleted rows in the file and uses them for new rows. After a change from 30 days to 48 hours, about 3.3 GB would stay in the file with no use. So the service thread compacts the file after a confirmed delete that removes 1 000 000 rows or more. This is about 23 MB of Samples. Such a delete is a range delete, a delete of the whole history or a change to a shorter period. The compaction is a `VACUUM` and then a WAL checkpoint that truncates.

A prune of the schedule, at the start and once per hour, never compacts. Reason: the prune at the start removes more than 1 000 000 rows after about 5 hours without the app. A compaction at each such start would write the whole store again, and the Samples of the next hours fill the free pages.

The compaction writes every row that stays, first to a copy and then through the WAL file. The SQLite that the app builds keeps this copy in a file.

- The copy must be on the filesystem of the store and not in memory. The app sets the temporary directory of SQLite (`SQLITE_TMPDIR`) to the directory of `history.db`. It does this first in `main`, before the runtime and the service threads start, because a change of the environment is safe only with one thread.
- The compaction needs free disk space of up to twice the size of the rows that stay. The service thread reads the free space first, with `statvfs` of `rustix`, which the workspace has already. With less than twice the size of the pages in use (`page_count` minus `freelist_count`), it does not compact and reports the reason.
- The writer open sets a size limit of 64 MiB for the WAL file (`journal_size_limit`). So the WAL file of a large delete or of a compaction does not keep its size.
- A reader can hold the WAL file longer than the busy timeout. The checkpoint then ends at a later write, and the row "Size on disk" shows the real size until then.

The Samples of the compaction time wait in the event channel (H18). Task 3.6 measures the time for the database of the size test and writes it to `docs/app.md`.

A compaction that fails changes no row. The app shows the error, and the next large delete tries again.

Rejected: a button "Compact". The user would have to know that a delete gives no space back.

Rejected: the incremental vacuum mode of SQLite (`auto_vacuum = INCREMENTAL`). It gives free pages back in steps and writes no copy of the file. A database that exists needs one full `VACUUM` to get the mode, and the app would need a rule for the size and the time of each step. The open question on the compaction trigger keeps it as an option.

### H11. Schema version 2 adds the Notes, and a newer file is not written (sensor-history)

Both open functions of `HistoryDb` (H5) read `schema_version` first. Only the writer open changes the file.

- A new file gets version 2.
- A version 1 file gets the table `notes`, its index and version 2 in one transaction. No row of `samples`, `minutes` or `series` changes.
- A version above 2 gives an error in both open functions. The app writes nothing, records nothing and draws no chart from the file. The History screen shows its database error state with a text that says that a newer version of nicecooling wrote the file.

`docs/storage.md` has the same rule for the files of the daemon: a file with a higher schema version than the program knows is not written.

Rejected: read a newer file and draw the tables that this version knows. A newer schema can give these tables another meaning, and the chart would then show wrong data with no sign.

### H12. A View is a chart setup in its own file (history-chart)

The file is `$XDG_CONFIG_HOME/nicecooling/history-views.toml`. A View is a setting of the user, so it is in the configuration directory, and `history.db` can be removed without it. The format is TOML, which the app reads and writes already.

A View holds its name, the range length, its Series in their order and the fixed axis limits per unit. Each Series entry holds the sensor id, the name at the time of the save, the color, the line width, the fill and the hidden state. A View holds no time position and no zoom: it opens at the present time. The name of a View is unique, so a View has no other id. The file also holds the name of the last loaded View.

The app window writes the file with the atomic write of `docs/storage.md`. It writes at "Save view", "Save view as", "Rename view" and "Delete view", and when the user loads another View.

A chart setup that is not saved stays in memory, as now. The first open of the History screen from the sidebar loads the last loaded View. An open with one sensor from another screen loads no View, also when it is the first open.

A file that does not parse gives no View, and the History screen shows the path. The app leaves that file as it is until the user saves a View. It then keeps the old content in a copy with the suffix `.bad`. A file with a higher schema version than 1 gives no View and is not written: the commands that save a View are off with the reason.

Two app processes of the same user both write the file. The last write wins, as for `settings.toml`.

Rejected: the Views in `settings.toml`. The loader of that file gives the defaults for the whole file when one value does not parse. A damaged View would reset the language and the notification switches. An older app also removes a table that it does not know at its next save, so a downgrade would delete the Views.

Rejected: the Views in `history.db`. A user who removes the database to start again would lose the chart setups.

### H13. The style and the axis limits belong to the chart setup (history-chart)

Each Series of the chart has a color, a line width of 1 to 4 px and a fill switch. Each unit has an automatic axis or a fixed minimum and maximum. A View stores them with its Series. So a second View can show the same sensor in another color.

- The default color is the series color of the position, as now. The color of a Series is a series color (`s1` to `s6`) or a custom `#RRGGBB`. A series color follows the theme. A custom color is the same in both themes.
- A Series at position 7 or later has a dashed line, as now, with any color. The View stores the order, so it needs no field for the dash.
- `docs/ui/DESIGN.md` keeps red and amber for Alarms and Warnings. A custom color in that range gets a caption and applies (ADR-0007: warn, never refuse).
- The fill goes from the line to the lower edge of the plot at 10% opacity. The band of a Minute row stays at 20%, so both are visible on one chart.
- Fixed limits cut a line at the edge of the plot. The legend row says that values are outside, and its minimum and maximum still come from all values.

Rejected: one style per sensor for every chart. The style would need a place and a cleanup for a sensor that no View holds.

Rejected: a line width up to 8 px. The chart is 340 px high, and a line of 4 px is 1.2% of it already.

### H14. A Note is a row of the history database (history-chart)

A Note has a time and a text of 1 to 120 characters. It is a row of the table `notes` of `history.db`. Every chart of the History screen that shows that time draws it, with every View and every set of Series. The charts of other screens draw no Note.

Reason: a Note marks a change on the machine, for example "Fan curve changed". The change is true for every sensor. The row is in the file of the data that it comments, and a query by time finds the Notes of a window. A user who removes the file `history.db` removes the Notes together with the data of their times. A View depends on no data, so it is in another file (H12).

A Note is text of the user and not recorded data. So a range delete keeps the Notes (H8), and the delete of the whole history keeps them unless the user turns the switch on (H9).

The user adds a Note with a secondary click in the chart or with the key N at the cursor. A dialog "Notes" lists all of them and moves the window to one. The limit is 500 Notes. The service thread writes a Note (H5).

Rejected: Notes as a part of a View. A Note would show in one chart only, and it would leave with its View.

Rejected: a Note with a time and a value at a free place of the plot. With two axes and several charts, a value has no single place.

### H15. The image is an SVG document that `resvg` turns into a PNG (history-chart)

One pure function, the chart layout, turns the model of the screen and a plot size into shapes. The shapes are lines, filled areas, the hatched areas of the gaps, dashed lines and the places of the labels. The chart layout holds the geometry of the chart once: the scales, the tick values, the cut at fixed limits, the fill, the band, the gaps, the Alarm and Warning markers and the Notes.

The painter of the screen draws the shapes with the primitives of `widgets/paint.rs`. A second pure function writes the same shapes, the palette and the texts as one SVG document. `resvg` draws it into 1920 x 1080 px, and its pixel type writes the PNG.

`gpui-pre` is the GPUI release that the app builds on. Its version 0.3.12 builds `resvg` 0.46.0 with the features `text` and `system-fonts` already. So the direct dependency adds no crate to the lock file.

- The size is fixed at 1920 x 1080 px. The colors are those of the theme that the app shows (owner decision 6).
- The image holds the title, the range, the charts and the legend. It has no cursor and no control. Task 4.1 puts its layout, with the sizes of each part, in a section "Image" of `docs/ui/screens/history.md`, and the owner approves it at the gate of task 4.3.
- The text asks for the font list `Adwaita Sans, Inter, sans-serif`. The font database loads the fonts of the system, so the image does not fail without the interface font.
- The file write follows H7: a temporary file, then a rename.

Reason for a second output: `gpui-pre` 0.3.12 reads the pixels of a window back only in its test feature, and its Linux platform crate has no such function. The SVG path also does not depend on the size of the app window, and a test can read the document as text.

Cost: the painter of the screen moves from its own geometry to the chart layout. Each output places its own text, because GPUI and `resvg` measure text in different ways. Task 9.2 compares the two outputs by hand.

Rejected: a second painter with its own geometry. The style, the axis limits and the Notes of this change would each be written twice, and only a check by hand would find a difference.

Rejected: an SVG file as the output. The text of an SVG file depends on the fonts of the reader's machine.

### H16. Places in the interface (sensor-history, history-chart)

- History toolbar: the View button after the sensor picker. It shows the name of the loaded View and opens the View menu. The View menu holds the Views and the commands "Save view", "Save view as", "Rename view" and "Delete view".
- History toolbar: the menu button "Chart" at the end, with "Notes", "Axis limits", "Export to CSV", "Save as image" and "Delete data".
- Legend row: a click on the color mark opens the style popover. A click on the rest of the row hides or shows the Series, as now.
- Chart: a secondary click opens "Add note here". The key N does the same at the cursor. A click on the icon of a Note opens the Note.
- Settings: a new card "History" with the two periods and their estimates, "Recorded sensors", "Size on disk" and "Delete the whole history".

`docs/ui/DESIGN.md` has this rule for a card header: one text button at most, and the other commands in a menu button. Task 4.2 adds a rule of the same kind for a toolbar to that file: "A toolbar has one menu button for the commands that are not navigation." Tasks 4.1 and 4.2 write the screen files, and the owner approves them before a screen task starts.

Rejected: one toolbar button per command. The toolbar has the picker, six range buttons and four navigation buttons already. Nine more commands as buttons make it longer than the content of the smallest app window (1100 px, with a sidebar of 220 px).

### H17. A View keeps a sensor that the daemon does not report (history-chart)

A device can be away for a day. A View that drops its sensor at the load would lose it permanently. So the Series stays, with the name that the View stored, the tag "Sensor not found" and the rows that the store still holds. When a sensor with the same id returns, the Series is live again. The tag shows only while the daemon is connected, because an offline daemon reports no sensor.

Rejected: remove the Series at the load. A sensor that is away for a day would leave every View that holds it.

Rejected: a question at each load. The user removes the Series from the chart with one click when the sensor does not come back.

### H18. Samples wait while the service thread works (sensor-history)

A delete, a prune or a compaction can hold the service thread for seconds or minutes. The daemon events wait in the event channel during that time. The event channel holds 1024 events. This is about 17 minutes at one event per second, and less when the daemon sends more events.

Each sensor event has the time of its values. So the service thread records the waiting Samples with their own times when the operation ends. An operation leaves no gap when 1024 events or fewer arrive in its time. With more events, the oldest are lost, and the chart shows a gap there.

The service thread decides from the Sample times when a minute is complete. While events arrive, its present time is the time of the newest Sample. After a wait of 1 s with no event, its present time is the clock. The result "events were lost" of the event channel is not such a wait: 1024 events still wait behind it.

Reason: a thread that uses the clock alone closes the minutes of the waiting time before their Samples arrive. These Samples then get no Minute row.

Rejected: a delete in steps, with a record between two steps. The service thread would need a state for a delete in progress, and the event channel gives the same time already.

### H19. A prune, a count and a delete read the rows by Series (sensor-history)

The primary key of `samples` and of `minutes` is `(series, t)`. A statement with a time and no Series reads the whole table. The current prune is such a statement, and a Sample period of 30 days gives 155 million rows for 60 Series.

So each prune, each count of a confirmation and each range delete runs one statement per Series on the range of the primary key. Its cost follows the rows that it touches. A prune and a range delete commit after each Series. Reason: one transaction for a large delete writes all its pages to the WAL file before the first checkpoint.

A stop of the app in the middle leaves a delete done for some Series only. The user starts it again. The delete of the whole history has no condition and empties each table in one statement.

Rejected: a second index on the time. It adds a second tree to the largest table.

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
last_view = "Loop temperatures"   # the name of the last loaded View; absent when no View is loaded

[[views]]
name = "Loop temperatures"   # unique without case
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

The app is the only writer. A rename of the last loaded View also changes `last_view`.

## Limits

Bounds of an input:

| Limit | Number | Reason |
|---|---|---|
| Sample period | 24 hours, 48 hours, 7 days, 30 days | Owner decision 1 |
| Minute period | 30 days, 90 days, 1 year, 2 years, 5 years | Owner decision 1 |
| Views | 32, with 32 Series each | 1024 Series entries of about 150 bytes are about 150 KB |
| Name of a View | 1 to 48 characters | The toolbar button shows the name in one line |
| Notes | 500 | The table stays below 300 KB |
| Text of a Note | 1 to 120 characters | One line in the list |
| Line width | 1 to 4 px | H13 |
| Axis limit | -1 000 000 to 1 000 000 | Every unit of the app fits |
| Note text in the image | 40 characters, then "..." | The label stays inside its chart |

At a count limit (Views, Series of a View, Notes) the command that adds is off and shows the reason. A text field takes no more characters than its limit. A number outside its range is not applied, and the field shows the reason. The rule "warn, never refuse" of ADR-0007 is for a choice that is valid and has a risk, for example a long period or a red color. A 33rd View is outside the model.

Sizes and capacities:

| Limit | Number | Reason |
|---|---|---|
| Largest store, 60 Series | about 3.6 GB of Samples and 6.9 GB of Minute rows | The longest choices of H2. The app warns above 2 GB per period |
| Default store, 60 Series | about 238 MB of Samples and 1.4 GB of Minute rows | 48 hours and 1 year |
| Window with Samples | 48 hours, 172 800 points per Series | H4 |
| Rows of one export | 2 592 001 Samples (30 days), 525 601 Minute rows (1 year) | The longest window of each kind |
| Size of one export | No upper limit. Warning icon above 100 MB. 60 Series for 30 days at 1 s are about 1.5 GB | H7 |
| History operations at one time | 1 | H5 |
| Start of a request in the service thread | 1 s when the thread is not in a write | H5 |
| Events that wait while the service thread works | 1024, about 17 minutes at one event per second | H18 |
| Compaction | After a delete that the user confirmed and that removed 1 000 000 rows or more | H10 |
| Free disk space for a compaction | Twice the size of the pages in use | H10 |
| WAL file after a checkpoint | 64 MiB | H10 |
| Image | 1920 x 1080 px, 8 294 400 bytes before the PNG compression | H15 |

## Requirements of add-nicecooling-v1 that this change extends

The archive step of OpenSpec merges the specs of a change into `openspec/specs/`. `add-nicecooling-v1` is not archived, so `openspec/specs/` does not hold its requirements yet, and this change cannot change them there. It extends this requirement, and task 8.4 edits it in the spec file of `add-nicecooling-v1`:

| Capability | Change |
|---|---|
| `desktop-app` | The requirement "Sensor history": 48 hours and 1 year are the defaults of two settings, and a sensor of the list of not recorded sensors is not recorded. The scenario "Old samples" holds for the defaults. |

Files of the contract that change: `docs/storage.md`, `docs/app.md`, `docs/ui/screens/history.md`, `docs/ui/screens/settings.md`, `docs/ui/DESIGN.md`, `docs/ui/components.md`, `docs/i18n-terms.md` and `GLOSSARY.md`. ADR-0005 names 48 hours and 1 year as the times of the history: task 8.3 changes that sentence to "the defaults of two settings". Two procedure files are new in `docs/hardware-checks/`, for the checks that need a person on the reference desktop: `history-export.md` (export, delete, periods) and `history-chart.md` (Views, Notes, image). `docs/api.md` does not change.

## Order with the other changes

Two other changes are on `main` next to this one. Each task of this change reads a shared file as it is on `main` when the task starts.

`add-node-graph` is in implementation. Its section 3 is merged before section 1 of this change.

- `app_settings.rs`: task 3.3 of `add-node-graph` makes the public function for the atomic write. Task 6.1 of this change uses that function and adds no second one. The tables `[mqtt]` and `[smtp]` of `add-node-graph` and the table `[history]` are separate tables of `AppSettings` and of `docs/storage.md`.
- The fixture of task 1.7 is a copy of `AppSettings` as it is on `main` when the task starts, with the tables that are merged.
- `services.rs` and `main.rs`: `add-node-graph` adds `spawn_graphs`, and this change gives `spawn_history` two more arguments. The two service threads share no state.
- The stop of the app: task 3.6 of `add-node-graph` gives "Quit" and SIGTERM one path that waits for the Graph service. The history service thread is not in that path and ends with the process, as now. The Samples of the last 10 s that are not in the store are lost, as now. A stop during an export leaves the file `<name>.part`. A stop during a delete or a compaction is the case of H19 and H10: no row is half written.
- The Settings screen: `add-node-graph` adds the group "Network" (its tasks 7.5 and 8.10). The card "History" comes after the cards that are in `docs/ui/screens/settings.md` when task 4.2 starts. Each change has its own owner gate for its part of the screen file.
- A Graph sensor is a sensor that the daemon reports. The app records it, the dialog "Recorded sensors" lists it, and a View can hold it.
- The id of a removed Graph sensor is not used again while the daemon keeps its counter. A downgrade of the daemon and a second upgrade start the counter again (design of `add-node-graph`, Migration). An id can then name another Graph sensor, and its Series holds the rows of both.
- A Graph sensor can change its unit and keep its id. The store keeps one Series per id with the unit of its first Sample, so the rows before the change have the old unit under the new name. The user deletes them with a range delete. A Derived sensor has the same limit.
- `GLOSSARY.md`, `docs/app.md` and `docs/i18n-terms.md`: both changes add sections. `add-node-graph` has its downgrade steps in `docs/graphs.md`.

`add-dashboard-pages` has its specs and no code. No order between it and this change is necessary. The change that is merged second adapts to the first:

- `load_series`: this change replaces its argument `now` with the data kind, and adds the check of H4 as its own function (task 2.2). The chart Tile calls the check for its one Series and then `load_series`, as the Overview charts do.
- A chart Tile is 24 hours long or shorter, and every Sample period is 24 hours or longer. So a chart Tile still draws Samples with every choice of H2.
- A chart Tile reads with the reader open of H5. A newer database gives it an error, which it shows as an empty plot.
- A chart Tile of a sensor that is not recorded draws the stored rows and then nothing, as the Overview charts do (H1).
- A chart Tile draws no Note (H14).
- A click on a Tile opens the History screen with one sensor. As for the Sensors screen, the History screen then loads no View.
- `SettingsStore`: `add-dashboard-pages` adds the field for the Dashboards, and this change adds the shared history settings. Both use the atomic write function of `app_settings.rs`.
- `docs/ui/DESIGN.md`: both changes add to the rule that red and amber are for Alarms and Warnings. H13 adds the caption of a custom color, and `add-dashboard-pages` adds the Tile limits.
- `docs/app.md`: both changes add a row to the table "Settings and files". Each change has its downgrade steps in a subsection "Downgrade" of its own section.
- `GLOSSARY.md`, `docs/storage.md`, `docs/i18n-terms.md`, `docs/ui/components.md` and the four locale files: both changes add entries.
- A damaged file: both changes keep the content of the user in a file with the suffix `.bad` and show the path. `add-dashboard-pages` renames the file on a command. This change makes the copy at the next save of a View, because a chart works with no View.

The three changes also edit these files. Each one adds its own lines and keeps the lines of the others:

- `crates/nicecooling/src/app.rs`: this change adds the field `history` to `Backend` and `AppModel` (task 1.4). Each test file that builds `Backend` gets the new field, also the test files of the other changes.
- `crates/nicecooling/src/services.rs`: this change gives `next_event` a third result (task 1.8). `spawn_graphs` of `add-node-graph` is one more caller.
- `crates/nicecooling/src/screen_view.rs` and `crates/nicecooling/src/screens/handlers.rs`: each change adds variants of `InputKey`, their arms, and its own state.
- `crates/nicecooling/src/screens/mod.rs`: each change adds module lines.
- `crates/nicecooling/src/screens/format.rs`: `add-dashboard-pages` makes `digits` public, and this change adds `size_text`.
- `crates/nicecooling/src/screens/settings.rs`, `settings_view.rs` and `crates/nicecooling/tests/settings.rs`: `add-node-graph` adds the group "Network" with its tests. This change adds `SettingsUi`, a third argument of `SettingsModel::build`, and the card "History".
- `crates/nicecooling/tests/dashboards_chart.rs` of `add-dashboard-pages` calls `load_series`. Task 2.2 changes that call when the file is on `main`.
- `crates/nicecooling/tests/screens_draw.rs`: each change adds its draw tests.
- The root `Cargo.toml`, `crates/nicecooling/Cargo.toml` and `Cargo.lock`: `add-node-graph` adds crates. This change adds `resvg` and `rustix` as direct dependencies and no crate to the lock file.
- `openspec/changes/add-nicecooling-v1/specs/desktop-app/spec.md`: each change edits another requirement.

This change is archived after `add-nicecooling-v1`, because it extends a requirement of that change.

## Migration and rollback

- Forward: the first start of the new app brings `history.db` to version 2 (H11). `settings.toml` gets the table `[history]` at the first change of a setting. `history-views.toml` appears at the first saved View. No stored row is converted.
- Back: an older app opens a version 2 database. It does not read the version, it ignores the table `notes`, and it records as before. Its prune uses 48 hours and 365 days again, at its first run after the start. At that prune, a user with a longer period loses the rows beyond these two times. The older app ignores the table `[history]` and removes it at its next save of the settings. It never reads `history-views.toml`.
- Before a downgrade: export what must stay, or copy `history.db`. `docs/app.md` has these steps in a subsection "Downgrade" of its section "History".
- Forward again: the Notes and the Views are still there. If the older app saved the settings, the two periods and the list of not recorded sensors are the defaults. The check of H4 makes the chart right for the rows that the older app removed.

Task 1.7 tests the two "back" statements with the old loader and the old schema text as test fixtures.

## Risks / Trade-offs

- [The estimate of 44 bytes per Minute row is computed, not measured] -> task 2.6 measures it with the size test. When the measured number is below 40 or above 48, task 2.6 sets it in the code, in `docs/storage.md`, in this design and in the specs, before the Settings card is built.
- [One year of Minute rows for 60 Series is about 1.4 GB, more than the 238 MB of the Samples] -> the Settings card shows both estimates and the size on disk, and the Minute period has two shorter choices.
- [A compaction holds the service thread for its length] -> it runs only after a delete of 1 000 000 rows or more, which the user started and confirmed. The Samples of that time wait in the event channel (H18). Task 3.6 measures the time.
- [A compaction needs free disk space of up to twice the size of the rows that stay] -> the service thread reads the free space first and does not compact with less (H10).
- [A compaction after 1 000 000 removed rows writes the whole store again, also when the store is 10 GB and the delete freed 23 MB] -> the open question on the compaction trigger.
- [An operation with more than 1024 waiting events leaves a gap] -> the event channel gives about 17 minutes. Task 3.6 measures the count, the delete and the compaction of the size test.
- [The count of a confirmation reads every row that the delete removes, so the dialog waits] -> the dialog shows a counting state, and task 3.6 measures the count of the size test.
- [A second app process of the same user does not see a delete or a new period of the first] -> H5 states the limit, and `docs/app.md` says it. The data of the range can come back for up to one minute, and the user can delete again.
- [A settings file that is absent or that does not parse gives the default periods, and the next prune deletes with them] -> the open question on the place of the periods.
- [A delete cannot be undone] -> every delete has a confirmation with the exact count, the destructive button and the text "You cannot undo this."
- [A downgrade prunes at 48 hours and 365 days] -> the subsection "Downgrade" of `docs/app.md` says it and gives the steps.
- [A stop of the app during an export or an image save leaves a file `<name>.part`] -> the file is next to the chosen file and has its name. The user removes it.
- [A spreadsheet program reads a field that starts with `=`, `+`, `-` or `@` as a formula, and a sensor name can start so] -> accepted. The user gives the names, and the file is plain CSV for every reader.
- [A sensor that changes its unit keeps one Series with the first unit] -> "Order with the other changes" states the limit. The user deletes the old rows.
- [The image can differ from the screen in the text and in the way that each output draws a shape] -> both outputs use one chart layout, the layout tests check each part of the model, and the manual check of task 9.2 compares both.
- [The font of the image depends on the fonts of the system] -> the font list ends with `sans-serif`, and the tests do not compare text pixels.
- [A custom color can be red or amber, against the series rule of `docs/ui/DESIGN.md`] -> the caption warns, and the default colors stay the six series colors.
- [An export of 1.5 GB takes minutes] -> the dialog shows the size before the start, the export has a progress bar and "Cancel", and the recording goes on.
- [Another local user can read an exported file with open permissions] -> the file has the default permissions of the user, as every file that the user saves. ADR-0007 gives no defense against other local users.

## Product choices that the owner can change

Each row is a choice that the owner decisions do not close. The specs hold the option of the column "In the specs", and a row changes only when the owner changes it.

| Choice | In the specs | Alternative |
|---|---|---|
| Place of the recording switch | One dialog in the Settings screen, for sensors and Channels | A button in each row of the Sensors screen, which shows no Channel |
| Rows of a sensor whose switch goes off | They stay until a delete or the prune | Delete them with the switch, after a confirmation |
| Data kind choice in the export | The dialog also offers Minute rows when Samples exist | No choice: always the kind of the check of H4 |
| Decimals in the CSV file | 3 at most, no trailing zero | The full stored number, up to 17 digits |
| Byte order mark and line end | None, and one line feed | A byte order mark and CR LF for spreadsheet programs on other systems |
| Two Series with the same name | The id in square brackets after the name | The same header twice |
| Edge minutes of a range delete | A Minute row that overlaps the range is deleted whole | Keep the Minute rows that are only partly in the range |
| Notes at the delete of the whole history | They stay, with a switch to delete them | Always delete them |
| Disk space after a delete | Automatic compaction after 1 000 000 removed rows | A button "Compact", or no compaction |
| Place of the style and the axis limits | In the View | One style per sensor for every chart |
| Colors of a Series | Six series colors or a custom `#RRGGBB`, with a caption for red and amber | The six series colors only |
| Line width | 1 to 4 px | Up to 8 px |
| What a Note is | A time and a text, in `history.db`, drawn in every chart of the History screen | A Note of one View, or a text at a free place of the plot |
| Limits | 32 Views, 32 Series per View, 500 Notes, 120 characters | Other numbers |
| View at the start of the app | The last loaded View | An empty History screen, as now |
| Sensor of a View that is not found | It stays with a tag and its stored rows | Remove it at the load |

## Open Questions

Two questions are open for the owner. The specs and the tasks hold the option "In the specs" of each question. The design recommends the option "Recommended". The scenarios of the specs hold for these two options of each question.

1. The trigger of the compaction (H10).
   - In the specs: after a confirmed delete of 1 000 000 rows or more. The rule is one number. The cost: a delete of 23 MB in a store of 10 GB writes 10 GB again and needs 20 GB of free space.
   - Recommended: after a confirmed delete, when the free pages are 25% of the file or more and 64 MB or more. SQLite gives both numbers with no scan (`freelist_count`, `page_count`). The compaction then runs only when it gives back 25% of the file or more. The cost: two numbers in place of one.
   - Other: the incremental vacuum mode. It never writes the whole store again after the first time. The cost: a step rule in the service thread and one full `VACUUM` for each database that exists.
2. The place of the two periods (H2).
   - In the specs: the table `[history]` of `settings.toml`. A settings file that is absent or that does not parse gives 48 hours and 1 year. The next prune then deletes the rows beyond these times with no count and no question, against the rule of H3. The same occurs when the user removes the settings directory to start again.
   - Recommended: two keys in the table `meta` of `history.db`, which the service thread writes after the confirmation of H3. The periods then stay with the data that they keep, and a prune uses only a period that the user chose. The list of not recorded sensors stays in `settings.toml`. The cost: the service thread writes the periods, the Settings screen reads them with the reader open, and the period rows are off when the store is not open.

The three things that tasks measure or check (the bytes of a Minute row, the time of a compaction, the look of the image) change no decision of this design.
