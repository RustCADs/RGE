# Task Packet

DISPATCH_ID: MAIN-ORDERED-JOB-007-CADPROJECTION-FOLLOWUP
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-14_03-37-06+0300
RELATED_FILES:
- ai_handoffs/MAIN-ORDERED-JOB-006-CADPROJECTION-GATE-AUDIT_TASK_2026-05-14_03-37-05+0300.md
- crates/cad-projection/**
- docs/§18/CAD_PROJECTION.md
- Status.md
- HANDOFF.md
- change.md
STATUS: OPEN

## Goal

Perform the smallest cad-projection follow-up identified by Job 6, if and only if Job 6 recommends proceeding. This may be a focused test hardening or a docs-only reconciliation, but it must not expand into new projection runtime/editor features.

## Scope

### MAY edit
- crates/cad-projection/**
- docs/§18/CAD_PROJECTION.md
- Status.md
- HANDOFF.md
- change.md
- ai_handoffs/MAIN-ORDERED-JOB-007-CADPROJECTION-FOLLOWUP_EXEC_*.md

### MUST NOT edit
- crates/cad-core/**
- crates/editor-shell/**
- crates/gfx/**
- kernel/**
- Cargo.toml
- Cargo.lock
- plans/PLAN.md
- plans/IMPLEMENTATION.md
- ai_handoffs/AI_HANDOFF_PROTOCOL.md
- ai_handoffs/templates/**

### MAY add new files
- Focused tests under `crates/cad-projection/tests/` only if Job 6 identifies an exact missing invariant.
- Exactly one EXEC packet for this dispatch under `ai_handoffs/`

### MUST NOT add new files
- New crates
- New ADRs
- New architecture lints
- New doctrine docs

## Deliverables

- Execute only the smallest follow-up named by Job 6.
- Keep changes within cad-projection and docs.
- Update Status/HANDOFF/change if a result lands.
- If no safe follow-up exists, write a BLOCKED EXEC packet and do no source edits.

## Acceptance Criteria

- Job 6 closeout exists and explicitly recommends proceeding.
- No files outside the MAY list are changed.
- cad-projection tests pass.
- Architecture lints pass with no new exemption count.
- If changes land, one local commit is permitted; do not push.

## Constraints / Non-Goals

- Do not add projection runtime/editor feature surfaces.
- Do not alter cad-core semantics.
- Do not touch renderer/editor-shell integration.
- Do not create a broad API redesign.

## Verification Gates

The Executor MUST run and document:

- `cargo +nightly fmt --check -p rge-cad-projection`
- `cargo test -p rge-cad-projection --all-targets --no-fail-fast`
- `cargo test --workspace --no-fail-fast`
- `cargo run -q -p rge-tool-architecture-lints -- all`
- `git diff --check`

## Halt Conditions

The Executor MUST halt with `HANDOFF_STATUS: BLOCKED` and `NEXT_ROLE: PLANNER_AI` if:

- No Job 6 closeout exists.
- Job 6 does not explicitly recommend proceeding.
- Work requires cad-core/editor/gfx changes.
- Architecture-lint exemption count shifts.

## Planner Notes

This is conditional. The audit decides whether it becomes implementation, docs-only reconciliation, or skip.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-ORDERED-JOB-007-CADPROJECTION-FOLLOWUP
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
