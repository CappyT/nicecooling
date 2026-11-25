# Design

## Context

This design belongs to the change of `proposal.md`. `GLOSSARY.md` defines the project terms. The spec of this change holds the numbers and the scenarios: `specs/dashboards/spec.md`. This document gives the decisions and their reasons.

`add-nicecooling-v1` gives the project two processes. The root daemon `nicecoolingd` owns the hardware, the Curves, the Alarms and the alarm response. The desktop app runs in the user session, and owns the window, the notifications and the history store (ADR-0005).

The app has these parts that this change builds on:

- The shell: a sidebar of pages, a header bar, global banners and a status bar (`crates/nicecooling/src/shell.rs`, `docs/ui/screens/app-shell.md`). A page with tabs puts a tab strip above its content, as the Leakshield page does.
- The Overview: one fixed screen with a card per Loop (`crates/nicecooling/src/screens/overview.rs`).
- The live values: one `SensorsUpdated` signal of the daemon per second, kept in the app state with the last values (`crates/nicecooling/src/model.rs`).
- The history store: `history.db`, 1 s samples for 48 hours, written every 10 s (`crates/nicecooling/src/history/`).
- The painters: lines, discs and rectangles on a canvas (`crates/nicecooling/src/widgets/paint.rs`), and a line chart over time with reference lines and gaps (`time_chart` in `crates/nicecooling/src/screens/leakshield/charts.rs`).
- The app settings: `settings.toml` in `$XDG_CONFIG_HOME/nicecooling/`, with an atomic write (`crates/nicecooling/src/app_settings.rs`).

The owner decisions that bind this design are these. This document cites them by number:

1. Pages of tiles on a cell grid of 12 columns. A tile has a column span and a row span. Tiles do not overlap. No layers, no free pixel canvas.
2. The fixed Overview stays as it is, and it stays the screen that opens on an Alarm. A page has a name and an ordered list of tiles.
3. The tile kinds are value, gauge, bar, chart and text.
4. Per tile: the sensor, an own title or the sensor name, decimals 0 to 3, a minimum and a maximum for gauge and bar, the time span for the chart, and two optional limits that color the tile with the Warning and Alarm colors of the theme.
5. An edit mode: add, remove, move, resize.
6. A generator that makes one page from a Loop, computed from the parts of the Loop.
7. The pages are stored in the app settings of the user.
8. Not in this change: an image tile, a control tile, free markup, desktop pages or own windows, import and export.

The standing owner rules also hold. No action asks a password. The app warns and never refuses a valid choice of the user. The app has no defense against another local user. Nothing new runs in the root daemon. A tile limit colors a tile only.

## Goals / Non-Goals

**Goals:**

- A user builds a page of the values that this user wants to watch, and the page is useful at once: every Tile has a value, and a chart has its line, when the page opens.
- The page holds its layout from a window of 1100x700 up, with no second layout.
- A Tile limit never looks like an Alarm of the daemon and never acts like one.
- A user who has no mouse can make every change.
- A broken file never costs the user the Dashboards and never costs the other app settings.
- The root daemon and the D-Bus contract do not change.

**Non-Goals:**

- The page files and the page model of the vendor program. The app imports none.
- A Tile that writes: a Duty, a Profile, a Leakshield Mode.
- A Tile that shows an image, a clock, free markup or the Loop diagram.
- A Dashboard outside the main window: on the desktop, or in its own window.
- Import and export of a Dashboard.
- A Tile source that is not a sensor: the Duty of a Channel, the active Profile, an Alarm state.
- Dashboards that several users share.
- A defense against another local user (a Non-Goal of `add-nicecooling-v1`).

## Terms

`GLOSSARY.md` gets these entries with this change. The spec, the code and the interface use them. The owner decisions say "page" and "tile". This design names a user page a Dashboard, because the app already has pages of other kinds: the pages of the shell and the display pages of the Leakshield.

| Term | Meaning |
|---|---|
| Dashboard | A page that the user builds from Tiles. It has a name and an ordered list of Tiles. Avoid: page, user page, overview page. |
| Tile | One box of a Dashboard: a value, a gauge, a bar, a chart or a text. It has a place and a size on the grid. Avoid: widget, item, card. |
| Tile limit | A warning limit or an alarm limit that the user sets on a Tile. It colors the Tile. It is not an Alarm and has no alarm response. |
| Edit mode | The state of the Dashboards screen in which the user adds, removes, moves and resizes Tiles. |

"Cell" is one column of one row of the grid. It needs no glossary entry. `docs/ui/widgets.md` keeps "widget" for a custom GPUI element, so a Tile is never a widget.

## Decisions

### P1. Dashboards are in the app only

The Dashboard model, its file and its screen are in the app crate `nicecooling`. A Dashboard reads what the app already has: the sensor list and the live values of the daemon events, and the history store. No command of a Dashboard calls the daemon, so no command asks polkit, and a user outside the group `wheel` edits Dashboards without a dialog.

Reason: a Dashboard is a view of one user. The daemon serves every user of the machine and has no view state.

Rejected: Dashboards in the state of the daemon. It adds members to the D-Bus contract and root code for data that only a window needs, against the owner rule that nothing new runs in the daemon.

### P2. One sidebar entry and one tab per Dashboard

The sidebar gets one entry, "Dashboards", directly after "Overview". The Dashboards screen has a tab strip: one tab per Dashboard in the stored order, and at its right end the commands "New Dashboard" and "Edit". Tabs that do not fit go into a "More" menu, by the tab overflow rule of `docs/ui/DESIGN.md`. The tab strip of the Leakshield page clips its tabs and has no such menu in the code, so this change builds the menu for its own strip. The Overview stays the first entry and the start screen, and an Alarm opens it (decision 2).

Reason: the sidebar at a window height of 700 px has room for about five more rows, and it does not scroll. The count of Dashboards is the choice of the user. A tab strip with an overflow menu holds 16 names at every window size.

Cost: Alt+2 opens Dashboards, so the entries after it move by one number. `Page::Dashboards` is one new variant of the shell page, and the selected Dashboard is a state of the screen.

Rejected: one sidebar entry per Dashboard in a group "Dashboards". It gives each Dashboard a place in the sidebar and its own shortcut, and it needs a sidebar that scrolls or a limit of about four Dashboards at the minimum window.

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

Reason for the fixed row height: the text styles of `docs/ui/DESIGN.md` have fixed sizes. A row that grows with the window would double the height of every Tile at 1920 px and leave the same text in it. A row of 60 px holds one caption line and one value line, which is the smallest useful Tile. The gap of 12 px is the card gap of the app.

Reason for one layout: `docs/ui/DESIGN.md` allows one layout from 1100x700 up. The grid always has 12 columns. The minimum sizes of P5 fit at the column width of 58.3 px, and a test draws them there in the four languages.

Rejected: square cells, where the row height is the column width. Rejected: a second layout with 6 columns for a narrow window. The window has a minimum width, so the narrow case does not exist.

### P4. Model and limits

A Dashboard has a name and a list of Tiles. A Tile has a kind, a column, a row, a column span, a row span and the settings of its kind. Neither has an id: a Dashboard is its position in the list, and a Tile is its place on the grid, which is unique because Tiles do not overlap.

The order of the Tile list is reading order: by row, then by column. Decision 2 asks for an ordered list. This order needs no extra field, and it is the order that the keyboard and a screen reader follow.

| Limit | Number | Reason |
|---|---|---|
| Dashboards of one user | 16 | The tab strip and its "More" menu stay short |
| Tiles in one Dashboard | 48 | 4 full rows of the smallest Tile. One frame draws 48 Tiles from values that the app holds |
| Chart Tiles in one Dashboard | 12 | Each chart Tile reads the history store (P9) |
| Columns | 12 | Decision 1 |
| Rows | 64 | A bound for the file. 64 rows are 4596 px |
| Column span | 1 to 12 | |
| Row span | 1 to 8 | 8 rows fill the minimum window |
| Name of a Dashboard, own title of a Tile | 40 characters | A tab and a Tile title truncate after that at the minimum window |
| Text of a text Tile | 200 characters | A note, not a document |
| Chart time span | 1 to 1440 minutes | P9 |
| Decimals | 0 to 3 | Decision 4 |

The vendor program has no limits. These limits bound the file, the draw time of one frame and the read load on the history store.

A limit stops the command that adds, with the reason in the tooltip of the command. The owner rule "the app warns and never refuses" is about a choice that is valid and unsafe. A 17th Dashboard is not unsafe: it is outside the model.

### P5. Tile kinds and sizes

| Kind | Shows | Minimum size | Default size |
|---|---|---|---|
| value | title, number, unit | 2x1 | 3x1 |
| gauge | title, a ring of 270 degrees with the number and the unit in it | 2x2 | 3x3 |
| bar | title, number, unit, a horizontal bar on a track | 2x1 | 4x1 |
| chart | title, the newest value, a line over the last N minutes | 3x2 | 6x3 |
| text | a heading (one line) or a note (wrapped) | 1x1 | 4x1 |

Sizes are columns by rows. The minimum size is the smallest size at which the Tile shows its value with no cut at the column width of 58.3 px. A Tile is a card with 12 px of padding. A heading has no card ground, so it reads as a title between rows of Tiles.

A value Tile and a gauge Tile use the numeral text style when they have room, and the value style when they are small: a value Tile from 2 rows up, a gauge Tile from 3 columns and 3 rows up. The screen file gives the geometry.

The gauge is a ring, drawn as a stroked arc of straight segments with the polyline painter of `widgets/paint.rs`. It is one new custom widget, `widgets/ring_gauge.rs`. The pressure gauge of the Leakshield page is a horizontal scale with a band for one fixed range, and it stays as it is. The bar is a track and a filled rectangle. The chart is the `time_chart` painter with its reference lines and its runs of points.

The filled part of a ring, a bar and the line of a chart have the first series color `s1`. `docs/ui/DESIGN.md` keeps blue for data and red and amber for severity.

Rejected: one kind with a style option that picks a text, a bar, a chart or a gauge, as the vendor program has. Five kinds with one job each need no style option, and each has a short dialog.

### P6. The source of a Tile is a sensor

A Tile with a value shows one sensor of the sensor list of the daemon. The Tile stores the sensor id. The sensor picker of the app lists the sensors, and it has no filter by origin.

- A Graph sensor of the change `add-node-graph` is in the sensor list of the daemon (its decision G4). A Tile takes it as any sensor, with no code of this change and no dependency on that change. While the app does not feed it, it has no value, and the Tile shows `--`.
- A Derived sensor and the calibrated flow of a Loop are sensors of the list, so a Tile takes them.
- The Duty of a Channel is not a sensor of the list. A Tile does not show it. The speed of a Channel is a sensor, and a Tile shows it.

Reason: one source kind gives one picker, one value path and one rule for a missing value.

Rejected: a filter that hides Graph sensors. It needs a reason that this design does not have.

### P7. Title, number and range

- Title: the own title of the Tile, or the name of the sensor when the own title is empty. The Tile does not store the sensor name, so a rename shows at once.
- Number: 0 to 3 decimals, with the number format of the app (`Locale::decimal`) and the unit text of the sensor (`unit_text`). A new Tile takes the decimals that the app uses for the unit of its sensor: 1 for `degC`, `V`, `A`, `W` and `mbar/min`, else 0.
- Range of a gauge and a bar: a minimum and a maximum that the user sets, with the maximum above the minimum. A new Tile takes a range from the unit: 0 to 100 for `%` and `degC`, 0 to 3000 for `rpm`, 0 to 300 for `l/h`, 0 to 450 for `mbar` (the scale of the pressure gauge of the app), else 0 to 100.
- A value outside the range fills the ring or the bar fully or not at all. The number is always the real value.

The Tile settings refuse a maximum that is not above the minimum: such a range has no scale. This is a value that is not valid. It is not a choice that the app refuses.

### P8. A Tile limit is a look, not an event

A Tile has an optional warning limit and an optional alarm limit. Each has a direction, "above" or "below", and a value. The comparison is strict, and the alarm limit wins when both hold.

The look of a limit state:

| Part | Warning state | Alarm state |
|---|---|---|
| Ground of the Tile | `warning-tint` | `alarm-tint` |
| Icon before the title | filled triangle | filled octagon |
| Number | `warning` | `alarm` |
| Filled part of a ring, bar | `warning-bg` | `alarm-bg` |
| Line of a chart | `s1`, unchanged | `s1`, unchanged |
| Limit lines of a chart | dashed, `warning` | dashed, `alarm` |
| Text | "above 35.0 °C" as a caption from 2 rows up, and always in the tooltip and the accessible text |

- The Tile never has the solid Alarm field. `docs/ui/DESIGN.md` keeps that field for an active Alarm and for "Daemon down". A tint, an icon and a text are the three signals that the severity system asks for.
- A Tile limit raises nothing in the daemon and nothing in the shell: no Alarm, no Warning, no notification, no window, no tray state, no status bar text. The Tile settings say so in one line.
- The app evaluates a Tile limit only for a live value of an online daemon. A Tile with no value, and a Tile of an offline daemon, is in no limit state.
- No hysteresis. A value that moves around a limit changes the look each second. The look has no cost, so this design adds no state for it.
- The generator sets no Tile limit. The limits of the Leakshield belong to its Profile slot, and a copy in a Tile would go stale.

Reason for the theme colors: decision 4. Reason for the tint and not the solid field: the user must tell an Alarm of the daemon, which can power the PC off, from a color that this user set on a Tile.

Rejected: a free list of colors per value, as the vendor program has. It breaks the rule that red and amber mean severity. Rejected: a Tile that marks a sensor with an active Alarm of the daemon. Decision 4 asks for limits of the user. The Overview already marks the parts of a Loop.

### P9. A chart reads the history store

A chart Tile draws the 1 s samples of the history store over its time span of 1 to 1440 minutes. The Tile settings offer 5 minutes, 15 minutes, 1 hour, 6 hours and 24 hours. The file takes every whole number in the range.

- The line has data when the Dashboard opens, because the history store records every sensor while the app runs, with or without a window (decision 3).
- The store writes every 10 s, so the line ends up to 10 s before the present time. The header of the Tile shows the newest live value.
- The read runs on the background executor, as the History screen does, and never in the drawing of a frame. A chart Tile reads again at an interval of N / 4 whole seconds for a span of N minutes, between 2 s and 60 s: 2 s for 5 minutes, 15 s for 1 hour, 60 s for 24 hours. At a width of 400 px, one pixel of a 1 hour chart is 9 s.
- Two chart Tiles with the same sensor and the same span share one read.
- The read gives at most 600 points for one unbroken part of the line, made with `downsample` of `history/query.rs`. A gap in the samples is a break in the line.
- The limit of 1440 minutes keeps every chart inside the 48 hours of 1 s samples, so a chart has one resolution. A read of 24 hours is 86 400 rows. Twelve chart Tiles of 24 hours read about one million rows per minute, in the background.
- An error of the history store gives an empty plot. The History screen names the error.

Rejected: a value history in memory, filled while the window is open, as the vendor program has. It is empty after each start. Rejected: spans above 24 hours. The History screen has them, with its zoom and its two resolutions.

### P10. What a Tile shows without a live value

| Case | The Tile shows |
|---|---|
| The sensor has no value | `--` without a unit and the dashed-circle icon. A ring and a bar have their track only. A chart line ends where the value stopped. No limit state |
| The sensor is not in the sensor list | "Sensor not found" and the stored sensor id. No value, ring, bar or line. The Tile keeps the id |
| Daemon offline or down | The last value in `fg-dim` with its unit. No limit state. One line above the grid: "Last values 21:40:12". A chart still draws the stored history |
| No answer of the daemon in this run | `--`. No Tile says "Sensor not found", because the app has no sensor list to compare with |
| The app connects at its start | The frame of each Tile with a grey bar, for at most 2 s |
| An Alarm is raised | The window shows the Overview (decision 2). Edit mode ends |

- "Sensor not found" keeps the id, so the Tile works again when the sensor returns: a Derived sensor that the user makes again with the same id does not, and a device that comes back does.
- The offline case follows the common rule of `docs/ui/widgets.md`: a widget shows its last values in `fg-dim`.
- The global banners of the shell show above the tab strip, as on every screen.

### P11. Edit mode stores each change at once

"Edit" turns Edit mode on. The screen then shows the cells of the grid as outlines, a toolbar and the selection. Each command stores the Dashboards when it ends. Edit mode keeps the last 50 changes of its session for "Undo" and "Redo". "Done", Escape and a change to another screen end Edit mode and clear the list.

Reason: an Alarm opens the Overview at any time (decision 2). A draft that only "Done" stores would be lost then, or would need a question on top of an Alarm. The other screens of the app also apply a change when a drag ends or an input loses the focus.

Rejected: a draft with "Save" and "Discard".

Move and resize:

- A drag of a Tile moves it. A drag of the handle at the lower right corner resizes it. The target is the cell nearest to the pointer.
- A move is made only onto free cells of the grid. Else the Tile goes back. During the drag, an outline shows the target: solid in the accent color when the move is possible, dashed in `border-strong` when it is not. The dashed outline is the "not confirmed" sign of the design system. It uses no severity color.
- A resize stops at the last size that is free, at the minimum size of the kind, at the right edge and at 8 rows.
- Nothing moves other Tiles, and nothing closes an empty row. The user decides each place.
- A new Tile goes to the first free place in reading order, with the default size of its kind. The generator uses the same rule.

Rejected: a move that pushes other Tiles down. It changes places that the user did not touch, and it needs rules for chains of Tiles.

Keyboard. The grid is one focus stop, as `docs/ui/widgets.md` asks for a custom widget. The keys follow the LED timeline of the app, where Alt with an arrow moves a bar and Shift with an arrow changes its length.

| Key | Outside Edit mode | In Edit mode |
|---|---|---|
| Left, Right | Select the previous or the next Tile in reading order | The same |
| Up, Down | Select the nearest Tile above or below that shares a column | The same |
| Enter | Open History with the sensor of the Tile | Open the Tile settings |
| Alt + arrow | | Move the Tile by one cell |
| Shift + Right, Shift + Down | | Make the Tile one cell larger |
| Shift + Left, Shift + Up | | Make the Tile one cell smaller |
| Delete | | Remove the Tile |
| Insert | | Open "Add Tile" |
| Ctrl+Z, Ctrl+Shift+Z | | Undo, redo |
| Escape | Close a menu | Close a dialog or a menu, else end Edit mode. During a drag: put the Tile back |
| Tab | Leave the grid | Leave the grid |

The tab strip, "New Dashboard", "Edit" and the toolbar buttons are focus stops before the grid. A user turns Edit mode on with Tab and Enter.

The Tile settings are a dialog of 480 px, as the Derived sensor editor: "Kind", "Sensor" (the sensor picker), "Title", "Decimals", "Minimum" and "Maximum" for gauge and bar, "Time span" for chart, "Warning limit" and "Alarm limit" (a switch, a choice "above" or "below", a number with the unit), the line of P8, and "Remove", "Cancel" and "Save". A text Tile has "Kind", "Text" and "Style". "Save" is one change. A removed Tile needs no question, because "Undo" brings it back. "Delete Dashboard" asks, because it removes many Tiles.

### P12. The generator computes Tiles from the parts of a Loop

"Create a Dashboard from this Loop" makes a normal Dashboard. It has no link to the Loop afterwards.

| Order | Part of the Loop | Condition | Tile |
|---|---|---|---|
| 1 | Pressure | The Loop has a Leakshield and the sensor `<leakshield id>_pressure` exists | gauge 3x3, range 0 to 450 |
| 2 | Fill level | The Loop has a Leakshield and the sensor `<leakshield id>_fill_level` exists | bar 3x1, range 0 to 100 |
| 3 | Flow | The Loop has a calibrated flow | value 3x1 |
| 4 | Water temperature | The Loop has a water temperature sensor | chart 6x2, 60 minutes |
| 5 | Pump speed | Each Pump channel of the Loop that has a speed sensor | value 3x1, own title: the name of the Channel |
| 6 | Fans | Each fan Channel of the Loop that has a speed sensor, 8 at most | value 3x1, own title: the name of the Channel |

Each Tile goes to the first free place, in this order. For a Loop with every part, one pump and two fans, the rows 0 to 2 are full: the gauge at the left, the bar, the flow and the pump speed in row 0, the chart under the bar and the flow, and the fans at the right.

- The water temperature is one chart Tile. Its header shows the newest value, so the Dashboard needs no second Tile for it.
- A Loop model has no fans. The fan Channels of a Loop are the Channels that are not a Pump channel of any Loop and whose Curve reads the water temperature sensor of the Loop, directly or through a Derived sensor. When no Channel matches, they are all Channels that are not a Pump channel. A machine with one Loop and Hardware curves so gets its fans.
- A Loop with no Leakshield gets no pressure Tile and no fill level Tile. The generator makes the rest and does not refuse.
- A Tile of a Channel takes the name of the Channel as its own title, because the speed sensor of a Channel has its own name, which says less.
- The command is in the Loops screen, in the menu of the Loop header, and in the menu of "New Dashboard", as one row per Loop.

Reason for a computed layout: decision 6. A fixed template per device, as the vendor program has, does not fit hwmon hardware that differs per Board.

Rejected: a pressure Tile and a fill level Tile with no sensor for a Loop without a Leakshield. A Tile that can never have a value is noise.

### P13. Storage: one file in the settings directory of the user

The Dashboards are in `$XDG_CONFIG_HOME/nicecooling/dashboards.toml`, next to `settings.toml`. The app is the only writer. It writes with the atomic write of `docs/storage.md`: a temporary file, `fsync`, rename, `fsync` of the directory. `docs/storage.md` gets the format before the code that writes it.

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

- The order of `dashboards` is the tab order. The app writes `tiles` in reading order.
- A field that a kind does not use is absent. `title` defaults to empty, `decimals` to 0, `span_min` to 60, `style` to `note`. `warning` and `alarm` are absent when the limit is off.
- The loader reads `schema_version` first. A higher number than 1 means a file of a newer app: the app does not show it and does not write it, as the common rule of `docs/storage.md` says for the daemon.
- A file is valid when it parses and every Dashboard and Tile obeys the limits of P4, the grid rules and the minimum sizes of P5. The app does not repair a file. A file that is not valid is not shown and not written. The screen shows the path and the first fault, with "Try again" and "Start with no Dashboard". The second command renames the file to `dashboards.toml.bad`, so the user starts again and loses nothing.
- A missing file means no Dashboard. The app creates the file at the first change.
- A failed write keeps the change in memory and shows the error. The next change writes again.

Reason for an own file, and how this reads decision 7: the app settings of the user are the files of the app in the settings directory of the user. `settings.toml` is one of them. An own file has two effects that a table in `settings.toml` does not have:

- Downgrade. An app without Dashboards parses `settings.toml`, ignores a table that it does not know, and writes the file without it at the next settings change. The Dashboards would be gone. With an own file, an older app never opens it.
- Fault isolation. `AppSettings::load` gives the defaults for a file that does not parse. A fault in one Tile would reset the language, the tray switch and the hidden sensors.

Reason for TOML: the app has the `toml` crate, and the settings directory then has one format. The model is two levels of arrays, which TOML holds.

Rejected: tables `[[dashboards]]` in `settings.toml`, for the two effects above. Rejected: JSON. It adds a dependency to the app for no gain.

### P14. Code layout and reuse

| Path | Content |
|---|---|
| `crates/nicecooling/src/dashboards/mod.rs` | The model: Dashboard, Tile, kind, Tile limit, the limits of P4 |
| `crates/nicecooling/src/dashboards/grid.rs` | The grid rules with no GPUI: valid place, first free place, move, resize, reading order, the pixel geometry of P3 |
| `crates/nicecooling/src/dashboards/store.rs` | The file: load, the valid check, write, the states of P13 |
| `crates/nicecooling/src/dashboards/generate.rs` | The generator of P12, from the snapshot of the daemon |
| `crates/nicecooling/src/screens/dashboards.rs` | The screen model: tabs, the view of each Tile with its state of P10 and its limit state, Edit mode, the undo list, the draft of the Tile settings |
| `crates/nicecooling/src/screens/dashboards_view.rs` | The GPUI view |
| `crates/nicecooling/src/widgets/ring_gauge.rs` | The ring painter |

The settings store of the app (`SettingsStore` in `app_settings.rs`) holds the Dashboard store as a field, so the window and the tests get it on the path that exists. `AppSettings`, the content of `settings.toml`, gets no field. A store with no path, which a test uses, keeps the Dashboards in memory.

The screen reuses: `time_chart` and `Reference` of `screens/leakshield/charts.rs` for the chart Tile, `load_series` and `downsample` of `history/query.rs` for its data, the sensor picker (`select_field` in `screens/controls.rs`, `picker_list` in `screens/sensors_view.rs`), the dialog, the popover and the menu of `screens/ui.rs`, the tab strip pattern of the Leakshield page (`screens/leakshield/frame_view.rs`), `Locale::decimal` and `unit_text` of `screens/format.rs` for the number, and `open_history_for` of `screen_view.rs` for the click on a Tile. The atomic write of `AppSettings::save` becomes one public function that both stores call.

### P15. The design system gets a rule for Tiles

`docs/ui/DESIGN.md` forbids stat tiles, round gauges and sparklines as decoration, and red and amber for anything that is not an Alarm or a Warning. A Dashboard has all of these. This change edits the rules, and it does not break them:

- A Tile, a ring gauge and a small chart are allowed on a Dashboard, because the user put them there. The fixed screens keep the rule.
- A Tile limit uses the Warning and the Alarm tokens as a tint, with the icon and a text. The solid Alarm field stays for an active Alarm.
- `docs/ui/widgets.md` gets the specs of the Tile grid and the ring gauge. The component map gets the rows of the Dashboards screen.

The owner approves the screen file `docs/ui/screens/dashboards.md` and its two comps before the screen is built, as for the screens of `add-nicecooling-v1`.

## Product choices that the owner did not close

The owner decisions leave these choices open. The spec has the recommended option. Each row names what changes with the other option.

| Choice | In the spec | Alternative |
|---|---|---|
| Name of a user page | Dashboard, with Tile and Tile limit | "Page" as in the decisions. It collides with the pages of the shell and of the Leakshield display |
| Navigation (P2) | One sidebar entry "Dashboards" and one tab per Dashboard | One sidebar entry per Dashboard, with a sidebar that scrolls |
| Row height (P3) | Fixed 60 px, gap 12 px | Square cells that grow with the window |
| Limits (P4) | 16 Dashboards, 48 Tiles, 12 chart Tiles, 64 rows | Other numbers, or no limit |
| Tile sizes (P5) | The table of minimum and default sizes | Other sizes, after the comps |
| Graph sensor as a source (P6) | Yes, as any sensor of the list | A filter that hides it |
| Duty of a Channel as a source (P6) | No | A second source kind for a Channel |
| Default range of a gauge and a bar (P7) | A table by unit | Always 0 to 100 |
| Look of a limit state (P8) | Tint, icon and text | The solid Warning or Alarm field |
| Hysteresis of a Tile limit (P8) | None | A band of a fixed share of the range |
| Chart time span (P9) | 1 to 1440 minutes, five presets | Up to 48 hours, or a free number input |
| Tile of an offline daemon (P10) | The last value, dim | `--`, as the bubbles of the Overview |
| A sensor that no longer exists (P10) | "Sensor not found" with the id, the Tile stays | The app removes the Tile |
| How Edit mode stores (P11) | Each change at once, with undo of 50 changes | A draft with "Save" and "Discard" |
| A move onto a Tile (P11) | Not made | The other Tiles move down |
| Click on a Tile outside Edit mode | Opens History with the sensor | No action |
| Tiles of the generator (P12) | Gauge, bar, value, chart, value, value, with no Tile limit | Other kinds, or Tile limits from the Leakshield limits |
| Fans of a Loop (P12) | The Channels whose Curve reads the water temperature of the Loop, else all fan Channels, 8 at most | No fan Tile, or always all fan Channels |
| Loop without a Leakshield (P12) | The Dashboard has no pressure and no fill level Tile | The command is off for such a Loop |
| Place of the generator command (P12) | The Loops screen and the "New Dashboard" menu | One of the two |
| File (P13) | An own file `dashboards.toml` in the settings directory | Tables in `settings.toml` |
| A file that is not valid (P13) | Not shown, not written, with "Start with no Dashboard" | The app repairs what it can |
| Words in the other languages | Dashboard stays; Tile is "Riquadro", "Kachel", "Tuile" | Other words |

## Requirements of add-nicecooling-v1 that this change extends

`add-nicecooling-v1` is not archived, so this change has no delta against `openspec/specs/`. It extends this contract, and a task edits it:

| Capability | Change |
|---|---|
| `desktop-app` | One new requirement that names the Dashboards screen and its file, and points to the capability `dashboards`. The requirement "App settings" stays: `settings.toml` does not change. |

Files of the contract that change: `docs/storage.md`, `docs/app.md`, `docs/ui/DESIGN.md`, `docs/ui/components.md`, `docs/ui/widgets.md`, `docs/ui/screens/app-shell.md`, `docs/ui/screens/loops.md`, `docs/i18n-terms.md` and `GLOSSARY.md`, and a new `docs/ui/screens/dashboards.md`. `docs/api.md` and the D-Bus XML do not change.

## Migration and rollback

This change converts no stored data.

- Forward: the app creates `dashboards.toml` at the first change of a Dashboard. A user with no Dashboard has no file.
- Back: an app without Dashboards never opens `dashboards.toml`. The file stays in the settings directory. `settings.toml` has no new key, so the older app reads and writes it as before.
- Forward again: the app loads the file, and the Dashboards are back.
- A later version that changes the format raises `schema_version` to 2. This version then shows the notice of P13 and does not write the file. A user who goes back to this version keeps the file of the newer one.
- A user removes the feature data with `rm ~/.config/nicecooling/dashboards.toml`. `docs/app.md` says this.

## Risks / Trade-offs

- [A Tile limit looks like an Alarm of the daemon] -> a tint and not the solid field, no entry in Active Alarms, no banner, and the line in the Tile settings. The manual check has a step where the person tells the two apart.
- [`docs/ui/DESIGN.md` forbids tiles and round gauges] -> P15 edits the rule for a Dashboard, and the owner approves the screen file before the screen is built.
- [Twelve chart Tiles of 24 hours read one million rows per minute] -> the read is in the background, at most once per minute per chart, and shared between equal Tiles. A task measures it on the reference desktop.
- [A sensor id changes when hardware changes] -> the Tile says "Sensor not found" with the id and keeps it. The user picks a sensor in the Tile settings.
- [The user edits the file by hand and breaks it] -> the app does not write a file that is not valid, names the first fault, and has "Start with no Dashboard".
- [Two sessions of one user write the file] -> the last write wins, as for `settings.toml`. The write is atomic, so the file is never half written.
- [A move onto a Tile is not made, so a swap of two Tiles takes three moves] -> the grid has rows below every Dashboard for the middle step. The alternative is in the choices table.
- [The generator makes a Dashboard that goes stale when the Loop changes] -> it is a normal Dashboard, and the user edits it or makes it again.
- [The Alt numbers of the sidebar entries after "Overview" change] -> `docs/ui/screens/app-shell.md` gives the new order, and Alt+1 stays the Overview.

## Open Questions

None that change the spec or the tasks. The owner can change each row of the choices table before the tasks start.
