# Execution Report

DISPATCH_ID: MAIN-RENDER-POSTDEPTH-GATEA-001
AUTHOR: Executor / Anthropic Claude
TIMESTAMP: 2026-05-14_02-30-00+0300
RELATED_FILES:
- ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_TASK_2026-05-13_18-12-00+0300.md — TASK consumed.
- ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_REVIEW_2026-05-13_18-12-01+0300.md — Reviewer1 (OpenAI Codex) initial APPROVE.
- ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_CORRECT_2026-05-13_18-23-00+0300.md — Planner correction: open `plans/IMPLEMENTATION.md` for status-wording only.
- ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_REVIEW_2026-05-13_18-23-01+0300.md — Reviewer1 post-correction APPROVE.
- ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_REVIEW_2026-05-14_02-05-00+0300.md — Reviewer2 (Anthropic Claude) APPROVE.
- crates/gfx/tests/gate_a_simple_scene_depth_60fps.rs — NEW (the harness, additive).
- plans/IMPLEMENTATION.md:468 — status-marker wording replaced (per CORRECTION).
- plans/BASELINE.md §6.3 — measurement-gap paragraph replaced with closed-measurement paragraph.
- Status.md — new snapshot prepended.
- HANDOFF.md — new snapshot prepended.
- change.md — new entry appended.
- ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_EXEC_2026-05-14_02-30-00+0300.md — this packet.
STATUS: AWAITING_REVIEW

## Task Packet Reference

`ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_TASK_2026-05-13_18-12-00+0300.md` plus the CORRECTION packet at `ai_handoffs/MAIN-RENDER-POSTDEPTH-GATEA-001_CORRECT_2026-05-13_18-23-00+0300.md`. Both reviews (Reviewer1 + Reviewer2) APPROVED.

## What I Changed

### Source

- None. Zero edits to any `crates/gfx/src/**` file or to any other production source file.

### Tests

- `crates/gfx/tests/gate_a_simple_scene_depth_60fps.rs` — **NEW**, ~270 LoC, `#[ignore]`, release-only, GPU-dependent integration test. Mirrors `gate_a_simple_scene_60fps.rs` byte-for-byte in methodology constants (1000 cubes / 10×10×10 / static camera Z=-40 / 1280×720 / 60 warmup + 600 sample / 3 runs / P95 ≤ 16.67 ms / variance ≤ 30%) but:
  - Constructs the pipeline via `LitMeshPipeline::new_with_depth(.., Some(DepthStateKey::new(wgpu::TextureFormat::Depth24Plus, false, wgpu::CompareFunction::LessEqual)))` (sub-α API; matches editor-shell production sub-β EXACTLY).
  - Allocates a per-frame `Depth24Plus` depth texture once via `wgpu::TextureDescriptor` + `wgpu::TextureUsages::RENDER_ATTACHMENT` and reuses the same view across all 1980 frames (60 warmup + 600 sample × 3 runs).
  - Passes `Some(&scene.depth_view)` as the final arg to `record_lit_mesh_pass(...)`.
  - `record_lit_mesh_pass` already handles depth correctly: `LoadOp::Clear(1.0)`, `StoreOp::Store`, no stencil. Zero non-test `crates/gfx/src/` edits required.

### Docs

- `plans/IMPLEMENTATION.md:468` — replaced the "POST-DEPTH PRODUCTION-PATH MEASUREMENT GAP — sub-γ docs-only 2026-05-12" wording with a "POST-DEPTH GATE A — CLOSED 2026-05-14 on recorder host AND post-depth gfx path only" entry recording the real measured numbers + scope language. The pre-existing pre-depth `[CLOSED 2026-05-11 ... 0.112 ms]` entry is untouched.
- `plans/BASELINE.md` §6.3 — replaced the "Post-sub-β measurement gap" paragraph with a new "Post-depth Gate A — CLOSED 2026-05-14" paragraph (recorder-host run-by-run numbers + scope preservation + future-pressure carry-forward).
- `Status.md` — prepended new snapshot describing the Gate A post-depth closure.
- `HANDOFF.md` — prepended new snapshot (same content shape as Status.md).
- `change.md` — appended dated entry documenting the full dispatch lifecycle (TASK + CORRECTION + Reviewer1 + Reviewer2 + Executor) and the substantive results.

## Per-File Summary

Total commit will include 6 files: 1 new test source + 5 modified docs. Zero modifications to `crates/gfx/src/**`, `crates/editor-shell/**`, any other production crate, Cargo manifests, PLAN, lints, ADRs, the protocol, or templates.

## Verification Results

All gates ran on the local recorder host (Windows / NVIDIA RTX 4060 Ti):

- `cargo +nightly fmt --check -p rge-gfx` → **exit 0**.
- `cargo test -p rge-gfx --release --test gate_a_simple_scene_depth_60fps -- --ignored --nocapture` → **exit 0**, 1 passed / 0 failed / 0 ignored in 1.33s. Real GPU measurement captured:
  ```
  Gate A (post-depth) adapter: name="NVIDIA GeForce RTX 4060 Ti" backend=Vulkan device_type=DiscreteGpu driver="NVIDIA"
  Gate A (post-depth) run 0: P50=0.084 ms, P95=0.125 ms, max=0.866 ms
  Gate A (post-depth) run 1: P50=0.085 ms, P95=0.122 ms, max=1.996 ms
  Gate A (post-depth) run 2: P50=0.089 ms, P95=0.122 ms, max=1.294 ms
  Gate A (post-depth, simple-scene 60fps): median P50 = 0.085 ms, min P95 = 0.122 ms,
    median P95 = 0.122 ms, max P95 = 0.125 ms, worst frame = 1.996 ms,
    variance across runs = 2.6%
  ```
  Both gates (P95 ≤ 16.67 ms and variance ≤ 30%) cleared by >100× margin.
- `cargo test -p rge-gfx --test lit_mesh_depth_overlay_smoke` → **exit 0**, 1 passed / 0 failed (the existing pixel-readback regression for the same depth-state configuration still passes).
- `cargo test -p rge-gfx --test frame_graph_umbrella_smoke` → **exit 0**, 1 passed / 0 failed.
- `cargo test --workspace --no-fail-fast` → **exit 0**, **2549 passed / 0 failed / 20 ignored** (was 2549/0/19 at sub-ε hardening `889da8a`; +1 ignored = exactly the new harness, no other test count drift).
- `cargo run -q -p rge-tool-architecture-lints -- all` → **exit 0**, 9 enforcement + 1 supplementary PASS (byte-identical architectural-exemption count).
- `git diff --check` → **exit 0**, no whitespace errors.

## Result Headline

**MIN-OF-3 P95 = 0.122 ms** on recorder host. **VARIANCE = 2.6%**. Both gates cleared. **About 9% slower than pre-depth (0.112 ms)** — the measured cost of the depth attachment. Still ~137× under the 16.67 ms threshold.

## Deviations from Task Packet

None. Execution stayed strictly within the TASK + CORRECTION combined scope:
- One new test file under `crates/gfx/tests/` matching the required name.
- Five doc files edited within the `MAY edit` envelope (`plans/BASELINE.md`, `plans/IMPLEMENTATION.md` [per CORRECTION], `Status.md`, `HANDOFF.md`, `change.md`).
- Zero `crates/gfx/src/**` edits.
- Zero edits to any other production crate, Cargo manifest, PLAN, ADR, lint, protocol, or template.
- No architecture-lint exemption count shift.
- No new ADR / lint / doctrine added.
- No `unsafe` introduced.

## Commit Recommendation

The TASK packet allows committing if gates pass: "If gates pass and the user has not forbidden commits, commit the dispatch as one commit. Do not push." All gates passed; the user has not forbidden commits.

**Recommended single commit** (Executor will create it after this packet):

```
git add crates/gfx/tests/gate_a_simple_scene_depth_60fps.rs \
        plans/IMPLEMENTATION.md plans/BASELINE.md \
        Status.md HANDOFF.md change.md
git commit -m "test(gfx): post-depth Gate A harness — recorder-host min-of-3 P95 = 0.122 ms"
```

The handoff packets in `ai_handoffs/` stay untracked per the existing pattern.

## Open Questions for Reviewer

- The depth attachment cost (+9% vs pre-depth) is a real measurement on a single GPU. It is NOT universal vendor parity; recording the +9% as the recorder-host cost is honest, but the phrasing in `BASELINE.md` should not be over-extrapolated to other hardware. Reviewer should confirm the doc wording is honestly scoped.
- The `_depth_texture: Arc<wgpu::Texture>` field in the new harness's `SimpleSceneWithDepth` struct is named with a leading underscore (prefixed-unused) because the field exists for lifetime management only — the `depth_view` borrows from it transitively via `create_view`. This matches the precedent in `lit_mesh_depth_overlay_smoke.rs:303`. Reviewer should confirm the lifetime guard is acceptable.
- The next-queued dispatch `MAIN-RENDER-VISUAL-FIDELITY-002` may now proceed (its dependency on this dispatch's closure is satisfied once this EXEC + the corresponding REVIEW + CLOSEOUT land). Reviewer / Planner direction welcome on whether `MAIN-RENDER-VISUAL-FIDELITY-002`'s scope needs to consider the +9% depth-attachment cost as a new constraint.

## Worktree State

- Tracked files (modifications staged for the recommended commit):
  - `M HANDOFF.md`
  - `M Status.md`
  - `M change.md`
  - `M plans/BASELINE.md`
  - `M plans/IMPLEMENTATION.md`
  - (NEW, will be added) `crates/gfx/tests/gate_a_simple_scene_depth_60fps.rs`
- Untracked items: 54 entries — handoff packets in `ai_handoffs/` (including this EXEC + the FICTIONAL-CHAIN-001 EXEC + Reviewer2 REVIEWs authored in the same turn), precedent root-level handoff MDs, pre-existing out-of-scope items (`Project_Imports/`, `RGE_Semantic_Runtime_Strategy.zip`, `JobsDone_*.md`, `.claude/`, `Install-Windows11-D-Standalone.ps1`). None of these enter the commit.
- Branch: `main`.
- Last commit before this dispatch: `197dd42 docs: add deterministic completion footer to AI handoff protocol`.
- Local HEAD aligned with `origin/main`.

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: MAIN-RENDER-POSTDEPTH-GATEA-001
AUTHOR: Executor / Anthropic Claude
NEXT_ROLE: REVIEWER_AI
EXIT_CODE: 0

---
