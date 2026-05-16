# Task Packet

DISPATCH_ID: MAIN-RENDER-POSTDEPTH-GATEA-001
AUTHOR: Planner / OpenAI Codex
TIMESTAMP: 2026-05-13_18-12-00+0300
RELATED_FILES:
- plans/BASELINE.md
- plans/IMPLEMENTATION.md
- crates/gfx/tests/gate_a_simple_scene_60fps.rs
- crates/gfx/tests/lit_mesh_depth_overlay_smoke.rs
- crates/gfx/src/lit_mesh_pipeline.rs
- crates/gfx/src/frame_graph/resource_map.rs
- crates/gfx/src/frame_graph/texture_pool.rs
- crates/gfx/src/frame.rs
- Status.md
- HANDOFF.md
- change.md
STATUS: OPEN

## Goal

Close the currently documented Phase 6 post-depth Gate A measurement gap with the smallest honest renderer-side dispatch: add a synthetic, depth-attached gfx-level 1000-cube performance harness that mirrors the existing Gate A methodology but uses `LitMeshPipeline::new_with_depth(.., Some(DepthStateKey { Depth24Plus, false, LessEqual }))` and `record_lit_mesh_pass(.., Some(&depth_view))`. This is not editor-shell end-to-end certification; it is the bounded gfx-level re-measurement path explicitly listed in `plans/BASELINE.md` and `plans/IMPLEMENTATION.md`.

## Scope

### MAY edit
- `crates/gfx/tests/gate_a_simple_scene_depth_60fps.rs`
- `plans/BASELINE.md`
- `Status.md`
- `HANDOFF.md`
- `change.md`

### MUST NOT edit
- `crates/gfx/src/**`
- `crates/editor-shell/**`
- `crates/cad-core/**`
- `crates/cad-projection/**`
- `kernel/**`
- `Cargo.toml`
- `Cargo.lock`
- `plans/PLAN.md`
- `plans/IMPLEMENTATION.md`
- `tools/architecture-lints/**`
- `ai_handoffs/templates/**`
- `ai_handoffs/AI_HANDOFF_PROTOCOL.md`

### MAY add new files
- `crates/gfx/tests/gate_a_simple_scene_depth_60fps.rs`
- This dispatch's `EXEC`, `REVIEW`, and `CLOSEOUT` packets under `ai_handoffs/`

### MUST NOT add new files
- New crates
- New ADRs
- New architecture lints
- New doctrine or section-18 companion docs
- New production modules

## Deliverables

- One new ignored, release-mode, GPU-dependent gfx integration test named clearly around post-depth Gate A, e.g. `gate_a_simple_scene_depth_60fps`.
- The test should mirror the existing Gate A scene shape:
  - 1000 cubes, arranged 10 x 10 x 10
  - 1 directional light
  - static camera
  - 1280 x 720 headless target
  - 60 warmup frames
  - 600 measured frames
  - 3 runs
  - P95 <= 16.67 ms
  - variance across runs <= 30%
- The new harness must use:
  - `LitMeshPipeline::new_with_depth`
  - `DepthStateKey::new(wgpu::TextureFormat::Depth24Plus, false, wgpu::CompareFunction::LessEqual)`
  - a depth texture view passed to `record_lit_mesh_pass(..., Some(&depth_view))`
- Update `plans/BASELINE.md`, `Status.md`, `HANDOFF.md`, and `change.md` with the measured result if the GPU run succeeds.
- If no GPU adapter is available, write an EXEC packet with `HANDOFF_STATUS: BLOCKED` and do not update result docs with fake numbers.
- If gates pass and the user has not forbidden commits, commit the dispatch as one commit. Do not push.

## Acceptance Criteria

- Existing pre-depth Gate A test remains intact.
- New post-depth harness is additive and isolated in its own integration-test file.
- The new harness is `#[ignore]` and release-mode documented, matching the existing Gate A style.
- The test prints adapter metadata and per-run P50/P95/max plus final min/median/max P95.
- On a GPU-capable recorder host, the post-depth harness exits 0 and records a real measured result in the docs.
- Docs state the scope precisely:
  - certifies the synthetic depth-attached gfx path on the recorder host
  - does not certify editor-shell production end-to-end
  - does not certify vendor parity, cold-start, thermal behavior, CI regression coverage, or universal 60fps
- No production source files are modified.
- No architecture lint exemption count shifts.

## Constraints / Non-Goals

- Do not build a non-winit editor-shell harness.
- Do not edit `editor-shell::render_frame`.
- Do not tune the renderer to chase the number; record and escalate if the number fails.
- Do not broaden into visual assertions; Turn 2 handles visual harness broadening separately.
- Do not change `LitMeshPipeline`, `record_lit_mesh_pass`, `TexturePool`, `FrameGraph`, `ResourceMap`, or PSO cache behavior.
- Do not update PLAN targets or redefine Gate A.

## Verification Gates

The Executor MUST run and document the result of each feasible gate in their `EXECUTION_REPORT`:

- `cargo +nightly fmt --check -p rge-gfx` -> expected exit 0
- `cargo test -p rge-gfx --release --test gate_a_simple_scene_depth_60fps -- --ignored --nocapture` -> expected exit 0 on GPU host, or documented BLOCKED if no GPU adapter
- `cargo test -p rge-gfx --test lit_mesh_depth_overlay_smoke` -> expected exit 0 or GPU-skip success
- `cargo test -p rge-gfx --test frame_graph_umbrella_smoke` -> expected exit 0
- `cargo test --workspace --no-fail-fast` -> expected exit 0; test count may gain one ignored test
- `cargo run -q -p rge-tool-architecture-lints -- all` -> expected exit 0 (9 enforcement + 1 supplementary PASS)
- `git diff --check` -> expected exit 0

## Halt Conditions

The Executor MUST halt without committing and request guidance if:

- Implementing the harness requires production `crates/gfx/src/**` changes.
- Implementing the harness requires editor-shell changes.
- The post-depth harness fails the P95 or variance gates on a GPU-capable host.
- The harness cannot keep the existing Gate A methodology recognizable.
- Any architecture lint exemption count changes.
- Any non-gfx crate must be edited.
- Any test failure appears outside the new harness and cannot be explained as an environment skip.

## Planner Notes

This is the first return-to-main-work turn after D-Fillet closure and the AI-handoff dry runs. It deliberately chooses a bounded renderer measurement gap that is explicitly documented as deferred in `plans/BASELINE.md` and `plans/IMPLEMENTATION.md`. Turn 2 should wait until this dispatch is closed, because both turns update shared status docs.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-RENDER-POSTDEPTH-GATEA-001
AUTHOR: Planner / OpenAI Codex
NEXT_ROLE: REVIEWER_AI
EXIT_CODE: 0

---
