# Main Ordered Serial State

AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-14_03-52-01+0300
STATUS: ACTIVE

## Purpose

This file is the serial control note for the `MAIN-ORDERED-JOB-001` through `MAIN-ORDERED-JOB-010` queue. It exists because both agents correctly stopped at a serial boundary after Job 1; the queue now needs an explicit "current active job" signal.

## Serial Rule

Only one job is active at a time.

The active job may proceed only if every earlier job has either:

- an OpenAI `CLOSEOUT` packet with `HANDOFF_STATUS: COMPLETE`, or
- a formal Planner `CORRECT`, `BLOCKED`, `SKIP`, or human-direction packet.

Do not execute a later job from stale assumptions. After each closeout, later queued jobs must be reconsidered before the next release.

## Current State

- Job 1: CLOSED by `ai_handoffs/MAIN-ORDERED-JOB-001-PREFLIGHT_CLOSEOUT_2026-05-14_03-48-30+0300.md`
- Job 2: RELEASED NOW
- Jobs 3-10: HELD

## Active Job

Claude should execute exactly this task next:

- `ai_handoffs/MAIN-ORDERED-JOB-002-PUBLISH-READINESS_TASK_2026-05-14_03-37-01+0300.md`

## Job 2 Constraints Reminder

- Read-only push-readiness report.
- No tracked edits.
- No commit.
- No push.
- No staging.
- Produce exactly one EXEC packet.
- Footer should route to `NEXT_ROLE: REVIEWER_AI`.

## After Job 2

Claude should stop after Job 2 EXEC. OpenAI will review and close or correct Job 2, then release exactly one next job.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-ORDERED-SERIAL-STATE
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
