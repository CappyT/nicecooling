# Proposal

## Why

nicecooling has one fixed Overview. It answers one question: is every Loop safe now. A user who wants to watch other values, a GPU temperature next to a fan speed, has no screen for them: the Sensors screen lists every value, and the History screen shows charts only. The owner wants pages that the user builds, as the vendor program has them, in a form that fits a window that changes its size.

## What Changes

- A **Dashboard** is a page that the user builds. It has a name and an ordered list of Tiles. The fixed Overview does not change, and it stays the screen that opens on an Alarm.
- A **Tile** is one box on a grid of 12 columns. It has a column span and a row span. Tiles do not overlap. A Dashboard has no layers and no pixel positions.
- Five Tile kinds: value (number, unit, name), gauge (a ring with a minimum and a maximum), bar, chart (a line over the last N minutes) and text (a heading or a note).
- A chart Tile reads the history store of the app, so it has data when the Dashboard opens.
- Each Tile has a sensor, an own title or the name of the sensor, 0 to 3 decimals, a range for gauge and bar, and a time span for the chart.
- Each Tile has two optional **Tile limits**: a warning limit and an alarm limit, each "above" or "below" a value. A Tile limit colors its Tile with the Warning or the Alarm color of the theme. It is not an Alarm of the daemon, and it runs no alarm response.
- An **Edit mode** adds, removes, moves and resizes Tiles, with the mouse and with the keyboard. It stores each change at once and has undo and redo.
- A generator, "Create a Dashboard from this Loop", makes one Dashboard with the pressure, the fill level, the flow, the water temperature, the pump speed and the fans of a Loop. It computes the Tiles from the parts of the Loop.
- The sidebar gets one entry, "Dashboards", after "Overview". The screen has one tab per Dashboard.
- The app stores the Dashboards of a user in a file of the settings directory of that user.
- Not in this change: an image Tile, a control Tile, free markup, a Dashboard on the desktop or in its own window, import and export of a Dashboard.

## Capabilities

### New Capabilities

- `dashboards`: the Dashboard and Tile model, the grid, the five Tile kinds, the Tile limits, what a Tile shows without a value, the Edit mode, the generator from a Loop, the navigation and the storage.

### Modified Capabilities

None. The capabilities of `add-nicecooling-v1` are not archived yet. This change depends on that change and extends one of its contracts, `desktop-app`. `design.md` lists the extended requirements.

## Impact

- Code: the app crate `nicecooling` only. A new module for the Dashboard model, the grid rules, the store, the generator and the chart data, a new screen, and one new custom widget (the ring gauge). The shell gets one screen. `history/query.rs` gets a function that reduces rows to time buckets, and `screens/ui.rs` gets a tab strip with a "More" menu.
- The root daemon `nicecoolingd`, the crates `nicecooling-api`, `nicecooling-core` and `nicecooling-protocol`, and the D-Bus contract do not change.
- Dependencies: none new. The store uses the `toml` crate that the app has, and `write_atomic` of `app_settings.rs`.
- Other changes: `add-node-graph` and `add-history-export` change the same crate. `design.md` gives the order.
- Storage: one new file, `$XDG_CONFIG_HOME/nicecooling/dashboards.toml`. `settings.toml` and `history.db` do not change.
- Design system: `docs/ui/DESIGN.md` gets a rule for Tiles and for Tile limits, because it forbids stat tiles and round gauges as decoration and keeps red and amber for an Alarm and a Warning.
- Reverse engineering source: the feature study of aquasuite X.96, section 4, kept outside the repository.
