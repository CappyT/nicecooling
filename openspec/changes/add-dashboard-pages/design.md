# Design

## Context

This design belongs to the change of `proposal.md`. `GLOSSARY.md` defines the project terms. The spec of this change holds the numbers and the scenarios: `specs/dashboards/spec.md`. This document gives the decisions and their reasons.

`add-nicecooling-v1` gives the project two processes. The root daemon `nicecoolingd` owns the hardware, the Curves, the Alarms and the alarm response. The desktop app runs in the user session. It owns the window, the tray icon, the notifications and the history store, and it runs with or without a window (`docs/adr/0005-desktop-app-is-the-session-agent.md`). A command that changes the daemon goes through polkit. A command that changes a file of the app does not.

The app has these parts that this change builds on:

- The shell: a sidebar of screens, a header bar, global banners and a status bar (`crates/nicecooling/src/shell.rs`, `docs/ui/screens/app-shell.md`). A screen with tabs puts a tab strip above its content, as the Leakshield screen does.
- The Overview: one fixed screen with a card per Loop (`crates/nicecooling/src/screens/overview.rs`).
- The live values: one `SensorsUpdated` signal of the daemon per second, kept in the app state with the last values (`crates/nicecooling/src/model.rs`). The app state also holds the snapshot: the sensor list, the Loops, the Channels and their Curves, as the daemon gave them last.
- The history store: `history.db`, 1 s samples for 48 hours, written every 10 s by one service thread while the app runs (`crates/nicecooling/src/history/`). The History screen reads it and draws charts with a zoom.
- The painters: lines, discs and rectangles on a canvas (`crates/nicecooling/src/widgets/paint.rs`), and a line chart over time with reference lines and gaps (`time_chart` in `crates/nicecooling/src/screens/leakshield/charts.rs`).
- The app settings: `settings.toml` in `$XDG_CONFIG_HOME/nicecooling/`, written with `write_atomic` (`crates/nicecooling/src/app_settings.rs`).

The color and text style names of this document (`card`, `s1`, `fg`, `fg-dim`, `warning`, `warning-bg`, `warning-tint`, `alarm`, `alarm-bg`, `alarm-tint`, `track`, `border-strong`, `accent`, `caption`, `value`, `numeral`) are the tokens of `docs/ui/DESIGN.md`. A comp is an HTML mockup of a screen in `docs/ui/comps/`. The reference desktop is the machine of the owner with the reference Board and the Leakshield. "The vendor program" is aquasuite X.96: `proposal.md` names the feature study.

The owner gave these decisions. Their record is [issue #89](https://github.com/CappyT/nicecooling/issues/89) of the repository `CappyT/nicecooling`, in its comments of 2026-11-25. This document cites them by number:

1. Tiles on a cell grid of 12 columns. A Tile has a column span and a row span. Tiles do not overlap. No layers, no free pixel canvas.
2. The fixed Overview stays as it is, and it stays the screen that opens on an Alarm. A page that the user builds has a name and an ordered list of Tiles.
3. The Tile kinds are value, gauge, bar, chart and text.
4. Per Tile: the sensor, an own title or the sensor name, decimals 0 to 3, a minimum and a maximum for gauge and bar, the time span for the chart, and two optional limits that color the Tile with the Warning and Alarm colors of the theme.
5. An Edit mode: add, remove, move, resize.
6. A generator that makes one such page from a Loop, computed from the parts of the Loop.
7. These pages are stored in their own file `dashboards.toml` in the configuration directory of the user, not inside `settings.toml`.
8. Not in this change: an image Tile, a control Tile, free markup, desktop pages or separate windows, import and export.
9. A page that the user builds is a Dashboard, and its element is a Tile. The sidebar gets one entry "Dashboards" after "Overview". The screen has one tab per Dashboard and a "More" menu.
10. The app writes `dashboards.toml` within 2 s of a change, at most once per second, and at Quit (P11).
11. Alt with an arrow moves a Tile to the next free place in that direction, over the Tiles in its way (P11).
12. While the app connects at its start, the shell shows its text in place of the Dashboards screen, as on every screen (P10).
13. The tooltip of a Tile shows on a hover only (P11).
14. This change and `add-history-export` have no fixed order. The change that lands second adapts ("Order with the other changes").

The standing owner rules also hold. No command asks for a password. The app warns and never refuses a valid choice of the user. The app has no defense against another local user. Nothing new runs in the root daemon. A Tile limit is not an Alarm and runs no alarm response.

## Goals / Non-Goals

**Goals:**

- A user builds a Dashboard of the values to watch. When it opens, every Tile with a sensor has a value, and every chart Tile has its line.
- A Dashboard keeps one layout from a window of 1100x700 up.
- A Tile limit never looks like an Alarm of the daemon and never acts like one.
- A user who has no mouse can make every change.
- A Dashboards file that is not valid does not delete the Dashboards and does not reset the other app settings.
- The root daemon and the D-Bus contract do not change.

**Non-Goals:**

- The page files and the page model of the vendor program. The app imports none.
- A Tile that writes: a Duty, a Profile, a Leakshield Mode.
- A Tile that shows an image, a clock, free markup or the Loop diagram.
- A Dashboard outside the main window: on the desktop, or in a separate window.
- Import and export of a Dashboard.
- A Tile source that is not a sensor: the Duty of a Channel, the active Profile, an Alarm state.
- Dashboards that several users share.
- A defense against another local user (a Non-Goal of `add-nicecooling-v1`).

## Terms

`GLOSSARY.md` gets these entries with this change. The spec, the code and the interface use them. The app already uses "page" for the display pages of the Leakshield and for the `Page` type of the shell. So a page that the user builds is a Dashboard (decision 9).

| Term | Meaning |
|---|---|
| Dashboard | A page that the user builds from Tiles. It has a name and an ordered list of Tiles. Avoid: page, user page, overview page. |
| Tile | One box of a Dashboard: a value, a gauge, a bar, a chart or a text. It has a place and a size on the grid. Avoid: widget, item, card. |
| Tile limit | A warning limit or an alarm limit that the user sets on a Tile. It colors the Tile. It is not an Alarm and has no alarm response. |
| Edit mode | The state of the Dashboards screen in which the user adds, removes, moves and resizes Tiles. |

"Cell" is one column of one row of the grid. It needs no glossary entry. `docs/ui/widgets.md` keeps "widget" for a custom GPUI element, so a Tile is never a widget. A "count limit" in this document is a limit of P4. It is not a Tile limit.

A Graph sensor is a sensor that a Graph of the app computes. The change `add-node-graph` adds it (`openspec/changes/add-node-graph/design.md`, decision G4). The app sends its value to the daemon each second, and the daemon lists it with the origin `graph`.

## Decisions

### P1. Dashboards are in the app only

The Dashboard model, its file and its screen are in the app crate `nicecooling`. A Dashboard reads what the app already has: the sensor list and the live values of the daemon events, and the history store. No command of a Dashboard calls the daemon. So no Dashboard command goes through polkit, and a user outside the group `wheel` edits Dashboards with no authorization dialog.

Reason: a Dashboard is a view of one user. The daemon serves every user of the machine and has no view state.

Rejected: Dashboards in the state of the daemon. It adds members to the D-Bus contract and root code for data that only a window needs. It breaks the owner rule that nothing new runs in the daemon.

### P2. One sidebar entry, one tab per Dashboard, and the commands on a Dashboard

The sidebar gets one entry, "Dashboards", directly after "Overview" (decision 9). The Dashboards screen has a tab strip: one tab per Dashboard in the stored order, and at its right end the commands "New Dashboard" and "Edit". Tabs that do not fit go into a "More" menu, by the tab overflow rule of `docs/ui/DESIGN.md`. The Overview stays the first entry and the start screen, and an Alarm opens it (decision 2).

The tab strip with its "More" menu is one function of `crates/nicecooling/src/screens/ui.rs`. The tab strip of the Leakshield screen is inline code that clips its tabs and has no overflow menu. This change does not change the Leakshield screen. The function is in `ui.rs` so that the Leakshield screen can take it in a later change.

The commands on a Dashboard:

- "New Dashboard" opens a menu: "Empty Dashboard", and one row per Loop for the generator of P12. It is in the tab strip, in and outside Edit mode. An empty Dashboard gets the name "Dashboard <n>", is selected, and opens in Edit mode.
- The toolbar of Edit mode has a menu for the selected Dashboard: "Rename", "Move left", "Move right" and "Delete Dashboard". "Rename" opens a popover with one input. "Delete Dashboard" asks before it deletes, because it removes many Tiles.
- "Undo" covers a rename, a move of a tab and a delete of a Dashboard, because the user makes them in Edit mode (P11). It does not cover "New Dashboard".

The app keeps the selected Dashboard in the app model, next to the shown screen. So the selection stays when the user closes the window to the tray and opens it again.

Reason: the sidebar does not scroll, and the count of Dashboards is the choice of the user. A tab strip with an overflow menu holds 16 names at every window size.

Cost: Alt+1 to Alt+9 open the first nine sidebar entries, and Alt+2 opens Dashboards. `add-node-graph` adds the entry "Graphs" to the group "Control". The order with both changes is: Overview, Dashboards, Loops, Channels, Sensors, one entry per Leakshield, Profiles, Graphs, Alarms, History, Settings. With one Leakshield these are eleven entries, so History and Settings have no Alt number.

Rejected: one sidebar entry per Dashboard in a group "Dashboards". It gives each Dashboard a sidebar row and a shortcut. At the minimum window it needs a sidebar that scrolls, or a low limit for the count of Dashboards.

Rejected: tabs on the Overview. The Overview would get an edit command and a tab strip, and decision 2 keeps it as it is.

### P3. The grid has a fixed row height

The grid has 12 columns with a gap of 12 px, in the content width less 24 px at each side. A row is 60 px high and the row gap is 12 px, at every window size. The column width follows the window.

| Window width | Grid width | Column width | Tile 3x3 | Tile 6x3 |
|---|---|---|---|---|
| 1100 px (minimum) | 832 px | 58.3 px | 199 x 204 px | 410 x 204 px |
| 1280 px | 1012 px | 73.3 px | 244 x 204 px | 500 x 204 px |
| 1920 px | 1652 px | 126.7 px | 404 x 204 px | 820 x 204 px |

The width of the grid is the window width less the sidebar of 220 px and less 48 px of padding. A Tile of c columns is c columns and c - 1 gaps wide. A Tile of r rows is 60 r + 12 (r - 1) px high.

At the minimum window of 1100x700 the content is 626 px high after the header bar (46 px) and the status bar (28 px). The tab strip takes 38 px and the top padding 12 px. 576 px stay: 8 rows are 564 px. In Edit mode the toolbar takes 46 px more, and 7 rows show. A banner takes 46 px or 64 px more. The Dashboard scrolls vertically.

Reason for the fixed row height: the text styles of `docs/ui/DESIGN.md` have fixed sizes. A row that grows with the window would double the height of every Tile at 1920 px and leave the same text in it. A row of 60 px holds one caption line and one value line with the Tile padding of P5. That is the content of a value Tile of 1 row. The gap of 12 px is the card gap of the app.

Reason for one layout: `docs/ui/DESIGN.md` allows one layout from 1100x700 up. The grid always has 12 columns. The minimum sizes of P5 fit at the column width of 58.3 px, and a test draws them there in English, Italian, German and French.

Rejected: square cells, where the row height is the column width. Rejected: a second layout with 6 columns for a narrow window. The window has a minimum width, so the narrow case does not exist.

### P4. Model and count limits

A Dashboard has a name and a list of Tiles. A Tile has a kind, a column, a row, a column span, a row span and the settings of its kind. Neither has an id. The position in the list identifies a Dashboard. The place on the grid identifies a Tile. The place is unique because Tiles do not overlap. Two Dashboards can have the same name.

The order of the Tile list is reading order: by row, then by column. Decision 2 asks for an ordered list. This order needs no extra field, and the keyboard selection follows it. The screen model also holds an accessible text for each Tile, as the Channels screen does for its Curve editor (`crates/nicecooling/src/screens/channels.rs`). The tests read that text.

| Count limit | Number | Reason |
|---|---|---|
| Dashboards of one user | 16 | The tab strip and its "More" menu stay short |
| Tiles in one Dashboard | 48 | 4 full rows of the smallest Tile. One frame draws 48 Tiles from values that the app holds |
| Chart Tiles in one Dashboard | 12 | Each chart Tile reads the history store (P9) |
| Columns | 12 | Decision 1 |
| Rows | 64 | A bound for the file. 64 rows are 4596 px |
| Column span | 1 to 12 | |
| Row span | 1 to 8 | 8 rows fill the minimum window |
| Name of a Dashboard, own title of a Tile | 40 characters | The longest name that a tab shows in full at the minimum window. A Tile title truncates earlier |
| Text of a text Tile | 200 characters | The spec scenario "Note" puts 180 characters in a Tile of 4x2 |
| Chart time span | 1 to 1440 minutes | P9 |
| Decimals | 0 to 3 | Decision 4 |

The vendor program has no such limits. These count limits bound the file, the draw time of one frame and the read load on the history store.

At a count limit, the command that adds a Dashboard, a Tile or a chart Tile is off, and its tooltip gives the reason. The owner rule "the app warns and never refuses" covers a choice that is valid and unsafe. A 17th Dashboard is outside the model, so that rule does not apply.

No command makes a state that the loader of P13 refuses:

- The store takes a new state only when the valid check of the loader accepts it. Every command, an undo, a redo and the generator give their result to the store. A refused state is a defect of the code, and the test `every_command_keeps_the_file_valid` finds it. The rules below turn the command off first.
- A change of the kind in the Tile settings obeys the same count limits. With 12 chart Tiles, the kind "Chart" is off for a Tile that is not a chart Tile.
- A change of the kind can need a larger minimum size. The Tile grows at its place when the cells are free. Otherwise it moves to the first free place. With no free place, "Save" is off and the Tile settings say "This Dashboard has no free place for this Tile."
- The generator cuts a name and a title to the limit of 40 characters (P12).

A later version that changes a count limit or a minimum size raises `schema_version` (P13). So two versions never disagree on a file with the same version number.

### P5. Tile kinds and sizes

| Kind | Shows | Minimum size | Default size |
|---|---|---|---|
| value | title, number, unit | 2x1 | 3x1 |
| gauge | title, a ring arc of 270 degrees with the number and the unit in it | 2x2 | 3x3 |
| bar | title, number, unit, a horizontal bar on a track | 2x1 | 4x1 |
| chart | title, the newest value, a line over the last N minutes | 3x2 | 6x3 |
| text | a heading (one line) or a note (wrapped) | 1x1 | 4x1 |

Sizes are columns by rows. The minimum size is the smallest size at which the Tile shows its value with no cut at the column width of 58.3 px.

A Tile has the `card` ground, 12 px of padding at the sides and 8 px at the top and the bottom. A card of the app has 18 px of body padding, so P15 adds the Tile padding to `docs/ui/DESIGN.md` as a separate rule. The geometry inside a Tile:

- A Tile of 1 row has 44 px inside. The title is a caption line of 16 px, and the number is a value line of 22 px. 6 px stay.
- A bar Tile of 1 row has its track in these 6 px: a track of 4 px under the value line. From 2 rows up, the track is 8 px high.
- A value Tile of 2 rows or more shows the number in the numeral text style. A value Tile of 1 row uses the value text style.
- A gauge Tile of 3x3 or more shows the number in the numeral text style. A smaller gauge Tile uses the value text style. A gauge Tile of 2x2 has 105 x 116 px inside: the title line and a ring of 96 px.
- A chart Tile has a header line of 22 px with the title and the newest value. A chart Tile of 4x3 or more shows the value labels of its axis. A smaller chart Tile shows the line and the limit lines with no axis label. A chart of 3x2 has 116 px inside. The header line takes 22 px and the frame of `time_chart` takes 28 px, so the plot is 66 px high.
- A heading has no card ground, so it reads as a title between rows of Tiles.

`docs/ui/screens/dashboards.md` gives the full geometry of each kind (P15).

The gauge is a ring: an arc of 270 degrees with its opening at the bottom. It is one new custom widget, the ring gauge widget (`widgets/ring_gauge.rs`). The pressure gauge of the Leakshield screen is a horizontal scale for one fixed range, so the ring gauge does not reuse it. The bar is a track and a filled rectangle. The chart is the `time_chart` painter with its reference lines and its runs of points.

The filled part of a ring, a bar and the line of a chart have the first series color `s1`. `docs/ui/DESIGN.md` keeps blue for data and red and amber for severity.

Rejected: one kind with a style option that picks a text, a bar, a chart or a gauge, as the vendor program has. Five kinds with one job each need no style option, and each dialog shows only the fields of its kind.

### P6. The source of a Tile is a sensor

A value, gauge, bar or chart Tile shows one sensor of the sensor list of the daemon. The Tile stores the sensor id. The sensor picker of the app lists the sensors, and it has no filter by origin.

- A Graph sensor is in the sensor list of the daemon. A Tile takes it as any sensor, with no code of this change and no dependency on `add-node-graph`. While the app sends no value for a Graph sensor, the sensor has no value, and the Tile shows `--`. This is also the case for a user outside the group `wheel`: the daemon refuses the values of that user.
- A Derived sensor and the calibrated flow of a Loop are sensors of the list, so a Tile takes them.
- The Duty of a Channel is not a sensor of the list. A Tile does not show it. The speed of a Channel is a sensor, and a Tile shows it.

Reason: one source kind gives one picker, one value path and one rule for a missing value.

Rejected: a filter that hides Graph sensors. A Graph sensor has a name, a unit and a value as every sensor, so a filter has no reason.

Rejected: a second source kind for the Duty of a Channel. It needs a second picker and a second rule for a missing value.

### P7. Title, number and range

- Title: the own title of the Tile, or the name of the sensor when the own title is empty. The Tile does not store the sensor name, so a rename shows at once.
- Number: 0 to 3 decimals, with the number format of the app (`Locale::decimal`) and the unit text of the sensor (`unit_text`). A new Tile takes the decimals that the app uses for the unit of its sensor: `digits` of `screens/format.rs` gives 1 for `degC`, `V`, `A`, `W` and `mbar/min`, else 0.
- Range of a gauge and a bar: a minimum and a maximum that the user sets, with the maximum above the minimum. A new Tile takes a range from the unit: 0 to 3000 for `rpm`, 0 to 300 for `l/h`, 0 to 450 for `mbar` (`GAUGE_MAX`, the scale of the pressure gauge of the app), else 0 to 100.
- In the Tile settings of a new Tile, each pick of a sensor sets the decimals and the range from the unit, until the user edits that field. In the Tile settings of a stored Tile, a pick of a sensor changes neither.
- A value outside the range fills the ring or the bar fully or not at all. The number is always the real value.

The Tile settings refuse a maximum that is not above the minimum, because such a range has no scale. The owner rule "the app warns and never refuses" covers valid choices only.

Reason for a range from the unit: a gauge on a fan at 1 400 rpm fills about half of its ring with no input of the user.

Rejected: always 0 to 100. A fan at 1 400 rpm would fill the ring fully until the user sets a range.

Rejected: a range that follows the lowest and the highest value of the history. The fill of the same value would change over time.

### P8. A Tile limit changes the look of the Tile only

A Tile has an optional warning limit and an optional alarm limit. Each has a direction, "above" or "below", and a value. The comparison is strict, and the alarm limit wins when both hold.

The look of a limit state:

| Part | Warning state | Alarm state |
|---|---|---|
| Ground of the Tile | `warning-tint` | `alarm-tint` |
| Icon before the title | filled triangle | filled octagon |
| Number | `warning` | `alarm` |
| Filled part of a ring, bar | `warning` | `alarm` |
| Title, unit, caption | `fg` | `fg` |
| Line of a chart | `s1`, unchanged | `s1`, unchanged |
| Limit lines of a chart | dashed, `warning` | dashed, `alarm` |
| Caption, from 2 rows up | "above 35.0 °C" | "above 40.0 °C" |
| Tooltip and accessible text, always | "Warning limit: above 35.0 °C" | "Alarm limit: above 40.0 °C" |

- The Tile never has the solid Alarm field. `docs/ui/DESIGN.md` keeps that field for an active Alarm and for "Daemon down". A tint, an icon and a text are the three signals that the severity system asks for.
- The filled part has the text tokens `warning` and `alarm`, not `warning-bg` and `alarm-bg`. `warning-bg` on `warning-tint` has a contrast of about 2:1, and the text tokens have more than 4.5:1 on their tint in both themes.
- The title, the unit and the caption are `fg` on a tint. `docs/ui/DESIGN.md` forbids `fg-dim` on a tint.
- A Tile limit raises nothing in the daemon and nothing in the shell: no Alarm, no Warning, no notification, no window, no tray state, no status bar text. The Tile settings say so in one line.
- The app evaluates a Tile limit only for a live value of an online daemon. A Tile with no value is in no limit state. While the daemon is offline or down, no Tile is in a limit state.
- No hysteresis. A value that moves around a Tile limit changes the look once per second at most, because a value arrives once per second. A change of the look raises no event and stores nothing. The rule "no flash" of `docs/ui/DESIGN.md` is for an Alarm, and a Tile limit is not one.
- The generator sets no Tile limit. The limits of the Leakshield belong to its Leakshield slot. A copy in a Tile does not follow a change of the slot.

Reason for the theme colors: decision 4. Reason for the tint: the user must tell an Alarm of the daemon from a Tile limit. The alarm response of an Alarm can power off the PC.

Rejected: a free list of colors per value, as the vendor program has. It breaks the rule that red and amber mean severity. Rejected: a Tile that marks a sensor with an active Alarm of the daemon. Decision 4 asks for limits of the user. The Overview already marks the parts of a Loop.

### P9. A chart reads the history store

A chart Tile draws the stored values of its sensor over its time span of 1 to 1440 minutes. The Tile settings offer 5 minutes, 15 minutes, 1 hour, 6 hours and 24 hours. The file takes every whole number in the range.

- The line has data when the Dashboard opens. The history store records every sensor while the app runs, with or without a window (ADR-0005).
- The store gets new rows every 10 s, so the line ends up to 10 s before the present time. The header of the Tile shows the newest live value.
- A chart Tile with a time span of N minutes reads again every N / 4 seconds, rounded down to a whole second. The interval is at least 10 s and at most 60 s: 10 s for 5 minutes, 15 s for 1 hour, 60 s for 24 hours. A read less than 10 s after the last read can return the same rows.
- Only the Dashboard that the window shows reads. A Dashboard behind another tab, and a closed window, read nothing.
- The reads run on the background executor, never in the drawing of a frame. The screen runs one read task at a time. A task opens the store once and reads every chart of the shown Dashboard that is due.
- A task opens the store with `HistoryDb::open`, as the History screen does for each of its reads. rusqlite gives each connection a busy timeout of 5 s, so the open waits for a batch of the writer thread.
- The screen keeps the lines by the key (sensor, time span). Two chart Tiles with the same sensor and the same time span share one read and one line.
- A render of the screen starts the reads. While the daemon is online, the values make a render each second. While it is offline, the store gets no new rows.
- `load_series` of `history/query.rs` reads the rows. A new function in that file, next to `downsample`, reduces them to time buckets. A bucket is N / 10 seconds wide, rounded up to a whole second, and starts at a multiple of its width in Unix time. Each bucket with a value gives one point: the mean of its values. So a line has 601 points at most, and a bucket keeps its point from one read to the next.
- A break in the line is where the stored rows have a gap that is wider than one bucket. `load_series` gives the rows as unbroken segments. It ends a segment at a row with no value and at a gap above `Resolution::gap_s`: 10 s for the 1 s samples, 150 s for the 1 minute rows. Two segments stay two runs of the line when their bucket points are more than one bucket apart.
- A read that fails keeps the line of the last read. A Tile with no good read shows "The history cannot be read." The Tile does not show the error text. The History screen shows it.
- A read of 24 hours is 86 400 rows. Twelve chart Tiles of 24 hours read about one million rows per minute, in the background. The History screen reads more: 86 400 rows per sensor every 2 s for a live window of 24 hours. The manual check has one step for the load.

Reason for the limit of 1440 minutes: the store holds 1 s samples for 48 hours, so every chart is inside them. With `add-history-export` the store keeps the 1 s samples for 24 hours or more, which is the longest chart. The chart draws the mean of the rows that `load_series` gives, so it also draws the 1 minute rows that `load_series` gives after that change.

Reason for the time buckets: `downsample` of `history/query.rs` reduces one unbroken run at a time, so a line with many breaks has no bound. It also makes its buckets by count, so the shape of the line changes at each read of a window that moves.

Rejected: a read-only connection of this change for the chart reads. One read task opens the store once per 10 s or less often, and the open already waits for the writer. `add-history-export` moves every reader to a read-only connection.

Rejected: a value history in memory, filled while the window is open, as the vendor program has. It is empty after each start. Rejected: time spans above 24 hours. The History screen has them, with its zoom. Rejected: a free number input for the time span in the Tile settings. Five choices need no range text, and the file still takes every whole number. Rejected: a read of only the new rows after the first read. It needs a cache of raw rows per chart and a rule for deleted rows. The load is below the load of the History screen, so this change does not need it.

### P10. What a Tile shows without a live value

The daemon is online when the app has a connection. It is offline when the app lost the connection or never got an answer. It is down when systemd gave up its restarts (`docs/ui/screens/app-shell.md`).

| Case | The Tile shows |
|---|---|
| The sensor has no value | `--` without a unit and the dashed-circle icon. A ring and a bar have their track only. A chart line ends where the value stopped. No limit state |
| The sensor is not in the sensor list | "Sensor not found" and the stored sensor id. No value, ring, bar or line. The Tile keeps the id |
| Daemon offline or down, after values in this run | The last value in `fg-dim` with its unit. No limit state. One line above the grid: "Last values 21:40:12". A chart still draws the stored history |
| Daemon offline or down, with no answer in this run | `--`. No Tile says "Sensor not found", because the app has no sensor list to compare with |
| The app connects at its start | The shell shows "Connecting to the daemon" in place of the screen, for at most 2 s, as on every screen. Then the row above applies |
| The daemon raises an Alarm | The window shows the Overview (decision 2). Edit mode ends |

- The Tile keeps the sensor id. When a later sensor list holds the id, the Tile shows the value again. A device that comes back returns its sensor ids.
- The app cannot tell a sensor that returns from a new sensor with the same id. The daemon gives a new Derived sensor the id number after the highest id number in use. So a new Derived sensor gets the id of a deleted one that had the highest id number, and the Tile then shows the new sensor.
- A Tile follows the offline rule of `docs/ui/widgets.md`: it shows its last values in `fg-dim`. The time of the line of the last values is `last_value_time` of `widgets/loop_diagram.rs`, as on the Overview.
- The app has a sensor list of this run when the snapshot of the app state holds one sensor or more.
- The global banners of the shell show above the tab strip, as on every screen.

Reason for the last value while offline: the user sees the state at the time of the loss, and the dim color with the line of the last values says that it is old.

Rejected: `--` for every Tile while offline, as the value bubbles of the Loop diagram on the Overview show. A Dashboard is a list of numbers, and a Dashboard of `--` says less than old numbers with their time.

Rejected: the app removes a Tile whose sensor is not found. A device that is unplugged for one day would cost the user the Tile and its settings.

### P11. Edit mode stores each change at once

"Edit" turns Edit mode on. The screen then shows the cells of the grid as outlines, a toolbar and the selection. Each command changes the Dashboards in memory and stores them. Edit mode keeps the last 50 changes of its session for "Undo" and "Redo". An entry of the undo list is the list of Dashboards and the selected Dashboard before the change. "New Dashboard" clears the undo list. "Done", Escape and a change to another screen end Edit mode and clear the undo list.

- Stored at once means within 2 s (decision 10). A command marks the store. The tick of the app model, which runs once per second (`app.rs`), writes a marked store. So a held key writes once per second, not at each repeat of the key.
- The app writes a marked store before it quits.
- A screen change has no single place in the code: an Alarm, the tray and the sidebar each set the screen. So the Dashboards screen ends Edit mode when it finds, at a render, that the app shows another screen.
- An Alarm also closes an open dialog of the Dashboards screen. The dialog drops the input that the user did not save.
- The commands of Edit mode write a file of the app. They do not use the write gate of the shell (`write_gate` in `shell.rs`), so they work while the daemon is offline.

Reason: an Alarm opens the Overview at any time (decision 2). A draft that only "Done" stores would be lost then, or would need a question on top of an Alarm. The other screens of the app also apply a change when a drag ends or an input loses the focus.

Rejected: a draft with "Save" and "Discard".

Move and resize:

- A drag of a Tile moves it. A drag of the handle at the lower right corner resizes it. The place and the size change by the distance of the drag, to the nearest cell. So the result does not depend on the point of the Tile that the user takes.
- The app makes a move only onto free cells of the grid. Otherwise the Tile goes back. During the drag, an outline shows the target: solid in the accent color when the move is possible, dashed in `border-strong` when it is not. The dashed outline is the "not confirmed" sign of the design system. It uses no severity color.
- A resize stops at the last size that is free, at the minimum size of the kind, at the right edge, at 8 rows and at the last row, row 63.
- Nothing moves other Tiles, and nothing closes an empty row. The user decides each place.
- A new Tile goes to the first free place in reading order, with the default size of its kind. The generator uses the same rule.
- In Edit mode the grid shows the cells of every row down to 8 rows below the last Tile, so a drag has free rows as a target, down to row 63.
- The toolbar of Edit mode is under the tab strip and does not scroll with the grid.
- A drag does not scroll the grid. The keyboard moves a Tile to a row that does not show.

Rejected: a move that pushes other Tiles down. It changes places that the user did not touch, and it needs rules for chains of Tiles.

Keyboard:

The grid is one focus stop, as `docs/ui/widgets.md` asks for a custom widget. The keys follow the LED timeline editor of `docs/ui/widgets.md`, where Alt with an arrow moves a bar and Shift with an arrow changes its length.

| Key | Outside Edit mode | In Edit mode |
|---|---|---|
| Left, Right | Select the previous or the next Tile in reading order | The same |
| Up, Down | Select the nearest Tile above or below that shares a column | The same |
| Enter | Open History with the sensor of the Tile | Open the Tile settings |
| Hover | Show the tooltip of the Tile | The same |
| Alt+arrow | | Move the Tile to the next free place in that direction |
| Shift+Right, Shift+Down | | Make the Tile one cell larger |
| Shift+Left, Shift+Up | | Make the Tile one cell smaller |
| Delete | | Remove the Tile |
| Insert | | Open "Add Tile" |
| Ctrl+Z, Ctrl+Shift+Z | | Undo, redo |
| Escape | Close a menu | Close a dialog or a menu, else end Edit mode. During a drag: put the Tile back |
| Tab | Leave the grid | Leave the grid |

- Decision 11 gives the rule of Alt with an arrow. It moves the Tile by one cell when the cells there are free. Otherwise the Tile goes over the Tiles in its way, to the first place in that direction where all its cells are free. With no such place, nothing changes. So the keyboard reaches every place that a drag reaches.
- The grid scrolls so that the selected Tile shows, after each key.
- The tabs, "More", "New Dashboard", "Edit" and the toolbar buttons are focus stops before the grid, as the buttons of the other screens are. A user turns Edit mode on with Tab and Enter.
- Left and Right stop at the first and the last Tile. They do not go around.
- Outside Edit mode, Enter and a click on a Tile open History with its sensor. A Tile shows one number, and the History screen shows where it came from. With no action, the user would search the sensor in the picker of the History screen.
- The number inputs of the Tile settings take the text that `parse` of `screens/handlers.rs` reads, as the other number inputs of the app. "Save" is off while a number does not parse.

The Tile settings are a dialog 480 px wide, built as the Derived sensor editor is (`screens/sensors_view.rs`). `modal` of `screens/ui.rs` is 440 px wide and holds a text and buttons only: the question of "Delete Dashboard" uses it. In Edit mode, Enter or a double click on a Tile opens the Tile settings. Their fields:

- "Kind", "Sensor" (the sensor picker), "Title" and "Decimals".
- "Minimum" and "Maximum", for gauge and bar.
- "Time span", for chart.
- "Warning limit" and "Alarm limit": a switch, a choice "above" or "below", and a number with the unit.
- The line of P8 that says what a Tile limit does.
- For a text Tile: "Kind", "Text" and "Style" only.
- "Remove", "Cancel" and "Save".

"Save" is one change. A removed Tile needs no question, because "Undo" brings it back.

### P12. The generator computes Tiles from the parts of a Loop

"Create a Dashboard from this Loop" makes a Dashboard that the user edits as any other. It has no link to the Loop afterwards.

| Order | Part of the Loop | Condition | Tile |
|---|---|---|---|
| 1 | Pressure | The Loop has a Leakshield and the sensor `<leakshield id>_pressure` exists | gauge 3x3, range 0 to 450 |
| 2 | Fill level | The Loop has a Leakshield and the sensor `<leakshield id>_fill_level` exists | bar 3x1, range 0 to 100 |
| 3 | Flow | The Loop has a calibrated flow | value 3x1 |
| 4 | Water temperature | The Loop has a water temperature sensor | chart 6x2, 60 minutes |
| 5 | Pump speed | Each Pump channel of the Loop that has a speed sensor | value 3x1, own title: the name of the Channel |
| 6 | Fans | Each fan Channel of the Loop that has a speed sensor, 8 at most | value 3x1, own title: the name of the Channel |

Each Tile goes to the first free place, in this order. Example: a Loop with every part, one pump and two fans fills the rows 0 to 2. Row 0 holds the gauge, the bar, the flow and the pump speed. The chart is under the bar and the flow. The fans are at the right.

- The water temperature is one chart Tile. Its header shows the newest value, so the Dashboard needs no second Tile for it.
- The pump speed Tiles are for the Pump channels of this Loop, in the order of its list.
- The Loop model has no fans. A fan Channel of a Loop is a Channel that obeys three rules. It is not a Pump channel of any Loop. Its control kind is Curve. The Input of its Curve is the water temperature sensor of the Loop, or a Derived sensor that has this sensor as a direct source.
- The Curve is the one that the daemon reports for the Channel, which is the Curve of the active Profile. A Curve on a Graph sensor does not count: the app does not follow the sources of a Graph.
- When no Channel matches, the fan Channels are all Channels that are not a Pump channel of any Loop. So a machine with one Loop and only Hardware curves still gets fan Tiles.
- A Loop with no Leakshield gets no pressure Tile and no fill level Tile. The generator makes the rest and does not refuse.
- A Tile of a Channel gets the Channel name as its own title. The name of the speed sensor tells the user less than the Channel name. A Channel name has up to 64 characters, and the generator cuts the title to 40.
- The Dashboard gets the name of the Loop. When a Dashboard has that name, the name gets a number from 2 up. The generator cuts the Loop name so that the name with its number has 40 characters at most.
- The command is in the Loops screen, in the menu of the Loop header, and in the menu of "New Dashboard", as one row per Loop.

Reason for a computed layout: decision 6. A fixed template per device, as the vendor program has, does not fit hwmon hardware that differs per Board.

Reason for the fan Tiles: the proposal names the fans of a Loop, and the fans on the water temperature are the fans of the radiators.

Rejected: no fan Tile. The Dashboard of a Loop would not show if its radiators get air. Rejected: always all Channels that are not a Pump channel. A Loop Dashboard would then show the CPU fan and the fans of another Loop.

Rejected: a pressure Tile and a fill level Tile with no sensor for a Loop without a Leakshield. A Tile that never has a value gives the user no information. Rejected: the command is off for a Loop without a Leakshield. Such a Loop still has a flow, a water temperature and speeds to show.

Rejected: the command in one place only. A user who works on a Loop finds the command in the Loops screen. A user who adds a Dashboard finds it in "New Dashboard".

### P13. Storage: one file in the settings directory of the user

The Dashboards are in `$XDG_CONFIG_HOME/nicecooling/dashboards.toml`, next to `settings.toml` (decision 7). The app is the only writer. It writes with `write_atomic` of `app_settings.rs`, which is the atomic write of `docs/storage.md`: a temporary file, `fsync`, rename, `fsync` of the directory. `docs/storage.md` gets the format before the code that writes it.

```toml
schema_version = 1

[[dashboards]]
name = "Main loop"

[[dashboards.tiles]]
kind = "gauge"            # "value", "gauge", "bar", "chart" or "text"
col = 0                   # 0 to 11
row = 0                   # 0 to 63
col_span = 3              # 1 to 12
row_span = 3              # 1 to 8
sensor = "leakshield_20417_31072_pressure"
title = ""                # empty: the name of the sensor
decimals = 0              # 0 to 3
min = 0.0                 # gauge and bar
max = 450.0
warning = { direction = "below", value = 260.0 }   # optional
alarm = { direction = "below", value = 250.0 }     # optional

[[dashboards.tiles]]
kind = "chart"
col = 3
row = 1
col_span = 6
row_span = 2
sensor = "nct6798_temp1"
title = ""
decimals = 1
span_min = 60             # chart: 1 to 1440

[[dashboards.tiles]]
kind = "text"
col = 0
row = 3
col_span = 12
row_span = 1
text = "Radiators"        # 1 to 200 characters
style = "heading"         # "heading" or "note"
```

- The order of `dashboards` is the tab order.
- A file must have `kind`, `col`, `row`, `col_span` and `row_span` for each Tile, `sensor` for a Tile with a sensor, and `text` for a text Tile. The other fields have defaults: `title` empty, `decimals` 0, `min` 0, `max` 100, `span_min` 60, `style` `note`, and no Tile limit.
- The app writes every field that the kind uses, also with its default value. It writes no field that the kind does not use, and no `warning` or `alarm` that is off. It writes `tiles` in reading order.
- The loader puts the Tiles in reading order. It ignores a key that it does not know, and a field that the kind does not use. A file with these is valid.
- The loader reads `schema_version` first. A `schema_version` above 1 means a file of a newer version. The app does not show that file and does not write it. `docs/storage.md` gives the same rule for the daemon.
- A file is valid when it parses and every Dashboard and Tile obeys the count limits of P4, the grid rules, the minimum sizes of P5 and the field ranges of the spec. The app does not repair a file.
- The file has four states: no file, loaded, not valid, and newer version. The store also holds the error text of a failed write, next to the state.
- For a file that is not valid, the screen shows the path and the first fault, with "Try again" and "Start with no Dashboard". The app does not show and does not write the file. "Start with no Dashboard" renames the file to `dashboards.toml.bad`. It replaces an older file of that name.
- For a file of a newer version, the screen says so and shows the path. It has no command that changes a Dashboard, and no "Start with no Dashboard".
- A missing file means no Dashboard. The app creates the file at the first change.
- A failed write keeps the change in memory and shows the error with "Try again". The next change also writes again.

Reason for a separate file: decision 7. It has two effects that a table in `settings.toml` does not have:

- Downgrade. An app without Dashboards parses `settings.toml`, ignores a table that it does not know, and writes the file without it at the next settings change. The Dashboards would be gone. With a separate file, an older app never opens it.
- Fault isolation. `AppSettings::load` gives the defaults for a file that does not parse. A fault in one Tile would reset the language, the tray switch and the hidden sensors.

Reason for the whole file as the unit of the valid check: the app writes only valid files, so a fault comes from an edit by hand. The screen names the first fault, and "Try again" loads the file after the user repairs it.

Reason for TOML: the app reads and writes `settings.toml` with the `toml` crate. The model is two levels of arrays, which TOML holds.

Rejected: the app repairs what it can. A repair removes or moves Tiles with no question, and the user cannot see what the app changed.

Rejected: a load of one Dashboard at a time, as `add-node-graph` loads `graphs.json`. A Dashboard that is not valid then shows as read-only. The store would keep the raw text of that Dashboard through each write. A Graph that is not valid must not stop the Graphs that control a Curve. A Dashboard controls nothing.

Rejected: one shared store type for `dashboards.toml`, `graphs.json` and `history-views.toml`. The three files have different models and different fault rules in their specs. The shared parts are `write_atomic` and the rule for `schema_version`.

Rejected: tables `[[dashboards]]` in `settings.toml`, for the two effects above. Rejected: JSON. The other file of the settings directory that a user edits by hand, `settings.toml`, is TOML, and TOML already holds the model.

### P14. Code layout and reuse

| Path | Content |
|---|---|
| `crates/nicecooling/src/dashboards/mod.rs` | The model: Dashboard, Tile, kind, Tile limit, the count limits of P4 |
| `crates/nicecooling/src/dashboards/grid.rs` | The grid rules with no GPUI: the valid check, first free place, move, resize, reading order, the pixel geometry of P3 |
| `crates/nicecooling/src/dashboards/store.rs` | The file: load, the file states of P13, the valid check of each new state, the mark for the write, the selected Dashboard |
| `crates/nicecooling/src/dashboards/generate.rs` | The generator of P12, from the snapshot of the app state |
| `crates/nicecooling/src/dashboards/chart.rs` | The chart data of P9 with no GPUI: the interval, the key, the read of the store |
| `crates/nicecooling/src/screens/dashboards.rs` | The screen model: tabs, the view of each Tile with its state of P10 and its limit state, Edit mode, the undo list, the draft of the Tile settings |
| `crates/nicecooling/src/screens/dashboards_view.rs` | The GPUI view |
| `crates/nicecooling/src/widgets/ring_gauge.rs` | The ring gauge widget |

The app model (`AppModel` in `app.rs`) holds the Dashboard store as a field, next to `settings` and `history_path`. A test uses a store with no file path. That store keeps the Dashboards in memory. `AppSettings`, the content of `settings.toml`, gets no field.

Reason: `SettingsStore` is a value that the Settings screen copies at each render, and its copies can write. A Dashboard store inside it would be copied with it.

The screen reuses these parts:

- `time_chart` and `Reference` of `screens/leakshield/charts.rs`: the chart Tile.
- `load_series` of `history/query.rs`: the rows of a chart Tile.
- `picker_rows` and `picker_list` of `screens/sensors_view.rs`, `select_trigger` of `screens/ui.rs` and `opens_picker` of `screens/controls.rs`: the sensor picker, as the Derived sensor editor combines them.
- `shape_with` of `screens/leakshield/charts.rs`: the width of a tab label.
- `parse` of `screens/handlers.rs`: the number inputs of the Tile settings.
- The popover, the menu and `modal` of `screens/ui.rs`.
- `Locale::decimal`, `unit_text` and `digits` of `screens/format.rs`: the number and the default decimals. `digits` becomes public.
- `GAUGE_MAX` of `screens/leakshield/charts.rs`: the 450 mbar of P7 and P12. It becomes public.
- `last_value_time` of `widgets/loop_diagram.rs`: the time of the line of the last values.
- `open_history_for` of `screen_view.rs`: the click on a Tile.
- `write_atomic` of `app_settings.rs`: the write of the file.

New code that an existing part does not cover:

- The tab strip with its "More" menu, in `screens/ui.rs` (P2).
- The function of `history/query.rs` that reduces rows to time buckets (P9). `downsample` stays for the History screen and the Leakshield screen.
- The read task of the chart Tiles. It follows the form of `pressure_history` in `screens/leakshield/overview_view.rs`: a key, a pending flag and the time of the last read. That function reads one chart, and this change does not change it.

`time_chart` stays in `screens/leakshield/charts.rs`. A move to `widgets/` changes two files of the Leakshield screen and no behavior, so it is not in this change.

### P15. The design system gets rules for Tiles

A Dashboard has stat tiles, round gauges, small charts and red and amber limit states. `docs/ui/DESIGN.md` forbids the first three as decoration and keeps red and amber for an Alarm and a Warning. This change edits these rules of `docs/ui/DESIGN.md`:

- A Dashboard can hold a Tile, a ring gauge and a small chart, because the user puts them there. The fixed screens keep the rule.
- A Tile limit uses the Warning and the Alarm tokens as a tint, with the icon and a text. The solid Alarm field stays for an active Alarm.
- A Tile has the padding of P5.

This change also edits these documents:

- `docs/ui/widgets.md` gets the specs of the Tile grid and the ring gauge. Its common rule hides the edit handles of a widget while the daemon is offline. The rule gets one exception: the Tile grid keeps its handles, because it writes a file of the app and not the daemon.
- `docs/ui/screens/app-shell.md` gets one sentence in the row "Daemon offline" of its States table: the Dashboards still work. It also gets the sidebar entry and the Alt numbers of P2.
- The component map of `docs/ui/DESIGN.md` and `docs/ui/components.md` get the rows of the Dashboards screen.

The owner approves the screen file `docs/ui/screens/dashboards.md` and its two comps before a task builds the screen, as for the screens of `add-nicecooling-v1`.

Reason: the rules of the design system protect the fixed screens from decoration. A Dashboard is content that the user chose, and the rules for severity still hold on it.

Rejected: a Dashboard with no gauge and no chart Tile, so that no rule changes. Decision 3 asks for both kinds.

## Order with the other changes

Two other changes in `openspec/changes/` also change the app crate. `tasks.md` names the files that each overlap changes.

`add-node-graph` is in implementation, and the part of its code that this change needs is on `main`:

- `write_atomic(path, bytes, mode)` is in `app_settings.rs`. This change calls it with the mode `0o666` of a settings file and adds no second function. So this change starts from a `main` that holds that function.
- Both changes add one sidebar entry. P2 gives the order with both. The two edits of the shell do not depend on each other.
- The Graph sensor needs no code of this change (P6).
- The storage document, the glossary, the terms of the four languages and the component map each get a section or rows from each change.

`add-history-export` has its spec and no code. The two changes can land in either order (decision 14):

- `load_series` loses its argument `now` there, and every reader of the store moves to a read-only connection (its H4 and H5). This change has one read of the store, in `dashboards/chart.rs`. The change that lands second edits that one read.
- The time that the store keeps the 1 s samples becomes a setting of 24 hours or more (its H2). `load_series` then gives 1 minute rows for a range whose 1 s samples are gone (its H4). The chart Tile draws both (P9).
- A sensor that the app does not record (its H1) has no rows. Its chart Tile shows "No data in this time span." The reason text "Not recorded" for a Tile needs the list of that change. It is a follow-up when the code of both changes is on `main`.
- A delete of rows and a compaction (its H8 to H10) change the store. The next read of a chart shows the store as it is. A read task holds its connection only for the time of its reads.
- A history store of a newer schema (its H11) fails the read. The chart Tile shows "The history cannot be read."
- Both changes write steps for a downgrade into `docs/app.md`. The section "Downgrade" is one section with one paragraph per feature. The change that lands second adds its paragraph.

## Requirements of add-nicecooling-v1 that this change extends

`add-nicecooling-v1` is not archived, so `openspec/specs/` has no spec that a delta of this change can name. This change extends the contract in the table below, and a task edits `openspec/changes/add-nicecooling-v1/specs/desktop-app/spec.md`:

| Capability | Change |
|---|---|
| `desktop-app` | One new requirement that names the Dashboards screen and its file, and points to the capability `dashboards`. The requirement "App settings" stays: `settings.toml` does not change. |

Files of the contract that change: `docs/storage.md`, `docs/app.md`, `docs/ui/DESIGN.md`, `docs/ui/components.md`, `docs/ui/widgets.md`, `docs/ui/screens/app-shell.md`, `docs/ui/screens/loops.md`, `docs/i18n-terms.md` and `GLOSSARY.md`, and a new `docs/ui/screens/dashboards.md`. `docs/api.md` and the D-Bus XML do not change.

## Migration and rollback

This change converts no stored data.

- Forward: the app creates `dashboards.toml` at the first change of a Dashboard. A user with no Dashboard has no file.
- Back: an app without Dashboards never opens `dashboards.toml`. The file stays in the settings directory. `settings.toml` has no new key, so the older app reads and writes it as before.
- Forward again: the app loads the file, and the Dashboards are back.
- A later version that changes the format raises `schema_version` to 2, and so does a change of P4. This version then shows the newer-version state of P13 and does not write the file. A user who goes back to this version keeps the file of the newer version.
- A user removes the feature data with `rm "$XDG_CONFIG_HOME/nicecooling/dashboards.toml"`. The default path is `~/.config/nicecooling/dashboards.toml`. `docs/app.md` says this.

## Risks / Trade-offs

- [A Tile limit looks like an Alarm of the daemon] -> a tint and not the solid field, no entry in the list "Active Alarms", no banner, and the line in the Tile settings. The manual check has a step where the person tells the two apart.
- [`docs/ui/DESIGN.md` forbids tiles and round gauges] -> P15 edits the rule for a Dashboard, and the owner approves the screen file before a task builds the screen.
- [Twelve chart Tiles of 24 hours read about one million rows per minute] -> one read task at a time in the background, one read per minute for each chart of 24 hours, and one read for equal Tiles. The History screen already reads more for one live window.
- [A read of the history store fails] -> the chart Tile keeps the line of the last read, and the next interval reads again.
- [A sensor id changes when hardware changes] -> the Tile says "Sensor not found" with the id and keeps it. The user picks a sensor in the Tile settings.
- [A new sensor gets the id of a deleted one] -> the Tile shows the new sensor, and its title is the name of that sensor. A Tile with an own title keeps the title, and its tooltip has the name of the sensor.
- [The user edits the file by hand and makes a fault] -> the app does not write a file that is not valid, names the first fault, and has "Start with no Dashboard".
- [The user edits a valid file by hand while the app runs] -> the app reads the file at its start only, and its next change replaces the edit. `docs/app.md` says to stop the app first.
- [Two sessions of one user write the file] -> the last write wins and replaces the whole file, as for `settings.toml`. The write is atomic, so the file is never half written.
- [The app is killed within 1 s of a change] -> that change can be lost. A normal Quit writes it.
- [A drag does not scroll the grid] -> Alt with an arrow moves a Tile to a row that does not show, and the grid follows the selected Tile.
- [The app does not move a Tile onto another Tile, so a swap of two Tiles takes three moves] -> the grid has rows below every Dashboard for the middle step. The alternative is in the choices table.
- [A generated Dashboard does not follow a later change of the Loop] -> the user edits it as any Dashboard, or makes it again.
- [History and Settings lose their Alt number with one Leakshield] -> `docs/ui/screens/app-shell.md` gives the order, and Alt+1 stays the Overview.
- [`add-history-export` lets the user stop the recording of a sensor] -> the chart Tile of that sensor shows "No data in this time span." with no reason, until the follow-up of "Order with the other changes".

## Product choices that the owner can change

Decisions 1 to 14 do not fix these choices. The owner accepts every row. The spec has the option of each row. Each row names the other option.

| Choice | In the spec | Alternative |
|---|---|---|
| Row height (P3) | Fixed 60 px, gap 12 px | Square cells that grow with the window |
| Count limits (P4) | 16 Dashboards, 48 Tiles, 12 chart Tiles, 64 rows | Other numbers, or no limit |
| Tile sizes (P5) | The table of minimum and default sizes | Other sizes, after the comps |
| Graph sensor as a source (P6) | Yes, as any sensor of the list | A filter that hides it |
| Duty of a Channel as a source (P6) | No | A second source kind for a Channel |
| Default range of a gauge and a bar (P7) | A table by unit | Always 0 to 100 |
| Look of a limit state (P8) | Tint, icon and text | The solid Warning or Alarm field |
| Hysteresis of a Tile limit (P8) | None | A band of a fixed share of the range |
| Chart time span (P9) | 1 to 1440 minutes, five presets | Up to 48 hours, or a free number input |
| Tile of an offline daemon (P10) | The last value, dim | `--`, as the value bubbles of the Overview |
| A sensor that no longer exists (P10) | "Sensor not found" with the id, the Tile stays | The app removes the Tile |
| How Edit mode stores (P11) | Each change at once, with undo of 50 changes | A draft with "Save" and "Discard" |
| A move onto a Tile (P11) | The app does not make it | The other Tiles move down |
| Click on a Tile outside Edit mode (P11) | Opens History with the sensor | No action |
| Tiles of the generator (P12) | Gauge, bar, value, chart, value, value, with no Tile limit | Other kinds, or Tile limits from the Leakshield limits |
| Fans of a Loop (P12) | The Channels whose Curve reads the water temperature of the Loop, else all Channels that are not a Pump channel of any Loop, 8 at most | No fan Tile, or always all Channels that are not a Pump channel |
| Loop without a Leakshield (P12) | The Dashboard has no pressure and no fill level Tile | The command is off for such a Loop |
| Place of the generator command (P12) | The Loops screen and the "New Dashboard" menu | One of the two |
| A file that is not valid (P13) | Not shown, not written, with "Start with no Dashboard" | The app repairs what it can |
| Words in the other languages (task 1.1) | Dashboard is "Dashboard" in every language; Tile is "Riquadro", "Kachel", "Tuile" | Other words |

## Open Questions

None. Issue #89 holds the owner answers to the six questions of the review: decisions 10 to 14 and the place of the record.
