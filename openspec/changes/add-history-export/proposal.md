# Proposal

## Why

The app records every sensor and draws the result in the History screen. The user cannot take the data out, cannot remove a bad test run, cannot choose how long the data stay, and loses the chart setup when the app closes. The owner wants these four functions in the History: export, delete, retention settings and a chart manager.

## What Changes

- The app records every sensor and every Channel, as it does now. A switch per sensor stops the recording of that sensor.
- The Sample period and the Minute period become settings with fixed choices. The 1 s samples stay for 24 hours, 48 hours, 7 days or 30 days. The 1 minute rows stay for 30 days, 90 days, 1 year, 2 years or 5 years. The defaults are 48 hours and 1 year. The app stores the two periods in `history.db`, with the data that they keep.
- The History screen exports the shown range and the shown Series to one CSV file: comma separator, decimal point, time as ISO 8601 with the UTC offset, UTF-8, one header row, one row per time stamp, one column per Series. The export writes the 1 s samples where the store holds them, else the 1 minute rows. When samples exist, the user can also pick the minute rows. The minute rows have the minimum and the maximum as an option.
- The History screen deletes the rows of a time range, for the shown Series or for all Series. The Settings screen deletes the whole history. Each delete has a confirmation that names the number of rows.
- A View is a saved chart setup: a name, the Series, the range length, the style of each Series (color, line width, fill) and the axis limits per unit.
- A Note is a text at a time of the history. Every chart of the History screen that shows that time draws it.
- The History screen saves the chart as a PNG image.
- A database with a higher schema version than the app knows is not written.
- No automatic export to a file. No recording in the root daemon. No new D-Bus member. No action asks a password.

## Capabilities

### New Capabilities

- `sensor-history`: what the app records, the recording switch per sensor, the Sample period and the Minute period, the choice between Samples and Minute rows, the CSV export, the delete of a range and of the whole history, the compaction of the database file and the schema version rule.
- `history-chart`: the Views, the Series styles, the axis limits per unit, the Notes, and the image of the chart.

### Modified Capabilities

None. The capabilities of `add-nicecooling-v1` are not archived yet. This change depends on that change and extends its capability `desktop-app`. `design.md` lists the extended requirement.

## Impact

- Code: `crates/nicecooling` only. The history store (`src/history/`), the History and Settings screens, the two Overview charts and the sparkline of the Sensors screen (they read the store), the app settings and the service thread of the history. No change in `nicecoolingd`, `nicecooling-api` or `nicecooling-core`.
- API: none.
- Dependencies of the app: `resvg` as a direct dependency, at the version that `gpui-pre` builds already, and `rustix` with its feature `fs` for the read of the free disk space, at the version of the workspace. The lock file gets no new crate. The daemon gets no new dependency.
- Storage: schema version 2 of `history.db` (one new table for the Notes and two keys for the periods), one new table `[history]` in `settings.toml` for the list of not recorded sensors, and the new file `history-views.toml` in the configuration directory of the user.
- Documents: `docs/storage.md`, `docs/app.md`, `docs/ui/screens/history.md`, `docs/ui/screens/settings.md`, `docs/ui/DESIGN.md`, `docs/ui/components.md`, `docs/i18n-terms.md`, `GLOSSARY.md`, and two procedure files in `docs/hardware-checks/`.
