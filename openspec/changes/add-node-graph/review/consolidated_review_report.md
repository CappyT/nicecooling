# Consolidated review report: add-node-graph

Date: 2026-11-24 and 2026-11-25. Review mode: after `design.md` and `tasks.md` were written and before implementation.

Each reviewer ran in a fresh context with only the change files, the glossary, the ADRs, the code and the writing style. Thirteen reviewer runs, all completed:

| Stage | Target | Reviewers | Result |
|---|---|---|---|
| 5 Initial zoom out | design.md, first version | zoom-out, inventive catalyst | 0 Blocker, 20 Major |
| 6 Comprehension | design.md, first version | comprehension | 9 Major |
| 7 Critic | design.md, first version | design, structural, style | 2 Blocker, about 40 Major |
| 8 Intermediate zoom out | design.md after the rewrite | zoom-out | 0 Blocker, 7 Major |
| 9 Implementation readiness, round 1 | tasks.md | readiness, codebase details, style | 4 Blocker, 32 Major rows |
| 9 Implementation readiness, round 2 | tasks.md after the rewrite | readiness, codebase details, style | 3 Blocker, 18 Major |

The stage reports are next to this file and are not in git. The design was rewritten between stage 7 and stage 8 because the owner moved the Graph engine from the daemon to the app. Stage 8 is the one re-audit of the design, and round 2 is the one re-audit of the tasks. No reviewer checked the state after the last fixes of round 2.

## Consolidated findings

Priority: Blocker stops the implementation, Major changes what is built, Nit is wording.

| # | Priority | Finding | Sources |
|---|---|---|---|
| C1 | Blocker | The protection against a poweroff or reboot loop did not work: the minimum interval and the run state lived in memory, and no cancel is possible before the login. | dc-2, st-12, ic-2, co-4, iz-1, iz-2 |
| C2 | Blocker | Decisions had no reason or alternative: "rejected by the owner", and no alternatives for the editor, the dispatch of responses, the place of the network clients, the place of the evaluation. | dc-1, st-1, st-2, st-3, sy-8 |
| C3 | Blocker | The Graph sensor D-Bus contract was not defined, and the specs disagreed on the unit. | ir-B1, cd-B3, ir-M9, r2-M5 |
| C4 | Blocker | The screens were built before their screen files existed and before the owner approved them. | ir-B2 |
| C5 | Blocker | The tasks named files and functions that do not hold the code: the consumer checks, the process scan, the Derived source check, the contract test with its count of interfaces. | cd-B1, cd-B2, ir-M3, r2-B1, r2-M11 |
| C6 | Blocker | The link between the Graph service thread and the window, the tray, the notifications, the daemon client and the stop of the app was not decided. | r2-B3, r2-M14, ir-M1, cd-M2 |
| C7 | Blocker | A verify step failed on the current code (the daemon uses `serde_json` today). | r2-B2 |
| C8 | Major | Network clients, user logic and a command runner in the root daemon. | ic-5, dc-11, st-11 |
| C9 | Major | No fault and cost isolation of a Graph inside the control tick; an unbounded window memory. | ic-1, dc-5 |
| C10 | Major | A Curve could get a number that no sensor measured: an end node input with the default 0, the "keep the value over a restart" option, old samples in a window after a gap. | zo-6, zo-7, ic-7, dc-6 |
| C11 | Major | A Graph could delay the poweroff of an Alarm; two countdowns had no rule. | ic-3, zo-4 |
| C12 | Major | A response could run twice: a broadcast to every app, and a rerun after a restart of the daemon or a lost value. | ic-4, co-7, iz-1 |
| C13 | Major | A Graph could not see a lost sensor. | ic-6 |
| C14 | Major | Reuse claims did not match the code: the Calibration interpolation, the comparison tolerance, the hold logic, the history writer, a process scan in the app, a monotonic clock for the app. | zo-3, zo-13, dc-8, iz-4, iz-5, iz-8, iz-9, ir-M4, cd-M9, r2-M2, r2-M12 |
| C15 | Major | The position of the Graph stage in the tick differed between design and spec; the clock was named as wall clock and simulated at once. | zo-1, zo-2, dc-3, dc-4, st-7, st-8, ic-10, ic-11 |
| C16 | Major | The consumers of a Graph sensor were not one list; a loop through a Derived sensor; a measured Loop sensor replaced by a Graph sensor; the values call could write a measured sensor. | iz-3, ir-M18, r2-M1 |
| C17 | Major | "Action" and "Test" collided with glossary terms; "feed" collided with Feed; new terms had no glossary entry. | zo-12, dc-13, st-13, sy-1, st-M1, st-M2 |
| C18 | Major | The promised list of extended requirements, the changed contract files, the storage inventory and a migration plan were missing. | zo-10, zo-14, zo-15, dc-9, dc-10, st-4, st-5, st-10, iz-13, ir-M10, cd-M4 |
| C19 | Major | The daemon could not know which sensors are hidden; MQTT queue and acknowledgment with the client crate were not shown to work; unit text, clear message and Alarms active at start were open. | dc-7, dc-12, ic-9, r2-M6 |
| C20 | Major | The polkit cache: premature and wrong after a user switch; a dialog each second for a user outside wheel; tests that the mock cannot run. | iz-7, ir-M17, r2-M10 |
| C21 | Major | The model and the specs missed data that the tasks need: node label, link to the Graph sensor id, units, parameter types and ranges, refusal codes, generator formulas, file formats, environment variables. | co-3, ir-M11, ir-M12, ir-M13, ir-M14, ir-M16, cd-M14, r2-M9 |
| C22 | Major | Dependencies were not chosen; the copied canvas code against the workspace lints; the spike needs a second dependency line. | ir-M5, cd-M3, cd-M13, r2-M13, dc-12 |
| C23 | Major | Tasks: incomplete file lists, strings in the wrong file, work split over two tasks, tests before the code they need, requirements with no task, test files not named. | ir-M1, ir-M2, ir-M7, ir-M15, cd-M1, cd-M7, r2-M15, r2-M16 |
| C24 | Major | Pass rules of MANUAL tasks that could not be measured or that correct code fails. | st-M10..12, r2-M18 |
| C25 | Major | Export and import had no stated need. | zo-8, iz-6 |
| C26 | Major | The design repeated spec numbers and some copies differed; the kind count was 48, 49 and 50 in one document. | zo-11, co-1, co-2, st-6, st-9, sy-2 |
| C27 | Major | Open cases of the model: an unknown kind in the stored file, the interpolation for a not-a-number input, the repeat after a restart, a Try source without a test value, the start-up comparison of Graph sensors. | r2-M4, r2-M7 |
| C28 | Nit | Style: bold inside sentences, long and verbless sentences, history in the text, passive voice, undefined words. | zo-16, st-16..24, sy-3..32, st-M6, r2-M17 |

## Resolution

Categories: FIXED (the files of the change were edited), DECIDED (the owner chose), CONTESTED (kept, with the reason).

| # | Category | Resolution |
|---|---|---|
| C1 | DECIDED, FIXED | The owner chose to add a Response pause as the maintenance mode of aquasuite (issue #88, decision 10), after the first answer rested on a wrong premise. The run state and the last run time are stored. Power kinds have a default minimum interval of 600 s. The pause counts poweroff and reboot runs and resets after 60 minutes of stored app run time. A response holds its state while its input has no value. Design G3, G8; spec graph-responses. |
| C2 | FIXED | Each decision has a reason and its rejected alternatives. The owner decisions are numbered in issue #88 and cited by number. |
| C3 | FIXED | Design G14 has the four members with signatures, the origin, the unit rule, the id counter, the no-value entry and the name rule. |
| C4 | FIXED | The screen files and an owner approval come before the section that builds the screens (tasks 7.5, 7.6). |
| C5 | FIXED | The tasks name `hub.rs` with its functions, `check_derived`, `dbus_contract.rs` as modified, and move the process scan to `nicecooling-core`. |
| C6 | FIXED | Design G15 names each link of the Graph service and the one stop path. Task 3.6. |
| C7 | FIXED | The rule is: the daemon does not depend on `nicecooling-graph`. |
| C8 | DECIDED, FIXED | The owner decided that Graphs, responses and network clients run in the app and never in the root daemon (issue #88, decision 5). Design G1. |
| C9 | FIXED | The evaluation is in the app, outside the control tick, with a panic boundary per Graph and a limit of 200 000 window samples. Design G2, G7. |
| C10 | FIXED | An end node input has no default. The "keep the value" option is removed. A window removes every sample older than its span. Design G3, G5. |
| C11 | FIXED | The alarm poweroff wins: no Graph power response during its countdown, and a Graph countdown is cancelled. Design G8. |
| C12 | FIXED | No broadcast exists. Only the app of the active session runs responses. The stored run state and the no-value rule stop a rerun. |
| C13 | DECIDED, FIXED | The owner chose the node "has value" (issue #88, decision 4). |
| C14 | FIXED | The interpolation comes from the Curve, the compare function and the hold logic become usable in core, the process scan moves to core, a system monotonic clock and a wall-clock trait are added, the atomic write of the app settings is the file writer. |
| C15 | FIXED | No tick stage exists. A Graph sensor reaches the value table as a source. The clocks are named in design G6. |
| C16 | DECIDED, FIXED | The owner chose that the app sends only the number (issue #88, decision 6). The daemon takes a Graph sensor as a Curve Input, a Profile rule source and a Software sensor, refuses it as a Derived source and as a Loop sensor, and takes values only for ids of Graph sensors. Design G4, G14. |
| C17 | FIXED | "Graph response", "Try", "Run once", "Graph sensor values". The design has a Terms table, and a task adds the glossary entries. |
| C18 | FIXED | The design lists the extended capabilities, the contract files, the storage table and the migration with the downgrade result that the code gives. No stored data is converted: the Derived sensors stay (issue #88, decision 7). |
| C19 | FIXED | MQTT is in the app, which has the hidden list. One pending value per sensor topic and a queue for events. An MQTT spike is the first MQTT task and can change the spec before the client is built. Design G9, G15. |
| C20 | FIXED | No cache. The values call has its own path without the interactive flag. The check with a user outside wheel is a MANUAL step. |
| C21 | FIXED | The specs hold the label rule, the link to the Graph sensor id, the units, the parameter types and ranges, 19 refusal codes, the generator formulas, the file formats and the environment variables. |
| C22 | FIXED | `rumqttc` and `lettre` with `rustls`, `fastrand`, `rcgen`, `libc`, `serde_json` are named. The task that adds a crate pins the release it finds. The copied crate has a lint allow and an entry in `THIRD_PARTY.md`. |
| C23 | FIXED | The task list was rewritten twice: 89 tasks in 10 sections, each with its files marked create or modify and a Verify line, one owner per test name. |
| C24 | FIXED | The pass rules state what a person sees, with times that fit the smoothing of a Curve, the 3 s staleness and the keep-alive of MQTT. |
| C25 | CONTESTED | Export and import stay. aquasuite has them, and the aim of the change is to have what aquasuite has (issue #88, decision 1). They are in the app only and add no daemon code. Design G12 states the need as a Goal. |
| C26 | FIXED | The design gives decisions and reasons and points to the specs for numbers. The model has 51 kinds: 49 of aquasuite, the alarm event source and "has value". |
| C27 | FIXED | Design G2, G5, G8, G15 decide each case. |
| C28 | FIXED in part | The design and the specs were rewritten in the project style. The tasks keep some long sentences. This does not block the implementation. |

## What stays open

- The editor spike (design G13) and the MQTT spike (design G9) decide two parts of the design. Both are the first task of their section.
- No reviewer saw the specs and the tasks after the last pass of round 2. The validator passes, and the agent that made the pass reports every round 2 finding as resolved. The first implementer of each section reports a task that does not fit the code as a question.
- The drift review runs before the archive of the change.

`openspec validate add-node-graph --strict` passes with no warning.
