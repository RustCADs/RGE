# Task Packet

DISPATCH_ID: MAIN-ORDERED-JOB-002-PUBLISH-READINESS
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-14_03-37-01+0300
RELATED_FILES:
- ai_handoffs/MAIN-ORDERED-JOB-001-PREFLIGHT_TASK_2026-05-14_03-37-00+0300.md
- HANDOFF.md
- Status.md
- change.md
STATUS: OPEN

## Goal

Prepare a push-readiness report for the three local-only commits without pushing them. This is Job 2 and depends on Job 1 closeout. Its purpose is to make the push/no-push decision explicit before the queue creates more local commits.

## Scope

### MAY edit
- ai_handoffs/MAIN-ORDERED-JOB-002-PUBLISH-READINESS_EXEC_*.md

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

- Confirm Job 1 has a Planner closeout or halt.
- Summarize the local-only commits and whether each is safe to push.
- Identify any commit that should remain local pending user decision.
- Recommend a push command, but do not run it.
- State clearly that no push was performed.

## Acceptance Criteria

- Job 1 closeout exists before this job executes.
- No tracked files are modified.
- No push is performed.
- The EXEC packet includes a concise push-readiness verdict.
- Footer has exactly one `HANDOFF_STATUS: COMPLETE`.

## Constraints / Non-Goals

- Do not push.
- Do not commit.
- Do not alter the local branch.
- Do not rewrite history.
- Do not stage files.

## Verification Gates

The Executor MUST run and document:

- `git status --short --untracked-files=no`
- `git rev-list --left-right --count origin/main...HEAD`
- `git log --oneline origin/main..HEAD`
- `git remote -v`

## Halt Conditions

The Executor MUST halt with `HANDOFF_STATUS: BLOCKED` and `NEXT_ROLE: PLANNER_AI` if:

- No Job 1 closeout exists.
- Job 1 reports a dirty tracked tree.
- Any local-only commit is not understood from HANDOFF/Status/handoff packets.
- A push would be required to proceed.

## Planner Notes

This job is a guardrail, not a publish action. The human decides whether the local commits are pushed.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-ORDERED-JOB-002-PUBLISH-READINESS
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
