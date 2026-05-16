# Task Packet

DISPATCH_ID: MAIN-ORDERED-JOB-003-STATUS-RECONCILE
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-14_03-37-02+0300
RELATED_FILES:
- ai_handoffs/MAIN-ORDERED-JOB-002-PUBLISH-READINESS_TASK_2026-05-14_03-37-01+0300.md
- Status.md
- HANDOFF.md
- change.md
- ai_handoffs/AI_HANDOFF_PROTOCOL.md
STATUS: OPEN

## Goal

Reconcile the visible project status docs after the protocol v2 amendment and the three local-only commits. This is a docs-only Job 3 and depends on Jobs 1 and 2 closing cleanly.

## Scope

### MAY edit
- Status.md
- HANDOFF.md
- change.md
- ai_handoffs/MAIN-ORDERED-JOB-003-STATUS-RECONCILE_EXEC_*.md

### MUST NOT edit
- Any source file
- Any test file
- Cargo.toml
- Cargo.lock
- plans/PLAN.md
- plans/IMPLEMENTATION.md
- plans/BASELINE.md
- ai_handoffs/AI_HANDOFF_PROTOCOL.md
- ai_handoffs/templates/**

### MAY add new files
- Exactly one EXEC packet for this dispatch under `ai_handoffs/`

### MUST NOT add new files
- Source files
- Test files
- ADRs
- Architecture lints
- Doctrine docs

## Deliverables

- Add a concise status entry that local `main` is three commits ahead of origin unless Job 2 says otherwise.
- Record that protocol v2 Rule 7 is live.
- Record that future Claude executions should use `Pre-Execution Review Consumed` instead of duplicate Reviewer2 approval when concurring.
- Update `change.md` with the docs reconciliation.

## Acceptance Criteria

- Job 2 closeout exists before this job executes.
- Docs do not claim local `main` is five commits ahead.
- No source/test/build files are changed.
- If docs change, one local commit is permitted; do not push.
- Footer has exactly one `HANDOFF_STATUS: COMPLETE`.

## Constraints / Non-Goals

- Do not alter protocol text in this job.
- Do not push.
- Do not make roadmap decisions beyond status reconciliation.
- Do not run expensive tests unless the Executor changed more than docs.

## Verification Gates

The Executor MUST run and document:

- `git diff --check`
- `git status --short --untracked-files=no`
- If committing: `git show --stat --name-status --oneline HEAD`
- `git rev-list --left-right --count origin/main...HEAD`

## Halt Conditions

The Executor MUST halt with `HANDOFF_STATUS: BLOCKED` and `NEXT_ROLE: PLANNER_AI` if:

- No Job 2 closeout exists.
- The docs need source-of-truth reconciliation that conflicts with Status or HANDOFF.
- The local-ahead count differs from Job 2 and cannot be explained.

## Planner Notes

This is the only docs-update job in the first queue segment. It should leave the repo easier to resume.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-ORDERED-JOB-003-STATUS-RECONCILE
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
