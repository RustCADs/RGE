# Task Packet

DISPATCH_ID: MAIN-RENDER-VISUAL-FIDELITY-002
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-13_18-12-02+0300
RELATED_FILES:
- crates/gfx/tests/lit_mesh_depth_overlay_smoke.rs
- crates/gfx/tests/render_mesh_smoke.rs
- crates/gfx/src/lit_mesh_pipeline.rs
- plans/BASELINE.md
- Status.md
- HANDOFF.md
- change.md
STATUS: OPEN

## Goal

Broaden the existing Phase 6 gfx-level Z-fight visual harness from one overlay sample, one cuboid-only sample, and one background sample into a slightly stronger pixel-regression boundary. This is Turn 2 after `MAIN-RENDER-POSTDEPTH-GATEA-001`; execute it only after Turn 1 is closed or the human explicitly says to run it in parallel.

## Scope

### MAY edit
- `crates/gfx/tests/lit_mesh_depth_overlay_smoke.rs`
- `Status.md`
- `HANDOFF.md`
- `change.md`

### MUST NOT edit
- `crates/gfx/src/**`
- `crates/gfx/tests/gate_a_simple_scene_depth_60fps.rs`
- `crates/gfx/tests/gate_a_simple_scene_60fps.rs`
- `crates/editor-shell/**`
- `crates/cad-core/**`
- `crates/cad-projection/**`
- `kernel/**`
- `Cargo.toml`
- `Cargo.lock`
- `plans/PLAN.md`
- `plans/IMPLEMENTATION.md`
- `plans/BASELINE.md`, unless Turn 1's closeout explicitly asks this task to append a non-perf note
- `tools/architecture-lints/**`
- `ai_handoffs/templates/**`
- `ai_handoffs/AI_HANDOFF_PROTOCOL.md`

### MAY add new files
- This dispatch's `EXEC`, `REVIEW`, and `CLOSEOUT` packets under `ai_handoffs/`

### MUST NOT add new files
- New production files
- New test files
- New ADRs
- New architecture lints
- New section-18 companion docs

## Deliverables

- Strengthen `lit_mesh_depth_overlay_smoke.rs` with multiple sample points for each existing category:
  - overlay region: at least three pixels that must be orange-dominated
  - cuboid-only region: at least three pixels that must remain white-ish
  - background region: at least three pixels that must remain near-black
- Keep the same synthetic cuboid plus one-triangle overlay geometry.
- Keep the same depth state: `Depth24Plus`, `depth_write_enabled = false`, `LessEqual`.
- Prefer small helper functions in the test file if they reduce repetition, but do not introduce a broad shared test utility module.
- Update `Status.md`, `HANDOFF.md`, and `change.md` with a concise Turn 2 result if the dispatch lands.
- If gates pass and the user has not forbidden commits, commit the dispatch as one commit. Do not push.

## Acceptance Criteria

- The existing `lit_mesh_depth_overlay_pixel_readback` test still proves the same core behavior.
- The broadened assertions catch region leakage better than the current single-pixel checks.
- No production code changes.
- No performance claims added.
- No editor-shell end-to-end claim added.
- The test remains GPU-gated with skip behavior on machines without a headless adapter.
- Architecture lints remain unchanged.

## Constraints / Non-Goals

- Do not change the renderer, pipeline, material, camera, light, or readback production APIs.
- Do not add a second camera-pose harness unless it is clearly smaller than the multi-sample strengthening; the main target is stronger sampling, not a broad visual suite.
- Do not add image snapshots or binary artifacts.
- Do not update `plans/BASELINE.md` with performance numbers; this dispatch is visual-regression coverage, not a perf measurement.
- Do not build editor-shell or winit-bypass infrastructure.

## Verification Gates

The Executor MUST run and document the result of each feasible gate in their `EXECUTION_REPORT`:

- Confirm `MAIN-RENDER-POSTDEPTH-GATEA-001` is closed, or record explicit human approval to run Turn 2 before Turn 1 closes.
- `cargo +nightly fmt --check -p rge-gfx` -> expected exit 0
- `cargo test -p rge-gfx --test lit_mesh_depth_overlay_smoke -- --nocapture` -> expected exit 0 or GPU-skip success
- `cargo test -p rge-gfx --test render_mesh_smoke -- --nocapture` -> expected exit 0 or GPU-skip success
- `cargo test --workspace --no-fail-fast` -> expected exit 0
- `cargo run -q -p rge-tool-architecture-lints -- all` -> expected exit 0 (9 enforcement + 1 supplementary PASS)
- `git diff --check` -> expected exit 0

## Halt Conditions

The Executor MUST halt without committing and request guidance if:

- Turn 1 is not closed and the human has not authorized parallel execution.
- Strengthening the test requires production code edits.
- The existing visual test becomes flaky or requires loosening thresholds.
- The sample-point expectations are not stable under the current headless target and camera.
- Any architecture lint exemption count changes.
- Any non-gfx source crate must be edited.

## Planner Notes

This is deliberately smaller than a full editor-shell visual harness. It adds confidence exactly where the current harness is weakest: single-point sampling can miss region leakage or diagonal-boundary mistakes.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-RENDER-VISUAL-FIDELITY-002
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: REVIEWER_AI
EXIT_CODE: 0

---
