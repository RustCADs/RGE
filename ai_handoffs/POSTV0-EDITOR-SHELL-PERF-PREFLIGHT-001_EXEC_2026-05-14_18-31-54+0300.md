# Execution Report

DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001
AUTHOR: Executor / Anthropic Claude
TIMESTAMP: 2026-05-14_18-31-54+0300
RELATED_FILES:
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001_TASK_2026-05-14_18-26-15+0300.md — TASK consumed.
- V0_RELEASE_CERTIFICATION.md — inspected (read-only) for v0 cert state evidence.
- Status.md — inspected (read-only) for current snapshot.
- HANDOFF.md — inspected (read-only).
- plans/BASELINE.md — inspected (read-only) for deferral evidence at §6.3 post-depth Gate A note.
- plans/IMPLEMENTATION.md — inspected (read-only) for Phase 6 exit criteria + frame-graph umbrella close-out.
- crates/editor-shell/src/render_path.rs — inspected (read-only) for `EditorShell::render_frame` body + `init_render_state` shape.
- crates/editor-shell/src/lifecycle.rs — inspected (read-only) for `ApplicationHandler<()>` impl + `resumed` call into `init_render_state`.
- crates/editor-shell/tests/editor_frame_idle.rs — inspected (read-only) for the existing empty-shell Gate B harness idiom.
- crates/editor-shell/tests/render_input_boundary.rs — inspected (read-only) for the existing non-GPU boundary-test idiom.
- ai_handoffs/POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001_EXEC_2026-05-14_18-31-54+0300.md — this packet (the only new file).
STATUS: AWAITING_REVIEW

## Task Packet Reference

`ai_handoffs/POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001_TASK_2026-05-14_18-26-15+0300.md`

## Pre-Execution Review Consumed

No pre-execution review issued for this dispatch. The Planner's TASK packet footer routes `NEXT_ROLE: EXECUTOR_AI` directly under v2 Rule 7 (single pre-execution reviewer; no duplicate Reviewer2 rubber-stamp for read-only preflight work). The OPENAItoCLAUDE root note explicitly states "Use protocol v2 Rule 7: no duplicate Reviewer2 approval packet is needed." Executor proceeded to execution.

## What I Changed

### Source
- None.

### Tests
- None.

### Docs
- `ai_handoffs/POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001_EXEC_2026-05-14_18-31-54+0300.md`: created this preflight execution packet (the only filesystem change).

## Per-File Summary

Read-only preflight. Zero tracked-file edits. Zero non-handoff untracked edits. Single filesystem effect: this new untracked EXEC packet under `ai_handoffs/`.

## Verification Gates

Per the TASK's `Verification Gates` section, the four required gates all PASS:

| Gate | Command | Result |
| --- | --- | --- |
| Tracked-tree clean | `git status --short --untracked-files=no` | empty output (clean) ✓ |
| Origin sync | `git rev-list --left-right --count origin/main...HEAD` | `0 0` ✓ |
| HEAD anchored | `git log --oneline --decorate -3` | `b13c176 (HEAD -> main, origin/main) docs(cert): v0 release certification at 6aaf7f1` ✓ |
| Evidence-gathering ripgrep | (multi-pattern grep over `crates/editor-shell` + `plans/` + cert/status/handoff docs) | hits used as concrete file:line citations below ✓ |

No cargo build or test was run (TASK explicitly does not require it for this read-only preflight).

## Halt-Condition Checks

All six TASK halt conditions evaluated; none triggered:

| Halt condition | Status |
| --- | --- |
| Any tracked file modified before or during dispatch | NOT TRIPPED — tracked tree clean before and after this packet write |
| `origin/main...HEAD` not `0 0` and reason not understood | NOT TRIPPED — `0 0` |
| V0 cert record missing or not showing `CERTIFIED v0` | NOT TRIPPED — `V0_RELEASE_CERTIFICATION.md` line 7 reads `**Decision** \| **CERTIFIED v0**` |
| Current render path cannot be inspected locally | NOT TRIPPED — `crates/editor-shell/src/render_path.rs:289` `pub(crate) fn render_frame(&mut self) -> bool` read successfully |
| Feasibility analysis would itself require source edits | NOT TRIPPED — analysis below is grounded in read-only inspection |
| Smallest honest harness needs broad architecture / real winit bypass / public API redesign / new dependency | NOT TRIPPED — design map below identifies a small, bounded source edit that does NOT add a dependency, redesign public API, or perform a real winit bypass |

## Concrete File/Line Evidence

### Current `EditorShell::render_frame` implementation

- `crates/editor-shell/src/render_path.rs:289` — `pub(crate) fn render_frame(&mut self) -> bool` — production signature.
- `crates/editor-shell/src/render_path.rs:297–331` — frame-graph prologue: `tex_pool.begin_frame()` + `buf_pool.begin_frame()` + `build_resource_map(...)` + extract `DEPTH_RESOURCE_ID` view (the existing transient-substrate dance per ADR-118).
- `crates/editor-shell/src/render_path.rs:333–356` — eight `Option::as_ref()` early-returns on `gfx_ctx` / `surface_ctx` / `pipeline` / `gfx_camera` / `light` / `material` / `cuboid_mesh` / `window`; `render_frame` returns `false` if any field is `None`. **This is why the existing empty-shell Gate B harness measures ~100 ns/call**: `EditorShell::new()` leaves all eight as `None`, and the first early-return fires immediately.
- `crates/editor-shell/src/render_path.rs:364–375` — surface acquire: `surface_ctx.surface().get_current_texture()` (wgpu 29 returns `CurrentSurfaceTexture`, not `Result`); skip-on-`Timeout/Occluded/Outdated/Lost/Validation` requests another redraw via `window.request_redraw()`. **This is the winit-dependent portion of the body.**
- `crates/editor-shell/src/render_path.rs:380–429+` — winit-INDEPENDENT body: command encoder, render pass with depth attachment (`build_resource_map`'s output), pipeline bind, three bind groups (camera/light/material), vertex/index buffers, `draw_indexed`.

### Pre-`render_frame` initialisation surface

- `crates/editor-shell/src/render_path.rs:172` — `pub(crate) fn init_render_state(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String>` — the populator for the eight `Option<_>` fields above.
- `crates/editor-shell/src/render_path.rs:184–187` — Step 1 (winit-bound): `event_loop.create_window(attrs)` + `Arc::new(window)`.
- `crates/editor-shell/src/render_path.rs:190` — Step 2 (winit-INDEPENDENT): `GfxContext::new_headless()` — gfx context is **already headless-capable**.
- `crates/editor-shell/src/render_path.rs:193–194` — Step 3 (winit-bound): `SurfaceContext::new(&gfx_ctx, Arc::clone(&window))`.
- Steps 4–7 (per the surrounding doc-comment at lines 161–166) — Material / DirectionalLight / GfxCamera / LitMeshPipeline / RenderMesh / UBO update — all winit-INDEPENDENT.
- `crates/editor-shell/src/lifecycle.rs:660–661` — `impl ApplicationHandler<()> for EditorShell` / `fn resumed(&mut self, event_loop: &ActiveEventLoop)` — the unique production caller of `init_render_state`.

### Documented `EditorShell::render_frame` end-to-end measurement deferral

- `plans/BASELINE.md:248` (Post-depth Gate A CLOSED note, MAIN-RENDER-POSTDEPTH-GATEA-001 closeout, 2026-05-14):

  > "**Scope (recorder-host-only)**: NOT universal, NOT vendor parity, NOT cold-start, NOT sustained thermal, NOT realistic geometry complexity, NOT CI regression coverage, **NOT editor-shell `render_frame` end-to-end** (the harness exercises the gfx-level primitives that editor-shell production consumes post-sub-β; it does not exercise editor-shell's winit + `SurfaceContext` + `FrameGraph` + `build_resource_map` substrate ceremony — that remains a separate non-winit-perf-harness scope, **blocked on `EditorShell::render_frame` accepting a mock event loop**, not pursued by this dispatch)."

- `plans/IMPLEMENTATION.md:473` (Frame-graph chapter umbrella close-out 2026-05-12):

  > "Runtime-perf re-validation against Gate A's recorder-host CLOSED marker (line 468 above — pre-allocator triangle path certified on NVIDIA GeForce RTX 4060 Ti / Vulkan; commit `35e5078`) is deferred until real pass-record sites grow transient-resource consumers; the just-shipped substrate is NOT what Gate A certified, and re-measurement is appropriate when consumer pressure surfaces."

  (This deferral is about gfx-level re-measurement, distinct from the editor-shell-end-to-end deferral at `BASELINE.md:248`; both are listed in cert doc §4.)

### v0 certification state

- `V0_RELEASE_CERTIFICATION.md:7` — `**Decision** | **CERTIFIED v0**`.
- `V0_RELEASE_CERTIFICATION.md` §4 lists "**Editor-shell mock-event-loop perf harness**" as a known v0 deferral and explicitly says it is **NOT a v0 blocker**.
- `Status.md` most recent snapshot line (line 5 post-this-cert) opens with `2026-05-14 (v0 RELEASE CERTIFIED at commit \`6aaf7f1\`...)`.
- `HANDOFF.md` most recent snapshot line opens with `2026-05-14 (**v0 RELEASE CERTIFIED at commit \`6aaf7f1\`**...)`.

## Feasibility Verdict

**PROCEED — with bounded source edits in the next implementation dispatch.**

The "blocked on `EditorShell::render_frame` accepting a mock event loop" framing in `BASELINE.md:248` is somewhat pessimistic given the current substrate. Reading `init_render_state` + `render_frame` reveals that:

1. **`GfxContext::new_headless()` is already winit-independent** (`render_path.rs:190`).
2. **The winit-bound surface I/O is isolated to a small region of `render_frame`** — surface acquire (`render_path.rs:364–375`) and `window.request_redraw()` (line 372). The rest of the body (encoder creation, render pass record, pipeline bind, draw calls, command-buffer submission) operates on the winit-independent `GfxContext` + pipeline + bind groups.
3. **Steps 4–7 of `init_render_state`** (material / light / camera / pipeline / mesh / UBO update) are winit-independent.
4. **A small refactor that extracts the winit-independent encode/submit body of `render_frame` into a `pub(crate)` helper that takes a target view as a parameter** plus **a sibling headless init path mirroring `init_render_state` steps 2 and 4–7** would expose an "end-to-end except winit's surface dance" measurement surface without any public-API redesign, dependency addition, or architectural change.

This is NOT a "mock event loop" in the literal sense — it is a controlled per-frame driver that bypasses winit's surface acquire/present and measures the per-frame editor-shell substrate cost that production `render_frame` performs after surface acquire. That is the honest measurement gap the deferral describes; "mock event loop" was a useful summary phrase but conflates the architectural barrier with the implementation shape.

The smallest honest implementation dispatch is bounded enough to scope cleanly without broad editor-shell refactoring or new dependencies.

## Design Map (smallest honest harness)

### Substrate layout

A single new sibling file inside the existing crate, plus two small `pub(crate)` extractions inside `render_path.rs`:

1. **Extract `crates/editor-shell/src/render_path.rs:render_frame_to_target(target_view: &wgpu::TextureView) -> bool`** — the winit-independent body of `render_frame` (everything from line 380 through the `queue.submit` + `frame.present` site, with the present site moved up into the caller). The existing `render_frame` becomes a 3-line wrapper that acquires the surface texture, calls `render_frame_to_target(&view)`, and presents.

2. **Add `crates/editor-shell/src/render_path.rs:init_render_state_headless(&mut self, target_format: wgpu::TextureFormat) -> Result<(), String>`** — mirrors `init_render_state` steps 2 + 4–7 (Steps 1 + 3 — the winit window + winit-bound `SurfaceContext` — are skipped; the harness owns the target texture and passes the format).

3. **Add `crates/editor-shell/tests/render_frame_e2e_perf.rs`** — `#[ignore]`-gated release-only timing harness following the existing `editor_frame_idle.rs` idiom (batch K of N frames; P50/P95; variance < 30%) but driving `init_render_state_headless` + `render_frame_to_target` against a `wgpu::TextureFormat::Bgra8UnormSrgb` color target allocated once.

Note: integration tests under `crates/editor-shell/tests/` can only call `pub` items. Two viable shapes:
   - **(a)** Add `pub fn render_frame_to_target_for_perf_only(...)` + `pub fn init_render_state_headless_for_perf_only(...)` with names that signal harness-only intent (matching the existing `tick_redraw` public-but-thin idiom).
   - **(b)** Keep both `pub(crate)` and put the harness inside `src/` behind `#[cfg(test)] mod render_frame_e2e_perf;` (Rust unit-test idiom).
   - **Recommendation: (b)** — keeps the public surface unchanged (no API redesign per TASK halt condition) and avoids polluting downstream crate consumers with perf-only entry points.

### Measurement gate

- **Format**: matches Gate A's recorder-host close-out (`plans/BASELINE.md:240–248`) and Gate B's batch idiom (`crates/editor-shell/tests/editor_frame_idle.rs:17–56`):
  - 60 warmup + 600 sample frames per run × 3 runs (Gate A's scheme), OR batch K=10 × N=1000 (Gate B's scheme). The honest call: match Gate A since this harness is per-frame-perf-focused.
  - Compute P50, P95, min, max, worst-frame across the sample window.
  - Variance gate: `(max P95 − min P95) / median P95 < 30%`.
  - Threshold gate: **NOT** the 16.67 ms Gate A budget — this harness measures encode+submit of a single-cuboid scene, not 1k cubes. Likely budget: ≤ 1 ms recorder-host (substantially under both Gate A's pre-depth 0.112 ms and post-depth 0.122 ms, because the editor-shell scene is single-cuboid not 1k-cuboid).
  - Honest naming: `editor_shell_render_frame_e2e_minus_surface_p95_under_budget_recorder_host`.

- **What this would certify**:
  - Per-frame editor-shell substrate cost: `build_resource_map`, encoder creation, render pass record (with depth attachment), pipeline bind, three bind-group binds, vertex/index buffer set, single `draw_indexed`, command-buffer submit, queue wait/poll.
  - P95 stability under the variance gate.
  - Substrate-rest correctness across 3 runs on the recorder host.

- **What this would NOT certify** (explicit non-goals matching Gate A's posture):
  - `surface.get_current_texture()` latency or vsync/present cost — the harness skips winit's surface dance by design.
  - `window.request_redraw()` queuing latency or winit event-loop scheduling.
  - Universal vendor parity, cold-start, sustained thermal, realistic-geometry complexity (single-cuboid scene).
  - CI regression coverage (recorder-host-only, like Gate A).
  - Multi-frame compositor / triple-buffer behaviour.
  - Any threading model (single-threaded today per PLAN §1.5.2; Gate C remains structurally deferred).

## Tracked Files Likely to Require Edits in a Future Implementation Dispatch

Exact tracked files the next implementation dispatch is expected to touch:

| File | Edit shape |
| --- | --- |
| `crates/editor-shell/src/render_path.rs` | Extract winit-independent body of `render_frame` into `pub(crate) fn render_frame_to_target(&mut self, target_view: &wgpu::TextureView) -> bool`; add `pub(crate) fn init_render_state_headless(&mut self, target_format: wgpu::TextureFormat) -> Result<(), String>` mirroring `init_render_state` steps 2 + 4–7. |
| `crates/editor-shell/src/lib.rs` | One additional `#[cfg(test)] mod render_frame_e2e_perf;` line (if the harness lives under `src/`), OR no edit if the harness lives under `tests/` and the helpers become `pub`. |
| `crates/editor-shell/src/render_frame_e2e_perf.rs` | NEW: the `#[cfg(test)]`-gated `#[ignore]` release-only timing harness (Option (b) above; preferred). |

Files NOT expected to be touched:
- `crates/editor-shell/src/lifecycle.rs` — production `resumed` path stays unchanged; existing `init_render_state` is preserved alongside the new headless variant.
- `crates/editor-shell/src/render_input.rs` — no boundary changes; this is a `render_frame` body-level harness, not a snapshot-handoff harness.
- `crates/editor-shell/tests/editor_frame_idle.rs` / `render_input_boundary.rs` — both retained unchanged; the new harness is additive.
- `crates/gfx/**` / `kernel/**` / `runtime/**` — no edits expected.
- Any plan / doctrine / ADR / lint / Cargo / dependency manifest.

## Risk List (where `EditorShell::render_frame` coupling could balloon)

Six concrete balloon-risks, ranked by likelihood:

1. **`init_render_state_headless` parity drift with `init_render_state`** — Steps 4–7 (material / light / camera / pipeline / mesh / UBO update) are ~150 LoC; the headless variant must remain byte-identical or it's measuring a different substrate. Mitigation: extract a shared `pub(crate) fn init_render_state_post_surface(&mut self, surface_format_or_target_format: wgpu::TextureFormat) -> Result<(), String>` helper that both `init_render_state` and `init_render_state_headless` call, eliminating duplication.
2. **Surface format choice in the harness affects pipeline compile** — `LitMeshPipeline::new(...)` takes the color format; the harness's chosen target format (`Bgra8UnormSrgb` is the canonical match for `SurfaceContext` outputs on the recorder host) must match what `init_render_state`'s `surface_ctx` would produce, or the harness measures a different PSO than production. Mitigation: document the format-match invariant in the harness module doc + add a structural assertion that the harness's target format equals what `SurfaceContext`'s default config selects.
3. **`TexturePool` frame-index rhythm** — `tex_pool.begin_frame()` + `buf_pool.begin_frame()` in `render_path.rs:310–311` advance the frame index; the harness must call them each iteration in the same order or transient-resource aliasing diverges from production. Mitigation: the new `render_frame_to_target` should keep this call sequence verbatim.
4. **Single-cuboid scene under-represents production frame cost** — the existing production setup is `setup_cuboid_scene` (one cuboid + one overlay); a future production loaded scene will be heavier. The harness is honest about this (single-cuboid scope is explicit, matching `editor_frame_idle.rs`'s "NOT a loaded-editor measurement" note at lines 7–11) but downstream readers may over-credit it. Mitigation: name the gate `..._single_cuboid_...` so the scope is visible at the call site.
5. **Headless device-feature mismatch** — `GfxContext::new_headless()` may negotiate a different feature/limit set than the winit-bound surface path; if the harness's GPU adapter selection diverges, the encode/submit cost diverges. Mitigation: the harness should construct `GfxContext::new_headless()` the same way production `init_render_state:190` does, and the future dispatch should add a 3-line assertion that the negotiated adapter's `Backend` matches the production path's expected backend.
6. **Recorder-host scope must be respected for the budget gate** — like Gate A, this is recorder-host-only. The future dispatch must NOT enable the test under `--workspace` default invocations (must stay `#[ignore]`) and the close-out must document the recorder-host adapter triple (NVIDIA GeForce RTX 4060 Ti / Vulkan / DiscreteGpu) explicitly, matching Gate A's `BASELINE.md:240` precedent.

None of these are halt-condition triggers — all are scope-discipline concerns appropriate for the next implementation dispatch's TASK packet to encode.

## Recommended Next Dispatch Packet Shape (do NOT create now)

This dispatch does NOT author the implementation TASK; the shape below is the Executor's recommendation for the Planner's consideration.

- **DISPATCH_ID**: `POSTV0-EDITOR-SHELL-PERF-HARNESS-001`
- **STATUS**: OPEN
- **AUTHOR**: Planner / OpenAI Codex
- **MAY edit**:
  - `crates/editor-shell/src/render_path.rs` (bounded refactor + headless init variant).
  - `crates/editor-shell/src/lib.rs` (single `#[cfg(test)] mod render_frame_e2e_perf;` declaration, only if Option (b) above is taken).
- **MAY add new files**:
  - `crates/editor-shell/src/render_frame_e2e_perf.rs` — the `#[cfg(test)]` `#[ignore]` release-only timing harness.
  - Exactly one EXEC packet under `ai_handoffs/`.
- **MUST NOT edit**:
  - `crates/editor-shell/src/lifecycle.rs` (production `resumed` path is invariant for this dispatch).
  - Any other `crates/**`, `kernel/**`, `runtime/**`, `editor/**`, `plans/**`, ADRs, architecture lints, `Cargo.toml` / `Cargo.lock`, protocol docs / templates.
  - `Status.md` / `HANDOFF.md` / `change.md` / `V0_RELEASE_CERTIFICATION.md` (this is a perf-harness landing, not a cert-doc update; a separate docs-only follow-up can record the close-out).
- **Deliverables**:
  - `render_frame_to_target(&mut self, target_view: &wgpu::TextureView) -> bool` extracted from `render_frame`.
  - `init_render_state_headless(&mut self, target_format: wgpu::TextureFormat) -> Result<(), String>` mirroring `init_render_state` steps 2 + 4–7 (shared step 4–7 helper recommended per Risk 1).
  - `#[cfg(test)] mod render_frame_e2e_perf` with one `#[ignore]` release-only test producing P50 / P95 / min / max / worst-frame / variance across 3 runs of 60 warmup + 600 sample frames.
  - 9-lint architecture gate PASS (`forbidden-dep` / `split-exemption` / `no-utils` / `graph-foundation` / `editor-state-ownership` / `command-bus` / `projection-modules` / `kernel-isolation` / `failure-class`).
  - `cargo fmt` / `cargo check -p rge-editor-shell` / `cargo test -p rge-editor-shell --release --test render_frame_e2e_perf -- --ignored --nocapture` all PASS.
- **Acceptance Criteria**:
  - Existing production `render_frame` behaviour is preserved byte-for-byte (no functional change to production path).
  - New harness produces a numeric P95 value with variance under 30% across 3 runs on the recorder host.
  - No public API additions (Option (b) keeps the new helpers `pub(crate)`).
  - No new cargo dependency.
- **Halt Conditions**:
  - Refactoring `render_frame` requires changes outside the `render_path.rs` body (e.g., a `lifecycle.rs` edit, a `SurfaceContext` API change, a new trait, a new dependency).
  - The headless init path requires breaking changes to `GfxContext::new_headless()` or `LitMeshPipeline::new(...)`.
  - The harness measurement is unstable across runs (variance ≥ 30% repeatedly).
  - Any production tests regress.

## v0 Certification State

**v0 certification at commit `6aaf7f1` (cert commit `b13c176`) remains valid regardless of this preflight.** This dispatch is a read-only post-v0 analysis; it makes no changes to source, tests, Cargo, plans, ADRs, lints, protocol, templates, or cert docs. The editor-shell mock-event-loop perf harness was listed as a `Known v0 deferral (NOT blocker)` in `V0_RELEASE_CERTIFICATION.md` §4 at certification time, and this analysis confirms that posture: it is a pressure-driven future implementation, not a v0 retroactive obligation. v0 certification stands.

## Recommendation on Timing

**Start the next implementation dispatch when the next session begins** rather than as a same-session follow-up. The v0 release-certification dispatch landed this session at 18:13:03 (commit `b13c176`); this preflight landed at 18:31:54. Three reasons to defer the implementation dispatch to the next session:

1. **Session cleanliness**: cert + preflight + implementation in the same session bundles substrate-affecting work onto a celebratory milestone. Better signal separation by starting the implementation dispatch fresh.
2. **Pre-execution review headroom**: the implementation dispatch DOES touch tracked source (unlike this preflight), so v2 Rule 7 makes a pre-execution review more valuable than for this read-only preflight. A new-session start gives Codex (or whichever model is Planner) time to issue an OPTIONAL pre-exec review packet if desired.
3. **No pressure**: nothing else in the queue waits on this. v0 is certified, no consumer is blocked on `render_frame` perf numbers, and the recorder-host gate exists for a future dispatch to fire when the harness lands.

This is a recommendation, not a constraint. If the human authorizes a same-session implementation, the design map above is ready to consume.

## Deviations from Task Packet

None. Execution stayed strictly within the TASK scope:

- Exactly one new file produced (this EXEC packet under `ai_handoffs/`).
- Zero tracked-file edits.
- Zero edits to `Status.md` / `HANDOFF.md` / `change.md` / `V0_RELEASE_CERTIFICATION.md` / `crates/**` / `kernel/**` / `runtime/**` / `editor/**` / `plans/**` / `docs/**` / `Cargo.toml` / `Cargo.lock` / protocol / templates.
- No source / test / Cargo / ADR / lint / doctrine / Cargo entry / script / automation edits.
- No commit, no push, no expensive test run.
- No new files outside the single permitted EXEC packet.
- No new TASK, REVIEW, CORRECT, or CLOSEOUT packets authored from inside this preflight.
- v0 cert docs untouched.

## Open Questions for Reviewer / Planner

1. **Public-vs-`pub(crate)` for the new helpers**: Option (b) (`#[cfg(test)] mod render_frame_e2e_perf` under `src/`) keeps the public surface unchanged but means the harness is a unit test, not an integration test. Option (a) (publish two `pub fn ..._for_perf_only` items + harness under `tests/`) gives integration-test scope but adds two `pub` items. The Executor recommends (b). Planner / Reviewer signal-off welcomed.
2. **Measurement format**: Gate A's `60 warmup + 600 sample × 3 runs` vs Gate B's `batch K=10 × N=1000`. Executor recommends Gate A's format because this harness is per-frame-perf-focused (matching Gate A's intent), not CPU-rest-batch-amortised (matching Gate B's intent).
3. **Budget threshold**: the harness measures a single-cuboid scene, so the natural budget is substantially under Gate A's 16.67 ms. Executor proposed `≤ 1 ms` as a placeholder; Planner may want to anchor this against a `cargo test` dry-run number from the recorder host before pinning the threshold in the next TASK packet.

## Worktree State

- Tracked files: clean (verified by empty `git status --short --untracked-files=no`).
- New untracked items from this turn: 1 (this EXEC packet).
- Other untracked items: the new TASK packet authored by Codex at 18:26:15; the root-level `OPENAItoCLAUDE_2026-05-14_18-26-16+0300.md`; the prior CLAUDEtoOPENAI / OPENAItoCLAUDE root notes; the `_v0cert_test.log` build artifact from the prior v0-cert dispatch (gitignored); precedent dispatch packets from earlier in the session.
- Branch: `main`.
- HEAD: `b13c176 docs(cert): v0 release certification at 6aaf7f1`.
- `origin/main...HEAD`: `0 0` (synced).

---

HANDOFF_STATUS: COMPLETE
DISPATCH_ID: POSTV0-EDITOR-SHELL-PERF-PREFLIGHT-001
AUTHOR: Executor / Anthropic Claude
NEXT_ROLE: REVIEWER_AI
EXIT_CODE: 0

---
