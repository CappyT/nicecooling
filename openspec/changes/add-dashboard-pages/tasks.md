# Tasks

In this file, a Dashboard is a page that the user builds, and a Tile is one box of it. `design.md` has the decisions P1 to P15. `specs/dashboards/spec.md` has the requirements. "The Dashboard store" is the type `DashboardStore` of task 1.5. "The history store" is `history.db`.

Paths are relative to the repository root. Each path that a task changes has the marker "Create" or "Modify". A path with no marker is a reference, and the task does not change it. A test name in a task is a test that the task creates. A test marked "(existing)" exists already and must still pass. Each test name belongs to one task only.

This change changes the app crate `crates/nicecooling` only. No task changes a file under `crates/nicecoolingd`, `crates/nicecooling-api`, `crates/nicecooling-core` or `crates/nicecooling-protocol`, and no task changes `docs/api.md`.

This change starts from a `main` that holds the function `write_atomic(path: &Path, bytes: &[u8], mode: u32)` in `crates/nicecooling/src/app_settings.rs`. Before task 1.1, run `rg -c 'pub fn write_atomic' crates/nicecooling/src/app_settings.rs`. When it does not print 1, stop and report: the branch is not on that `main`.

`add-node-graph` and `add-history-export` change files that these tasks also change. A task adds its lines next to the lines of those changes and removes none of them. Tasks 2.4, 4.1 and 6.1 need more than that. Each of them says what to do.

A task names a new type or function when a task of another section or a test of another file uses it. The implementer names the others.

A task marked MANUAL needs a person on the reference desktop. The person follows a procedure file in `docs/hardware-checks/`. The file lists steps, the result that the person sees, and one pass or fail line per step. A MANUAL task passes when every step has a pass line.

Each section ends with a checkpoint task. The checkpoint command is:
`cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check && python3 -I tools/re/check-identifiers.py && openspec validate add-dashboard-pages --strict`.

Each task adds its strings to the four locale files `crates/nicecooling/locales/en.yml`, `it.yml`, `de.yml` and `fr.yml`. The strings go under the new top-level key `dashboards`. The sidebar entry goes under `nav.dashboards`. The four files keep the same key order. The tests `every_key_has_every_language`, `every_language_keeps_the_order_of_the_keys` and `every_key_used_in_source_exists` in `crates/nicecooling/tests/i18n.rs` (existing) check them. The English text of a string is the quoted text in the spec. Task 3.1 writes the English text and the Italian text of each string into the screen file, and the owner approves that file in task 3.4. The implementer writes the German and the French text with the terms of `docs/i18n-terms.md`.

A new integration test file of the app is under `crates/nicecooling/tests/` and starts with `mod support;`. A test that needs a window builds it as `start_with` in `crates/nicecooling/tests/keyboard.rs` does: the `Backend` gets a `commands` sender whose receiver the test keeps. So the test sees each command that the window sends. The fixture `full()` of `crates/nicecooling/tests/support/fixtures.rs` gives:

- the Loop "Main loop" with a Leakshield;
- the calibrated flow `loop_1_flow`;
- the water temperature sensor `nct6798_temp1`;
- the Pump channel "Waterpump" with the speed sensor `nct6798_fan6`;
- the Channel "Chassis Fan 1" with no speed sensor.

## 1. Terms, storage contract, model, grid rules and store

- [ ] 1.1 Modify `GLOSSARY.md`: add a section "Dashboards" with the entries Dashboard, Tile, Tile limit and Edit mode. Each entry has the text of the design table "Terms". Dashboard and Tile also get the "Avoid" line of that table. Modify `docs/i18n-terms.md`: the rows Dashboard (Dashboard, Dashboard, Dashboard), Tile (Riquadro, Kachel, Tuile), Tile limit (Limite del riquadro, Kachelgrenze, Limite de tuile) and Edit mode (Modalità modifica, Bearbeitungsmodus, Mode édition). This task comes before the first string. Verify: `rg -c '^\*\*(Dashboard|Tile|Tile limit|Edit mode)\*\*' GLOSSARY.md` prints 4, and `rg -c '^\| (Dashboard|Tile|Tile limit|Edit mode) \|' docs/i18n-terms.md` prints 4.
- [ ] 1.2 Modify `docs/storage.md`: add a section "App Dashboards: `$XDG_CONFIG_HOME/nicecooling/dashboards.toml`" directly before the section "App history". This task comes before the code that writes the file. The section has:
  - the example file of design P13;
  - a table with one row per field: the name in backticks, the type, the range, the default, and the mark "required" for a field with no default;
  - the rules of design P13 for `schema_version`, the fields that the app writes, the keys that the loader ignores, the order of the Tiles, a file that is not valid and a file of a newer version;
  - the file `dashboards.toml.bad`, which the next "Start with no Dashboard" replaces;
  - the rule of the spec requirement "Changes are stored at once": the app writes the file at most once per second.

  Verify: `rg -c 'dashboards.toml.bad' docs/storage.md` prints 1 or more, `rg -c '^\| .(col|row|col_span|row_span|span_min). \|' docs/storage.md` prints 5, `rg -c 'at most once per second' docs/storage.md` prints 1 or more, and `python3 -I tools/re/check-identifiers.py` exits 0.
- [ ] 1.3 Create `crates/nicecooling/src/dashboards/mod.rs` and `crates/nicecooling/tests/dashboards.rs`. Modify `crates/nicecooling/src/lib.rs` (module line). The model holds:
  - `Dashboard`: a name and a list of `Tile`.
  - `Tile`: one struct with every field of the TOML form. `sensor`, `title` and `text` are texts. `min`, `max` and the value of a Tile limit are `f64`. `warning` and `alarm` are `Option<TileLimit>`. A field that the kind does not use holds its default.
  - `Kind`: `Value`, `Gauge`, `Bar`, `Chart` and `Text`.
  - `TileLimit`: a `Direction` (`Above` or `Below`) and a value.
  - `TextKind`, the style of a text Tile: `Heading` or `Note`.
  - The count limits of design P4, as constants: `MAX_DASHBOARDS`, `MAX_TILES`, `MAX_CHART_TILES`, `COLUMNS`, `ROWS`, `MAX_ROW_SPAN`, `MAX_NAME_CHARS`, `MAX_TEXT_CHARS` and `MAX_SPAN_MIN`.
  - The minimum size and the default size of a `Kind`, by the table of design P5.
  - `normalize` of `Tile`: it sets each field that the kind does not use to its default.
  - `to_toml` and `from_toml`: the TOML form of `docs/storage.md`.

  `to_toml` gives the text of a list of Dashboards. It writes every field that the kind uses, also when the field has its default value. It writes no field that the kind does not use, and no Tile limit that is off.

  `from_toml` reads a text. It has three results: a list of Dashboards, "newer version", and a parse error with a text.
  - It reads `schema_version` first. A text that is not TOML, and a `schema_version` that is missing or below 1, give a parse error.
  - A `schema_version` above 1 gives "newer version", and `from_toml` reads no more of the text.
  - For version 1, these faults give a parse error with the error text of the `toml` crate: a missing `kind`, `col`, `row`, `col_span` or `row_span`; a value of the wrong type; a negative `col`, `row`, `col_span`, `row_span`, `decimals` or `span_min`; a `kind`, `style` or `direction` that the model does not know.
  - A whole number in `min`, `max` or the value of a Tile limit is valid, and so is a negative number there.
  - A missing `sensor` and a missing `text` are empty texts: the valid check of task 1.4 names them. The other fields take the defaults of design P13.
  - `from_toml` ignores a key that it does not know, and a field that the kind does not use. It gives each Tile in the form of `normalize`.
  - It keeps the Tiles in the order of the file. Task 1.5 puts them in reading order after the valid check.

  Verify: the new tests `limits_are_16_48_12_and_64`, `kind_size_table`, `toml_roundtrip_one_tile_of_each_kind`, `fields_left_out_take_their_defaults`, `gauge_without_min_and_max_has_0_to_100`, `unused_fields_are_not_written`, `used_field_with_its_default_is_written`, `limit_that_is_off_is_not_written`, `text_tile_has_no_sensor_field`, `unknown_key_is_ignored`, `negative_minimum_is_valid`, `text_that_is_not_toml_is_a_parse_error`, `missing_schema_version_is_a_parse_error`, `unknown_kind_is_a_parse_error`, `tile_without_col_is_a_parse_error`, `tiles_keep_the_order_of_the_file` and `schema_version_2_with_an_unknown_kind_is_newer` pass.
- [ ] 1.4 Create `crates/nicecooling/src/dashboards/grid.rs`. Modify `crates/nicecooling/src/dashboards/mod.rs` (module line) and `crates/nicecooling/tests/dashboards.rs`. The file has no GPUI type. A place is a column and a row. The file holds:
  - `check(dashboards) -> Result<(), Fault>`, the valid check of a list of Dashboards. It gives the first fault.
  - `Fault`: the number of the Dashboard, the number of the Tile for a fault in a Tile, and a fault kind. Both numbers count from 1, by the position in the list that `check` gets. The fault "more than 16 Dashboards" has the Dashboard number 0.
  - The fault kinds: more than 16 Dashboards; more than 48 Tiles; more than 12 chart Tiles; a Tile outside the grid; a Tile under the minimum size of its kind; a Tile that covers a cell of an earlier Tile, with the number of that Tile; no sensor; and a field that is not valid, with the name of the field (`name`, `col_span`, `row_span`, `title`, `decimals`, `max`, `span_min`, `text`, `warning`, `alarm`).
  - The order of the check. First the count of Dashboards. Then each Dashboard in the order of the list: its name, its count of Tiles, its count of chart Tiles, and then each Tile in the order of the list. For a Tile: the fields, then "outside the grid", then the minimum size, then "no sensor", then the cells of the earlier Tiles.
  - The field rules: the ranges of the spec requirements "Dashboard model", "Grid", "Tile title", "Value format", "Range of a gauge and a bar", "Chart Tile" and "Text Tile". A `max` that is not above `min` is the fault of the field `max`. A number that is not finite is not valid.
  - `sort_reading_order(tiles)`: by row, then by column.
  - `first_free(tiles, col_span, row_span) -> Option<place>`: the first free place for a size, in reading order.
  - `can_place(tiles, skip, place, col_span, row_span) -> bool`: true when every cell is in the grid and free. `skip` is the index of a Tile whose own cells count as free.
  - `key_move(tiles, index, arrow) -> Option<place>`: the move with a key, by the spec requirement "Keyboard move of a Tile". `Arrow` is `Left`, `Right`, `Up` or `Down`.
  - `resize(tiles, index, col_span, row_span) -> (col_span, row_span)`: the size that the stops of the spec requirement "Resize a Tile" allow.
  - `change_kind(tiles, index, kind) -> Option<Tile>`: the Tile with its new kind, size and place, by the spec scenarios "Kind change to a larger minimum", "Kind change with no room" and "Kind change with no free place". It gives none when no place is free.
  - `neighbor(tiles, selected, arrow) -> Option<index>`: the Tile that an arrow key selects, by the spec requirements "Keyboard selection in the grid" and "Ends and ties of the keyboard selection".
  - `tile_bounds(tile, grid_width) -> (x, y, width, height)`: the pixel geometry of design P3.
  - `drag_cells(dx, dy, grid_width) -> (columns, rows)`: the change of a place or a size for a drag distance in px, to the nearest cell.
  - `scroll_offset(top, bottom, offset, view_height) -> offset`. It takes the top and the bottom of a Tile, the old offset and the height of the view. It gives the nearest offset at which the whole Tile shows. An offset is the distance from the top of the content, and it is 0 or more.
  - `edit_rows(tiles) -> rows`: the count of rows that Edit mode shows, by the spec requirement "Rows of the grid in Edit mode".

  Verify: in `crates/nicecooling/tests/dashboards.rs` the new tests `tile_past_column_12_is_a_fault`, `two_tiles_on_one_cell_is_a_fault`, `tile_under_its_minimum_size_is_a_fault`, `seventeen_dashboards_is_a_fault`, `forty_nine_tiles_is_a_fault`, `thirteen_chart_tiles_is_a_fault`, `tile_without_a_sensor_is_a_fault`, `maximum_not_above_the_minimum_is_a_fault`, `name_of_41_characters_is_a_fault`, `count_of_dashboards_is_checked_before_a_tile`, `fault_numbers_follow_the_order_of_the_list`, `first_free_place_next_to_a_gauge`, `first_free_place_skips_a_full_row`, `no_free_place_in_64_rows`, `move_by_one_cell`, `move_onto_a_tile_is_not_made`, `key_move_takes_the_next_cell_when_it_is_free`, `key_move_goes_over_a_neighbor`, `key_move_with_no_free_place_changes_nothing`, `key_move_goes_below_a_wide_tile`, `resize_stops_at_a_neighbor`, `resize_stops_at_the_minimum_size`, `resize_stops_at_8_rows`, `resize_stops_at_the_right_edge`, `resize_stops_at_the_last_row`, `kind_change_grows_to_the_minimum`, `kind_change_with_no_room_moves_the_tile`, `kind_change_with_no_free_place_is_refused`, `reading_order_is_row_then_column`, `right_at_the_last_tile_changes_nothing`, `arrow_with_no_selection_gives_the_first_tile`, `down_takes_the_tile_that_shares_a_column`, `down_with_two_tiles_takes_the_lowest_column`, `geometry_at_1100_px`, `geometry_at_1920_px`, `drag_of_150_px_is_2_columns`, `drag_of_80_px_down_is_1_row`, `offset_shows_a_tile_below_the_view`, `offset_stays_for_a_tile_in_the_view`, `edit_mode_shows_8_rows_below_the_last_tile` and `edit_rows_stop_at_row_63` pass.
- [ ] 1.5 Create `crates/nicecooling/src/dashboards/store.rs` and `crates/nicecooling/tests/dashboards_store.rs`. Modify `crates/nicecooling/src/dashboards/mod.rs` (module line). The type `DashboardStore` holds the Dashboards, the index of the selected Dashboard, the file state, the error text of the last write, the mark "not written", the count of its writes and the file path. The file state is one of: no file; loaded; not valid, with the path and the parse error or the `Fault`; newer version, with the path. The store has these functions:
  - `DashboardStore::default()`: no path. It keeps the Dashboards in memory and writes nothing.
  - `DashboardStore::at(path)`: it loads the file at the path. It calls `from_toml` and `check`, and then `sort_reading_order` for each Dashboard. So a `Fault` has the Tile numbers of the file. A file that exists and that the app cannot read gives the state "not valid" with the error text of the system. A test uses `at` with a file in a temporary directory.
  - `DashboardStore::open()`: `at` with the file `dashboards.toml` in the directory of `settings_path()` of `app_settings.rs`.
  - `set(dashboards)`: it calls `normalize` for each Tile and `sort_reading_order` for each Dashboard. It takes the new list only when `check` accepts it, and then sets the mark. Otherwise it changes nothing and gives the `Fault` back. It changes nothing while the file state is "not valid" or "newer version". When the selected index is past the end of the new list, the store sets it to the last index.
  - `write_if_marked() -> bool`: when the mark is set, it writes the text of `to_toml` with `write_atomic` and the mode `0o666`, clears the mark and adds 1 to the count of writes. `write_atomic` already creates a missing directory. A failed write keeps the Dashboards in memory and sets the error text. A good write clears the error text and sets the file state "loaded". It gives true when it made a write or a write failed.
  - `try_again()`: for a failed write, it sets the mark. For a file that is not valid, it loads the file again.
  - `start_empty()`: for a file that is not valid, it renames the file to `dashboards.toml.bad` and goes to the state "no file". A rename that fails sets the error text and keeps the state. For every other state it changes nothing.
  - `select(index)`: it sets the selected index.
  - `can_add() -> CanAdd`: `Yes`, `AtLimit` at 16 Dashboards, `NotValid` or `Newer`.

  Verify: the new tests `missing_file_gives_no_dashboard_and_writes_nothing`, `first_change_creates_the_file`, `first_write_creates_the_directory`, `dashboards_survive_a_restart`, `tiles_out_of_order_load_in_reading_order`, `parse_error_is_not_shown_and_not_written`, `fault_names_dashboard_1_tile_2`, `fault_numbers_are_the_numbers_of_the_file`, `seventeenth_dashboard_in_the_file_is_a_fault`, `invalid_file_keeps_its_bytes`, `try_again_loads_a_repaired_file`, `start_with_no_dashboard_renames_the_file_to_bad`, `second_start_with_no_dashboard_replaces_the_bad_file`, `schema_version_2_is_not_shown_and_not_written`, `schema_version_is_read_before_the_tile_kinds`, `newer_file_has_no_start_empty`, `store_refuses_a_state_that_is_not_valid`, `unmarked_store_writes_nothing`, `two_changes_before_a_write_give_one_write`, `failed_write_keeps_the_change_and_shows_the_error`, `next_change_writes_again_and_clears_the_error`, `try_again_writes_after_a_failed_write`, `selected_index_stays_inside_the_list`, `can_add_is_at_limit_at_16_dashboards` and `store_without_a_path_keeps_the_dashboards_in_memory` pass.
- [ ] 1.6 Put the Dashboard store into the app.
  - Modify `crates/nicecooling/src/app.rs`. `Backend` and `AppModel` get the field `dashboards: DashboardStore`. `AppModel::new` gives `DashboardStore::default()`. The start code copies the Dashboard store of the `Backend` into the model, as it does for `settings`.
  - In `app.rs`, the closure of the tick of 1 s calls `write_if_marked` on the Dashboard store of the model. When the call gives true, the closure calls `cx.notify()`. The arm `Command::Quit` calls `write_if_marked` before `cx.quit()`.
  - Modify `crates/nicecooling/src/main.rs`: the `Backend` gets `DashboardStore::open()`.
  - Modify each file that `rg -l 'Backend \{' crates/nicecooling/tests` lists: it gets the line `dashboards: Default::default(),`. The command lists `alarm_tables.rs`, `app.rs`, `header_bar.rs`, `keyboard.rs`, `leakshield_display.rs`, `leakshield_wizard.rs`, `loop_diagram_clicks.rs`, `loop_diagram_layout.rs`, `rename_inputs.rs`, `rename_shell.rs` and `screens_draw.rs`. It also lists `graph_links.rs` when that file of `add-node-graph` is on the branch.
  - Modify `crates/nicecooling/tests/app.rs`: the three new tests build their `Backend` with `DashboardStore::at` and a file in a temporary directory. They read the count of writes of the store.
  - Modify `crates/nicecooling/tests/dashboards_store.rs`: the three new tests use `SettingsStore::new` with a path in a temporary directory, and a `DashboardStore::at` in the same directory.
  - `AppSettings` and `SettingsStore` get no field.

  Verify: in `crates/nicecooling/tests/app.rs` the new tests `tick_writes_a_marked_dashboard_store`, `tick_writes_at_most_once_per_second` and `quit_writes_a_marked_dashboard_store` pass. In `crates/nicecooling/tests/dashboards_store.rs` the new tests `settings_file_keeps_its_bytes`, `dashboards_file_keeps_its_bytes_on_a_settings_change` and `settings_toml_gets_no_dashboard_key` pass. The tests in `crates/nicecooling/tests/settings.rs` (existing) pass.
- [ ] 1.7 Checkpoint. Verify: the checkpoint command exits 0.

## 2. Tile values, limit states, missing values, chart data and the generator

- [ ] 2.1 Create `crates/nicecooling/src/screens/dashboards.rs` and `crates/nicecooling/tests/dashboards_tiles.rs`. Modify `crates/nicecooling/src/screens/mod.rs` (module line), `crates/nicecooling/src/screens/format.rs` (make `digits` public: the default decimals of a unit) and `crates/nicecooling/src/screens/leakshield/charts.rs` (make `GAUGE_MAX` public: the default maximum for `mbar`). `screens/dashboards.rs` holds `TileView`, the view of one Tile. It builds the view from a Tile and the `ScreenInput`. The file has:
  - the title of the view;
  - the number text and the unit text of the view, as two texts;
  - the fraction of a ring or a bar;
  - the text style of the number (`value` or `numeral`);
  - `default_decimals(unit)`, which calls `digits`, and `default_range(unit)`, which gives the minimum and the maximum of design P7.

  Verify: the new tests `title_follows_the_sensor_name`, `own_title_wins_over_the_sensor_name`, `value_with_1_and_2_decimals`, `thousands_are_grouped`, `italian_has_a_decimal_comma`, `number_and_unit_are_two_texts`, `default_decimals_by_unit`, `default_range_by_unit`, `fraction_of_298_in_0_to_450_is_66_percent`, `fraction_stays_between_0_and_1`, `value_tile_of_2_rows_has_the_numeral_style`, `gauge_under_3x3_has_the_value_style`, `derived_sensor_shows_its_value`, `sensor_of_an_unknown_origin_shows_its_value` and `calibrated_flow_shows_115_l_h` pass.
- [ ] 2.2 Modify `crates/nicecooling/src/screens/dashboards.rs` and `crates/nicecooling/tests/dashboards_tiles.rs`: the limit state of a Tile (none, warning, alarm), the caption, the tooltip text and the accessible text.
  - `TileView` gives a color for each part as a new enum `TileColor`. The parts are those of the design table of P8: the ground, the number, the filled part, and the title, the unit and the caption.
  - The variants of `TileColor` are the tokens of design P5, P8 and P10: `Card`, `Transparent`, `WarningTint`, `AlarmTint`, `Fg`, `FgDim`, `Warning`, `Alarm`, `S1` and `Track`. `Transparent` is the ground of a heading. `FgDim` is the color of an offline value and of the icon of a missing value.
  - `TileView` holds each limit line of a chart Tile as a value, a `TileColor` and a label. Task 4.4 makes a `Reference` of `crates/nicecooling/src/screens/leakshield/charts.rs` from it, with the dash `(5.0, 4.0)`. `time_chart` already makes its value axis cover the `Reference` values.
  - Add the strings to the four locale files.

  Verify: the new tests `value_36_2_is_over_the_warning_limit_above_35`, `alarm_limit_wins_over_the_warning_limit`, `value_equal_to_the_limit_is_not_over_it`, `value_142_is_under_the_alarm_limit_below_250`, `tile_without_a_limit_has_no_state`, `caption_shows_from_2_rows_up`, `tooltip_and_accessible_text_hold_the_limit`, `warning_state_has_the_tint_and_the_warning_number`, `unit_and_caption_are_fg_in_a_limit_state`, `filled_part_has_the_text_token_of_the_state`, `chart_has_a_line_for_each_limit`, `chart_state_comes_from_the_live_value` and `text_tile_has_no_limit` pass.
- [ ] 2.3 Modify `crates/nicecooling/src/screens/dashboards.rs` and `crates/nicecooling/tests/dashboards_tiles.rs`: the cases of design P10. The view reads these facts from the `ScreenInput`:
  - The live value of a sensor: the field `sensors` of the app state. The last value: the field `last_sensors`.
  - The app has a sensor list of this run: the snapshot of the app state holds one sensor or more. A Tile shows "Sensor not found" only in this case.
  - The line of the last values, while the daemon is offline or down: the new string `dashboards.last_values`, "Last values %{time}", with the time of `last_value_time` of `crates/nicecooling/src/widgets/loop_diagram.rs`.

  The shell draws the case "The app connects at its start". This task adds nothing for it. Add the strings to the four locale files. Verify: the new tests `sensor_without_a_value_shows_dashes_and_the_others_stay_live`, `gauge_without_a_value_has_its_track_only`, `no_limit_state_without_a_value`, `sensor_out_of_the_list_shows_not_found_and_its_id`, `not_found_tile_keeps_its_own_title`, `sensor_that_returns_shows_its_value`, `offline_shows_the_last_value_dim`, `offline_tile_has_no_limit_state`, `offline_line_has_the_time_of_the_last_values` and `never_online_shows_dashes_and_no_not_found` pass.
- [ ] 2.4 The chart data, with no GPUI type. Create `crates/nicecooling/tests/dashboards_chart.rs`.
  - Modify `crates/nicecooling/src/history/query.rs`: add the function `time_buckets(series: &SeriesData, width_s: i64) -> Vec<Vec<ChartPoint>>`, next to `downsample`. A point of the series is in the bucket `t.div_euclid(width_s)`. Each bucket with a point gives one bucket point. Its `t` is the start of the bucket. Its `mean` is the mean of the `mean` values of its points. Its `min` is the lowest `min`, and its `max` is the highest `max`.
  - `time_buckets` then makes the runs of the line. Two consecutive bucket points are in one run when they are `width_s` apart or less. They are also in one run when one segment of the series has points in both buckets. Otherwise a new run starts. `downsample` does not change.
  - Create `crates/nicecooling/src/dashboards/chart.rs` and modify `crates/nicecooling/src/dashboards/mod.rs` (module line). In this list, N is the time span of a chart Tile in minutes. The file holds:
    - `ChartKey`: the sensor id and N.
    - `ChartEntry`: the runs as `(time, mean)` pairs, the flag "a good read exists", the error flag of the last read, and the time of the last read in Unix seconds.
    - `interval_s(N)`: N / 4 seconds, rounded down to a whole second, at least 10 and at most 60.
    - `bucket_width_s(N)`: N / 10 seconds, rounded up to a whole second, at least 1.
    - `is_due(entry, N, now)`: true for no entry, and for an entry whose last read is `interval_s` old or older.
    - The time domain: the last N minutes up to `now`.
    - `read(path, keys, now)`. It opens the history store once with `HistoryDb::open`. For each key it calls `load_series(&db, sensor, now - 60 * N, now, now)` and `time_buckets`. It gives the runs of every key, or one error text. An error of the open or of one key is the error of the whole read.
  - `HistoryDb::open` creates a history store that does not exist. So the error test uses a path whose parent is a file.
  - When `add-history-export` is on the branch, `load_series` has no argument `now`, and `crates/nicecooling/src/history/db.rs` has a read-only open. Then `read` calls `load_series` without that argument and uses the read-only open.
  - Modify `crates/nicecooling/src/screens/dashboards.rs`: `TileView` takes the `ChartEntry` of the key of a chart Tile, or none. With no entry, the plot has no text. An entry with no good read and with the error flag gives the text "The history cannot be read." An entry with a good read and no point gives the text "No data in this time span." An entry with a good read and the error flag keeps its runs. A chart Tile under 4 columns or under 3 rows has no axis label. Add the strings to the four locale files.

  Verify: the new tests `interval_is_10_s_15_s_and_60_s`, `interval_of_50_minutes_is_12_s`, `bucket_is_1_s_6_s_and_144_s`, `two_equal_chart_tiles_share_one_key`, `entry_is_due_after_its_interval`, `domain_is_the_last_60_minutes`, `line_covers_the_10_stored_minutes`, `gap_of_5_minutes_is_a_break`, `86400_samples_give_601_points_or_fewer`, `dropout_each_minute_gives_601_points_or_fewer`, `dropout_of_5_s_breaks_a_5_minute_line`, `dropout_of_5_s_does_not_break_a_24_hour_line`, `minute_rows_of_one_segment_are_one_run`, `sample_of_21_00_07_is_in_the_bucket_of_21_00_06`, `bucket_keeps_its_point_when_the_window_moves`, `data_is_there_after_one_read_of_the_store` (it fills a `HistoryDb` with 60 minutes and reads once), `store_under_a_file_gives_an_error`, `no_sample_gives_the_empty_text`, `failed_first_read_gives_the_error_text`, `failed_read_keeps_the_last_line` and `chart_of_3x2_has_no_axis_label` pass. The tests in `crates/nicecooling/tests/history.rs` (existing) pass.
- [ ] 2.5 Create `crates/nicecooling/src/dashboards/generate.rs` and `crates/nicecooling/tests/dashboards_generate.rs`. Modify `crates/nicecooling/src/dashboards/mod.rs` (module line). `generate(loop_info, snapshot, names)` takes a `LoopInfo`, the `Snapshot` of `crate::model` and the names of the Dashboards that exist. It gives one Dashboard, by design P12 and the spec requirements "Generator from a Loop" to "Fan Channels when no Curve matches". `empty_name(names)` gives the name "Dashboard <n>" of an empty Dashboard. These rules hold:
  - The pump speed Tiles are for the ids of `LoopInfo::pump_channels`, in the order of that list. Each id is a key of `Snapshot::channels`.
  - The speed sensor of a Channel is `ChannelInfo::fan_sensor`.
  - A Channel is "not a Pump channel of any Loop" when its field `is_pump` is false.
  - The Input of a Curve is `CurveSpec::input`. It matches the water temperature sensor when it is the id of that sensor. It also matches when the sensor with that id has a `derived` definition whose `sources` hold the id of the water temperature sensor. No other Input matches. So a Graph sensor does not match.
  - The order of the fan Channels is the order of the Channels screen, which is the order of `Snapshot::channels`.
  - The decimals of a generated Tile are `default_decimals` of the unit of its sensor.
  - The maximum of the pressure gauge is `GAUGE_MAX`.

  Verify: the new tests `loop_with_every_part_gives_5_tiles`, `places_of_the_5_tiles_and_2_fans`, `five_fans_use_row_3`, `ten_fans_give_8_fan_tiles`, `loop_without_a_leakshield_has_no_pressure_and_no_fill_level`, `places_without_a_leakshield`, `loop_with_no_sensor_gives_a_dashboard_with_no_tile`, `pump_without_a_speed_sensor_gives_no_tile`, `pump_of_another_loop_gets_no_pump_tile`, `part_whose_sensor_is_not_in_the_list_gives_no_tile`, `fans_are_the_channels_on_the_water_temperature`, `derived_input_counts_as_the_water_temperature`, `derived_of_a_derived_is_no_match`, `graph_sensor_input_is_no_match`, `fixed_channel_with_an_old_input_is_no_fan`, `no_curve_on_the_water_gives_all_fan_channels`, `pump_channel_of_another_loop_is_no_fan`, `channel_tiles_take_the_channel_name`, `channel_name_of_64_characters_is_cut_to_40`, `generated_tiles_take_the_decimals_of_their_unit`, `generator_sets_no_tile_limit`, `second_dashboard_of_a_loop_is_main_loop_2`, `long_loop_name_keeps_room_for_its_number`, `generated_dashboard_passes_the_valid_check` and `empty_dashboard_takes_the_lowest_free_number` pass.
- [ ] 2.6 Checkpoint. Verify: the checkpoint command exits 0.

## 3. Screen file, comps and the design system

- [ ] 3.1 Create `docs/ui/screens/dashboards.md`, in the form of `docs/ui/screens/overview.md`, with these sections:
  - Layout: the tab strip with its "More" menu, the grid of design P3 with its numbers, the geometry inside each Tile kind at its minimum and its default size by design P5, the toolbar of Edit mode, the Tile settings dialog. This task chooses the numbers that design P5 does not give: the width of the ring, the diameter of the ring at each size, and the places of the labels. The owner approves them in task 3.4.
  - States: every row of design P10, the limit states of design P8 with the token of each part, the two empty states, the four file states of design P13, the line of a failed write with "Try again", Edit mode, and the target outline of a drag, possible and not possible.
  - Interactions: the key table of design P11.
  - Copy: every quoted English text of the spec, with the Italian text as a length check. It also has the text of each fault kind of task 1.4. The fault "The file has more than 16 Dashboards." has no prefix. The other texts come after "Dashboard <n>:" or "Dashboard <n>, Tile <m>:". The spec gives two of them: "it covers a cell of Tile <k>" and "it has no sensor". The others are: "it has more than 48 Tiles", "it has more than 12 chart Tiles", "it is outside the grid", "it is under the minimum size of its kind" and "the field `<name>` is not valid".

  Verify: `rg -c '^## (Layout|States|Interactions|Copy)' docs/ui/screens/dashboards.md` prints 4. Each of these texts is in the file: "Sensor not found", "Last values", "No Dashboard yet", "No Tile yet", "Try again", "Start with no Dashboard", "The history cannot be read.", "Alt", "Insert" and "it is outside the grid".
- [ ] 3.2 Make the two comps and their renders. Every value in the comps is synthetic.
  - Create `docs/ui/comps/dashboards.html`: a Dashboard at 1280x800 with one Tile of each kind. One Tile is in the warning state, one is in the alarm state, one has no value, and one shows "Sensor not found".
  - Create `docs/ui/comps/dashboards-edit.html`: Edit mode at 1100x700, with the grid cells, a selected Tile with its resize handle, a target outline and each kind at its minimum size. With the query `&dialog=tile` it shows the Tile settings dialog.
  - Modify `docs/ui/comps/nc.css` (the Tile styles), `docs/ui/comps/nc.js` (the sidebar entry "Dashboards" and the icons `grid` and `redo`) and `docs/ui/comps/it.js` (the Italian strings).
  - Modify `docs/ui/comps/README.md`: one row for each of the two comps, a row for the query `&dialog=tile`, and `dashboards-edit.html` in the row of `&lang=it`.
  - Create the renders `docs/ui/comps/shots/dashboards-light.png`, `dashboards-dark.png`, `dashboards-edit-min-light.png`, `dashboards-edit-min-dark.png`, `dashboards-edit-min-it-light.png` and `dashboards-edit-dialog-min-light.png`. Use the command of the section "Renders" of `docs/ui/comps/README.md`: `chrome-headless-shell` with `--window-size=1280,800` for the first two and `--window-size=1100,700` for the other four.

  Verify: the six PNG files exist and none is empty, `rg -c 'dashboards' docs/ui/comps/README.md` prints 3 or more, and `rg -c 'Sensor not found' docs/ui/comps/dashboards.html` prints 1 or more.
- [ ] 3.3 Modify the documents of the design system, by design P15:
  - `docs/ui/DESIGN.md`: the rule for a Dashboard and the rule for a Tile limit, in "Do's and Don'ts" and in "Named rules"; the padding of a Tile (12 px at the sides, 8 px at the top and the bottom); the use of the `value` and `numeral` text styles in a Tile; and the rows of the Dashboards screen in the component map.
  - `docs/ui/widgets.md`: the sections "Tile grid" (one focus stop, the keys, the target outline, the scroll to the selected Tile) and "Ring gauge" (270 degrees, the opening at the bottom, the ring width, the track, the colors). In "Common rules", the rule for an offline daemon gets the exception: "The Tile grid keeps its edit handles, because it writes a file of the app."
  - `docs/ui/components.md`: the sidebar entry, the screen "Dashboards" and the two custom widgets.
  - `docs/ui/screens/app-shell.md`: the sidebar entry "Dashboards" after "Overview"; the note that Alt+2 opens it; the entries that have no Alt number when the machine has one Leakshield; and the Copy row with the English text "Dashboards" and the Italian text "Dashboard". The row "Daemon offline" of the States table must say "Every control that writes to the daemon is disabled". When it says "Every control that writes is disabled", change it to that text. Then add the sentence "The Dashboards still work: the app holds them."
  - `docs/ui/screens/loops.md`: the command "Create a Dashboard from this Loop" of the Loop header and its off state.

  Verify: `rg -c 'Tile limit' docs/ui/DESIGN.md` prints 1 or more, `rg -c '^## (Tile grid|Ring gauge)' docs/ui/widgets.md` prints 2, `rg -c 'Tile grid keeps' docs/ui/widgets.md` prints 1, `rg -c 'The Dashboards still work' docs/ui/screens/app-shell.md` prints 1, and `rg -l 'Dashboard' docs/ui/components.md docs/ui/screens/app-shell.md docs/ui/screens/loops.md` prints 3 files.
- [ ] 3.4 Gate: the owner reads `docs/ui/screens/dashboards.md`, the two comps and the edits of task 3.3, and approves them, as for the screens of `add-nicecooling-v1`. Sections 4 and 5 do not start before this gate. Verify: the owner approves these files in the thread.
- [ ] 3.5 Checkpoint. Verify: the checkpoint command exits 0.

## 4. Dashboards screen: navigation and view

In this section, tests put Dashboards into the Dashboard store with `set`. The view draws the buttons "New Dashboard" and "Edit". They have no action until section 5.

- [ ] 4.1 Add the screen to the shell.
  - Modify `crates/nicecooling/src/shell.rs`: `Page::Dashboards`, its `title_key` `nav.dashboards`, and the entry directly after `Page::Overview` in the first group of `sidebar`. When `Page::Graphs` of `add-node-graph` is on the branch, keep its lines.
  - Modify `crates/nicecooling/src/shell_view.rs`: `nav_icon` gives `NcIcon::Grid`.
  - Modify `crates/nicecooling/src/icons.rs`: the icons `Grid` and `Redo`. Create `crates/nicecooling/assets/nc/grid.svg` and `crates/nicecooling/assets/nc/redo.svg`, with the shapes of task 3.2.
  - Modify `crates/nicecooling/src/screens/dashboards.rs`: the new type `DashboardsUi`, the state of the Dashboards screen. In this task it is empty and has `Default`.
  - Create `crates/nicecooling/src/screens/dashboards_view.rs` and modify `crates/nicecooling/src/screens/mod.rs` (module line): `render_dashboards` of `ScreenView`. In this task it draws the card "No Dashboard yet" with its text.
  - Modify `crates/nicecooling/src/screen_view.rs`: the field `dashboards: DashboardsUi`, and the arm of `Page::Dashboards` in `render`, which calls `render_dashboards`.
  - Modify `crates/nicecooling/tests/screens_draw.rs`: add `Page::Dashboards` to `PAGES` and raise its length by 1.
  - Modify `crates/nicecooling/tests/shell.rs`: the table of `app_shell_labels` gets the row `("nav.dashboards", "Dashboards", "Dashboard")`, and its list of the first sidebar group becomes `["Panoramica", "Dashboard", "Loop", "Canali", "Sensori"]`.
  - Add `nav.dashboards` and the texts of the card to the four locale files.

  Verify: the tests in `crates/nicecooling/tests/screens_draw.rs` and `crates/nicecooling/tests/shell.rs` (existing, modified) pass, and in `crates/nicecooling/tests/shell.rs` the new tests `dashboards_entry_follows_overview` and `second_page_of_the_shell_is_dashboards` pass.
- [ ] 4.2 Create `crates/nicecooling/src/widgets/ring_gauge.rs`. Modify `crates/nicecooling/src/widgets/mod.rs` (module line) and `crates/nicecooling/tests/dashboards_tiles.rs`. The widget paints a track and a filled arc with `polyline` of `crates/nicecooling/src/widgets/paint.rs`, as straight segments. One function with no GPUI type gives the points of an arc for a fraction and a radius, around the center (0, 0). An angle is in degrees, with 0 at the right and with the angle that grows clockwise on the screen, where y grows down. The arc starts at 135 degrees, at the lower left. A full arc ends at 405 degrees, at the lower right. Verify: the new tests `full_arc_covers_270_degrees`, `fraction_0_has_no_arc`, `arc_of_66_percent_ends_at_its_angle`, `full_arc_has_48_segments_or_more` and `opening_is_at_the_bottom` pass.
- [ ] 4.3 The view of the screen. Modify `crates/nicecooling/src/screens/dashboards_view.rs`, `crates/nicecooling/src/screens/dashboards.rs`, `crates/nicecooling/src/screens/ui.rs`, `crates/nicecooling/src/screens/controls.rs`, `crates/nicecooling/src/screen_view.rs` and `crates/nicecooling/tests/app.rs`. Create `crates/nicecooling/tests/dashboards_view.rs`.
  - `screens/ui.rs` gets the function `tab_strip`. It has the look of the tab strip in `crates/nicecooling/src/screens/leakshield/frame_view.rs`: 38 px high, 18 px of side padding, a gap of 3 px, and tabs with 12 px of side padding. The Leakshield screen does not change.
  - `tab_strip` takes: the labels; the selected index; the open state of "More"; a handler for a click on a tab, with its index; a handler that opens and closes "More"; the elements of its right end and their width; the free width for the tabs; the window; and the palette.
  - The width of a tab is the width of its label plus 24 px. `shape_with` of `crates/nicecooling/src/screens/leakshield/charts.rs` gives the width of a label, with the font of the text style of the window, the weight SEMIBOLD and the size 14.
  - The strip shows the first tabs that fit. When a tab does not fit, the button "More" shows after the last tab that fits, and its menu lists the other tabs. The menu is `with_menu` of `screens/ui.rs`. When the selected tab is in the menu, the button "More" has the look of a selected tab, and the row of the tab in the menu has a check mark.
  - The count of tabs that fit is one function with no GPUI type. It takes the widths of the tabs, the free width and the width of "More".
  - Each tab and the button "More" is a focus stop that takes Enter, with `keyed_focus` of `screens/ui.rs`, as the buttons of that file.
  - The view gets its widths from the window. The content width is the width of `window.viewport_size()` less 220 px for the sidebar. The grid width is the content width less 48 px. The width of the right end is, for each of its buttons, the width of its label from `shape_with` plus 48 px. The free width for the tabs is the content width less 36 px and less the width of the right end.
  - `DashboardsUi` gets the open state of the "More" menu. `close_popovers` of `screens/controls.rs` closes it.
  - In `screen_view.rs`, the tab strip goes into `strip`, as for `Page::Leakshield`. In `crates/nicecooling/tests/app.rs`, the list of `review_every_page_starts_under_the_header` gets `Page::Dashboards`.
  - The view draws the tab strip, with "New Dashboard" and "Edit" at its right end. It draws the two empty states and the screens of the file states "not valid" and "newer version". It draws the line of a failed write with "Try again", the line of the last values, and the grid with `tile_bounds` of task 1.4.
  - The screen of a file that is not valid shows the path and the fault, with the texts of task 3.1. A parse error shows "The Dashboards file cannot be read." and the text of the error. "Try again" calls `try_again` of the Dashboard store, and "Start with no Dashboard" calls `start_empty`. "Try again" of the line of a failed write calls `try_again` too.
  - The view draws each Tile kind with the `TileView` of tasks 2.1 to 2.3, with the padding of design P5: the value, the ring of task 4.2, the bar with `rect` of `widgets/paint.rs`, and the text. It maps a `TileColor` to the `Palette`. The icons are `NcIcon::Warn` for the warning state, `NcIcon::Alarm` for the alarm state and `NcIcon::Off` for a missing value.
  - Each Tile has the debug selector `tile-<col>-<row>`. Its number has the debug selector `tile-<col>-<row>-value`.
  - The selected Dashboard is the index of the Dashboard store. A click on a tab calls `select`.
  - Add the strings to the four locale files.

  Verify: the new tests `tabs_show_in_the_stored_order`, `sixteen_tabs_overflow_into_more_at_1100_px`, `selected_tab_in_the_menu_marks_more`, `count_of_tabs_that_fit`, `selected_dashboard_stays_across_screens`, `selected_dashboard_stays_when_the_window_closes`, `no_dashboard_shows_the_empty_card`, `dashboard_without_a_tile_shows_no_tile_yet`, `tile_bounds_at_1100_px_and_1920_px`, `eight_rows_show_at_1100x700`, `every_kind_draws_in_light_and_dark`, `warning_and_alarm_states_draw_with_their_icon`, `limit_state_keeps_the_status_bar_and_sends_no_command` (with a Tile over its alarm limit, the status bar says "No active Alarms" and the test gets no command), `minimum_sizes_keep_the_value_in_en_it_de_fr`, `long_title_truncates_and_the_value_stays`, `heading_is_one_bold_line_with_no_card_ground`, `note_wraps_and_ends_with_an_ellipsis`, `invalid_file_screen_has_no_tab_and_no_new_dashboard`, `invalid_file_screen_names_dashboard_1_tile_2`, `try_again_and_start_with_no_dashboard_call_the_store`, `newer_file_screen_has_no_command`, `failed_write_shows_the_error_line_and_try_again` and `connecting_shows_the_shell_text_and_no_tile` pass. The tests in `crates/nicecooling/tests/screens_draw.rs` and `crates/nicecooling/tests/app.rs` (existing, modified) pass.
- [ ] 4.4 The chart Tile and its reads. Modify `crates/nicecooling/src/screens/dashboards_view.rs`, `crates/nicecooling/src/screen_view.rs` and `crates/nicecooling/tests/dashboards_view.rs`.
  - `ScreenView` gets a map from `ChartKey` to `ChartEntry`, and one pending flag.
  - At a render of the Dashboards screen, the view asks `is_due` of task 2.4 for the key of each chart Tile of the shown Dashboard. The time is `now_us()` in whole seconds, as in `pressure_history` of `crates/nicecooling/src/screens/leakshield/overview_view.rs`.
  - When a key is due and the pending flag is off, the view sets the flag and starts one task on the background executor. The task calls `read` of task 2.4 with every due key, as `pressure_history` does for one chart.
  - When the task ends with runs, each key of the read gets its runs, the flag "a good read exists" and no error flag. When it ends with an error, each key of the read keeps its runs and gets the error flag. Both cases set the time of the last read and clear the pending flag.
  - When the app model has no `history_path`, the view starts no read.
  - The view draws the runs of a chart Tile with `time_chart` of `crates/nicecooling/src/screens/leakshield/charts.rs`, under the header line of design P5. It makes the `Reference` values from the limit lines of task 2.2.

  Verify: the new tests `chart_tile_has_its_line_after_the_first_read`, `chart_read_does_not_run_in_the_frame`, `two_chart_tiles_make_one_read`, `one_read_task_runs_at_a_time`, `dashboard_behind_another_tab_reads_nothing`, `chart_with_a_store_error_shows_the_error_text`, `failed_read_keeps_the_line_on_the_screen` and `chart_draws_while_the_daemon_is_offline` pass.
- [ ] 4.5 The selection and the keys outside Edit mode. Modify `crates/nicecooling/src/screens/dashboards.rs`, `crates/nicecooling/src/screens/dashboards_view.rs`, `crates/nicecooling/src/screen_view.rs` and `crates/nicecooling/tests/dashboards_view.rs`.
  - `DashboardsUi` gets the selected Tile of the shown Dashboard, or none.
  - The grid is one focus stop. The arrow keys select a Tile with `neighbor` of task 1.4. Tab leaves the grid. The selected Tile has the focus ring.
  - Outside Edit mode, Enter and a click on a Tile with a sensor call `open_history_for` of `screen_view.rs`. A text Tile takes no click.
  - A hover shows the tooltip of the spec requirement "Open the sensor from a Tile".
  - The view keeps the bounds of the grid in an `Rc<Cell<Bounds<Pixels>>>` that the grid sets at paint, as `plot_bounds` of `ScreenView` does.
  - `ScreenView` gets a `ScrollHandle`. In `ScreenView::render`, the scroll container `screen-scroll` tracks the handle (`track_scroll`) only while the screen is `Page::Dashboards`. The other screens keep their own scroll position.
  - After a key that changes the selection, the view calls `scroll_offset` of task 1.4 and sets the offset of the handle. The top of a Tile in the scroll content is: the top of the grid bounds, less the top of the bounds of the handle, less the y of the offset of the handle, plus the y of the Tile in the grid. The y of the offset of the handle is 0 or negative: it is the offset of task 1.4 with a minus sign.

  Verify: the new tests `click_on_a_tile_opens_history_with_its_sensor`, `text_tile_takes_no_click`, `right_selects_the_next_tile_in_reading_order`, `down_selects_the_tile_that_shares_a_column`, `arrow_with_no_selection_selects_the_first_tile`, `enter_opens_history`, `tab_leaves_the_grid`, `selected_tile_at_row_12_scrolls_into_view`, `other_screens_keep_their_scroll_position`, `tooltip_has_the_name_the_value_the_limit_and_the_age`, `drag_outside_edit_mode_moves_no_tile`, `alarm_opens_the_overview_from_a_dashboard` and `poweroff_banner_shows_above_the_tab_strip` pass.
- [ ] 4.6 Checkpoint. Verify: the checkpoint command exits 0.

## 5. Edit mode, Tile settings, Dashboard commands and the generator command

- [ ] 5.1 The state and the commands of Edit mode. Create `crates/nicecooling/tests/dashboards_edit.rs`. Modify `crates/nicecooling/src/screens/dashboards.rs` and `crates/nicecooling/src/screen_view.rs`.
  - `DashboardsUi` gets the Edit mode flag and the undo list of 50 changes.
  - `screens/dashboards.rs` gets the commands: add, remove, move, resize, change the settings, rename, move a tab, delete a Dashboard and new Dashboard.
  - Each command gives its new list of Dashboards to `set` of the Dashboard store. A command that adds, moves or brings back a Dashboard then calls `select` with the new index of that Dashboard.
  - An entry of the undo list is the list of Dashboards and the selected index before the change. The undo list gets an entry only when `set` accepts the change. An undo and a redo give the list of their entry to `set` and call `select`.
  - "New Dashboard" clears the undo list.
  - In `screen_view.rs`: at a render, when the screen of the app model is not `Page::Dashboards`, the view ends Edit mode, clears the undo list and closes the dialogs and the menus of the Dashboards screen.

  Verify: the new tests `edit_turns_on_and_selects_the_first_tile`, `each_command_marks_the_store_and_the_tick_writes_it`, `every_command_keeps_the_file_valid`, `undo_brings_a_removed_tile_back`, `redo_removes_it_again`, `undo_list_holds_50_changes`, `undo_of_a_deleted_dashboard_selects_it`, `undo_takes_a_rename_back`, `undo_takes_a_tab_move_back`, `new_dashboard_clears_the_undo_list`, `done_clears_the_undo_list`, `leaving_the_screen_ends_edit_mode`, `another_tab_keeps_edit_mode`, `remove_selects_the_next_tile`, `remove_of_the_last_tile_selects_the_previous_tile`, `remove_moves_no_other_tile` and `quit_without_done_keeps_three_moves` pass.
- [ ] 5.2 The toolbar and the drags of Edit mode. Modify `crates/nicecooling/src/screens/dashboards_view.rs`, `crates/nicecooling/src/screen_view.rs` (two variants of `Drag`: the move and the resize of a Tile) and `crates/nicecooling/tests/dashboards_view.rs`.
  - The button "Edit" of the tab strip turns Edit mode on.
  - The view draws the toolbar of Edit mode: "Add Tile", "Undo", "Redo", the button of the Dashboard menu and "Done". The toolbar goes into `strip` of `screen_view.rs`, under the tab strip, so it does not scroll. "Undo", "Redo" and "Done" call the commands of task 5.1. Tasks 5.4 and 5.5 give "Add Tile" and the Dashboard menu their actions.
  - The view draws the cells of the grid for `edit_rows` of task 1.4, the selection, the resize handle and the target outline.
  - It maps a drag to the commands of task 5.1. The new place or size comes from `drag_cells` of task 1.4, with the drag distance and the grid width.
  - The commands do not use `write_gate` of `crates/nicecooling/src/shell.rs`: the resize handle and the drag work while the daemon is offline.
  - Add the strings to the four locale files.

  Verify: the new tests `seven_rows_show_in_edit_mode_at_1100x700`, `cells_show_8_rows_below_the_last_tile`, `drag_of_150_px_moves_the_tile_2_columns`, `drop_on_a_tile_puts_the_tile_back`, `escape_in_a_drag_puts_the_tile_back`, `target_outline_is_dashed_over_a_tile`, `handle_drag_of_140_px_adds_2_columns`, `handle_drag_stops_at_the_neighbor`, `handle_drag_of_900_px_gives_8_rows`, `handle_works_while_the_daemon_is_offline`, `add_tile_is_aggiungi_riquadro_and_kachel_hinzufuegen` and `alarm_in_edit_mode_keeps_the_moved_tile` pass.
- [ ] 5.3 Modify `crates/nicecooling/src/screens/dashboards_view.rs` and `crates/nicecooling/tests/dashboards_view.rs`: these keys of Edit mode of the design table in P11: Alt with an arrow, Shift with an arrow, Delete, Ctrl+Z, Ctrl+Shift+Z and Escape. Task 5.4 adds Insert and Enter. After each key, the view sets the scroll offset of task 4.5 for the selected Tile. Verify: the new tests `alt_right_moves_the_tile_one_cell`, `alt_right_goes_over_a_neighbor`, `alt_right_with_no_free_place_changes_nothing`, `alt_down_goes_below_a_wide_tile`, `moved_tile_scrolls_into_view`, `shift_down_adds_one_row`, `shift_left_stops_at_the_minimum_size`, `delete_removes_the_selected_tile`, `ctrl_z_and_ctrl_shift_z_undo_and_redo` and `escape_ends_edit_mode_with_no_dialog_open` pass.
- [ ] 5.4 "Add Tile" and the Tile settings. Modify `crates/nicecooling/src/screens/dashboards.rs`, `crates/nicecooling/src/screens/dashboards_view.rs`, `crates/nicecooling/src/screen_view.rs`, `crates/nicecooling/src/screens/handlers.rs`, `crates/nicecooling/src/screens/controls.rs`, `crates/nicecooling/tests/dashboards_edit.rs` and `crates/nicecooling/tests/dashboards_view.rs`.
  - "Add Tile" and the key Insert open a menu of the five kinds, and a pick opens the Tile settings. The button "Add Tile" of the empty state "No Tile yet" turns Edit mode on and opens the same menu.
  - In Edit mode, a double click or Enter on a Tile opens the Tile settings of that Tile.
  - The Tile settings are the dialog of design P11. Build the dialog 480 px wide, as the Derived sensor editor in `crates/nicecooling/src/screens/sensors_view.rs`, not with `modal`. It holds a draft of one Tile in `DashboardsUi`. "Save" is one command of task 5.1.
  - The sensor field is the sensor picker: `picker_rows` and `picker_list` of `sensors_view.rs`, `select_trigger` of `screens/ui.rs` and `opens_picker` of `screens/controls.rs`, as the Derived sensor editor combines them.
  - "Kind", "Decimals", "Style", "Time span" and the direction of a Tile limit are selects, with `select_field` of `screens/controls.rs`.
  - `screen_view.rs` gets the `InputKey` variants `TileTitle`, `TileText`, `TileMinimum`, `TileMaximum`, `TileWarning` and `TileAlarm`. `on_input` of `screens/handlers.rs` gets one arm for each: it puts the typed text into the draft.
  - The arm of `TileTitle` cuts the text to 40 characters, and the arm of `TileText` to 200. Each sets the value of the input to the cut text.
  - A number input takes the text that `parse` of `screens/handlers.rs` reads. While a number of the draft does not parse, "Save" is off and the dialog says "Enter a number."
  - Escape closes the dialog and drops the draft. `close_popovers` of `screens/controls.rs` closes the menu of "Add Tile".
  - The select "Kind" follows the count limits, and it uses `change_kind` of task 1.4.
  - In the Tile settings of a new Tile, each pick of a sensor sets the decimals and the range from the unit, until the user edits that field. In the Tile settings of a stored Tile, a pick of a sensor changes neither.
  - After "Save", the new Tile is the selected Tile.
  - Add the strings to the four locale files.

  Verify: in `crates/nicecooling/tests/dashboards_edit.rs` the new tests `add_tile_menu_has_five_kinds`, `add_tile_of_an_empty_dashboard_turns_edit_mode_on`, `add_tile_works_while_the_daemon_is_offline`, `save_puts_the_new_tile_at_the_first_free_place`, `saved_new_tile_is_the_selected_one`, `cancel_adds_no_tile`, `double_click_opens_the_tile_settings`, `gauge_dialog_has_minimum_and_maximum_and_no_time_span`, `text_dialog_has_text_and_style`, `save_is_off_without_a_sensor`, `save_is_off_without_a_text`, `maximum_under_the_minimum_turns_save_off`, `number_that_does_not_parse_turns_save_off`, `title_input_keeps_40_characters`, `new_gauge_on_a_speed_has_0_to_3000`, `second_pick_in_a_new_tile_sets_the_range_again`, `edited_range_stays_at_a_pick`, `sensor_change_keeps_the_range`, `time_span_has_five_choices`, `limit_switch_off_gives_no_limit`, `limit_input_has_the_unit_of_the_sensor`, `dialog_holds_the_limit_note`, `escape_keeps_the_old_decimals`, `chart_row_is_off_at_12_chart_tiles`, `chart_kind_is_off_at_12_chart_tiles_for_a_value_tile`, `chart_tile_keeps_its_kind_at_12_chart_tiles`, `kind_change_with_no_free_place_turns_save_off`, `add_tile_is_off_at_48_tiles`, `no_free_place_turns_save_off`, `not_found_tile_takes_a_new_sensor`, `picker_lists_the_last_sensors_while_offline`, `picker_is_empty_with_no_sensor_list_and_a_text_tile_saves` and `remove_in_the_dialog_removes_the_tile` pass. In `crates/nicecooling/tests/dashboards_view.rs` the new tests `insert_opens_add_tile`, `escape_closes_the_dialog_and_keeps_edit_mode`, `alarm_closes_the_tile_settings_and_keeps_the_stored_tile` and `every_command_works_with_the_keyboard_only` pass.
- [ ] 5.5 The commands on a Dashboard. Modify `crates/nicecooling/src/screens/dashboards.rs`, `crates/nicecooling/src/screens/dashboards_view.rs`, `crates/nicecooling/src/screen_view.rs`, `crates/nicecooling/src/screens/handlers.rs`, `crates/nicecooling/src/screens/controls.rs` and `crates/nicecooling/tests/dashboards_edit.rs`.
  - "New Dashboard" opens a menu with "Empty Dashboard" and one row per Loop. "Empty Dashboard" uses `empty_name` of task 2.5 and turns Edit mode on. A row of a Loop calls `generate` of task 2.5 and keeps Edit mode as it is.
  - The Dashboard menu of the toolbar has "Rename", "Move left", "Move right" and "Delete Dashboard".
  - "Rename" opens a popover with one input. `screen_view.rs` gets the `InputKey` variant `DashboardRename`. In `screens/handlers.rs`, `on_input` gets its arm: it cuts the text to 40 characters, and Enter gives the rename command the typed text. `drop_rename` gets its arm: Escape closes the popover and drops the typed text.
  - "Delete Dashboard" opens a dialog that uses `modal` of `crates/nicecooling/src/screens/ui.rs`.
  - `close_popovers` of `screens/controls.rs` closes the two menus and the rename popover.
  - The test `dashboards_screen_file_labels_are_in_en_yml` calls `labels::missing_labels` for `dashboards.md`, as `crates/nicecooling/tests/leakshield_screens.rs` does for its screen files. Its examples are the quoted texts of the sections Layout and States that hold a name, a number or a time of an example.
  - Add the strings to the four locale files.

  Verify: the new tests `new_dashboard_menu_has_one_row_per_loop`, `empty_dashboard_is_dashboard_1_in_edit_mode`, `from_loop_row_adds_the_dashboard_of_the_loop`, `from_loop_row_keeps_edit_mode_as_it_is`, `rename_keeps_40_characters`, `empty_rename_keeps_the_name`, `escape_in_the_rename_keeps_the_name`, `move_right_changes_the_tab_order`, `move_right_keeps_the_moved_dashboard_selected`, `move_left_is_off_for_the_first_tab`, `move_right_is_off_for_the_last_tab`, `delete_asks_with_the_count_of_9_tiles`, `cancel_keeps_the_dashboard`, `delete_of_the_last_tab_selects_the_second`, `new_dashboard_is_off_at_16_dashboards` and `dashboards_screen_file_labels_are_in_en_yml` pass.
- [ ] 5.6 The generator command of the Loops screen. Modify `crates/nicecooling/src/screens/loops.rs`, `crates/nicecooling/src/screens/loops_view.rs`, `crates/nicecooling/tests/loops.rs` and `crates/nicecooling/tests/dashboards_edit.rs`.
  - `LoopsModel::build` gets a third argument: the `CanAdd` of the Dashboard store. `loops_view.rs` passes `can_add()` of the store of the app model. The calls of `build` in `crates/nicecooling/tests/loops.rs` get `CanAdd::Yes`.
  - The menu of the Loop header is one row with the button `loop-delete`. The row gets the button "Create a Dashboard from this Loop" before "Delete Loop", with the icon `NcIcon::Grid`.
  - The button calls `generate` of task 2.5 and gives the new list to `set` of the Dashboard store. Then it selects the new Dashboard and opens `Page::Dashboards` with Edit mode off.
  - The button is off, with the reason in its tooltip, for every `CanAdd` that is not `Yes`. It does not use `write_gate`: it is on while the daemon is offline.
  - Add the strings to the four locale files.

  Verify: the tests in `crates/nicecooling/tests/loops.rs` (existing, modified) pass. In that file the new tests of the model `loop_menu_has_create_a_dashboard`, `create_is_off_at_16_dashboards`, `create_is_off_for_a_newer_file` and `create_is_on_while_the_daemon_is_offline` pass. In `crates/nicecooling/tests/dashboards_edit.rs` the new tests `create_from_a_loop_opens_the_new_dashboard` and `loop_rename_keeps_the_generated_dashboard` pass.
- [ ] 5.7 Check that a Dashboard sends nothing to the daemon. Modify `crates/nicecooling/tests/dashboards_edit.rs`. Verify: the new test `dashboard_commands_send_no_command` passes. After a new Dashboard, a generated Dashboard, an add, a move, a resize, a settings change, a delete of a Tile and a delete of a Dashboard, the test gets no command from the window. `git diff --stat main -- crates/nicecoolingd crates/nicecooling-api crates/nicecooling-core crates/nicecooling-protocol docs/api.md` prints nothing.
- [ ] 5.8 Checkpoint. Verify: the checkpoint command exits 0, and `env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p nicecooling` exits 0.

## 6. Documents

- [ ] 6.1 Modify `docs/app.md`:
  - Add a section "Dashboards". It says: what a Dashboard is, and the five Tile kinds; that a Tile limit colors a Tile and is not an Alarm; Edit mode and its keys; the generator; that a chart shows the history that the app records; that the user stops the app before an edit of the file by hand.
  - Add a row for `~/.config/nicecooling/dashboards.toml` to the table of "Settings and files".
  - The section "Downgrade" gets one paragraph "Dashboards" with the steps of the design section "Migration and rollback" and the command that removes the file. When `docs/app.md` has no section "Downgrade", create it with this paragraph. When it has one, add the paragraph and change no other paragraph.

  Verify: `rg -c 'dashboards.toml' docs/app.md` prints 2 or more, `rg -c '^## Dashboards' docs/app.md` prints 1, and `rg -c '^#+ Downgrade' docs/app.md` prints 1.
- [ ] 6.2 Modify `openspec/changes/add-nicecooling-v1/specs/desktop-app/spec.md`: add one requirement "Dashboards" that names the screen, the file `dashboards.toml` and the capability `dashboards`, with one scenario. Remove no requirement. The requirement "App settings" keeps its text. Verify: `openspec validate add-nicecooling-v1 --strict` exits 0, and `rg -c '^### Requirement: Dashboards' openspec/changes/add-nicecooling-v1/specs/desktop-app/spec.md` prints 1.
- [ ] 6.3 Checkpoint. Verify: the checkpoint command exits 0 and `openspec validate --all --strict` exits 0.

## 7. MANUAL checks with the owner

- [ ] 7.1 MANUAL: create `docs/hardware-checks/dashboards.md` and run it on the reference desktop with GNOME on Wayland. Steps: create a Dashboard from the Loop; build a second Dashboard by hand with one Tile of each kind; set a warning limit and an alarm limit, and make a real value go over each of them; unplug the USB cable of the Leakshield; stop the daemon, and move and resize a Tile while the daemon is stopped; resize the window from 1100x700 to the full screen; switch the system theme; restart the app. The person sees these results:
  - The generated Dashboard shows the pressure, the fill level, the flow, the water temperature, the pump speed and the fans with live values, and the chart has its line within 1 s. Each value agrees with the Sensors screen.
  - A Tile over its warning limit has the amber tint and the triangle. A Tile over its alarm limit has the red tint and the octagon. On each tint, in the light and the dark theme, the person can tell the filled part of a ring from its track.
  - The status bar still says "No active Alarms", the tray icon is normal and no notification shows.
  - With the cable out, the pressure Tile shows `--`, and the Overview opens with the Alarm "Leakshield unreachable".
  - With the daemon stopped, each Tile shows its last value dim, the line "Last values" shows, and the move and the resize work.
  - At 1100x700 no Tile is cut and the Dashboard has no horizontal scroll. The light and the dark theme both draw every Tile.
  - After the restart both Dashboards have the same Tiles, places and settings.

  Verify: the file has a pass line for each step.
- [ ] 7.2 MANUAL: add these steps to `docs/hardware-checks/dashboards.md` and run them. Steps: with the keyboard only, turn Edit mode on, add a Tile, move it past another Tile with Alt and an arrow, resize it, change its sensor, undo and redo, remove it, and leave Edit mode; hold Alt+Down for 3 s on a Tile; build a Dashboard with 12 chart Tiles of 24 hours on a history store that holds 24 hours or more, and measure the CPU use of the app over 60 s with `top`; log in as a second user who is outside the group `wheel`, and edit a Dashboard. The person sees these results:
  - Each keyboard step works with no mouse, and the focus ring shows on the selected Tile.
  - While the person holds the key, the Tile moves at each repeat of the key, and the grid follows it.
  - The CPU use of the app with the 12 chart Tiles is below 10% of one core, and the number is in the file. While the charts read, a click on another tab shows that Dashboard within 1 s.
  - The second user sees "No Dashboard yet", edits a Dashboard and sees no polkit dialog.

  Verify: the file has a pass line for each step.
- [ ] 7.3 Final checkpoint. Verify: the checkpoint command exits 0, `openspec validate --all --strict` exits 0, and `docs/hardware-checks/dashboards.md` has a pass line for each step.

## Workflow follow-up

- Run the `spec-review` skill in drift mode before `openspec archive add-dashboard-pages`.
- Archive the change after the owner archives `add-nicecooling-v1`, so the deltas merge in order.
- When this change lands before `add-history-export`, task 2.2 of `add-history-export` also edits `crates/nicecooling/src/dashboards/chart.rs`. It removes the argument `now` from the call of `load_series` in that file, and it moves the read of that file to the read-only open of its task 1.3.
- When the code of both changes is on `main`: a chart Tile on a sensor that the app does not record shows "Not recorded" in its plot. This is one task of the change that lands second.
- The tab strip of the Leakshield screen can take `tab_strip` of `screens/ui.rs` in a later change. `docs/ui/DESIGN.md` asks for the "More" menu on every tab strip.
