# Task Packet

DISPATCH_ID: MAIN-ORDERED-JOB-001-PREFLIGHT
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-14_03-37-00+0300
RELATED_FILES:
- HANDOFF.md
- Status.md
- ai_handoffs/AI_HANDOFF_PROTOCOL.md
- ai_handoffs/templates/EXECUTION_REPORT.md
STATUS: OPEN

## Goal

Establish the exact current repository and protocol state before any further main-project work. This is Job 1 of the ordered 10-job queue. It is read-only and exists to prevent stale assumptions from propagating into later jobs.

## Scope

### MAY edit
- ai_handoffs/MAIN-ORDERED-JOB-001-PREFLIGHT_EXEC_*.md

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
- Additional task packets

## Deliverables

- Produce one EXEC packet summarizing current local-vs-origin state.
- Record the three local-only commits, if still present.
- Record whether the tracked tree is clean.
- Record whether `cargo` is available in the Executor environment and its path/version if available.
- Record the current protocol rule for pre-execution review after commit `d017a35`.
- Recommend whether Job 2 can proceed unchanged.

## Acceptance Criteria

- No tracked files are modified.
- The EXEC packet references this TASK by filename.
- `git status --short --untracked-files=no` is recorded.
- `git rev-list --left-right --count origin/main...HEAD` is recorded.
- `git log --oneline origin/main..HEAD` is recorded.
- Cargo availability is recorded without running the workspace test suite.
- Footer has exactly one `HANDOFF_STATUS: COMPLETE`.

## Constraints / Non-Goals

- Do not push.
- Do not commit.
- Do not run expensive tests.
- Do not modify Status/HANDOFF/change.
- Do not start Job 2 from inside this job.

## Verification Gates

The Executor MUST run and document:

- `git status --short --untracked-files=no`
- `git rev-list --left-right --count origin/main...HEAD`
- `git log --oneline origin/main..HEAD`
- `git show --stat --oneline HEAD`
- `Get-Command cargo` or equivalent cargo availability check

## Halt Conditions

The Executor MUST halt with `HANDOFF_STATUS: BLOCKED` and `NEXT_ROLE: PLANNER_AI` if:

- The working tree has tracked modifications not explained by this queue.
- Local `HEAD` is not `d017a35` or a known descendant.
- The protocol file does not contain Rule 7 single pre-execution reviewer text.
- Any command indicates repository corruption or an unreadable git state.

## Planner Notes

This job intentionally does not change project state. It is the anchor for the remaining nine jobs.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-ORDERED-JOB-001-PREFLIGHT
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
