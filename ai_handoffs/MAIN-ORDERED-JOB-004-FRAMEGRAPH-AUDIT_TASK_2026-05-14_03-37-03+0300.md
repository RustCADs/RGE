# Task Packet

DISPATCH_ID: MAIN-ORDERED-JOB-004-FRAMEGRAPH-AUDIT
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-14_03-37-03+0300
RELATED_FILES:
- ai_handoffs/MAIN-ORDERED-JOB-003-STATUS-RECONCILE_TASK_2026-05-14_03-37-02+0300.md
- crates/gfx/src/frame_graph/**
- crates/gfx/tests/**
- Status.md
- HANDOFF.md
- plans/IMPLEMENTATION.md
- plans/BASELINE.md
STATUS: OPEN

## Goal

Read-only audit of the current Phase 6 frame-graph/transient-resource state. The docs contain historical references to frame-graph work; this job determines whether any real follow-up remains before implementation is attempted.

## Scope

### MAY edit
- ai_handoffs/MAIN-ORDERED-JOB-004-FRAMEGRAPH-AUDIT_EXEC_*.md

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

- Confirm whether `crates/gfx/src/frame_graph/` exists and summarize its public surface.
- Identify the smallest concrete frame-graph follow-up, or state that no follow-up is currently warranted.
- Recommend whether Job 5 should proceed, be corrected, or be skipped.
- Record relevant tests and docs that already cover frame-graph behavior.

## Acceptance Criteria

- Job 3 closeout exists before this job executes.
- No tracked files are modified.
- The EXEC packet gives a clear proceed/correct/skip recommendation for Job 5.
- Footer has exactly one `HANDOFF_STATUS: COMPLETE`.

## Constraints / Non-Goals

- Do not implement frame-graph changes.
- Do not update docs.
- Do not run the full workspace test suite unless needed to resolve ambiguity.

## Verification Gates

The Executor MUST run and document:

- File listing for `crates/gfx/src/frame_graph`
- Search for frame-graph tests under `crates/gfx/tests` and inline modules
- `git status --short --untracked-files=no`

## Halt Conditions

The Executor MUST halt with `HANDOFF_STATUS: BLOCKED` and `NEXT_ROLE: PLANNER_AI` if:

- No Job 3 closeout exists.
- Frame-graph state is ambiguous enough that Job 5 would be stale or unsafe.
- The audit finds the docs contradict the code in a way that requires Planner correction.

## Planner Notes

This job prevents unnecessary implementation against an already-closed or differently-shaped frame-graph substrate.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-ORDERED-JOB-004-FRAMEGRAPH-AUDIT
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
