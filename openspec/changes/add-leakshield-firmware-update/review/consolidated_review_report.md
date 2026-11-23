# Consolidated review report: add-leakshield-firmware-update

Spec review with the EGM goldfish reviewers (opus, high), 2026-11-23.

| Stage | Report | Result |
|---|---|---|
| 5 Zoom out | `zoom_out_report.md` | 9 Major, 7 Nit |
| 6 Comprehension | `comprehension_report.md` | 8 Major, 5 Nit |
| 7 Critic, round 1 | `critic_report.md` | 1 Blocker, 33 Major, many Nits |
| 7 Critic, round 2 | `critic_round2_report.md` | 13 Major, 6 Nit |
| 8 Final zoom out | `final_zoom_out_report.md` | 6 Major, 6 Nit |
| Readiness | `readiness_report.md` | 1 Blocker, 19 Major, 7 Nit |
| Verification | `verification_report.md` | 8 Major, 11 Nit, all resolved |

## Owner decisions

Recorded in [issue #70](https://github.com/CappyT/nicecooling/issues/70), comments of 2026-11-23:

1. The app passes a file descriptor; the daemon reads it bounded into memory with a defensive SQLite open.
2. A failed transfer leaves the device in the bootloader; the daemon offers a resume.
3. `leakshield-unreachable` is held while the job runs and forced to `notify` while the update is interrupted.
4. `unverified_target` stays behind a second confirmation.
5. No deliberate bad frame to the real device.
6. The daemon's Alarm poweroff takes priority over the update.
7. `ListFirmwareImages` is class `write`.
8. A daemon stop stops the job at once.
9. On a resume, the same-version check does not apply, and the downgrade check compares with `firmware_before` of the record.

## Resolution of the main findings

| Finding | Resolution |
|---|---|
| Traffic during the flash from many users (zoom out 1, 3; catalyst 1; critic 2, M3, M4) | One lease on `DeviceIo` (D6, D7): every other user gets a busy transport; the worker flushes save-settings, waits for `settings_dirty` 0, drops and fails Profile requests. |
| Path opened by root (zoom out 3; catalyst 2; critic 9; M9) | File descriptor, 16 MiB bound, 10 s read limit, one parse at a time, defensive SQLite, table check, class `write` (D2). |
| Alarm in the bootloader and after device-lost (catalyst 3; critic 3; M2) | `UpdateState` `interrupted` with the forced `notify` on the Alarm instance, resolved in `alarms/response.rs` (D7). |
| Alarm poweroff and the inhibitor (final 5; M1) | The Alarm takes priority (owner decision); the lock blocks user and system requests only (D6, Risks). |
| Language change refused as downgrade (critic 1) | Check 4 refuses only a lower version (D4). |
| Identity precedence and partial identity (readiness 8; M10) | Identity rules first, an absent field skipped only with `unverified_target`, a known field still checked (D4). |
| Blocking HID calls and the test clock (zoom out 2; critic 11; readiness Blocker) | `spawn_blocking` for each HID call, `std::time::Instant` for the chunk time, tokio sleeps for the waits; tokio 1.54 inhibits auto-advance during a blocking task (D6, D12). |
| 300 ms limit and the 5 s kernel timeout (critic 6) | A slow return fails the chunk without a retry (D6). |
| Missing 2 s settle (critic 7) | Step `settle` (D6). |
| Bootloader detection and attach (critic 8, 13; M5; final 2) | The `Attacher` classifies by report 1 on HID interface 1, at start and on hot-plug, with a HID-only open; the record is found by USB path or serial; the job waits on the `Info` watch (D8). |
| Report 5 and report 1 size after the flash (M6, M7; final 12) | Read-back of reports 3 and 8; identity fields of a report 1 of any length; unreadable reports listed (D9). |
| Shutdown, suspend, upgrade (critic 12; M13) | Inhibitor lock; a stop stops the job at once (owner decision), finishing the transfer rejected (D6). |
| Tasks not executable (readiness) | Tasks rewritten: exact dependencies, APIs, schema, error order, files, mechanical Verify lines, gate scope, missing scenario tests. |

## Open after this review

- OQ1, OQ2, OQ3 of `design.md` stay open until the capture (gate 1.6).
- Gate 1.6 runs `spec-review` again on the parts that the capture changes.
