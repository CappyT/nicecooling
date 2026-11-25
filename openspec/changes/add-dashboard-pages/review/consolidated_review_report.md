# Consolidated review report: add-dashboard-pages

Date: 2026-11-25. Review mode: after `design.md` and `tasks.md` were written and before implementation.

Each reviewer ran in a fresh context with only the change files, the glossary, the ADR, the code, the two neighbor changes and the writing style. Fifteen reviewer runs, all completed, none failed:

| Stage | Target | Reviewers | Result |
|---|---|---|---|
| 5 Initial zoom out | `design.md`, first version | zoom-out (12), inventive catalyst (18) | 0 Blocker, 15 Major, 15 Nit |
| 6 Comprehension | `design.md`, first version | comprehension (18) | 0 Blocker, 4 Major, 14 Nit |
| 7 Critic | `design.md`, first version | design (24), structural (20), style (31) | 1 Blocker, 39 Major, 35 Nit |
| 8 Intermediate zoom out | `design.md` after the fixes | zoom-out (8) | 0 Blocker, 1 Major, 7 Nit |
| 9 Implementation readiness, round 1 | `tasks.md` | readiness (23), codebase details (16), style (21) | 0 Blocker, 34 Major, 26 Nit |
| 9 Implementation readiness, round 2 | `tasks.md` and `design.md` after the fixes | readiness (25), codebase details (12), style (37) | 2 Blocker, 40 Major, 32 Nit |

The total is 265 raw findings: 3 Blocker, 133 Major and 129 Nit. The stage reports are next to this file and are not in git. Stage 8 is the one re-audit of the design, and round 2 is the one re-audit of the tasks. No reviewer checked the files after the fixes of round 2.

## Consolidated findings

Priority: Blocker stops the implementation, Major changes what is built, Nit is wording.

| # | Priority | Finding | Sources |
|---|---|---|---|
| C1 | Blocker | The app could store a file that its own loader refuses: a 13th chart Tile through a kind change, a kind change with no free place, a generated name of 42 characters, a Channel name of 64 characters as a title of at most 40. | dc-1 |
| C2 | Blocker | Four tests of the drag task and the key task needed the menu and the dialog of a later task. | r2-ir-1 |
| C3 | Blocker | The generator took `is_pump` for the Pump channels of a Loop. `is_pump` is true for a Pump channel of any Loop, so a Loop got the pump Tile of another Loop. | r2-cd-1, r2-ir-6 |
| C4 | Major | The design did not name the two neighbor changes: the atomic write function made three times, the sidebar entry and the Alt numbers with "Graphs", and three facts of the chart that `add-history-export` changes. | zo-1, zo-3, zo-8, ic-5, ic-13, ic-17, dc-7, dc-8, st-7, ir-20 |
| C5 | Major | The chart read path: an interval below the write period of the store, a line with no bound of points, a shape that changed at each read, no rule for the Dashboard that reads, a false text for a failed read, a read-only connection with wrong reasons. | zo-4, ic-2, ic-3, ic-4, dc-6, dc-11, dc-19, iz-1, iz-2, iz-3, cd-15, r2-ir-11, r2-ir-12 |
| C6 | Major | The write of the file: two `fsync` calls on the UI thread at each repeat of a key, no retry, no write at Quit, and the store inside `SettingsStore`, which the Settings screen copies at each render. | ic-6, dc-5, zo-11, ic-12, iz-4, r2-ir-15 |
| C7 | Major | The file rules: numbers that can change inside the valid check with no rule for `schema_version`, one fault that hides every Dashboard, `dashboards.toml.bad` replaced with no word, no required fields, no rule for the order and for unknown keys, no parse error, fault numbers by reading order. | ic-1, dc-2, dc-18, dc-24, st-9, st-13, st-14, zo-2, ir-2, r2-ir-2, r2-ir-5, r2-ir-17 |
| C8 | Major | The geometry of a Tile: 36 px inside a Tile of 1 row for 38 px of text, a plot of about 30 px in a chart of 3x2, a padding that differs from the card rule, two thresholds with no requirement. | dc-3, st-8, st-19, r2-ts-26 |
| C9 | Major | Edit mode while the daemon is offline, against the rules of `docs/ui/widgets.md` and `docs/ui/screens/app-shell.md` that hide edit handles and disable every control that writes. | ic-9, dc-4, st-6, ir-22 |
| C10 | Major | A new sensor can get the id of a deleted one (`next_number` gives the highest number plus 1). The design said the opposite. | co-2, dc-9, st-5, sy-15 |
| C11 | Major | The keyboard could not do what the mouse does: a move stopped at a Tile. No scroll to the selected Tile, and no rule for the ends and for a tie. | dc-10, ic-18, dc-23, ir-7, ir-19, cd-6, r2-ir-9, r2-cd-2 |
| C12 | Major | The look of a limit state: no color for the title, the unit and the caption on a tint, and a fill of about 2:1 on its tint. | ic-10, dc-12 |
| C13 | Major | The owner decisions had no source. "Choices that the owner did not close" contradicted "Open Questions: None". Four decisions and eight alternatives had no reason. | dc-13, st-2, st-3, st-4, dc-20 |
| C14 | Major | Scope against owner choices: the fan Tiles of the generator, the undo list of 50 changes, the Duty of a Channel as a source, no hysteresis. The fan rule also did not say which Curve it reads. | zo-5, zo-6, ic-14, dc-21, ic-16, dc-14, co-17, r2-ir-7 |
| C15 | Major | The "More" menu for one tab strip only, and `time_chart` in a module of the Leakshield screen. | zo-7, dc-16, zo-10, iz-7 |
| C16 | Major | A reader could not follow the design: the wrong citation "(decision 3)", no definition of a Graph sensor, almost nothing on the commands of a Dashboard, the states "offline" and "down", the vendor program, the tokens. | co-1, co-3 to co-18, st-1, sy-14 |
| C17 | Major | Screen state: no mechanism that ends Edit mode on a screen change, a dialog lost on an Alarm, the selected Dashboard lost when the window closes, grey bars that the shell never lets show. | dc-15, ic-8, ic-11, st-10, r2-ir-8 |
| C18 | Major | The tasks named code that does not fit: `select_field` as the sensor picker, an equality of `ShellModel`, a list of the shell test, `shape` at 12 px for a tab of 14 px, a window test in `tests/loops.rs`, a `Command` channel that the app drains, a model with no way to the store, no fact "sensor list of this run". | cd-2, cd-3, cd-4, cd-7, cd-8, cd-9, cd-10, cd-14, cd-16, ir-10, ir-15, ir-16, ir-21, ir-23, r2-cd-3, r2-cd-4, r2-cd-6 |
| C19 | Major | The tasks left design choices: a store constructor with a path, names of types and functions, a type for a color token, the chart entry and the result of a read, the drag rule, the empty rows of Edit mode, the screen state, the widths at a render, the form of an undo entry, the rules of the number inputs. | ir-1, cd-1, ir-3, ir-4, ir-5, ir-6, ir-8, ir-9, ir-11, ir-14, r2-ir-3, r2-ir-4, r2-ir-10, r2-ir-13, r2-ir-14, r2-ir-18 |
| C20 | Major | The tasks: file lists with no test file, a task that used the selection before the task that makes it, spec scenarios with no test, Verify lines that cannot fail, a render command that the checkout does not have. | ir-12, ir-13, cd-5, ir-17, ir-18, cd-11, cd-12, cd-13, r2-ir-20 to r2-ir-25, r2-cd-5, r2-cd-10, r2-cd-11, r2-cd-12 |
| C21 | Major | An edit by hand of a valid file is lost at the next change of the app. Two sessions of one user replace the whole file. The temporary file has a fixed name. | ic-7 |
| C22 | Major | A Tile on a Graph sensor shows `--` for a user outside `wheel`, and the design did not say so. | ic-15 |
| C23 | Nit | Style: "Action", "feed", "Profile slot", "page" and "screen", "limit" and "count limit", "span" and "time span", long sentences, reveal constructions, passive voice, history. | sy-1 to sy-31, ts-1 to ts-21, r2-ts-1 to r2-ts-37, st-12, st-15 to st-18, st-20 |
| C24 | Nit | The design repeats numbers of the spec and the TOML format. | zo-9, st-11, iz-8 |
| C25 | Nit | No mechanism gives a text to a screen reader. A tooltip for the keyboard selection needs a custom element. | dc-22, r2-ir-19 |
| C26 | Nit | The measurement of the chart load guards a load below the load of the History screen. | iz-2 |

## Resolution

Categories: FIXED (the files of the change were edited), CONTESTED (kept, with the evidence), ANSWERED (the owner decided, and the number is the owner answer below).

| # | Category | Resolution |
|---|---|---|
| C1 | FIXED | The Dashboard store takes a new state only when the valid check of the loader accepts it. A kind change obeys the count limits and has a rule for no free place. The generator cuts a name and a title to 40 characters. Design P4, P12; spec "Count limits", "Generator from a Loop", "Titles of the generated Tiles". |
| C2 | FIXED | The four tests and the keys Insert and Enter are in task 5.4. |
| C3 | FIXED | The pump Tiles come from `LoopInfo::pump_channels`. `is_pump` serves only the rule "not a Pump channel of any Loop". Task 2.5 has a test with two Loops. |
| C4 | FIXED, ANSWERED 2 | The design has the section "Order with the other changes". This change uses `write_atomic` of `main` and adds no second function. P2 gives the sidebar order with "Graphs" and says that History and Settings lose their Alt number with one Leakshield. The chart Tile draws 1 s samples and 1 minute rows. The order with `add-history-export` is owner answer 2. |
| C5 | FIXED | One read task at a time, only for the shown Dashboard. The interval is N / 4 s, at least 10 s. The line has one point per time bucket, 601 at most, and the buckets do not move. A failed read keeps the line, and a Tile with no good read says "The history cannot be read." The read-only connection is rejected: rusqlite sets a busy timeout of 5 s at every open (`inner_connection.rs:90`), and the History screen opens the store every 2 s. A read of only the new rows is rejected. Design P9; spec "Chart read rate", "Points of a chart line", "Breaks of a chart line". |
| C6 | FIXED, ANSWERED 4 | A command marks the store. The tick of 1 s of the app model writes a marked store, and Quit writes it. A failed write has "Try again". The store is a field of `AppModel`. Design P11, P13, P14; spec "Changes are stored at once", "Failed write". |
| C7 | FIXED, CONTESTED in part | The spec and P13 give the required fields, the defaults, the fields that the app writes, the ignored keys and the order. A change of a count limit or a minimum size raises `schema_version`. `from_toml` has a parse error, and a fault has the Tile number of the file. "Start with no Dashboard" replaces an older `.bad` file, and the spec says so. Contested, owner decision: a load of one Dashboard at a time. The row "A file that is not valid" of the choices table stays as the owner accepted it, and P13 gives the reason. A shared store type for three files is rejected: the three specs have different fault rules. |
| C8 | FIXED | A Tile has 12 px of padding at the sides and 8 px at the top and the bottom, so a Tile of 1 row has 44 px inside. P5 gives the arithmetic of each kind, and P15 adds the padding to `docs/ui/DESIGN.md`. The spec has the two thresholds and the rule for the axis labels of a chart. |
| C9 | FIXED | P15 and task 3.3 add the exception to `docs/ui/widgets.md` and one sentence to `docs/ui/screens/app-shell.md`. The commands do not use `write_gate`. Spec "Edit mode without the daemon". |
| C10 | FIXED | P10 states the id rule of the daemon, and the Risks section has the case. |
| C11 | FIXED, ANSWERED 3 | Alt with an arrow moves a Tile to the next free place in that direction, over the Tiles in its way. The grid scrolls to the selected Tile with a `ScrollHandle` that only the Dashboards screen uses. A drag does not scroll: the Risks section says so. Spec "Keyboard move of a Tile", "Ends and ties of the keyboard selection", "Rows of the grid in Edit mode". |
| C12 | FIXED | The title, the unit and the caption are `fg` on a tint. The filled part has the text tokens `warning` and `alarm`, which have more than 4.5:1 on their tint. Design P8. |
| C13 | FIXED, ANSWERED 1 | The Context cites issue #89 of the repository `CappyT/nicecooling`, with its comments of 2026-11-25, and lists the owner decisions. The choices table is "Product choices that the owner can change", after the Risks section, and "Open Questions" says "None". P7, P10, P12, P13, P14 and P15 have their reasons and rejected options. |
| C14 | CONTESTED, FIXED in part | Contested, owner decision: the fan Tiles, the undo list of 50 changes, no Duty as a source and no hysteresis are rows of the choices table that the owner accepted. Fixed: the fan rule names the Curve of the active Profile, the control kind Curve, a Derived sensor with the water temperature as a direct source, and no match for a Graph sensor. P8 gives the reason for no hysteresis. |
| C15 | FIXED in part | `tab_strip` with its "More" menu is in `screens/ui.rs`. The Leakshield screen and `time_chart` do not move: the change does not change the Leakshield screen. The task list has the follow-up. |
| C16 | FIXED | The Context defines the tokens, a comp, the reference desktop and the vendor program. Terms defines a Graph sensor and a count limit. P2 has the commands on a Dashboard. P10 defines online, offline and down. |
| C17 | FIXED, ANSWERED 5 | The Dashboards screen ends Edit mode at a render when the app shows another screen. An Alarm closes an open dialog, and the spec has the scenario. The selected Dashboard is in the Dashboard store of the app model. While the app connects, the shell shows its text in place of the screen: the spec requirement "Loading" says this now. |
| C18 | FIXED | The tasks name `picker_rows`, `picker_list`, `select_trigger` and `opens_picker`, `shape_with` at 14 px SEMIBOLD, the window rig of `tests/keyboard.rs`, a third argument of `LoopsModel::build`, and the fact "the snapshot holds one sensor or more". The test on `ShellModel` is a window test on the status bar. |
| C19 | FIXED | The tasks name `DashboardStore::at`, the model types, the functions of `grid.rs`, `TileColor`, `ChartKey`, `ChartEntry`, `CanAdd`, `generate` and `empty_name`. They give the drag rule by distance, the rows of Edit mode, the widths from the viewport, the undo entry and the rules of the inputs. |
| C20 | FIXED | Each task lists its test file. The selection is in task 4.5. The task list has about 110 more test names than its first version, and they cover the scenarios that had none. The render command is the one of `docs/ui/comps/README.md` on `main`. |
| C21 | CONTESTED | Accepted risk, recorded in the Risks section. `settings.toml` has the same behavior (`SettingsStore::update`, `app_settings.rs`), and the temporary name belongs to `write_atomic`. `docs/app.md` tells the user to stop the app before an edit by hand. |
| C22 | FIXED | P6 names the case. |
| C23 | FIXED in part | The design was rewritten in the project style, and the spec and the tasks use "count limit", "time span", "screen" and "Dashboard store". The tasks keep some long sentences. This does not block the implementation. |
| C24 | CONTESTED | The sibling designs hold their storage formats too. The numbers of the design and the spec agree after this review. |
| C25 | FIXED, ANSWERED 6 | P4 says that the accessible text is a text of the screen model, as on the Channels screen. The tooltip shows on a hover only. |
| C26 | FIXED in part | P9 names the load of the History screen. One step of the manual check stays. |

### Owner answers

The record is [issue #89](https://github.com/CappyT/nicecooling/issues/89) of the repository `CappyT/nicecooling`, in its comments of 2026-11-25. The files of the change hold each answer.

1. The record of the owner decisions.
   - Question: no issue held the nine decisions and the standing rules. Is the record a comment of an issue, or the design alone?
   - Answer: the record is issue #89 of the repository `CappyT/nicecooling`, in its comments of 2026-11-25. The Context of the design cites it.
2. The order with `add-history-export`.
   - Question: is the order free, or does `add-history-export` land first, so that this change uses its read-only connection and its list of not recorded sensors from the start?
   - Answer: no fixed order. The change that lands second adapts: it edits the one read in `dashboards/chart.rs`. Design decision 14 and the section "Order with the other changes".
3. The keyboard move.
   - Question: does Alt with an arrow go over the Tiles in its way to the next free place, or does it move one cell only and stop at a Tile?
   - Answer: the owner accepts the applied rule. Alt with an arrow moves a Tile to the next free place in that direction, over the Tiles in its way. Design decision 11 and P11.
4. The time of a write.
   - Question: does the app write `dashboards.toml` within 2 s, at most once per second and at Quit, or at the end of each command? With the first rule, a change of the last second is lost when the app is killed.
   - Answer: the owner accepts the applied rule: the write within 2 s, at most one write per second, and a write at Quit. Design decision 10 and P11.
5. The start of the app.
   - Question: while the app connects, does the Dashboards screen show the shell text "Connecting to the daemon", or the frame of each Tile with a grey bar?
   - Answer: the owner accepts the shell text at the start of the app, as on every screen. Design decision 12 and P10.
6. The tooltip of a Tile.
   - Question: does the tooltip show on a hover only, or also for the keyboard selection, which needs a custom element?
   - Answer: the owner accepts the tooltip on a hover only. The caption from 2 rows up and the Tile settings show the limit of a selected Tile. Design decision 13.

## What stays open

- No reviewer saw `design.md`, `tasks.md` and the spec after the fixes of round 2. The validator passes. The first implementer of each section reports a task that does not fit the code as a question.
- The spec was reading material of every stage and the target of none. The review changed 29 of its 58 requirements, renamed one and added 8. It now has 66 requirements and 212 scenarios.
- The facts about `main` come from `git show main:` in this review: `write_atomic`, the section "Renders" of `docs/ui/comps/README.md` and the row "Daemon offline" of `docs/ui/screens/app-shell.md`. This branch is 25 commits behind `main`.
- The GPUI facts of tasks 4.3 and 4.5 come from one reviewer's reading of `gpui-pre` 0.3.12: `track_scroll`, the sign of `ScrollHandle::set_offset`, and `shape_with`. No code was compiled.
- The numbers of the ring gauge and the texts in German and French are the work of tasks 3.1 and the string tasks. The owner approves the screen file in task 3.4.
- The drift review runs before the archive of the change.

`openspec validate --all --strict` passes with no warning: 5 passed, 0 failed.
