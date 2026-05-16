# Review Report

DISPATCH_ID: MAIN-RENDER-POSTDEPTH-GATEA-001
AUTHOR: Reviewer / Anthropic Claude
TIMESTAMP: 2026-05-14_02-05-00+0300
RELATED_FILES:
- ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_TASK_2026-05-13_18-12-00+0300.md
- ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_REVIEW_2026-05-13_18-12-01+0300.md (Reviewer1 / Codex)
- ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_CORRECT_2026-05-13_18-23-00+0300.md (Planner correction)
- ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_REVIEW_2026-05-13_18-23-01+0300.md (Reviewer1 re-review of correction)
- crates/gfx/tests/gate_a_simple_scene_60fps.rs (existing harness, to mirror)
- crates/gfx/tests/lit_mesh_depth_overlay_smoke.rs (existing depth-state reference)
- crates/gfx/src/lit_mesh_pipeline.rs (`new_with_depth` + `record_lit_mesh_pass` API)
- plans/IMPLEMENTATION.md:468 (post-depth measurement gap text)
- plans/BASELINE.md:248 (post-depth measurement gap text)
STATUS: APPROVED

## References

- Task Packet: `ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_TASK_2026-05-13_18-12-00+0300.md`
- Reviewer1 Review #1: `ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_REVIEW_2026-05-13_18-12-01+0300.md` (APPROVED)
- Correction Packet: `ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_CORRECT_2026-05-13_18-23-00+0300.md` (CORRECTION_OPEN; allows `plans/IMPLEMENTATION.md` for status-wording only)
- Reviewer1 Review #2: `ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_REVIEW_2026-05-13_18-23-01+0300.md` (APPROVED after correction)

## Independently Re-Run Gates

- Read TASK + both Reviewer1 reviews + CORRECTION packet → success.
- Read `crates/gfx/tests/gate_a_simple_scene_60fps.rs` → 268-line existing harness; 1000 cubes / 10×10×10 / static camera / 1280×720 / 60 warmup / 600 sample / 3 runs / P95 ≤ 16.67 ms / variance ≤ 30%; calls `record_lit_mesh_pass(.., None)` (no depth).
- Read `crates/gfx/tests/lit_mesh_depth_overlay_smoke.rs` → 375-line existing depth reference; uses `DepthStateKey::new(Depth24Plus, false, LessEqual)` + `LitMeshPipeline::new_with_depth(.., Some(depth_state))` + manual `wgpu::Texture` for depth + inline render-pass (not `record_lit_mesh_pass`).
- Read `crates/gfx/src/lit_mesh_pipeline.rs::record_lit_mesh_pass` (lines 664-712) → already handles `depth_view: Option<&wgpu::TextureView>` correctly; `Some(view)` triggers `depth_stencil_attachment = Some(...)` with `LoadOp::Clear(1.0)` + `StoreOp::Store`. So the new harness can use the high-level recording function unchanged — zero production-code edits required.
- Confirmed `plans/IMPLEMENTATION.md:468` and `plans/BASELINE.md:248` both carry the "POST-DEPTH PRODUCTION-PATH MEASUREMENT GAP" wording that this dispatch is targeted to close (per the correction packet).
- Cargo / build / test gates → NOT RUN at review time. Executor will run them.

## Findings

### Correct

- The TASK + CORRECTION combination is well-shaped:
  - Bounded to ONE new ignored release test file under `crates/gfx/tests/`.
  - MAY edit envelope (`tests/gate_a_simple_scene_depth_60fps.rs`, `plans/BASELINE.md`, `Status.md`, `HANDOFF.md`, `change.md`, plus `plans/IMPLEMENTATION.md` per correction) is the minimum surface required.
  - MUST NOT edit envelope correctly protects `crates/gfx/src/**`, `crates/editor-shell/**`, all other crates, Cargo, PLAN, lints, and the protocol/templates.
  - Acceptance criteria pin the methodology (60 warmup / 600 sample / 3 runs / P95 ≤ 16.67 ms / variance ≤ 30%) and the API surface (`new_with_depth` + `DepthStateKey` + `record_lit_mesh_pass(.., Some(&depth_view))`).
  - The honest-failure path is documented: NO GPU → `HANDOFF_STATUS: BLOCKED` + no fake numbers; failed gates → record + escalate, do not tune.
- Reviewer1's two passes are sound. The correction is a real fix (the original TASK forbade editing `plans/IMPLEMENTATION.md` even though that's where line 468 lives).
- The depth API surface is already production-ready (sub-α + sub-β both landed before this dispatch); this Turn 1 only adds a measurement harness over the existing API.

### Needs Correction

- None after the existing correction packet. The Planner / Reviewer1 chain has already addressed the one scope contradiction.

### Latent Risks (Not Blocking)

- **Build-environment risk on Claude's local machine**: earlier in today's session (during the sub-ε review of `889da8a`), the workspace test gate did NOT complete on Claude's machine due to transient rlib-format cache errors (`wgpu_hal`, then `parry3d`) plus `STATUS_STACK_BUFFER_OVERRUN` in `rge-editor-shell` / `rge-cad-projection` test compilation. If those rlib-cache issues persist, `cargo test --workspace --no-fail-fast` may not complete cleanly even though the new gfx test itself compiles fine. **Pre-warning**: if the workspace gate fails with the same rlib-format pattern (NOT a test failure, but a build environment failure unrelated to this dispatch's source), the Executor should document it explicitly in the EXEC packet and reach the HALT condition for tests outside the new harness only if the failure cannot be explained as an environment skip.
- **GPU adapter availability** (already flagged by Reviewer1): if no GPU adapter is reachable from the test process, the harness's `ctx_or_skip` will short-circuit and the `#[ignore]` body returns early. In that case, the Executor must produce `HANDOFF_STATUS: BLOCKED` and not touch result docs — per TASK acceptance criteria.
- **Code duplication acceptable** (already flagged): the new harness will duplicate `push_cube` + `ctx_or_skip` + the build-scene helper from the existing Gate A test. Inside integration tests, this is the correct call — production abstraction is explicitly forbidden by the TASK.
- **Variance > 30% on a noisy host** is plausible. The TASK halt condition correctly says "record then escalate" rather than tune. If variance trips, record the run-by-run numbers and the failure; do not commit a passing-but-fake result.

## Test Coverage Assessment

- **Strong** for the dispatched invariant: depth-attached `record_lit_mesh_pass` exercised at Gate A's 1000-cube workload across 3 runs.
- **Acceptable gaps** per TASK design:
  - Does NOT certify editor-shell production end-to-end (`editor-shell::render_frame` is structurally tested by frame-graph smoke + the depth-overlay pixel-readback; perf via this dispatch is gfx-level only).
  - Does NOT certify vendor parity, cold-start, sustained thermal, or universal 60fps.
  - Does NOT certify CI regression coverage (the test is `#[ignore]` by design).

These exclusions are correct for Turn 1 and called out in the TASK's `Acceptance Criteria` doc-wording bullets.

## Doc Accuracy Check

- The current docs (BASELINE.md:248, IMPLEMENTATION.md:468) honestly describe the post-depth measurement as DEFERRED. If this dispatch's measurement lands, the doc updates must:
  - Replace or qualify the DEFERRED wording with the real result, keeping the recorder-host-only scope language.
  - Preserve the existing pre-depth Gate A result for `gate_a_simple_scene_60fps.rs` (0.112 ms min-P95) — that result remains valid and should not be overwritten.
  - Add a new BASELINE.md section or extend §6.3 with the post-depth run-by-run numbers + variance percentage + adapter metadata.
- If BLOCKED, the doc deferral wording stays exactly as is; the EXEC packet records the BLOCKED reason without touching docs.

## Recommended Action

**APPROVE for Executor handoff** — TASK + correction together form a clean, bounded dispatch. The Executor (Claude) may now:
1. Author `crates/gfx/tests/gate_a_simple_scene_depth_60fps.rs` mirroring the existing Gate A harness with depth attachment.
2. Run the verification gates documented in the TASK.
3. Either update result docs (if measurement succeeds) or record `HANDOFF_STATUS: BLOCKED` (if GPU adapter unavailable OR if the workspace-test gate fails for the same rlib-format environment reason flagged in the latent-risks section above).
4. Commit per the TASK ("one commit; do not push") only if all gates pass with a real measurement.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-RENDER-POSTDEPTH-GATEA-001
AUTHOR: Reviewer / Anthropic Claude
NEXT_ROLE: EXECUTOR_AI
EXIT_CODE: 0

---
