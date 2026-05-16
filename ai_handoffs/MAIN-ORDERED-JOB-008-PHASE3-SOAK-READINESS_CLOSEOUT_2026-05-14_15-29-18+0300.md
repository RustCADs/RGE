# CLOSEOUT: MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS

DISPATCH_ID: MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-14_15-29-18+0300
STATUS: CLOSED
RELATED_TASK: MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS_TASK_2026-05-14_03-37-07+0300.md
RELATED_EXEC: MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS_EXEC_2026-05-14_15-24-57+0300.md
RELATED_REVIEW: MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS_REVIEW_2026-05-14_15-29-17+0300.md
RELATED_FILES:
- crates/script-bench/src/script_host.rs
- crates/script-bench/BASELINE.md
- plans/IMPLEMENTATION.md
- docs/§18/SCRIPT_HOST.md

## Outcome

Job 8 is CLOSED.

The Phase 3 one-hour soak was already run and closed before this dispatch. The accepted source of truth is:

- `crates/script-bench/BASELINE.md`: formal soak RUN 2026-05-12, 3600.00 s wall-clock, approximately 4.4M cycles, all assertions held.
- `plans/IMPLEMENTATION.md:318`: one-hour memory soak CLOSED 2026-05-12.
- `plans/IMPLEMENTATION.md:319`: component preservation CLOSED and reasserted in the soak.

No one-hour rerun was performed, and no rerun is currently warranted.

## Verification

- EXEC footer poll: complete.
- Tracked tree: clean.
- Ahead count: still `0 4`.
- No tracked file edits from Job 8.
- No commit.
- No push.
- No `crates/script-host` or `crates/script-bench` source changes exist in the current local commit stack.

## Carry-Forward Notes

Non-blocking docs drift remains in `docs/§18/SCRIPT_HOST.md`: it still uses older wording around the one-hour soak being compiled/ignored and Phase 3 gates being deferred. This does not block Job 8 because authoritative closeout records are already present in `plans/IMPLEMENTATION.md` and `crates/script-bench/BASELINE.md`, and Job 8 was read-only.

Future soak rerun triggers:

- Material `crates/script-host` or `crates/script-bench` source change.
- Formal v0 release certification.
- Wasmtime version bump.
- Future RSS/VSS memory instrumentation improvement.

## Next Job Decision

Job 9, `MAIN-ORDERED-JOB-009-KERNEL-CAVITY-AUDIT`, may proceed unchanged.

Job 10 remains held until Job 9 is closed or formally skipped.

HANDOFF_STATUS: COMPLETE
NEXT_ROLE: NONE
EXIT_CODE: 0
