# Consolidated review report: add-history-export

Date: 2026-11-25. Review mode: after `design.md` and `tasks.md` were written and before implementation.

Each reviewer ran in a fresh context with only the change files, the glossary, the ADRs, the code and the writing style. Fourteen reviewer runs, all completed, none failed:

| Stage | Target | Reviewers | Result |
|---|---|---|---|
| 5 Initial zoom out | design.md, first version | zoom-out, inventive catalyst | 0 Blocker, 14 Major, 14 Nit |
| 6 Comprehension | design.md after the stage 5 fixes | comprehension | 0 Blocker, 5 Major, 11 Nit |
| 7 Critic | design.md after the stage 6 fixes | design, structural, style | 1 Blocker, 34 Major, 35 Nit |
| 8 Intermediate zoom out | design.md after the stage 7 rewrite | zoom-out | 0 Blocker, 1 Major, 9 Nit |
| 9 Implementation readiness, round 1 | tasks.md | readiness, codebase details, style | 2 Blocker, 40 Major, 33 Nit |
| 9 Implementation readiness, round 2 | tasks.md after the rewrite | readiness, codebase details, style | 0 Blocker, 17 Major, 43 Nit |

The sum is 3 Blocker, 111 Major and 145 Nit raw findings. The two Blockers of stage 9 are one fault that two reviewers found. The stage reports are next to this file and are not in git. Stage 8 is the one re-audit of the design, and round 2 is the one re-audit of the tasks. No reviewer checked the files after the fixes of round 2.

The design critic checked the size estimate against the table definitions in `crates/nicecooling/src/history/db.rs`. A Minute row is a cell of 40 bytes. With the page overhead that the size test measured for the Samples, it takes 44.4 to 46 bytes. The 44 bytes of the design are 0 to 4% low, and "about 1.4 GB" for one year of 60 Series (525 600 x 60 x 44 = 1 387.6 MB) is right. Task 2.6 measures the number.

## Consolidated findings

Priority: Blocker stops the implementation, Major changes what is built, Nit is wording.

| # | Priority | Finding | Sources |
|---|---|---|---|
| C1 | Blocker | Rejections named the owner with no link, and the owner decisions had no source. | st-1, dc-1 |
| C2 | Blocker | Task 2.1 removed two constants that `query.rs` uses until task 2.2, so the crate did not build. | ir-1, cd-1 |
| C3 | Major | The compaction ran after a scheduled prune too, for example at each start after 5 hours without the app. Its free space need was understated, the place of its copy was not fixed, and the WAL file kept its size. | zo-1, ic-2, dc-4, iz-6, cd-10, r2-cd-18 |
| C4 | Major | The trigger of the compaction counts removed rows. A delete of 23 MB in a store of 10 GB writes 10 GB again. | zo-1, ic-2, dc-8 |
| C5 | Major | Notes in `history.db` cost schema version 2 and three write requests, against the reasons that the design gives for the Views. The spec said that Notes work during a compaction, and the thread that writes them runs the compaction. | zo-2, ic-4, dc-16 |
| C6 | Major | The design did not say how the settings reach the service thread, and in which order a new period and its prune apply. | zo-3, ic-12 |
| C7 | Major | "Every read is read-only" against an open function that runs the schema statements at each open. A reader could migrate the file. | zo-4, ic-6, iz-3, dc-15, cd-21 |
| C8 | Major | Two painters held the chart geometry twice. | zo-5, ic-8 |
| C9 | Major | The custom `#RRGGBB` color is more than owner decision 5 asks. The dash of a seventh Series had no place in the View. | zo-6 |
| C10 | Major | A long operation gives a delay and not a gap, because 1024 events wait. The writer closed minutes by the clock, so the waiting Samples got no Minute row. The result "events were lost" was read as "no event". | zo-9, ic-1, dc-3, dc-10, r2-ir-4 |
| C11 | Major | A delete dropped the Samples in memory and not the ones that still wait in the event channel. The delete of the whole history left the minute sums. | ic-3, dc-13, iz-7, r2-ir-3 |
| C12 | Major | A settings file that is absent or does not parse gives the default periods. The next prune then deletes up to 28 days of Samples and 4 years of Minute rows with no count and no question. | ic-5, zo-12 |
| C13 | Major | The prune, the counts and the deletes read the whole table: the primary key is `(series, t)` and the statements had no Series. At 30 days this is 155 million rows each hour. | ic-7, dc-17 |
| C14 | Major | A reader of the design alone could not follow the path from the daemon to the store, the words Channel, snapshot and "shown series", and the source of the dialog "Recorded sensors" with the daemon offline. | co-1, co-3, co-4, co-5, co-6 to co-16 |
| C15 | Major | A second app process of the same user: the design named it and gave no rule. With an empty table `series`, it would write the Samples of one sensor under the id of another. | co-2, dc-2, dc-14 |
| C16 | Major | A Graph sensor id can come back after a downgrade of the daemon, and a sensor can change its unit and keep its id. | dc-5 |
| C17 | Major | Overlaps with `add-node-graph` and `add-dashboard-pages` were missing: shared files, the stop path, the Settings screen, the callers of `load_series` and `next_event`, the chart Tile. | dc-6, zo-11, zo-13, ic-13, cd-8, cd-9, cd-14, ir-10, r2-cd-11 |
| C18 | Major | A Note was drawn by "every chart" in the design and the proposal, and by the charts of the History screen in the spec. | dc-7, iz-1 |
| C19 | Major | The design, the specs and the proposal disagreed: the rule for Samples or Minute rows, the last second of a switch that goes off, the rule under the Limits table, the font list, a toolbar rule that `docs/ui/DESIGN.md` does not have, "Rename" and "Rename view", two meanings of "recommended", the decimal point rule, the migration at "the first open". | st-2, st-4 to st-9, dc-11, sy-6 to sy-8, iz-2, iz-3, iz-8, iz-9, r2-sy-3 |
| C20 | Major | One word for two things and two words for one thing: window, channel, writer, the list of not recorded sensors, snapshot, series color, capitals of Series, Sample and Minute row, "waiting Samples", "warning". | st-3, st-15, sy-1 to sy-5, sy-9, sy-10, sy-13, sy-14, sy9-1, sy9-12, sy9-13, r2-sy-1, r2-sy-2 |
| C21 | Major | Sentences above 25 words, wrong actors, a claim "every tool reads", history of the text, filler. | sy-11, sy-12, sy-15 to sy-18, st-18, dc-18, sy9-2 to sy9-8, sy9-14, sy9-15, r2-sy-4 |
| C22 | Major | The tasks left API choices to the implementer: the requests and their replies, the handle, the operation guard, the error of a newer database, the writer settings, the state type of the Settings screen, the carrier of the list and of the Notes in the History model. | ir-3, ir-4, ir-6, ir-7, cd-3, cd-4, r2-ir-1, r2-ir-2, r2-ir-11, r2-cd-1, r2-cd-2, r2-cd-4, r2-cd-5, r2-cd-7 |
| C23 | Major | Tasks missed files that their work breaks: 11 test files that build `Backend`, `history_view.rs` for the color type, `handlers.rs` for each text input, the hook for the open of a screen, test file markers. | ir-2, cd-2, cd-5, cd-6, cd-7, r2-cd-6, r2-ir-7, r2-ir-12 |
| C24 | Major | Work in the wrong order: the count of the Notes before its module, the term rows after the strings, tests before the code that they observe. | ir-5, ir-11, ir-20, cd-16, r2-cd-3 |
| C25 | Major | Tests that need a failure had no way to make it (free space, `VACUUM`, a write error), and `set_var` is `unsafe` in edition 2024. | ir-8, ir-9, cd-10 |
| C26 | Major | Verify lines that cannot fail or cannot pass: "needs no display server", a lock file check on changes that are not committed, counts over alternations, a pattern that `en.yml` has already, a heading that another change adds. | ir-16 to ir-18, cd-20, r2-ir-5, r2-ir-6, r2-ir-15, r2-cd-13, r2-cd-14 |
| C27 | Major | Rules that no file gave: the size text, the layout of the image, the first View load against an open from the Sensors screen, the moments of the read of "Size on disk", the text of the row "Recorded sensors". | ir-12 to ir-15, sy9-9 |
| C28 | Major | 25 scenarios of the specs had no test. | ir coverage list, r2-ir-14, r2-ir-17 |
| C29 | Major | Code to reuse was not named: `chrono::Local`, `unit_text`, `settings_path`, `with_popover`, `Pixmap::encode_png`. | cd-11, cd-22, zo-10 |
| C30 | Nit | Choices beyond the owner decisions: Minute rows as a choice of the export, a third tier of hourly rows. | zo-7, ic-11 |
| C31 | Nit | View ids, the id counter and the read limit of 1 MiB protect nothing. Two decisions had no rejected alternative. Year periods had no length in days. Two risks were missing (a `.part` file after a stop, a formula in a CSV field). ADR-0005 states the two times as facts. | zo-8, zo-15, st-10, st-11, st-16, st-17, dc-9, dc-12, iz-10 |
| C32 | Nit | One rule for all history operations stops more than its one conflict. A newer database stops the recording, and only the History screen says so. Numbers are in the design and in the specs. | ic-9, ic-10, st-12, iz-5 |

## Resolution

Categories: FIXED (the files of the change were edited), CONTESTED (kept, with the reason), ANSWERED (the owner decided, and the files of the change hold the answer).

| # | Category | Resolution |
|---|---|---|
| C1 | FIXED | The Context links issue #89 of the repository `CappyT/nicecooling`, which holds the owner decisions in its comments of 2026-11-25, and ADR-0007 and ADR-0005. The two rejections have a technical reason. |
| C2 | FIXED | Task 2.2 removes the constants together with `Resolution::of_window`. |
| C3 | FIXED | Design H10: only a delete that the user confirmed compacts. The copy is a file in the directory of the store (`SQLITE_TMPDIR`, `temp_store = FILE`). The service thread reads the free space first and needs twice the pages in use. The WAL file has a limit of 64 MiB. Spec "Compaction" and "Compaction that does not run or fails". Task 3.5. |
| C4 | ANSWERED | Owner answer 1 below. Design H10, spec "Compaction", task 3.5 and the row "Disk space after a delete" of the product choices have the rule of 25% and 64 MB. The free space check of C3 stays. |
| C5 | CONTESTED: owner decision | The row "What a Note is" of the product choices puts a Note in `history.db`, and the owner changed no row. Design H14 now gives the reason against the Views case: a Note belongs to a time of the data and leaves with the file. The conflict with the spec is fixed: the Note commands are off while a history operation runs. |
| C6 | FIXED | Design H5 and H3: `SettingsStore` shares the history settings behind a mutex, the service thread reads them at each step, and the app writes the setting before it sends the prune request. |
| C7 | FIXED | Design H5 and H11: a writer open for the service thread and a read-only reader open for every read. Task 1.3 with `OpenError`. |
| C8 | FIXED | Design H15: one chart layout gives the shapes, and the screen painter and the SVG writer both use them. Task 6.4. |
| C9 | CONTESTED: owner decision | The row "Colors of a Series" has the custom color, and owner decision 5 asks for a color per Series. The dash is now a rule of the position (design H13, spec "Series style"). |
| C10 | FIXED | Design H18: the Samples wait in the event channel and keep their times. The service thread closes a minute from the Sample times, and "events were lost" is its own result. Spec "Samples during a long operation". Task 1.8. |
| C11 | FIXED | Design H8 and H9: the service thread keeps the range and drops each Sample of it that arrives later. The delete of the whole history drops the minute sums too. Tasks 3.3 and 3.4. |
| C12 | ANSWERED | Owner answer 2 below. The two periods are two keys in the table `meta` of `history.db`. Design H2, H3, H5 and H11, the design section "Storage", the spec requirements on the periods, and tasks 1.1 to 1.5, 1.7, 2.1, 2.4, 4.2 and 5.1. |
| C13 | FIXED | Design H19: one statement per Series on the primary key, with a commit after each Series. Tests read the query plan. Task 3.6 measures the count. |
| C14 | FIXED | The Context has the path of an event, the snapshot and the glossary terms. The section "Terms" defines the shown Series, a window, the app window, the two channels and the list of not recorded sensors. The dialog lists the ids of the list that the snapshot does not hold. |
| C15 | FIXED | Design H5 states what a second process does not get, and H9 keeps the table `series`, so an id never names another sensor. `docs/app.md` says it (task 8.3). |
| C16 | FIXED as a stated limit | The design section "Order with the other changes" and the risks state both cases. The store keeps one Series per id, and the user deletes the old rows. No code of this change prevents it. |
| C17 | FIXED | The design has the section "Order with the other changes": `add-node-graph` section 3 is merged first, no order is necessary with `add-dashboard-pages`, and each shared file has its rule. The tasks 1.3, 1.4, 1.8 and 2.2 name the callers of the other changes. |
| C18 | FIXED | Only the charts of the History screen draw a Note: glossary entry, design H14, proposal. |
| C19 | FIXED | One wording of each rule in the design and the specs. The specs got new requirements where one text was too long: "Stored Samples of a range", "Estimate of a period", "Size text", "Rows of the dialog Recorded sensors". |
| C20 | FIXED | The design section "Terms" fixes each word, and the specs, the tasks and the proposal use it. |
| C21 | FIXED in part | The design and the specs were rewritten. Some task items keep more than one action in a sentence. This does not block the implementation. |
| C22 | FIXED | The tasks name each type and function with its arguments: `OpenError`, `HistoryHandle::channel`, `send(request, guard)`, `begin`, `busy`, `HistoryService::new` and `step`, `set_prefs`, `prune_now`, `data_kind`, `window_kind`, `SettingsUi`, the fields of `HistoryUi`. |
| C23 | FIXED | Each task lists the files that its change breaks, and the preamble gives the rule for a text input and for a test file. |
| C24 | FIXED | Task 1.1 adds the term rows with their four words. Task 3.4 counts the Notes. The fixture of the old schema is in task 1.3. |
| C25 | FIXED | The service thread takes the free space function and the compaction function, and the export writer takes the function that opens its file. `set_sqlite_temp_dir` has the `unsafe` block, and the test checks the pure function. |
| C26 | FIXED | Each check is a command that fails on the current files and passes only with the work. |
| C27 | FIXED | Design H2 (size text, read of the size), H12 (first open), H15 and task 4.1 (section "Image" of the screen file, before the owner gate), spec requirements for each. |
| C28 | FIXED in part | The tasks got about 40 more tests and four more MANUAL steps for the scenarios. Two points have no direct check: the font that the image uses on a system with Inter and no Adwaita Sans (only the font list is tested), and "the memory of an export does not grow" (design H7). |
| C29 | FIXED | The tasks name each function to reuse. |
| C30 | CONTESTED: owner decision | The row "Data kind choice in the export" keeps the choice. Owner decision 1 has two periods and no third tier. |
| C31 | FIXED | A View has no id and the file has no read limit. H11 and H16 have a rejected alternative. A year is 365 days. The two risks are in the list. Task 8.3 changes the sentence of ADR-0005. |
| C32 | CONTESTED | One state "operation running" is simpler than one state per kind, and the design says so. A newer database is the result of a downgrade that the user did, and the charts of every screen are then empty. The specs hold the numbers, and the design repeats only those that a decision needs. |

### Owner answers

The record is [issue #89](https://github.com/CappyT/nicecooling/issues/89) of the repository `CappyT/nicecooling`, in its comments of 2026-11-25.

1. The trigger of the compaction (C4, design H10).
   - Question: `VACUUM` writes every row that stays. Which delete starts it: a delete of a fixed number of rows, a delete that leaves a share of the file free, or the incremental vacuum mode of SQLite?
   - Answer: the app compacts the database after a confirmed delete only when the free pages are 25% of the file or more and 64 MB or more. It reads `freelist_count` and `page_count`, with no table scan. The check of the free disk space before a compaction stays. A file of 10 GB with 23 MB of free pages does not compact. A file of 400 MB with 120 MB of free pages compacts.
2. The place of the two periods (C12, design H2).
   - Question: `AppSettings::load` gives the defaults for a `settings.toml` that is absent or does not parse. Are the two periods in that file, or in the table `meta` of `history.db`?
   - Answer: the Sample period and the Minute period are two keys in the table `meta` of `history.db`, written by the service thread. They are not in `settings.toml`. A settings file that is absent or broken changes no period and deletes no row. A database with no such keys gets the defaults 48 hours and 1 year at the first open. A key with a value that is not a choice reads as the default of that key, and the app does not prune its rows until the user confirms the period in the Settings screen. When the store is not open, the two period rows of the Settings screen are off with a text. The list of not recorded sensors stays in `settings.toml`.

## What stays open

- No reviewer saw the files after the fixes of round 2: the API names of tasks 1.4, 3.5 and 3.7, `SettingsUi` of task 5.1, and the last edits of the design and the specs. The first implementer of each section reports a task that does not fit the code as a question.
- The review chose these values, and the owner gate of task 4.3 or the owner must confirm them: the Italian, German and French words of the seven terms (task 1.1), the sizes of the image layout (task 4.1), the text "<n> of <m> recorded".
- Claims that no reviewer could prove from the files: `SQLITE_TMPDIR` and `journal_size_limit` on the SQLite that the app builds (3.54.1), the features of `resvg` 0.46.0, the pixel read of `gpui-pre`, that no rule stops a second app process, and the reuse of the id of a removed Derived sensor.
- The drift review runs before the archive of the change.

`openspec validate --all --strict` passes with no warning: `Totals: 5 passed, 0 failed (5 items)`. `python3 -I tools/re/check-identifiers.py` exits 0.
