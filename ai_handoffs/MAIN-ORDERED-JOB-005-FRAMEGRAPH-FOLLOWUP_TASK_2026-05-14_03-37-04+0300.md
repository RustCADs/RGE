# Task Packet

DISPATCH_ID: MAIN-ORDERED-JOB-005-FRAMEGRAPH-FOLLOWUP
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-14_03-37-04+0300
RELATED_FILES:
- ai_handoffs/MAIN-ORDERED-JOB-004-FRAMEGRAPH-AUDIT_TASK_2026-05-14_03-37-03+0300.md
- crates/gfx/src/frame_graph/**
- crates/gfx/tests/**
- Status.md
- HANDOFF.md
- change.md
STATUS: OPEN

## Goal

Implement the smallest frame-graph follow-up identified by Job 4, if and only if Job 4 recommends proceeding without correction. If Job 4 says frame-graph is closed or ambiguous, this job must not perform source edits.

## Scope

### MAY edit
- crates/gfx/src/frame_graph/**
- crates/gfx/tests/**
- Status.md
- HANDOFF.md
- change.md
- ai_handoffs/MAIN-ORDERED-JOB-005-FRAMEGRAPH-FOLLOWUP_EXEC_*.md

### MUST NOT edit
- crates/gfx/src/lit_mesh_pipeline.rs
- crates/gfx/src/pso_cache.rs
- crates/editor-shell/**
- crates/cad-core/**
- kernel/**
- Cargo.toml
- Cargo.lock
- plans/PLAN.md
- plans/IMPLEMENTATION.md
- plans/BASELINE.md
- ai_handoffs/AI_HANDOFF_PROTOCOL.md
- ai_handoffs/templates/**

### MAY add new files
- New tests under `crates/gfx/tests/` only if Job 4 identifies an exact missing regression boundary.
- Exactly one EXEC packet for this dispatch under `ai_handoffs/`

### MUST NOT add new files
- New crates
- New ADRs
- New architecture lints
- New doctrine docs
- New production modules outside `crates/gfx/src/frame_graph/`

## Deliverables

- Execute only the smallest follow-up named by Job 4.
- Add or adjust focused tests for the changed frame-graph invariant.
- Update Status/HANDOFF/change only if a code/doc result lands.
- If no safe follow-up exists, write a BLOCKED EXEC packet and do no source edits.

## Acceptance Criteria

- Job 4 closeout exists and explicitly recommends proceeding.
- No files outside the MAY list are changed.
- Architecture lints pass with no new exemption count.
- Workspace tests pass or a narrower gate plus clear reason is documented.
- If changes land, one local commit is permitted; do not push.

## Constraints / Non-Goals

- Do not redesign the renderer.
- Do not introduce async or multi-queue scheduling.
- Do not touch material runtime or PSO cache.
- Do not claim editor-shell end-to-end behavior.

## Verification Gates

The Executor MUST run and document:

- `cargo +nightly fmt --check -p rge-gfx`
- `cargo test -p rge-gfx --all-targets --no-fail-fast`
- `cargo test --workspace --no-fail-fast`
- `cargo run -q -p rge-tool-architecture-lints -- all`
- `git diff --check`

## Halt Conditions

The Executor MUST halt with `HANDOFF_STATUS: BLOCKED` and `NEXT_ROLE: PLANNER_AI` if:

- No Job 4 closeout exists.
- Job 4 does not explicitly recommend proceeding.
- Implementation requires editing outside `crates/gfx/src/frame_graph/**`.
- Architecture-lint exemption count shifts.
- The work requires a new ADR or broad renderer redesign.

## Planner Notes

This job is intentionally conditional. Job 4 owns the decision whether this is real work or stale queue pressure.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-ORDERED-JOB-005-FRAMEGRAPH-FOLLOWUP
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
