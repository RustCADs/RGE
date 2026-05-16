# Task Packet

DISPATCH_ID: MAIN-ORDERED-JOB-009-KERNEL-CAVITY-AUDIT
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-14_03-37-08+0300
RELATED_FILES:
- ai_handoffs/MAIN-ORDERED-JOB-008-PHASE3-SOAK-READINESS_TASK_2026-05-14_03-37-07+0300.md
- kernel/shared/**
- kernel/asset-view/**
- kernel/asset-streaming/**
- kernel/job-system/**
- kernel/io-scheduler/**
- Status.md
- HANDOFF.md
STATUS: OPEN

## Goal

Audit the current kernel cavity/stub state and identify the next bounded kernel job, if any. The docs have historical references to partial cavities and stubs; this job refreshes the truth before scheduling kernel implementation.

## Scope

### MAY edit
- ai_handoffs/MAIN-ORDERED-JOB-009-KERNEL-CAVITY-AUDIT_EXEC_*.md

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

- List Tier-1 kernel crates and classify each as implemented, partial cavity, doctrine cavity, or empty stub.
- Identify whether any empty Tier-1 kernel stubs remain.
- Recommend the single next kernel job, if any.
- Recommend whether Job 10 should consolidate the roadmap or issue a new implementation task.

## Acceptance Criteria

- Job 8 closeout exists before this job executes.
- No tracked files are modified.
- EXEC packet includes a crate-by-crate classification table.
- Footer has exactly one `HANDOFF_STATUS: COMPLETE`.

## Constraints / Non-Goals

- Do not implement kernel cavities.
- Do not update docs.
- Do not create new lints or ADRs.
- Do not run workspace tests.

## Verification Gates

The Executor MUST run and document:

- Directory listing of `kernel/`
- Search for `Failure class:` declarations in kernel crate roots
- Search for empty/stub module docs in kernel crates
- `git status --short --untracked-files=no`

## Halt Conditions

The Executor MUST halt with `HANDOFF_STATUS: BLOCKED` and `NEXT_ROLE: PLANNER_AI` if:

- No Job 8 closeout exists.
- The classification cannot be made from code and docs.
- A kernel implementation decision is needed before a safe recommendation can be made.

## Planner Notes

This job may reveal that the old "remaining stubs" language is stale. Treat code as source of truth.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-ORDERED-JOB-009-KERNEL-CAVITY-AUDIT
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
