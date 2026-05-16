# Task Packet

DISPATCH_ID: MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-14_03-37-07+0300
RELATED_FILES:
- ai_handoffs/MAIN-ORDERED-JOB-007-CADPROJECTION-FOLLOWUP_TASK_2026-05-14_03-37-06+0300.md
- crates/script-bench/**
- crates/script-host/**
- docs/§18/SCRIPT_HOST.md
- plans/BASELINE.md
- Status.md
- HANDOFF.md
STATUS: OPEN

## Goal

Prepare the Phase 3 one-hour soak / release-readiness decision without automatically running a one-hour test. This job determines whether the soak should be run now, deferred, or converted into a CI/release-readiness task.

## Scope

### MAY edit
- ai_handoffs/MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS_EXEC_*.md

### MUST NOT edit
- Any tracked repository file
- Any source file
- Any test file
- Status.md
- HANDOFF.md
- change.md

### MAY add new files
- Exactly one EXEC packet for this dispatch under `ai_handoffs/`

### MUST NOT add new files
- Source files
- Test files
- ADRs
- Architecture lints
- Doctrine docs

## Deliverables

- Summarize current Phase 3.3/3.4 gate status from code and docs.
- Locate the ignored one-hour soak test or confirm it is absent.
- Recommend whether to run the soak as a later explicit human-approved command.
- Recommend whether Job 9 should proceed unchanged.

## Acceptance Criteria

- Job 7 closeout exists, or Job 7 is formally blocked/skipped by Planner.
- No tracked files are modified.
- No one-hour soak is run.
- The EXEC packet gives a clear recommendation.
- Footer has exactly one `HANDOFF_STATUS: COMPLETE`.

## Constraints / Non-Goals

- Do not run a one-hour test.
- Do not change script-host or script-bench.
- Do not update baselines.
- Do not commit.

## Verification Gates

The Executor MUST run and document:

- Search for ignored Phase 3 soak tests.
- Search for Phase 3 gate status in Status/HANDOFF/BASELINE/docs.
- `git status --short --untracked-files=no`

## Halt Conditions

The Executor MUST halt with `HANDOFF_STATUS: BLOCKED` and `NEXT_ROLE: PLANNER_AI` if:

- Prior jobs are not closed or formally skipped.
- The Phase 3 docs and code disagree in a way that needs Planner correction.
- Running tests would exceed the intended read-only readiness scope.

## Planner Notes

This job protects the queue from accidentally launching a long-running soak.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
